use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use std::net::SocketAddr;
use std::time::Instant;
use tracing::{info, warn};

#[derive(Clone)]
pub struct AdminScope {
    pub secret: String,
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
