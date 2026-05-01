---
name: responsive-breakpoints-telegram
description: Mobile-first responsive — Telegram WebApp viewport floor is ~320px. Base styles for 320, progressively enhance with sm:/md:/lg:. Container queries for component-level.
---

# Responsive Breakpoints — Telegram-aware (Vixen website)

**Source:** [Tailwind responsive design](https://tailwindcss.com/docs/responsive-design).

**Read first:**

- [website/docs/rules/styling.md](../../../../website/docs/rules/styling.md).
- [website/docs/architecture.md](../../../../website/docs/architecture.md).

## Vixen breakpoints

| Token | Min width | Use |
| --- | --- | --- |
| (base) | 320px | Telegram WebApp floor |
| `sm:` | 640px | Mobile landscape / small tablet |
| `md:` | 768px | Tablet |
| `lg:` | 1024px | Desktop |
| `xl:` | 1280px | Wide desktop (rare; public reports only) |

## Mobile-first

Write base styles for 320px; layer up.

```tsx
<div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
  <ChatCard />
  <ChatCard />
</div>
```

Don't write `lg:grid-cols-3` without a base — base = 1 column at 320px.

## No fixed widths above viewport

`w-[400px]` on a 320px viewport → horizontal scroll → scoring an instant UX fail in WebApp. Use `w-full max-w-md` or `w-full sm:w-96`.

## Container queries

Component-level responsiveness, independent of viewport:

```tsx
<aside class="@container">
  <div class="grid grid-cols-1 gap-2 @sm:grid-cols-2">
    <Stat label="..." value="..." />
  </div>
</aside>
```

Tailwind v4: `@container` opens the scope; `@sm:`, `@md:`, etc., target the **container's** width, not the viewport. Use this for sidebars, cards, tiles that re-flow at the component level.

## Telegram WebApp specifics

- `Telegram.WebApp.viewportHeight` is **dynamic** — changes when keyboard opens or user expands the modal. Subscribe to `viewportChanged` if you depend on it; pair with cleanup (see `solid-async-cleanup`).
- The top header is owned by Telegram. **Don't reserve space for our own header in WebApp mode.** Detect via `Telegram.WebApp?.initData` presence.
- Safe areas (notches) — Telegram client handles them. No manual `env(safe-area-inset-*)` inside the WebApp container.
- The container can be < 360px on small Android phones — design assumes 320px floor.

## Public report sizing

Targets 320 → 1024+. Charts: `width: 100%; height: auto` and SVG-native. Tables collapse to stacked rows below `sm`.

## Hide / show patterns

```tsx
<nav class="hidden md:block" />            {/* desktop-only */}
<button class="md:hidden" />               {/* mobile-only menu icon */}
```

Symmetric pair: anything hidden on small must have a small-screen equivalent (often a drawer or accordion).

## Min-height

Use `min-h-dvh` (Tailwind v4 dynamic viewport) over `min-h-screen` — `100vh` on iOS Safari includes browser chrome and overflows.

## Gotchas

- `lg:` alone for "desktop only" — without a base it crashes at < 1024. Always specify base.
- Mixing pixel media queries with Tailwind tokens = drift. All breakpoints via `sm:`/`md:`/`lg:`/`xl:` only.
- Forgetting WebApp keyboard — input at the bottom of the viewport is hidden by the keyboard if you anchored with `bottom-0` and didn't subscribe to `viewportChanged`.
- Testing only on desktop DevTools — TG WebApp on a real Android with Russian carrier latency surfaces issues you won't see locally. Use [https://web.telegram.org/k/](https://web.telegram.org/k/) or a real device.

## Verification

DevTools at 320px wide → no horizontal scroll on every primary screen (chats list, chat detail, settings, login, captcha, public report).

## Related

- `tailwind-styling` — base utility conventions.
- `design-tokens-system` — `--spacing-*` scale.
- `dashboard-vs-public-layout` — shell differences (when this skill exists).
