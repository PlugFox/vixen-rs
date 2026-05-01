---
name: telegram-login-widget
description: Embed Telegram Login Widget script in a SolidJS component (browser mode auth), capture callback, POST signed payload to /auth/telegram/login, store JWT in memory.
---

# Telegram Login Widget (Vixen website)

**Read first:**

- [website/docs/auth.md](../../../../website/docs/auth.md) — browser-mode auth flow.
- [server/docs/auth.md](../../../../server/docs/auth.md) — server-side validator.
- [Telegram Login Widget spec](https://core.telegram.org/widgets/login).

## Pattern

```tsx
let containerRef!: HTMLDivElement;

onMount(() => {
  const script = document.createElement("script");
  script.src = "https://telegram.org/js/telegram-widget.js?22";
  script.async = true;
  script.setAttribute("data-telegram-login", import.meta.env.VITE_BOT_USERNAME);
  script.setAttribute("data-size", "large");
  script.setAttribute("data-onauth", "onTelegramAuth(user)");
  script.setAttribute("data-request-access", "write");
  containerRef.appendChild(script);
  onCleanup(() => containerRef.removeChild(script));
});

(window as any).onTelegramAuth = async (user: TgWidgetUser) => {
  await authStore.signIn(user); // POSTs to /auth/telegram/login
};
```

## Server payload composition

The widget callback gives flat fields: `id`, `first_name`, `last_name`, `username`, `auth_date`, `hash`. Compose into a sorted `key=value\nkey=value` string and POST as one field. Server validates against `SHA256(bot_token)` — **not** WebApp's HMAC algorithm. Server sniffs the payload shape and picks the matching algorithm.

## Files

- `website/src/features/auth/components/login-widget.tsx`.
- `website/src/features/auth/store.ts` — JWT in memory.

## Gotchas

- **Never trust `initDataUnsafe` or callback fields directly.** Always submit the full signed payload to the server for HMAC validation. Client-side checks are decoration.
- **JSX `<script>` is stripped.** Solid (and React) strip `<script>` from JSX — must use `document.createElement` + `appendChild`.
- **Login Widget secret = `SHA256(bot_token)`** (raw SHA-256, not HMAC). Different from WebApp's `HMAC_SHA256("WebAppData", bot_token)`. Sending one shape to the wrong validator fails silently with "invalid hash".
- **JWT in memory only.** Auth store, never `localStorage` / `sessionStorage`. Reload re-prompts the widget — acceptable UX trade-off for not having a stealable token at rest.
- **`data-request-access="write"` is required.** Without it, the widget omits a field, the hash set differs, and server validation fails.
- **Cleanup the script on unmount via `onCleanup`** — duplicate widgets render on remount otherwise.
- **User ID is `i64`** — model the type on the TypeScript side as `bigint` or string-with-validation, not `number` (loses precision past `2^53`).

## Verification

- `bun run typecheck && bun run build`.
- Manual: open dashboard → click widget → DevTools → Application → Storage should be empty for the vixen domain. JWT lives in memory only.

## Related

- `tg-webapp-auth` — server validator (HMAC variant).
- `add-feature-module` — feature folder structure.
- `solid-async-cleanup` — `onMount` / `onCleanup` discipline.
