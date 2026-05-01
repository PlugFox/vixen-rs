---
description: Start local Postgres via docker compose
allowed-tools: Bash
---

Bring up the local dev database and verify it's healthy.

1. `docker compose -f docker/docker-compose.yml up -d postgres` (or `cd docker && docker compose up -d postgres` if compose file is per-dir).
2. Wait up to 30s for the container to report healthy via `docker compose ps`.
3. Print the final status (container, state, ports) in a compact table.

If the container fails to become healthy: show the last 30 lines of `docker compose logs postgres` and stop. Do not run `docker compose down -v` to "fix" — that wipes the volume; ask the user first.

Vixen-rs is single-tenant and has no S3/MinIO dependency.
