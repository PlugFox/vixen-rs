# Styling Rules

Read this file before writing Tailwind classes or theme tokens.

## Tailwind v4 + CVA + `cn()`

The only styling pair in this codebase. No CSS modules. No `@apply` for component classes. No styled-components. No inline `style` attribute.

```tsx
import { cn } from "@/shared/lib/cn";
import { cva } from "class-variance-authority";

const button = cva("inline-flex items-center justify-center rounded-md font-medium ...", {
  variants: {
    intent: { primary: "bg-primary ...", danger: "bg-destructive ..." },
    size: { sm: "h-8 px-3 text-sm", md: "h-9 px-4 text-sm", lg: "h-10 px-5 text-base" },
  },
  defaultVariants: { intent: "primary", size: "md" },
});

export function Button(props) {
  const [local, rest] = splitProps(props, ["intent", "size", "class", "children"]);
  return <button class={cn(button({ intent: local.intent, size: local.size }), local.class)} {...rest}>{local.children}</button>;
}
```

## Decision: CVA vs inline utilities

- **Reusable component with >1 variant, >1 state, or shipped from `shared/ui/`** → CVA. Variants live in the component file.
- **One-off layout in a page or feature** → inline utilities. Don't extract prematurely.

## Design tokens

All colors, radii, spacing, and font stacks live in `@theme` in the global CSS:

```css
@theme {
  --color-background: oklch(1 0 0);
  --color-foreground: oklch(0.15 0 0);
  --color-primary: oklch(0.58 0.18 265);
  --color-primary-foreground: oklch(0.98 0 0);
  --color-destructive: oklch(0.55 0.22 25);
  --color-border: oklch(0.92 0 0);
  --color-muted: oklch(0.96 0 0);
  --color-muted-foreground: oklch(0.45 0 0);
  --radius: 0.5rem;
  --font-sans: "Inter", ui-sans-serif, system-ui, sans-serif;
}
```

- **Never hard-code hex in a component.** Use `bg-primary`, `text-foreground`, `border-border`. If a token is missing, add it to `@theme` — don't inline the value.
- **OKLCH over HSL/hex.** Better perceptual uniformity.
- **Semantic naming, not visual.** `--color-destructive`, not `--color-red`.

## Dark mode

CSS `prefers-color-scheme` via Tailwind v4's `@variant`:

```css
@variant dark {
  --color-background: oklch(0.13 0 0);
  --color-foreground: oklch(0.98 0 0);
  /* ... */
}
```

Browser mode honors `prefers-color-scheme` plus an optional `vixen_theme` localStorage override (`light` | `dark` | `system`). WebApp mode honors `Telegram.WebApp.colorScheme` (live signal, updates on theme change events).

The toggle in `data-kb-theme` on `<html>` drives both Tailwind variants and Kobalte's internal a11y queries.

## Utility conventions

- **Order in className** (consistency, not strict): layout → box model → typography → colors → state.
- **`cn()` for composition.** Always merge incoming `class` prop with `cn()` so callers can override.
- **Arbitrary values (`w-[347px]`, `text-[#abc]`) — avoid.** Code smell: either the token is missing (add it) or the design is off-grid (push back).
- **`!important` (`!w-full`) — forbidden.** If you need to override, restructure.
- **Responsive prefixes** (`sm:`, `md:`, `lg:`) only where responsive behaviour is intentional.

## States

- `hover:`, `focus-visible:`, `active:`, `disabled:`, `aria-[expanded=true]:`, `data-[state=open]:`.
- Prefer `focus-visible` over `focus` — keyboard-only focus ring, no ring on mouse click.
- Kobalte primitives expose `data-*` attributes for every state; target those instead of managing local JS state for styling.

## Spacing scale

- Stick to the default Tailwind scale: `p-0, p-0.5, p-1, p-1.5, p-2, p-2.5, p-3, p-4, p-5, p-6, p-8, p-10, p-12, p-16, p-20, p-24`.
- 4px and 8px grid. Anything off-grid (`p-[7px]`) is probably wrong.
- **Gap over margin** inside flex/grid containers.

## Typography

See `.claude/skills/website/typography-scale/SKILL.md`. Highlights:

- `text-sm` / 14px is the body text floor.
- Headings: `text-xl font-semibold tracking-tight` (dialog), `text-2xl` (page), `text-3xl` (public-report only).
- `tabular-nums` for any column of numbers.

## Telegram WebApp viewport

Inside Telegram WebApp the viewport can be very narrow (`< 360px` on small phones). Test mobile-first:

- No fixed widths above viewport.
- No horizontal scroll on the dashboard's primary screens.
- Touch targets ≥ 44×44px.

## Common mistakes

- Duplicating the same 8 Tailwind classes across three components → extract to CVA in `shared/ui/`.
- Using arbitrary values for colors instead of extending the theme.
- `class` prop typed as `string | undefined` but not merged via `cn()` → caller override silently lost.
- Mixing Tailwind with a sibling CSS file that uses hardcoded colors → tokens desync.
- Leaving `text-red-500` test classes in committed code → replace with `destructive`/`primary`/`success`/`warning`.
- `text-gray-400` body text → contrast fail, swap to `text-muted-foreground`.

## Related

- Skill: `.claude/skills/website/tailwind-styling/SKILL.md`
- Skill: `.claude/skills/website/design-anti-patterns/SKILL.md`
- Skill: `.claude/skills/website/typography-scale/SKILL.md`
- Skill: `.claude/skills/website/ui-accessibility/SKILL.md`
