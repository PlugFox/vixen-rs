//! HTTP router builder. Assembles `/health`, `/about`, the OpenAPI JSON spec
//! and (optionally) the Scalar UI behind a CORS + request-id + tracing
//! middleware stack.

use axum::Router;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderName, HeaderValue, Method};
use axum::response::Html;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::routes_about::AboutResponse;
use crate::api::routes_health::{HealthChecks, HealthResponse};
use crate::api::state::AppState;
use crate::api::{routes_about, routes_health};

/// Top-level OpenAPI document. Schemas are picked up automatically via
/// `utoipa-axum::routes!` ↦ `OpenApiRouter::routes`.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "vixen-server",
        description = "Telegram anti-spam bot — operational + dashboard API.",
    ),
    components(schemas(HealthResponse, HealthChecks, AboutResponse)),
    tags((name = "ops", description = "Health + build metadata"))
)]
struct ApiDoc;

const REQUEST_ID_HEADER: &str = "x-request-id";

/// Build the application router with state, routes and middleware.
pub fn build_router(state: AppState) -> Router {
    let openapi_ui = state.config.resolve_openapi_ui();
    let cors_origins = state.config.cors_origins.clone();

    // Routes wired into both the Axum router and the OpenAPI spec.
    let (api_router, mut openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes_health::health))
        .routes(routes!(routes_about::about))
        .split_for_parts();

    // Pin a stable version label on the spec so dashboards can detect it.
    openapi.info.version = crate::build_info::VERSION.to_string();

    let openapi_json = openapi.clone();
    let mut app = api_router.with_state(state).route(
        "/api/v1/openapi.json",
        axum::routing::get(move || {
            let spec = openapi_json.clone();
            async move { axum::Json(spec) }
        }),
    );

    if openapi_ui {
        app = app.route(
            "/scalar",
            axum::routing::get(|| async { Html(SCALAR_HTML) }),
        );
    }

    let request_id = HeaderName::from_static(REQUEST_ID_HEADER);
    let cors = build_cors(&cors_origins);

    app.layer(SetRequestIdLayer::new(request_id.clone(), MakeRequestUuid))
        .layer(PropagateRequestIdLayer::new(request_id))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

/// Inline Scalar UI loading the spec from `/api/v1/openapi.json`. CDN script,
/// no static assets needed; gated by `Config::resolve_openapi_ui`.
const SCALAR_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>vixen-server API</title>
  <meta name="viewport" content="width=device-width, initial-scale=1" />
</head>
<body>
  <script id="api-reference" data-url="/api/v1/openapi.json"></script>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body>
</html>"#;

fn build_cors(origins: &[String]) -> CorsLayer {
    let layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION]);

    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    if parsed.is_empty() {
        layer
    } else {
        layer.allow_origin(parsed)
    }
}
