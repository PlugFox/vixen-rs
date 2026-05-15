//! `/api/v1/chats/{chat_id}/moderation/*` — moderator actions surfaced to the
//! dashboard. Every endpoint requires a JWT (`webapp_auth_middleware`) and
//! enforces the chat-scope IDOR guard via [`AuthContext::can_access`].
//!
//! Mutations (`ban`, `unban`, `verify`) re-use the same service paths the
//! Telegram slash-command handlers use, so the audit trail, idempotency
//! guarantees and Redis cache writes are identical between UI-driven and
//! command-driven moderation. This keeps the ledger a single source of truth
//! regardless of where the action originated.
//!
//! Reads (`actions`, `verified`, `banned`) use **opaque keyset cursors**
//! (`crate::utils::cursor`) rather than offset pagination — the ledger grows
//! append-only and offset pagination would re-walk the prefix on every page.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use utoipa::{IntoParams, ToSchema};

use crate::api::AuthContext;
use crate::api::response::ApiResult;
use crate::api::state::AppState;
use crate::models::moderation_action::ActorKind;
use crate::services::captcha::Outcome as CaptchaOutcome;
use crate::services::moderation_service::{Action, ApplyContext, Outcome as ModOutcome};
use crate::utils::cursor;
use crate::{api_error, api_success};

/// Hard cap on `limit`; clients can ask for less but never more.
const MAX_PAGE_SIZE: i64 = 200;
/// Default `limit` when the client omits it.
const DEFAULT_PAGE_SIZE: i64 = 50;

fn clamp_limit(opt: Option<i64>) -> i64 {
    opt.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE)
}

// ── DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ModerationActionItem {
    pub id: Uuid,
    pub chat_id: i64,
    pub target_user_id: i64,
    pub action: String,
    pub actor_kind: String,
    pub actor_user_id: Option<i64>,
    pub message_id: Option<i32>,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ModerationActionsListResponse {
    pub items: Vec<ModerationActionItem>,
    pub has_more: bool,
    /// Opaque cursor to feed back into `?cursor=…` to fetch the next page.
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VerifiedUserItem {
    pub user_id: i64,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VerifiedUsersListResponse {
    pub items: Vec<VerifiedUserItem>,
    pub has_more: bool,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BannedUserItem {
    pub user_id: i64,
    pub banned_at: DateTime<Utc>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BannedUsersListResponse {
    pub items: Vec<BannedUserItem>,
    pub has_more: bool,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ModerationActionResponse {
    pub id: Option<Uuid>,
    /// `applied` (new row inserted) or `already_applied` (idempotency hit).
    pub outcome: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BanRequest {
    pub user_id: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UnbanRequest {
    pub user_id: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyRequest {
    pub user_id: i64,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ActionsQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    /// One of the values in `ModerationActionKind` (`ban`, `unban`, `verify`,
    /// `delete`, `captcha_expired`, `captcha_failed`, `kick`, `mute`,
    /// `unmute`, `unverify`). Single-valued for v1 — UI filters one kind
    /// at a time.
    pub action: Option<String>,
    /// `bot` or `moderator`.
    pub actor_kind: Option<String>,
    /// Filter by target user ID (a single user's history).
    pub target_user_id: Option<i64>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

// ── GET .../moderation/actions ─────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/chats/{chat_id}/moderation/actions",
    tag = "moderation",
    params(
        ("chat_id" = i64, Path, description = "Telegram chat ID (i64)"),
        ActionsQuery,
    ),
    responses(
        (status = 200, description = "Audit page"),
        (status = 400, description = "Bad cursor or unknown filter value"),
        (status = 401, description = "Missing or invalid JWT"),
        (status = 403, description = "Caller is not a moderator of this chat"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_actions(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(chat_id): Path<i64>,
    Query(q): Query<ActionsQuery>,
) -> ApiResult<ModerationActionsListResponse> {
    if !ctx.can_access(chat_id) {
        return api_error!(
            "FORBIDDEN",
            "You are not a moderator of this chat",
            StatusCode::FORBIDDEN
        );
    }

    // Decode cursor up front — bad cursors are 400.
    let cursor_pair: Option<(DateTime<Utc>, Uuid)> = match q.cursor.as_deref() {
        None => None,
        Some(s) => match cursor::decode(s) {
            Ok(p) => Some(p),
            Err(_) => {
                return api_error!("BAD_CURSOR", "cursor is malformed", StatusCode::BAD_REQUEST);
            }
        },
    };

    let limit = clamp_limit(q.limit);

    // Fetch limit + 1 to know whether there's a next page without a second
    // round-trip.
    let fetch = limit + 1;
    let (cur_ts, cur_id): (Option<DateTime<Utc>>, Option<Uuid>) = match cursor_pair {
        Some((t, i)) => (Some(t), Some(i)),
        None => (None, None),
    };

    let rows = match sqlx::query!(
        r#"
        SELECT
            id,
            chat_id,
            target_user_id,
            action,
            actor_kind,
            actor_user_id,
            message_id,
            reason,
            created_at
        FROM moderation_actions
        WHERE chat_id = $1
          AND ($2::text       IS NULL OR action       = $2)
          AND ($3::text       IS NULL OR actor_kind   = $3)
          AND ($4::bigint     IS NULL OR target_user_id = $4)
          AND (
              $5::timestamptz IS NULL
              OR (created_at, id) < ($5, $6)
          )
        ORDER BY created_at DESC, id DESC
        LIMIT $7
        "#,
        chat_id,
        q.action,
        q.actor_kind,
        q.target_user_id,
        cur_ts,
        cur_id,
        fetch,
    )
    .fetch_all(state.db.pool())
    .await
    {
        Ok(r) => r,
        Err(e) => return api_error!("DB_ERROR", e.to_string()),
    };

    let has_more = rows.len() as i64 > limit;
    let mut items: Vec<ModerationActionItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| ModerationActionItem {
            id: r.id,
            chat_id: r.chat_id,
            target_user_id: r.target_user_id,
            action: r.action,
            actor_kind: r.actor_kind,
            actor_user_id: r.actor_user_id,
            message_id: r.message_id,
            reason: r.reason,
            created_at: r.created_at,
        })
        .collect();

    let cursor_out = if has_more {
        items
            .last()
            .map(|last| cursor::encode(&(last.created_at, last.id)))
    } else {
        None
    };

    // `items` is already in the right shape; drop the &mut once we have the
    // cursor.
    let _ = &mut items;

    api_success!(ModerationActionsListResponse {
        items,
        has_more,
        cursor: cursor_out,
    })
}

// ── POST .../moderation/ban ────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/chats/{chat_id}/moderation/ban",
    tag = "moderation",
    params(("chat_id" = i64, Path)),
    request_body = BanRequest,
    responses(
        (status = 200, description = "Action applied (or already in effect)"),
        (status = 400, description = "Malformed body"),
        (status = 401, description = "Missing or invalid JWT"),
        (status = 403, description = "Caller is not a moderator of this chat"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn ban_user(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(chat_id): Path<i64>,
    Json(body): Json<BanRequest>,
) -> ApiResult<ModerationActionResponse> {
    if !ctx.can_access(chat_id) {
        return api_error!(
            "FORBIDDEN",
            "You are not a moderator of this chat",
            StatusCode::FORBIDDEN
        );
    }
    if body.user_id <= 0 {
        return api_error!(
            "BAD_USER_ID",
            "user_id must be a positive Telegram user id",
            StatusCode::BAD_REQUEST
        );
    }

    let action = Action::Ban {
        reason: body
            .reason
            .clone()
            .unwrap_or_else(|| "manual ban (dashboard)".to_string()),
        until: None,
    };
    let apply_ctx = ApplyContext {
        chat_id,
        target_user_id: body.user_id,
        message_id: None,
        actor_kind: ActorKind::Moderator,
        actor_user_id: Some(ctx.user_id),
    };

    match state.moderation.apply(action, apply_ctx).await {
        Ok(ModOutcome::Applied) => api_success!(ModerationActionResponse {
            id: None,
            outcome: "applied".into(),
        }),
        Ok(ModOutcome::AlreadyApplied) => api_success!(ModerationActionResponse {
            id: None,
            outcome: "already_applied".into(),
        }),
        Err(e) => api_error!("MODERATION_FAILED", e.to_string()),
    }
}

// ── POST .../moderation/unban ──────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/chats/{chat_id}/moderation/unban",
    tag = "moderation",
    params(("chat_id" = i64, Path)),
    request_body = UnbanRequest,
    responses(
        (status = 200, description = "Action applied (or already in effect)"),
        (status = 400, description = "Malformed body"),
        (status = 401, description = "Missing or invalid JWT"),
        (status = 403, description = "Caller is not a moderator of this chat"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn unban_user(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(chat_id): Path<i64>,
    Json(body): Json<UnbanRequest>,
) -> ApiResult<ModerationActionResponse> {
    if !ctx.can_access(chat_id) {
        return api_error!(
            "FORBIDDEN",
            "You are not a moderator of this chat",
            StatusCode::FORBIDDEN
        );
    }
    if body.user_id <= 0 {
        return api_error!(
            "BAD_USER_ID",
            "user_id must be a positive Telegram user id",
            StatusCode::BAD_REQUEST
        );
    }

    let apply_ctx = ApplyContext {
        chat_id,
        target_user_id: body.user_id,
        message_id: None,
        actor_kind: ActorKind::Moderator,
        actor_user_id: Some(ctx.user_id),
    };

    match state.moderation.apply(Action::Unban, apply_ctx).await {
        Ok(ModOutcome::Applied) => api_success!(ModerationActionResponse {
            id: None,
            outcome: "applied".into(),
        }),
        Ok(ModOutcome::AlreadyApplied) => api_success!(ModerationActionResponse {
            id: None,
            outcome: "already_applied".into(),
        }),
        Err(e) => api_error!("MODERATION_FAILED", e.to_string()),
    }
}

// ── POST .../moderation/verify ─────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/chats/{chat_id}/moderation/verify",
    tag = "moderation",
    params(("chat_id" = i64, Path)),
    request_body = VerifyRequest,
    responses(
        (status = 200, description = "Verification applied (or already verified)"),
        (status = 400, description = "Malformed body"),
        (status = 401, description = "Missing or invalid JWT"),
        (status = 403, description = "Caller is not a moderator of this chat"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn verify_user(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(chat_id): Path<i64>,
    Json(body): Json<VerifyRequest>,
) -> ApiResult<ModerationActionResponse> {
    if !ctx.can_access(chat_id) {
        return api_error!(
            "FORBIDDEN",
            "You are not a moderator of this chat",
            StatusCode::FORBIDDEN
        );
    }
    if body.user_id <= 0 {
        return api_error!(
            "BAD_USER_ID",
            "user_id must be a positive Telegram user id",
            StatusCode::BAD_REQUEST
        );
    }

    let outcome = match state
        .captcha
        .verify_manual(chat_id, body.user_id, ctx.user_id)
        .await
    {
        Ok(o) => o,
        Err(e) => return api_error!("MODERATION_FAILED", e.to_string()),
    };

    // Populate the Redis verified-cache so the next join skips a PG hit.
    // Best-effort — a Redis miss here just means lazy fill on next join,
    // identical to the `/verify` slash command's behaviour.
    if let Err(e) = state
        .captcha_state
        .mark_verified(chat_id, body.user_id)
        .await
    {
        tracing::warn!(error = ?e, "redis mark_verified (dashboard verify) failed");
    }

    let outcome_str = match outcome {
        CaptchaOutcome::Solved => "applied",
        CaptchaOutcome::AlreadyVerified => "already_applied",
        _ => "applied",
    };
    api_success!(ModerationActionResponse {
        id: None,
        outcome: outcome_str.into(),
    })
}

// ── GET .../moderation/verified ────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/chats/{chat_id}/moderation/verified",
    tag = "moderation",
    params(
        ("chat_id" = i64, Path),
        ListQuery,
    ),
    responses(
        (status = 200, description = "Verified users page"),
        (status = 400, description = "Bad cursor"),
        (status = 401, description = "Missing or invalid JWT"),
        (status = 403, description = "Caller is not a moderator of this chat"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_verified(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(chat_id): Path<i64>,
    Query(q): Query<ListQuery>,
) -> ApiResult<VerifiedUsersListResponse> {
    if !ctx.can_access(chat_id) {
        return api_error!(
            "FORBIDDEN",
            "You are not a moderator of this chat",
            StatusCode::FORBIDDEN
        );
    }

    let cursor_pair: Option<(DateTime<Utc>, i64)> = match q.cursor.as_deref() {
        None => None,
        Some(s) => match cursor::decode(s) {
            Ok(p) => Some(p),
            Err(_) => {
                return api_error!("BAD_CURSOR", "cursor is malformed", StatusCode::BAD_REQUEST);
            }
        },
    };
    let limit = clamp_limit(q.limit);
    let fetch = limit + 1;
    let (cur_ts, cur_uid) = match cursor_pair {
        Some((t, u)) => (Some(t), Some(u)),
        None => (None, None),
    };

    let rows = match sqlx::query!(
        r#"
        SELECT user_id, verified_at
        FROM verified_users
        WHERE chat_id = $1
          AND (
              $2::timestamptz IS NULL
              OR (verified_at, user_id) < ($2, $3)
          )
        ORDER BY verified_at DESC, user_id DESC
        LIMIT $4
        "#,
        chat_id,
        cur_ts,
        cur_uid,
        fetch,
    )
    .fetch_all(state.db.pool())
    .await
    {
        Ok(r) => r,
        Err(e) => return api_error!("DB_ERROR", e.to_string()),
    };

    let has_more = rows.len() as i64 > limit;
    let items: Vec<VerifiedUserItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| VerifiedUserItem {
            user_id: r.user_id,
            verified_at: r.verified_at,
        })
        .collect();

    let cursor_out = if has_more {
        items
            .last()
            .map(|last| cursor::encode(&(last.verified_at, last.user_id)))
    } else {
        None
    };

    api_success!(VerifiedUsersListResponse {
        items,
        has_more,
        cursor: cursor_out,
    })
}

// ── GET .../moderation/banned ──────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/chats/{chat_id}/moderation/banned",
    tag = "moderation",
    params(
        ("chat_id" = i64, Path),
        ListQuery,
    ),
    responses(
        (status = 200, description = "Currently-banned users page"),
        (status = 400, description = "Bad cursor"),
        (status = 401, description = "Missing or invalid JWT"),
        (status = 403, description = "Caller is not a moderator of this chat"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_banned(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(chat_id): Path<i64>,
    Query(q): Query<ListQuery>,
) -> ApiResult<BannedUsersListResponse> {
    if !ctx.can_access(chat_id) {
        return api_error!(
            "FORBIDDEN",
            "You are not a moderator of this chat",
            StatusCode::FORBIDDEN
        );
    }

    let cursor_pair: Option<(DateTime<Utc>, i64)> = match q.cursor.as_deref() {
        None => None,
        Some(s) => match cursor::decode(s) {
            Ok(p) => Some(p),
            Err(_) => {
                return api_error!("BAD_CURSOR", "cursor is malformed", StatusCode::BAD_REQUEST);
            }
        },
    };
    let limit = clamp_limit(q.limit);
    let fetch = limit + 1;
    let (cur_ts, cur_uid) = match cursor_pair {
        Some((t, u)) => (Some(t), Some(u)),
        None => (None, None),
    };

    // "Banned" = users whose *most recent terminal action* is `ban` (not
    // `unban`). The CTE picks the latest of {ban, unban} per user, the
    // outer SELECT filters to ban and keyset-paginates.
    let rows = match sqlx::query!(
        r#"
        WITH last_action AS (
            SELECT DISTINCT ON (target_user_id)
                target_user_id,
                action AS last_action,
                created_at AS last_at,
                reason
            FROM moderation_actions
            WHERE chat_id = $1 AND action IN ('ban', 'unban')
            ORDER BY target_user_id, created_at DESC
        )
        SELECT
            target_user_id AS "user_id!",
            last_at        AS "banned_at!",
            reason
        FROM last_action
        WHERE last_action = 'ban'
          AND (
              $2::timestamptz IS NULL
              OR (last_at, target_user_id) < ($2, $3)
          )
        ORDER BY last_at DESC, target_user_id DESC
        LIMIT $4
        "#,
        chat_id,
        cur_ts,
        cur_uid,
        fetch,
    )
    .fetch_all(state.db.pool())
    .await
    {
        Ok(r) => r,
        Err(e) => return api_error!("DB_ERROR", e.to_string()),
    };

    let has_more = rows.len() as i64 > limit;
    let items: Vec<BannedUserItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| BannedUserItem {
            user_id: r.user_id,
            banned_at: r.banned_at,
            reason: r.reason,
        })
        .collect();

    let cursor_out = if has_more {
        items
            .last()
            .map(|last| cursor::encode(&(last.banned_at, last.user_id)))
    } else {
        None
    };

    api_success!(BannedUsersListResponse {
        items,
        has_more,
        cursor: cursor_out,
    })
}
