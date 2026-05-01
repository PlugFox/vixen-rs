---
description: Run website pre-commit checks (biome, typecheck, build)
allowed-tools: Bash
---

Run the full website validation pipeline and report results.

Execute in order, stop on first failure:

1. `cd website && bun run check` — Biome lint + format check.
2. `cd website && bun run typecheck` — `tsc -b --noEmit`.
3. `cd website && bun run build` — production Vite build.

If a step fails: show the failing output (tail only), identify the root cause, propose a fix. For pure formatting, you may run `bun run check:fix` but show the diff before applying.

If all pass: report `website-check: OK` with bundle size summary from the Vite output.
