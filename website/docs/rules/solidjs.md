# SolidJS Rules

Read this file before writing or modifying SolidJS components.

SolidJS is NOT React. Components run once (no re-renders). Reactivity is tracked through signal access in reactive contexts. Breaking these rules silently kills reactivity.

## Never Destructure Props

Destructuring breaks reactivity — the value is read once and never updates.

```tsx
// WRONG — breaks reactivity
function Card({ title, count }: Props) {
  return <div>{title} ({count})</div>;
}

// CORRECT — reactive access
function Card(props: Props) {
  return <div>{props.title} ({props.count})</div>;
}

// CORRECT — splitProps when forwarding subsets
function Card(props: Props & { class?: string }) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return <div class={cn("rounded-lg", local.class)} {...rest}>{local.children}</div>;
}

// CORRECT — mergeProps for defaults
function Card(props: Props) {
  const merged = mergeProps({ variant: "default" as const }, props);
  return <div>{merged.variant}</div>;
}
```

## Control Flow Components

Use SolidJS control flow components, not JS expressions. Ternaries and `.map()` evaluate eagerly and bypass fine-grained reactivity.

```tsx
// WRONG
{isLoading() ? <Spinner /> : <Content />}
{items().map(item => <Card item={item} />)}

// CORRECT
<Show when={!isLoading()} fallback={<Spinner />}>
  <Content />
</Show>

<For each={items()}>{(item) => <Card item={item} />}</For>

<Switch>
  <Match when={status() === "loading"}><Spinner /></Match>
  <Match when={status() === "error"}><ErrorState /></Match>
  <Match when={status() === "success"}><Content /></Match>
</Switch>
```

`<Show>` with callback form for narrowing:

```tsx
<Show when={user()}>
  {(u) => <span>{u().name}</span>}
</Show>
```

## Lifecycle and Effects

No `useEffect`, `useState`, `useMemo`, `useCallback` — these are React APIs.

| React | SolidJS | Notes |
|-------|---------|-------|
| `useState` | `createSignal` | Returns `[getter, setter]`, getter is a function |
| `useReducer` | `createStore` | For complex/nested state |
| `useEffect` | `createEffect` | Tracks dependencies automatically |
| `useEffect(_, [])` | `onMount` | Runs once after first render |
| `useEffect` cleanup | `onCleanup` | Register cleanup inside effect or component |
| `useMemo` | `createMemo` | Cached derived value |
| `useCallback` | Not needed | Functions don't cause re-renders |
| `useRef` | `let ref!: HTMLElement` | Direct variable assignment |

```tsx
function Timer() {
  const [count, setCount] = createSignal(0);
  const doubled = createMemo(() => count() * 2);

  onMount(() => {
    console.log("mounted");
  });

  createEffect(() => {
    console.log("count changed:", count());
  });

  onCleanup(() => {
    console.log("disposing");
  });

  return <span>{doubled()}</span>;
}
```

## Data Fetching

Use `createResource` for async data. Do not use `createEffect` + `createSignal` for fetching.

```tsx
const [chat] = createResource(() => chatId(), (id) => chatsApi.get(id));

<Show when={chat()} fallback={<Skeleton />}>
  {(c) => <ChatHeader chat={c()} />}
</Show>
```

For mutations, call `refetch()`:

```tsx
const [chats, { refetch }] = createResource(() => chatsApi.list());

async function handleBan() {
  await moderationApi.ban(chatId, userId);
  await refetch();
}
```

## Refs

Assign refs directly, no `useRef`:

```tsx
function FocusableInput() {
  let inputRef!: HTMLInputElement;

  onMount(() => {
    inputRef.focus();
  });

  return <input ref={inputRef} />;
}
```

## Event Handlers

- Props: `on` prefix (`onClick`, `onDelete`).
- Internal handlers: `handle` prefix (`handleClick`, `handleSubmit`).
- SolidJS uses native DOM events — no synthetic event system.

```tsx
interface Props {
  onBan?: (userId: number) => void;
}

function ModerationActionRow(props: Props) {
  const handleBan = () => props.onBan?.(props.userId);
  return <button onClick={handleBan}>{t("moderation.ban")}</button>;
}
```

## Common Mistakes

| Mistake | Why it breaks | Fix |
|---|---|---|
| `const { name } = props` | Reads once, never updates | `props.name` |
| `{cond ? <A /> : <B />}` | Eager evaluation | `<Show>` / `<Switch>` |
| `{list.map(...)}` | No keyed diffing | `<For each={list}>` |
| `useEffect(...)` | Does not exist | `createEffect` |
| `const [x, setX] = createSignal(0); x` | Missing `()` — reads initial value | `x()` |
| `setTimeout(() => setCount(count()))` | Stale closure | `setCount(c => c + 1)` |
| `<For each={items().filter(...)}>` | Recomputes on every render | `<For each={filtered()}>` where `filtered = createMemo(() => items().filter(...))` |
