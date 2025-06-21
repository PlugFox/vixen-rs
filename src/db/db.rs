use anyhow::Result;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

/// Initialize the SQLite connection pool
pub async fn init_db(database_url: &str) -> Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    Ok(pool)
}
