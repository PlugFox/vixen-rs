# Changelog

All notable changes are tracked here using the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format and adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The vixen-rs monorepo ships two artifacts with separate version numbers:

- **server** — Rust crate `vixen-server`, source in [`server/`](server/), versioned in `server/Cargo.toml`.
- **website** — TypeScript SPA `vixen-website`, source in [`website/`](website/), versioned in `website/package.json`.

Each release entry calls out the affected component(s) via a `(server)` / `(website)` / `(infra)` tag. Skip CHANGELOG updates only for trivial internal-only changes (formatting, comment tweaks, refactors with no behaviour change).

## [Unreleased]

### Fixed

- moderation ledger no longer rolls back the row outside of a transaction after
  a fatal Telegram failure. `ModerationService::apply` now performs the INSERT,
  the bot call, and the COMMIT inside one tx; a fatal bot error rolls back the
  whole tx atomically, so a parallel `apply` for the same `(chat, user, action,
  message_id)` cannot observe the row before it is durable and can no longer
  receive a misleading `AlreadyApplied` for an action that ultimately did not
  stick. (server)
- CAS fail-open verdicts (network error, non-2xx, body parse failure) are no
  longer cached. Previously a brief CAS outage poisoned Moka for 1 h and
  Redis for 24 h with stale `Clean` results; now `lookup` retries the upstream
  on the next call. Genuine `Clean` / `Flagged` from the upstream are still
  cached. (server)
- `/verify` permission gate widened to `chat_moderators` OR chat admin,
  matching `/ban` and `/unban`. Previously only chat admins could run it,
  which was an undocumented asymmetry with the rest of the moderator
  surface. (server)
- chat-admin Redis cache write in `/ban` / `/unban` / `/verify` now filters
  `Banned` / `Left` admins, matching the `message_gate` filter. Stale
  ex-admin ids can no longer leak into the cache via the command path. (server)

### Changed

- captcha message captions redesigned for clarity. The progress mask now shows
  the digits the user has actually typed (keycap emoji `1️⃣` for filled slots,
  `⬜` for empty) instead of opaque `●` / `○` circles, so the user can verify
  what they entered before the 4th press triggers a solve. Captions across the
  lifecycle (initial issue, in-progress edit, wrong attempt, refresh) now share
  a single renderer (`services::captcha::caption`), each sentence sits on its
  own line, and short emoji headers (👋 / 🔐 / ❌ / 🎯) replace the previous
  run-on text. Plain text only — no `parse_mode`, so user mentions still work
  without MarkdownV2 escaping. (server)

### Added

- M2 spam pipeline. `services::spam::SpamService::inspect(message)` runs the
  cascade documented in `server/docs/spam-detection.md`: NFKC + lowercase +
  zero-width / combining-mark strip + whitespace collapse → xxh3-64 →
  `spam_messages` dedup → CAS lookup → weighted n-gram phrase match. Verdicts
  short-circuit; messages shorter than 48 normalized chars are Allowed
  outright. The handler dispatches the resulting `Verdict::{Allow, Delete,
  Ban}` through `ModerationService::apply` so the ledger row and the bot
  side-effect stay paired. Wired into `telegram::handlers::message_gate`'s
  verified-user fast path (admins skip; unverified users still go through
  captcha). (server)
- `services::cas_client::CasClient` — Combot Anti-Spam lookup with two-tier
  cache: Moka 1 h front, Redis 24 h back, HTTP `{base_url}/check?user_id={id}`
  on miss with a 3 s timeout. Any failure (network, 5xx, timeout) returns
  `Verdict::Clean` (fail-open per docs — false positives are worse than
  missed catches). Positive AND negative verdicts are write-through cached so
  clean traffic doesn't hammer CAS. `base_url` is injected so wiremock
  integration tests at `server/tests/cas_client.rs` can swap it. (server)
- `services::moderation_service::ModerationService` — single source of truth
  for ban / unban / delete (auto and manual). `apply()` writes the ledger row
  via `INSERT … ON CONFLICT DO NOTHING RETURNING id`; on conflict the bot
  call is skipped and the function returns `Outcome::AlreadyApplied`.
  Non-fatal Telegram errors (bot not admin, user not in chat, message gone)
  keep the ledger row to record intent; fatal errors roll the row back so a
  retry can succeed. id-mode bans/unbans (`message_id IS NULL`) open a
  transaction and `SELECT … FOR UPDATE` on the chat row before a behaviour
  check (last terminal action == target?), serialising concurrent id-mode
  attempts that the NULL-distinct unique constraint can't dedup on its own.
  `is_moderator(chat_id, user_id)` reads `chat_moderators` with a Moka 5 min
  cache; `invalidate_moderator` flushes a single (chat, user) entry after
  external writes. Integration suite: `server/tests/moderation_service.rs`
  covers message-scoped idempotency, id-mode atomicity, and unban-after-ban
  via `teloxide_tests::MockBot`. (server)
