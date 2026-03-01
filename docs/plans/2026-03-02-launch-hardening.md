# Launch Hardening (Auth + Reliability) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add production-grade API authentication basics and minimal reliability controls for registry/billing HTTP services.

**Architecture:** Introduce a shared API key module used by both registry and billing servers. Keep existing single bearer token support as backward-compatible fallback. Add in-process per-key rate limiting and append-only access logging at the service root.

**Tech Stack:** Rust CLI (`clap`), TOML store, existing TCP HTTP handlers, `chrono`, `serde`.

---

### Task 1: Launch TODO and plan docs

**Files:**
- Create: `docs/TODO_LAUNCH.md`
- Create: `docs/plans/2026-03-02-launch-hardening.md`

### Task 2: Shared auth module

**Files:**
- Create: `src/auth.rs`
- Modify: `src/main.rs`

Add API key store model + helpers:
- create/list/revoke keys
- lookup by bearer token
- scope/org/expiry checks

### Task 3: Registry auth integration

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Modify: `src/registry.rs`

Add support for:
- `--auth-store` on `registry-serve`
- route scope checks (`registry.read`, `registry.write`, `registry.admin`)
- org-boundary enforcement

### Task 4: Billing auth integration

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Modify: `src/billing.rs`

Add support for:
- `--auth-store` on `billing-serve`
- route scope checks (`billing.read`, `billing.write`, `billing.admin`)
- org-boundary enforcement for org-scoped billing routes

### Task 5: Reliability controls

**Files:**
- Modify: `src/registry.rs`
- Modify: `src/billing.rs`

Implement:
- per-key rate limiting (requests/minute)
- access log append per request with status and route

### Task 6: API key CLI

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Modify: `README.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/PHASE4_IMPLEMENTATION.md`

Commands:
- `devsync auth-key-create`
- `devsync auth-key-ls`
- `devsync auth-key-revoke`

### Task 7: Tests and validation

**Files:**
- Modify: `src/auth.rs` (unit tests)
- Modify: `src/registry.rs` (auth/rate-limit tests)
- Modify: `src/billing.rs` (auth/rate-limit tests)

Run:
- `cargo test`

