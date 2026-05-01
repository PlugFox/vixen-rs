---
name: form-error-ux
description: Show inline validation errors with aria-invalid + aria-describedby. Focus first invalid field on submit. Use Kobalte Form.Field for a11y wiring.
---

# Form Error UX (Vixen website)

**Source:** [Smashing Magazine — accessible form validation](https://www.smashingmagazine.com/2023/02/guide-accessible-form-validation/), [WebAIM form validation](https://webaim.org/techniques/formvalidation/).

**Read first:**

- [website/docs/rules/components.md](../../../../website/docs/rules/components.md).
- [website/docs/rules/styling.md](../../../../website/docs/rules/styling.md).

## Inline error pattern (Kobalte)

```tsx
<Form.Field name="threshold" validationState={isValid() ? "valid" : "invalid"}>
  <Form.Label>{t("settings.spam-threshold")}</Form.Label>
  <Form.Input type="number" min={0} max={100} value={threshold()} onInput={onInput} />
  <Form.Description>{t("settings.threshold-hint")}</Form.Description>
  <Form.ErrorMessage>{errorMsg()}</Form.ErrorMessage>
</Form.Field>
```

Kobalte wires `aria-invalid="true"` on the input and links the error via `aria-describedby` automatically when `validationState="invalid"`.

## Validation timing

- **On `blur`** — first validation. Don't fire mid-typing; users hate it.
- **On every keystroke after first blur** — live revalidation, so the error clears as soon as fixed.
- **On submit** — validate everything, focus first invalid.

## Submit-time focus + announce

```tsx
async function onSubmit(e: SubmitEvent) {
  e.preventDefault();
  const errors = validateAll(form);
  if (errors.length > 0) {
    setErrorCount(errors.length);                                   // for aria-live
    requestAnimationFrame(() => errors[0].field.focus());
    return;
  }
  await save();
}
```

```tsx
<div aria-live="polite" class="sr-only">
  <Show when={errorCount() > 0}>{t("forms.errors-count", { count: errorCount() })}</Show>
</div>
```

## Don't disable the submit button

Disabled submit hides errors from SR users and is confusing. Let users submit; surface errors inline and at the form level.

## Server errors → field errors

Map `ApiError.code` to a specific field when possible; fall back to a form-level `<Alert>`:

```ts
catch (e) {
  if (e instanceof ApiError && e.code === "THRESHOLD_OUT_OF_RANGE")
    setFieldError("threshold", t("errors.threshold-range"));
  else
    setFormError(t("errors.generic"));
}
```

## Copy

Error messages explain **what** + **how to fix**.

- Bad: `Invalid input`.
- Good: `Threshold must be between 0 and 100. Try 50.`
- Bad: `Required`.
- Good: `Reason is required to log this ban.`

## Vixen specifics

- **Destructive moderation forms** (ban): confirmation dialog before submit. Copy: `Ban @user? This action is logged.`
- **Settings forms** (per-chat config): debounced auto-save with a discreet save indicator (`Saved · 2s ago`). No separate Save button when changes are non-destructive.
- **Captcha submission**: single-shot; on wrong answer show `Incorrect — new image generated` and refetch.

## Gotchas

- `aria-describedby` pointing at a non-existent id → SR reads nothing or stale text. Always render the error element (use `hidden` attr if no error) so the id is stable.
- Color-only error indication (red border, no icon, no text) → WCAG 1.4.1 fail. Pair color + icon + text.
- Error in `placeholder` → vanishes on type, fails a11y. Always use a separate element.
- Validation that auto-trims silently → user thinks `"  bob  "` was accepted; show normalized value.

## Verification

- Keyboard-only: Tab through, intentionally fail, submit, focus lands on first invalid.
- Screen reader (VoiceOver / NVDA): announces error count and field-level error.
- Light + dark themes: error icon and text remain readable.

## Related

- `ui-accessibility` — focus-visible, aria patterns.
- `interaction-states-kobalte` — `data-[invalid]` styling.
- `loading-empty-error-states` — the wider error UX system.
