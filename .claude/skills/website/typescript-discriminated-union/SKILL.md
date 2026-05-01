---
name: typescript-discriminated-union
description: Model API responses, ledger entries, modal states with discriminated unions. The discriminant must be required and non-optional. Narrow with if/switch.
---

# TypeScript Discriminated Unions (Vixen website)

**Source:** [TypeScript narrowing handbook](https://www.typescriptlang.org/docs/handbook/2/narrowing.html#discriminated-unions).

**Read first:**

- [website/docs/rules/typescript.md](../../../../website/docs/rules/typescript.md).
- [website/docs/api-client.md](../../../../website/docs/api-client.md).

## Shape

Each variant carries the same field as a **literal type** (`kind`, `type`, `status`, `action` — pick one and stick with it per domain).

```ts
type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: { code: string; message: string } };
```

## Narrowing

```ts
function render<T>(r: ApiResult<T>) {
  if (r.ok) return r.data;        // T
  return r.error.message;         // narrowed to error variant
}
```

TS narrows automatically once the discriminant is checked.

## Vixen API contract

The server returns `{ status: "ok", ... } | { status: "error", code, message }`. Mirror it in TS — never pretend errors are exceptions only.

## Modal states

```ts
type ModerationDialog =
  | { kind: "ban"; user: User }
  | { kind: "unban"; user: User }
  | { kind: "verify"; user: User }
  | { kind: "closed" };

const [dialog, setDialog] = createSignal<ModerationDialog>({ kind: "closed" });
```

Pair with `<Switch>`:

```tsx
<Switch>
  <Match when={dialog().kind === "ban" && dialog()}>
    {(d) => <BanDialog user={(d() as Extract<ModerationDialog, { kind: "ban" }>).user} />}
  </Match>
  <Match when={dialog().kind === "verify" && dialog()}>{...}</Match>
</Switch>
```

## Action ledger entries

```ts
type ModerationAction =
  & { id: string; chatId: number; createdAt: string }
  & (
    | { action: "ban"; reason: string }
    | { action: "unban"; reason: string | null }
    | { action: "verify"; method: "captcha" | "manual" }
    | { action: "delete"; messageId: number }
  );
```

The intersection adds shared fields without losing variant narrowing on `action`.

## Anti-patterns

- `kind?: "x"` — discriminant must be **required**, never optional.
- `{ data?: T; error?: E }` — both optional, neither narrows. Use a union.
- `kind: string` — non-literal type; narrowing degrades to string equality only, exhaustiveness lost.
- `instanceof` checks for plain objects — only works for class instances.

## Exhaustiveness

```ts
function describe(a: ModerationAction): string {
  switch (a.action) {
    case "ban":    return `Banned: ${a.reason}`;
    case "unban":  return "Unbanned";
    case "verify": return `Verified via ${a.method}`;
    case "delete": return `Deleted msg ${a.messageId}`;
    default: { const _e: never = a; throw new Error(`unhandled: ${JSON.stringify(_e)}`); }
  }
}
```

Add a new variant → TS errors here → you can't forget to handle it.

## Component example

```tsx
<Show
  when={props.result.ok && props.result.data}
  fallback={<ErrorBox error={!props.result.ok ? props.result.error : undefined} />}
>
  {(chat) => <ChatDetail chat={chat()} />}
</Show>
```

## Verification

`bun run typecheck`.

## Related

- `add-feature-module` — `types.ts` shape.
- `solid-resource-pattern` — pairs with `ApiResult<T>` from fetchers.
