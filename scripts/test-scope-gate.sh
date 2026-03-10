#!/usr/bin/env bash
# test-scope-gate.sh — Exercise scope-gate.sh against real-world scenarios
#
# Runs the scope gate with simulated PR data and asserts the verdict.
# Uses the actual script — no mocking.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GATE="${SCRIPT_DIR}/scope-gate.sh"
PASS=0
FAIL=0
TOTAL=0

run_test() {
  local name="$1"
  local expected_verdict="$2"
  # env vars PR_BODY, ADDITIONS, DELETIONS, CHANGED_FILES must be set by caller
  TOTAL=$((TOTAL + 1))

  local output verdict exit_code
  set +e
  output=$(bash "$GATE" 2>&1)
  exit_code=$?
  set -e

  # Extract verdict from output
  verdict=$(echo "$output" | grep '^Verdict:' | awk '{print $2}')

  if [ "$verdict" = "$expected_verdict" ]; then
    echo "  ✅ ${name} → ${verdict}"
    PASS=$((PASS + 1))
  else
    echo "  ❌ ${name} → expected '${expected_verdict}', got '${verdict}' (exit=${exit_code})"
    echo "     Output: $(echo "$output" | head -5)"
    FAIL=$((FAIL + 1))
  fi
}

echo "━━━ Scope Gate Tests ━━━"
echo ""

# ─── Test 1: Clean, focused PR ───────────────────────────
echo "1. Clean, focused PR (source + tests, small, has issue ref)"
export PR_BODY="Implements test improvements

Closes #42"
export ADDITIONS=200
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
crates/pi-daemon-api/src/server.rs
crates/pi-daemon-api/tests/api_integration.rs
Cargo.lock"
run_test "focused source+tests PR" "pass"

# ─── Test 2: No issue reference ──────────────────────────
echo ""
echo "2. Missing issue reference"
export PR_BODY="Some changes without an issue reference"
export ADDITIONS=50
export DELETIONS=10
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
run_test "no issue ref" "block"

# ─── Test 3: Too large (>1500 lines) ─────────────────────
echo ""
echo "3. Oversized PR (>1500 lines)"
export PR_BODY="Closes #99"
export ADDITIONS=1200
export DELETIONS=400
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
crates/pi-daemon-api/src/server.rs"
run_test "1600 lines blocks" "block"

# ─── Test 4: Warning zone (800-1500 lines) ───────────────
echo ""
echo "4. Warning zone (800-1500 lines)"
export PR_BODY="Closes #99"
export ADDITIONS=600
export DELETIONS=300
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
crates/pi-daemon-api/src/server.rs"
run_test "900 lines warns" "warn"

# ─── Test 5: 4+ workstreams → block ──────────────────────
echo ""
echo "5. Too many workstreams (4+)"
export PR_BODY="Closes #50"
export ADDITIONS=200
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
crates/pi-daemon-api/tests/api_integration.rs
.github/workflows/ci.yml
docs/Architecture.md
.github/pull_request_template.md
scripts/test-local.sh"
run_test "4 workstreams blocks" "block"

# ─── Test 6: 3 workstreams at 500+ lines → warn ─────────
echo ""
echo "6. Three workstreams at 500+ lines"
export PR_BODY="Closes #50"
export ADDITIONS=400
export DELETIONS=200
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
.github/workflows/ci.yml
docs/Architecture.md
Cargo.toml"
run_test "3 workstreams 600 lines warns" "warn"

# ─── Test 7: 3 workstreams under 500 lines → pass ───────
echo ""
echo "7. Three workstreams but small (<500 lines)"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
.github/workflows/ci.yml
docs/Architecture.md"
run_test "3 workstreams 150 lines passes" "pass"

# ─── Test 8: source + test-code merge as one ─────────────
echo ""
echo "8. Source + test-code count as one workstream"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
crates/pi-daemon-api/src/server.rs
crates/pi-daemon-api/tests/api_integration.rs
crates/pi-daemon-kernel/tests/kernel_lifecycle.rs
crates/pi-daemon-kernel/src/kernel.rs"
run_test "source+tests = 1 workstream" "pass"

