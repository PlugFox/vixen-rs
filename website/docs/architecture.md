# Website Architecture

SolidJS SPA bundling two surfaces:

1. **Moderator dashboard** (`/app/*`) — auth-gated via Telegram. Lists chats, shows action ledger, edits per-chat config, triggers manual moderation.
2. **Public chat report** (`/report/{chat_slug}`) — unauthenticated, indexable, redacted aggregates.

One Vite build, two route trees in the same SPA. Auth state determines which is reachable.

## Stack

| Concern | Choice |
|---|---|
| Language | TypeScript strict |
| UI framework | SolidJS |
| Routing | `@solidjs/router` |
| Component primitives | Kobalte (`@kobalte/core`) |
| Styling | Tailwind CSS v4 + CVA + `cn()` (`clsx + tailwind-merge`) |
| Build tool | Vite + `vite-plugin-solid` + `vite-plugin-pwa` |
| Lint / format | Biome (single tool, replaces ESLint + Prettier) |
| Package manager | bun |
| Testing | vitest + `@solidjs/testing-library` (planned, not in M0) |

## Directory structure

```
website/
├── package.json
├── tsconfig.json
├── biome.json
├── vite.config.ts
├── tailwind.config.cjs
├── postcss.config.cjs
├── index.html
├── i18n/
│   └── messages/
│       ├── en/{common,auth,chats,moderation,reports,settings,errors}.yaml
│       └── ru/{common,auth,chats,moderation,reports,settings,errors}.yaml
├── public/
│   ├── robots.txt
│   ├── 404.html
│   └── (favicon, manifest, icons)
└── src/
    ├── main.tsx              # Vite entry
    ├── app/
    │   ├── router.tsx
    │   ├── root-layout.tsx
    │   ├── webapp-layout.tsx        # narrower layout when inside Telegram WebApp
    │   └── providers.tsx            # MetaProvider, ThemeProvider, etc.
    ├── pages/                       # Route entry points (export default for lazy())
    │   ├── home.tsx
    │   ├── chat.tsx
    │   ├── moderation.tsx
    │   ├── reports.tsx
    │   ├── settings.tsx
    │   ├── public-report.tsx
    │   ├── auth-callback.tsx        # Telegram Login Widget redirect target
    │   └── not-found.tsx
    ├── features/
    │   ├── auth/
    │   │   ├── api.ts
    │   │   ├── types.ts
    │   │   ├── store.ts             # JWT signal + initData submit
    │   │   └── components/
    │   │       ├── login-widget.tsx
    │   │       └── webapp-bootstrap.tsx
    │   ├── chats/{api,types,store,components}/
    │   ├── moderation/{api,types,components}/
    │   ├── reports/{api,types,components}/
    │   ├── users/{api,types,components}/
    │   └── settings/{api,types,components}/
    └── shared/
        ├── api/
        │   ├── client.ts            # fetch wrapper + interceptor chain
        │   ├── interceptors.ts      # auth, logging, metadata
        │   └── types.ts             # ApiResponse, ApiError, PaginatedResponse
        ├── ui/                      # Kobalte wrappers + plain primitives
        │   ├── button.tsx
        │   ├── card.tsx
        │   ├── dialog.tsx
        │   ├── dropdown-menu.tsx
        │   ├── input.tsx
        │   ├── badge.tsx
        │   └── ...
        ├── lib/
        │   ├── cn.ts                # clsx + tailwind-merge
        │   ├── format.ts            # date / number / chat title formatting
        │   └── telegram-webapp.ts   # detect WebApp container, read initData
        ├── hooks/
        │   ├── use-media-query.ts
        │   └── use-pagination.ts
        └── i18n/
            ├── i18n.ts              # t(), tp()
            ├── rich.tsx             # <T msg=...> for rich text
            ├── types.ts
            └── generated/           # auto-generated from YAML; gitignored
```

## Design principles

- **Feature isolation** — each feature is self-contained; imports from `shared/` only. Cross-feature imports are a smell.
- **Pages are thin** — route entry points, no business logic. Any non-trivial logic moves into a feature module.
- **Flat over nested** — max 3 levels deep, prefer wide over deep.
- **Small files** — target < 150 lines. Split if larger.
- **Co-location** — types live in `feature/{name}/types.ts` or `shared/api/types.ts`.
- **Build only what you need** — UI primitives are added to `shared/ui/` only when used by 2+ features. One-off UI lives in the feature.

## State management

SolidJS signals only — no external state library.

