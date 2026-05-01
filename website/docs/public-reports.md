# Public Reports

`/report/{chat_slug}` is unauthenticated, indexable, and intentionally redacted. The same data the moderators see in the dashboard, minus everything that identifies a person.

## What's shown

- Chat title (already public Telegram metadata).
- Daily aggregates: messages_seen, messages_deleted, users_verified, users_banned, captcha_attempts.
- Hourly bar chart for the last 24h.
- Top spam phrase **categories** (e.g. "investment scam: 12 hits") — not raw phrase text.
- "Last updated at" timestamp.

## What's NOT shown

- **Usernames or display names.** Top users / banned users by name → omitted entirely.
- **User IDs.**
- **Raw message bodies.** Top spam phrases → categorized + redacted (`[link]`, `[user]`, `[phone]`).
- **Action ledger.** That contains `target_user_id` and goes through the auth-gated dashboard.
- **Chat invite links / private metadata.**

## Routes

| Route | Purpose |
|---|---|
| `GET /report/{chat_slug}` | HTML page (SPA). Server returns the SPA shell + meta tags. |
| `GET /report/{chat_slug}/chart.png` | The daily chart as a standalone PNG. Cached `max-age=3600`. Used for OG image and direct embedding. |
| `GET /report/{chat_slug}/data.json` | The aggregated JSON the page consumes. Same redaction as the page. |

`chat_slug` is set per-chat by a moderator via the dashboard (`PATCH /api/v1/chats/{id}/config { slug: "..." }`). Chats without a slug have NO public report.

## Page structure

```tsx
// pages/public-report.tsx
const params = useParams<{ chatSlug: string }>();
const [data] = createResource(() => params.chatSlug, fetchPublicReport);

return (
  <main>
    <h1>{t("public-report.title", { chat: data()?.title })}</h1>
    <ChartImage chatSlug={params.chatSlug} />
    <DailyAggregatesGrid stats={data()} />
    <TopPhraseCategories categories={data()?.top_categories} />
    <Footer generatedAt={data()?.generated_at} />
  </main>
);
```

## Meta tags

See `.claude/skills/website/seo-meta/SKILL.md`. Public report routes need:

- `<title>` — chat-name first, brand at end (max 60 chars).
- `<meta name="description">` — factual, one-sentence, no PII (max 160 chars).
- `<link rel="canonical">` — absolute URL.
- Open Graph: title, description, image (= the chart PNG), url, type=website.
- Twitter card: same as OG.
- JSON-LD: `Dataset` schema, `creator: SoftwareApplication, name: "Vixen"`.

The dashboard (`/app/*`) is `noindex` via `X-Robots-Tag` server header. **Don't** rely on `<meta name="robots">` for that — server-side enforcement only.

## Caching headers

Server controls these:

- HTML page: `Cache-Control: public, max-age=300, stale-while-revalidate=3600` (5min, 1h SWR).
- Chart PNG: `Cache-Control: public, max-age=3600, immutable` (1h, immutable since URL doesn't change within a day).
- JSON data: same as HTML page.

The chart PNG is the OG image — high cache hit rate matters for crawlers.

## Reduction-only redactions

The redaction step happens **server-side** in `report_service::redact_for_public()` before serializing. The website never sees the raw user data, so a website bug cannot leak PII.

Categorization of phrases (e.g. "investment scam") happens via a small mapping table (`top_phrase_classifier.rs`). New categories are added by ops, not by Claude — they need human review for false-positive risk.

## Performance

Target:

- LCP < 2s (chart PNG is the candidate; preload it).
- INP < 200ms (the page is mostly static; little JS interaction).
- CLS < 0.1 (reserve chart space with `width × height` attrs).
- Initial JS payload < 30KB gz (route-split; the public-report bundle should NOT include the dashboard's Kobalte components).

The router is configured so `pages/public-report.tsx`'s lazy chunk does NOT pull in `features/moderation/`, `features/settings/`, etc. Tree-shake-friendly imports are mandatory.

## Two-mode rendering inside Telegram WebApp

The public report is **not** meant to be rendered inside a Telegram WebApp container — it's a public web page. If a user opens the URL inside Telegram (via a forwarded link), the page still renders correctly but doesn't try to use any WebApp APIs (`isInWebApp()` check is gated to the dashboard layout).

## i18n

Public reports respect the user's locale (browser `Accept-Language` or stored `vixen_locale`). The set of strings is small — under `i18n/messages/{locale}/reports.yaml` namespace `reports.public.*`.

Cyrillic chat titles render correctly in any locale (the page sets `<html lang>` based on the title's script if no user preference is detected — heuristic, not perfect).

## When NOT to add to the public report

- Anything that could let a determined viewer correlate timestamps with a specific user (e.g. "user X was banned at 14:32:17" — even without the username).
- Anything that requires opt-in from the chat (members count? title? — title is already public Telegram metadata, members count is borderline).

When in doubt, default to NOT exposing. The public report's value is in its trustworthy non-leaking nature.
