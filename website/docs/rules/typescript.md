# TypeScript Rules

Read this file before writing or modifying TypeScript code in the website.

## File Naming

- All files: `kebab-case.ts` / `kebab-case.tsx`.
- Components: `chat-card.tsx` (not `ChatCard.tsx`).
- Tests: `chat-card.test.tsx` (next to source).
- Types-only: `types.ts`.
- API: `api.ts`.
- Store: `store.ts`.
- Target < 150 lines per file. Split if larger.

## Imports

```ts
// Type-only imports
import type { Chat, ChatConfig } from "./types";

// Path alias: @/ = src/
import { Button } from "@/shared/ui/button";
import type { ChatCardProps } from "@/features/chats/types";
```

- Named exports everywhere.
- `export default` only in `pages/` (for `lazy()` route imports).
- Use `import type` for type-only imports — Biome enforces this.

## Type Conventions

### No enums

```ts
// WRONG
enum ActorKind { Bot, Moderator }

// CORRECT
export const ActorKinds = {
  Bot: "bot",
  Moderator: "moderator",
} as const;

export type ActorKind = (typeof ActorKinds)[keyof typeof ActorKinds];
```

### No `any`

```ts
// WRONG
function parse(data: any) { ... }

// CORRECT
function parse(data: unknown) {
  if (typeof data === "string") { ... }
}
```

`tsconfig.json` has `"strict": true`. Biome enforces `noExplicitAny: error`.

### Nullability

Match server conventions:

- `null` for absent DB values (server returns `null`).
- `undefined` for optional params not provided by client.

### Telegram IDs

`chat_id`, `user_id` are `i64` server-side. In TS:

- Use `number` for storage and display — JS Number is safe up to 2^53; Telegram IDs fit.
- Use `bigint` only for exact-bit operations (none in vixen v1).
- The API layer normalizes both: server-side JSON serializes `i64` as `number`, the client deserializes back to `number`.

```ts
export type ModerationAction = {
  id: string;            // UUID
  chatId: number;        // i64 → number
  targetUserId: number;
  action: "ban" | "unban" | "mute" | "unmute" | "delete" | "verify" | "unverify";
  actorKind: "bot" | "moderator";
  actorUserId: number | null;
  createdAt: string;     // ISO 8601
};
```

### Telegram WebApp types

The `Telegram.WebApp` global lives in `shared/lib/telegram-webapp.ts` with a hand-written `.d.ts` augmentation:

```ts
declare global {
  interface Window {
    Telegram?: {
      WebApp?: {
        initData: string;
        initDataUnsafe: { user?: { id: number; username?: string; language_code?: string } };
        ready(): void;
        close(): void;
        colorScheme: "light" | "dark";
        themeParams: Record<string, string>;
        viewportHeight: number;
        BackButton: { show(): void; hide(): void; onClick(cb: () => void): void };
        MainButton: { show(): void; hide(): void; setText(text: string): void; onClick(cb: () => void): void };
      };
    };
  }
}

export {};
```

Or via `@twa-dev/types` if it stays maintained — check before adding the dep.

## Feature Structure

Each feature module follows this layout:

```
features/{name}/
  api.ts          — Typed API functions (calls shared client)
  types.ts        — Request/response types, domain types
  store.ts        — createSignal/createStore (if stateful)
  components/     — Feature-specific components
```

- Components never import the API `client` directly — only through `features/{name}/api.ts`.
- Update `types.ts` if API shape changes.
- Check that `api.ts` matches server endpoint signatures (see `../../server/docs/api.md`).

## i18n

Never hardcode user-facing text. Use i18n functions:

```tsx
import { t, tp } from "@/shared/i18n/i18n";
import { T } from "@/shared/i18n/rich";
import { common, chats } from "@/shared/i18n/generated";

<button>{t(common.save)}</button>           // simple
<span>{tp(chats.list.count, count())}</span> // plural
<p><T msg={common.deleteWarning} /></p>      // rich text
```

After adding labels to `i18n/messages/{locale}/{namespace}.yaml`, run `bun run i18n:gen`.

See `../i18n.md` for full reference.

## Error handling

Throw typed errors, catch by type:

```ts
import { ApiError, NetworkError } from "@/shared/api/client";

try {
  await chatsApi.list();
} catch (e) {
  if (e instanceof ApiError && e.code === "MODERATOR_REQUIRED") {
    // handle structured error
  } else if (e instanceof NetworkError) {
    // handle network failure
  } else {
    // unexpected — let the ErrorBoundary catch it
    throw e;
  }
}
```

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
