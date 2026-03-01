# Phase 3 Validation Report

- Generated at (UTC): 2026-03-01T20:44:09Z
- Temp root: `/tmp/devsync-phase3-validate-7Iq6un`

## Status

| Check | Result |
|---|---|
| Policy (unpinned runtime) | expected-fail |
| Policy (pinned runtime) | pass |
| Secret lint | pass |
| Local registry + audit | pass |
| Remote auth token flow | pass |

## Notes

- init: ok
- policy without pin: expected fail
- policy with pin: pass
- secret-lint: pass
- local registry audit events: create/use/list found
- remote auth token: unauthorized denied and authorized allowed
