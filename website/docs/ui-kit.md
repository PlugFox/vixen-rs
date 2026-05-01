# UI Kit

Custom UI primitives in `src/shared/ui/`. Pattern: CVA variants + Tailwind + `cn()` for overrides. Kobalte for primitives needing accessibility patterns (Dialog, Menu, Select, Tooltip, Toast); plain HTML+CVA for simple ones (Button, Card, Badge, Input).

## Inventory

Initial set (M4):

- `button.tsx` — primary / secondary / destructive / ghost variants; sm / md / lg sizes.
- `card.tsx` — root + header + body + footer subcomponents.
- `badge.tsx` — neutral / success / warning / destructive / outline variants.
- `input.tsx` — text / email / password / number; error state via `data-invalid`.
- `label.tsx` — `<label>` with consistent typography.
- `dialog.tsx` — Kobalte wrapper; portal + overlay + content.
- `dropdown-menu.tsx` — Kobalte wrapper.
- `tooltip.tsx` — Kobalte wrapper.
- `toast.tsx` — Kobalte wrapper; managed by a global toast store.
- `switch.tsx` — Kobalte wrapper; controlled.
- `tabs.tsx` — Kobalte wrapper.
- `skeleton.tsx` — placeholder block for loading states.
- `empty-state.tsx` — icon + headline + description + optional CTA.
- `error-state.tsx` — error icon + message + retry button.
- `data-table.tsx` — minimal; sticky header, cursor pagination support.

Additional primitives are added only when used by 2+ features. One-off UI lives in the feature folder.

## Decision: Kobalte vs plain HTML

| Need | Use |
|---|---|
| Accessible modal with focus trap | Kobalte Dialog |
| Dropdown with keyboard navigation | Kobalte DropdownMenu |
| Custom select with search | Kobalte Combobox |
| Toast notifications | Kobalte Toast |
| Tooltip with delay/positioning | Kobalte Tooltip |
| Toggle / checkbox / switch | Kobalte Switch / Checkbox |
| Tabs with keyboard navigation | Kobalte Tabs |
| Simple button / card / badge | Plain HTML + Tailwind + CVA |
| Text input / textarea | Plain HTML + Tailwind |

Rule: if the component needs a11y patterns you'd have to implement manually (focus trap, ARIA roles, keyboard navigation), use Kobalte. Otherwise plain HTML.

## Plain primitive — pattern

```tsx
import { type VariantProps, cva } from "class-variance-authority";
import type { JSX } from "solid-js";
import { splitProps } from "solid-js";
import { cn } from "@/shared/lib/cn";

const badgeVariants = cva(
  "inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground",
        secondary: "bg-secondary text-secondary-foreground",
        destructive: "bg-destructive text-destructive-foreground",
        outline: "border border-border text-foreground",
      },
    },
    defaultVariants: { variant: "default" },
  }
);

interface BadgeProps
  extends JSX.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge(props: BadgeProps) {
  const [local, rest] = splitProps(props, ["variant", "class", "children"]);
  return (
    <span class={cn(badgeVariants({ variant: local.variant }), local.class)} {...rest}>
      {local.children}
    </span>
  );
}

export { badgeVariants };
```

## Kobalte primitive — pattern (shadcn-style wrapper)

```tsx
import { Dialog as KDialog } from "@kobalte/core/dialog";   // submodule! not "@kobalte/core"
import type { ParentProps } from "solid-js";
import { cn } from "@/shared/lib/cn";

export function Dialog(props: ParentProps<{ open: boolean; onOpenChange: (v: boolean) => void }>) {
  return (
    <KDialog open={props.open} onOpenChange={props.onOpenChange}>
      {props.children}
    </KDialog>
  );
}

export const DialogTrigger = KDialog.Trigger;

export function DialogContent(props: ParentProps<{ class?: string }>) {
  return (
    <KDialog.Portal>
      <KDialog.Overlay class="fixed inset-0 z-50 bg-black/80" />
      <KDialog.Content
        class={cn(
          "fixed left-1/2 top-1/2 z-50 w-full max-w-lg -translate-x-1/2 -translate-y-1/2 rounded-lg border bg-background p-6 shadow-lg",
          props.class
        )}
      >
        {props.children}
      </KDialog.Content>
    </KDialog.Portal>
  );
}
```

Key points:

- Import from `@kobalte/core/dialog` (submodule), not `@kobalte/core` — tree-shaking.
- Kobalte handles a11y (ARIA, focus trap, keyboard) — don't re-implement.
- Re-export sub-components as needed.

## Mandatory rules

1. **Accept `class` prop** — every component accepts optional `class` merged via `cn()`.
2. **Use `splitProps`** — separate component-specific props from HTML attributes, spread rest onto root.
3. **Export variants** — `export { badgeVariants }` so styles can be reused on other elements.
4. **No business logic** — UI components are stateless and presentational. No API calls, no feature-specific imports.
5. **No `style` attribute** — all styling through Tailwind classes.
6. **No margin on root element** — spacing is the consumer's responsibility.
7. **Use theme tokens** — `bg-primary`, `text-foreground`, `border-border`. Never hardcode hex colors.
8. **One file per component** — `shared/ui/button.tsx`. No barrel `index.ts` files.
9. **Target < 150 lines** — split compound components if larger.

See `docs/rules/components.md` for the rule rationale.

## Theme tokens

Defined in the global CSS via `@theme` (Tailwind v4):

```css
@theme {
  --color-background: oklch(1 0 0);
  --color-foreground: oklch(0.15 0 0);
  --color-primary: oklch(0.58 0.18 265);
  --color-primary-foreground: oklch(0.98 0 0);
  --color-secondary: oklch(0.95 0 0);
  --color-secondary-foreground: oklch(0.15 0 0);
  --color-destructive: oklch(0.55 0.22 25);
  --color-destructive-foreground: oklch(0.98 0 0);
  --color-success: oklch(0.55 0.18 145);
  --color-warning: oklch(0.7 0.18 80);
  --color-border: oklch(0.92 0 0);
  --color-muted: oklch(0.96 0 0);
  --color-muted-foreground: oklch(0.45 0 0);
  --color-ring: oklch(0.58 0.18 265 / 0.4);
  --radius: 0.5rem;
  --font-sans: "Inter", ui-sans-serif, system-ui, -apple-system, sans-serif;
}
```

Dark theme overrides (`@variant dark`):

```css
@variant dark {
  --color-background: oklch(0.13 0 0);
  --color-foreground: oklch(0.98 0 0);
  /* ... */
}
```

In WebApp mode, the theme is also influenced by `Telegram.WebApp.themeParams` — a small adapter overrides the relevant tokens at runtime (see `app/webapp-layout.tsx`).

## When to add a new primitive

- Used by 2+ features → add to `shared/ui/`.
- Used by 1 feature → keep in `features/{name}/components/`.
- Used by 1 feature, but expected to be used elsewhere → still keep in the feature; promote later.

Don't pre-extract for hypothetical reuse.

## Showcase

A planned UI Kit showcase under `src/ui-kit/` (separate Vite entry point) renders every primitive with all variants, states, and themes. Useful as a visual regression baseline. Not in M0 — added when the primitive set stabilizes.
