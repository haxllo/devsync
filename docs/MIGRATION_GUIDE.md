# Migration Guide: Existing DevContainers/Nix/Docker To DevSync

## Goal
Adopt DevSync incrementally without breaking existing team workflows.

## Step 1: Baseline Detection
Run:

```bash
devsync --path /path/to/repo survey --json
```

Review detected stacks/runtimes/services before writing artifacts.

## Step 2: Generate Lockfile First
Run:

```bash
devsync --path /path/to/repo init --skip-devcontainer
```

Commit `devsync.lock` and validate with `devsync doctor`.

## Step 3: Add Governance Checks
Run:

```bash
devsync --path /path/to/repo policy
devsync --path /path/to/repo secret-lint
```

Tune `devsync.policy.toml` if needed during migration.

## Step 4: Generate/Adopt DevContainer
If you already have custom `.devcontainer/*`, keep it and run lock-only flows.
If you want DevSync-generated devcontainer:

```bash
devsync --path /path/to/repo init --force
```

## Step 5: Team Registry Rollout
- Publish first shared environment:
  - `devsync push org/repo@v1 --actor <name>`
- Teammate pull and validate:
  - `devsync --path /path/to/consumer pull org/repo@latest --actor <name>`

## Step 6: CI and Activation
- Add `doctor`, `policy`, and `secret-lint` into CI.
- Run `devsync activate` periodically for onboarding health checks.