- spam phrase corpus ported from the Dart prototype
  (`vixen/lib/src/anti_spam.dart` `$spamPhrases`). 115 entries, English +
  Russian mix across finance / urgency / discounts / health / crypto /
  gambling / loans / real-estate / courses / MLM categories, plus
  obfuscated variants like `'для yдaлённoгo зaрaбoткa'`. Stored as a static
  `LazyLock<PhraseSet>`; matches by substring on normalized text. Default
  weight per phrase is 1.0; `chat_config.spam_weights` JSONB overrides
  per-phrase. `score(normalized, &weights)` returns the aggregate score and
  the matched-phrase list for the moderation ledger's `reason` JSON
  (`{"matched_rules":["ngram"], "ngram_phrases":[…], "score":2.0,
  "threshold":1.0}`). (server)
- `/ban` and `/unban` slash commands. `/ban` accepts both reply-mode
  (replied-to message — sets `message_id` on the ledger row) and id-mode
  (`/ban <user_id> [reason]`). `/unban <user_id>` is id-only by design —
  banned users have their messages deleted, so a reply target wouldn't
  exist. Permission gate: moderator (DB row in `chat_moderators`, Moka 5 min
  cache) OR chat admin (existing M1 admin cache, Redis 6 h TTL → live
  `getChatAdministrators` fallback). Successful action best-effort deletes
  the moderator's command message; non-moderators see "Only chat moderators
  or admins can run /ban". A double-ban replies "User N is already banned"
  instead of writing a duplicate ledger row. (server)
- `services::spam::dedup` helpers — `lookup`, `bump`, `record` for the
  `spam_messages` table. n-gram and CAS branches `record()` so subsequent
  copies short-circuit at the dedup step in O(1). Sample bodies are
  truncated to 4 KiB on a UTF-8 char boundary before storage. (server)
- `jobs::spam_cleanup` background job. Tick every 24 h, cancel-aware. Single
  `DELETE FROM spam_messages WHERE last_seen < NOW() - $retention_days`.
  Configurable via `CONFIG_SPAM_RETENTION_DAYS` (default 14, matches the
  Dart prototype). Wired into `jobs::spawn_all` alongside `captcha_expiry`.
  Tests in `server/tests/spam_cleanup.rs` seed 1 d / 7 d / 20 d / 100 d rows
  and assert only the > 14 d ones drop. (server)
- spam corpus regression tests under `server/tests/spam_corpus/`:
  `phrase_match.yaml`, `clean_messages.yaml`, `xxh3_dedup.yaml`. Loader in
  `server/tests/spam_pipeline.rs` walks every YAML through
  `SpamService::inspect` and asserts the labelled verdict per sample.
  `must_ban_after_first` schema feeds dedup by running the same input
  twice and expecting a Ban on the second pass. Adding a new spam rule
  without corpus samples fails the test. (server)
- `teloxide_tests = "0.2"` dev-dependency (the last release targeting
  teloxide 0.13). Used by `tests/moderation_service.rs` for `MockBot` +
  recorded API call assertions. Handler-level integration suites built on
  the same harness: `tests/handlers_ban.rs` (5 tests — `/ban`/`/unban`
  reply-mode, id-mode, permission denial, idempotency),
  `tests/handlers_member_update.rs` (4 tests — fresh-join captcha,
  already-verified skip, owner-join skip, role-change skip),
  `tests/handlers_message_gate.rs` (3 tests — unverified delete + captcha,
  live-challenge skip-reissue, verified-user spam delete),
  `tests/handlers_captcha_callback.rs` (5 tests — digit press, correct
  solve, wrong solve, ownership rejection, backspace).
  `tests/common/mod.rs` ships shared helpers (`make_state`, seeding,
  `unique_chat_id`, `unique_message_id`) so each suite stays focused on
  the behaviour it asserts. (server)
- `CONFIG_CAS_BASE_URL` (default `https://api.cas.chat`) — testable CAS
  endpoint override. `CONFIG_SPAM_RETENTION_DAYS` (default 14) — spam
  message retention window. (server)
- `unicode-normalization` dependency for NFKC pass in
  `services::spam::normalize`. (server)
