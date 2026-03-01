#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-$HOME/Projects}"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$PROJECT_DIR/docs/reports"
STAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
REPORT_FILE="$REPORT_DIR/phase1-validation-$(date -u +%Y-%m-%d).md"

mkdir -p "$REPORT_DIR"

cd "$PROJECT_DIR"
cargo build --quiet
BIN="$PROJECT_DIR/target/debug/devsync"

rows_file="$(mktemp)"
cmd_log="$(mktemp)"
trap 'rm -f "$rows_file" "$cmd_log"' EXIT

while IFS= read -r git_dir; do
  repo="${git_dir%/.git}"

  start_ms="$(date +%s%3N)"

  if ! survey_json="$($BIN --path "$repo" survey --json 2>"$cmd_log")"; then
    end_ms="$(date +%s%3N)"
    duration_ms="$((end_ms - start_ms))"
    note="survey failed: $(tail -n 1 "$cmd_log" | sed 's/|/\\|/g')"
    printf '%s\terror\t-\t-\t-\tfail\t%s\t%s\n' "$repo" "$duration_ms" "$note" >>"$rows_file"
    continue
  fi

  stacks_csv="$(jq -r '.detected_stacks | if length == 0 then "-" else join(",") end' <<<"$survey_json")"
  primary_stack="$(jq -r '.primary_stack // "-"' <<<"$survey_json")"
  primary_run="$(jq -r '.primary_run_hint // "-"' <<<"$survey_json" | sed 's/|/\\|/g')"
  stack_count="$(jq -r '.detected_stacks | length' <<<"$survey_json")"

  if [[ "$stack_count" -gt 0 ]]; then
    scope="in-scope"

    if [[ -f "$repo/devsync.lock" ]]; then
      if $BIN --path "$repo" doctor --fail-on none >"$cmd_log" 2>&1; then
        validation="pass"
        note="existing lockfile"
      else
        validation="fail"
        note="existing lockfile invalid: $(tail -n 1 "$cmd_log" | sed 's/|/\\|/g')"
      fi
    else
      if $BIN --path "$repo" init --skip-devcontainer >"$cmd_log" 2>&1; then
        validation="pass"
        note="init ok (temp lock cleaned)"
        rm -f "$repo/devsync.lock"
      else
        validation="fail"
        note="init failed: $(tail -n 1 "$cmd_log" | sed 's/|/\\|/g')"
        if [[ -f "$repo/devsync.lock" ]]; then
          rm -f "$repo/devsync.lock"
        fi
      fi
    fi
  else
    scope="out-of-scope"
    validation="n/a"
    note="not Node/Python/Rust"
  fi

  end_ms="$(date +%s%3N)"
  duration_ms="$((end_ms - start_ms))"

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$repo" "$scope" "$stacks_csv" "$primary_stack" "$primary_run" "$validation" "$duration_ms" "$note" >>"$rows_file"
done < <(find "$ROOT_DIR" -mindepth 2 -maxdepth 2 -type d -name .git | sort)

total_repos="$(wc -l <"$rows_file" | tr -d ' ')"
in_scope_repos="$(awk -F'\t' '$2=="in-scope"{c++} END{print c+0}' "$rows_file")"
passed_repos="$(awk -F'\t' '$2=="in-scope" && $6=="pass"{c++} END{print c+0}' "$rows_file")"

if [[ "$in_scope_repos" -gt 0 ]]; then
  success_rate="$(awk -v p="$passed_repos" -v t="$in_scope_repos" 'BEGIN { printf "%.1f", (p/t)*100 }')"
else
  success_rate="0.0"
fi

phase1_gate="not-met"
if awk -v r="$success_rate" 'BEGIN { exit (r >= 70.0 ? 0 : 1) }'; then
  phase1_gate="met"
fi

{
  echo "# Phase 1 Validation Report"
  echo
  echo "- Generated at (UTC): $STAMP"
  echo "- Root scanned: \`$ROOT_DIR\`"
  echo "- Total repos scanned: $total_repos"
  echo "- In-scope repos: $in_scope_repos"
  echo "- In-scope pass count: $passed_repos"
  echo "- In-scope success rate: ${success_rate}%"
  echo "- Phase 1 >=70% gate: **$phase1_gate**"
  echo
  echo "## Repository Results"
  echo
  echo "| Repo | Scope | Stacks | Primary Stack | Primary Run | Validation | Duration (ms) | Notes |"
  echo "|---|---|---|---|---|---|---:|---|"

  while IFS=$'\t' read -r repo scope stacks primary_stack primary_run validation duration_ms note; do
    repo_display="${repo#$ROOT_DIR/}"
    echo "| \`$repo_display\` | $scope | \`$stacks\` | \`$primary_stack\` | \`$primary_run\` | $validation | $duration_ms | $note |"
  done <"$rows_file"
} >"$REPORT_FILE"

echo "Phase 1 report written to: $REPORT_FILE"
