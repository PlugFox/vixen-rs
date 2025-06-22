use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use futures::FutureExt;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::api::response::ApiResult;

#[derive(Clone)]
pub struct AdminScope {
    pub secret: String,
}

/// Rate limiting state
#[derive(Clone)]
pub struct RateLimitState {
    pub requests: Arc<RwLock<HashMap<String, (u32, Instant)>>>,
    pub max_requests: u32,
    pub window_duration: std::time::Duration,
}

impl RateLimitState {
    pub fn new(max_requests: u32, window_seconds: u64) -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
            max_requests,
            window_duration: std::time::Duration::from_secs(window_seconds),
        }
    }
}

pub async fn auth_middleware(
    State(private): State<AdminScope>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|header| header.to_str().ok());

    match auth_header {
        Some(token) if token.starts_with("Bearer ") => {
            let token = &token[7..]; // Remove "Bearer " prefix
            if token == private.secret {
                Ok(next.run(request).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Middleware для логирования запросов с информацией о времени выполнения и IP
pub async fn logging_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let user_agent = request
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Получаем реальный IP из заголовков (если есть прокси)
    let real_ip = get_real_ip(&request, addr);

    let response = next.run(request).await;
    let duration = start.elapsed();
    let status = response.status();

    // Логируем запрос
    if status.is_server_error() {
        warn!(
            ip = %real_ip,
            method = %method,
            uri = %uri,
            status = %status,
            duration_ms = duration.as_millis(),
            user_agent = user_agent,
            "Request completed with server error"
        );
    } else if status.is_client_error() {
        warn!(
            ip = %real_ip,
            method = %method,
            uri = %uri,
            status = %status,
            duration_ms = duration.as_millis(),
            user_agent = user_agent,
            "Request completed with client error"
        );
    } else {
        info!(
            ip = %real_ip,
            method = %method,
            uri = %uri,
            status = %status,
            duration_ms = duration.as_millis(),
            user_agent = user_agent,
            "Request completed successfully"
        );
    }

    response
}

/// Извлекает реальный IP адрес из заголовков или использует адрес подключения
fn get_real_ip(request: &Request, fallback_addr: SocketAddr) -> String {
    // Проверяем заголовки в порядке приоритета
    let headers = request.headers();

    // X-Forwarded-For (наиболее распространённый)
    if let Some(forwarded_for) = headers.get("x-forwarded-for") {
        if let Ok(value) = forwarded_for.to_str() {
            // Берём первый IP из списка
            if let Some(first_ip) = value.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }

    // X-Real-IP (используется Nginx)
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            return value.to_string();
        }
    }

    // CF-Connecting-IP (Cloudflare)
    if let Some(cf_ip) = headers.get("cf-connecting-ip") {
        if let Ok(value) = cf_ip.to_str() {
            return value.to_string();
        }
    }

    // X-Forwarded (менее распространённый)
    if let Some(forwarded) = headers.get("x-forwarded") {
        if let Ok(value) = forwarded.to_str() {
            if let Some(for_part) = value
                .split(';')
                .find(|part| part.trim().starts_with("for="))
            {
                if let Some(ip) = for_part.split('=').nth(1) {
                    return ip.trim_matches('"').to_string();
                }
            }
        }
    }

    // Если ничего не найдено, используем адрес подключения
    fallback_addr.ip().to_string()
}

/// Middleware для добавления security заголовков
pub async fn security_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();

    // Добавляем security заголовки
    headers.insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    headers.insert(
        "X-XSS-Protection",
        HeaderValue::from_static("1; mode=block"),
    );
    headers.insert(
        "Referrer-Policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        "Content-Security-Policy",
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'",
        ),
    );

    response
}

