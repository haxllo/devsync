# DevSync

DevSync is an AI-assisted environment standardizer for development teams.

This repository currently contains the MVP CLI that:
- detects Node/Python/Rust project requirements,
- generates a deterministic `devsync.lock`,
- scaffolds a `.devcontainer` setup,
- runs local environment diagnostics via `devsync doctor`.

## Why

Teams lose time on onboarding and environment drift:
- "works on my machine" failures,
- inconsistent runtime versions,
- dependency lock drift,
- delayed developer ramp-up.

DevSync solves this by turning repository signals into a reproducible baseline.

## MVP Status

Implemented commands:
- `devsync init`
- `devsync lock`
- `devsync survey`
- `devsync doctor`
- `devsync policy`
- `devsync secret-lint`
- `devsync activate`
- `devsync roi`
- `devsync dashboard-export`
- `devsync push`
- `devsync pull`
- `devsync registry-ls`
- `devsync registry-audit`
- `devsync registry-serve`
- `devsync auth-key-create`
- `devsync auth-key-ls`
- `devsync auth-key-revoke`
- `devsync entitlement-check`
- `devsync billing-plan-ls`
- `devsync billing-subscribe`
- `devsync billing-subscription-ls`
- `devsync billing-cycle`
- `devsync billing-invoice-ls`
- `devsync billing-invoice-pay`
- `devsync billing-events`
- `devsync billing-event-ack`
- `devsync billing-serve`
- `devsync up`

## Quick Start

```bash
cargo run -- init
cargo run -- doctor
cargo run -- up
```

Use a different project path:

```bash
cargo run -- --path /path/to/project init
```

## Generated Files

- `devsync.lock`: environment manifest with stacks/runtimes/package managers/services
- includes run hints plus `primary_run_hint` and `primary_stack` when detectable
- `.devcontainer/devcontainer.json`: Dev Container metadata
- `.devcontainer/Dockerfile`: baseline build image

## Command Reference

### `devsync init`
Scans project and generates lock + devcontainer files.

Flags:
- `--force` overwrite existing generated files
- `--skip-devcontainer` generate only `devsync.lock`
- `--primary-only` generate devcontainer for the inferred primary stack only

### `devsync lock`
Regenerates `devsync.lock` only.

Flags:
- `--force` overwrite existing lockfile

### `devsync doctor`
Validates local runtime/tooling against `devsync.lock` and reports mismatch warnings.

Flags:
- `--json` output structured report for CI and automation
- `--fail-on <policy>` controls non-zero exit behavior by check type
  policies: `all`, `runtime`, `lockfile`, `tooling`, `runtime-and-lock`, `none`

### `devsync up`
Starts environment with Dev Container CLI when available. Falls back to `docker build` guidance.

### `devsync policy`
Runs governance checks over generated artifacts and lockfile expectations.

Checks:
- approved `.devcontainer/Dockerfile` base image
- pinned runtime policy for detected stacks

Flags:
- `--policy <path>` optional policy file path (`devsync.policy.toml` is auto-detected)
- `--json` machine-readable output

### `devsync secret-lint`
Scans generated artifacts for likely secret exposure patterns.

Scanned files (if present):
- `devsync.lock`
- `.devcontainer/devcontainer.json`
- `.devcontainer/Dockerfile`

Flags:
- `--json` machine-readable output

### `devsync activate`
Checks repository activation readiness and prints actionable next steps.

Readiness checks include:
- lockfile presence/parse health
- `.devcontainer` artifact presence
- policy pass state
- secret-lint pass state

Flags:
- `--json` machine-readable output

### `devsync roi`
Estimates monthly savings and ROI for DevSync adoption.

Key flags:
- `--team-size <n>` (required)
- `--monthly-hires <n>`
- `--onboarding-hours-before <h>`
- `--onboarding-hours-after <h>`
- `--drift-incidents-per-dev <n>`
- `--drift-hours-per-incident <h>`
- `--drift-reduction-pct <0-100>`
- `--hourly-rate <usd>`
- `--price-per-dev <usd>`
- `--json` machine-readable output

