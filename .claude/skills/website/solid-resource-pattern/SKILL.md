---
name: solid-resource-pattern
description: Use createResource for async data with loading/error/refetch. Avoid createSignal+createEffect for fetching. Use .state for granular pending|errored|ready.
---

# SolidJS createResource Pattern (Vixen website)

**Source:** [SolidJS createResource](https://docs.solidjs.com/reference/basic-reactivity/createresource).

**Read first:**

- [website/docs/rules/solidjs.md](../../../../website/docs/rules/solidjs.md).
- [website/docs/api-client.md](../../../../website/docs/api-client.md).

## Shape

```ts
const [chats, { refetch, mutate }] = createResource(
  () => filters().query,                     // dependency (signal)
  (q) => chatsApi.list({ query: q }),        // fetcher; throws on HTTP error
);
chats();           // T | undefined
chats.loading;     // boolean
chats.error;       // unknown | undefined
chats.state;       // "unresolved" | "pending" | "ready" | "refreshing" | "errored"
```

The resource is callable — `chats()` returns the latest value or `undefined`.

## State (granular)

Use `resource.state` instead of `loading`:

- `unresolved` — never fetched (deps undefined).
- `pending` — first fetch in flight.
- `ready` — has data.
- `refreshing` — has stale data, new fetch in flight.
- `errored` — last fetch threw.

`refreshing` keeps the previous value visible — perfect for "showing stale + spinner" UX.

## `<Show>` usage

```tsx
<Show when={chats()} fallback={<ChatListSkeleton />}>
  {(c) => <ChatList chats={c()} />}
</Show>
```

The callback form `(c) => ...` re-narrows on every reactive read.

## Refetch on mutation

```tsx
async function save() {
  await chatsApi.update(local.id, input);
  refetch();                                  // revalidate
}
```

## Optimistic updates

```tsx
mutate((prev) => prev && { ...prev, name: "new" });
try { await chatsApi.rename(id, "new"); refetch(); }
catch { refetch(); }                          // server is source of truth
```

## Error UI

```tsx
<Show when={chats.error} fallback={...}>
  {(err) => <ErrorBox code={(err() as ApiError).code} onRetry={refetch} />}
</Show>
```

Vixen `ApiError` exposes `.code` for branching (`NETWORK`, `UNAUTHORIZED`, `NOT_FOUND`).

## Auth / 401

Leave to the api-client interceptor — it refreshes JWT or re-prompts. The resource fetcher just throws `ApiError`; don't add auth logic in fetchers.

## Full example + dependent resources

```tsx
const [selected] = createSignal<number | null>(null);
const [chat, { refetch }] = createResource(selected, (id) => chatsApi.get(id));

return (
  <Switch>
    <Match when={chat.error}><ErrorState onRetry={refetch} /></Match>
    <Match when={chat.loading}><ChatDetailSkeleton /></Match>
    <Match when={chat()}>{(c) => <ChatDetail chat={c()} onSaved={refetch} />}</Match>
  </Switch>
);
```

When the dep signal is `null`/`undefined`, the fetcher does not run (`unresolved` state).

## Gotchas

- Do **not** `createEffect(() => fetcher().then(setData))` — race conditions on rapid dep change. Use `createResource`.
- Fetcher must **throw** on HTTP error so `.error` populates. The api-client throws `ApiError` for non-2xx.
- Pending requests **abort automatically** on disposal (no manual `AbortController` for the resource itself).
- For multiple deps, return an object: `createResource(() => ({ a: a(), b: b() }), ({ a, b }) => ...)`.
- Don't read `chats()` outside reactive scope — value will be stale.

## Verification

`bun run typecheck && bun run build`.

## Related

- `solid-async-cleanup` — onCleanup for non-resource async.
- `add-feature-module` — where resources fit in feature layout.
- `loading-empty-error-states` — the five UI states for resources.