/// Middleware для обработки паник и конвертации их в ApiErrorResponse
pub async fn panic_recovery_middleware(request: Request, next: Next) -> Response {
    let result = std::panic::AssertUnwindSafe(next.run(request))
        .catch_unwind()
        .await;

    match result {
        Ok(response) => response,
        Err(panic_info) => {
            let panic_message = if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "Unknown panic occurred".to_string()
            };

            error!("Panic occurred in request handler: {}", panic_message);

            // Создаем стандартный error response
            let error_response = ApiResult::<()>::error_with_status(
                "INTERNAL_SERVER_ERROR",
                "An internal server error occurred",
                StatusCode::INTERNAL_SERVER_ERROR,
            );

            error_response.into_response()
        }
    }
}

/// Middleware для добавления метрик производительности в заголовки ответа
pub async fn metrics_middleware(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();

    // Запускаем обработчик
    let mut response = next.run(request).await;

    let duration = start.elapsed();
    let headers = response.headers_mut();

    // Добавляем метрики в заголовки
    if let Ok(duration_ms) = HeaderValue::from_str(&duration.as_millis().to_string()) {
        headers.insert("X-Response-Time-Ms", duration_ms);
    }

    if let Ok(timestamp) = HeaderValue::from_str(&chrono::Utc::now().timestamp().to_string()) {
        headers.insert("X-Timestamp", timestamp);
    }

    headers.insert("X-Server", HeaderValue::from_static("vixen-rs"));
    headers.insert(
        "X-Version",
        HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
    );

    // Логируем метрики для анализа
    info!(
        method = %method,
        uri = %uri,
        duration_ms = duration.as_millis(),
        status = %response.status(),
        "Request metrics"
    );

    response
}

/// Rate limiting middleware для защиты от злоупотреблений
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(rate_limit): State<RateLimitState>,
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let client_ip = addr.ip().to_string();
    let now = Instant::now();

    {
        let mut requests = rate_limit.requests.write().await;

        // Очищаем старые записи
        requests.retain(|_, (_, timestamp)| {
            now.duration_since(*timestamp) < rate_limit.window_duration
        });

        // Проверяем лимит для текущего IP
        let (count, first_request) = requests.entry(client_ip.clone()).or_insert((0, now));

        if now.duration_since(*first_request) > rate_limit.window_duration {
            // Окно истекло, сбрасываем счетчик
            *count = 1;
            *first_request = now;
        } else {
            *count += 1;
            if *count > rate_limit.max_requests {
                warn!(
                    ip = %client_ip,
                    requests = *count,
                    limit = rate_limit.max_requests,
                    "Rate limit exceeded"
                );

                let error_response = ApiResult::<()>::error_with_status(
                    "RATE_LIMIT_EXCEEDED",
                    "Too many requests. Please try again later.",
                    StatusCode::TOO_MANY_REQUESTS,
                );

                return Err(error_response.into_response());
            }
        }
    }

    Ok(next.run(request).await)
}

/// Middleware для валидации размера запроса и других параметров
pub async fn request_validation_middleware(
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let method = request.method();
    let uri = request.uri();

    // Проверяем максимальную длину URI
    if uri.path().len() > 2048 {
        warn!(uri = %uri, "Request URI too long");
        let error_response = ApiResult::<()>::error_with_status(
            "URI_TOO_LONG",
            "Request URI is too long",
            StatusCode::URI_TOO_LONG,
        );
        return Err(error_response.into_response());
    }

    // Проверяем метод запроса
    if !matches!(
        method,
        &axum::http::Method::GET
            | &axum::http::Method::POST
            | &axum::http::Method::PUT
            | &axum::http::Method::DELETE
            | &axum::http::Method::OPTIONS
            | &axum::http::Method::HEAD
    ) {
        warn!(method = %method, "Unsupported HTTP method");
        let error_response = ApiResult::<()>::error_with_status(
            "METHOD_NOT_ALLOWED",
            "HTTP method not allowed",
            StatusCode::METHOD_NOT_ALLOWED,
        );
        return Err(error_response.into_response());
    }

    Ok(next.run(request).await)
}