| Scope | Tool |
|---|---|
| Local component | `createSignal` (form input, modal open/close) |
| Feature-level | module-scope `createSignal` (`features/auth/store.ts` keeps the JWT) |
| Cross-feature | module-scope `createSignal` (theme, locale) |
| Server data | `createResource` with `refetch()` for mutations |

The auth JWT is intentionally kept in memory (not `localStorage`) — initData re-submission is cheap, localStorage adds session-fixation surface.

## API client

`shared/api/client.ts` — `fetch` wrapper with an interceptor chain:

```
buildUrl → buildBody → interceptors.before → fetch → interceptors.after → parseResponse
```

Built-in interceptors:

- **auth** — injects `Authorization: Bearer <jwt>` from the auth store. On 401, signals the auth store to re-submit initData (in WebApp mode) or prompts re-login (browser mode).
- **logging** — debug-level request / response logging in dev.
- **metadata** — adds `X-Request-ID` (UUIDv4) for traceability.

Response format mirrors the server's:

```ts
type ApiResponse<T> = { status: "ok"; data: T } | { status: "error"; error: { code: string; message: string } };
```

The client throws `ApiError` (with `code`, `message`, `status`) or `NetworkError` so consumers can handle structurally.

See `docs/api-client.md`.

## Routing

`@solidjs/router` with lazy-loaded routes. The router determines whether to mount `RootLayout` (browser) or `WebappLayout` (inside Telegram WebApp) based on `window.Telegram?.WebApp?.initData`:

```tsx
<Router root={Telegram?.WebApp?.initData ? WebappLayout : RootLayout}>
  <Route path="/" component={lazy(() => import("./pages/home"))} />
  <Route path="/chats/:chatId" component={lazy(() => import("./pages/chat"))} />
  {/* ... */}
  <Route path="/report/:chatSlug" component={lazy(() => import("./pages/public-report"))} />
  <Route path="/auth/callback" component={lazy(() => import("./pages/auth-callback"))} />
  <Route path="*" component={lazy(() => import("./pages/not-found"))} />
</Router>
```

Routes under `/app/*` (or `/chats/*`, `/reports/*`, etc.) are gated by a `<Protected>` wrapper that checks `auth.jwt` and `auth.chat_ids`. Public report routes do NOT use this wrapper.

## Two-mode rendering

`WebappLayout` (inside Telegram WebApp container):

- No top-level header (the parent Telegram client provides one).
- BackButton wired to Telegram's native back via `Telegram.WebApp.BackButton`.
- MainButton (Telegram's bottom button) used for primary actions in form-heavy screens.
- Theme synced from `Telegram.WebApp.colorScheme` and `Telegram.WebApp.themeParams`.
- Viewport adapts to `Telegram.WebApp.viewportHeight`.

`RootLayout` (regular browser):

- Standard top header with logo + login button.
- Standard light/dark theme toggle (per `prefers-color-scheme`).

A small helper `shared/lib/telegram-webapp.ts` detects the mode and exposes the WebApp object as a typed signal.

## Build & bundle

- Vite production build with code-splitting per lazy route.
- Source maps disabled in prod (re-enable for staging via env var).
- Initial load target: < 50KB gzipped (SolidJS runtime ~7KB + app shell).
- PWA: precache + manifest via `vite-plugin-pwa` (offline fallback for the dashboard shell).

## Error handling

- Network errors → toast with retry CTA.
- API errors → toast with localized message keyed by server's `error.code`.
- Unexpected exceptions → Solid `<ErrorBoundary>` at the layout level; logs to console (and the planned telemetry endpoint).

## Theme

CSS variables in `index.css`, toggled via `data-kb-theme` on `<html>`. Kobalte reads `data-kb-theme` natively. UI kit references variables via Tailwind (`hsl(var(--primary))` or OKLCH equivalents).

In WebApp mode, the theme follows `Telegram.WebApp.colorScheme`. In browser mode, it follows `prefers-color-scheme` with localStorage override (`vixen_theme`).

## What's deliberately not here

- No SSR / SSG (Vite only). Public report SEO needs minimum either pre-rendering at build time (planned) or a small server-side renderer (also planned). v1 ships as pure SPA — public report SEO is degraded on launch.
- No external state library (Redux, Zustand). SolidJS signals are sufficient.
- No GraphQL. Plain REST against the typed OpenAPI from server.
- No Playwright in M0 (manual verification only). Added later when the surface stabilizes.