# ─── Test 9: test-infra alone ────────────────────────────
echo ""
echo "9. Test-utils crate is its own workstream (test-infra)"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-test-utils/src/client.rs
crates/pi-daemon-test-utils/src/server.rs
crates/pi-daemon-api/tests/api_integration.rs
crates/pi-daemon-api/src/routes.rs"
run_test "test-infra + source+tests = 2 workstreams" "pass"

# ─── Test 10: deps + changelog never counted ─────────────
echo ""
echo "10. Deps and changelog don't count as workstreams"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
Cargo.toml
Cargo.lock
CHANGELOG.md"
run_test "deps+changelog ignored" "pass"

# ─── Test 11: The PR #117 scenario ──────────────────────
echo ""
echo "11. Real-world: PR #117 scenario (22 files, 5 workstreams, 1877 lines)"
export PR_BODY="Closes #116"
export ADDITIONS=1877
export DELETIONS=662
export CHANGED_FILES="crates/pi-daemon-test-utils/src/server.rs
crates/pi-daemon-test-utils/src/client.rs
crates/pi-daemon-test-utils/src/macros.rs
crates/pi-daemon-test-utils/src/lib.rs
crates/pi-daemon-test-utils/tests/integration.rs
crates/pi-daemon-api/tests/api_integration.rs
crates/pi-daemon-api/tests/openai_integration.rs
crates/pi-daemon-api/tests/webchat_integration.rs
crates/pi-daemon-api/tests/websocket_integration.rs
crates/pi-daemon-api/src/routes.rs
crates/pi-daemon-kernel/tests/kernel_lifecycle.rs
crates/pi-daemon-kernel/src/kernel.rs
.github/workflows/code-review.yml
.github/workflows/ci.yml
.github/workflows/sandbox-test.yml
docs/Testing.md
docs/PR-Reviews.md
docs/Contributing.md
.github/pull_request_template.md
scripts/test-local.sh
Cargo.toml
CHANGELOG.md"
run_test "PR #117 blocks (size + workstreams)" "block"

# ─── Test 12: Fixes/Refs variants ────────────────────────
echo ""
echo "12. Issue reference variants"

export PR_BODY="Fixes #10"
export ADDITIONS=50
export DELETIONS=10
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
run_test "Fixes #N accepted" "pass"

export PR_BODY="Refs #10"
run_test "Refs #N accepted" "pass"

export PR_BODY="closes #10"
run_test "lowercase closes accepted" "pass"

export PR_BODY="Implements #10"
run_test "Implements #N accepted" "pass"

# ─── Test 13: Empty PR ──────────────────────────────────
echo ""
echo "13. Edge case: empty/minimal PR"
export PR_BODY=""
export ADDITIONS=0
export DELETIONS=0
export CHANGED_FILES=""
run_test "empty PR blocks (no issue ref)" "block"

# ─── Test 14: Only deps changes ─────────────────────────
echo ""
echo "14. Only deps changes (Cargo.toml + Cargo.lock)"
export PR_BODY="Closes #77"
export ADDITIONS=50
export DELETIONS=10
export CHANGED_FILES="Cargo.toml
Cargo.lock"
run_test "deps-only PR passes" "pass"

# ─── Test 15: Exactly at thresholds ──────────────────────
echo ""
echo "15. Boundary: exactly 800 lines (should pass, not warn)"
export PR_BODY="Closes #10"
export ADDITIONS=500
export DELETIONS=300
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
run_test "exactly 800 lines passes" "pass"

echo ""
echo "16. Boundary: exactly 801 lines (should warn)"
export PR_BODY="Closes #10"
export ADDITIONS=500
export DELETIONS=301
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
run_test "801 lines warns" "warn"

echo ""
echo "17. Boundary: exactly 1500 lines (should warn, not block)"
export PR_BODY="Closes #10"
export ADDITIONS=1000
export DELETIONS=500
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
run_test "exactly 1500 lines warns" "warn"

echo ""
echo "18. Boundary: exactly 1501 lines (should block)"
export PR_BODY="Closes #10"
export ADDITIONS=1000
export DELETIONS=501
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
run_test "1501 lines blocks" "block"

