---
name: ui-accessibility
description: Ship accessible UI in the vixen-rs website — semantic HTML, correct ARIA, keyboard-only flows, focus management in Kobalte dialogs, WCAG AA contrast, reduced-motion support. Use when adding an interactive component, a dialog/menu/popover, a custom control, or reviewing a UI change for a11y.
---

# UI Accessibility (Vixen website)

**Source:** [WCAG 2.2 (W3C)](https://www.w3.org/TR/WCAG22/), [Kobalte accessibility docs](https://kobalte.dev/docs/core/overview/introduction), and the [WAI-ARIA Authoring Practices](https://www.w3.org/WAI/ARIA/apg/patterns/).

## Principle: semantic HTML first, ARIA second

If a native element does the job, use it. `<button>` not `<div onClick>`. `<a href>` not `<span role="link">`. ARIA is a patch for when semantics aren't enough — not a default.

## Hard rules

- **Every interactive element is a real button / link / input.** Click-bound divs are banned.
- **Every form field has a label.** Either `<label for>` + `<input id>` or a `<Label>` from Kobalte. A `placeholder` is not a label.
- **Every image has `alt`.** Decorative → `alt=""`. Functional (icon button) → descriptive text.
- **Focus ring is visible.** `focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2`. Never `outline: none` without a replacement.
- **Contrast ≥ 4.5:1 for body text, ≥ 3:1 for large (≥ 18.66px bold or 24px).** Verify with a contrast tool, not your eye.
- **Touch targets ≥ 44×44px** on mobile interactive elements (Kobalte defaults usually meet this; custom icon buttons often don't). Telegram WebApp container is mobile-by-default — this matters.
- Touch targets ≥ 44×44 px with ≥ 10 px spacing between adjacent targets (matters most in Telegram WebApp's narrow viewport). Icon-only buttons need padding to inflate the visual size.
- Form inputs: `aria-invalid="true"` on the input PLUS `aria-describedby="<errorId>"` linking to the visible error message. Render the error element always (toggled `hidden` if empty) so the id is stable across renders.
- Icon-only buttons need a non-visual label: `<button aria-label="Delete"><TrashIcon aria-hidden="true" /></button>` OR a sibling `<span class="sr-only">Delete</span>`.
- A skip-to-main link as the first focusable element on every page; use semantic landmarks (`<main>`, `<nav>`, `<aside>`) for region navigation.
- Dark mode contrast must meet WCAG AA in BOTH themes — don't assume "dark = automatically accessible".

## Keyboard

Every flow must be completable with keyboard only:

- `Tab` / `Shift+Tab` — move focus in visual order.
- `Enter` / `Space` — activate focused button.
- `Escape` — close dialogs, menus, comboboxes (Kobalte does this; don't disable it).
- Arrow keys inside listboxes, menus, radio groups, tabs.
- **Focus trap** inside modal dialogs (Kobalte `Dialog` handles it).
- **Focus return** — when a dialog closes, focus returns to the trigger.

Test: unplug the mouse and complete the flow. If you can't, it's broken.

## ARIA patterns (use Kobalte first)

Kobalte ships correct ARIA for: Dialog, Popover, Menu, Combobox, Listbox, RadioGroup, Tabs, Tooltip, Toast, Select, Switch, Slider, Separator, Toggle, Collapsible, Accordion, Breadcrumbs. **Use these.** Rolling your own `role="menu"` is a bug.

If you must add ARIA manually:

- `aria-label` for icon-only buttons: `<button aria-label={t("ban-user")}>`.
- `aria-live="polite"` for non-critical async updates (toast container).
- `aria-live="assertive"` only for critical errors.
- `aria-expanded`, `aria-controls`, `aria-haspopup` for disclosure patterns.
- `aria-hidden="true"` on decorative icons inside labelled buttons (else the label is read twice).
- Never `tabindex` > 0.

## Screen-reader-only text

```tsx
<button>
  <BanIcon aria-hidden="true" class="h-4 w-4" />
  <span class="sr-only">{t("moderation.ban-user")}</span>
</button>
```

Use `sr-only` for any icon-only button, context, or status text.

## Motion

Respect `prefers-reduced-motion`:

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
  }
}
```

For specific fancy animations, gate them: `@media (prefers-reduced-motion: no-preference) { ... }`.

## Color + state

- **Never color-only to convey state.** Error red must also have an icon + text. Colorblind users must still understand.
- **Dark mode contrast matters too.** Check both themes.
- **Disabled** → `aria-disabled="true"` with `opacity-50` + `pointer-events-none`.

## Telegram WebApp specifics

- The Telegram WebApp container theme can be light or dark; respect `Telegram.WebApp.colorScheme` and update the local theme accordingly.
- Telegram's BackButton and MainButton are accessible by default — don't reimplement them as in-page buttons.

## Deterministic checks (grep the diff)

- `onClick=\{` on `<div|<span` — wrong element.
- `outline-none` without `focus-visible:ring-` — invisible focus.
- `aria-label=""` — broken.
- `tabindex="\d+"` with N > 0 — tab order hack.
- `alt=""` on an `<img>` that conveys content — wrong.

## Related

- `design-anti-patterns` — low-contrast text callout.
- `ui-critique` — includes an a11y pass.
- Kobalte primitive wrappers: `website/src/shared/ui/`.
