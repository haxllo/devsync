# Phase 3 Repository Validation Report

- Generated at (UTC): 2026-03-01T20:44:49Z
- Root scanned: `/home/dev/Projects`
- Total repos scanned: 7
- In-scope repos: 4
- Policy pass count (in-scope): 4
- Secret-lint pass count (in-scope): 4

## Repository Results

| Repo | Scope | Stacks | Init | Policy | Secret Lint | Remote Auth | Duration (ms) | Notes |
|---|---|---|---|---|---|---|---:|---|
| `Notepads` | out-of-scope | `-` | n/a | n/a | n/a | n/a | 17 | not Node/Python/Rust |
| `codex` | in-scope | `node` | pass | pass | pass | n/a | 54 | init lock-only |
| `devsync` | in-scope | `rust` | pass | pass | pass | n/a | 35 | init lock-only |
| `gemini-cli` | in-scope | `node` | pass | pass | pass | n/a | 104 | init full |
| `minios` | out-of-scope | `-` | n/a | n/a | n/a | n/a | 15 | not Node/Python/Rust |
| `nimbus` | out-of-scope | `-` | n/a | n/a | n/a | n/a | 15 | not Node/Python/Rust |
| `sch` | in-scope | `node,rust` | preexisting | pass | pass | n/a | 64 | ok |
