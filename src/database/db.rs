use anyhow::Result;
use sqlx::{SqlitePool, migrate::MigrateDatabase, sqlite::SqlitePoolOptions};
use std::{path::Path, time::Duration};
use tracing::{debug, error, info};

use crate::telegram;

/// Thread-safe wrapper around SqlitePool
///
/// SQLite with WAL mode supports multiple concurrent readers and one writer.
/// SqlitePool is already thread-safe and handles connection management internally.
/// 1. SqlitePool implements Clone and is thread-safe
/// 2. WAL mode allows concurrent reads
/// 3. sqlx handles write synchronization internally
/// 4. Connection pooling prevents resource exhaustion
#[derive(Clone)]
pub struct DB {
    users_verified: std::sync::Arc<tokio::sync::RwLock<std::collections::HashSet<i64>>>,
    pool: SqlitePool,
}

impl DB {
    /// Create a new DB from an existing SqlitePool
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            users_verified: std::sync::Arc::new(tokio::sync::RwLock::new(
                std::collections::HashSet::new(),
            )),
            pool,
        }
    }

    /// Connect to the SQLite database at the specified URL
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = init_db_pool(database_url).await?;
        Ok(Self::new(pool))
    }

    /// Execute a query that returns rows
    pub async fn fetch_all(
        &self,
        query: &str,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query(query).fetch_all(&self.pool).await
    }

    /// Execute a query that returns a single row
    pub async fn fetch_one(&self, query: &str) -> Result<sqlx::sqlite::SqliteRow, sqlx::Error> {
        sqlx::query(query).fetch_one(&self.pool).await
    }

    /// Execute a query that returns an optional row
    pub async fn fetch_optional(
        &self,
        query: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query(query).fetch_optional(&self.pool).await
    }

    /// Execute a query that doesn't return rows
    pub async fn execute(
        &self,
        query: &str,
    ) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
        sqlx::query(query).execute(&self.pool).await
    }

    /// Execute a prepared query with parameters
    pub async fn execute_with_params<'a>(
        &self,
        query: sqlx::query::Query<'a, sqlx::sqlite::Sqlite, sqlx::sqlite::SqliteArguments<'a>>,
    ) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
        query.execute(&self.pool).await
    }

    /// Get access to the underlying pool for complex operations
    pub fn get_pool(&self) -> &SqlitePool {
        &self.pool
    }
    /// Begin a transaction
    pub async fn begin(&self) -> Result<sqlx::Transaction<sqlx::Sqlite>, sqlx::Error> {
        self.pool.begin().await
    }

    /// Check if the database connection is healthy
    pub async fn health_check(&self) -> Result<bool, sqlx::Error> {
        let result: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&self.pool).await?;
        Ok(result == 1)
    }

    /// Check if a user is already verified
    pub async fn is_user_verified(&self, user_id: i64) -> bool {
        {
            let read = self.users_verified.read().await;
            if read.contains(&user_id) {
                return true;
            }
        }

        // Check the database for the user
        let verified = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT user_id FROM users_verified WHERE user_id = ? LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .is_ok_and(|row| row.is_some());

        // If the user is verified, add them to the in-memory set
        if verified {
            let mut write = self.users_verified.write().await;
            write.insert(user_id);
        }

        verified
    }

    /// Mark a user as verified in the database and in-memory set
    pub async fn mark_user_verified(
        &self,
        user_id: i64, /* user: &telegram::User */
    ) -> Result<()> {
        // Insert the user into the database
        sqlx::query("INSERT INTO users_verified (user_id) VALUES (?)")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        // Add to the in-memory set
        let mut write = self.users_verified.write().await;
        write.insert(user_id);

        Ok(())
    }

    /// Upsert a message and an user to the database in a single transaction using batch operations
    pub async fn upsert_message(&self, message: &telegram::Message) -> Result<()> {
        let mut tx = self.begin().await?;

        // Get message metadata (computed lazily and cached)
        let metadata = message.metadata();

        // Get reply_to_message_id if exists
        let reply_to = message.reply_to_message.as_ref().map(|m| m.message_id);

        // Convert chat type to string
        let chat_type_str = match message.chat.chat_type {
            telegram::ChatType::Private => "private",
            telegram::ChatType::Group => "group",
            telegram::ChatType::Supergroup => "supergroup",
            telegram::ChatType::Channel => "channel",
        };

        // Check if we have a sender
        let user = message
            .from
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Message without sender not supported"))?;

        // Batch upsert using separate queries to avoid multi-statement issues
        // First upsert chat info
        sqlx::query(
            r#"INSERT INTO chat_info (chat_id, chat_type, title, username, first_name, last_name, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, strftime('%s', 'now'))
               ON CONFLICT(chat_id) DO UPDATE SET
                   chat_type = excluded.chat_type,
                   title = excluded.title,
                   username = excluded.username,
                   first_name = excluded.first_name,
                   last_name = excluded.last_name,
                   updated_at = strftime('%s', 'now')"#,
        )
        .bind(message.chat.id)
        .bind(chat_type_str)
        .bind(&message.chat.title)
        .bind(&message.chat.username)
        .bind(&message.chat.first_name)
        .bind(&message.chat.last_name)
        .execute(&mut *tx)
        .await?;

        // Upsert user
        sqlx::query(
            r#"INSERT INTO users (user_id, is_bot, first_name, last_name, username, language_code)
               VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT(user_id) DO UPDATE SET
                   is_bot = excluded.is_bot,
                   first_name = excluded.first_name,
                   last_name = excluded.last_name,
                   username = excluded.username,
                   language_code = excluded.language_code"#,
        )
        .bind(user.id)
        .bind(if user.is_bot { 1 } else { 0 })
        .bind(&user.first_name)
        .bind(&user.last_name)
        .bind(&user.username)
        .bind(&user.language_code)
        .execute(&mut *tx)
        .await?;

        // Upsert chat_user relationship
        sqlx::query(
            r#"INSERT INTO chat_user (chat_id, user_id) VALUES (?, ?)
               ON CONFLICT(chat_id, user_id) DO NOTHING"#,
        )
        .bind(message.chat.id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;

        // Insert/update message
        sqlx::query(
            r#"INSERT INTO messages (message_id, chat_id, user_id, date, message_type, reply_to, length, content)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(message_id) DO UPDATE SET
                   chat_id = excluded.chat_id,
                   user_id = excluded.user_id,
                   date = excluded.date,
                   message_type = excluded.message_type,
                   reply_to = excluded.reply_to,
                   length = excluded.length,
                   content = excluded.content"#,
        )
        .bind(message.message_id)
        .bind(message.chat.id)
        .bind(user.id)
        .bind(message.date as i64)
        .bind(metadata.message_type)
        .bind(reply_to)
        .bind(metadata.length)
        .bind(&metadata.content)
        .execute(&mut *tx)
        .await?;

        // Commit transaction
        tx.commit().await?;
        Ok(())
    }
}

