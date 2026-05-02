# Captcha Pipeline

## Policy (M1)

The bot **never restricts and never kicks** users for failing or ignoring a
captcha. The only enforcement primitive is **message deletion**: every message
from an unverified, non-admin user is silently deleted, and a fresh captcha is
issued (or the existing one re-anchored). A user who never solves the captcha
just has every message they send disappear; eventually the M2 spam pipeline
will catch them on duplicate-message hashing and ban them, or they leave.

Concretely:

- No `restrict_chat_member` calls anywhere in the captcha pipeline.
- No `kick_chat_member` / `unban_chat_member` calls anywhere.
- The expiry job only deletes the captcha photo and writes a `captcha_expired`
  audit row.
- `Outcome::Expired` and `Outcome::WrongFinal` from `solve()` only delete the
  challenge row + write a `captcha_expired` / `captcha_failed` audit row.
- The `kick` action remains in the `moderation_actions.action` CHECK constraint
  for the M2 spam-ban path, but M1 code never writes one.

## State storage

Captcha state is split between PostgreSQL (durable, the source of truth) and
Redis (ephemeral UI scratchpad). The split is deliberate: PG holds anything a
restart must survive; Redis holds anything that can vanish without harm
(at worst the user has to press refresh).

| State | Where | TTL | Source of truth |
|---|---|---|---|
| `captcha_challenges` (id, solution, attempts_left, expires_at, telegram_message_id) | PG | per-chat `captcha_lifetime_secs` (default 60s) | PG |
| In-progress digit input | Redis `cap:input:{chat_id}:{user_id}` | = challenge lifetime | Redis (ephemeral UI state) |
| Callback meta (owner_user_id, uuid_short, lifetime_secs) | Redis `cap:meta:{chat_id}:{message_id}` | = challenge lifetime | Redis (for O(1) ownership check without PG) |
| `is_verified` cache | Redis `cap:verified:{chat_id}:{user_id}` = `"1"` | 7 days | PG (cache; PG is authoritative) |
| Chat admins | Redis `cap:admins:{chat_id}` = JSON `Vec<i64>` | 6 hours | TG `get_chat_administrators` (cache) |
| `verified_users`, `moderation_actions` | PG | — | PG |

All Redis keys live under the `cap:` namespace. Helpers are in
`services/captcha/state.rs::CaptchaState`. A flaky Redis must degrade the
captcha UI gracefully (callback handlers warn-log + silent return) — never
panic up to the dispatcher.

## State machine

```
                    ┌──────────────────┐
                    │     (no row)     │◀───────┐
                    └────────┬─────────┘        │
              join OR        │                  │
              unverified-msg │ issue_challenge  │
                             ▼                  │
                    ┌──────────────────┐        │
            ┌──────▶│      Issued      │◀──────┐│
            │       └────────┬─────────┘       ││
            │                │ digit press     ││
            │                ▼                 ││
            │       ┌──────────────────┐       ││
            │       │   InputBuilding  │───────┘│ refresh
            │       │ (caption mask)   │        │
            │       └────────┬─────────┘        │
            │                │ length == 4      │
            │                ▼                  │
            │       ┌──────────────────┐        │
            │       │  solve(input)    │        │
            │       └─┬────┬────┬────┬─┘        │
            │         │    │    │    │          │
            │ wrong   │    │    │    └─ correct │
            └─────────┘    │    │           │   │
                  attempts │    │ final     ▼   │
                  expired  ▼    ▼ wrong ┌────────┐
                       ┌─────────────┐  │ Solved │
                       │ row deleted │  └────────┘
                       │ photo gone; │
                       │ user stays  │
                       │ in chat,    │
                       │ unverified  │──────────┘
                       └─────────────┘   next message →
                                         issue_challenge
```

`Reissued` is a self-loop on the same `(chat_id, user_id)` row: the refresh
button calls `issue_challenge` again with the upsert path, replacing solution
+ image and resetting `attempts_left` and `expires_at`.

## Atomicity contract

Every transition runs in a single transaction:

