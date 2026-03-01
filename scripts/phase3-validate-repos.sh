#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-$HOME/Projects}"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$PROJECT_DIR/docs/reports"
STAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
REPORT_FILE="$REPORT_DIR/phase3-repo-validation-$(date -u +%Y-%m-%d).md"

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
    printf '%s\terror\t-\t-\tfail\t-\t-\t%s\t%s\n' "$repo" "$duration_ms" "$note" >>"$rows_file"
    continue
  fi

  stacks_csv="$(jq -r '.detected_stacks | if length == 0 then "-" else join(",") end' <<<"$survey_json")"
  stack_count="$(jq -r '.detected_stacks | length' <<<"$survey_json")"

  if [[ "$stack_count" -eq 0 ]]; then
    end_ms="$(date +%s%3N)"
    duration_ms="$((end_ms - start_ms))"
    printf '%s\tout-of-scope\t%s\tn/a\tn/a\tn/a\tn/a\t%s\tnot Node/Python/Rust\n' \
      "$repo" "$stacks_csv" "$duration_ms" >>"$rows_file"
    continue
  fi

  scope="in-scope"
  init_status="skipped"
  policy_status="n/a"
  secret_status="n/a"
  note_parts=()

  existed_lock=false
  existed_devcontainer_dir=false
  existed_dc_json=false
  existed_dc_docker=false

  [[ -f "$repo/devsync.lock" ]] && existed_lock=true
  [[ -d "$repo/.devcontainer" ]] && existed_devcontainer_dir=true
  [[ -f "$repo/.devcontainer/devcontainer.json" ]] && existed_dc_json=true
  [[ -f "$repo/.devcontainer/Dockerfile" ]] && existed_dc_docker=true

  # Generate missing artifacts without overwriting existing files.
  if [[ "$existed_lock" == false || "$existed_dc_json" == false || "$existed_dc_docker" == false ]]; then
    if [[ "$existed_devcontainer_dir" == true && ( "$existed_dc_json" == true || "$existed_dc_docker" == true ) ]]; then
      if $BIN --path "$repo" init --skip-devcontainer >"$cmd_log" 2>&1; then
        init_status="pass"
        note_parts+=("init lock-only")
      else
        init_status="fail"
        note_parts+=("init failed: $(tail -n 1 "$cmd_log" | sed 's/|/\\|/g')")
      fi
    else
      if $BIN --path "$repo" init >"$cmd_log" 2>&1; then
        init_status="pass"
        note_parts+=("init full")
      else
        init_status="fail"
        note_parts+=("init failed: $(tail -n 1 "$cmd_log" | sed 's/|/\\|/g')")
      fi
    fi
  else
    init_status="preexisting"
  fi

  if $BIN --path "$repo" policy >"$cmd_log" 2>&1; then
    policy_status="pass"
  else
    policy_status="fail"
    note_parts+=("policy: $(tail -n 1 "$cmd_log" | sed 's/|/\\|/g')")
  fi

  if $BIN --path "$repo" secret-lint >"$cmd_log" 2>&1; then
    secret_status="pass"
  else
    secret_status="fail"
    note_parts+=("secret-lint: $(tail -n 1 "$cmd_log" | sed 's/|/\\|/g')")
  fi

  # Cleanup temporary artifacts created by this script.
  if [[ "$existed_lock" == false && -f "$repo/devsync.lock" ]]; then
    rm -f "$repo/devsync.lock"
  fi
  if [[ "$existed_dc_json" == false && -f "$repo/.devcontainer/devcontainer.json" ]]; then
    rm -f "$repo/.devcontainer/devcontainer.json"
  fi
  if [[ "$existed_dc_docker" == false && -f "$repo/.devcontainer/Dockerfile" ]]; then
    rm -f "$repo/.devcontainer/Dockerfile"
  fi
  if [[ "$existed_devcontainer_dir" == false && -d "$repo/.devcontainer" ]]; then
    rmdir "$repo/.devcontainer" 2>/dev/null || true
  fi

  end_ms="$(date +%s%3N)"
  duration_ms="$((end_ms - start_ms))"

  if [[ "${#note_parts[@]}" -eq 0 ]]; then
    notes="ok"
  else
    notes="$(IFS='; '; echo "${note_parts[*]}")"
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$repo" "$scope" "$stacks_csv" "$init_status" "$policy_status" "$secret_status" "n/a" "$duration_ms" "$notes" >>"$rows_file"
done < <(find "$ROOT_DIR" -mindepth 2 -maxdepth 2 -type d -name .git | sort)

total_repos="$(wc -l <"$rows_file" | tr -d ' ')"
in_scope_repos="$(awk -F'\t' '$2=="in-scope"{c++} END{print c+0}' "$rows_file")"
policy_pass_repos="$(awk -F'\t' '$2=="in-scope" && $5=="pass"{c++} END{print c+0}' "$rows_file")"
secret_pass_repos="$(awk -F'\t' '$2=="in-scope" && $6=="pass"{c++} END{print c+0}' "$rows_file")"

{
  echo "# Phase 3 Repository Validation Report"
  echo
  echo "- Generated at (UTC): $STAMP"
  echo "- Root scanned: \`$ROOT_DIR\`"
  echo "- Total repos scanned: $total_repos"
  echo "- In-scope repos: $in_scope_repos"
  echo "- Policy pass count (in-scope): $policy_pass_repos"
  echo "- Secret-lint pass count (in-scope): $secret_pass_repos"
  echo
  echo "## Repository Results"
  echo
  echo "| Repo | Scope | Stacks | Init | Policy | Secret Lint | Remote Auth | Duration (ms) | Notes |"
  echo "|---|---|---|---|---|---|---|---:|---|"

  while IFS=$'\t' read -r repo scope stacks init_status policy_status secret_status remote_auth duration_ms notes; do
    repo_display="${repo#$ROOT_DIR/}"
    echo "| \`$repo_display\` | $scope | \`$stacks\` | $init_status | $policy_status | $secret_status | $remote_auth | $duration_ms | $notes |"
  done <"$rows_file"
} >"$REPORT_FILE"

echo "Phase 3 repo report written to: $REPORT_FILE"