/// Initialize the SQLite connection pool
async fn init_db_pool(database_url: &str) -> Result<SqlitePool> {
    // Ensure directory exists for database file
    fn ensure_database_directory(database_url: &str) -> Result<()> {
        if let Some(path_part) = database_url.strip_prefix("sqlite://") {
            if let Some(db_path) = path_part.split('?').next() {
                if let Some(parent) = Path::new(db_path).parent() {
                    // Create the parent directory if it does not exist
                    debug!("ensuring database directory exists: {}", parent.display());
                    std::fs::create_dir_all(parent)?;
                }
            }
        }
        Ok(())
    }

    // Ensure the database directory exists
    ensure_database_directory(database_url)?;

    // Create database if it doesn't exist
    if !sqlx::Sqlite::database_exists(database_url)
        .await
        .unwrap_or(false)
    {
        info!("creating database at: {}", database_url);
        sqlx::Sqlite::create_database(database_url).await?;
    } // Create the connection pool with optimized settings for SQLite
    debug!("connecting to database at: {}", database_url);
    let pool = SqlitePoolOptions::new()
        // SQLite with WAL mode can handle multiple readers + 1 writer
        // For web servers, 2-5 connections is usually optimal for SQLite
        .max_connections(5)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600)) // 10 minutes
        .max_lifetime(Duration::from_secs(1800)) // 30 minutes        // Enable WAL mode and other optimizations after connecting
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                // Enable WAL mode for better concurrency
                sqlx::query("PRAGMA journal_mode = WAL;")
                    .execute(&mut *conn)
                    .await?;

                // Set synchronous to NORMAL for better performance
                // FULL is safer but much slower
                sqlx::query("PRAGMA synchronous = NORMAL;")
                    .execute(&mut *conn)
                    .await?;

                // Set a reasonable busy timeout (5 seconds)
                sqlx::query("PRAGMA busy_timeout = 5000;")
                    .execute(&mut *conn)
                    .await?;

                // Enable foreign keys
                sqlx::query("PRAGMA foreign_keys = ON;")
                    .execute(&mut *conn)
                    .await?;

                // Optimize cache size (in KB)
                sqlx::query("PRAGMA cache_size = -64000;") // 64MB
                    .execute(&mut *conn)
                    .await?;

                Ok(())
            })
        })
        .connect(database_url)
        .await?;

    // Run migrations
    info!("running database migrations");
    match sqlx::migrate!("./migrations").run(&pool).await {
        Ok(_) => debug!("database migrations completed successfully"),
        Err(e) => {
            error!("failed to run database migrations: {}", e);
            return Err(e.into());
        }
    }

    // VACUUM the database to optimize it
    debug!("running VACUUM on the database to optimize it");
    sqlx::query("VACUUM").execute(&pool).await.map_err(|e| {
        error!("failed to VACUUM the database: {}", e);
        e
    })?;

    Ok(pool)
}
