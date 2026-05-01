---
name: design-anti-patterns
description: Avoid the default "AI-generated" look when designing or reviewing vixen-rs UI — no purple/blue gradients, no nested cards, no Inter-everywhere, no low-contrast grey-on-grey, no emoji-only states, no fluid clamp() in product UI. Use when creating a new screen, styling a component, or reviewing a UI diff — before writing Tailwind classes, enumerate the reflex defaults you will NOT use.
---

# Design Anti-Patterns (Vixen website)

**Source:** adapted from [impeccable.style](https://impeccable.style/) ("Gallery of Shame" + anti-attractor procedure by Paul Bakaus).

## The procedure

Before you write a single class, **enumerate the 6 reflex defaults you will not use.** LLMs (and most web templates) snap to the same narrow aesthetic. Naming the trap avoids it.

## The 6 traps

1. **Purple/blue AI gradients.** `bg-gradient-to-br from-purple-500 to-blue-500`, "radial gradient at 30% 30%" hero blobs, gradient-on-text for headlines. This is the single most recognisable "AI slop" signal. Use a flat brand color or a very subtle single-hue gradient (same hue, two adjacent lightness stops) — never a two-hue rainbow.
2. **Inter Everywhere.** Inter is fine; Inter as the only answer is lazy. Pick a typography pairing intentionally.
3. **Cardocalypse.** Every block is a `rounded-2xl shadow-lg border bg-white p-6` card. Cards inside cards inside cards. Rule: **never nest a card inside a card.** If you feel you need to, you need a section, a divider, or just whitespace.
4. **Template layouts.** Hero + 3-column features + stat strip + testimonial carousel + CTA. This is a marketing-site cliché. Vixen's website is split between an admin dashboard (density + focus) and a public report page (single content column, no marketing fluff). Neither is a landing page.
5. **Low contrast.** `text-gray-400 on bg-gray-50`, placeholder-colored body text, thin 200-weight fonts at 13px. Body text must meet WCAG AA (≥ 4.5:1). Don't prove it by eye — check with a contrast tool (see `ui-accessibility` skill).
6. **Emoji / icon-only states.** An empty state that is just 🎉 with no copy, or a 64px icon with no explanation. Every empty / error / loading state needs words — see `ui-critique` skill. Vixen specifically: don't drop a 🤖 / 🚫 / 🔒 anywhere as the sole signal.

## Deterministic checks (run before committing UI)

Grep the diff for these patterns and investigate each hit:

- `from-\w+-\d+\s+to-\w+-\d+` — multi-hue gradient. Verify hues match or drop to a flat fill.
- `text-gray-400`, `text-gray-300`, `text-neutral-400` on non-dark backgrounds — likely too low contrast.
- Nested `rounded-(xl|2xl|3xl)` with `shadow-` on both parent and child — cardocalypse.
- `clamp\(` in anything under `src/` that isn't a marketing page — see `typography-scale` skill, fixed scale for app UI.
- `bg-clip-text` + gradient on text — AI-gradient-text smell. Use a solid color.
- Emoji-only children of `<EmptyState>` / error blocks — add copy.
- Pure black `oklch(0 0 0)` background in dark mode — causes halation on OLED screens. Use a near-black like `oklch(0.13 0 0)`.
- Clickable element without `cursor: pointer` (or `class="cursor-pointer"` on non-`<button>`/`<a>` elements) — affordance failure.
- Animations slower than 300ms or faster than 100ms — too sluggish or too jarring. Vixen targets 150–250ms for state transitions.

## What to do instead

- **Color:** one brand accent, a neutral ramp, semantic (success/warn/danger/info). Document hex values in `website/DESIGN.md` (when it lands). Never introduce a new hue inline in a component.
- **Elevation:** prefer a single 1px border + bg-contrast surface over stacked shadows. Reserve shadows for floating layers (popover, dialog, toast).
- **Whitespace > dividers.** If two blocks need to feel separate, increase the gap before you add a `border-t`.
- **Icons:** use the project's icon set. Never drop an emoji into production UI.

## When reviewing someone else's UI

Ask, in order:
1. Is there a gradient? If yes, is it single-hue?
2. Are any cards nested? If yes, why?
3. What's the contrast ratio of the smallest text? (≥ 4.5:1.)
4. Are there empty/error/loading states, and do they have copy?
5. Does the screen look like every other AI-generated SaaS dashboard? If yes, reject.

## Related

- `ui-critique` — Nielsen heuristics + state coverage review.
- `typography-scale` — fixed scale for app UI.
- `tailwind-styling` — tokens and CVA patterns.