| Transition | Statements | Notes |
|---|---|---|
| issue / reissue | `INSERT … ON CONFLICT (chat_id,user_id) DO UPDATE SET id, solution, attempts_left, telegram_message_id=NULL, expires_at, created_at=NOW() RETURNING id, attempts_left, expires_at` | Single statement; image render happens *after* the row is durable so a crash never leaves an orphan image. |
| solve (correct) | `BEGIN; SELECT … FOR UPDATE; DELETE captcha_challenges; INSERT verified_users ON CONFLICT DO NOTHING; INSERT moderation_actions (action='verify') ON CONFLICT DO NOTHING; COMMIT` | A re-fired callback after a successful solve hits the `NotFound` branch and re-checks `verified_users`, returning `AlreadyVerified`. |
| solve (wrong, n>1) | `BEGIN; SELECT … FOR UPDATE; UPDATE captcha_challenges SET attempts_left = $1; COMMIT` | Each tap decrements once; `FOR UPDATE` serialises parallel callbacks on the same row. |
| solve (wrong, final) | `BEGIN; …; DELETE captcha_challenges; INSERT moderation_actions (action='captcha_failed') ON CONFLICT DO NOTHING; COMMIT` | The handler drops the captcha photo. **No kick.** The user stays in the chat; their next message will trip the gate and issue a fresh challenge. |
| solve (expired) | `BEGIN; …; DELETE captcha_challenges; INSERT moderation_actions (action='captcha_expired') ON CONFLICT DO NOTHING; COMMIT` | Cleanup runs inside the locking tx so the expiry job's sweep doesn't fire a duplicate ledger row. **No kick.** |
| expiry sweep | `DELETE FROM captcha_challenges WHERE id IN (SELECT id WHERE expires_at < NOW() ORDER BY expires_at LIMIT 200) RETURNING …` | Bounded batch; loop until empty or shutdown is requested. The job then performs the bot-side `delete_message` per row and writes the `captcha_expired` audit row. |

Image rendering (`services/captcha/render.rs`) is a pure function — no I/O,
no globals — so it can be wrapped in `tokio::task::spawn_blocking` without
ordering hazards. The renderer's only inputs are the challenge UUID, the
solution string and the bundled font; the same triple always produces
byte-identical bytes (regression-pinned by
`render::tests::deterministic_for_same_inputs`).

## Trigger

A `ChatMemberUpdated` whose old kind is "left" / "banned" and new status is
`Member`. The dispatcher branch is `Update::filter_chat_member()` (see
`src/telegram/dispatcher.rs`). `MyChatMember` (the bot itself) is not handled
in M1.

A user already in `verified_users(chat_id, user_id)` short-circuits without
issuing — re-issuing a captcha to a verified user is a bug.

## Issuance

Issuance fires from two places:

