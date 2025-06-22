use crate::api::{api::ApiState, response::ApiResult};
use axum::extract::{Path, State};
use reqwest::StatusCode;
use serde::Serialize;
use tracing::{debug, error};

#[derive(Serialize)]
pub struct HealthStatus {
    status: String,
    database: String,
}

/// Health-check handler
/// Checks the health of the application and the database connection
pub async fn get_health(State(state): State<ApiState>) -> ApiResult<HealthStatus> {
    match state.db.health_check().await {
        Ok(true) => ApiResult::success(HealthStatus {
            status: "OK".to_string(),
            database: "Connected".to_string(),
        }),
        Ok(false) => ApiResult::error_with_status(
            "DATABASE_ERROR",
            "Database health check failed",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        Err(e) => {
            error!("database connection failed: {}", e);
            ApiResult::error_with_status(
                "DATABASE_ERROR",
                "Database connection failed",
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

// TODO(plugfox): Use `moka` or another LRU TTL cache for caching `get_about` and `get_report` public endpoints

/// About handler
/// Returns information about the application, such as version
pub async fn get_about() -> ApiResult<String> {
    // Здесь можно добавить информацию о приложении, например, версию
    let version = env!("CARGO_PKG_VERSION");
    ApiResult::success(format!("Application version: {}", version))
}

/// Get report for last 24 hours
pub async fn get_report() -> ApiResult<String> {
    // Здесь логика для получения отчета
    let report = "This is a placeholder for the report".to_string();
    ApiResult::success(report)
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
pub async fn admin_get_download_database(State(_state): State<ApiState>) -> ApiResult<()> {
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
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> ApiResult<String> {
    debug!("fetching log with ID: {}", id);
    let log = sqlx::query_scalar::<_, String>("SELECT log FROM logs WHERE id = ?")
        .bind(&id)
        .fetch_optional(state.db.get_pool())
        .await;
    match log {
        Ok(Some(log)) => ApiResult::success(log),
        Ok(None) => {
            ApiResult::error_with_status("LOG_NOT_FOUND", "Log not found", StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("error fetching log: {}", e);
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

pub async fn admin_put_users_verified(State(state): State<ApiState>) -> ApiResult<()> {
    // Здесь логика для обновления статуса пользователей
    let updated = sqlx::query("UPDATE users SET verified = 1 WHERE id = 1")
        .execute(state.db.get_pool())
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
            error!("error updating users: {}", e);
            ApiResult::error_with_status(
                "DATABASE_ERROR",
                "Failed to update users in database",
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

pub async fn admin_delete_verified_users(State(state): State<ApiState>) -> ApiResult<()> {
    // Здесь логика для удаления проверенных пользователей
    let deleted = sqlx::query("DELETE FROM users WHERE verified = 1")
        .execute(state.db.get_pool())
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
            error!("error deleting verified users: {}", e);
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
    State(state): State<ApiState>,
    Path(cid): Path<String>,
) -> ApiResult<String> {
    // Здесь логика для получения сводки по cid
    let summary = sqlx::query_scalar::<_, String>("SELECT summary FROM summaries WHERE cid = ?")
        .bind(&cid)
        .fetch_optional(state.db.get_pool())
        .await;

    match summary {
        Ok(Some(data)) => ApiResult::success(data),
        Ok(None) => ApiResult::error_with_status(
            "SUMMARY_NOT_FOUND",
            "Summary not found for the given cid",
            StatusCode::NOT_FOUND,
        ),
        Err(e) => {
            error!("error fetching summary: {}", e);
            ApiResult::error_with_status(
                "DATABASE_ERROR",
                "Failed to fetch summary from database",
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}
