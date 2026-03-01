# Phase 1 Validation Report

- Generated at (UTC): 2026-03-01T19:33:31Z
- Root scanned: `/home/dev/Projects`
- Total repos scanned: 7
- In-scope repos: 4
- In-scope pass count: 4
- In-scope success rate: 100.0%
- Phase 1 >=70% gate: **met**

## Repository Results

| Repo | Scope | Stacks | Primary Stack | Primary Run | Validation | Duration (ms) | Notes |
|---|---|---|---|---|---|---:|---|
| `Notepads` | out-of-scope | `-` | `-` | `-` | n/a | 20 | not Node/Python/Rust |
| `codex` | in-scope | `node` | `node` | `-` | pass | 24 | init ok (temp lock cleaned) |
| `devsync` | in-scope | `rust` | `rust` | `cargo run` | pass | 25 | init ok (temp lock cleaned) |
| `gemini-cli` | in-scope | `node` | `node` | `npm run start` | pass | 25 | init ok (temp lock cleaned) |
| `minios` | out-of-scope | `-` | `-` | `-` | n/a | 20 | not Node/Python/Rust |
| `nimbus` | out-of-scope | `-` | `-` | `-` | n/a | 20 | not Node/Python/Rust |
| `sch` | in-scope | `node,rust` | `rust` | `cargo run -p swiftfind-core` | pass | 24 | existing lockfile |
