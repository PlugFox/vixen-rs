# Captcha Pipeline

## Trigger

A `Message` from an unverified user OR a `ChatMemberUpdated` event (new member joined). Whichever fires first.

The unverified-user check is `SELECT EXISTS(SELECT 1 FROM verified_users WHERE chat_id = $1 AND user_id = $2)`. Hot-path; cached in Moka with 10min TTL, invalidated on verify.

## Atomic challenge issuance

```
1. Generate solution: 4 random digits via seeded RNG.
2. Render image: image-rs + ab_glyph using fonts from server/assets/captcha/. PNG bytes.
3. INSERT INTO captcha_challenges (id, chat_id, user_id, solution, attempts_left, expires_at, created_at)
   VALUES (uuid_v4, $1, $2, $3, 5, NOW()+INTERVAL '1 minute', NOW())
4. bot.restrict_chat_member(chat_id, user_id, ChatPermissions::default())  -- silence the user
5. msg = bot.send_photo(chat_id, image_bytes).caption(...).reply_markup(digit_pad_keyboard).await
6. UPDATE captcha_challenges SET telegram_message_id = $1 WHERE id = $2  -- record for later edit / delete
```

If step 4, 5, or 6 fails, the row from step 3 is rolled back. We never have an orphan image-without-row or a row-without-message.

## Solve flow

User taps a digit. The CallbackQuery handler:

1. Decode `data` (`vc:<short>:<digit>`).
2. Append the digit to the current in-memory input (derived from the message caption — captcha state is stateless across handler calls).
3. If input length < 4: update the caption with current emoji-box display, answer the callback, return.
4. If input length == 4:
   - Open transaction.
   - `SELECT solution, attempts_left FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2 FOR UPDATE`.
   - If `solution == input`:
     - `DELETE FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2`.
     - `INSERT INTO verified_users (chat_id, user_id, verified_at) VALUES (...)`.
     - `INSERT INTO moderation_actions (..., action = 'verify', actor_kind = 'bot', ...)`.
     - Commit.
     - `bot.delete_message(captcha_message_id)` — clean up the image.
     - `bot.restrict_chat_member(chat_id, user_id, full_permissions)` — lift restriction.
     - Invalidate the verified-user cache for this `(chat_id, user_id)`.
   - If `solution != input`:
     - `UPDATE captcha_challenges SET attempts_left = attempts_left - 1 WHERE id = $1`.
     - If `attempts_left == 0`: `DELETE` + ban via `ModerationService` with `reason = "captcha_failed"`.
     - Otherwise: edit caption with "wrong, {N} attempts left", reset input.
     - Commit.

## Refresh

If user taps refresh:

1. Generate a new solution + image.
2. `UPDATE captcha_challenges SET solution = $new_solution, attempts_left = 5, expires_at = NOW()+INTERVAL '1 minute' WHERE id = $1`.
3. `bot.edit_message_media(captcha_message_id, InputMedia::Photo(new_image)).await`.

The challenge ID stays the same; only the contents rotate. Old solution is overwritten — not preserved.

## Expiry

`captcha_expiry` background job (every 60s):

```sql
DELETE FROM captcha_challenges
WHERE expires_at < NOW()
RETURNING chat_id, user_id, telegram_message_id;
```

For each returned row:
- `bot.delete_message(chat_id, telegram_message_id)` — remove the captcha image.
- `bot.unban_chat_member(chat_id, user_id, only_if_banned=true)` — lift restrict (or no-op).
- `bot.kick_chat_member(chat_id, user_id)` — remove from the chat. Then immediately `unban_chat_member` (so they can rejoin if they want — kick != ban). This implements "give up = leave, not banned forever".

The job is idempotent: a re-run after a crash just finds zero expired rows.

## Asset immutability

`server/assets/captcha/` contains the fonts and any sprite atlases:

- `DejaVuSans.ttf` (carried over from the Dart prototype's `vixen/assets/font.webp` replacement; old Rust attempt also used this exact file under `.old/src/captcha/assets/DejaVuSans.ttf`).
- Future picture-pick assets (cat / dog / car / etc.) — separate folder per mode.

**Never edit an existing asset file in place.** Pending captcha challenges in `captcha_challenges` reference the asset path baked into the renderer at challenge creation. If you replace the file, the deterministic-render test breaks AND any in-flight challenge becomes unsolvable for the user (because their displayed image was generated from the old bytes but the verifier reads the new file).

To change a captcha look:

1. Add a new asset file with a new name (`DejaVuSans-v2.ttf` or `assets/captcha/v2/...`).
2. Bump the asset selector in `captcha_service.rs` (constant or per-chat config).
3. Add an entry to `assets/captcha/CHANGELOG` (one line: `2026-05-01 added DejaVuSans-v2.ttf for higher-contrast rendering`).
4. Existing challenges continue rendering against the OLD asset until they expire or are solved.

## Determinism

`captcha_service::render(challenge_id, solution, asset_version)` MUST produce byte-identical output for the same inputs. This is enforced by:

- Seeded RNG for noise placement (seed = `xxh3(challenge_id)`).
- Stable iteration order (no hashmap walks).
- Fixed asset paths (no env-dependent fallbacks).

Tests assert this by rendering twice and `assert_eq!(bytes_a, bytes_b)`.

## Implementation notes

- Image is 240×180 PNG. RGB. Background is dark; digits high-contrast.
- Font loaded as `&[u8]` constant (embedded in the binary via `include_bytes!`); not read from disk per request.
- Rendering happens inside `tokio::task::spawn_blocking` — `image` + font shaping is CPU work, not async-friendly.
- The Dart prototype used a custom font atlas in `assets/font.webp`. We use TrueType + ab_glyph for cleaner extensibility.

## Failure modes & remediation

| Failure | Effect | Recovery |
|---|---|---|
| Image render panics | Handler returns `Err`; user sees no captcha; restrict still applied | `tracing::error!`, `captcha_expiry` will lift restrict in 60s |
| `restrict_chat_member` fails (permissions) | Bot is not admin in chat — log, skip captcha for this chat | Surface in `/status` so moderator notices |
| `bot.send_photo` fails (TG outage) | DB row rolls back, user temporarily restricted | `captcha_expiry` lifts restrict in 60s |
| User leaves before solving | Their `captcha_challenges` row still present; expiry job cleans it up | No action needed |
| Concurrent captchas for same `(chat_id, user_id)` | UNIQUE index on `(chat_id, user_id)` rejects the second INSERT | Handler treats `Conflict` as "already gated, skip" |

## Related

- Issuance: `src/services/captcha_service.rs`
- Handlers: `src/telegram/handlers/{member_update,captcha}.rs`
- Expiry job: `src/jobs/captcha_expiry.rs`
- Skill: `.claude/skills/server/captcha-pipeline/SKILL.md` (added in M1)
