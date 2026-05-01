# Spam Detection Pipeline

Triggered by every `Message` from a verified user. (Unverified users go through CAPTCHA first; their messages are deleted immediately if they speak before solving.)

## Cascade

```
incoming message
   │
   ▼
1. Pre-checks
   ├─ verified_users lookup (Moka cache) — if not verified: route to captcha pipeline, return.
   └─ clown_chance roll — random reaction emoji on the message, does not skip the rest.
   │
   ▼
2. Normalize body
   - Unicode NFKC
   - lowercase
   - strip combining marks
   - whitespace collapse (multi-space → single, trim)
   - zero-width strip (​ ‌ ‍ ﻿)
   - homoglyph mapping (Cyrillic ⟷ Latin look-alike table)
   │
   ▼
3. xxh3-64 of normalized body
   │
   ▼
4. spam_messages lookup
   SELECT 1 FROM spam_messages WHERE xxh3_hash = $1
   ├─ HIT: it's a known spam → action = ban (10min) + delete message + INSERT moderation_action
   │       UPDATE spam_messages SET hit_count = hit_count + 1, last_seen = NOW().
   └─ MISS: continue.
   │
   ▼
5. CAS API check (gated by CONFIG_CAS=on)
   ├─ Cached verdict (1h TTL): if FLAGGED → action = ban + delete, INSERT moderation_action.
   └─ MISS or NOT FLAGGED: continue.
   │
   ▼
6. n-gram phrase match
   - Compare normalized body against the curated phrase set
     (HashSet<&'static str>, ~80–100 phrases ported from the Dart prototype's spam_phrases.dart).
   - Score = sum of weights of matched phrases (default weight 1.0; tunable per-chat in chat_config.spam_weights).
   ├─ score ≥ chat_config.spam_threshold → action = delete + soft-warn + INSERT spam_messages (so future copies are O(1)).
   └─ otherwise: pass.
   │
   ▼
7. Pass — message stays. Optional: log to allowed_messages for analytics (gated by chat_config.log_allowed_messages).
```

## Idempotency

Every action goes through `ModerationService`, which inserts into `moderation_actions` with the uniqueness key `(chat_id, target_user_id, action, message_id)`. Re-processing the same `Message` (Telegram retried, bot restarted mid-handler) hits the unique-violation, which the service treats as success without re-running the side-effect (ban / delete).

## CAS integration

`src/services/cas_client.rs`:

- `GET https://api.cas.chat/check?user_id={id}`
- 3-second timeout per request.
- Failure (network / 5xx) is **fail-open** — treated as "not flagged". Falsely flagging a legitimate user is worse than missing a spammer.
- Verdict cached in Moka (1h TTL). Cache key = `user_id`.
- Per-chat opt-out via `chat_config.cas_enabled BOOLEAN DEFAULT TRUE`.

## Spam-message TTL

`spam_messages` rows expire after 14 days (`spam_messages_cleanup` job, runs daily). The hit-count then resets if the same message reappears later. This trades off memory growth vs. catching long-tail spam recurrences. 14 days is the Dart prototype's default; tunable via `CONFIG_SPAM_RETENTION_DAYS`.

## Per-chat tuning

`chat_config` columns relevant to spam:

- `spam_enabled BOOLEAN` — global kill switch.
- `spam_threshold REAL DEFAULT 1.0` — n-gram score threshold.
- `spam_weights JSONB` — per-feature weight overrides (NULL = global default).
- `cas_enabled BOOLEAN DEFAULT TRUE` — CAS lookup on/off.
- `clown_chance SMALLINT DEFAULT 0` — % chance of clown emoji reaction on verified users' messages.
- `log_allowed_messages BOOLEAN DEFAULT FALSE` — whether to record `allowed_messages` rows for analytics.

Edit via the dashboard (`PATCH /api/v1/chats/{chat_id}/config`) or directly in DB during development.

## Adding a new spam rule

1. Implement the rule as a function returning a score contribution (`fn detect(normalized: &str) -> f32`).
2. Wire it into the cascade in `spam_service.rs`.
3. Add a corpus YAML at `server/tests/spam_corpus/<rule>.yaml` — minimum 5 positives + 5 negatives.
4. Update `tests/spam_pipeline.rs` to walk the new corpus.
5. Add a `chat_config.spam_weights.<rule>` weight (default in code, override per-chat).
6. Update this document.

The skill `.claude/skills/server/spam-rule/SKILL.md` (added in M2) walks through this.

## Explainability

Every ban records the rule(s) that fired in `moderation_actions.reason` (free text JSON):

```json
{
  "matched_rules": ["xxh3_dedup", "ngram"],
  "ngram_phrases": ["заработай $1000", "пиши в личку"],
  "score": 2.5,
  "threshold": 1.0
}
```

A planned `/spam-replay` slash command will re-run the pipeline against a stored message_id and print the same decision tree to the moderator.

## Failure modes

| Failure | Effect | Recovery |
|---|---|---|
| CAS API down | Pipeline skips CAS step | Auto — fail-open |
| Postgres down | Service errors at insert; handler returns Err | Dispatcher logs, retries on next message |
| Same message arrives twice (Telegram retry) | Uniqueness key on moderation_actions makes second insert no-op | Auto |
| New rule has too many false positives | Spike in moderation actions; moderators complain | Lower the rule's weight or disable per-chat in `chat_config` |

## Related

- Service: `src/services/spam_service.rs`
- CAS client: `src/services/cas_client.rs`
- Phrase corpus: `src/services/spam_phrases.rs` (ported from Dart `vixen/lib/src/spam_phrases.dart`)
- Cleanup job: `src/jobs/spam_cleanup.rs`
- Schema: see [database.md](database.md)
- Skill: `.claude/skills/server/spam-rule/SKILL.md`
