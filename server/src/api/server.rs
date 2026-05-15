//! HTTP router builder. Assembles `/health`, `/about`, the M4 auth + config
//! routes, the `/admin/*` ops surface and (optionally) the Scalar UI behind
//! a CORS + request-id + tracing middleware stack.

use axum::Router;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderName as TowerHeaderName};
use axum::http::{HeaderName, HeaderValue, Method};
use axum::middleware::from_fn_with_state;
use axum::response::Html;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_scalar::Scalar;

use crate::api::admin_secret_middleware::{ADMIN_SECRET_HEADER, admin_secret_middleware};
use crate::api::routes_about::AboutResponse;
use crate::api::routes_admin::AdminPingResponse;
use crate::api::routes_auth::{LoginRequest, LoginResponse, LoginUser, MeResponse};
use crate::api::routes_chats::{ChatSummary, ChatsListResponse};
use crate::api::routes_health::{HealthChecks, HealthResponse};
use crate::api::state::AppState;
use crate::api::webapp_auth_middleware::webapp_auth_middleware;
use crate::api::{
    routes_about, routes_admin, routes_auth, routes_chats, routes_config, routes_health,
};
use crate::models::{ChatConfigDto, ChatConfigPatch};

/// Top-level OpenAPI document. Schemas land in `components(schemas(...))` so
/// the Scalar UI can render the request / response bodies for every route.
/// Security schemes (`bearerAuth` / `adminSecret`) are added in
/// [`build_router`] via the `modifiers` hook so routes can `security(...)`-tag
/// themselves without a circular dep.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "vixen-server",
        description = "Telegram anti-spam bot — operational + dashboard API.",
    ),
    components(schemas(
        HealthResponse,
        HealthChecks,
        AboutResponse,
        LoginRequest,
        LoginResponse,
        LoginUser,
        MeResponse,
        ChatSummary,
        ChatsListResponse,
        ChatConfigDto,
        ChatConfigPatch,
        AdminPingResponse,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "ops", description = "Health + build metadata"),
        (name = "auth", description = "Telegram WebApp / Login Widget authentication"),
        (name = "chats", description = "Watched chats the moderator can manage"),
        (name = "config", description = "Per-chat configuration CRUD"),
        (name = "admin", description = "Operational endpoints behind a shared secret"),
    )
)]
struct ApiDoc;

/// Registers the bearer-JWT + admin-secret schemes on the OpenAPI document
/// so the Scalar UI's "Authorize" affordance offers them.
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .get_or_insert_with(utoipa::openapi::Components::new);
        components.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
        components.add_security_scheme(
            "adminSecret",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new(ADMIN_SECRET_HEADER))),
        );
    }
}

const REQUEST_ID_HEADER: &str = "x-request-id";

/// Build the application router with state, routes and middleware.
pub fn build_router(state: AppState) -> Router {
    let openapi_ui = state.config.resolve_openapi_ui();
    let cors_origins = state.config.cors_origins.clone();

    // Public (no auth): /health, /about, /api/v1/auth/telegram/login.
    let public = OpenApiRouter::new()
        .routes(routes!(routes_health::health))
        .routes(routes!(routes_about::about))
        .routes(routes!(routes_auth::telegram_login));

    // JWT-protected: /api/v1/auth/me, /api/v1/chats, /api/v1/chats/{id}/config.
    let protected = OpenApiRouter::new()
        .routes(routes!(routes_auth::me))
        .routes(routes!(routes_chats::list_chats))
        .routes(routes!(
            routes_config::get_config,
            routes_config::patch_config
        ))
        .layer(from_fn_with_state(state.clone(), webapp_auth_middleware));

    // Admin-secret: /admin/ping.
    let admin = OpenApiRouter::new()
        .routes(routes!(routes_admin::ping))
        .layer(from_fn_with_state(state.clone(), admin_secret_middleware));

    let (api_router, mut openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(public)
        .merge(protected)
        .merge(admin)
        .split_for_parts();

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
        let scalar_html = Scalar::new(openapi).to_html();
        app = app.route(
            "/scalar",
            axum::routing::get(move || {
                let html = scalar_html.clone();
                async move { Html(html) }
            }),
        );
    }

    let request_id = HeaderName::from_static(REQUEST_ID_HEADER);
    let cors = build_cors(&cors_origins);

    app.layer(SetRequestIdLayer::new(request_id.clone(), MakeRequestUuid))
        .layer(PropagateRequestIdLayer::new(request_id))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

fn build_cors(origins: &[String]) -> CorsLayer {
    let admin_header: TowerHeaderName = ADMIN_SECRET_HEADER
        .parse()
        .expect("admin secret header name is a valid lowercase token");
    let layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION, admin_header]);

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