# ─── Test 19: Issue ref embedded in longer text ──────────
echo ""
echo "19. Issue ref buried in long PR body"
export PR_BODY="This is a long description.

## What changed
Lots of things.

## Why
Because reasons.

Closes #42

Some trailing text."
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
run_test "issue ref in long body" "pass"

# ─── Test 20: Multiple issue refs (takes first) ─────────
echo ""
echo "20. Multiple issue refs (takes first)"
export PR_BODY="Closes #42
Also refs #43"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
run_test "multiple refs takes first" "pass"

# ─── Test 21: Files in nested src dirs ───────────────────
echo ""
echo "21. Nested source directories"
export PR_BODY="Closes #10"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/handlers/auth.rs
crates/pi-daemon-api/src/middleware.rs"
run_test "nested src dirs categorized as source" "pass"

# ─── Test 22: Unknown file types fall to 'other' ────────
echo ""
echo "22. Unknown files categorized as 'other'"
export PR_BODY="Closes #10"
export ADDITIONS=50
export DELETIONS=10
export CHANGED_FILES="README.md
.gitignore
deny.toml"
run_test "unknown files as other workstream" "pass"

# ─── Test 23: source + tests + test-infra = 2, not 3 ────
echo ""
echo "23. source + tests + test-infra = 2 workstreams (source+tests merged)"
export PR_BODY="Closes #10"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
crates/pi-daemon-api/tests/api_integration.rs
crates/pi-daemon-test-utils/src/client.rs"
run_test "source+tests+test-infra = 2 workstreams" "pass"

# ─── Test 24: Block wins over warn ───────────────────────
echo ""
echo "24. Block from issue ref + warn from size = block"
export PR_BODY="No issue ref here"
export ADDITIONS=600
export DELETIONS=300
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
run_test "block overrides warn" "block"

# ═══════════════════════════════════════════════════════════
# Phase 2 Tests: Issue Alignment Validation
# ═══════════════════════════════════════════════════════════

echo ""
echo "━━━ Phase 2: Issue Scope Detection (Check 4) ━━━"

# ─── Test 25: 3+ pillars → block ─────────────────────────
echo ""
echo "25. Issue with 3 pillars → block"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Big refactor"
export ISSUE_BODY_TEXT="## Overview

### Pillar 1: Testing
Fix tests.

### Pillar 2: CI
Fix CI.

### Pillar 3: Docs
Fix docs."
run_test "3 pillars blocks" "block"

# ─── Test 26: 5 phases → block ──────────────────────────
echo ""
echo "26. Issue with 5 phases → block"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Multi-phase rollout"
export ISSUE_BODY_TEXT="# Rollout plan

## Phase 1: prep
Do prep.

## Phase 2: build
Build it.

## Phase 3: test
Test it.

## Phase 4: deploy
Deploy it.

## Phase 5: monitor
Monitor it."
run_test "5 phases blocks" "block"

# ─── Test 27: 2 pillars → pass (below threshold) ────────
echo ""
echo "27. Issue with 2 pillars → pass (below threshold)"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Small change"
export ISSUE_BODY_TEXT="## Overview

### Pillar 1: Testing
Fix tests.

### Pillar 2: CI
Fix CI."
run_test "2 pillars passes" "pass"

# ─── Test 28: 16 ACs across 6 sections → warn ───────────
echo ""
echo "28. Issue with 16 ACs across 6 sections → warn"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Broad issue"
export ISSUE_BODY_TEXT="## API Routes
- [ ] AC 1
- [ ] AC 2
- [ ] AC 3

## Error Handling
- [ ] AC 4
- [ ] AC 5
- [ ] AC 6

## Middleware
- [ ] AC 7
- [ ] AC 8
- [ ] AC 9

## WebSocket
- [ ] AC 10
- [ ] AC 11
- [ ] AC 12

## Authentication
- [ ] AC 13
- [ ] AC 14
- [ ] AC 15

## Logging
- [ ] AC 16"
run_test "16 ACs 6 sections warns" "warn"

# ─── Test 29: 15 ACs across 5 sections → pass (boundary) ─
echo ""
echo "29. Issue with 15 ACs across 5 sections → pass (at boundary, not over)"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Focused issue"
export ISSUE_BODY_TEXT="## API Routes
- [ ] AC 1
- [ ] AC 2
- [ ] AC 3

