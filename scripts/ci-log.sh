#!/usr/bin/env bash
# =============================================================================
# scripts/ci-log.sh — Centralized GitHub Actions / CI process logging helper
#
# Principles:
# 1. Two lines on main log:
#      [HH:MM:SS] ● <Title> ...
#      [HH:MM:SS] ✔ <Title>   (12.4s)
#    On error:
#      [HH:MM:SS] ✖ <Title>   (3.1s) exit=1
# 2. Raw stdout+stderr captured to build/logs/<slug>.log.
# 3. Output wrapped in ::group::<Title> (log) ... ::endgroup:: (closed by default).
# 4. On failure, 25 tail lines printed outside group + ::error annotation.
# 5. Preserves exact exit code.
# 6. Accumulates results in build/logs/summary.tsv for Markdown summary.
# =============================================================================

set +x

LOG_DIR="${REVANCEX_LOG_DIR:-build/logs}"
LOG_MODE="${LOG_MODE:-group}"

if [ -n "$NO_COLOR" ] || [ -n "$REVANCEX_ASCII" ]; then
  SYM_START="[..]"
  SYM_OK="[OK]"
  SYM_FAIL="[FAIL]"
  SYM_SKIP="[SKIP]"
  COLOR_START=""
  COLOR_OK=""
  COLOR_FAIL=""
  COLOR_RESET=""
else
  SYM_START="●"
  SYM_OK="✔"
  SYM_FAIL="✖"
  SYM_SKIP="↷"
  if [ -t 1 ] || [ -n "$GITHUB_ACTIONS" ]; then
    COLOR_START="\033[36m"
    COLOR_OK="\033[32m"
    COLOR_FAIL="\033[31m"
    COLOR_RESET="\033[0m"
  else
    COLOR_START=""
    COLOR_OK=""
    COLOR_FAIL=""
    COLOR_RESET=""
  fi
fi

ci_now() {
  date +"%H:%M:%S"
}

ci_now_ms() {
  local ns
  ns=$(date +%s%N 2>/dev/null || echo "")
  if [ -n "$ns" ] && [ "${#ns}" -ge 13 ]; then
    echo "$(( ns / 1000000 ))"
  else
    local s
    s=$(date +%s)
    echo "$(( s * 1000 ))"
  fi
}

ci_format_duration() {
  local start_ms="$1"
  local end_ms="$2"
  local diff_ms=$(( end_ms - start_ms ))
  if [ "$diff_ms" -lt 0 ]; then
    diff_ms=0
  fi

  local sec=$(( diff_ms / 1000 ))
  local tenth=$(( (diff_ms % 1000) / 100 ))

  if [ "$sec" -ge 60 ]; then
    local min=$(( sec / 60 ))
    local rem_sec=$(( sec % 60 ))
    printf "%dm %02ds" "$min" "$rem_sec"
  else
    printf "%d.%ds" "$sec" "$tenth"
  fi
}

slugify() {
  echo "$1" \
    | tr '[:upper:]' '[:lower:]' \
    | sed -e 's/[^a-z0-9._-]/-/g' -e 's/-\{2,\}/-/g' -e 's/^-//' -e 's/-$//'
}

step_mask() {
  local val="$1"
  if [ -n "$val" ] && [ "${#val}" -ge 4 ]; then
    if [ -n "$GITHUB_ACTIONS" ]; then
      echo "::add-mask::${val}"
    fi
  fi
}

for _var in GITHUB_TOKEN KEYSTORE_B64 KEYSTORE_PASSWORD TG_TOKEN; do
  if [ -n "${!_var:-}" ]; then
    step_mask "${!_var}"
  fi
done