### `devsync dashboard-export`
Exports a JSON dashboard combining activation readiness across repos plus an ROI scenario.

Key flags:
- `--root <path>` repository root scan path
- `--output <path>` write JSON output file
- `--max-repos <n>` optional cap
- ROI scenario flags (`--team-size`, `--hourly-rate`, etc.) match `devsync roi`

### `devsync push <org/project@version>`
Publishes local `devsync.lock` to the team registry.

Flags:
- `--registry <path>` registry root (defaults to `~/.devsync/registry`)
- `--registry-url <url>` use remote registry over HTTP (example `http://127.0.0.1:8787`)
- `--actor <name>` identity used for role checks/audit fields
- `--grant <subject:role>` repeatable role bindings (`admin/member/viewer`)
- `--prebuild-cache <pointer>` store prebuild cache pointer metadata
- `--auth-token <token>` bearer token for remote registry auth (or `DEVSYNC_AUTH_TOKEN`)
- `--force` overwrite existing version

### `devsync pull <org/project@version>`
Fetches an environment lock from the team registry.

Flags:
- `--registry <path>` registry root (defaults to `~/.devsync/registry`)
- `--registry-url <url>` use remote registry over HTTP (example `http://127.0.0.1:8787`)
- `--actor <name>` identity used for role checks
- `--force` overwrite existing local `devsync.lock`
- `--with-devcontainer` regenerate `.devcontainer` from pulled lock
- `--primary-only` with `--with-devcontainer`, generate primary stack only
- `--auth-token <token>` bearer token for remote registry auth (or `DEVSYNC_AUTH_TOKEN`)

### `devsync registry-ls <org/project>`
Lists published versions for a registry project.

Flags:
- `--registry <path>` registry root (defaults to `~/.devsync/registry`)
- `--registry-url <url>` use remote registry over HTTP (example `http://127.0.0.1:8787`)
- `--actor <name>` identity used for role checks
- `--auth-token <token>` bearer token for remote registry auth (or `DEVSYNC_AUTH_TOKEN`)
- `--json` machine-readable output

### `devsync registry-audit <org/project>`
Lists audit events for a registry project (admin role required).

Events include:
- `environment.create`
- `environment.update`
- `environment.use`
- `environment.list`

Flags:
- `--registry <path>` registry root (defaults to `~/.devsync/registry`)
- `--registry-url <url>` use remote registry over HTTP (example `http://127.0.0.1:8787`)
- `--actor <name>` identity used for role checks
- `--auth-token <token>` bearer token for remote registry auth (or `DEVSYNC_AUTH_TOKEN`)
- `--limit <n>` max events returned (default `50`)
- `--json` machine-readable output

### `devsync registry-serve`
Runs a local HTTP registry server backed by the file registry layout.

Flags:
- `--bind <addr>` bind address (default `127.0.0.1:8787`)
- `--registry <path>` registry root (defaults to `~/.devsync/registry`)
- `--billing <path>` billing store root for entitlement checks (defaults to `~/.devsync/billing`)
- `--enforce-entitlements` require active org subscription for registry routes
- `--auth-token <token>` require bearer token for all HTTP routes
- `--auth-store <path>` load scoped API keys (created via `auth-key-*`)
- `--once` handle one request then exit (useful for smoke tests)

### `devsync auth-key-create`
Creates an API key for `registry-serve` and/or `billing-serve`.

Flags:
- `--auth-store <path>` auth key store path (defaults to `~/.devsync/auth_keys.toml`)
- `--subject <name>` subject label for audit/access logs
- `--service <registry|billing|*>` service binding (default `*`)
- `--org <org>` optional org binding
- `--scope <scope>` repeatable scope (`registry.read`, `registry.write`, `registry.admin`, `billing.read`, `billing.write`, `billing.admin`, `*`)
- `--ttl-days <n>` optional key expiry in days
- `--rate-limit-rpm <n>` per-key requests per minute (default `120`)
- `--note <text>` optional operator note
- `--json` machine-readable output

