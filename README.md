# Vixen-rs

Telegram anti-spam bot. Rust + PostgreSQL backend, SolidJS dashboard + public reports.

Watches a fixed set of Telegram chats, gates new users behind a CAPTCHA challenge, deduplicates spam by xxhash + Combot Anti-Spam, posts daily aggregated reports back into each chat, and exposes a moderator dashboard authenticated via Telegram Login.

## Repository

| Directory | Description | Status |
|-----------|-------------|--------|
| `server/` | Rust backend (Axum + PostgreSQL + teloxide) | In development |
| `website/` | SolidJS dashboard + public reports | Planned |
| `docker/` | Docker Compose setup | Planned |
| `.github/` | CI/CD workflows | Planned |

## Quick Start

```bash
# Start PostgreSQL
docker compose -f docker/docker-compose.yml up -d postgres

# Run database migrations
cd server && sqlx migrate run

# Start the server (HTTP + bot polling)
cd server && cargo run

# Start the dashboard
cd website && bun install && bun run dev
```

Configuration: see `server/config/template.env` for all `CONFIG_*` variables; secrets policy in [docs/deployment.md](docs/deployment.md).

## Features

**Planned (port from the Dart prototype):**
- Automatic CAPTCHA challenge for unverified users (digit-pad image)
- xxhash dedup spam detection + Combot Anti-Spam integration + n-gram phrase match
- Daily PNG report posted into each watched chat
- Optional OpenAI summary of the day's discussion (per-chat token budget)
- Slash commands: `/start /help /status /verify /ban /unban /stats`
- Per-chat configuration (thresholds, captcha mode, report hour, AI summary on/off, clown reaction chance)
- Moderator dashboard (Telegram Login authenticated)
- Public chat report page (redacted, indexable)

**Roadmap**: see [docs/roadmap.md](docs/roadmap.md). **Backlog**: see [docs/features.md](docs/features.md).

## Documentation

- Architecture: [docs/architecture.md](docs/architecture.md)
- Deployment: [docs/deployment.md](docs/deployment.md)
- Server: [server/docs/](server/docs/)
- Website: [website/docs/](website/docs/)
- LLM conventions: [AGENTS.md](AGENTS.md), [CLAUDE.md](CLAUDE.md)
- Changelog: [CHANGELOG.md](CHANGELOG.md)

## License

MIT — same as the original Dart implementation at [github.com/PlugFox/vixen](https://github.com/PlugFox/vixen).
