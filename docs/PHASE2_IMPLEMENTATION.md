# Phase 2 Implementation Status

## Scope
Phase 2 target: move from local-only utility to a team-shareable environment registry.

## Delivered
- Registry version format implemented: `org/project@version`
- `devsync push` command implemented for publishing environment versions
- `devsync pull` command implemented for consuming environment versions
- `devsync registry-ls` command implemented for project version listing
- `devsync registry-serve` command implemented for HTTP access
- File-backed registry store implemented (default root: `~/.devsync/registry`)
- Role-based sharing implemented with `admin/member/viewer` bindings
- Prebuild cache pointer metadata supported per version
- `latest` version alias handling implemented for pull
- Pull integration with devcontainer regeneration (`--with-devcontainer`, `--primary-only`)
- Remote registry mode implemented for `push/pull/registry-ls` via `--registry-url`
- Unit tests added for parsing, push/pull roundtrip, and permission enforcement

## Registry Layout

```text
~/.devsync/registry/
  <org>/
    <project>/
      index.toml
      versions/
        <version>.toml
```

`index.toml` tracks:
- role bindings
- latest version pointer
- version metadata (creator, timestamp, prebuild cache)

`versions/<version>.toml` tracks:
- full environment payload including `devsync.lock`

## Command Examples

Publish:

```bash
devsync --path /path/to/repo push acme/api@v1 \
  --actor alice \
  --grant bob:viewer \
  --prebuild-cache s3://prebuilds/acme-api:v1
```

List:

```bash
devsync registry-ls acme/api --actor bob --json
```

Pull:

```bash
devsync --path /path/to/consumer pull acme/api@latest \
  --actor bob \
  --with-devcontainer \
  --primary-only
```

## Validation Summary
- `cargo test` passing with registry tests included.
- CLI help verified for `push`, `pull`, `registry-ls`.
- End-to-end smoke test executed with temporary registry root in `/tmp`.
- End-to-end remote HTTP smoke test executed through `registry-serve`.

## Caveats
- Current registry backend is file-based, designed for immediate team workflows and pilot validation.
- Remote hosted API and multi-tenant auth are intentionally deferred to next iteration.
- HTTP transport currently supports plain `http://` (TLS termination should be handled by reverse proxy in production).
