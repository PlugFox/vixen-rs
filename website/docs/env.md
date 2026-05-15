# Environment Variables

The dashboard reads two classes of configuration:

1. **Build-time `VITE_*` variables** — baked into the JS bundle by Vite. Set
   in `website/.env` or `website/.env.local` for dev; injected by CI from
   `github.sha` / `github.run_id` for production builds. See
   [`../config/template.env`](../config/template.env) for the canonical list.
2. **Runtime `window.__BOT_USERNAME__`** — single ops override hatch for the
   bot username so an already-built bundle can be re-pointed without a
   rebuild.

The server's `CONFIG_*` variables are documented separately in
[`../../server/config/template.env`](../../server/config/template.env) and
[`../../server/docs/config.md`](../../server/docs/config.md); a few of them
shape the dashboard's behaviour and are flagged below.

## Build-time variables

All `VITE_*` keys are exposed in two places:

- `vite.config.ts` `define` block — referenced as bare `__CLIENT_VERSION__`,
  `__BUILD_TIME__`, `__GIT_COMMIT__`, `__GIT_BRANCH__`, `__API_URL__`,
  `__BOT_USERNAME__` from application code.
- `import.meta.env.VITE_*` — also available directly, but the project
  convention is to read through the `__` constants for type safety
  (declared in `src/global.d.ts`).

| Key | Required | Default | Purpose |
|---|---|---|---|
| `VITE_BOT_USERNAME` | yes (for Login Widget) | `""` | Telegram bot username **without** `@`. The Login Widget will refuse to render when empty or unknown to Telegram. |
| `VITE_API_URL` | no | `/api/v1` | API base URL. Same-origin in dev (Vite proxies `/api/*` to `:8000`). Set an absolute URL for split deploys. |
| `VITE_VERSION` | no | `package.json` version | Embedded in the build for /about display. |
| `VITE_BUILD_TIME` | no | `new Date().toISOString()` | Ditto. |
| `VITE_GIT_COMMIT` | no | `"unknown"` | Ditto. |

`VITE_*` reads happen at **build time** — changing them requires a rebuild.

## Runtime override

The Login Widget's bot username is the only value we expect ops to override
post-build (e.g. when running the same bundle against a staging bot vs a
production bot):

```html
<!-- index.html — injected by the orchestrator before the bundle script -->
<script>
  window.__BOT_USERNAME__ = "vixen_prod_bot";
</script>
```

`shared/lib/telegram-webapp.ts::botUsername()` returns
`window.__BOT_USERNAME__ ?? __BOT_USERNAME__`, so the runtime value wins.

## Server-side knobs that affect the dashboard

These live on the server, not in the website build, but the dashboard's
behaviour depends on them:

| Server key | Affects | Note |
|---|---|---|
| `CONFIG_BOT_TOKEN` | Login + initData HMAC | Must match the bot whose `username` matches `VITE_BOT_USERNAME`. Mismatch = every login fails 401 `INVALID_INIT_DATA`. |
| `CONFIG_JWT_SECRET` | JWT signing | ≥ 32 bytes. Rotating it invalidates every live dashboard session. |
| `CONFIG_JWT_TTL_SECS` | JWT lifetime | Default 3600 (1h). The dashboard re-submits initData on 401, so a short TTL is fine. |
| `CONFIG_INIT_DATA_MAX_AGE_SECS` | initData freshness | Default 86400 (24h). Rejects stale Login Widget callbacks. |
| `CONFIG_CORS_ORIGINS` | CORS | Must include the dashboard origin (default `http://localhost:3000` in dev). Production: explicit absolute origin, **no wildcards**. |
| `CONFIG_CHATS` | Which chats appear | Dashboard only lists chats in this set (intersected with the JWT's `chat_ids`). |
| `CONFIG_OPENAPI_UI` | Scalar UI gate | Default `true` in dev, `false` elsewhere. Mounted at `/scalar`. |

## Local development

```bash
# website/.env (gitignored)
VITE_BOT_USERNAME=vixen_test_bot
# Leave VITE_API_URL unset to use the Vite proxy.

# server/.env (gitignored, see server/config/template.env)
CONFIG_BOT_TOKEN=1234567890:AAEzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz
CONFIG_CHATS=-1001234567890
CONFIG_JWT_SECRET=$(openssl rand -hex 32)
CONFIG_ADMIN_SECRET=$(openssl rand -hex 32)
CONFIG_CORS_ORIGINS=http://localhost:3000
```

See [`setup.md`](./setup.md) for the end-to-end provisioning walkthrough.

## CI injection

`.github/workflows/website-ci.yml::build` sets:

```yaml
env:
  VITE_VERSION: ${{ github.sha }}
  VITE_GIT_COMMIT: ${{ github.sha }}
  VITE_BUILD_TIME: ${{ github.run_id }}
```

`VITE_BOT_USERNAME` is **not** set in CI — production builds inject it via
the orchestrator (Docker build arg, k8s ConfigMap, etc.) or via the
`window.__BOT_USERNAME__` runtime hatch.
