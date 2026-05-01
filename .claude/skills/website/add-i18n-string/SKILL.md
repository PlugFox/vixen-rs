---
name: add-i18n-string
description: Add a user-facing translation string to the vixen-rs website i18n files (en + ru) and use it correctly via t()/tp()/<T>. Use when the user asks to add a label, button text, error message, placeholder, or any UI text — and when fixing a hardcoded string.
---

# Add i18n String (Vixen website)

**Hard rule** from [website/CLAUDE.md](../../../../website/CLAUDE.md): **never hardcode user-facing text.** This includes buttons, labels, tooltips, aria-labels, placeholder attributes, error toasts.

**Read first**: [website/docs/i18n.md](../../../../website/docs/i18n.md) — key naming, plural rules, interpolation syntax.

## Files

Messages live in [website/i18n/messages/](../../../../website/i18n/messages/) as `.yaml`, one file per domain:

- `common.yaml` — generic actions (save, cancel, delete).
- `auth.yaml` — Telegram login flow, errors.
- `errors.yaml` — user-visible error messages (mirror server `AppError` codes).
- `chats.yaml`, `moderation.yaml`, `reports.yaml`, `settings.yaml` — feature-specific.

Add new keys to the **existing** file for the feature. Create a new file only if the feature is genuinely new.

## Locales

Vixen ships RU + EN from day one. Every key MUST exist in **both** locale files. The Russian audience is the primary user base; missing RU translations are a P1 bug.

## Key naming

- Dot-separated, kebab-case segments: `chats.list.empty-state`.
- Prefix by feature / page: `chats.detail.title`, `chats.detail.spam-threshold-label`.
- For errors: `errors.<domain>.<reason>` — matches server error codes where applicable (e.g. `errors.auth.invalid-init-data`).

## Adding a key

```yaml
# website/i18n/messages/chats.yaml (en)
chats:
  list:
    title: Watched chats
    empty-state: "No chats are being watched yet. Add a chat to your bot's CONFIG_CHATS env var to start."
  detail:
    title: "{title}"
    spam-threshold-label: Spam detection threshold
```

```yaml
# website/i18n/messages/chats.yaml (ru)
chats:
  list:
    title: Отслеживаемые чаты
    empty-state: "Пока нет отслеживаемых чатов. Добавьте чат в переменную CONFIG_CHATS бота, чтобы начать."
  detail:
    title: "{title}"
    spam-threshold-label: Порог детекции спама
```

## Usage

```tsx
import { t, tp, T } from "~/shared/i18n";

// Simple
<h1>{t("chats.list.title")}</h1>

// With interpolation
<p>{t("chats.detail.title", { title: chat().title })}</p>

// Plural
<p>{tp("moderation.actions.count", actions().length)}</p>

// With inline formatting / rich text
<T path="chats.detail.invite-hint" values={{ botName }} />
```

For `aria-label`, `placeholder`, `title` attributes — use `t()` too, not string literals.

## Plurals — RU has more cases than EN

Russian uses `one` / `few` / `many` / `other`:

```yaml
# en
moderation:
  actions:
    count:
      one: "{n} action"
      other: "{n} actions"
# ru
moderation:
  actions:
    count:
      one: "{n} действие"
      few: "{n} действия"
      many: "{n} действий"
      other: "{n} действия"
```

Use `tp()` — it picks the right branch from the count according to the active locale's CLDR rules.

- English uses `one | other`. Russian uses `one | few | many | other` — translating `count: { one: "1 message", other: "{count} messages" }` directly into RU drops `few` and `many` cases. The `tp()` helper returns `undefined` if the form is missing.
- Validate by running the i18n linter (or by sight) after a copy update; the codegen catches missing forms.
- For YAML literal `{` `}` (e.g., describing a placeholder syntax in help text), escape as `'{{'` and `'}}'` — Single-brace usage is consumed by ICU MessageFormat.

## Checklist

- Key added to **every** locale file (en + ru) in `website/i18n/messages/`.
- No leftover hardcoded text in the component (grep the feature folder for quoted English/Russian strings).
- `aria-label`, `placeholder`, `title`, `alt` use `t()`.
- Error messages route through `errors.*` so the global error toast can localize them.
- RU translation provided (TODO-leaving Russian as English is a regression).

## After writing

1. `/website-check` — the i18n typed-key generation step catches missing keys.
2. If text rendering changes (new strings, plurals, long content), verify visually in both locales — RU strings are ~30% longer on average.
