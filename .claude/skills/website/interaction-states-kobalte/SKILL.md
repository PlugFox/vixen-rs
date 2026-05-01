---
name: interaction-states-kobalte
description: Map Kobalte data-* attributes to Tailwind variants. hover/focus-visible/active/disabled with consistent visual diff. State precedence — disabled > active > hover > default.
---

# Interaction States with Kobalte + Tailwind (Vixen website)

**Source:** [Kobalte components](https://kobalte.dev/docs/core/overview/introduction), [Tailwind state variants](https://tailwindcss.com/docs/hover-focus-and-other-states).

**Read first:**

- [website/docs/rules/components.md](../../../../website/docs/rules/components.md).
- [website/docs/rules/styling.md](../../../../website/docs/rules/styling.md).

## Kobalte data-* attributes

Every primitive sets state via `data-*` attributes you can target with Tailwind:

| Attribute | Where |
| --- | --- |
| `data-[state=open\|closed]` | Dialog, Menu, Popover, Collapsible |
| `data-[state=on\|off]` | Toggle |
| `data-[disabled]` | Any disabled control |
| `data-[invalid]` / `data-[valid]` | Form fields |
| `data-[checked]` | Checkbox, Switch, RadioGroup item |
| `data-[highlighted]` | Menu items, Combobox options |
| `data-[expanded]` | Accordion, DropdownMenu |
| `data-[orientation=horizontal\|vertical]` | Tabs, Slider, Separator |
| `data-[selected]` | Tabs, Listbox |

## Tailwind variants on data-*

```tsx
class="data-[disabled]:opacity-50 data-[disabled]:pointer-events-none
       data-[state=open]:bg-accent
       data-[checked]:bg-primary data-[checked]:text-primary-foreground"
```

## Precedence (visual + CSS specificity)

1. **disabled** — overrides everything; muted, no events.
2. **active** (`data-[state=open]`, `:active`) — pressed/open; darker shade.
3. **hover** — slight tint.
4. **focus-visible** — ring on top of any state (a11y; never overridden).
5. **default**.

Order utility classes from least → most specific so the cascade resolves correctly.

## focus vs focus-visible

Always `focus-visible:` for the keyboard ring. `:focus` fires on mouse click and feels noisy. The pattern:

```tsx
"focus:outline-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
```

## Standard CVA button

```tsx
const button = cva(
  "inline-flex items-center justify-center rounded-md font-medium transition-colors " +
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 " +
    "disabled:opacity-50 disabled:pointer-events-none " +
    "data-[disabled]:opacity-50 data-[disabled]:pointer-events-none",
  {
    variants: {
      intent: {
        primary: "bg-primary text-primary-foreground hover:bg-primary/90 active:bg-primary/80",
        ghost:   "hover:bg-accent hover:text-accent-foreground active:bg-accent/80",
      },
    },
  },
);
```

Both `disabled:` (HTML attr) and `data-[disabled]:` (Kobalte attr) — covers native `<button disabled>` and Kobalte `<Button disabled>`.

## Loading state

Not a Kobalte primitive. Manage via a `data-loading` attribute you set yourself:

```tsx
<button data-loading={isLoading() ? "" : undefined}
        class="data-[loading]:cursor-progress data-[loading]:opacity-80">
  <Show when={isLoading()} fallback={<SaveIcon />}><Spinner /></Show>
  <span class="data-[loading]:opacity-50">{t("save")}</span>
</button>
```

## Test in both themes

Light + dark. States must be **distinguishable in both** — a `bg-primary/90` hover that's invisible in dark mode = bug. Verify on real backgrounds, not in isolation.

## Compound states

```tsx
"data-[disabled]:data-[checked]:bg-muted"        /* checked AND disabled */
"data-[state=open]:data-[orientation=vertical]:rounded-l-none"
```

## aria-* alternative

`aria-[expanded=true]:` works too — Kobalte sets both `data-*` and `aria-*` in sync. Prefer `data-*` for styling (semantic separation: ARIA for screen readers, data for styling).

## Gotchas

- Don't manage state with JS class swaps (`isOpen() ? 'class-a' : 'class-b'`) — let Kobalte's `data-*` drive Tailwind. JS swaps fight the cascade and fail on SSR/hydration.
- `:hover` on touch — sticky hover after tap. `hover:` styles must also work without hover (don't put critical info there).
- Forgetting `focus-visible:ring-offset-2` on dark backgrounds → ring blends in. Use ring-offset matching the parent bg.
- Removing `outline-none` without a replacement focus ring = a11y fail.

## Verification

Keyboard `Tab` through every screen — every focusable element shows a visible focus ring. Toggle theme — same statement holds in both.

## Related

- `tailwind-styling` — utility conventions.
- `add-solid-component` — CVA skeleton.
- `ui-accessibility` — focus, ARIA.
