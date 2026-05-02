# Spam corpus

Each YAML in this directory is a labelled set of messages that the spam
pipeline must classify the documented way. The harness lives in
[`tests/spam_pipeline.rs`](../spam_pipeline.rs); it walks every file here and
runs each sample through `SpamService::inspect`.

Schema:

```yaml
must_ban: ["…"]      # SpamService must return Verdict::Ban
must_delete: ["…"]   # Verdict::Delete (n-gram match without dedup hit)
must_allow: ["…"]    # Verdict::Allow (clean / borderline)
```

A sample under the wrong key fails the test and blocks the PR.

## Adding a sample

1. Pick the file matching the rule (or create a new YAML).
2. Append the message body to `must_ban` / `must_delete` / `must_allow`.
3. Re-run `cargo test --test spam_pipeline` to confirm.
4. If the corpus disagrees with current behaviour, fix the **code**, not the
   sample — see `server/docs/rules/testing.md` §"Reacting to a failing test".

## Categories

- `phrase_match.yaml` — n-gram phrase hits + negatives that look spammy but
  don't match (regression seeds for false positives).
- `clean_messages.yaml` — high-confidence negatives: PR notifications, daily
  conversation, code review, meeting reminders.
- `xxh3_dedup.yaml` — `must_ban_after_first` schema: each entry is fed
  through the pipeline twice; the **second** pass must Ban.

`cas_flagged.yaml` is intentionally absent — CAS is a per-user signal, not a
per-message one, so it's exercised in the dedicated `tests/cas_client.rs`
wiremock integration tests.
