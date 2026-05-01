---
name: docker-multi-stage
description: Write small, fast, reproducible Dockerfiles for the vixen-rs server (Rust) and website (bun) using multi-stage builds, cache mounts, distroless/alpine runtime images, and non-root users. Use when editing server/Dockerfile, website/Dockerfile, docker-compose.yml, or when the user asks to shrink an image or speed up a Docker build.
---

# Docker Multi-Stage (Vixen)

**Source:** Docker Buildx + base-image docs, plus foxic-derived patterns.

## Principles

- **Stage 1 builds, stage 2 runs.** The runtime image ships only the artifact, not the toolchain.
- **Pin everything**: base image `rust:1.XX-slim`, `oven/bun:1.X`, `debian:bookworm-slim`. No `latest`.
- **Cache what is expensive, rebuild what is cheap.** Deps before source. Use BuildKit cache mounts.
- **Non-root runtime.** `USER appuser` in the final stage. Read-only filesystem where practical.
- **One process per container.** Don't stuff `postgres + server` into one image; that's what docker-compose is for.
- **Bake the bot-token-redaction discipline into the runtime.** Never expose the token via `ENV` printed at build time. Inject at runtime via Compose / orchestrator secret.

## Rust server — reference skeleton

```dockerfile
# syntax=docker/dockerfile:1.7
FROM rust:1.85-slim AS builder
WORKDIR /app

RUN --mount=type=cache,target=/var/cache/apt \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

# 1) Cache dependencies separately from source.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main(){}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && rm -rf src

# 2) Real build (uses .sqlx/ offline cache).
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    SQLX_OFFLINE=true cargo build --release --bin vixen-server \
 && cp target/release/vixen-server /usr/local/bin/vixen-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 && rm -rf /var/lib/apt/lists/* \
 && useradd -r -u 10001 -g nogroup appuser
WORKDIR /app
COPY --from=builder /usr/local/bin/vixen-server /usr/local/bin/vixen-server
# Bake captcha assets into the image (immutable per build).
COPY --from=builder /app/assets/captcha /app/assets/captcha
USER appuser
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/vixen-server"]
```

Notes:
- `SQLX_OFFLINE=true` + `.sqlx/` committed → no DB needed at build time.
- `--mount=type=cache` requires BuildKit (`DOCKER_BUILDKIT=1` or modern Docker). Keep the `# syntax=` directive on line 1.
- Captcha font assets baked into the image — they're immutable and shared by all challenges.
- For a smaller image, use `gcr.io/distroless/cc-debian12` as the runtime stage.

## bun website — reference skeleton

```dockerfile
# syntax=docker/dockerfile:1.7
FROM oven/bun:1.1 AS builder
WORKDIR /app
COPY package.json bun.lock ./
RUN --mount=type=cache,target=/root/.bun/install/cache \
    bun install --frozen-lockfile
COPY . .
RUN bun run build

FROM nginx:1.27-alpine AS runtime
COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
```

Notes:
- `--frozen-lockfile` for reproducibility; fails on drift.
- Static SPA → nginx.

## `.dockerignore` is mandatory

```
.git
.github
**/target
**/node_modules
**/dist
**/.sqlx-cache
*.log
.env*
.secrets/
```

## Image size / security quick wins

- Combine `apt-get update && apt-get install ... && rm -rf /var/lib/apt/lists/*` in one `RUN`.
- `--no-install-recommends` trims tens of MB.
- Copy only the built binary into the runtime stage.
- Scan images: `docker scout cves <image>` before publishing.
- Add `HEALTHCHECK` to the runtime stage: `HEALTHCHECK --interval=10s --timeout=3s CMD curl -fsS http://localhost:8000/health || exit 1`. Catches hung processes that bind the port but stop responding.
- SQLx offline build: the build stage needs `ENV SQLX_OFFLINE=true` and the `.sqlx/` directory copied in. The `.sqlx/` directory MUST be committed to the repo and refreshed via `/db-migrate` after migration changes.

## Secrets

**Never** bake secrets into layers via `ARG SECRET=...` — visible in `docker history`. Use `--secret id=foo,src=...` with `RUN --mount=type=secret,id=foo`. For Vixen specifically: `TELEGRAM_BOT_TOKEN`, `OPENAI_API_KEY`, `JWT_SECRET`, `ADMIN_SECRET` are runtime-only — Compose/orchestrator injects them as env at `docker run`, never at `docker build`.

## Common mistakes

- Rebuilding deps on every source change — caused by copying `Cargo.toml` and `src/` in the same `COPY`. Split them.
- `COPY --chown=...` without creating the user first — ordering matters.
- Running as root in production; `USER appuser` is one line.
- `EXPOSE` is documentation, not a firewall. Still publish with `-p` or compose `ports:`.

## docker-compose.yml

Local dev stack lives in `docker/docker-compose.yml` (postgres + server + website). Keep:
- Named volumes for DB data, not bind mounts, to avoid permission issues on macOS.
- `depends_on` with `condition: service_healthy` for the server → postgres dependency.
- Override with `docker-compose.override.yml` for local tweaks.
- **Never** include `down -v` shortcuts in scripts — that wipes the DB volume.

## Related

- Local dev infra: `/db-up` starts the stack.
- Prod build: see `.github/workflows/` for the CI Docker build.
