---
name: verifiable-goal
description: Convert any task to a check that fails before and passes after. "Add validation" → tests for invalid inputs. "Fix bug" → reproduction + assertion. Use before declaring work done, when planning multi-step work, or when the user says "make sure this works".
---

# Verifiable Goal (Vixen)

**Source:** [`CLAUDE.md`](../../CLAUDE.md) "After Writing Code".

## The principle

No work is done until a check proves it. **Build green ≠ feature works.** Define the check before coding, not after.

## Categories

- **Bugfix** → write a failing test that reproduces the bug, fix, confirm it passes. Verify the test runs in CI, not just locally.
- **New feature** → tests covering: happy path, error path, permission gap, idempotency.
- **Refactor** → existing test suite passes, no new tests needed. Verify coverage didn't drop on the touched modules.
- **Migration** → `cargo sqlx prepare --check` passes; an integration test runs against the migrated schema; `.down.sql` actually reverses (manual verification once).
- **UI change** → screenshot before/after at 320px and 1024px; manually walk the golden flow plus one edge case (empty / error / loading).
- **Performance** → benchmark or timing assertion before and after. Without numbers it's a guess.

## Multi-step work

Break into steps. **One verifiable checkpoint per step.** Don't tee up "I'll verify all at the end" — that's when half of the steps are wrong and you have to rip them out together.

Example: adding a new captcha mode →

1. Asset added under new versioned path. Check: `cargo test captcha::asset_loader::loads_picture_pick_v1`.
2. `ChallengeKind` enum variant + serde. Check: round-trip serde test.
3. Solver in callback handler. Check: integration test stubbing TG client.
4. Per-chat opt-in. Check: API + DB migration test.

## Vixen-specific verification

- **Server**: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test && cargo sqlx prepare --check`. Use `/server-check`.
- **Website**: `bun run check && bun run typecheck && bun run build`. Use `/website-check`.
- **Both**: server first, then website.

## Don't ship

- "Should work" without a test.
- "Tests pass locally" without confirming they run on the right branch (CI feedback loop).
- "Will verify after merge" — that's not done. Verify before.
- "Tests pass" with no new tests added for the change. The bar moved; tests should reflect it.

## When tests can't easily exist

State it explicitly. Don't paper over it.

- **UI feel / animation** → "I cannot automate this; manually verified at 320px and 1024px in light + dark."
- **Telegram production behavior** → "Tested in dev (mock bot / seeded chat). Production smoke test pending: send `/start` in the staging chat after deploy."
- **External API timeout handling** → "Cannot easily induce CAS API timeout; reviewed code path manually and confirmed `tokio::time::timeout` wraps the call."

## Related

- `plan-before-code` — defines the goal you're going to verify.
- `code-review-self` — last gate before push.
- `verify-changes` — run the canonical pipeline.
