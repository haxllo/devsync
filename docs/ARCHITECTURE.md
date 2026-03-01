# DevSync Architecture (MVP)

## System Overview
DevSync v0.1 is a local-first CLI with deterministic output generation.

Data flow:
1. `detect`: inspect repo manifests and lockfiles
2. `lockfile`: normalize detected signals into `devsync.lock`
3. `devcontainer`: generate baseline runtime container files
4. `doctor`: verify host tooling and runtime compatibility
5. `policy`: enforce governance checks (runtime pinning + base-image allowlist)
6. `secrets`: lint generated artifacts for likely secret leakage
7. `activation`: calculate readiness score and next actions
8. `roi`: compute pilot/business ROI estimate
9. `dashboard`: aggregate multi-repo activation + ROI reporting
10. `billing`: file-backed plans/subscriptions/invoices/events + HTTP API
11. `registry`: publish/pull/list/audit environment versions
12. `up`: delegate environment startup to existing container tooling

## Module Map

- `src/cli.rs`
  - command/flag schema
- `src/detect.rs`
  - repository signal detection and heuristics
- `src/lockfile.rs`
  - lockfile schema + read/write utilities
- `src/devcontainer.rs`
  - generated `.devcontainer` artifacts
- `src/doctor.rs`
  - runtime/toolchain health diagnostics
- `src/policy.rs`
  - policy loading + base-image/runtime pinning enforcement
- `src/secrets.rs`
  - generated artifact secret-lint checks
- `src/activation.rs`
  - activation checklist/report generation
- `src/roi.rs`
  - deterministic ROI model for sales/pilot calculations
- `src/dashboard.rs`
  - root-level repository aggregation for GTM dashboard JSON export
- `src/billing.rs`
  - self-serve billing backend slice and local billing HTTP API
- `src/up.rs`
  - launch flow for dev environment startup
- `src/main.rs`
  - command orchestration and output
- `src/registry.rs`
  - phase-2/3 team registry backend, audit logging, HTTP auth token support

## Key Design Decisions

1. Rules-first detection engine.
- deterministic and testable
- no AI dependency in core path

2. Repo-scoped reproducibility instead of machine cloning.
- lower complexity
- faster path to reliable outcomes

3. Piggyback on Dev Containers + Docker.
- improves adoption
- reduces runtime orchestration burden

4. Human-readable lockfile in TOML.
- easy code review
- versionable in Git

5. File-backed audit log for registry activity.
- append-only JSON-lines (`audit.log`)
- records create/update/use/list events per project

6. Token-based auth hook for remote registry.
- optional bearer token required by `registry-serve`
- enables SSO-proxied deployments without coupling CLI to a specific IdP SDK

## Security and Privacy Defaults

- no secret value extraction
- scan only file metadata and known manifests
- no telemetry in v0.1

## Compatibility Policy

v0.1 supported target:
- Linux host
- WSL2 host path support
- Node/Python/Rust repositories

Not guaranteed in v0.1:
- native Windows runtime setup
- macOS-specific package managers
- GPU driver validation

## Evolution Path

v0.2:
- JSON output for doctor command
- richer template generation by project archetype
- fixture-based detection test matrix

v0.3:
- hosted environment registry API
- authenticated project environment push/pull
- policy checks for base image + runtime pinning

Current phase-2 implementation:
- file-backed shared registry (`~/.devsync/registry`)
- versioned env publishing (`org/project@version`)
- role-bound access (`admin/member/viewer`)
- optional HTTP transport via built-in `registry-serve` command

Current phase-3 implementation:
- `devsync policy` for runtime and base-image governance checks
- `devsync secret-lint` for generated artifact leakage checks
- append-only registry audit events exposed by `devsync registry-audit`
- optional remote bearer-token enforcement (`--auth-token`, `DEVSYNC_AUTH_TOKEN`)

Current phase-4 slice:
- `devsync activate` readiness scoring and next-action reporting
- `devsync roi` commercial value model for onboarding + drift reduction scenarios
- `devsync dashboard-export` for aggregated GTM reporting
- billing command/API slice (`billing-*`, `billing-serve`) with webhook-ready event outbox