## Error Handling
- [ ] AC 4
- [ ] AC 5
- [ ] AC 6

## Middleware
- [ ] AC 7
- [ ] AC 8
- [ ] AC 9

## WebSocket
- [ ] AC 10
- [ ] AC 11
- [ ] AC 12

## Authentication
- [ ] AC 13
- [ ] AC 14
- [ ] AC 15"
run_test "15 ACs 5 sections passes" "pass"

echo ""
echo "━━━ Phase 2: Workstream vs Issue Alignment (Check 5) ━━━"

# ─── Test 30: Workflow mismatch → warn ───────────────────
echo ""
echo "30. PR touches workflows but issue doesn't mention CI → warn"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES=".github/workflows/ci.yml
crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Fix API routes"
export ISSUE_BODY_TEXT="Fix the broken API routes in the daemon."
run_test "workflow mismatch warns" "warn"

# ─── Test 31: Workflow match → pass ──────────────────────
echo ""
echo "31. PR touches workflows and issue mentions CI → pass"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES=".github/workflows/ci.yml
crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Fix CI workflow"
export ISSUE_BODY_TEXT="The CI workflow is broken, fix it."
run_test "workflow match passes" "pass"

# ─── Test 32: Docs mismatch → warn ──────────────────────
echo ""
echo "32. PR touches docs but issue doesn't mention docs → warn"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="docs/Architecture.md
crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Fix API routes"
export ISSUE_BODY_TEXT="Fix the broken API routes in the daemon."
run_test "docs mismatch warns" "warn"

# ─── Test 33: Docs match → pass ─────────────────────────
echo ""
echo "33. PR touches docs and issue mentions documentation → pass"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="docs/Architecture.md
crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Update docs and routes"
export ISSUE_BODY_TEXT="Update the documentation for the new API routes."
run_test "docs match passes" "pass"

# ─── Test 34: Template mismatch → warn ──────────────────
echo ""
echo "34. PR touches PR template but issue doesn't mention templates → warn"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES=".github/pull_request_template.md
crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Fix API routes"
export ISSUE_BODY_TEXT="Fix the broken API routes."
run_test "template mismatch warns" "warn"

# ─── Test 35: Scripts mismatch → warn ───────────────────
echo ""
echo "35. PR touches scripts but issue doesn't mention scripts → warn"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="scripts/deploy.sh
crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Fix API routes"
export ISSUE_BODY_TEXT="Fix the broken API routes."
run_test "scripts mismatch warns" "warn"

# ─── Test 36: Scripts match (mentions scope gate) → pass ─
echo ""
echo "36. PR touches scripts and issue mentions scope gate → pass"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="scripts/scope-gate.sh
scripts/test-scope-gate.sh"
export ISSUE_TITLE="Scope Gate Phase 2"
export ISSUE_BODY_TEXT="Extend the scope gate script with alignment checks."
run_test "scripts match passes" "pass"

# ─── Test 37: Test-infra mismatch → warn ────────────────
echo ""
echo "37. PR touches test-utils but issue doesn't mention test infra → warn"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-test-utils/src/client.rs
crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Fix API routes"
export ISSUE_BODY_TEXT="Fix the broken API routes."
run_test "test-infra mismatch warns" "warn"

# ─── Test 38: No issue body → Phase 2 skipped ───────────
echo ""
echo "38. No issue body → Phase 2 checks skipped gracefully"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES=".github/workflows/ci.yml
crates/pi-daemon-api/src/routes.rs"
unset ISSUE_TITLE
unset ISSUE_BODY_TEXT
run_test "no issue body skips Phase 2" "pass"

# ─── Test 39: Phase 1 block + Phase 2 block stack ───────
echo ""
echo "39. Phase 1 size block + Phase 2 pillar block stack"
export PR_BODY="Closes #50"
export ADDITIONS=1200
export DELETIONS=400
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Mega issue"
export ISSUE_BODY_TEXT="## Phase 1: prep
Do prep.
## Phase 2: build
Build it.
## Phase 3: test
Test it."
run_test "Phase 1+2 blocks stack" "block"

