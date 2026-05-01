---
name: design-tokens-system
description: Govern OKLCH theme tokens — semantic naming, light+dark pairs, contrast validation. Add new tokens via @theme; never hard-code hex in components.
---

# Design Tokens System (Vixen website)

**Source:** [Tailwind v4 @theme](https://tailwindcss.com/docs/theme), [InclusiveColors](https://www.inclusivecolors.com/).

**Read first:**

- [website/docs/rules/styling.md](../../../../website/docs/rules/styling.md).
- [website/docs/ui-kit.md](../../../../website/docs/ui-kit.md).

## Token categories

| Category | Examples |
| --- | --- |
| Color | `--color-background`, `--color-foreground`, `--color-primary`, `--color-destructive`, `--color-success`, `--color-warning`, `--color-muted`, `--color-border`, `--color-ring` |
| Spacing | Default Tailwind scale; add custom only for vixen-specific (e.g., `--spacing-content` = max content width) |
| Radius | `--radius` base + `--radius-sm`, `--radius-lg` derived |
| Font | `--font-sans`, `--font-mono` |
| Shadow | `--shadow-card`, `--shadow-overlay` |

## Naming — semantic, not visual

- Good: `--color-destructive` (intent — "the thing that warns"). Survives a brand repaint.
- Bad: `--color-red-500` (visual — tied to one shade). Breaks the moment design shifts hue.

## OKLCH over HSL/hex

Better perceptual uniformity — the same lightness at different hues looks balanced. Easier to author light/dark pairs. WCAG contrast still measured in sRGB, so validate ratios with WebAIM (OKLCH tooling lags).

## Light + dark pairs

Every color token has both:

```css
@theme {
  --color-background: oklch(1 0 0);
  --color-foreground: oklch(0.15 0 0);
  --color-primary: oklch(0.58 0.18 265);
  --color-primary-foreground: oklch(0.98 0 0);
  --color-destructive: oklch(0.55 0.22 25);
  --color-success: oklch(0.65 0.15 150);
  --color-warning: oklch(0.75 0.16 80);
}

@variant dark {
  --color-background: oklch(0.13 0 0);
  --color-foreground: oklch(0.98 0 0);
  --color-primary: oklch(0.65 0.18 265);
  --color-destructive: oklch(0.62 0.22 25);
}
```

Skip a dark pair → token reverts to light value at runtime → ugly contrast in dark mode.

## Contrast (WCAG AA)

| Type | Min ratio |
| --- | --- |
| Body text (< 18.66px bold or < 24px) | 4.5:1 |
| Large text | 3:1 |
| UI components / focus indicators | 3:1 |

Validate every fg/bg pair you ship. Tools: WebAIM Contrast Checker, axe DevTools.

## Adding a new token

1. Add to `@theme` (light) **and** `@variant dark` (dark).
2. Validate contrast against neighbor tokens it'll pair with (e.g., new `--color-info` paired with `--color-info-foreground`).
3. Use it in **at least one shipped component**, or remove it.
4. Document the intent in [website/docs/ui-kit.md](../../../../website/docs/ui-kit.md).

## Vixen ledger surfaces

The moderation ledger needs status colors:

- `--color-success` — verified, unbanned.
- `--color-destructive` — banned.
- `--color-warning` — captcha pending.
- `--color-info` (or `--color-muted`) — neutral metadata (CAS check, message deleted).

## Anti-patterns

- `bg-[#ff0000]` — token missing or design off-grid. Add a token or push back.
- `bg-red-500` (raw Tailwind palette) — bypasses the semantic layer. Use `bg-destructive`.
- `--color-error` AND `--color-destructive` defined as the same value — pick one name and stick with it.
- Hard-coded `box-shadow: 0 1px 2px ...` — should be `--shadow-card`.

## Token sprawl

Review quarterly. If `grep -r 'bg-special' website/src` returns 0, drop `--color-special`. Unused tokens are ambient cost.

## Spacing — when to add custom

Default Tailwind 4px/8px scale is sufficient 95% of the time. Add a custom token only when:

- It's used > 3 times across the codebase, AND
- It expresses a domain meaning (`--spacing-content-max`, `--spacing-sidebar`).

Else use `w-72` etc. directly.

## Gotchas

- OKLCH chroma > 0.4 + low lightness → out of sRGB gamut → unpredictable rendering. Keep chroma sane (~ 0.1–0.25).
- `--color-foreground` defined in `:root` instead of `@theme` → Tailwind doesn't pick it up for `text-foreground`. Always inside `@theme`.
- Forgetting that `bg-primary/90` is opacity, not a separate token. If you want a real hover shade, add `--color-primary-hover`.

## Verification

Visual check at light **and** dark. WebAIM Contrast Checker on every fg/bg pair. `grep -r 'bg-\[#\|text-\[#' website/src` returns 0.

## Related

- `tailwind-styling` — utility usage.
- `dark-mode-discipline` — light/dark parity (when this skill exists).
- `ui-accessibility` — contrast and focus tokens.
