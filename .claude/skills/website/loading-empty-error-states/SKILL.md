---
name: loading-empty-error-states
description: Every async surface needs five states — loading (skeleton), empty, error (with retry), partial/stale, success. Skeleton over spinner. Empty-state copy must include CTA.
---

# Loading / Empty / Error States (Vixen website)

**Source:** [UX Writing Hub — empty states](https://uxwritinghub.com/empty-state-examples/).

**Read first:**

- [website/docs/rules/components.md](../../../../website/docs/rules/components.md).

## The five states

1. **Loading** — skeleton matching final layout (no CLS).
2. **Empty** — first-time, no data ever. Copy + CTA.
3. **Error** — recoverable. Show retry.
4. **Partial / stale** — offline or `state === "refreshing"`. Show stale data + indicator.
5. **Success** — happy path with data.

Don't conflate **empty** and **error** — "no rows" is not a failure.

## Skeleton vs spinner

- **Skeleton** — content regions (lists, cards, charts). Prevents layout shift.
- **Spinner** — discrete async actions (submit button, inline mutation).

## Empty-state copy template

- **Headline (5–7 words)**: `No moderation actions yet.`
- **Reason (1 line)**: `When the bot bans a user, you'll see it here.`
- **CTA**: `[Configure detection rules]` → `/settings/spam`.

Always one CTA. If there's no action the user can take, surface a help link instead.

## Error-state copy

- **What broke**: `Couldn't load chat list.` (not "Something went wrong").
- **Recovery**: `[Retry]` button bound to `refetch()`.
- **Optional code**: `Code: NETWORK` — useful for support.

## Skeleton sizing

Match expected real content dimensions ±10%. A 3-line list skeleton showing one line = jarring snap on load.

## Vixen surfaces

| Surface | States needed |
| --- | --- |
| Chats list | empty (no watched chats), loading, error |
| Moderation ledger | empty (new chat), loading, error, partial (filter narrows to 0) |
| Chat detail | loading, error |
| Public report `/report/{slug}` | loading, error → **404 page**, not inline |
| Captcha image | loading (preload), error (retry) |
| Settings form | success only after save; field errors via `form-error-ux` |

## Pattern

```tsx
<Switch>
  <Match when={chats.error}>
    <ErrorState
      title={t("chats.load-failed")}
      code={(chats.error as ApiError).code}
      onRetry={refetch}
    />
  </Match>
  <Match when={chats.loading}><ChatListSkeleton rows={6} /></Match>
  <Match when={chats()?.length === 0}>
    <EmptyState
      title={t("chats.empty-title")}
      body={t("chats.empty-body")}
      cta={{ label: t("chats.empty-cta"), href: "/settings/chats" }}
    />
  </Match>
  <Match when={chats()}>{(c) => <ChatList chats={c()} />}</Match>
</Switch>
```

Order matters: `error` before `loading` so a stale error doesn't hide behind a skeleton on `refreshing`.

## Stale data (refreshing)

```tsx
<div class="relative">
  <ChatList chats={chats() ?? []} />
  <Show when={chats.state === "refreshing"}>
    <Spinner class="absolute right-2 top-2" aria-label={t("status.refreshing")} />
  </Show>
</div>
```

## Gotchas

- Empty state ≠ error state.
- "Something went wrong" is unhelpful — at minimum say what was attempted.
- Spinners that never resolve = perceived hang. Set a ~10s ceiling, then show error.
- Skeletons that mismatch real layout = flash of layout shift. Audit alignment.
- Empty CTA pointing nowhere useful → drop the CTA, show help text only.

## Verification

DevTools → Network → Offline. Every async surface must show error + retry. Network → throttle to slow 3G — skeletons appear within 100ms, no spinner over content.

## Related

- `solid-resource-pattern` — `resource.state` drives this.
- `form-error-ux` — field-level errors.
- `ui-critique` — empty/error pass.
