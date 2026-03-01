#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$PROJECT_DIR/docs/reports"
STAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
REPORT_FILE="$REPORT_DIR/phase4-validation-$(date -u +%Y-%m-%d).md"

mkdir -p "$REPORT_DIR"

cd "$PROJECT_DIR"
cargo build --quiet
BIN="$PROJECT_DIR/target/debug/devsync"

TMP_ROOT="$(mktemp -d /tmp/devsync-phase4-validate-XXXXXX)"
ROOT="$TMP_ROOT/projects"
REPO="$ROOT/demo"
mkdir -p "$REPO/.git"
BILLING_ROOT="$TMP_ROOT/billing-store"

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

cat >"$REPO/Cargo.toml" <<'EOF'
[package]
name = "phase4-validate"
version = "0.1.0"
edition = "2021"
EOF

echo "1.79.0" >"$REPO/rust-toolchain"

status_init="fail"
status_policy="fail"
status_secret="fail"
status_activate="fail"
status_roi="fail"
status_dashboard="fail"
status_billing="fail"
notes=()

if $BIN --path "$REPO" init --force >/dev/null 2>&1; then
  status_init="pass"
  notes+=("init: pass")
else
  notes+=("init: fail")
fi

if $BIN --path "$REPO" policy >/dev/null 2>&1; then
  status_policy="pass"
  notes+=("policy: pass")
else
  notes+=("policy: fail")
fi

if $BIN --path "$REPO" secret-lint >/dev/null 2>&1; then
  status_secret="pass"
  notes+=("secret-lint: pass")
else
  notes+=("secret-lint: fail")
fi

activate_json="$($BIN --path "$REPO" activate --json || true)"
if [[ "$activate_json" == *'"ready": true'* ]]; then
  status_activate="pass"
  notes+=("activate: ready")
else
  notes+=("activate: not ready")
fi

roi_json="$($BIN roi --team-size 25 --hourly-rate 100 --price-per-dev 15 --json)"
roi_net="$(jq -r '.monthly_net_savings' <<<"$roi_json")"
roi_plan="$(jq -r '.recommended_plan' <<<"$roi_json")"
if awk -v value="$roi_net" 'BEGIN { exit (value > 0 ? 0 : 1) }'; then
  status_roi="pass"
  notes+=("roi: positive net savings ($roi_net), plan=$roi_plan")
else
  notes+=("roi: non-positive net savings ($roi_net), plan=$roi_plan")
fi

dashboard_out="$TMP_ROOT/dashboard.json"
$BIN dashboard-export \
  --root "$ROOT" \
  --output "$dashboard_out" \
  --team-size 25 \
  --hourly-rate 100 \
  --price-per-dev 15 >/dev/null

if [[ -f "$dashboard_out" ]] && jq -e '.repos_scanned >= 1 and .roi.monthly_net_savings != null' "$dashboard_out" >/dev/null; then
  status_dashboard="pass"
  notes+=("dashboard-export: pass ($dashboard_out)")
else
  notes+=("dashboard-export: fail")
fi

plans_json="$($BIN billing-plan-ls --billing "$BILLING_ROOT" --json)"
if [[ "$(jq '[.[] | select(.id=="team")] | length' <<<"$plans_json")" -ge 1 ]]; then
  notes+=("billing plans: seeded")
else
  notes+=("billing plans: missing default plan")
fi

$BIN billing-subscribe acme --plan team --seats 5 --billing "$BILLING_ROOT" --json >/dev/null
$BIN billing-cycle --billing "$BILLING_ROOT" --at 2099-01-01T00:00:00Z --json >/dev/null
invoices_json="$($BIN billing-invoice-ls --billing "$BILLING_ROOT" --org acme --json)"
invoice_count="$(jq 'length' <<<"$invoices_json")"
invoice_id="$(jq -r '.[0].id // empty' <<<"$invoices_json")"
if [[ "$invoice_count" -gt 0 && -n "$invoice_id" ]]; then
  $BIN billing-invoice-pay "$invoice_id" --billing "$BILLING_ROOT" --json >/dev/null
  events_json="$($BIN billing-events --billing "$BILLING_ROOT" --org acme --pending-only --json)"
  event_id="$(jq -r '.[0].id // empty' <<<"$events_json")"
  if [[ -n "$event_id" ]]; then
    $BIN billing-event-ack "$event_id" --billing "$BILLING_ROOT" --json >/dev/null
    status_billing="pass"
    notes+=("billing flow: pass (invoice created/paid + event ack)")
  else
    notes+=("billing flow: pending event missing")
  fi
else
  notes+=("billing flow: invoice creation failed")
fi

{
  echo "# Phase 4 Validation Report"
  echo
  echo "- Generated at (UTC): $STAMP"
  echo "- Temp root: \`$TMP_ROOT\`"
  echo
  echo "## Status"
  echo
  echo "| Check | Result |"
  echo "|---|---|"
  echo "| init | $status_init |"
  echo "| policy | $status_policy |"
  echo "| secret-lint | $status_secret |"
  echo "| activate | $status_activate |"
  echo "| roi | $status_roi |"
  echo "| dashboard-export | $status_dashboard |"
  echo "| billing backend slice | $status_billing |"
  echo
  echo "## Notes"
  echo
  for note in "${notes[@]}"; do
    echo "- $note"
  done
} >"$REPORT_FILE"

echo "Phase 4 report written to: $REPORT_FILE"
