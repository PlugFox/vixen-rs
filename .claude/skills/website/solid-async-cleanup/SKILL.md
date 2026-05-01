---
name: solid-async-cleanup
description: Register onCleanup for intervals, event listeners, fetch aborts inside SolidJS components. Telegram WebApp BackButton/theme handlers must always clean up.
---

# SolidJS onCleanup (Vixen website)

**Source:** [SolidJS onCleanup](https://docs.solidjs.com/reference/lifecycle/oncleanup).

**Read first:**

- [website/docs/rules/solidjs.md](../../../../website/docs/rules/solidjs.md).
- [website/docs/auth.md](../../../../website/docs/auth.md).

## Pattern

`onCleanup` must be called inside a reactive scope: `onMount`, `createEffect`, `createRoot`, or the component setup body.

## Intervals / timeouts

```tsx
onMount(() => {
  const id = setInterval(refresh, 5000);
  onCleanup(() => clearInterval(id));
});
```

## Event listeners

```tsx
onMount(() => {
  const handler = () => updateTheme();
  Telegram.WebApp?.onEvent("themeChanged", handler);
  onCleanup(() => Telegram.WebApp?.offEvent("themeChanged", handler));
});
```

## AbortController for fetch

```tsx
onMount(() => {
  const ctrl = new AbortController();
  fetch(url, { signal: ctrl.signal }).then(...).catch(ignoreAbort);
  onCleanup(() => ctrl.abort());
});
```

For data fetching, prefer `createResource` (auto-aborts) — see `solid-resource-pattern`.

## Telegram WebApp specifics

Pair every `onEvent` / `onClick` registration with its inverse:

- `BackButton.onClick(cb)` → `BackButton.offClick(cb)`.
- `MainButton.onClick(cb)` → `MainButton.offClick(cb)`.
- `WebApp.onEvent("viewportChanged", cb)` → `WebApp.offEvent("viewportChanged", cb)`.
- `WebApp.onEvent("themeChanged", cb)` → `WebApp.offEvent("themeChanged", cb)`.

Skip cleanup → handlers stack and fire N times after each navigation.

## Kobalte primitives

Kobalte components handle their own cleanup. You only register `onCleanup` for **custom** refs/listeners you added (e.g., a `ResizeObserver` on a Kobalte trigger ref).

## createResource auto-cleanup

Pending `createResource` requests abort automatically on disposal — do **not** wrap them in your own `AbortController`.

## Where cleanup runs

- Inside `onMount(() => { onCleanup(...) })` — runs **once**, when the component unmounts.
- Inside `createEffect(() => { onCleanup(...) })` — runs **on every re-execution** of the effect, plus on unmount. Use this for cleanup tied to a changing dependency.

## Async gotcha

```tsx
onMount(async () => {
  const ctrl = new AbortController();
  onCleanup(() => ctrl.abort());                // attaches to onMount scope — OK
  await fetch(url, { signal: ctrl.signal });
  onCleanup(() => doSomething());               // STILL attaches to onMount scope, NOT to "after await"
});
```

`onCleanup` always attaches to the **surrounding reactive scope at call time**, regardless of `await`. If you need cleanup tied to async state, use `createResource` or a `createEffect`.

## Gotchas

- `onCleanup` outside reactive scope (e.g., inside an event handler that fires post-mount) is a **no-op**. Move it into `onMount` / `createEffect`.
- Don't mix React `useEffect`-style cleanup signatures (`return () => ...`). Solid uses explicit `onCleanup(fn)`.
- A `setTimeout` started after unmount still fires — guard with a captured `disposed` flag if needed.

## Verification

Navigate away from a screen with intervals/listeners, then back. DevTools → Performance / Event Listeners: no duplicates, no orphan timers.

## Related

- `solid-resource-pattern` — auto-cleanup for fetches.
- `add-solid-component` — component skeleton with onMount/onCleanup.