- captcha message gate. New non-command handler `telegram::handlers::message_gate` deletes every message from an unverified, non-admin user in a watched chat and (re-)issues a captcha on the spot if there's no live challenge already. Verified users bypass via the existing `cap:verified` Redis cache → PG fallback; chat admins bypass via a new `cap:admins:{chat_id}` JSON-list cache (6 h TTL, lazy-populated from `bot.get_chat_administrators` on miss). Wired into `telegram::dispatcher` after the `filter_command` branch so `/help`/`/status`/`/verify` keep working for unverified users. (server)
- `CaptchaService::active_challenge_message_id(chat_id, user_id) -> Result<Option<Option<i32>>>` — non-destructive check used by the message gate to decide whether to reissue. Outer `Option` is "live row?", inner `Option<i32>` is the recorded `telegram_message_id` (may still be NULL if the gate fires between `issue_challenge` and `record_message_id`). Covered by a new integration test that walks all four states. (server)
- `CaptchaState::set_admins` / `get_admins` — Redis-backed admin cache for the message gate. JSON-encoded `Vec<i64>` so a missing key stays distinguishable from an empty admin list with one round-trip. (server)
- `services::captcha::keyboard::CALLBACK_PREFIX_WITH_COLON` — `&'static str` constant used by the dispatcher's per-update filter so the prefix check no longer allocates a `String` via `format!()` on every callback. (server)
- [mise](https://mise.jdx.dev) project config: `mise.toml` pins bun, sqlx-cli (postgres+rustls, no MySQL/SQLite), taplo, jq, yq; sets `DATABASE_URL`, `REDIS_URL`, `RUST_LOG`, `SQLX_OFFLINE` defaults matching `docker/docker-compose.yml`; ships task wrappers mirroring the Claude Code slash commands — `mise run server:check / server:run / server:sqlx-prepare`, `mise run website:check / website:typecheck / website:build / website:dev`, `mise run db:up / db:down / db:migrate / db:psql`, `mise run bot:token`, plus aggregate `mise run check`. The slash commands keep working without mise; mise is purely additive. Rust toolchain is pinned via `server/rust-toolchain.toml` (channel 1.88, rustfmt + clippy, profile=minimal) so rustup remains the source of truth. `mise.local.toml` is gitignored for per-developer overrides; `.claude/settings.json` allow-lists read-only `mise *` invocations. (infra)
- M1 captcha pipeline. New joiners of a watched chat are silenced via `bot.restrict_chat_member`, then served a deterministic 480×180 lossless WebP digit-pad captcha (gradient backgrounds, ±15° rotated digits, Bezier-curve + dot-field noise; ≤30 KB; same UUID always renders byte-identical bytes). Inline keyboard `1 2 3 / 4 5 6 / 7 8 9 / ⌫ 0 ↻`; callback data `vc:{short}:{op}`. Solving deletes the challenge row, inserts `verified_users` and a `moderation_actions` ledger row in one transaction; `SELECT … FOR UPDATE` serialises concurrent taps, and a re-fired callback after a successful solve returns `AlreadyVerified` instead of double-acting. Wrong-final and refresh paths also live here. Implementation: `server/src/services/captcha/{service,render,keyboard,fonts}.rs`. (server)
- `captcha_expiry` background job (60 s tick). Single `DELETE FROM captcha_challenges WHERE expires_at < NOW() RETURNING …` sweeps stale rows; per-row cleanup is `delete_message` + `unban_chat_member` + `kick_chat_member` + `unban_chat_member` (kick = ban + immediate unban so the user can rejoin), followed by two `moderation_actions` ledger rows (`captcha_expired` + `kick`) protected by the existing `(chat_id,target_user_id,action,message_id)` uniqueness key. Wired into `bin/server.rs` via `jobs::spawn_all` under the shared `CancellationToken`. (server)
- Telegram handlers: `chat_member` → restrict-then-issue; `callback_query` filtered on the `vc:` prefix → digit-pad solve / backspace / refresh; `message` → slash-command dispatch. The watched-chats filter sits at the dispatcher trunk so non-watched chats never reach a handler. teloxide 0.13's `Dispatcher::dispatch` introspects the handler tree and auto-subscribes to the right `allowed_updates` (incl. `chat_member`). (server)
- `/verify` slash command. Reply-mode (`/verify` as a reply to the target user) and id-mode (`/verify <user_id>`). Permission check via `bot.get_chat_administrators`; on success the challenge row is deleted, `verified_users` row is inserted and `moderation_actions` is appended with `actor_kind='moderator'`. Idempotent — verifying an already-verified user is a no-op that returns `AlreadyVerified`. (server)
- Startup chat seeding. `bin/server.rs` calls `database::ensure_watched_chats` after Postgres connects, idempotently inserting a row in `chats` and `chat_config` for every `CONFIG_CHATS` entry so all foreign-key references resolve from the very first captcha. (server)
- `AppState.captcha: Arc<CaptchaService>` (initialised from `Fonts::load` + the existing PgPool) — shared across the HTTP handlers, the teloxide dispatcher and the `captcha_expiry` job. (server)
- New SQLx models: `models::CaptchaChallenge`, `models::VerifiedUser`, `models::ModerationAction` plus the `ModerationActionKind` and `ActorKind` enums. (server)
- 9 integration tests in `server/tests/captcha.rs` covering issue, idempotent re-issue, solve correct / wrong-decrement / wrong-final / expired / not-found, two-task concurrent solve (one winner + one `AlreadyVerified`), and `verify_manual` ledger writes. Plus 4 renderer unit tests pinning size budget, determinism for identical inputs, divergence for distinct seeds, and rejection of malformed solutions. (server)
- bot now publishes its slash-command list via `set_my_commands` on startup so `/help`, `/status`, `/verify` appear in Telegram's command menu. (server)

### Changed

- `bin/server.rs` now applies pending SQLx migrations on startup (right after the Postgres health check) via `Database::migrate`. The call is idempotent — SQLx tracks applied versions in `_sqlx_migrations` — so existing databases are unaffected; freshly-provisioned ones (or a dropped Docker volume in dev) come up without a separate `sqlx migrate run`. (server)
- `bin/server.rs` now searches both `.env` and `config/.env` (in that order, first match wins) when loading the local-dev env file via `dotenvy`. Previously only `.env` in CWD was tried, which forced developers using the repo-conventional `server/config/.env` location to pass everything via CLI flags. (server)
- `AppState` gains `spam: Arc<SpamService>` and `moderation: Arc<ModerationService>`. `bin/server.rs` constructs the `Bot` before `AppState` (M2 services capture it) and wires `CasClient` + `SpamService` + `ModerationService` into the shared state. `jobs::spawn_all` spawns `spam_cleanup` alongside `captcha_expiry`. (server)
- `telegram::handlers::message_gate` now runs `state.spam.inspect()` for verified non-admin messages with body text. Non-`Allow` verdicts are dispatched through `state.moderation.apply()`. The captcha gate (unverified non-admin path) is unchanged. Pipeline failures are logged at `warn!` but do not block the conversation — captcha is the hard guarantee, spam is defense in depth. (server)
- `Command::Ban(String)` and `Command::Unban(String)` added to the `BotCommands` derive in `telegram::commands`; `set_my_commands` on startup publishes them to Telegram's command menu. `/help` text updated to enumerate all 5 commands. (server)
- captcha policy pivot: **the bot no longer restricts or kicks anyone for failing or ignoring a captcha.** The only enforcement primitive is message deletion via the new `message_gate` handler (above). Removed: `bot.restrict_chat_member` from `member_update::handle` (join no longer mutes), the lift-restriction call from `commands::verify`, the `bot.kick_chat_member` + `unban_chat_member` round-trips from `captcha::on_kick` (renamed `on_failed`) and from `jobs::captcha_expiry::process_expired`. `Outcome::WrongFinal` and `Outcome::Expired` from `CaptchaService::solve()` no longer write a `kick` ledger row alongside `captcha_failed` / `captcha_expired` (M2 spam ban will own the `kick` action). The integration tests `solve_final_wrong_drops_row_and_writes_ledger` and `solve_expired_drops_row_and_writes_ledger` assert that `kick` rows are NOT written. The `kick` action stays in the `moderation_actions.action` CHECK constraint for the M2 spam-ban path. (server)
- `captcha_expiry` background job is now batched and cancel-aware. The single unbounded `DELETE … WHERE expires_at < NOW() RETURNING …` is replaced by a `LIMIT 200` CTE-driven `DELETE` that loops until the queue is empty or the shutdown token fires; the token is also checked between rows in a batch. After a long downtime the queue drains in bounded chunks instead of one statement that could hold locks for seconds. Per-row cleanup is now just `bot.delete_message` (best-effort) + the `captcha_expired` audit row. (server)
- captcha issuance now runs from two call sites: `chat_member` (fresh joiner, skipping owner/admin transitions) AND `message_gate` (every non-command message from an unverified non-admin user in a watched chat that has no live challenge already). Both paths funnel into the same `issue_challenge` + `send_photo` + `record_message_id` + `set_meta` sequence. (server)
- `moderation_actions.action` CHECK extended with `captcha_expired`, `captcha_failed`, `kick` so the M1 expiry / kick / final-wrong paths can land idempotent ledger rows under the existing `(chat_id, target_user_id, action, message_id)` uniqueness key. Migration: `server/migrations/20260502120000_extend_moderation_actions_check.{up,down}.sql`. `server/docs/database.md` updated. (server)
- `server/src/telegram/dispatcher.rs::build_dispatcher` now takes `AppState` (in addition to `Bot` + `WatchedChats`) and assembles three branches under the watched-chats filter: `chat_member` → captcha issuance, `callback_query` (`vc:` prefix) → captcha solve, `message` (command-parsed) → command dispatch. `bin/server.rs` constructs the `Bot` once and clones it into both the dispatcher and the job runner so they share the same `Throttle`-able handle. (server)
- `server/docs/captcha.md` rewritten to match the M1 implementation: state diagram, per-transition atomicity table, exact SQL, callback-data scheme `vc:{short}:{op}` with `bs`/`rf` ops, asset-immutability rules and failure-mode matrix. `server/docs/bot.md` callback section updated to spell out the same scheme. `server/assets/captcha/CHANGELOG` records the v1 visual style (gradients, ±15° rotation, Bezier noise) without adding a new asset file. (server)

- `bin/server.rs` now joins the `chat_config:*` Redis pubsub task at shutdown alongside the HTTP server and Telegram dispatcher, so the subscriber's final goodbye log lands inside the 30s drain window. (server)
- `bin/server.rs` shutdown signal listener is now `cfg(unix)`-gated. On Unix it still watches both `SIGTERM` and `SIGINT` (production deploys); on non-Unix it falls back to `tokio::signal::ctrl_c()` so the binary builds for `cargo check` / IDE workflows on Windows. (server)
- `/scalar` UI is now rendered by `utoipa-scalar` in standalone mode (`Scalar::new(spec).to_html()`): the OpenAPI document is embedded directly into the served HTML, so the page renders without the second round-trip to `/api/v1/openapi.json`. Standalone (rather than `Servable`) is chosen because `utoipa-scalar 0.3` pins `axum 0.8` while the project is on `axum 0.7`. (server)
- `server/docs/api.md` and `server/docs/observability.md` realigned with the actual `/health` and `/about` response shapes (`/health` reports both `db` and `redis` checks; `/about` exposes `built_at`, `rust_version`, `profile`, `target` rather than `started_at`). `observability.md` also documents the borrowed `RedactedToken<'a>` wrapper alongside the owning secret newtypes. (server)
- `server/docs/database.md` now correctly describes the per-connection `statement_timeout` as session-scoped `SET` applied once on connect (matching `Database::after_connect` in `server/src/database/postgres.rs`), rather than `SET LOCAL` (which would only last for one transaction). (server)
- `CONFIG_CORS_ORIGINS` doc-comment in `server/src/config/mod.rs` clarified: the `http://localhost:3000` default is the dev dashboard origin; production must override with the explicit dashboard origin, and an empty value disables cross-origin access entirely. (server)
- `server/config/template.env` instructs to copy to `server/.env` (matching `dotenvy::dotenv()` default search path) instead of the previously-documented `server/.env.local`. (server)
- `server/tests/redis_pubsub.rs` opt-in command corrected to `cargo test --test redis_pubsub -- --ignored`. (server)
- removed dead `allowed_updates()` helper — teloxide 0.13's `Dispatcher` auto-introspects allowed updates from the handler tree. (server)
- captcha renderer (`services::captcha::render`) rewritten for visual quality. (1) **2× supersample (960 × 540)** rasterised internally and **Lanczos3-downscaled** to the final 480 × 270 — every hard edge (rotated digit outlines, circle/rectangle/line strokes) gets free area-sampled anti-aliasing without bespoke AA primitives. (2) **Layered composition** ported from the Dart prototype (`.old/src/captcha/`): vertical gradient → 18..22 translucent background shapes (circles / rectangles / thick lines) → 4 rotated digits with per-digit accent colour → 30..40 quadratic Bézier "scribble" curves → 18..22 small foreground shapes (whites / greys, very low alpha) overlaid on top of the digits to defeat naive OCR while staying readable. (3) Six curated palettes (`twilight` / `forest` / `plum` / `sky-pastel` / `lavender-mist` / `coral`) — three deep, three light — picked deterministically from `xxh3(challenge_id)`. (4) **Lossless WebP encoded via `image::codecs::webp::WebPEncoder::new_lossless`** instead of the standalone `webp` crate; the captcha renderer now talks only to the `image` crate. Determinism is preserved (same UUID → byte-identical output, snapshot test pinned). Output ranges 60–110 KB typical, budget bumped to 150 KB to leave headroom (well under any Telegram limit). (server)
- removed `webp = "0.3"` from `server/Cargo.toml`. The captcha renderer is the only call site that ever needed it and now uses `image::codecs::webp::WebPEncoder` (already pulled in via the `image` crate's `webp` feature). One fewer transitive dep. (server)

### Fixed

- `commands::resolve_target` (`/verify <user_id>`) now rejects non-positive ids before the `as u64` cast that feeds Telegram APIs. A negative `i64` would otherwise wrap into a giant `u64` and the bot would issue malformed restrict/verify calls. Telegram user ids are always positive on the wire; non-positive input now falls through to the standard "Reply to a user or pass /verify <user_id>." reply. (server)
- `MetaPayload::from_redis_string` no longer relies on a dead `if it.next().is_some()` branch after a `splitn(3, '|')` (which can never yield a 4th element). The parser now uses `split('|')` + an explicit `len() == 3` check, so the trailing-field test asserts what it claims to assert instead of passing because `parse::<u64>("60|extra")` happens to fail. (server)
- captcha callback handler's `digit_pressed` and `backspace` now log Redis errors via `warn!` instead of swallowing them with `unwrap_or_default()`. The doc comment at `services/captcha/state.rs` already promised "warn-log + cache miss" for Redis failures; the implementation now matches. (server)
- callback dispatcher prefix filter no longer reallocates a `String` via `format!("{CALLBACK_PREFIX}:")` on every callback; uses the new `CALLBACK_PREFIX_WITH_COLON` `&'static str` constant directly. (server)
- `server/Cargo.toml`: drop the `ttf` feature from `plotters`. The transitive `yeslogic-fontconfig-sys` dependency required `libfontconfig1-dev` on the GitHub Actions runner, which broke `Clippy`, `Test`, and `SQLx prepare --check` in `.github/workflows/server-ci.yml`. Captcha rendering is unaffected — it uses `ab_glyph` directly with `include_bytes!(DejaVuSans.ttf)`. The `ttf` feature can be re-introduced in M3 when chart titles need on-image text, paired with an `apt install` step in CI. (infra)
- server test pipeline: `tests/captcha.rs`'s 9 `#[sqlx::test]` cases are now `#[ignore]`-gated (matching the existing convention in `tests/redis_pubsub.rs` and `tests/captcha_state.rs`) so the default `cargo test` runs DB-free and the `Test` job in `.github/workflows/server-ci.yml` stops failing on `DATABASE_URL must be set`. A new `Integration (postgres + redis)` job in the same workflow brings up `postgres:16-alpine` + `redis:7-alpine` service containers, exports `DATABASE_URL` + `CONFIG_REDIS_URL`, and runs `cargo test --workspace --all-features -- --include-ignored` so the captcha / captcha-state / redis-pubsub integration suites actually run on every PR. Toolchain pinned to `1.88` across `server/rust-toolchain.toml` and the `dtolnay/rust-toolchain@1.88` action calls (newer transitive deps `image 0.25`, `time 0.3`, `icu_* 2.2` require ≥ 1.88). 7 transitive `clippy::uninlined_format_args` warnings flagged by clippy 1.88 inlined via `cargo clippy --fix` in `services/captcha/keyboard.rs`, `telegram/handlers/{captcha,commands,member_update}.rs`. (infra)
- captcha kick flow no longer issues a no-op `unban_chat_member` before `kick_chat_member`; restrict was never cleared by unban anyway. (server)
- captcha `solve()` no longer runs a redundant `verified_users` check outside the `FOR UPDATE` lock — race resolution is handled inside the locked branch. (server)
- `telegram_message_id` and `moderation_actions.message_id` are now INTEGER (matching teloxide's `MessageId` i32) — eliminates a lossy i64→i32 cast. Migration: `server/migrations/20260502130000_message_id_to_integer.{up,down}.sql`. (server)
- captcha digit input is now persisted in Redis (`cap:input:{chat}:{user}`, TTL = challenge lifetime) between callback presses. Previously the handler reconstructed input lossily from the caption mask (counted `●` chars and replayed `"0".repeat(n) + last_digit`), making the captcha unsolvable in normal use. (server)
- captcha callback handlers now reject presses from non-target users via a Redis-backed meta lookup (`cap:meta:{chat}:{message}`); presser sees a "This isn't your captcha" toast. Previously any chat member could wipe a target's captcha by pressing buttons (the handler would call `solve()` with the stranger's user id → `Outcome::NotFound` → `delete_message`), leaving the target restricted until the expiry job kicked them. (server)
- captcha `is_verified` hot path on join now consults a Redis cache (`cap:verified:{chat}:{user}`, 7d TTL) before falling back to PostgreSQL, then writes back on PG hit. Eliminates a PG round-trip on every join event for returning users. (server)
- captcha `WrongLeft` now clears the input buffer in Redis and resets the caption mask, so the user can immediately retry instead of having to backspace four times to free a buffer pinned at `SOLUTION_LEN`. (server)
- `/verify` now populates the Redis verified-cache (`cap:verified:{chat}:{user}`), so a moderator-verified user skips the PG round-trip on their next join instead of relying on lazy fill. (server)
- captcha `Outcome::Expired` path is now transactional: `solve()` deletes the row and writes both `captcha_expired` + `kick` ledger rows inside the locking tx, so the expiry job's sweep no longer picks the row up and fires a duplicate kick + duplicate Telegram API calls. (server)
- captcha `Outcome::WrongFinal` path now also writes a `kick` ledger row alongside `captcha_failed`, matching the audit trail for the expiry path. (server)
- callback dispatcher prefix filter now matches `vc:` (with separator) instead of `vc`, so future unrelated callbacks like `vcoupon:…` won't be misrouted into the captcha handler. (server)
- `is_fresh_join` now treats `Restricted { is_member: true }` as a present-in-chat state on both the old and new sides of the transition, so chats with default-restricted permissions actually trigger the captcha on join. (server)
- `is_moderator` doc no longer claims a `chat_moderators` fallback that doesn't exist — on Telegram-API failure we deny the call and log; M2+ may add a per-chat moderator allow-list. (server)
- `bin/server.rs` now logs `JoinError` from each long-running task on shutdown (HTTP / dispatcher / jobs / pubsub) instead of silently dropping panics into `let _ = handle.await;`. (server)
- `member_update.rs` log on `send_photo` failure correctly says the expiry job will *kick* (not "lift the restrict"). (server)
- `jobs::spawn_named` doc no longer claims to log panics — panics surface as `JoinError` on the returned handle and are logged at the `bin/server.rs` await sites. (server)
- captcha callback handler now re-attaches the inline digit-pad on every `edit_message_caption` call (digit press / wrong-attempt reset / backspace). Telegram drops the existing `reply_markup` when a caption edit omits one, so previously the keyboard vanished after the first button press and the user could not finish the captcha. New helper `services::captcha::keyboard::digit_pad_from_short(short)` lets the callback handler rebuild the same keyboard from the meta row's `uuid_short` without round-tripping the full UUID. (server)
- captcha image is now rendered at **480 × 270 (16:9, 1.78:1)** instead of 480 × 180 (8:3, 2.67:1). Telegram's mobile clients crop photo previews wider than ~1.91:1 and were truncating the leftmost / rightmost digits, forcing the user to tap-to-expand to read the captcha. 16:9 sits safely below the crop threshold on iOS / Android / Desktop / Web, so the full captcha now displays in the chat preview on all devices. (Renderer rewrite under `### Changed` raises the typical file size to ~80 KB; budget bumped accordingly. Both changes ship together.) (server)

### Removed

- `IssuedChallenge.solution` field — the plaintext captcha solution no longer leaves `CaptchaService` through the public `IssuedChallenge` struct, eliminating an accidental-`tracing::debug!(?issued)` leak path. Tests that need the solution recompute it from `challenge_id` via the public deterministic helper `solution_for`. (server)

### Security

## [0.1.0] - 2026-05-02

M0 — Foundation & infra. `cargo run` boots the HTTP listener + bot poller +
job runner skeleton against Postgres + Redis (docker-compose). `/health`
returns 200 and `/about` reports the build SHA. Polling logs every update
from `CONFIG_CHATS`; no business handlers yet. CI green.

### Added

- `.github/workflows/server-ci.yml` — server CI pipeline. Four parallel jobs (`fmt`, `clippy -D warnings`, `test --workspace`, `sqlx prepare --check --workspace`) triggered on PR / push to `master` when `server/**` or the workflow itself changes. `SQLX_OFFLINE=true` globally; `Swatinem/rust-cache` reuses target cache; `concurrency.cancel-in-progress` kills superseded runs. (infra)
- Telegram polling worker stub: `server/src/telegram/dispatcher.rs::WatchedChats(Arc<HashSet<i64>>)` plus `build_dispatcher(bot, watched)` returning a teloxide `Dispatcher`. The handler graph filters every `Message` update through `WatchedChats::contains` and falls through to a single endpoint that emits `tracing::info!(update_id, chat_id, user_id, kind=…, "update received")`. `bin/server.rs::spawn_dispatcher` constructs the bot, builds the dispatcher, hooks teloxide's `ShutdownToken` to the global `CancellationToken` so SIGINT/SIGTERM cleanly drain polling, and joins the dispatcher handle alongside HTTP under the same 30s outer shutdown timeout. (server)
- HTTP API surface: `server/src/api/{server,response,routes_health,routes_about,state}.rs`. `GET /health` reports `{status, checks: {db, redis}}` with HTTP 200 if every probe is `ok`, 503 with `degraded` otherwise. `GET /about` returns the build metadata from `build_info` (name, version, commit_hash, built_at, rust_version, profile, target). OpenAPI 3.1 spec at `/api/v1/openapi.json` derives from `utoipa-axum::routes!` so route handlers and schemas stay in sync. Inline Scalar UI at `/scalar`, gated by `Config::resolve_openapi_ui()` (default true in dev, false elsewhere; explicit `CONFIG_OPENAPI_UI=false` disables, returns 404). `ApiResult<T>`/`ApiError` envelope plus `api_success!` / `api_error!` macros are wired up in `api/response.rs` for M1+ endpoints. Middleware: `tower-http` `SetRequestIdLayer`, `PropagateRequestIdLayer`, `TraceLayer::new_for_http`, and `CorsLayer` driven by `CONFIG_CORS_ORIGINS`. (server)
- `server/src/database/postgres.rs::Database` — PgPoolOptions wrapper with project-standard pool sizing (max=50, min=5, acquire=10s, idle=600s) and per-connection `SET statement_timeout = $CONFIG_DB_STATEMENT_TIMEOUT_MS` via `after_connect`; `connect`, `health_check`, `migrate`, `close`. `server/src/database/redis.rs::Redis` — `deadpool-redis` pool with `ping`, `publish`, and a generic `subscribe(pattern, cancel, on_message) -> JoinHandle` PSUBSCRIBE helper that uses a dedicated `redis::Client` connection (pooled connections aren't pubsub-clean) and stops on the shared `CancellationToken`. `bin/server.rs` connects both at startup, runs the health probes, and registers a `chat_config:*` subscription whose handler is a debug-log no-op until M4 wires the cache invalidator. End-to-end verified by `tests/redis_pubsub.rs::publish_subscribe_roundtrip` (ignored by default; opt-in via `--test redis_pubsub -- --ignored`) and a manual `docker exec vixen-redis redis-cli PUBLISH chat_config:42 invalidate` round-trip. (server)
- Initial database schema in `server/migrations/20260502000000_initial_schema.{up,down}.sql`. Eleven tables — `chats`, `chat_config` (per-chat tunables: captcha, spam, CAS, clown, report, summary, log-allowed-messages with CHECK ranges), `chat_moderators`, `verified_users`, `captcha_challenges` (UUID id, UNIQUE per `(chat,user)`, expiry index), `spam_messages` (xxh3-64 BIGINT key, last-seen index), `moderation_actions` (UUID id, idempotency anchor `UNIQUE (chat_id, target_user_id, action, message_id)`, action+actor_kind CHECK, audit-log index), `report_messages`, `daily_stats`, `chat_info_cache`, and the gated `allowed_messages`. `update_updated_at()` plpgsql trigger function with triggers on `chats`/`chat_config`/`chat_info_cache`. All FKs to `chats(chat_id)` cascade on delete. Migration is wrapped in BEGIN/COMMIT and is fully reversible (verified by `sqlx migrate revert` + re-apply round-trip). (server)
- `telemetry::init` builds a two-sink tracing subscriber: human-readable console layer (ANSI in dev) plus rolling JSON file appender (`vixen-server.YYYY-MM-DD.log`, daily rotation, 7-file retention) under `CONFIG_LOG_DIR`. `EnvFilter` prefers `RUST_LOG` and falls back to `CONFIG_LOG_LEVEL`. `bin/server.rs` now holds the returned `WorkerGuard` for the process lifetime. `crate::utils::RedactedToken<'a>(&'a str)` is the tracing-only redaction helper — `Display` prints `<id>:****` for `id:body` strings and `****` otherwise — distinct from the owning `BotToken`/`JwtSecret`/`AdminSecret`/`OpenAiKey` newtypes that fully redact to `***redacted***`. End-to-end verified: a fake bot token in env never appears in stdout or in `vixen-server.YYYY-MM-DD.log`. (server)
- Full clap `Config` parser in `server/src/config/mod.rs` covering every `CONFIG_*` env var that M0–M5 needs (secrets, connection URLs, address, environment, log level / dir, OpenAPI UI gate, CORS origins, telegram mode + webhook pair, JWT TTL, init-data max age, DB pool sizing). Secret newtypes `BotToken`, `JwtSecret`, `AdminSecret`, `OpenAiKey` redact to `***redacted***` in `Display`/`Debug`; `Config::validate()` enforces token format, non-empty chats, no-wildcard CORS, prod-only JWT/admin secrets, webhook url+secret pair and DB pool ordering — startup exits 2 with a clear message on failure. `server/config/template.env` documents every variable. (server)
- `server/bin/server.rs` entry point: HTTP listener on `CONFIG_ADDRESS` (default `0.0.0.0:8000`), SIGINT + SIGTERM listener that fires a shared `CancellationToken`, 30s outer shutdown timeout. Module skeletons under `server/src/{api,telegram,services,jobs,models,database,config,telemetry,utils}/`; `server/build.rs` captures git short-SHA, build date, rust version, profile and target as compile-time env vars exposed via `server/src/build_info.rs`. (server)
- `server/assets/captcha/DejaVuSans.ttf` (DejaVu Fonts 2.37, permissive license) and `server/assets/captcha/CHANGELOG` — first captcha asset, will be loaded via `include_bytes!` by the M1 digit-pad renderer. Asset immutability rules and bump-on-change protocol are documented in `server/docs/captcha.md`. (server)
- `docker/docker-compose.yml` and `docker/.env.example` — local dev infrastructure: PostgreSQL 16 + Redis 7 with healthchecks, named volumes (`vixen_pg-data`, `vixen_redis-data`), explicit `name: vixen` project namespace to avoid collisions with other repos that nest compose under a `docker/` directory. (infra)
- 29 new Claude Code skills covering meta-workflow, server subsystems, website patterns, and infra. Index in [.claude/skills/README.md](.claude/skills/README.md). (infra)
  - **Meta workflow** (7): `plan-before-code`, `verifiable-goal`, `code-review-self`, `debug-systematically`, `change-impact-assessment`, `pr-description`, `find-external-skill`.
  - **Server foundations** (4): `transaction-discipline`, `tracing-spans`, `connection-pool-tuning`, `serde-strict-deserialization`.
  - **Server vixen subsystems** (8): `add-telegram-handler`, `add-slash-command`, `captcha-pipeline`, `spam-rule`, `background-job`, `tg-webapp-auth`, `per-chat-config`, `seed-test-chat`.
  - **Website patterns** (8): `solid-resource-pattern`, `solid-async-cleanup`, `typescript-discriminated-union`, `form-error-ux`, `loading-empty-error-states`, `responsive-breakpoints-telegram`, `interaction-states-kobalte`, `design-tokens-system`.
  - **Website vixen-specific** (1): `telegram-login-widget`.
  - **Infra** (1): `infra/github-actions-workflow`.

### Changed

- Updated 10 existing skills with research-derived additions: `server/sqlx-query` (keyset pagination, `ON CONFLICT`), `server/postgres-optimization` (lock strength), `server/rust-error-handling` (`#[from]`, Telegram `inspect_err`), `server/rust-async-tokio` (cancel-safety table), `server/rust-testing` (`sqlx::test` fixtures + corpus tests), `website/add-solid-component` (refs + cleanup), `website/add-i18n-string` (RU plurals + ICU braces), `website/design-anti-patterns` (OLED black, cursor-pointer, motion timing), `website/ui-accessibility` (touch targets + ARIA), `verify-changes` (`.sqlx/` staging + concurrency stress), `docker-multi-stage` (healthcheck + SQLX_OFFLINE), `commit-message` (breaking-change footer + co-author rules). (infra)
- Roadmap rewritten as M0–M8 (foundation → captcha → spam → reports → web auth/hot-reload → dashboard → public reports + WebApp → SQLite migration → prod webhook). Redis is now a mandatory dependency from M0 (hot caches + `chat_config:{chat_id}` pub/sub for live config reload). Most tunables move out of env vars into `chat_config` (PostgreSQL JSONB, edited from the dashboard). Captcha output switches from PNG to WebP. Daily reports gain MarkdownV2 pseudographics alongside the chart, conditionally emitted. Bot adds `/info <user>` and `/report` commands. (infra)

### Fixed

### Removed

### Security
