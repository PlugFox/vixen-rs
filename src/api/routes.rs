use crate::api::response::ApiResult;
use axum::extract::State;
use reqwest::StatusCode;
use serde::Serialize;
use sqlx::SqlitePool;
use tracing::{debug, error, info};

#[derive(Serialize)]
pub struct HealthStatus {
    status: String,
    database: String,
}

/// Health-check handler
pub async fn health(State(db_pool): State<SqlitePool>) -> ApiResult<HealthStatus> {
    let row: (i32,) = match sqlx::query_as("SELECT 1 AS health")
        .fetch_one(&db_pool)
        .await
    {
        Ok(row) => row,
        Err(_) => {
            // Return 500 Internal Server Error if the database connection fails
            error!("Database connection failed");
            return ApiResult::error_with_status(
                "DATABASE_ERROR",
                "Database connection failed",
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    if row.0 == 1 {
        ApiResult::success(HealthStatus {
            status: "OK".to_string(),
            database: "Connected".to_string(),
        })
    } else {
        ApiResult::error_with_status(
            "DATABASE_ERROR",
            "Database health check failed",
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    }
}

/// Пример обработчика для скачивания файла
pub async fn download_database(State(db_pool): State<SqlitePool>) -> ApiResult<()> {
    // Здесь логика для создания резервной копии БД
    let backup_data = vec![1, 2, 3, 4, 5]; // Заглушка

    ApiResult::file(
        backup_data,
        "database_backup.db",
        "application/octet-stream",
    )
}

/// Fallback handler for 404 Not Found
pub async fn not_found() -> ApiResult<()> {
    ApiResult::error_with_status(
        "NOT_FOUND",
        "The requested resource was not found",
        StatusCode::NOT_FOUND,
    )
}
