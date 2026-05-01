# Authentication (Website)

The dashboard authenticates via Telegram. Two entry modes:

1. **Inside Telegram WebApp** — the bot exposes an "Open dashboard" inline button with a `web_app` field. Telegram opens the dashboard in a WebView, exposing `Telegram.WebApp.initData` immediately.
2. **Browser** — user navigates to the dashboard URL directly. The Telegram Login Widget renders; user signs in via Telegram, the widget posts a callback to `/auth/callback`. The website composes an `initData`-shaped string from the callback fields.

Both modes converge: POST raw signed `initData` to `POST /api/v1/auth/telegram/login`. Server validates HMAC, mints a JWT (1h TTL), returns `{token, user, chat_ids}`. JWT lives in **memory only**.

The matching server-side flow is in [`../../server/docs/auth.md`](../../server/docs/auth.md).

## WebApp mode

```tsx
// shared/lib/telegram-webapp.ts
declare global {
  interface Window {
    Telegram?: {
      WebApp?: {
        initData: string;
        initDataUnsafe: { user?: { id: number; username?: string } };
        ready(): void;
        close(): void;
        colorScheme: "light" | "dark";
        // ...
      };
    };
  }
}

export function isInWebApp(): boolean {
  return typeof window !== "undefined" && !!window.Telegram?.WebApp?.initData;
}

export function getInitData(): string | null {
  return window.Telegram?.WebApp?.initData ?? null;
}
```

```tsx
// features/auth/components/webapp-bootstrap.tsx
onMount(async () => {
  if (!isInWebApp()) return;
  window.Telegram!.WebApp!.ready();
  const initData = getInitData()!;
  await authStore.signInWithInitData(initData);
});
```

`Telegram.WebApp.ready()` MUST be called once — it tells the parent Telegram client that the WebView is loaded and ready to receive theme / viewport events.

**Critical**: never trust `Telegram.WebApp.initDataUnsafe`. It's "unsafe" because the client-side fields are not signed. Always submit the raw `initData` string and let the server validate the HMAC.

## Browser mode (Telegram Login Widget)

The Login Widget is a Telegram-hosted script. It must be injected via `onMount` + `appendChild` (NOT JSX `<script>` — JSX strips inline `<script>` tags):

```tsx
// features/auth/components/login-widget.tsx
onMount(() => {
  const script = document.createElement("script");
  script.async = true;
  script.src = "https://telegram.org/js/telegram-widget.js?22";
  script.setAttribute("data-telegram-login", import.meta.env.VITE_BOT_USERNAME);
  script.setAttribute("data-size", "large");
  script.setAttribute("data-onauth", "onTelegramAuth(user)");
  script.setAttribute("data-request-access", "write");
  containerRef.appendChild(script);
});

// Global callback the widget invokes:
(window as any).onTelegramAuth = async (user: TelegramLoginUser) => {
  const initDataLike = composeInitDataFromLoginWidget(user);
  await authStore.signInWithInitData(initDataLike);
};
```

The Login Widget callback shape differs from WebApp `initData`:

- Login Widget: flat fields `{id, first_name, last_name, username, photo_url, auth_date, hash}`.
- WebApp `initData`: URL-encoded query string with `user` JSON.

`composeInitDataFromLoginWidget(user)` builds a query-string-shaped payload that the server can validate (server detects the format and uses the matching algorithm — `HMAC_SHA256("WebAppData", bot_token)` for WebApp, `SHA256(bot_token)` for Login Widget).

## Auth store

```ts
// features/auth/store.ts
import { createSignal } from "solid-js";

export const [jwt, setJwt] = createSignal<string | null>(null);
export const [user, setUser] = createSignal<{ id: number; username?: string } | null>(null);
export const [chatIds, setChatIds] = createSignal<number[]>([]);

export async function signInWithInitData(initData: string): Promise<void> {
  const res = await api.post<{ token: string; user: User; chat_ids: number[] }>(
    "/api/v1/auth/telegram/login",
    { initData }
  );
  setJwt(res.token);
  setUser(res.user);
  setChatIds(res.chat_ids);
}

export function signOut(): void {
  setJwt(null);
  setUser(null);
  setChatIds([]);
  if (isInWebApp()) {
    window.Telegram?.WebApp?.close();
  }
}

export function isAuthenticated(): boolean {
  return jwt() !== null;
}
```

The auth store is module-scope (effectively a singleton). `<Protected>` components read `isAuthenticated()`; the API client's auth interceptor reads `jwt()` for the `Authorization` header.

## JWT in memory, not localStorage

This is intentional:

- **WebApp mode**: `Telegram.WebApp.initData` is always one read away; re-submitting on cold start is cheap.
- **Browser mode**: re-prompting the Login Widget is one click; cold start re-prompt is acceptable UX.
- **Security**: localStorage adds a session-fixation surface (a malicious script in the same origin can read it).

Page reload = re-auth. For the rare case where this is annoying (frequent reload during dev), `VITE_DEV_PERSIST_JWT=1` can persist to sessionStorage — never enabled in prod.

## Permission gating

The JWT carries `chat_ids: number[]`. The dashboard uses this to:

- Hide chat tabs for chats the user doesn't moderate.
- Pre-filter API requests (don't bother fetching chats the user can't see).

**Server-side double-check is mandatory**, not optional. The website's gating is a UX hint, not a security boundary. Every chat-scoped server endpoint re-verifies the path's `chat_id` is in the JWT's `chat_ids`.

```tsx
// app/protected.tsx
export function Protected(props: ParentProps & { chatId?: number }) {
  return (
    <Show when={isAuthenticated()} fallback={<LoginPrompt />}>
      <Show
        when={!props.chatId || chatIds().includes(props.chatId)}
        fallback={<NotAuthorized />}
      >
        {props.children}
      </Show>
    </Show>
  );
}
```

## Logout

- WebApp mode: `setJwt(null)` + `Telegram.WebApp.close()` — Telegram closes the WebView.
- Browser mode: `setJwt(null)` + redirect to `/`.

There's no server-side logout endpoint in v1 (no JWT revocation list). To force-revoke all sessions globally, ops rotates `CONFIG_JWT_SECRET` and redeploys.

## Failure modes

| Failure | Effect | UX |
|---|---|---|
| `initData` HMAC invalid | 401 `INVALID_INIT_DATA` | "Sign-in failed. Please try again." (likely a stale tab) |
| `initData` `auth_date` > 24h | 401 `INIT_DATA_EXPIRED` | Same — re-prompt |
| User is not in `chat_moderators` for any chat | 403 `MODERATOR_REQUIRED` | "You are not a moderator of any watched chat. Contact a chat owner to be added." |
| JWT expired (1h) | 401 on next request | Auth interceptor re-submits initData; transparent retry. |
| Bot token rotated by ops | All current JWTs become invalid (HMAC chain broken) | Auth interceptor catches 401, re-submits; if the bot token is also rotated server-side, re-submit also fails — user sees the sign-in failure message. |

## Related

- Server flow: [`../../server/docs/auth.md`](../../server/docs/auth.md)
- API endpoints: [`../../server/docs/api.md`](../../server/docs/api.md) (`/auth/telegram/login`, `/auth/me`)
- Skill: `.claude/skills/website/telegram-login-widget/SKILL.md` (added in M4)
