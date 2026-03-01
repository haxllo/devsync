# DevSync MVP Specification

## Summary
This document defines the v0.1 CLI scope and acceptance criteria for DevSync.

## In Scope
- Repository scanning for Node/Python/Rust indicators
- Generation of `devsync.lock`
- Generation of `.devcontainer` baseline files
- Diagnostics for local tool/runtime compatibility
- Startup command via Dev Container CLI or Docker fallback
- Team registry publish/pull/list flows (file-backed backend)
- Governance policy checks for generated artifacts and runtime pinning
- Secret exposure linting for generated artifacts
- Registry audit logs for create/update/use/list events
- Optional bearer-token auth for HTTP registry access
- Scoped API-key auth store for registry/billing HTTP APIs
- Per-key/org route authorization and in-process rate limiting for HTTP servers
- Org entitlement checks for registry routes (optional enforcement mode)
- Activation readiness report for onboarding guidance
- ROI estimator command for sales/pilot value modeling
- Dashboard export for GTM reporting across repositories
- File-backed billing backend with subscription/invoice/event workflows
- Billing HTTP API server for self-serve integration
- Remote billing client mode (`--billing-url`) with optional bearer auth

## Out of Scope
- Full machine-state snapshotting
- Windows native runtime support (without WSL2)
- Hosted registry and team sync
- Auto-fixing conflicts
- Secret synchronization
- Full SSO/OIDC identity flow (token-based auth hook only in v0.1)

## CLI Interface

### `devsync init [--force] [--skip-devcontainer] [--primary-only]`
Behavior:
- scan project path (default current directory)
- write `devsync.lock`
- optionally write `.devcontainer/devcontainer.json` and `.devcontainer/Dockerfile`
- with `--primary-only`, generate devcontainer for primary stack only
- print detection summary and recommendations

Failure conditions:
- existing generated files without `--force`
- unreadable/invalid manifest files

### `devsync lock [--force]`
Behavior:
- rescan project
- write/overwrite `devsync.lock`

Failure conditions:
- existing `devsync.lock` without `--force`

### `devsync survey [--json]`
Behavior:
- inspect project signals without mutating files
- print human summary or JSON payload

### `devsync doctor [--json] [--fail-on <policy>]`
Behavior:
- load `devsync.lock` if present
- check installed tooling (`docker`, `devcontainer`)
- compare installed runtime versions to lock expectations
- emit machine-readable report with `--json`
- return non-zero based on selected failure policy

Failure policies:
- `all` (default): fail on any warning
- `runtime`: fail only runtime checks
- `lockfile`: fail only lockfile checks
- `tooling`: fail only tooling checks
- `runtime-and-lock`: fail on runtime + lockfile warnings, ignore tooling warnings
- `none`: always return zero unless command execution errors

### `devsync up`
Behavior:
- if `devcontainer` CLI exists: run `devcontainer up --workspace-folder <path>`
- else if Docker exists: build fallback image and print run command
- else fail with guidance

### `devsync policy [--policy <path>] [--json]`
Behavior:
- load policy from explicit path, `<project>/devsync.policy.toml`, or built-in defaults
- check `.devcontainer/Dockerfile` base image against approved image allowlist
- enforce pinned runtimes for detected stacks when configured
- return non-zero when policy violations are present

### `devsync secret-lint [--json]`
Behavior:
- scan generated artifacts (`devsync.lock`, `.devcontainer/*`) for likely secret leakage patterns
- return non-zero when findings are present

### `devsync activate [--json]`
Behavior:
- compute readiness score and actionable checklist for repo activation
- evaluate lockfile, generated devcontainer artifacts, policy status, and secret-lint status
- return non-zero when checklist has pending action items

### `devsync roi --team-size <n> [--json]`
Behavior:
- estimate onboarding + drift savings with configurable assumptions
- output gross savings, subscription cost, net savings, and ROI percentage
- provide recommended plan tier by team size

### `devsync dashboard-export --root <path> --team-size <n>`
Behavior:
- scan repositories under root for activation readiness
- aggregate in-scope readiness score and coverage metrics
- include ROI scenario output in the same JSON payload
- optionally write to output file (`--output`)

### Billing command family
Behavior:
- maintain file-backed billing store (plans, subscriptions, invoices, events)
- support subscription creation/update, cycle runs, invoice payment, event acknowledgement
- expose webhook-ready outbox events (`billing-events`)
- provide HTTP API surface via `billing-serve`
- support local store mode (`--billing`) and remote API mode (`--billing-url`)

### `devsync auth-key-create|auth-key-ls|auth-key-revoke`
Behavior:
- manage file-backed API keys (`~/.devsync/auth_keys.toml` by default)
- create keys with service binding (`registry`/`billing`/`*`), scopes, optional org scope, TTL, and rate limit
- revoke keys without deleting history

### `devsync entitlement-check <org>`
Behavior:
- evaluates whether org has active subscription entitlement
- supports local billing store mode and remote billing API mode