# ─── Test 40: Real-world PR #117 scenario ────────────────
echo ""
echo "40. Real-world: PR #117 with full issue body (multi-pillar + size)"
export PR_BODY="Closes #116"
export ADDITIONS=1877
export DELETIONS=662
export CHANGED_FILES="crates/pi-daemon-test-utils/src/server.rs
crates/pi-daemon-test-utils/src/client.rs
crates/pi-daemon-api/tests/api_integration.rs
crates/pi-daemon-api/src/routes.rs
.github/workflows/code-review.yml
docs/Testing.md
.github/pull_request_template.md
scripts/test-local.sh"
export ISSUE_TITLE="Testing suite revamp — enhanced tests, LLM prompt engineering, self-updating PR template"
export ISSUE_BODY_TEXT="# Testing Suite Revamp

## Pillar 1: Testing Suite Overhaul
Rewrite all tests.
- [ ] AC 1
- [ ] AC 2

## Pillar 2: LLM Review Prompt Engineering
Rewrite prompts.
- [ ] AC 3
- [ ] AC 4

## Pillar 3: Self-Updating PR Template
Auto-update template.
- [ ] AC 5
- [ ] AC 6"
run_test "PR #117 multi-block (size + pillars)" "block"

# ─── Test 41: Step keyword detected as pillar ────────────
echo ""
echo "41. Issue with 3 steps → block"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Multi-step"
export ISSUE_BODY_TEXT="## Step 1: prepare
Prepare.

## Step 2: execute
Execute.

## Step 3: verify
Verify."
run_test "3 steps blocks" "block"

# ─── Test 42: Section keyword in heading ─────────────────
echo ""
echo "42. Issue with 4 sections → block"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export ISSUE_TITLE="Multi-section"
export ISSUE_BODY_TEXT="## Section 1: Intro
Intro.

## Section 2: Body
Body.

## Section 3: Conclusion
Conclusion.

## Section 4: Appendix
Appendix."
run_test "4 sections blocks" "block"

# ─── Test 43: Phase 2 warn doesn't escalate to block ────
echo ""
echo "43. Phase 2 alignment warn doesn't escalate to block"
export PR_BODY="Closes #50"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES=".github/workflows/ci.yml
docs/Testing.md
.github/pull_request_template.md"
export ISSUE_TITLE="Fix API routes"
export ISSUE_BODY_TEXT="Fix the broken API routes."
run_test "multiple alignment warns stay warn" "warn"

# ═══════════════════════════════════════════════════════════
# Phase 3 Tests: LLM-Assisted Split Suggestions
# ═══════════════════════════════════════════════════════════

echo ""
echo "━━━ Phase 3: LLM Split Suggestions ━━━"

# Helper: run the gate and check if output contains/excludes a pattern
run_test_content() {
  local name="$1"
  local expected_verdict="$2"
  local should_contain="$3"   # "contains:<pattern>" or "excludes:<pattern>"
  TOTAL=$((TOTAL + 1))

  local output verdict exit_code
  set +e
  output=$(bash "$GATE" 2>&1)
  exit_code=$?
  set -e

  verdict=$(echo "$output" | grep '^Verdict:' | awk '{print $2}')

  local content_ok=true
  local content_mode content_pattern
  content_mode="${should_contain%%:*}"
  content_pattern="${should_contain#*:}"

  if [ "$content_mode" = "contains" ]; then
    if ! echo "$output" | grep -qF "$content_pattern"; then
      content_ok=false
    fi
  elif [ "$content_mode" = "excludes" ]; then
    if echo "$output" | grep -qF "$content_pattern"; then
      content_ok=false
    fi
  fi

  if [ "$verdict" = "$expected_verdict" ] && [ "$content_ok" = true ]; then
    echo "  ✅ ${name} → ${verdict} (${content_mode} '${content_pattern}' ✓)"
    PASS=$((PASS + 1))
  else
    echo "  ❌ ${name} → expected '${expected_verdict}' (${content_mode} '${content_pattern}'), got '${verdict}' content_ok=${content_ok}"
    echo "     Output: $(echo "$output" | head -5)"
    FAIL=$((FAIL + 1))
  fi
}

