# Team Onboarding Guide (Phase 1)

## Purpose
Use DevSync to provision a consistent local dev environment from a repository baseline.

## Prerequisites
- OS: Linux or Windows with WSL2
- Required: Rust toolchain (for current source build)
- Recommended: Docker
- Optional: Dev Container CLI (`@devcontainers/cli`)

## First-Time Setup

```bash
# 1) Clone repository
cd <your-project>

# 2) Generate lock + devcontainer files
/path/to/devsync init

# 3) Run diagnostics
/path/to/devsync doctor

# 4) Run governance checks
/path/to/devsync policy
/path/to/devsync secret-lint

# 5) Check activation readiness
/path/to/devsync activate

# 6) Bring up environment
/path/to/devsync up
```

## CI-Friendly Diagnostics
Use JSON output for machine-readable validation:

```bash
/path/to/devsync doctor --json
```

Example CI policy (ignore local tooling checks, enforce runtime + lockfile):

```bash
/path/to/devsync doctor --json --fail-on runtime-and-lock
```

## Typical Workflow
- Existing environment changed:
  - update project manifests/lockfiles
  - run `devsync lock --force`
  - run `devsync policy` and `devsync secret-lint`
  - run `devsync activate`
  - commit updated `devsync.lock`
- New teammate joins:
  - run `devsync doctor`
  - run `devsync policy`
  - run `devsync up`

## Troubleshooting
- `doctor` reports missing Docker:
  - install Docker Desktop/Engine and rerun
- `doctor` reports runtime mismatch:
  - align local runtime to lockfile pinned version
- `up` fails without Dev Container CLI:
  - install CLI or use Docker fallback command printed by DevSync
- `policy` fails due runtime pinning:
  - add `.nvmrc` / `.python-version` / `rust-toolchain.toml` (or `rust-toolchain`)
- `secret-lint` reports findings:
  - remove embedded credentials from generated files and use env references only
- `activate` shows pending actions:
  - resolve each checklist item until readiness score reaches 100%

## Team Conventions
- Commit `devsync.lock` to version control.
- Commit generated `.devcontainer` files unless your team has custom templates.
- Treat runtime pinning files (`.nvmrc`, `.python-version`, `rust-toolchain.toml`) as required.

## Team Registry Workflow (Phase 2)

Publish a versioned environment:

```bash
devsync --path /path/to/repo push acme/repo@v1 \
  --actor alice \
  --grant bob:viewer \
  --prebuild-cache s3://prebuilds/acme-repo:v1
```

List versions:

```bash
devsync registry-ls acme/repo --actor bob
```

Pull latest into another checkout:

```bash
devsync --path /path/to/consumer pull acme/repo@latest \
  --actor bob \
  --with-devcontainer \
  --primary-only
```

Remote mode (HTTP):

```bash
# terminal 1
devsync registry-serve --bind 127.0.0.1:8787 --auth-token "$DEVSYNC_AUTH_TOKEN"

# terminal 2
devsync --path /path/to/repo push acme/repo@v2 \
  --registry-url http://127.0.0.1:8787 \
  --actor alice \
  --auth-token "$DEVSYNC_AUTH_TOKEN"
```

Read project audit trail (admin role):

```bash
devsync registry-audit acme/repo --actor alice --json
```
