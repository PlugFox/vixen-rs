# Captcha Pipeline

## State machine

```
                    ┌──────────────────┐
                    │     (no row)     │
                    └────────┬─────────┘
                             │ join → issue_challenge
                             ▼
                    ┌──────────────────┐
            ┌──────▶│      Issued      │◀──────┐
            │       └────────┬─────────┘       │
            │                │ digit press     │ refresh
            │                ▼                 │
            │       ┌──────────────────┐       │
            │       │   InputBuilding  │───────┘
            │       │ (caption mask)   │
            │       └────────┬─────────┘
            │                │ length == 4
            │                ▼
            │       ┌──────────────────┐
            │       │  solve(input)    │
            │       └─┬────┬────┬────┬─┘
            │         │    │    │    │
            │ wrong   │    │    │    └─ correct
            └─────────┘    │    │              │
                  attempts │    │ final wrong  ▼
                           ▼    ▼          ┌────────┐
                    expired │ kicked       │ Solved │
                           ▼  │            └────────┘
                    ┌──────────┴────────┐
                    │   row deleted +   │
                    │   user kicked     │
                    └───────────────────┘
```

`Reissued` is a self-loop on the same `(chat_id, user_id)` row: the refresh
button calls `issue_challenge` again with the upsert path, replacing solution
+ image and resetting `attempts_left` and `expires_at`.

## Atomicity contract

Every transition runs in a single transaction:

| Transition | Statements | Notes |
|---|---|---|
| issue / reissue | `INSERT … ON CONFLICT (chat_id,user_id) DO UPDATE SET id, solution, attempts_left, telegram_message_id=NULL, expires_at, created_at=NOW() RETURNING id, attempts_left, expires_at` | Single statement; image render happens *after* the row is durable so a crash never leaves an orphan image. |
| solve (correct) | `BEGIN; SELECT EXISTS verified_users; SELECT … FOR UPDATE; DELETE captcha_challenges; INSERT verified_users ON CONFLICT DO NOTHING; INSERT moderation_actions ON CONFLICT DO NOTHING; COMMIT` | The early `verified_users` probe makes a re-fired callback after a successful solve return `AlreadyVerified` instead of re-running. |
| solve (wrong, n>1) | `BEGIN; SELECT EXISTS verified_users; SELECT … FOR UPDATE; UPDATE captcha_challenges SET attempts_left = $1; COMMIT` | Each tap decrements once; `FOR UPDATE` serialises parallel callbacks on the same row. |
| solve (wrong, final) | `BEGIN; …; DELETE captcha_challenges; INSERT moderation_actions (action='captcha_failed') ON CONFLICT DO NOTHING; COMMIT` | The handler then issues `kick_chat_member` + `unban_chat_member` and writes a second ledger row `action='kick'`. |
| expiry | `DELETE FROM captcha_challenges WHERE expires_at < NOW() RETURNING …` | Atomic delete-and-stream. The job then performs the bot-side cleanup per row. |

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

```
1. Skip if state.captcha.is_verified(chat_id, user_id).
2. bot.restrict_chat_member(chat_id, user_id, ChatPermissions::empty()).
3. let issued = state.captcha.issue_challenge(chat_id, user_id).await?;
   // INSERT … ON CONFLICT DO UPDATE … RETURNING.
   // Renderer runs inside spawn_blocking and produces a 480×180 lossless WebP.
4. let msg = bot.send_photo(chat_id, InputFile::memory(issued.image_webp))
                .caption("Solve the captcha: ○○○○")
                .reply_markup(issued.keyboard)
                .await?;
5. state.captcha.record_message_id(chat_id, user_id, msg.id.0 as i64).await?;
```

Steps 2 / 4 / 5 are best-effort: a failure logs at warn and the
`captcha_expiry` job will lift the restriction and clean up within 60 s.
The DB row from step 3 must be durable before any bot call so the expiry
job can find it on a crash.

## Solve flow

The user taps a digit on the inline keyboard. CallbackQuery `data` is
`vc:{short}:{op}` where `short` is the first 8 hex characters of the
challenge UUID and `op` is one of `0`..`9`, `bs` (backspace), `rf` (refresh).