### `devsync auth-key-ls`
Lists API keys from auth store.

### `devsync auth-key-revoke <key_id>`
Revokes an API key in auth store.

### `devsync entitlement-check <org>`
Checks whether an org currently has active entitlement.

Flags:
- `--billing <path>` local billing store
- `--billing-url <url>` remote billing API
- `--auth-token <token>` bearer token for remote mode
- `--json` machine-readable output

### Billing Commands

File-backed billing backend root defaults to `~/.devsync/billing`.
Each billing client command can run in:
- local mode: `--billing <path>`
- remote mode: `--billing-url <url>` with optional `--auth-token <token>` (or `DEVSYNC_AUTH_TOKEN`)

- `devsync billing-plan-ls`
- `devsync billing-subscribe <org> --plan <plan> --seats <n>`
- `devsync billing-subscription-ls [--org <org>]`
- `devsync billing-cycle [--at <RFC3339>]`
- `devsync billing-invoice-ls [--org <org>]`
- `devsync billing-invoice-pay <invoice_id>`
- `devsync billing-events [--org <org>] [--pending-only]`
- `devsync billing-event-ack <event_id>`
- `devsync billing-serve --bind 127.0.0.1:8795 [--auth-token <token>] [--auth-store <path>]`

Remote billing example:

```bash
# terminal 1
devsync billing-serve --bind 127.0.0.1:8795 --auth-token "$DEVSYNC_AUTH_TOKEN"

# terminal 2
devsync billing-plan-ls \
  --billing-url http://127.0.0.1:8795 \
  --auth-token "$DEVSYNC_AUTH_TOKEN"
```

Billing API routes (`billing-serve`):
- `POST /v1/billing/plans/list`
- `POST /v1/billing/subscriptions/create`
- `POST /v1/billing/subscriptions/list`
- `POST /v1/billing/cycle/run`
- `POST /v1/billing/invoices/list`
- `POST /v1/billing/invoices/pay`
- `POST /v1/billing/events/list`
- `POST /v1/billing/events/ack`

Auth notes for HTTP servers:
- if neither `--auth-token` nor `--auth-store` is set, routes are unauthenticated (local/dev mode)
- `--auth-token` keeps simple shared-token behavior
- `--auth-store` enables scoped API keys + org boundaries + per-key rate limits + access logging

## Policy File

Optional file: `devsync.policy.toml`

```toml
schema_version = 1
approved_base_images = ["mcr.microsoft.com/devcontainers/base:ubuntu-24.04"]
require_pinned_runtimes = true
```

## Next Steps

See:
- `docs/ROADMAP.md`
- `docs/MVP_SPEC.md`
- `docs/ARCHITECTURE.md`
- `docs/ONBOARDING.md`
- `docs/PHASE1_IMPLEMENTATION.md`
- `docs/PHASE2_IMPLEMENTATION.md`
- `docs/PHASE3_IMPLEMENTATION.md`
- `docs/PHASE4_IMPLEMENTATION.md`
- `docs/PRICING_PACKAGING.md`
- `docs/CUSTOMER_SUCCESS_PLAYBOOK.md`
- `docs/MIGRATION_GUIDE.md`
- `docs/BILLING_API.md`

## Phase 1 Validation

Run local closeout validation across repositories under `~/Projects`:

```bash
./scripts/phase1-validate.sh ~/Projects
```

This writes a timestamped report to `docs/reports/`.

## Phase 3 Validation

Run phase-3 governance/security validation:

```bash
./scripts/phase3-validate.sh
```

To also validate remote auth-token flow:

```bash
./scripts/phase3-validate.sh --with-remote
```

## Phase 4 Validation

Run phase-4 activation and ROI validation:

```bash
./scripts/phase4-validate.sh
```

This includes:
- activation + ROI checks
- dashboard export validation
- billing backend workflow validation