- **`chat_member` handler** (`telegram::handlers::member_update`) when a fresh
  joiner appears. Skipped for owner/admin transitions (we don't ask admins to
  prove they're human).
- **Message gate** (`telegram::handlers::message_gate`) on every non-command
  message from an unverified non-admin user, *if* they have no live captcha
  row already. The user's message is deleted before the new captcha is posted.

The flow itself is identical in both call sites:

```
1. Skip if state.captcha.is_verified(chat_id, user_id) (cache → PG fallback).
2. let issued = state.captcha.issue_challenge(chat_id, user_id).await?;
   // INSERT … ON CONFLICT DO UPDATE … RETURNING.
   // Renderer runs inside spawn_blocking and produces a 480×180 lossless WebP.
3. let msg = bot.send_photo(chat_id, InputFile::memory(issued.image_webp))
                .caption(caption_initial(&mention, issued.attempts_left))
                .reply_markup(issued.keyboard)
                .await?;
4. state.captcha.record_message_id(chat_id, user_id, msg.id.0).await?;
5. state.captcha_state.set_meta(chat_id, msg.id.0, user_id, &short, lifetime).await?;
```

Steps 3 / 4 / 5 are best-effort: a failure logs at warn. The DB row from step
2 must be durable before any bot call so the expiry job can find it on a crash.

## Solve flow

The user taps a digit on the inline keyboard. CallbackQuery `data` is
`vc:{short}:{op}` where `short` is the first 8 hex characters of the
challenge UUID and `op` is one of `0`..`9`, `bs` (backspace), `rf` (refresh).

```
1. Parse data; if it doesn't start with "vc:" or fails to parse, drop.
2. Look up `cap:meta:{chat}:{message}` in Redis.
   - Miss → ack + return (TTL expired or restart wiped Redis).
3. Ownership check: presser_id == meta.owner_user_id?
   - No → answer_callback_query(text="This isn't your captcha", show_alert=false).
          Toast is visible only to the presser; captcha is not touched.
   - Yes → continue.
4. Stale-callback check: parsed.short == meta.uuid_short?
   - No → ack + silent drop (refresh issued a new uuid; the old keyboard's
          buttons still carry the old short).
5. ack the callback so Telegram stops retrying.
6. Read input from `cap:input:{chat}:{owner}` (empty on miss).
7. op:
   - "0".."9" — append; if length < 4: SET cap:input + edit_message_caption.
                if length == 4: state.captcha.solve(chat_id, owner_id, input).
   - "bs"     — pop last; SET cap:input + edit_message_caption with new mask.
   - "rf"     — DEL cap:input + DEL cap:meta + state.captcha.reissue +
                edit_message_media + record_message_id + SET cap:meta with
                the NEW challenge's short on the SAME message_id.
8. solve() outcomes:
   Solved | AlreadyVerified  → DEL cap:input + DEL cap:meta + SET cap:verified +
                               bot.delete_message.
   WrongLeft(n)              → keep cap:input + cap:meta (challenge still alive,
                               TTL is NOT extended); edit_message_caption.
   WrongFinal | Expired      → DEL cap:input + DEL cap:meta + bot.delete_message.
                               No kick. The user remains in the chat; their next
                               message will trip the gate and issue a fresh challenge.
   NotFound                  → DEL cap:input + DEL cap:meta + bot.delete_message
                               (true vanish — ownership was already verified).
```

The ownership check protects targets from disruption: without it, any other
chat member could press buttons on a stranger's captcha and trigger
`Outcome::NotFound` → `delete_message`. The check is two Redis ops + a string
compare, well within Telegram's ~15s callback-answer window.

## Refresh

`reissue` upserts the same `(chat_id, user_id)` row with a fresh UUID, fresh
solution, fresh image, attempts reset to the chat config default and a fresh
`expires_at`. The bot then `edit_message_media` the captcha photo so the
user sees a new picture with no incidental flicker.

## Expiry

`captcha_expiry` background job, every 60 s
(`src/jobs/captcha_expiry.rs`). The sweep is **batched** (`LIMIT 200` per
statement, looped until empty) and **cancel-aware** (the shutdown token is
checked between batches and between rows in a batch) so that after a long
downtime the queue drains in bounded chunks instead of one giant statement
that would block shutdown.

```sql
-- One batch per iteration; loop until the batch is empty or shutdown fires.
DELETE FROM captcha_challenges
WHERE id IN (
    SELECT id FROM captcha_challenges
    WHERE expires_at < NOW()
    ORDER BY expires_at
    LIMIT 200
)
RETURNING chat_id, user_id, telegram_message_id;
```

For each returned row:

```
bot.delete_message(chat_id, telegram_message_id);  -- best-effort
INSERT INTO moderation_actions … action='captcha_expired' actor_kind='bot'
    ON CONFLICT DO NOTHING;
```

**No kick.** M1 policy is "delete the message, give them a fresh captcha next
time they speak". The job is idempotent: a re-run after a crash just finds
zero expired rows. `ON CONFLICT DO NOTHING` covers the case where the same
`(chat_id, target_user_id, action, message_id)` row was already written by
`solve()`'s expired-during-solve path.

## Asset immutability

`server/assets/captcha/` contains the bundled fonts and any sprite atlases
(`DejaVuSans.ttf` from DejaVu 2.37, embedded via `include_bytes!`).

**Never edit an existing asset file in place.** Pending captcha challenges
in `captcha_challenges` reference the rendered output, which is derived from
the asset bytes baked into the renderer at challenge creation. If you replace
the file, the deterministic-render test breaks AND any in-flight challenge
becomes unsolvable.

To change a captcha look:

1. Add a new asset file with a new name (`DejaVuSans-v2.ttf` or
   `assets/captcha/v2/...`).
2. Bump the asset selector in `services/captcha/fonts.rs` (a `const`, or
   per-chat config when we get there).
3. Add an entry to `assets/captcha/CHANGELOG`
   (`2026-09-01 added v2 font for higher contrast`).
4. Existing challenges keep rendering against the OLD asset until they
   expire or are solved.

The same protocol applies to the visual-style versioning recorded in
`assets/captcha/CHANGELOG` even when no new file is added — the rendering
algorithm itself is part of the captcha contract.

## Determinism

`services::captcha::render::render_webp(challenge_id, solution, fonts)`
MUST produce byte-identical output for the same inputs. This is enforced by:

- Seeded RNG (xorshift64) with `seed = xxh3(challenge_id.as_bytes())`.
- Fixed iteration order (no hashmap walks).
- Embedded asset bytes (`include_bytes!`), no env-dependent fallbacks.
- Stable WebP encoder settings (`webp::Encoder::encode_lossless`).

Tests assert this by rendering twice and `assert_eq!(bytes_a, bytes_b)`.

## Implementation notes

- Image is **480 × 270 lossless WebP** (16:9, 1.78:1), 60 – 110 KB
  typical (budget ≤ 150 KB). RGBA, opaque (alpha = 255). The 16:9 ratio
  sits below Telegram's mobile preview crop threshold (~1.91:1) so
  captchas display in full on iOS / Android / Desktop / Web without a
  tap-to-expand.
- Internally rasterised at **2× supersample resolution (960 × 540)**
  and downscaled with **Lanczos3** before encoding. Cheap free
  anti-aliasing on rotated digit edges, geometric shapes and line
  strokes — the per-pixel coverage from `ab_glyph` survives the filter
  as a clean soft edge in the final output.
- Lossless WebP via `image::codecs::webp::WebPEncoder::new_lossless`.
  No standalone `webp` / `libwebp` dependency — the renderer talks
  only to the `image` crate.
- Layered composition (each layer alpha-composites onto the same
  buffer at supersampled resolution):
  1. **Background gradient** — top→bottom lerp picked deterministically
     from one of six palettes (twilight / forest / plum / sky-pastel /
     lavender-mist / coral). Light and dark themes share the codepath.
  2. **18..22 background shapes** — circles, rectangles, thick Bresenham
     lines drawn from the palette's accent colour set at low alpha.
  3. **4 digit glyphs** — `ab_glyph` outlines, scale-jittered (216..264 px
     in supersampled space), rotated ±15° around the glyph centre,
     position-jittered, per-digit accent colour from the palette.
  4. **30..40 quadratic Bézier curves** — organic noise overlay in the
     palette's `noise` colour.
  5. **18..22 foreground shapes** — small translucent whites/greys laid
     on top of the digits to break up monolithic glyph fills (a naive
     OCR signal) without harming readability.
- Font is loaded as `&'static [u8]` via `include_bytes!` in
  `services/captcha/fonts.rs` — never read from disk per request.
- Rendering happens inside `tokio::task::spawn_blocking` because `image` +
  `ab_glyph` rasterisation is CPU work, not async-friendly.
- Inline keyboard is 3×4: `1 2 3 / 4 5 6 / 7 8 9 / ⌫ 0 ↻`. Callback data:
  `vc:{short}:{op}` where `short` = first 8 hex chars of the challenge UUID,
  `op` ∈ `{0..9, bs, rf}`. The `vc:` prefix is checked at the dispatcher
  filter so non-captcha callbacks short-circuit.

## Failure modes & remediation

| Failure | Effect | Recovery |
|---|---|---|
| Image render panics | `issue_challenge` returns `Err`; user sees no captcha; their messages keep getting deleted by the gate | Next message gate run retries `issue_challenge` |
| `bot.delete_message` fails (bot not admin) | Unverified user's messages stay visible | Operator sees the warn; needs to grant the bot delete-message permission |
| `bot.send_photo` fails (TG outage) | DB row may exist with `telegram_message_id = NULL`; user is unrestricted but unverified | `captcha_expiry` cleans up the orphan; next message re-issues |
| Delayed callback after solve | Service returns `AlreadyVerified`, handler deletes the captcha photo (no restrict to lift) | None |
| Concurrent solve (two taps in flight) | `SELECT … FOR UPDATE` serialises; loser sees `AlreadyVerified` (or `NotFound` only if challenge was already deleted by an unrelated path) | None |
| User leaves before solving | Row stays; expiry job sweeps it | None |

## Related

- Service: `src/services/captcha/service.rs` — `CaptchaService` with `issue_challenge`, `solve`, `reissue`, `verify_manual`, `is_verified`, `record_message_id`, `active_challenge_message_id`.
- Renderer: `src/services/captcha/render.rs`.
- Keyboard: `src/services/captcha/keyboard.rs`.
- Ephemeral state (Redis): `src/services/captcha/state.rs` — input buffer, callback meta, verified cache, admin cache.
- Handlers: `src/telegram/handlers/{member_update,captcha,commands,message_gate}.rs`.
- Expiry job: `src/jobs/captcha_expiry.rs`.
- Rules: `src/docs/rules/telegram-handlers.md`, `src/docs/rules/background-jobs.md`, `src/docs/rules/migrations.md`.
- Tests: `tests/captcha.rs`.