# ─── Test 44: BLOCK without API key → no LLM suggestion ──
echo ""
echo "44. BLOCK verdict without OPENROUTER_API_KEY → no split suggestion"
export PR_BODY="Closes #50"
export ADDITIONS=1200
export DELETIONS=400
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
crates/pi-daemon-api/src/server.rs"
unset OPENROUTER_API_KEY 2>/dev/null || true
unset ISSUE_TITLE 2>/dev/null || true
unset ISSUE_BODY_TEXT 2>/dev/null || true
run_test_content "block without API key skips LLM" "block" "excludes:Suggested Split"

# ─── Test 45: PASS verdict never triggers LLM ────────────
echo ""
echo "45. PASS verdict → no split suggestion regardless of API key"
export PR_BODY="Closes #42"
export ADDITIONS=100
export DELETIONS=50
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export OPENROUTER_API_KEY="fake-key-for-testing"
run_test_content "pass never triggers LLM" "pass" "excludes:Suggested Split"

# ─── Test 46: WARN verdict never triggers LLM ────────────
echo ""
echo "46. WARN verdict → no split suggestion"
export PR_BODY="Closes #99"
export ADDITIONS=600
export DELETIONS=300
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export OPENROUTER_API_KEY="fake-key-for-testing"
run_test_content "warn never triggers LLM" "warn" "excludes:Suggested Split"

# ─── Test 47: Version stamp updated to v3 ────────────────
echo ""
echo "47. Version stamp is v3 Phase 1+2+3"
export PR_BODY="Closes #99"
export ADDITIONS=600
export DELETIONS=300
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
unset OPENROUTER_API_KEY 2>/dev/null || true
run_test_content "version stamp v3" "warn" "contains:Scope Gate v3"

# ─── Test 48: BLOCK comment contains version stamp v3 ────
echo ""
echo "48. BLOCK comment also has version stamp v3"
export PR_BODY="Closes #50"
export ADDITIONS=1200
export DELETIONS=400
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
unset OPENROUTER_API_KEY 2>/dev/null || true
run_test_content "block version stamp v3" "block" "contains:Scope Gate v3"

# ─── Test 49: BLOCK with empty API key → graceful skip ───
echo ""
echo "49. BLOCK with empty OPENROUTER_API_KEY → graceful skip"
export PR_BODY="Closes #50"
export ADDITIONS=1200
export DELETIONS=400
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
export OPENROUTER_API_KEY=""
run_test_content "empty API key skips LLM" "block" "excludes:Suggested Split"

# ─── Test 50: BLOCK with invalid API key → graceful skip ─
echo ""
echo "50. BLOCK with invalid API key → degrades gracefully (no crash)"
export PR_BODY="Closes #50"
export ADDITIONS=1200
export DELETIONS=400
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs
.github/workflows/ci.yml
docs/Architecture.md
.github/pull_request_template.md"
export OPENROUTER_API_KEY="invalid-key-will-get-401"
export ISSUE_TITLE="Big refactor project"
export ISSUE_BODY_TEXT="Refactor all the things."
# This will make a real HTTP call that returns an error — the gate
# should degrade gracefully and still produce a BLOCK verdict
run_test_content "invalid API key degrades gracefully" "block" "excludes:Suggested Split"

# ─── Test 51: Phase 3 log message on skip ────────────────
echo ""
echo "51. Phase 3 logs skip reason when API key missing"
export PR_BODY="Closes #50"
export ADDITIONS=1200
export DELETIONS=400
export CHANGED_FILES="crates/pi-daemon-api/src/routes.rs"
unset OPENROUTER_API_KEY 2>/dev/null || true
run_test_content "logs skip reason" "block" "contains:OPENROUTER_API_KEY not set"

# ─── Summary ─────────────────────────────────────────────
echo ""
echo "━━━ Results ━━━"
echo "Passed: ${PASS}/${TOTAL}"
if [ "$FAIL" -gt 0 ]; then
  echo "FAILED: ${FAIL}/${TOTAL}"
  exit 1
else
  echo "All tests passed."
fi