step_run() {
  local title="$1"
  shift

  mkdir -p "$LOG_DIR"
  local slug
  slug=$(slugify "$title")
  local log_file="${LOG_DIR}/${slug}.log"

  printf "[%s] %b%s%b %s ...\n" \
    "$(ci_now)" "$COLOR_START" "$SYM_START" "$COLOR_RESET" "$title"

  local start_ms
  start_ms=$(ci_now_ms)

  local exit_code=0
  if [ "$LOG_MODE" = "stream" ]; then
    local pipefail_was_on=0
    shopt -qo pipefail && pipefail_was_on=1
    set -o pipefail
    (
      if [ "$#" -eq 1 ]; then
        eval "$1"
      else
        "$@"
      fi
    ) 2>&1 | tee "$log_file" || exit_code=$?
    if [ "$pipefail_was_on" -eq 0 ]; then
      set +o pipefail
    fi
  else
    (
      set -o pipefail
      if [ "$#" -eq 1 ]; then
        eval "$1"
      else
        "$@"
      fi
    ) > "$log_file" 2>&1 || exit_code=$?
  fi

  local end_ms
  end_ms=$(ci_now_ms)
  local dur_str
  dur_str=$(ci_format_duration "$start_ms" "$end_ms")

  if [ "$LOG_MODE" != "stream" ] && { [ "$LOG_MODE" = "group" ] || [ "$exit_code" -ne 0 ]; }; then
    if [ -n "$GITHUB_ACTIONS" ]; then
      echo "::group::${title} (log)"
      sed -e 's/::group::/── [group] /g' -e 's/::endgroup:://g' "$log_file"
      echo "::endgroup::"
    fi
  fi

  printf "%s\t%s\t%s\t%d\t%d\n" \
    "$([ "$exit_code" -eq 0 ] && echo "$SYM_OK" || echo "$SYM_FAIL")" \
    "$title" \
    "$dur_str" \
    "$exit_code" \
    "$(( end_ms - start_ms ))" \
    >> "${LOG_DIR}/summary.tsv"

  if [ "$exit_code" -eq 0 ]; then
    printf "[%s] %b%s%b %s   (%s)\n" \
      "$(ci_now)" "$COLOR_OK" "$SYM_OK" "$COLOR_RESET" "$title" "$dur_str"
  else
    printf "[%s] %b%s%b %s   (%s) exit=%d\n" \
      "$(ci_now)" "$COLOR_FAIL" "$SYM_FAIL" "$COLOR_RESET" "$title" "$dur_str" "$exit_code"

    local cause
    cause=$(grep -v '^[[:space:]]*$' "$log_file" | tail -n 1 | cut -c 1-200)
    [ -z "$cause" ] && cause="process exited with code ${exit_code}"

    if [ -n "$GITHUB_ACTIONS" ]; then
      echo "::error title=${title}::${cause}"
    fi

    echo "    ── last 25 lines (${log_file}) ──"
    tail -n 25 "$log_file" | sed 's/^/    │ /'
    echo "    complete logs: ${log_file}"
  fi

  return "$exit_code"
}

step_skip() {
  local title="$1"
  local reason="${2:-skipped}"
  mkdir -p "$LOG_DIR"
  printf "[%s] %s %s   (skipped: %s)\n" \
    "$(ci_now)" "$SYM_SKIP" "$title" "$reason"
  printf "%s\t%s\t%s\t%s\t0\n" \
    "$SYM_SKIP" "$title" "-" "skip" >> "${LOG_DIR}/summary.tsv"
}

step_summary() {
  local artifact_name="${1:-}"
  local summary_file="${LOG_DIR}/summary.tsv"

  if [ ! -f "$summary_file" ]; then
    return 0
  fi

  local total_ok=0
  local total_fail=0
  local total_skip=0
  local total_ms=0
  local failed_lines=""

  while IFS=$'\t' read -r status title dur exit_code ms; do
    [ -z "$title" ] && continue
    case "$status" in
      "$SYM_OK"|"✔"|"[OK]")
        total_ok=$(( total_ok + 1 ))
        ;;
      "$SYM_FAIL"|"✖"|"[FAIL]")
        total_fail=$(( total_fail + 1 ))
        failed_lines="${failed_lines}- \`${title}\` (${dur}, exit ${exit_code})\n"
        ;;
      "$SYM_SKIP"|"↷"|"[SKIP]")
        total_skip=$(( total_skip + 1 ))
        ;;
    esac
    if [ -n "$ms" ] && [ "$ms" -gt 0 ] 2>/dev/null; then
      total_ms=$(( total_ms + ms ))
    fi
  done < "$summary_file"

  local total_dur
  total_dur=$(ci_format_duration 0 "$total_ms")

  if [ -n "$GITHUB_STEP_SUMMARY" ]; then
    local md=""
    if [ -n "$failed_lines" ]; then
      md="${md}\n**Failed steps:**\n${failed_lines}"
    fi
    local counts="${total_ok} ok"
    [ "$total_skip" -gt 0 ] && counts="${counts}, ${total_skip} skipped"
    [ "$total_fail" -gt 0 ] && counts="${counts}, ${total_fail} failed"
    md="${md}\n**Steps:** ${counts} (${total_dur})"
    if [ -n "$artifact_name" ]; then
      md="${md} · **Logs:** \`${artifact_name}\`"
    fi
    md="${md}\n"
    printf "%b" "$md" >> "$GITHUB_STEP_SUMMARY"
  fi

  echo ""
  echo "═══════════════════════════════════════════════════════════"
  printf "Total: %d success, %d skipped, %d failed — %s\n" \
    "$total_ok" "$total_skip" "$total_fail" "$total_dur"
  echo "═══════════════════════════════════════════════════════════"
}
