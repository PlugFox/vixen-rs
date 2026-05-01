# Website Conventions

Quick reference for code organization, naming, formatting, and the few opinionated patterns. The source of truth lives in `docs/rules/*.md` for rule details (SolidJS, TypeScript, components, styling).

## File naming

- All files: `kebab-case.{ts,tsx}` (`chat-card.tsx`, `moderation-action-row.tsx`).
- Components: file name = component name in kebab-case. Inside, the exported component is PascalCase (`export function ChatCard(...)`).
- Tests: `chat-card.test.tsx` next to source.
- Types-only: `types.ts` per feature.
- API: `api.ts` per feature.
- Store: `store.ts` per feature (when needed).
- Target < 150 lines per file. Split if larger.

## Imports

```ts
import type { Chat, ChatConfig } from "./types";
import { Button } from "@/shared/ui/button";
import type { ChatCardProps } from "@/features/chats/types";
```

- Path alias: `@/` = `src/`.
- `import type` for type-only imports — Biome enforces.
- Named exports everywhere.
- `export default` only in `pages/` (for `lazy()` route imports).

## TypeScript

See `docs/rules/typescript.md`. Highlights:

- `tsconfig.json` strict + `noUncheckedIndexedAccess`.
- No TS `enum` — use `as const` objects (`const Roles = { Viewer: 0, Editor: 1 } as const`).
- No `any` — use `unknown` and narrow.
- Telegram IDs (`chat_id`, `user_id`): TS `number` is safe up to 2^53; vixen IDs fit. For exact-bit operations, use `bigint`.

## Storage keys

All `localStorage` keys are prefixed `vixen_` to avoid collisions with other apps on the same origin:

- `vixen_locale` — chosen UI locale (`en` | `ru`).
- `vixen_theme` — `light` | `dark` | `system` (browser mode only).
- `vixen_dismissed_pwa_install` — version of the PWA install banner the user dismissed (planned).

The auth JWT is intentionally **not** stored — kept in memory only (see `docs/auth.md`).

## SolidJS

See `docs/rules/solidjs.md`. Hard rules:

- Never destructure props — use `props.field` or `splitProps`.
- Use `<Show>`, `<For>`, `<Switch>/<Match>` — not ternaries or `.map()` in JSX.
- No `useEffect` — use `createEffect`, `onMount`, `onCleanup`.
- Refs are direct variable assignments (`let inputRef!: HTMLInputElement; <input ref={inputRef} />`).

## Components

See `docs/rules/components.md`. Pattern: CVA variants + Tailwind + `cn()` for overrides. Use Kobalte for accessible primitives (Dialog, Menu, Select, Tooltip, Toast); plain HTML+CVA for simple ones (Button, Card, Badge).

## Styling

See `docs/rules/styling.md`. Pattern:

- Tailwind v4 utilities. CVA for component variants.
- Tokens via `@theme` in CSS — `bg-primary`, `text-foreground`, `border-border`. Never hardcode hex.
- OKLCH over HSL/hex.
- No `!important`. No arbitrary values (`w-[347px]`) — add the token if missing.

## i18n

Never hardcode user-facing text. See `docs/i18n.md`. Keys live in `i18n/messages/{locale}/{namespace}.yaml`. Use `t()`, `tp()`, `<T>` from `@/shared/i18n`.

Russian + English are first-class — every key MUST exist in both `en` and `ru`.

## API client

See `docs/api-client.md`. Pattern:

- Components never `fetch` directly — they call `features/{name}/api.ts`.
- `features/{name}/api.ts` calls the shared client in `shared/api/client.ts`.
- Errors are `ApiError` instances with `code`, `message`, `status`.

## Forms

- Use Kobalte's form primitives where keyboard / a11y matters (TextField, Select, RadioGroup, Switch).
- Validate on blur + on submit. Disable the primary action while invalid; show errors in helper text.
- Never trust client-side validation — the server validates again.

## Tables

- Use semantic `<table>`. Kobalte does not ship a Table primitive.
- Sticky headers for long lists (action ledger). Cursor pagination — never `OFFSET` (mirroring the server).
- Tabular data uses `tabular-nums` (`font-variant-numeric: tabular-nums`).

## Routing

- Routes are lazy-loaded: `lazy(() => import("./pages/chat"))`.
- Route entry files live in `pages/` and `export default`.
- Parameter typing: `useParams<{ chatId: string }>()`. Convert to `number` (or `bigint`) at the boundary.

## Date / time

- All API timestamps are ISO 8601 strings (server returns `chrono::DateTime<Utc>` JSON).
- Use `Intl.DateTimeFormat` for display, locale-aware. Never hand-format ("2026-05-01" → "May 1, 2026" via locale).
- Per-chat times (e.g. `report_hour`) are chat-local — display in the chat's `timezone`, not the user's.

## Telegram-specific UI conventions

- **Inside Telegram WebApp**: prefer the native MainButton for primary form actions. Hide the in-page submit button when MainButton is shown.
- **Inside Telegram WebApp**: BackButton wired to navigation history. Don't render an in-page back-arrow.
- **Browser mode**: standard top header + footer.
- Both modes: respect `Telegram.WebApp.colorScheme` (WebApp) or `prefers-color-scheme` (browser).

## Biome

Key enforced rules (see `biome.json`):

- Indent: 2 spaces
- Semicolons: always
- Quotes: double
- Trailing commas: all
- Line width: 100
- `noExplicitAny`: error
- `useConst`: error
- `noUnusedVariables`: error
- Import sorting: auto

Run before commits:

```bash
bun run check       # lint + format check
bun run check:fix   # auto-fix
```

## Validation pipeline

`/website-check` runs:

1. `bun run check` — Biome lint + format.
2. `bun run typecheck` — `tsc -b --noEmit`.
3. `bun run build` — Vite production build.

Don't ship a feature without all three green. UI changes additionally require manual browser verification (see CLAUDE.md).
