use anyhow::Result;
use sqlx::{SqlitePool, migrate::MigrateDatabase, sqlite::SqlitePoolOptions};
use std::path::Path;
use tracing::{debug, error, info, warn};

/// Ensure directory exists for database file
pub fn ensure_database_directory(database_url: &str) -> Result<()> {
    if let Some(path_part) = database_url.strip_prefix("sqlite://") {
        if let Some(db_path) = path_part.split('?').next() {
            if let Some(parent) = Path::new(db_path).parent() {
                // Create the parent directory if it does not exist
                debug!("Ensuring database directory exists: {}", parent.display());
                std::fs::create_dir_all(parent)?;
            }
        }
    }
    Ok(())
}

/// Initialize the SQLite connection pool
pub async fn init_db_pool(database_url: &str) -> Result<SqlitePool> {
    // Ensure the database directory exists
    ensure_database_directory(database_url)?;

    // Create database if it doesn't exist
    if !sqlx::Sqlite::database_exists(database_url)
        .await
        .unwrap_or(false)
    {
        info!("Creating database at: {}", database_url);
        sqlx::Sqlite::create_database(database_url).await?;
    }

    // Create the connection pool with a maximum of 5 connections
    debug!("Connecting to database at: {}", database_url);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Run migrations
    info!("Running database migrations");
    sqlx::migrate!("./migrations").run(&pool).await?;

    // VACUUM the database to optimize it
    debug!("Running VACUUM on the database to optimize it");
    sqlx::query("VACUUM").execute(&pool).await.map_err(|e| {
        error!("Failed to VACUUM the database: {}", e);
        e
    })?;

    Ok(pool)
}
