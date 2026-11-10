#!/usr/bin/env bash
set -e
export REVANCEX_LOG_DIR="build/test_logs"
rm -rf "$REVANCEX_LOG_DIR"

source scripts/ci-log.sh

echo "=== Test 1: Successful step_run ==="
step_run "Merge reddit-arm64-v8a" echo "Merging reddit dex and resources"

echo "=== Test 2: Multi-word command with pipe ==="
step_run "Resolve sources" "echo -e 'source1\nsource2' | grep source1"

echo "=== Test 3: Skipped step ==="
step_skip "Build single module microg" "microg does not need single module"

echo "=== Test 4: Failing step (must preserve exit code) ==="
set +e
step_run "Sign bad-app" bash -c 'echo "Error: Key entry not found"; echo "Details: Alias abc does not exist"; exit 42'
rc=$?
set -e
if [ "$rc" -ne 42 ]; then
  echo "FAIL: expected exit code 42, got $rc"
  exit 1
fi
echo "Successfully captured exit code $rc"

echo "=== Test 5: Summary rendering ==="
export GITHUB_STEP_SUMMARY="${REVANCEX_LOG_DIR}/summary.md"
step_summary "build-logs-build-arm64-v8a"

echo "=== Summary TSV Content ==="
cat "${REVANCEX_LOG_DIR}/summary.tsv"

echo "=== Summary Markdown Content ==="
cat "${REVANCEX_LOG_DIR}/summary.md"

echo "=== Test 6: ASCII mode ==="
export NO_COLOR=1
source scripts/ci-log.sh
step_run "Download apk" echo "Downloaded apk"

echo "=== Test 7: GitHub Actions annotations & groups ==="
unset NO_COLOR
export GITHUB_ACTIONS=true
source scripts/ci-log.sh
step_run "Group test" echo "Inside group line 1"
set +e
step_run "Group fail test" bash -c 'echo "Group failure reason"; exit 2'
set -e

echo "=== Test 8: Stream mode ==="
export LOG_MODE="stream"
step_run "Stream test" echo "Live progress line"
set +e
step_run "Stream fail test" bash -c 'echo "Stream failure"; exit 7'
rc=$?
set -e
if [ "$rc" -ne 7 ]; then
  echo "FAIL: expected stream exit code 7, got $rc"
  exit 1
fi
grep -q "Live progress line" "${REVANCEX_LOG_DIR}/stream-test.log"

echo "ALL CI-LOG TESTS PASSED!"
