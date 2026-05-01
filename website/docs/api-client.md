# API Client

`shared/api/client.ts` — `fetch` wrapper with an interceptor chain. All feature `api.ts` files use this; components never `fetch` directly.

## Usage

```ts
import { api } from "@/shared/api/client";
import type { Chat } from "./types";

export const chatsApi = {
  list: () => api.get<Chat[]>("/api/v1/chats"),
  get: (id: number) => api.get<Chat>(`/api/v1/chats/${id}`),
  patchConfig: (id: number, body: Partial<ChatConfig>) =>
    api.patch<Chat>(`/api/v1/chats/${id}/config`, body),
};
```

## Response envelope

The server returns:

```ts
type ApiResponse<T> =
  | { status: "ok"; data: T }
  | { status: "error"; error: { code: string; message: string } };
```

The client unwraps:

- `status: "ok"` → returns `T`.
- `status: "error"` → throws `ApiError { code, message, status: number }`.
- Network failure → throws `NetworkError`.

## Interceptor chain

```
request
  ├─ buildUrl(path, query)
  ├─ buildBody(body, headers)
  ├─ before-interceptors (auth, metadata, logging)
  ├─ fetch
  ├─ after-interceptors (logging)
  └─ parseResponse
       ├─ ok → return T
       └─ error → throw ApiError or NetworkError
```

### `auth` interceptor

Reads the JWT from `features/auth/store.ts`. If present, adds `Authorization: Bearer <jwt>`.

**Reactive 401 handling**:

- WebApp mode: read fresh `Telegram.WebApp.initData`, POST to `/auth/telegram/login`, retry the original request once.
- Browser mode: signal `auth.expired = true`, surface the Login Widget; do NOT retry.

This is **not** a JWT refresh flow (no refresh tokens server-side) — it's an initData re-submit.

### `metadata` interceptor

Adds `X-Request-ID: <uuid>` to every request. Used by the server to correlate logs.

### `logging` interceptor

Debug-level logging in dev (`import.meta.env.DEV`). Stripped in prod builds.

## Error handling at the call site

```tsx
try {
  const chats = await chatsApi.list();
  setChats(chats);
} catch (e) {
  if (e instanceof ApiError) {
    if (e.code === "MODERATOR_REQUIRED") {
      navigate("/");
      return;
    }
    showToast(t(`errors.${e.code.toLowerCase()}`));
  } else if (e instanceof NetworkError) {
    showToast(t("errors.network"));
  }
}
```

Localized error messages live under `i18n/messages/{locale}/errors.yaml`, keyed by `errors.<code>` matching the server's error codes (see `../server/docs/rules/error-handling.md`).

## With `createResource`

Most reads use `createResource`:

```tsx
const [chats, { refetch }] = createResource(() => chatsApi.list());

<Show when={!chats.loading} fallback={<Skeleton />}>
  <Show when={!chats.error} fallback={<ErrorState onRetry={refetch} />}>
    <For each={chats()}>{(chat) => <ChatCard chat={chat} />}</For>
  </Show>
</Show>
```

After mutations, call `refetch()` (no automatic cache invalidation).

## Pagination

```ts
list: (cursor?: string, limit = 50) =>
  api.get<{ items: ModerationAction[]; has_more: boolean; cursor?: string }>(
    `/api/v1/chats/${chatId}/moderation/actions`,
    { query: { cursor, limit } }
  );
```

The `cursor` is opaque — never construct it client-side.

## File / blob downloads

For PNG / file responses:

```ts
chartUrl: (chatId: number) => `${api.baseUrl}/api/v1/chats/${chatId}/reports/chart.png`;
```

Render directly via `<img src={chartUrl(chatId)} />`. The `auth` interceptor doesn't apply to `<img>` requests, so:

- Authenticated chart endpoints → server accepts `?token=<jwt>` query param as a fallback for image tags. The dashboard appends it.
- Public chart endpoint → no auth needed.

## Configuration

`api.baseUrl` defaults to the current origin (`window.location.origin`). For dev with separate ports, set `VITE_API_BASE_URL=http://localhost:8000` in `.env.local`. Vite injects via `import.meta.env`.

## Related

- Server endpoint reference: `../server/docs/api.md`
- Auth flow: `auth.md`
- Error codes: `../server/docs/rules/error-handling.md`
