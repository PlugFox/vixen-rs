---
name: spam-rule
description: Add a new spam-detection rule (text normalization → score → action). Wire into the cascade after dedup + CAS, with corpus tests under server/tests/spam_corpus/.
---

# Spam Rule (Vixen server)

**Read first:**

- [server/docs/spam-detection.md](../../../../server/docs/spam-detection.md) — pipeline order, rule registry, scoring.

## Pipeline order

1. Normalize: NFKC + lowercase + zero-width strip + homoglyph map.
2. Dedup: `xxh3_64` of normalized text → `spam_messages` lookup.
3. CAS lookup (1h cache, fail-open).
4. Rules cascade — sum weighted scores.
5. Threshold compare against `chat_config.spam_threshold`.
6. Action: warn / mute / kick / ban via `moderation_actions` ledger.

## Steps

1. Implement `pub fn detect(normalized: &str, config: &ChatConfig) -> f32` in `server/src/services/spam_{rule}.rs`.
2. Wire into `spam_service.rs` cascade (registered with a default weight).
3. Corpus YAML at `server/tests/spam_corpus/{rule}.yaml` — **≥5 positive + ≥5 negative**. No rule without corpus.
4. Register the weight key in `chat_config.spam_weights` JSONB schema.
5. Walk the corpus in `server/tests/spam_pipeline.rs`.
6. Record matched rules in `moderation_actions.reason` JSON for explainability.
7. Document in `server/docs/spam-detection.md` rule table.

## Gotchas

- **Input is already normalized.** Don't re-NFKC inside the rule — wastes CPU and the rule doesn't see what the rest of the cascade sees.
- **Score accumulates.** A single false-positive rule with high weight ruins the whole pipeline. Default weight low; tune in chat config.
- **Idempotency.** Rules firing AFTER dedup mean `spam_messages` is bypassed for re-processed retries. Gate the action on the `moderation_actions` uniqueness key `(chat_id, target_user_id, action, message_id)` before issuing any ban/kick.
- **CAS API can fail-open.** Network down → never block a message. Log the timeout, continue.
- **Corpus tests are mandatory.** Reviewer rejects PRs without YAML.
- **Telegram IDs `i64`** in any SQL the rule touches.

## Verification

- `cargo test spam_pipeline`.
- `cargo test --test spam_corpus_{rule}` if you split the corpus into its own test file.

## Related

- `add-migration` — `spam_weights` JSONB schema bumps.
- `transaction-discipline` — ledger writes.
- `tracing-spans` — rule-by-rule timing.
