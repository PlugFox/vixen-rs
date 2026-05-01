---
name: typography-scale
description: Apply vixen-rs fixed type scale for app UI — discrete sizes in rem, line-heights tied to size, no clamp()/fluid typography inside product surfaces, tight tracking for headings, loose for body. Use when adding text, headings, or when a designer suggests "responsive typography" inside the product.
---

# Typography Scale (Vixen website)

**Source:** [impeccable.style](https://impeccable.style/) `/typeset` (fixed scale for app UI, fluid only for marketing) + [Tailwind CSS v4 font-size scale](https://tailwindcss.com/docs/font-size).

## Core rule

**Vixen's website is an app, not a marketing site.** App UI uses a **fixed** type scale — text size does not interpolate with viewport width. Fluid typography (`clamp(1rem, 2vw, 1.5rem)`) belongs on landing pages where line-breaks are part of the composition; inside the product, the IDE-like feel is broken by text that grows on 4K monitors.

## The scale

| Token | Size | Line-height | Tracking | Weight | Use for |
|---|---|---|---|---|---|
| `text-xs` | 0.75rem / 12px | 1rem | 0.01em | 500 | Labels, badges, tooltip body |
| `text-sm` | 0.875rem / 14px | 1.25rem | 0 | 400–500 | Body, table cells, menu items |
| `text-base` | 1rem / 16px | 1.5rem | 0 | 400 | Default body, long copy |
| `text-lg` | 1.125rem / 18px | 1.5rem | -0.005em | 500 | Subtle section header |
| `text-xl` | 1.25rem / 20px | 1.75rem | -0.01em | 600 | Dialog title, card title |
| `text-2xl` | 1.5rem / 24px | 2rem | -0.015em | 600 | Page header |
| `text-3xl` | 1.875rem / 30px | 2.25rem | -0.02em | 700 | Public report headline only |

Anything larger than `text-3xl` inside product UI is almost certainly wrong.

## Rules

- **Body text floor: 14px (`text-sm`).** Never smaller for content a user needs to read. 12px is for labels/badges only.
- **Line-height scales with size.** Small text → tighter (1.4–1.5×). Large headings → tighter still (1.1–1.2×). Match the table.
- **Tracking negative on headings, zero/slight positive on small caps.** Never wider than 0.02em on body text.
- **One primary typeface.** A second face only for a deliberate purpose (e.g. monospace for chat IDs, code).
- **Weight steps ≥ 200.** Jumping 400 → 500 on the same size is indistinguishable. Use 400 → 600 or 500 → 700.
- **No `clamp()` / fluid typography in `website/src/features/**` and `website/src/pages/**`.** Use fixed Tailwind classes.
- **Numerals.** For tables or anywhere numbers align (moderation action counts, captcha attempts), use `tabular-nums`.

## Patterns

```tsx
// Dialog title
<h2 class="text-xl font-semibold tracking-tight">Manual ban</h2>

// Table cell with numbers (action ledger)
<td class="text-sm tabular-nums text-right">{actionCount}</td>

// Label above input
<label class="text-xs font-medium uppercase tracking-wide text-muted-foreground">
  Spam threshold
</label>

// Body paragraph
<p class="text-base leading-relaxed text-foreground">...</p>
```

## Anti-patterns (grep these)

- `text-(xs|sm)` with `font-thin` / `font-light` — falls apart on non-retina displays.
- `text-gray-400` for body text — contrast fail.
- `clamp(` inside `src/features/` or `src/pages/` — remove.
- `tracking-widest` on body — unreadable.
- Mixed `font-family` inside a single component without intent — collapse to one.

## i18n note

RU strings are on average ~30% longer than EN. Don't choose sizes/line-heights that barely fit the English string — they will break on translation. Test both locales for any new text surface. See `add-i18n-string` skill.

## Related

- `design-anti-patterns` — Inter-everywhere trap.
- `tailwind-styling` — tokens and CVA variants.
