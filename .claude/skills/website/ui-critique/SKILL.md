---
name: ui-critique
description: Review a vixen-rs UI screen or component against Nielsen's 10 heuristics, cognitive load, and the mandatory empty/loading/error/first-run state coverage. Produce P0–P3 severity findings. Use when the user asks to "review", "critique", "audit" a UI change, or before shipping a non-trivial screen.
---

# UI Critique (Vixen website)

**Source:** adapted from [impeccable.style](https://impeccable.style/) and [Nielsen Norman Group — 10 Usability Heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/).

## Checklist

Run every pass. Record findings with severity.

### 1. State coverage (hard requirement — P0 if missing)

Every data-driven view must handle **all five** states. Missing any is a P0 bug.

- **Empty** — first time; never had data. Needs a headline, one-line explanation, and a next-action CTA. No bare emoji.
- **Loading** — skeletons, not spinners, for content regions. Spinners only for discrete actions (button submitting).
- **Error** — what went wrong (human language), what to do (retry / contact / link), an error code for support. Never show raw stack traces.
- **Partial / stale** — cached data while refetching, offline banner, degraded functionality.
- **Full** — the happy path.

First-run flows specifically: greet, explain, set up something useful on the first click. Don't drop the user into an empty dashboard.

### 2. Nielsen's 10 heuristics

1. **Visibility of system status.** Every async op shows progress; every click has a response within 100ms.
2. **Match between system and the real world.** UI labels in user's language (vixen ships RU + EN via i18n, never hardcoded — see `add-i18n-string` skill). No jargon like "null" or "undefined" in user-facing copy.
3. **User control and freedom.** Undo for destructive ops (manual ban / unban); cancel buttons on dialogs; no "are you sure?" for reversible actions.
4. **Consistency and standards.** Same action has the same label everywhere. Primary button on the right in dialogs. Kobalte primitives wrapped consistently via `shared/ui/`.
5. **Error prevention.** Validate before submit, not only after. Disable the primary button when the form is invalid; show why in the helper text.
6. **Recognition rather than recall.** Field hints visible, recent items surfaced.
7. **Flexibility and efficiency.** Keyboard shortcuts for power users; bulk actions for moderation lists ≥ 20 items.
8. **Aesthetic and minimalist design.** Every pixel earns its place. See `design-anti-patterns` skill.
9. **Help users recognize, diagnose, recover from errors.** Inline validation at the field level; a single "retry" button on network failures.
10. **Help and documentation.** Contextual hints, not a buried help center. Tooltip for any non-obvious icon.

### 3. Cognitive load

- How many decisions does the user face on this screen? >7 → redesign.
- How many data points compete for attention? Prioritize one primary action per screen.
- Read the screen aloud in one sentence. If you can't, the hierarchy is wrong.

### 4. Keyboard + focus

- Tab order matches visual order (see `ui-accessibility` skill).
- Escape closes dialogs; Enter submits default forms.
- Focus ring is visible on every interactive element.

### 5. Perf + feel

- Time-to-interactive on the main view < 500ms perceived. Use `<Suspense>` boundaries.
- No layout shift (CLS). Reserve space for async content with skeletons.
- Transitions ≤ 200ms for UI; ≤ 300ms for route changes.

### 6. Vixen-specific concerns

- **Two-mode rendering** — does the screen work both inside a Telegram WebApp container (no top header, viewport may be small) and as a regular browser page?
- **Public report page** — does it show only redacted data (no usernames, no message bodies, only counts + categories)? P0 if PII leaks.
- **Moderation actions** — does the user see a confirmation that the bot will ban / unban? Audit log entry visible?

## Severity

- **P0** — broken functionality, data loss risk, accessibility blocker, missing state coverage, PII leak. Fix before merge.
- **P1** — usability defect, inconsistency with design-system, low contrast. Fix this PR or file a follow-up.
- **P2** — polish issue (alignment, minor copy). Batch these.
- **P3** — nitpick. Note, don't block.

## Deliverable

A numbered list grouped by severity. For each finding: where (file:line or screen name), what's wrong, what to do.

## Related

- `design-anti-patterns` — visual red flags.
- `ui-accessibility` — a11y-specific pass.
