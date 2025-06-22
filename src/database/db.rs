use anyhow::Result;
use sqlx::{SqlitePool, migrate::MigrateDatabase, sqlite::SqlitePoolOptions};
use std::{path::Path, time::Duration};
use tracing::{debug, error, info};

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
    pool: SqlitePool,
}

impl DB {
    /// Create a new DB from an existing SqlitePool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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
    sqlx::migrate!("./migrations").run(&pool).await?;

    // VACUUM the database to optimize it
    debug!("running VACUUM on the database to optimize it");
    sqlx::query("VACUUM").execute(&pool).await.map_err(|e| {
        error!("failed to VACUUM the database: {}", e);
        e
    })?;

    Ok(pool)
}
