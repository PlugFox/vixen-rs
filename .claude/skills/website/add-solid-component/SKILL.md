---
name: add-solid-component
description: Create or modify a SolidJS component in the vixen-rs website following project rules — no prop destructuring, no useEffect, proper control flow, CVA variants, Kobalte primitives. Use when the user asks to add, create, or modify a component, dialog, button, form, or any .tsx file under website/src/.
---

# Add SolidJS Component (Vixen website)

**Read first**:

- [website/docs/rules/solidjs.md](../../../../website/docs/rules/solidjs.md) — reactivity rules.
- [website/docs/rules/components.md](../../../../website/docs/rules/components.md) — UI primitives and variants.
- [website/docs/rules/typescript.md](../../../../website/docs/rules/typescript.md) — TS conventions.
- [website/docs/ui-kit.md](../../../../website/docs/ui-kit.md) — existing primitives you can reuse.

Check [website/src/shared/ui/](../../../../website/src/shared/ui/) first — the component you need may already exist.

## Hard rules (from website/CLAUDE.md)

- **Never destructure props** — breaks reactivity. Use `props.field` or `splitProps(props, [...])`.
- **Control flow**: `<Show>`, `<For>`, `<Switch>/<Match>`. Never ternaries-with-JSX or `array.map()` in the template.
- **No `useEffect`** — use `createEffect`, `onMount`, `onCleanup`. React idioms don't apply.
- **No hardcoded user-facing text** — every string through i18n (`t()`, `tp()`, `<T>`). See `add-i18n-string` skill.
- **File naming**: kebab-case (`chat-card.tsx`, not `ChatCard.tsx`).
- **Named exports only**. `export default` is reserved for `pages/` (lazy-loaded routes).
- **No `any`** — use `unknown` + narrowing. **No TS enums** — use `as const` objects.

## Skeleton

```tsx
import { splitProps, type ComponentProps } from "solid-js";
import { Show } from "solid-js";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "~/shared/lib/cn";

const styles = cva("base-classes", {
  variants: {
    size: { sm: "text-sm", md: "text-base", lg: "text-lg" },
    tone: { neutral: "...", danger: "..." },
  },
  defaultVariants: { size: "md", tone: "neutral" },
});

type Props = ComponentProps<"div"> &
  VariantProps<typeof styles> & {
    title: string;
  };

export function ChatCard(props: Props) {
  const [local, rest] = splitProps(props, ["title", "size", "tone", "class", "children"]);
  return (
    <div class={cn(styles({ size: local.size, tone: local.tone }), local.class)} {...rest}>
      <h3>{local.title}</h3>
      <Show when={local.children}>{local.children}</Show>
    </div>
  );
}
```

## Prefer primitives

- **Interactive UI** (dialog, menu, tooltip, select) → Kobalte primitives wrapped in `shared/ui/`.
- **Icons** → use a lucide-style set via `shared/ui/icon.tsx`.
- **Forms** → follow the validation pattern in existing feature forms rather than inventing a new one.

## Refs and lifecycle

- Refs: `let inputRef!: HTMLInputElement;` then `<input ref={inputRef} />`. Access only inside `onMount` or event handlers — outside the reactive scope the value is `undefined`.
- Components that attach DOM listeners (resize, theme change, Telegram WebApp `BackButton.onClick`) MUST register `onCleanup` to remove them. Not cleaning up = duplicate handlers after navigation.

## Placement

- Feature-specific → `website/src/features/{area}/components/{name}.tsx`.
- Shared / reusable → `website/src/shared/ui/{name}.tsx`.
- Route entry → `website/src/pages/{name}.tsx` with `export default` for `lazy()`.

## After writing

1. `/website-check` — Biome + typecheck + build must pass.
2. If the component has behavior (not just visual), exercise it manually in the browser. Playwright MCP is not yet wired into this repo.