```
1. bot.answer_callback_query(q.id).await — first thing, so Telegram stops retrying.
2. Parse data; if it doesn't start with "vc:", drop.
3. Recover the current input length from the captioned mask
   (○ = empty, ● = filled). Captcha state is otherwise stateless.
4. op:
   - "0".."9" — append; if length < 4: edit_message_caption with new mask.
                if length == 4: state.captcha.solve(chat_id, user_id, input).
   - "bs"     — pop last; edit_message_caption with new mask.
   - "rf"     — state.captcha.reissue + edit_message_media + record_message_id.
5. solve() outcomes:
   Solved | AlreadyVerified  → bot.delete_message + bot.restrict_chat_member(all).
   WrongLeft(n)              → edit_message_caption "Wrong, try again. Attempts left: n".
   WrongFinal | Expired      → bot.delete_message + bot.unban + bot.kick + bot.unban.
   NotFound                  → silent drop (delayed callback after cleanup).
```

The kick = ban + unban round-trip implements "give up = leave, not banned
forever" — the user can rejoin the chat.

## Refresh

`reissue` upserts the same `(chat_id, user_id)` row with a fresh UUID, fresh
solution, fresh image, attempts reset to the chat config default and a fresh
`expires_at`. The bot then `edit_message_media` the captcha photo so the
user sees a new picture with no incidental flicker.

## Expiry

`captcha_expiry` background job, every 60 s
(`src/jobs/captcha_expiry.rs`):

```sql
DELETE FROM captcha_challenges
WHERE expires_at < NOW()
RETURNING chat_id, user_id, telegram_message_id;
```

For each returned row:

```
bot.delete_message(chat_id, telegram_message_id);  -- best-effort
bot.unban_chat_member(chat_id, user_id);           -- clear restrict if any
bot.kick_chat_member(chat_id, user_id);            -- actual kick
bot.unban_chat_member(chat_id, user_id);           -- kick = ban + unban
INSERT INTO moderation_actions … action='captcha_expired' actor_kind='bot'
    ON CONFLICT DO NOTHING;
INSERT INTO moderation_actions … action='kick'            actor_kind='bot'
    ON CONFLICT DO NOTHING;
```

The job is idempotent: a re-run after a crash just finds zero expired rows.
The `ON CONFLICT DO NOTHING` clauses cover the case where the same
`(chat_id, target_user_id, action, message_id)` row was already written by
a previous, partially-succeeded pass.

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

- Image is **480 × 180 lossless WebP**, ≤ 30 KB. RGBA, opaque (alpha = 255).
  Background is a vertical gradient picked deterministically from one of
  three palettes. Digits are anti-aliased through `ab_glyph` outlines,
  rotated ±15° with a small scale jitter (108..132 px), per-digit colour
  picked from the palette.
- Noise is two layers: ~30..50 quadratic-Bezier curves with low alpha and
  ~220..320 single-pixel dots in the palette's `noise` colour.
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
| Image render panics | `issue_challenge` returns `Err`; user sees no captcha; restrict is still applied | `captcha_expiry` lifts restrict in ≤ 60 s |
| `restrict_chat_member` fails (bot not admin) | Captcha is skipped, ledger is empty | Surface in `/status` (M2) so a moderator notices |
| `bot.send_photo` fails (TG outage) | DB row exists with `telegram_message_id = NULL`; user is restricted | `captcha_expiry` deletes the row + lifts restrict in ≤ 60 s |
| Delayed callback after solve | Service returns `AlreadyVerified`, handler deletes the message and lifts restrict (a no-op if already lifted) | None |
| Concurrent solve (two taps in flight) | `SELECT … FOR UPDATE` serialises; loser sees `AlreadyVerified` (or `NotFound` only if challenge was already deleted by an unrelated path) | None |
| User leaves before solving | Row stays; expiry job sweeps it | None |

## Related

- Service: `src/services/captcha/service.rs` — `CaptchaService` with `issue_challenge`, `solve`, `reissue`, `verify_manual`, `is_verified`, `record_message_id`.
- Renderer: `src/services/captcha/render.rs`.
- Keyboard: `src/services/captcha/keyboard.rs`.
- Handlers: `src/telegram/handlers/{member_update,captcha,commands}.rs`.
- Expiry job: `src/jobs/captcha_expiry.rs`.
- Rules: `src/docs/rules/telegram-handlers.md`, `src/docs/rules/background-jobs.md`, `src/docs/rules/migrations.md`.
- Tests: `tests/captcha.rs`.
