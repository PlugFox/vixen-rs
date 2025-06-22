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
/// Checks the health of the application and the database connection
pub async fn get_health(State(db_pool): State<SqlitePool>) -> ApiResult<HealthStatus> {
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

/// About handler
/// Returns information about the application, such as version
pub async fn get_about() -> ApiResult<String> {
    // Здесь можно добавить информацию о приложении, например, версию
    let version = env!("CARGO_PKG_VERSION");
    ApiResult::success(format!("Application version: {}", version))
}

/// Fallback handler for 404 Not Found
pub async fn not_found() -> ApiResult<()> {
    ApiResult::error_with_status(
        "NOT_FOUND",
        "The requested resource was not found",
        StatusCode::NOT_FOUND,
    )
}

/// Пример обработчика для скачивания файла
pub async fn admin_get_download_database(State(db_pool): State<SqlitePool>) -> ApiResult<()> {
    // Здесь логика для создания резервной копии БД
    let backup_data = vec![1, 2, 3, 4, 5]; // Заглушка

    ApiResult::file(
        backup_data,
        "database_backup.db",
        "application/octet-stream",
    )
}

/// Read logs from the database
pub async fn admin_get_logs() -> ApiResult<String> {
    let logs = "This is a placeholder for logs".to_string();

    ApiResult::success(logs)
}

/// Read logs by ID from the database
pub async fn admin_get_logs_by_id(
    State(db_pool): State<SqlitePool>,
    id: String,
) -> ApiResult<String> {
    debug!("Fetching log with ID: {}", id);
    let log = sqlx::query_scalar("SELECT log FROM logs WHERE id = ?")
        .bind(&id)
        .fetch_one(&db_pool)
        .await;
    match log {
        Ok(Some(log)) => ApiResult::success(log),
        Ok(None) => {
            ApiResult::error_with_status("LOG_NOT_FOUND", "Log not found", StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Error fetching log: {}", e);
            ApiResult::error_with_status(
                "DATABASE_ERROR",
                "Failed to fetch log from database",
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

/// Get a list of verified users
pub async fn admin_get_users_verified() -> ApiResult<String> {
    // Здесь логика для получения списка проверенных пользователей
    let users = "This is a placeholder for verified users".to_string();
    ApiResult::success(users)
}

pub async fn admin_put_users_verified(State(db_pool): State<SqlitePool>) -> ApiResult<()> {
    // Здесь логика для обновления статуса пользователей
    let updated = sqlx::query("UPDATE users SET verified = 1 WHERE id = 1")
        .execute(&db_pool)
        .await;
    match updated {
        Ok(result) => {
            if result.rows_affected() > 0 {
                ApiResult::success(())
            } else {
                ApiResult::error_with_status(
                    "NO_USERS_UPDATED",
                    "No users were updated",
                    StatusCode::NOT_FOUND,
                )
            }
        }
        Err(e) => {
            error!("Error updating users: {}", e);
            ApiResult::error_with_status(
                "DATABASE_ERROR",
                "Failed to update users in database",
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

pub async fn admin_delete_verified_users(State(db_pool): State<SqlitePool>) -> ApiResult<()> {
    // Здесь логика для удаления проверенных пользователей
    let deleted = sqlx::query("DELETE FROM users WHERE verified = 1")
        .execute(&db_pool)
        .await;
    match deleted {
        Ok(result) => {
            if result.rows_affected() > 0 {
                ApiResult::success(())
            } else {
                ApiResult::error_with_status(
                    "NO_USERS_DELETED",
                    "No verified users were deleted",
                    StatusCode::NOT_FOUND,
                )
            }
        }
        Err(e) => {
            error!("Error deleting verified users: {}", e);
            ApiResult::error_with_status(
                "DATABASE_ERROR",
                "Failed to delete verified users from database",
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

pub async fn admin_get_messages_deleted() -> ApiResult<String> {
    // Здесь логика для получения удаленных сообщений
    let messages = "This is a placeholder for deleted messages".to_string();
    ApiResult::success(messages)
}

pub async fn admin_get_messages_deleted_hash() -> ApiResult<String> {
    // Здесь логика для получения хэша удаленных сообщений
    let hash = "This is a placeholder for deleted messages hash".to_string();
    ApiResult::success(hash)
}

pub async fn get_report() -> ApiResult<String> {
    // Здесь логика для получения отчета
    let report = "This is a placeholder for the report".to_string();
    ApiResult::success(report)
}

pub async fn admin_get_report() -> ApiResult<String> {
    // Здесь логика для получения отчета администратора
    let admin_report = "This is a placeholder for the admin report".to_string();
    ApiResult::success(admin_report)
}

pub async fn admin_get_chart() -> ApiResult<String> {
    // Здесь логика для получения данных для графика
    let chart_data = "This is a placeholder for chart data".to_string();
    ApiResult::success(chart_data)
}

pub async fn admin_get_chart_png() -> ApiResult<()> {
    // Здесь логика для получения графика в формате PNG
    let chart_png = vec![0u8; 100]; // Заглушка для PNG данных
    ApiResult::file(chart_png, "chart.png", "image/png")
}

pub async fn admin_get_summary(
    State(db_pool): State<SqlitePool>,
    cid: String,
) -> ApiResult<String> {
    // Здесь логика для получения сводки по cid
    let summary = sqlx::query_scalar("SELECT summary FROM summaries WHERE cid = ?")
        .bind(&cid)
        .fetch_one(&db_pool)
        .await;

    match summary {
        Ok(Some(data)) => ApiResult::success(data),
        Ok(None) => ApiResult::error_with_status(
            "SUMMARY_NOT_FOUND",
            "Summary not found for the given cid",
            StatusCode::NOT_FOUND,
        ),
        Err(e) => {
            error!("Error fetching summary: {}", e);
            ApiResult::error_with_status(
                "DATABASE_ERROR",
                "Failed to fetch summary from database",
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}
