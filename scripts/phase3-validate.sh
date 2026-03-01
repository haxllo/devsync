#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$PROJECT_DIR/docs/reports"
STAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
REPORT_FILE="$REPORT_DIR/phase3-validation-$(date -u +%Y-%m-%d).md"
WITH_REMOTE="${1:-}"

mkdir -p "$REPORT_DIR"

cd "$PROJECT_DIR"
cargo build --quiet
BIN="$PROJECT_DIR/target/debug/devsync"

TMP_ROOT="$(mktemp -d /tmp/devsync-phase3-validate-XXXXXX)"
REPO="$TMP_ROOT/repo"
REGISTRY="$TMP_ROOT/registry"
CONSUMER="$TMP_ROOT/consumer"
REMOTE_REGISTRY="$TMP_ROOT/remote-registry"
TOKEN="phase3token"

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

mkdir -p "$REPO" "$REGISTRY" "$CONSUMER" "$REMOTE_REGISTRY"

cat >"$REPO/Cargo.toml" <<'EOF'
[package]
name = "phase3-validate"
version = "0.1.0"
edition = "2021"
EOF

status_policy_unpinned="fail"
status_policy_pinned="fail"
status_secret_lint="fail"
status_local_registry="fail"
status_remote_auth="skipped"
notes=()

if $BIN --path "$REPO" init --force >/dev/null 2>&1; then
  notes+=("init: ok")
else
  notes+=("init: failed")
fi

if $BIN --path "$REPO" policy >/dev/null 2>&1; then
  status_policy_unpinned="unexpected-pass"
  notes+=("policy without pin: expected fail but passed")
else
  status_policy_unpinned="expected-fail"
  notes+=("policy without pin: expected fail")
fi

echo "1.79.0" >"$REPO/rust-toolchain"
$BIN --path "$REPO" lock --force >/dev/null 2>&1

if $BIN --path "$REPO" policy >/dev/null 2>&1; then
  status_policy_pinned="pass"
  notes+=("policy with pin: pass")
else
  notes+=("policy with pin: failed")
fi

if $BIN --path "$REPO" secret-lint >/dev/null 2>&1; then
  status_secret_lint="pass"
  notes+=("secret-lint: pass")
else
  notes+=("secret-lint: failed")
fi

$BIN --path "$REPO" push acme/phase3@v1 --registry "$REGISTRY" --actor alice --grant bob:viewer --force >/dev/null
$BIN --path "$CONSUMER" pull acme/phase3@latest --registry "$REGISTRY" --actor bob --force >/dev/null
$BIN registry-ls acme/phase3 --registry "$REGISTRY" --actor bob >/dev/null
audit_json="$($BIN registry-audit acme/phase3 --registry "$REGISTRY" --actor alice --json)"

if [[ "$audit_json" == *"environment.create"* && "$audit_json" == *"environment.use"* && "$audit_json" == *"environment.list"* ]]; then
  status_local_registry="pass"
  notes+=("local registry audit events: create/use/list found")
else
  notes+=("local registry audit events: missing expected actions")
fi

if [[ "$WITH_REMOTE" == "--with-remote" ]]; then
  status_remote_auth="fail"

  (cargo run -- registry-serve --bind 127.0.0.1:8791 --registry "$REMOTE_REGISTRY" --auth-token "$TOKEN" --once >/dev/null 2>&1 &) 
  sleep 0.5
  $BIN --path "$REPO" push acme/remote@v1 --registry-url http://127.0.0.1:8791 --actor alice --grant bob:viewer --auth-token "$TOKEN" --force >/dev/null
  wait

  (cargo run -- registry-serve --bind 127.0.0.1:8792 --registry "$REMOTE_REGISTRY" --auth-token "$TOKEN" --once >/dev/null 2>&1 &) 
  sleep 0.5
  set +e
  $BIN registry-ls acme/remote --registry-url http://127.0.0.1:8792 --actor bob >/dev/null 2>&1
  denied_status=$?
  set -e
  wait

  (cargo run -- registry-serve --bind 127.0.0.1:8793 --registry "$REMOTE_REGISTRY" --auth-token "$TOKEN" --once >/dev/null 2>&1 &) 
  sleep 0.5
  $BIN registry-ls acme/remote --registry-url http://127.0.0.1:8793 --actor bob --auth-token "$TOKEN" >/dev/null
  wait

  if [[ "$denied_status" -ne 0 ]]; then
    status_remote_auth="pass"
    notes+=("remote auth token: unauthorized denied and authorized allowed")
  else
    notes+=("remote auth token: unauthorized request unexpectedly succeeded")
  fi
else
  notes+=("remote auth: skipped (run with --with-remote)")
fi

{
  echo "# Phase 3 Validation Report"
  echo
  echo "- Generated at (UTC): $STAMP"
  echo "- Temp root: \`$TMP_ROOT\`"
  echo
  echo "## Status"
  echo
  echo "| Check | Result |"
  echo "|---|---|"
  echo "| Policy (unpinned runtime) | $status_policy_unpinned |"
  echo "| Policy (pinned runtime) | $status_policy_pinned |"
  echo "| Secret lint | $status_secret_lint |"
  echo "| Local registry + audit | $status_local_registry |"
  echo "| Remote auth token flow | $status_remote_auth |"
  echo
  echo "## Notes"
  echo
  for note in "${notes[@]}"; do
    echo "- $note"
  done
} >"$REPORT_FILE"

echo "Phase 3 report written to: $REPORT_FILE"
