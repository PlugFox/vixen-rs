---
name: tailwind-styling
description: Apply Tailwind v4 correctly in the vixen-rs website — use design tokens via CSS custom properties, pick CVA for variant-heavy components vs inline utilities for one-offs, avoid !important and arbitrary values, respect the @theme contract. Use when writing a component's class, adding a new color/spacing token, or reviewing Tailwind diffs.
---

# Tailwind Styling (Vixen website)

**Source:** [Tailwind CSS v4 docs](https://tailwindcss.com/docs/theme), [class-variance-authority docs](https://cva.style/docs), and impeccable.style guidance.

## Decision: CVA vs inline utilities

- **Reusable component with >1 variant, >1 state, or shipped from `shared/ui/`** → CVA. Variants live in the component file.
- **One-off layout in a page or feature** → inline utilities. Don't extract prematurely.
- **Never** a third option: don't invent a local "styles" helper, don't `@apply` in a CSS file for component classes. Tailwind v4 + CVA is the only pair.

```tsx
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "~/shared/lib/cn";

const button = cva(
  "inline-flex items-center justify-center rounded-md font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 disabled:pointer-events-none",
  {
    variants: {
      intent: {
        primary: "bg-primary text-primary-foreground hover:bg-primary/90",
        secondary: "bg-secondary text-secondary-foreground hover:bg-secondary/80",
        danger: "bg-destructive text-destructive-foreground hover:bg-destructive/90",
      },
      size: { sm: "h-8 px-3 text-sm", md: "h-9 px-4 text-sm", lg: "h-10 px-5 text-base" },
    },
    defaultVariants: { intent: "primary", size: "md" },
  },
);
```

## Design tokens — single source of truth

All colors, radii, spacing, and font stacks live in `@theme` in the global CSS:

```css
@theme {
  --color-primary: oklch(0.58 0.18 265);
  --color-primary-foreground: oklch(0.98 0 0);
  --color-border: oklch(0.92 0 0);
  --radius: 0.5rem;
  --font-sans: "Inter", ui-sans-serif, system-ui, ...;
}
```

- **Never hard-code hex in a component.** Use `bg-primary`, `text-foreground`, `border-border`. If the token is missing, add it to `@theme` — don't inline the value.
- **OKLCH over HSL / hex.** Better perceptual uniformity.
- **Semantic naming, not visual.** `--color-destructive`, not `--color-red`.

## Utility conventions

- **Order in className** (aim for consistency, not strict): layout → box model → typography → colors → state.
- **`cn()` for composition.** Always merge incoming `class` prop with `cn()` so callers can override.
- **Arbitrary values (`w-[347px]`, `text-[#abc]`) — avoid.** They are a code smell: either the token is missing (add it) or the design is off-grid (push back).
- **`!important` (`!w-full`) — forbidden.** If you need to override, restructure.
- **Responsive prefixes** (`sm:`, `md:`, `lg:`) only where responsive behaviour is intentional.
- **Telegram WebApp viewport.** When inside the Telegram WebApp container, the viewport can be very narrow (mobile webview). Test with `mobile` breakpoint = the floor of layout decisions; the dashboard doesn't need a desktop-first layout.
- **Dark mode.** Vixen targets CSS `prefers-color-scheme` via Tailwind v4's `@variant`.

## States — use the pseudo-classes, not JS

- `hover:`, `focus-visible:`, `active:`, `disabled:`, `aria-[expanded=true]:`, `data-[state=open]:`.
- Prefer `focus-visible` over `focus` — keyboard-only focus ring, no ring on mouse click.
- Kobalte primitives expose `data-*` attributes for every state; target those instead of managing local JS state for styling.

## Spacing scale

- Stick to the default Tailwind scale: `p-0, p-0.5, p-1, p-1.5, p-2, p-2.5, p-3, p-4, p-5, p-6, p-8, p-10, p-12, p-16, p-20, p-24`.
- **4px and 8px grid.** Anything off-grid (`p-[7px]`) is probably wrong.
- **Gap over margin** inside flex/grid containers.

## Common mistakes

- Duplicating the same 8 Tailwind classes across three components — extract to CVA in `shared/ui/`.
- Using arbitrary values for colors instead of extending the theme.
- `class` prop typed as `string | undefined` but not merged via `cn()` — caller override silently lost.
- Mixing Tailwind with a sibling CSS file that uses hardcoded colors — tokens desync.
- Leaving long-dead `text-red-500` test classes in committed code — replace with `destructive`/`primary`/`success`/`warning`.

## Related

- `design-anti-patterns` — gradients, cards, contrast.
- `typography-scale` — the type scale itself.
- `ui-accessibility` — `focus-visible`, ARIA states as `data-*` variants.
- `add-solid-component` — the CVA skeleton for new components.
