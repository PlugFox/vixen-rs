---
name: github-actions-workflow
description: Write or modify .github/workflows/*.yml — server-ci, website-ci, build-server, build-website. Concurrency, caching, sqlx offline mode, secrets via secrets.* refs. Use when adding/editing CI workflows or when the user asks about GH Actions.
---

# GitHub Actions Workflow (Vixen)

**Source:** [GH Actions docs](https://docs.github.com/en/actions).

## Workflow files (planned for vixen)

- `.github/workflows/server-ci.yml` — on PR touching `server/**` or main pushes.
- `.github/workflows/website-ci.yml` — on PR touching `website/**` or main pushes.
- `.github/workflows/build-server.yml` — on tag `v*`, push image to GHCR.
- `.github/workflows/build-website.yml` — on tag `v*`, build static site, push image.

## Concurrency

Cancel in-progress runs on push to the same ref:

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

## Server CI skeleton

```yaml
name: server-ci
on:
  pull_request:
    paths: ["server/**"]
  push:
    branches: [master]
    paths: ["server/**"]

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

permissions:
  contents: read

jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_PASSWORD: vixen_dev_password
          POSTGRES_USER: vixen
          POSTGRES_DB: vixen
        ports: ["5432:5432"]
        options: >-
          --health-cmd pg_isready --health-interval 10s
          --health-timeout 5s --health-retries 5
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt, clippy }
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: server }
      - name: fmt
        working-directory: server
        run: cargo fmt --check
      - name: clippy
        working-directory: server
        run: cargo clippy --all-targets --all-features -- -D warnings
      - name: sqlx-prepare-check
        working-directory: server
        env:
          DATABASE_URL: postgresql://vixen:vixen_dev_password@localhost:5432/vixen
        run: cargo sqlx migrate run && cargo sqlx prepare --check
      - name: test
        working-directory: server
        env:
          DATABASE_URL: postgresql://vixen:vixen_dev_password@localhost:5432/vixen
        run: cargo test --all-features
```

## Website CI skeleton

```yaml
- uses: oven-sh/setup-bun@v1
  with: { bun-version: "1.1" }
- run: bun install --frozen-lockfile
  working-directory: website
- run: bun run check && bun run typecheck && bun run build
  working-directory: website
```

## Image build (on tag)

```yaml
- uses: docker/setup-buildx-action@v3
- uses: docker/login-action@v3
  with:
    registry: ghcr.io
    username: ${{ github.actor }}
    password: ${{ secrets.GITHUB_TOKEN }}
- uses: docker/build-push-action@v5
  with:
    context: ./server
    file: ./server/Dockerfile
    push: true
    tags: ghcr.io/${{ github.repository }}/server:${{ github.ref_name }}
    cache-from: type=gha
    cache-to: type=gha,mode=max
```

## Secrets

- `secrets.GITHUB_TOKEN` is auto-provided per run.
- Custom secrets: `secrets.DOCKER_HUB_TOKEN`, `secrets.GHCR_PAT` etc. configured at repo level.
- Never echo a secret. GH masks them in logs but treat them as untrusted by `set -x`.

## Permissions

Default to `permissions: { contents: read }` at the workflow level. Opt into write per-job only where needed:

```yaml
permissions:
  contents: write   # for release notes
  packages: write   # for GHCR push
```

## Vixen specifics

- Server build uses `SQLX_OFFLINE=true` (Dockerfile ARG); CI uses a real Postgres service container.
- `.sqlx/` must be committed for the Docker build to work — CI verifies with `cargo sqlx prepare --check`.
- Bot token NEVER in CI env. CI doesn't need it (no `getMe` calls, no live Telegram interactions in CI).
- Keep the matrix small: one Rust toolchain (`stable`), one Bun version. Cross-version testing isn't worth the time.

## Related

- `docker-multi-stage` — the Dockerfiles invoked from build workflows.
- `verify-changes` — the local equivalent of CI.
- `infra/secrets-handling` (when added) — runtime secret injection patterns.
