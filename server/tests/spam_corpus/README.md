# Spam corpus

Each YAML in this directory is a labelled set of messages that the spam
pipeline must classify the documented way. The harness lives in
[`tests/spam_pipeline.rs`](../spam_pipeline.rs); it walks every file here and
runs each sample through `SpamService::inspect`.

Schema (every key is optional and may appear in any file):

```yaml
must_ban: ["…"]                # Verdict::Ban — harness pre-seeds spam_messages so dedup fires.
must_delete: ["…"]             # Verdict::Delete (n-gram match without dedup hit).
must_allow: ["…"]              # Verdict::Allow (clean / borderline / sub-threshold length).
must_ban_after_first: ["…"]    # First call Delete, second call from a different user Ban (dedup priming).
```

A sample under the wrong key fails the test and blocks the PR. The harness
runs `DELETE FROM spam_messages` between every individual sample, so
`must_delete[0]`'s recorded hash can't bleed into `must_allow[1]`.

## Adding a sample

1. Pick the file matching the rule (or create a new YAML — the harness
   auto-discovers it on the next run).
2. Append the message body under `must_ban` / `must_delete` / `must_allow`
   / `must_ban_after_first`.
3. Re-run `cargo test --test spam_pipeline -- --include-ignored` to confirm.
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
