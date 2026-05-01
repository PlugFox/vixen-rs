//! PostgreSQL connection pool. Wired in by `bin/server.rs` and shared as
//! `Arc<Database>` across services. See `server/docs/database.md` for the
//! pool sizing rationale and per-connection statement timeout.

use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Executor, PgPool};

use crate::config::Config;

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Build the pool, install the per-connection `statement_timeout` and
    /// verify the first connection is acquirable. Returns once at least one
    /// connection is healthy.
    pub async fn connect(config: &Config) -> Result<Self, sqlx::Error> {
        let connect_options: PgConnectOptions = config.database_url.parse()?;

        // `SET statement_timeout` is session-scoped — once set on a pooled
        // connection it sticks for the lifetime of that connection, which is
        // exactly what we want.
        let stmt_timeout_ms = config.db_statement_timeout_ms;

        let pool = PgPoolOptions::new()
            .max_connections(config.db_max_connections)
            .min_connections(config.db_min_connections)
            .acquire_timeout(Duration::from_millis(config.db_acquire_timeout_ms))
            .idle_timeout(Some(Duration::from_millis(config.db_idle_timeout_ms)))
            .after_connect(move |conn, _meta| {
                Box::pin(async move {
                    conn.execute(format!("SET statement_timeout = {stmt_timeout_ms}").as_str())
                        .await?;
                    Ok(())
                })
            })
            .connect_with(connect_options)
            .await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Cheap liveness probe used by `/health`. Acquires a connection and runs
    /// `SELECT 1`. Subject to the pool's `acquire_timeout`.
    pub async fn health_check(&self) -> Result<(), sqlx::Error> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }

    /// Apply pending migrations from `server/migrations/`. Operational tool —
    /// production typically runs migrations out-of-band via `sqlx migrate run`.
    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await
    }

    /// Drain in-flight queries and close the pool. Call before process exit.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}