### `devsync push <org/project@version>`
Behavior:
- read local `devsync.lock`
- publish versioned registry entry
- apply role bindings (`--grant subject:role`)
- attach optional prebuild cache pointer metadata
- support local file registry (`--registry`) and HTTP registry (`--registry-url`)
- emit audit event (`environment.create`/`environment.update`)
- forward optional bearer token for remote auth (`--auth-token` / `DEVSYNC_AUTH_TOKEN`)

### `devsync pull <org/project@version>`
Behavior:
- resolve version (`@latest` supported)
- enforce role-based access
- write pulled lock locally
- optionally regenerate `.devcontainer`
- support local file registry (`--registry`) and HTTP registry (`--registry-url`)
- emit audit event (`environment.use`)
- forward optional bearer token for remote auth (`--auth-token` / `DEVSYNC_AUTH_TOKEN`)

### `devsync registry-ls <org/project>`
Behavior:
- list project versions, latest pointer, and role bindings
- support JSON output for automation
- support local file registry (`--registry`) and HTTP registry (`--registry-url`)
- emit audit event (`environment.list`)
- forward optional bearer token for remote auth (`--auth-token` / `DEVSYNC_AUTH_TOKEN`)

### `devsync registry-audit <org/project>`
Behavior:
- list project-scoped audit events from registry backend
- require admin role for access
- support JSON output
- support local file registry (`--registry`) and HTTP registry (`--registry-url`)

### `devsync registry-serve`
Behavior:
- bind a local HTTP API over file-backed registry data
- optionally require bearer token on all routes
- optionally enforce scoped API keys from auth store (`--auth-store`)
- optionally enforce active org entitlement (`--enforce-entitlements`, billing-backed)
- apply per-key requests/minute limiting and write access logs
- routes:
  - `POST /v1/push`
  - `POST /v1/pull`
  - `POST /v1/list`
  - `POST /v1/audit`

### `devsync billing-serve`
Behavior:
- bind local billing HTTP API over file-backed billing data
- optionally require bearer token on all routes
- optionally enforce scoped API keys from auth store (`--auth-store`)
- apply per-key requests/minute limiting and write access logs
- routes:
  - `POST /v1/billing/plans/list`
  - `POST /v1/billing/subscriptions/create`
  - `POST /v1/billing/subscriptions/list`
  - `POST /v1/billing/cycle/run`
  - `POST /v1/billing/invoices/list`
  - `POST /v1/billing/invoices/pay`
  - `POST /v1/billing/events/list`
  - `POST /v1/billing/events/ack`

## Lockfile Contract

File: `devsync.lock` (TOML)

Top-level fields:
- `schema_version: integer`
- `generated_at: RFC3339 datetime`
- `project`
- `runtimes`
- `package_managers`
- `services`
- `run_hints`
- `primary_run_hint`
- `primary_stack`
- `recommendations`

Sections:

```toml
schema_version = 1
generated_at = "2026-03-01T00:00:00Z"
services = ["compose", "postgres"]
run_hints = ["cargo run -p api-service"]
primary_run_hint = "cargo run -p api-service"
primary_stack = "rust"
recommendations = ["Pin Node version with .nvmrc"]

[project]
name = "api-service"
root = "/path/to/api-service"
stacks = ["node", "python"]

[runtimes]
node = ">=20"
python = ">=3.11"
rust = "1.77.2"

[package_managers]
node = "pnpm"
python = "uv"
```

Deterministic refresh behavior:
- if lock content is unchanged, `generated_at` is preserved
- this prevents no-op refreshes from producing noisy diffs

## Detection Rules (v0.1)

### Stacks
- Node: `package.json`
- Python: `pyproject.toml` or `requirements*.txt` or `Pipfile`
- Rust: `Cargo.toml`

### Runtime Version Heuristics
- Node: `.nvmrc` -> `.node-version` -> `package.json.engines.node`
- Python: `.python-version` -> `pyproject.toml project.requires-python`
- Rust: `rust-toolchain.toml toolchain.channel` -> `rust-toolchain`

### Package Manager Heuristics
- Node: `pnpm-lock.yaml` > `yarn.lock` > `package-lock.json` > `bun.lock*`
- Python: `uv.lock` > `poetry.lock` > `Pipfile*` > `requirements.txt`

### Service Detection
- parse compose files for keywords: `postgres`, `mysql/mariadb`, `redis`, `mongo`

## Acceptance Criteria

1. `devsync init` creates expected files for Node-only, Python-only, and mixed repos.
2. Generated `devsync.lock` parses and round-trips cleanly.
3. `devsync doctor` exit code follows `--fail-on` policy.
4. `devsync up` calls Dev Container CLI when installed.
5. CLI help output documents all commands and flags.
6. `devsync doctor --json` emits structured output for CI parsing.
7. `devsync policy` fails when detected stack runtime pins are missing/unpinned.
8. `devsync secret-lint` reports and fails on obvious secret-like assignments.
9. Registry operations append audit log events and expose them through `registry-audit`.
10. `devsync activate` outputs a readiness score and clear next actions.
11. `devsync roi` outputs deterministic monthly ROI fields and supports JSON mode.
12. `devsync dashboard-export` emits aggregated readiness + ROI JSON report.
13. Billing commands persist subscriptions/invoices/events and support deterministic cycle runs.

## Future Interface Extensions (Reserved)
- `devsync init --template <archetype>`
