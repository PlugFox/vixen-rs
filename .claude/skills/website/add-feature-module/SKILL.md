---
name: add-feature-module
description: Scaffold a new feature module under website/src/features/ following the vixen layout (api.ts, types.ts, optional store.ts, components/). Use when the user asks to add a new feature, a new domain area, or a new page backed by API calls.
---

# Add Feature Module (Vixen website)

**Read first**:

- [website/docs/architecture.md](../../../../website/docs/architecture.md) — how features plug into the app.
- [website/docs/api-client.md](../../../../website/docs/api-client.md) — shared API client, interceptors, error contract.
- [website/docs/conventions.md](../../../../website/docs/conventions.md).
- [server/docs/api.md](../../../../server/docs/api.md) — server endpoint signatures the feature talks to.

## Layout

```
website/src/features/{feature}/
  api.ts          # fetch wrappers around shared API client
  types.ts        # DTOs matching server responses
  store.ts        # (optional) SolidJS store / createResource / signals
  components/
    {feature}-list.tsx
    {feature}-item.tsx
    ...
```

The vixen feature set is roughly: `auth`, `chats`, `moderation`, `reports`, `users`, `settings` (per-chat config). Look at an existing feature for the shape.

## File templates

### `types.ts`

```ts
export type Chat = {
  id: number;          // Telegram chat ID — i64 in server, number in TS
  title: string;
  type: "private" | "group" | "supergroup" | "channel";
  membersCount: number;
  createdAt: string;   // ISO 8601
};

export type UpdateChatConfigInput = {
  spamThreshold?: number;
  captchaEnabled?: boolean;
  reportHour?: number;
};
```

Match the **exact** shape from [server/docs/api.md](../../../../server/docs/api.md). If you drift, the runtime will hit unexpected `undefined`. Telegram IDs (`chat_id`, `user_id`) are `i64` server-side — TypeScript `number` is safe up to 2^53; vixen IDs fit comfortably, but for any exact-bit operations use `bigint`.

### `api.ts`

```ts
import { api } from "~/shared/api/client";
import type { Chat, UpdateChatConfigInput } from "./types";

export const chatsApi = {
  list: () => api.get<Chat[]>("/api/v1/chats"),
  get: (id: number) => api.get<Chat>(`/api/v1/chats/${id}`),
  updateConfig: (id: number, input: UpdateChatConfigInput) =>
    api.patch<Chat>(`/api/v1/chats/${id}/config`, input),
};
```

Never build URLs with string concatenation of user input without encoding — use template literals with known values only.

### `store.ts` (when the feature has shared state)

Use `createResource`, `createStore`, or signals depending on whether you need cache-like behavior, deep reactivity, or single values. Don't reach for a store when a local signal inside the component is enough.

## Routing

If the feature has pages:

1. Add a thin `website/src/pages/{feature}.tsx` with `export default` for `lazy()`.
2. Wire the route in the router (usually `website/src/app/router.tsx`).
3. If the page is moderator-only, gate it behind a `<Protected>` wrapper that checks the JWT's `chat_ids` claim.

## i18n

Before writing any user-facing string, create the i18n keys in **both** `en` and `ru` locales. See `add-i18n-string` skill.

## After writing

1. `/website-check` — must be green.
2. Open the feature in a browser and walk through the golden path manually.
