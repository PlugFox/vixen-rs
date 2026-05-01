# UI Component Rules

Read this file before creating or modifying UI components in `src/shared/ui/`.

## Decision: Kobalte vs Plain HTML

| Need | Use |
|---|---|
| Accessible modal with focus trap | Kobalte Dialog |
| Dropdown with keyboard navigation | Kobalte DropdownMenu |
| Custom select with search | Kobalte Combobox |
| Toast notifications | Kobalte Toast |
| Tooltip with delay/positioning | Kobalte Tooltip |
| Toggle / checkbox / switch | Kobalte Switch or Checkbox |
| Tabs with keyboard navigation | Kobalte Tabs |
| Simple button / card / badge | Plain HTML + Tailwind + CVA |
| Text input / textarea | Plain HTML + Tailwind |

Rule: if the component needs a11y patterns you'd have to implement manually (focus trap, ARIA roles, keyboard navigation), use Kobalte. Otherwise plain HTML.

## Creating a Simple Component

Pattern: CVA variants + Tailwind classes + `cn()` for overrides.

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

## Creating a Kobalte Component

Visual style: shadcn-like — minimal, clean, consistent with shadcn design tokens and patterns.

Kobalte component reference and styling guide: https://kobalte.dev/docs/core/overview/styling.

Pattern: thin wrapper that adds Tailwind styling to Kobalte primitives.

```tsx
import { Dialog as KDialog } from "@kobalte/core/dialog";   // specific submodule!
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

Key rules:

- Import from `@kobalte/core/dialog`, NOT `@kobalte/core` (tree-shaking).
- Kobalte handles a11y (ARIA, focus trap, keyboard) — don't re-implement.
- Re-export sub-components as needed.

## Mandatory Rules

1. **Accept `class` prop** — every component must accept optional `class` merged via `cn()`.
2. **Use `splitProps`** — separate component-specific props from HTML attributes, spread rest onto root element.
3. **Export variants** — `export { badgeVariants }` so styles can be reused on other elements.
4. **No business logic** — UI components are stateless and presentational. No API calls, no feature-specific imports.
5. **No `style` attribute** — all styling through Tailwind classes.
6. **No margin on root element** — spacing is the consumer's responsibility.
7. **Use theme tokens** — `bg-primary`, `text-foreground`, `border-border`. Never hardcode hex colors.
8. **One file per component** — `shared/ui/button.tsx`. No barrel `index.ts` files.
9. **Target < 150 lines** — split compound components if larger.

## Telegram-mode awareness

UI primitives stay neutral. WebApp-vs-browser detection happens at the **layout** level (`app/webapp-layout.tsx` vs `app/root-layout.tsx`), not inside primitives. A `Button` is a Button regardless of the host.

The one exception: `Dialog` may swap to a full-screen sheet inside very narrow viewports (typical of WebApp on phone). Implement via responsive Tailwind classes (`sm:max-w-lg`, full-width on mobile), not by inspecting the WebApp object.

## Checklist

- [ ] Decided Kobalte vs plain HTML.
- [ ] `class` prop accepted and merged via `cn()`.
- [ ] `splitProps` used for prop separation.
- [ ] Variants exported separately.
- [ ] No hardcoded colors — uses theme tokens.
- [ ] No business logic in the component.
- [ ] Works in both light and dark themes.
- [ ] Works at WebApp viewport widths (≤ 360px) without horizontal scroll.
