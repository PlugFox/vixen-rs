# vixen-website

SolidJS moderator dashboard for the [vixen-rs](../) Telegram anti-spam bot.

- TypeScript strict · Vite · bun
- SolidJS + `@solidjs/router` · Kobalte UI · Tailwind v4 + CVA
- Biome (lint + format) · Vitest (unit) · YAML→TS i18n codegen with EN+RU parity CI

## Quickstart

```bash
bun install
bun run i18n:gen    # populate gitignored src/shared/i18n/generated/
bun run dev         # http://localhost:3000 (proxies /api/* to :8000)
```

The dashboard requires the [server](../server/) running on `:8000` and a
real Telegram bot. Full provisioning walkthrough — including BotFather
setup, Login Widget domain registration, and the WebApp menu button — is in
[`docs/setup.md`](./docs/setup.md).

## Environment

Copy [`config/template.env`](./config/template.env) to `.env` (gitignored)
and fill in:

```bash
VITE_BOT_USERNAME=your_bot_username   # the @username from BotFather, no `@`
```

Everything else has sensible dev defaults. See [`docs/env.md`](./docs/env.md)
for the full reference, including how the matching server-side `CONFIG_*`
variables (`BOT_TOKEN`, `JWT_SECRET`, `CORS_ORIGINS`, …) shape dashboard
behaviour.

## Validation pipeline

```bash
bun run check       # biome lint + format check
bun run typecheck   # tsc --noEmit (project references)
bun run build       # production bundle to dist/
bun run i18n:check  # EN ↔ RU locale parity
bun run test        # vitest
```

The same five checks run in `.github/workflows/website-ci.yml`.

## Documentation

| File | What it covers |
|---|---|
| [`docs/setup.md`](./docs/setup.md) | End-to-end provisioning: BotFather, Login Widget, WebApp button, chat moderators, env. |
| [`docs/env.md`](./docs/env.md) | Full `VITE_*` reference + which server `CONFIG_*` knobs affect the dashboard. |
| [`docs/auth.md`](./docs/auth.md) | WebApp + Login Widget flows, JWT-in-memory, 401-recovery. |
| [`docs/architecture.md`](./docs/architecture.md) | Directory layout, routing, state, dual-mode rendering. |
| [`docs/api-client.md`](./docs/api-client.md) | `shared/api` interceptor chain + error handling. |
| [`docs/i18n.md`](./docs/i18n.md) | YAML codegen, runtime fetcher, locale switcher. |
| [`docs/ui-kit.md`](./docs/ui-kit.md) | Kobalte + CVA conventions. |
| [`docs/public-reports.md`](./docs/public-reports.md) | M6 public report redaction rules (planned). |
| [`docs/conventions.md`](./docs/conventions.md) | Patterns + best practices. |
| [`docs/rules/`](./docs/rules/) | LLM rules — read before writing code (`solidjs.md`, `components.md`, `typescript.md`, `styling.md`). |
| [`CLAUDE.md`](./CLAUDE.md) | Project-specific assistant rules — Telegram-auth integration, critical gotchas. |
