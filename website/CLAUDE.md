# Vixen Website

SolidJS SPA — moderator dashboard + public chat reports for the Vixen Telegram bot.

## Stack

TypeScript (strict), SolidJS, Kobalte (UI primitives), Tailwind CSS, CVA (variants), Vite, Biome, bun.

## Structure

```
src/
  app/          — Router, providers, layouts (root-layout, telegram-webapp-layout)
  features/     — Feature modules (auth, chats, moderation, reports, users, settings)
  pages/        — Route entry points (thin, use export default for lazy())
  shared/       — API client, UI primitives, lib helpers, hooks, i18n
  assets/       — Static files
```

Each feature has: `api.ts`, `types.ts`, `store.ts` (if stateful), `components/`.

Vixen-specific feature modules:

- `auth/` — Telegram Login Widget integration, initData submission, JWT in-memory store.
- `chats/` — list of watched chats, per-chat detail view.
- `moderation/` — action ledger, ban / unban / verify panels.
- `reports/` — daily report viewer, redo trigger, chart download.
- `users/` — per-user lookup across chats: verified status, action history.
- `settings/` — per-chat config form (spam threshold, captcha mode, report hour, AI summary, weights).

## Documentation

- Architecture and structure: `docs/architecture.md`
- Conventions and patterns: `docs/conventions.md`
- API client and interceptors: `docs/api-client.md`
- Telegram-based authentication: `docs/auth.md`
- UI Kit principles and showcase: `docs/ui-kit.md`
- i18n reference: `docs/i18n.md`
- Public report page (redaction rules): `docs/public-reports.md`
- Server API reference: `../server/docs/api.md`

## Key Commands

```bash
cd website && bun install           # Install dependencies
cd website && bun run dev           # Dev server (http://localhost:3000)
cd website && bun run build         # Production build
cd website && bun run lint          # Biome lint
cd website && bun run format        # Biome format
cd website && bun run check         # Biome lint + format check
cd website && bun run typecheck     # tsc --noEmit
```

## Rules

Read before writing code:

| File | When to read |
|------|-------------|
| `docs/rules/solidjs.md` | Before writing or modifying SolidJS components |
| `docs/rules/components.md` | Before creating or modifying UI components |
| `docs/rules/typescript.md` | Before writing TypeScript code |
| `docs/rules/styling.md` | Before writing Tailwind classes or theme tokens |

### Critical gotchas (SolidJS)

- Never destructure props — breaks reactivity. Use `props.field` or `splitProps`.
- Use `<Show>`, `<For>`, `<Switch>/<Match>` — not ternaries or `.map()`.
- No `useEffect` — SolidJS uses `createEffect`, `onMount`, `onCleanup`.

### Critical gotchas (TypeScript)

- Never hardcode user-facing text — use i18n (`t()`, `tp()`, `<T>`).
- No TS enums — use `as const` objects. No `any` — use `unknown`.
- Named exports only. `export default` only in `pages/` (for `lazy()`).
- kebab-case file names: `chat-card.tsx`, not `ChatCard.tsx`.

### When modifying features

- Update types in `features/{name}/types.ts` if API shape changes.
- Check that `api.ts` matches server endpoint signatures (see `../server/docs/api.md`).
- Update [`../CHANGELOG.md`](../CHANGELOG.md) under `[Unreleased]` for any user-visible change. Tag entries with `(website)`.

### Telegram-Auth integration (vixen-specific)

The dashboard runs in two modes — handle both:

- **Inside Telegram WebApp** (the bot exposes an "Open dashboard" button with `web_app` field): `Telegram.WebApp.initData` is available immediately. The website never reads `initDataUnsafe` — it always submits the raw signed `initData` string.
- **As a regular browser page**: render the Telegram Login Widget script (`telegram.org/js/telegram-widget.js`) via `onMount` + `appendChild` (NOT JSX `<script>`). On callback, compose an `initData`-shaped string and submit it the same way.

Both modes hit `POST /api/v1/auth/telegram/login`. JWT lives in **memory only** (not localStorage — initData re-submission is cheap, and localStorage adds session-fixation surface).

See `docs/auth.md` for the full flow and the matching server-side rules in `../server/docs/auth.md`.

### Verifying UI changes

After UI changes, do not report the task done until the flow has been exercised end-to-end in a browser:

1. Start the dev server: `bun run dev` (http://localhost:3000).
2. Walk through both the golden path and at least one edge case (empty state, error state, permission gap, public-report redaction).
3. Test both modes if the change touches auth UI: open as a regular page, then via a Telegram WebApp test bot.

If the change is purely visual, run `/website-check` (biome + typecheck + build) — typecheck and build are not a substitute for exercising the UI.

Playwright MCP is not yet wired into this repo; manual verification is the v1 standard.
