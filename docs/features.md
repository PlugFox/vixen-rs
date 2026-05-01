# Vixen-rs — Feature Backlog

Forward-looking work that is **not** part of the M0–M8 milestone roadmap (see [roadmap.md](roadmap.md)). Each entry is self-contained: motivation, sketch, expected impact, rough size, files most likely touched.

Status legend: 🟢 ready to start · 🟡 needs design first · 🔴 needs spec / discovery.

---

## Bot capability

### 🟡 Webhook horizontal scaling

**Why** — M8 lands webhook for one replica. Multi-replica unlocks zero-downtime deploys and survives a single-instance failure.

**Sketch** — leader-elect the daily-report / cleanup scheduler via Postgres advisory lock; webhook handlers themselves are stateless. CAS / verified-user caches via Redis are already shared.

**Touches** — `server/src/jobs/mod.rs` (advisory lock), production compose / k8s manifests.

**Effort** — 2–3 days plus production validation.

---

### 🟡 Picture-pick CAPTCHA mode

**Why** — digit-pad assumes the user can read Latin digits. A "tap the cat" 3-of-9 grid is more accessible for very mixed audiences.

**Sketch** — extend `CaptchaService::issue_challenge` to take a `mode: CaptchaMode` (per-chat config). Add `picture_pick` mode with 3-of-9 grid built from a curated asset bundle. Shares the same `(chat_id, user_id, challenge_id)` schema.

**Touches** — `server/src/services/captcha_service.rs`, `server/src/telegram/handlers/captcha.rs`, new asset bundle under `server/assets/captcha/picture_pick/`, migration for `chat_config.captcha_mode`.

**Effort** — 3–5 days.

---

### 🟡 Math CAPTCHA mode

**Why** — accessibility alternative; cheap to render; good for low-bandwidth chats.

**Sketch** — `math` mode (`a op b = ?` rendered as image). Same schema as digit-pad.

**Touches** — `server/src/services/captcha_service.rs`, `server/src/telegram/handlers/captcha.rs`.

**Effort** — 2 days.

---

### 🟡 Per-chat captcha policy override

**Why** — `chat_config.captcha_enabled` is binary today. Some chats want CAPTCHA only for users joining via deep links, only outside business hours, or only when CAS flags the user.

**Sketch** — `chat_config.captcha_policy` JSONB with simple predicates: `{"trigger": "join", "allowlist": [...]}`. Evaluated at challenge-issuance time.

**Touches** — `server/src/services/chat_config_service.rs`, `server/src/services/captcha_service.rs`, settings UI.

**Effort** — 2 days.

---

## Spam pipeline

### 🟡 CAS replacement / fallback

**Why** — `api.cas.chat` has had multi-hour outages. Vixen falls open today (no false-ban), but spam-detection efficacy drops noticeably during the outage window.

**Sketch** — add a second source: locally maintained block-list synced from one of the open Telegram-spammer datasets. Cache 24h. Combine via OR — either source flags = ban.

**Touches** — `server/src/services/cas_client.rs`, new `server/src/jobs/cas_sync.rs`.

**Effort** — 1–2 days.

---

### 🔴 Top-N spammed phrases on the public report

**Why** — moderators want to see which patterns the bot is catching this week, redacted enough that publishing them on the public page doesn't help spammers tune.

**Sketch** — server aggregates the top 10 most-hit `spam_messages` rows in the last 24h, double-redacts (URLs / @mentions / phone numbers / specific brand strings), exposes via `GET /report/{slug}/top-phrases`.

**Touches** — `server/src/services/report_service.rs`, redaction helper, `website/src/features/reports/components/top-phrases.tsx`.

**Effort** — 2 days. Needs a redaction-quality review before exposing publicly.

---

### 🟡 Spam-rule weight tuning per chat

**Why** — different communities have different baselines. A tech chat has more URLs; a music chat has more emoji. The current global weights are a compromise.

**Sketch** — `chat_config.spam_weights` JSONB with override per feature. Spam pipeline reads `chat_config` → falls back to global default. Dashboard form to edit.

**Touches** — `server/src/services/spam_service.rs`, `chat_config` migration, `website/src/features/settings/`.

**Effort** — 2–3 days.

---

## Reports

### 🟢 Ad-hoc report on demand

**Why** — moderators sometimes want a report for an arbitrary window ("last week", "this month") without waiting for the scheduler.

**Sketch** — `POST /api/v1/chats/{id}/reports/generate` with `from`/`to` query params. Same aggregator as the daily job, returns the chart inline + counts as JSON. Per-user cooldown to avoid abuse.

**Touches** — `server/src/api/routes_reports.rs`, dashboard generate-report form.

**Effort** — 1–2 days.

---

### 🟢 OpenAI summary cost guardrail

**Why** — the optional summary feature can run away if the chat has a heavy day. Today there's a per-chat per-day token cap, but no observability.

**Sketch** — record token usage per chat per day in `daily_stats('openai_tokens')`. Surface in the dashboard's report view. Hard cap stays; soft warning at 80%.

**Touches** — `server/src/services/summary_service.rs`, `server/src/services/report_service.rs`, dashboard chart.

**Effort** — 1 day.

---

## Dashboard

### 🟢 Audit-log search

**Why** — `moderation_actions` accumulates fast. Moderators should filter by user_id, action type, or date range.

**Sketch** — keyset-paginated `GET /api/v1/chats/{id}/actions?...filters...`. Reuse the existing audit-log table component from M5.

**Touches** — `server/src/api/routes_moderation.rs`, `website/src/features/moderation/`.

**Effort** — 1–2 days.

---

### 🟡 Bulk verify / unban

**Why** — when CAS or a heuristic mis-fires for a batch, manually unbanning each user is tedious.

**Sketch** — multi-select in the action ledger, "Reverse selected" button, atomic transaction.

**Touches** — `server/src/services/moderation_service.rs`, dashboard.

**Effort** — 1 day.

---

## Public report

### 🔴 PWA install promo

**Why** — Telegram WebApp offers a "Add to Home Screen" affordance, but the public report (browser-side) doesn't surface the standard `beforeinstallprompt`.

**Sketch** — small `PwaInstallBanner` component subscribed to `beforeinstallprompt`. Throttled by `localStorage`.

**Effort** — 1 day. Low priority — the public report is a read-only page.

---

## Cross-cutting follow-ups

- **Webhook horizontal scaling** is the single biggest architectural change post-M8.
- **Spam corpus** under `server/tests/spam_corpus/*.yaml` will outgrow git over time — at ~1k samples per rule consider Git LFS or a separate fixture repo.
- **i18n drift** — every new feature must add the same key in both `en` and `ru`. The CI parity check from M5 catches missed translations.
- **Captcha asset versioning** — the asset CHANGELOG is manual. A small CI step that rejects PRs touching `server/assets/captcha/*.ttf` or `*.webp` without an accompanying CHANGELOG entry would prevent the immutability rule being violated by accident.
