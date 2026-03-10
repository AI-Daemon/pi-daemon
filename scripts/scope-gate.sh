#!/usr/bin/env bash
# scope-gate.sh — Mechanical PR scope gate (Phase 1 + Phase 2 + Phase 3)
#
# Evaluates whether a PR is focused enough to review by checking:
#   Phase 1:
#     1. Issue reference (required)
#     2. Size thresholds (lines changed)
#     3. Workstream cohesion (distinct areas of concern)
#   Phase 2:
#     4. Issue scope detection (multi-concern issues)
#     5. Workstream vs issue alignment (PR changes match issue?)
#   Phase 3:
#     6. LLM-assisted split suggestions (only on BLOCK verdicts)
#
# Inputs (env vars):
#   PR_BODY        — full PR description text
#   ADDITIONS      — number of added lines
#   DELETIONS      — number of deleted lines
#   CHANGED_FILES  — newline-separated list of changed file paths
#   ISSUE_TITLE    — (Phase 2) issue title from gh issue view (optional)
#   ISSUE_BODY_TEXT — (Phase 2) issue body from gh issue view (optional)
#   OPENROUTER_API_KEY — (Phase 3) API key for LLM split suggestions (optional)
#
# Outputs (env vars, written to $GITHUB_OUTPUT if set):
#   VERDICT       — "pass" | "warn" | "block"
#   COMMENT_BODY  — markdown comment to post on the PR
#
# Exit codes:
#   0 — pass or warn
#   1 — block
#
set -euo pipefail

# ── Helpers ────────────────────────────────────────────────

VERDICT="pass"
FINDINGS=()
WARNINGS=()

block() { VERDICT="block"; FINDINGS+=("🚫 $1"); }
warn()  { if [ "$VERDICT" != "block" ]; then VERDICT="warn"; fi; WARNINGS+=("⚠️ $1"); }
info()  { FINDINGS+=("ℹ️ $1"); }

# ── Check 1: Issue Reference ──────────────────────────────

ISSUE_NUM=""
if [ -n "${PR_BODY:-}" ]; then
  # Match: Closes #N, Fixes #N, Refs #N, Implements #N (case-insensitive)
  ISSUE_NUM=$(echo "$PR_BODY" | grep -oiP '(?:closes?|fixes?|refs?|implements?)\s+#(\d+)' | grep -oP '\d+' | head -1 || true)
fi

if [ -z "$ISSUE_NUM" ]; then
  block "Every PR must reference an issue. Add \`Closes #N\` to the PR description."
fi

# ── Check 2: Size Thresholds ─────────────────────────────

ADDITIONS="${ADDITIONS:-0}"
DELETIONS="${DELETIONS:-0}"
TOTAL=$((ADDITIONS + DELETIONS))

if [ "$TOTAL" -gt 1500 ]; then
  block "PR is ${TOTAL} lines — too large for reliable review. Split into focused PRs under 800 lines."
elif [ "$TOTAL" -gt 800 ]; then
  warn "PR is ${TOTAL} lines — review quality drops above 800. Consider splitting."
fi

# ── Check 3: Workstream Cohesion ──────────────────────────

declare -A WORKSTREAMS
WORKSTREAM_FILES=""

categorize_file() {
  local file="$1"
  local cat=""
  case "$file" in
    crates/pi-daemon-test-utils/*)            cat="test-infra" ;;
    crates/*/tests/*.rs|crates/*/tests/*/*.rs) cat="test-code" ;;
    crates/*/src/*.rs|crates/*/src/*/*.rs)     cat="source" ;;
    .github/workflows/*.yml)                   cat="ci-workflows" ;;
    docs/*.md|docs/**/*.md)                    cat="docs" ;;
    .github/pull_request_template*)            cat="pr-template" ;;
    scripts/*)                                 cat="scripts" ;;
    Cargo.toml|Cargo.lock|*/Cargo.toml)        cat="deps" ;;
    CHANGELOG.md|CHANGELOG*)                   cat="changelog" ;;
    *)                                         cat="other" ;;
  esac
  echo "$cat"
}

# Count files per workstream
if [ -n "${CHANGED_FILES:-}" ]; then
  while IFS= read -r file; do
    [ -z "$file" ] && continue
    cat=$(categorize_file "$file")
    WORKSTREAMS[$cat]=$(( ${WORKSTREAMS[$cat]:-0} + 1 ))
    WORKSTREAM_FILES="${WORKSTREAM_FILES}${cat}|${file}\n"
  done <<< "$CHANGED_FILES"
fi

# source + test-code always count as one workstream per Google eng-practices
# (tests belong with their source code)
HAS_SOURCE=${WORKSTREAMS[source]:-0}
HAS_TEST_CODE=${WORKSTREAMS[test-code]:-0}
MERGED_SOURCE_TESTS=0
if [ "$HAS_SOURCE" -gt 0 ] && [ "$HAS_TEST_CODE" -gt 0 ]; then
  MERGED_SOURCE_TESTS=1
fi

# Count meaningful workstreams (exclude deps, changelog — always expected)
MEANINGFUL_COUNT=0
MEANINGFUL_NAMES=()
for ws in "${!WORKSTREAMS[@]}"; do
  case "$ws" in
    deps|changelog) continue ;;  # always expected, don't count
  esac
  # If we merged source+test-code, skip test-code as separate
  if [ "$MERGED_SOURCE_TESTS" -eq 1 ] && [ "$ws" = "test-code" ]; then
    continue
  fi
  if [ "$MERGED_SOURCE_TESTS" -eq 1 ] && [ "$ws" = "source" ]; then
    MEANINGFUL_NAMES+=("source+tests")
  else
    MEANINGFUL_NAMES+=("$ws")
  fi
  MEANINGFUL_COUNT=$((MEANINGFUL_COUNT + 1))
done

# Sort for stable output
IFS=$'\n' MEANINGFUL_NAMES=($(sort <<< "${MEANINGFUL_NAMES[*]}")); unset IFS

if [ "$MEANINGFUL_COUNT" -ge 4 ]; then
  block "PR contains ${MEANINGFUL_COUNT} distinct workstreams ($(IFS=', '; echo "${MEANINGFUL_NAMES[*]}")). Split each concern into its own PR."
elif [ "$MEANINGFUL_COUNT" -ge 3 ] && [ "$TOTAL" -gt 500 ]; then
  warn "PR spans ${MEANINGFUL_COUNT} workstreams at ${TOTAL} lines ($(IFS=', '; echo "${MEANINGFUL_NAMES[*]}")). Consider splitting."
fi

# ── Phase 2: Issue Alignment Validation ───────────────────
# These checks require issue metadata. If unavailable (no issue ref,
# or gh issue view failed), Phase 2 is skipped gracefully.

ISSUE_TITLE="${ISSUE_TITLE:-}"
ISSUE_BODY_TEXT="${ISSUE_BODY_TEXT:-}"
PHASE2_ACTIVE=false

if [ -n "$ISSUE_NUM" ] && { [ -n "$ISSUE_TITLE" ] || [ -n "$ISSUE_BODY_TEXT" ]; }; then
  PHASE2_ACTIVE=true
fi

# ── Check 4: Issue Scope Detection ───────────────────────
# Detect multi-concern issues by structural signals in the issue body.
# Issues with too many pillars/phases should be split before PR review.

if [ "$PHASE2_ACTIVE" = true ] && [ -n "$ISSUE_BODY_TEXT" ]; then
  # Count headings matching pillar/phase/part/section/step patterns
  PILLARS=$(echo "$ISSUE_BODY_TEXT" | grep -ciE "^#{1,3} .*(pillar|phase|part|section|step) [0-9]" || true)
  PILLARS="${PILLARS:-0}"

  # Count acceptance criteria checkboxes
  AC_COUNT=$(echo "$ISSUE_BODY_TEXT" | grep -c "^- \[ \]" || true)
  AC_COUNT="${AC_COUNT:-0}"

  # Count ## section headers
  IMPL_SECTIONS=$(echo "$ISSUE_BODY_TEXT" | grep -cE "^## " || true)
  IMPL_SECTIONS="${IMPL_SECTIONS:-0}"

  if [ "$PILLARS" -ge 3 ]; then
    block "Issue #${ISSUE_NUM} describes ${PILLARS} pillars/phases — split into ${PILLARS} separate issues first."
  fi

  if [ "$AC_COUNT" -gt 15 ] && [ "$IMPL_SECTIONS" -gt 5 ]; then
    warn "Issue #${ISSUE_NUM} has ${AC_COUNT} acceptance criteria across ${IMPL_SECTIONS} sections — likely too broad for one PR."
  fi
fi

# ── Check 5: Workstream vs Issue Alignment ────────────────
# Compare what the PR touches against what the issue describes.
# Flag file categories that aren't mentioned in the issue.

if [ "$PHASE2_ACTIVE" = true ]; then
  ISSUE_TEXT="${ISSUE_BODY_TEXT} ${ISSUE_TITLE}"

  # Helper: count mentions of keywords in issue text (case-insensitive)
  # Uses || true to prevent grep exit-code 1 from triggering set -e,
  # and captures in a variable to avoid double-output from || echo 0.
  count_mentions() {
    local n
    n=$(echo "$ISSUE_TEXT" | grep -ciE "$1" || true)
    echo "${n:-0}"
  }

  # Helper: count files matching a pattern in CHANGED_FILES
  count_touched() {
    if [ -n "${CHANGED_FILES:-}" ]; then
      local n
      n=$(echo "$CHANGED_FILES" | grep -c "$1" || true)
      echo "${n:-0}"
    else
      echo 0
    fi
  }

  # Workflows: PR touches .github/workflows/ but issue doesn't mention CI
  TOUCHES_WORKFLOWS=$(count_touched '\.github/workflows/')
  MENTIONS_WORKFLOWS=$(count_mentions 'workflow|ci/cd|github.actions|\.yml')
  if [ "$TOUCHES_WORKFLOWS" -gt 0 ] && [ "$MENTIONS_WORKFLOWS" -eq 0 ]; then
    warn "PR modifies ${TOUCHES_WORKFLOWS} workflow file(s) but issue #${ISSUE_NUM} doesn't mention CI/workflows. Are these changes in scope?"
  fi

  # Docs: PR touches docs/ but issue doesn't mention documentation
  TOUCHES_DOCS=$(count_touched '^docs/')
  MENTIONS_DOCS=$(count_mentions 'doc(s|umentation)|readme|PR.Reviews|Architecture\.md|Contributing\.md|Testing\.md')
  if [ "$TOUCHES_DOCS" -gt 0 ] && [ "$MENTIONS_DOCS" -eq 0 ]; then
    warn "PR modifies ${TOUCHES_DOCS} doc file(s) but issue #${ISSUE_NUM} doesn't mention documentation. Are these changes in scope?"
  fi

  # PR template: PR touches .github/pull_request_template but issue doesn't mention templates
  TOUCHES_TEMPLATE=$(count_touched '\.github/pull_request_template')
  MENTIONS_TEMPLATE=$(count_mentions 'template|pr.template|pull.request.template')
  if [ "$TOUCHES_TEMPLATE" -gt 0 ] && [ "$MENTIONS_TEMPLATE" -eq 0 ]; then
    warn "PR modifies the PR template but issue #${ISSUE_NUM} doesn't mention templates. Are these changes in scope?"
  fi

  # Scripts: PR touches scripts/ but issue doesn't mention scripting/tooling
  TOUCHES_SCRIPTS=$(count_touched '^scripts/')
  MENTIONS_SCRIPTS=$(count_mentions 'script|tooling|automation|scope.gate')
  if [ "$TOUCHES_SCRIPTS" -gt 0 ] && [ "$MENTIONS_SCRIPTS" -eq 0 ]; then
    warn "PR modifies ${TOUCHES_SCRIPTS} script file(s) but issue #${ISSUE_NUM} doesn't mention scripts/tooling. Are these changes in scope?"
  fi

  # Test infra: PR touches test-utils crate but issue doesn't mention test infra
  TOUCHES_TEST_INFRA=$(count_touched 'crates/pi-daemon-test-utils/')
  MENTIONS_TEST_INFRA=$(count_mentions 'test.util|test.infra|test.helper|test-utils')
  if [ "$TOUCHES_TEST_INFRA" -gt 0 ] && [ "$MENTIONS_TEST_INFRA" -eq 0 ]; then
    warn "PR modifies ${TOUCHES_TEST_INFRA} test-utils file(s) but issue #${ISSUE_NUM} doesn't mention test infrastructure. Are these changes in scope?"
  fi
fi

# ── Compute file count (used by Phase 3 + comment builder) ─

FILE_COUNT=0
if [ -n "${CHANGED_FILES:-}" ]; then
  FILE_COUNT=$(echo "$CHANGED_FILES" | grep -c '.' || true)
fi

# ── Phase 3: LLM-Assisted Split Suggestions ──────────────
# When the mechanical gate BLOCKs, ask an LLM how to split the PR.
# Only fires on BLOCK verdicts — cost is $0.00 for clean PRs.
# Requires OPENROUTER_API_KEY env var. Degrades gracefully if missing/failing.

LLM_SUGGESTION=""

if [ "$VERDICT" = "block" ] && [ -n "${OPENROUTER_API_KEY:-}" ]; then
  # Build the workstream summary for the LLM (file list + categories, no diffs)
  LLM_FILE_SUMMARY=""
  if [ -n "${WORKSTREAM_FILES:-}" ]; then
    # Group files by category with counts
    local_sorted_keys=()
    IFS=$'\n' local_sorted_keys=($(for k in "${!WORKSTREAMS[@]}"; do echo "$k"; done | sort)); unset IFS
    for ws in "${local_sorted_keys[@]}"; do
      ws_files=$(echo -e "$WORKSTREAM_FILES" | grep "^${ws}|" | sed "s/^${ws}|/  - /" || true)
      LLM_FILE_SUMMARY="${LLM_FILE_SUMMARY}${ws} (${WORKSTREAMS[$ws]} files):\n${ws_files}\n"
    done
  fi

  # Build the LLM prompt — tight, structured, asks for JSON
  LLM_SYSTEM="You are a senior engineer helping split an oversized pull request into focused, reviewable PRs.

Rules:
- Group related files together (source + its tests = one PR per Google eng-practices)
- Each suggested PR should be under 800 lines and touch 1-2 workstreams max
- Suggest a concrete issue title for each split PR
- If PRs have dependencies, suggest merge order
- Keep it brief — the developer knows their codebase

Output valid JSON only, no markdown fences:
{\"splits\":[{\"title\":\"string\",\"files\":[\"path\"],\"workstreams\":[\"string\"],\"estimated_lines\":N,\"rationale\":\"one sentence\"}],\"merge_order\":\"string or null\"}"

  LLM_USER="This PR was blocked by the scope gate. Help split it.

Total: ${TOTAL} lines (+${ADDITIONS}/-${DELETIONS}), ${FILE_COUNT} files, ${MEANINGFUL_COUNT} workstreams.

Issue: ${ISSUE_TITLE:-unknown}
$([ -n "${ISSUE_BODY_TEXT:-}" ] && echo "Issue body (first 2000 chars):" && echo "${ISSUE_BODY_TEXT:0:2000}" || echo "Issue body: not available")

Files by workstream:
$(echo -e "$LLM_FILE_SUMMARY")"

  # Call OpenRouter API (same pattern as _code-review.yml)
  LLM_RESPONSE=""
  set +e
  LLM_RESPONSE=$(curl -s --max-time 10 -X POST "https://openrouter.ai/api/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${OPENROUTER_API_KEY}" \
    -H "HTTP-Referer: https://github.com/AI-Daemon/pi-daemon" \
    -H "X-Title: pi-daemon-scope-gate" \
    -d "$(jq -n \
      --arg system "$LLM_SYSTEM" \
      --arg user "$LLM_USER" \
      '{
        "model": "google/gemini-2.5-flash",
        "messages": [
          {"role": "system", "content": $system},
          {"role": "user", "content": $user}
        ],
        "temperature": 0.1,
        "max_tokens": 2048
      }')" 2>/dev/null)
  set -e

  # Extract and parse the response
  LLM_CONTENT=""
  if [ -n "$LLM_RESPONSE" ]; then
    LLM_CONTENT=$(echo "$LLM_RESPONSE" | jq -r '.choices[0].message.content // empty' 2>/dev/null || true)
  fi

  if [ -n "$LLM_CONTENT" ]; then
    # Clean markdown fences if present
    LLM_CONTENT=$(echo "$LLM_CONTENT" | sed 's/^```json//; s/^```//; s/```$//' | sed '/^$/d')

    # Try to parse as JSON and build markdown suggestion
    if echo "$LLM_CONTENT" | jq . >/dev/null 2>&1; then
      SPLIT_COUNT=$(echo "$LLM_CONTENT" | jq '.splits | length' 2>/dev/null || echo 0)
      if [ "$SPLIT_COUNT" -gt 0 ]; then
        LLM_SUGGESTION=$'\n---\n\n### 💡 Suggested Split\n'

        for i in $(seq 0 $((SPLIT_COUNT - 1))); do
          SPLIT_TITLE=$(echo "$LLM_CONTENT" | jq -r ".splits[$i].title // \"PR $((i+1))\"")
          SPLIT_EST=$(echo "$LLM_CONTENT" | jq -r ".splits[$i].estimated_lines // \"?\"")
          SPLIT_RATIONALE=$(echo "$LLM_CONTENT" | jq -r ".splits[$i].rationale // empty")
          SPLIT_FILES=$(echo "$LLM_CONTENT" | jq -r ".splits[$i].files // [] | .[]" 2>/dev/null || true)

          LLM_SUGGESTION+=$'\n'"**PR $((i+1)): ${SPLIT_TITLE}** (~${SPLIT_EST} lines)"$'\n'
          LLM_SUGGESTION+="Create issue: \"${SPLIT_TITLE}\""$'\n'
          if [ -n "$SPLIT_FILES" ]; then
            LLM_SUGGESTION+="Files: $(echo "$SPLIT_FILES" | tr '\n' ', ' | sed 's/,$//')"$'\n'
          fi
          if [ -n "$SPLIT_RATIONALE" ]; then
            LLM_SUGGESTION+="*${SPLIT_RATIONALE}*"$'\n'
          fi
        done

        MERGE_ORDER=$(echo "$LLM_CONTENT" | jq -r '.merge_order // empty' 2>/dev/null || true)
        if [ -n "$MERGE_ORDER" ] && [ "$MERGE_ORDER" != "null" ]; then
          LLM_SUGGESTION+=$'\n'"**Merge order:** ${MERGE_ORDER}"$'\n'
        fi
      fi
    fi
  fi

  # Log result for CI debugging (never fails the gate)
  if [ -n "$LLM_SUGGESTION" ]; then
    echo "Phase 3: LLM split suggestion generated (${#LLM_SUGGESTION} chars)"
  else
    echo "Phase 3: LLM split suggestion skipped (empty or unparseable response)"
  fi
elif [ "$VERDICT" = "block" ]; then
  echo "Phase 3: LLM split suggestion skipped (OPENROUTER_API_KEY not set)"
fi

# ── Build Comment ─────────────────────────────────────────

build_workstream_table() {
  local table="| Workstream | Files |\n|------------|-------|\n"
  # Sort keys for deterministic output
  local sorted_keys
  IFS=$'\n' sorted_keys=($(for k in "${!WORKSTREAMS[@]}"; do echo "$k"; done | sort)); unset IFS
  for ws in "${sorted_keys[@]}"; do
    local label="$ws"
    # Show merged label when source + test-code are combined
    if [ "$MERGED_SOURCE_TESTS" -eq 1 ]; then
      if [ "$ws" = "source" ] || [ "$ws" = "test-code" ]; then
        label="${ws} *(counted as source+tests)*"
      fi
    fi
    table+="| ${label} | ${WORKSTREAMS[$ws]} |\n"
  done
  echo -e "$table"
}

COMMENT_BODY=""

GATE_VERSION="Scope Gate v3 · Phase 1+2+3"

case "$VERDICT" in
  block)
    COMMENT_BODY="## 🚫 Scope Gate: BLOCKED

This PR is too broad to review reliably (**${TOTAL} lines**, **${FILE_COUNT} files**, **${MEANINGFUL_COUNT} workstreams**).

**Findings:**
$(printf '%s\n' "${FINDINGS[@]}")
$([ ${#WARNINGS[@]} -gt 0 ] && printf '%s\n' "${WARNINGS[@]}" || true)

**Workstream breakdown:**
$(build_workstream_table)

**How to fix:** Split this PR so each addresses a single concern. Keep source code and its tests together (one workstream), but separate CI changes, docs, and infrastructure into their own PRs.
${LLM_SUGGESTION}
---
*🔬 ${GATE_VERSION}*"
    ;;
  warn)
    COMMENT_BODY="## ⚠️ Scope Gate: WARNING

This PR is getting broad (**${TOTAL} lines**, **${FILE_COUNT} files**, **${MEANINGFUL_COUNT} workstreams**).

**Warnings:**
$(printf '%s\n' "${WARNINGS[@]}")

**Workstream breakdown:**
$(build_workstream_table)

Review quality drops with larger PRs. Consider whether any changes could be a separate PR.

---
*🔬 ${GATE_VERSION}*"
    ;;
  pass)
    # No comment on pass — no clutter
    COMMENT_BODY=""
    ;;
esac

# ── Output ────────────────────────────────────────────────

# Write verdict to GITHUB_OUTPUT for workflow steps
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  echo "verdict=${VERDICT}" >> "$GITHUB_OUTPUT"
fi

# Write comment body to a well-known file for the workflow to pick up.
# This avoids passing markdown through GITHUB_OUTPUT multiline delimiters,
# which is fragile with special characters, pipes, and backticks.
COMMENT_FILE="${SCOPE_GATE_COMMENT_FILE:-/tmp/scope-gate-comment.md}"
if [ -n "$COMMENT_BODY" ]; then
  echo "$COMMENT_BODY" > "$COMMENT_FILE"
else
  # Ensure file exists but is empty (signals "pass, no comment")
  : > "$COMMENT_FILE"
fi

# Human-readable output for local testing
echo "━━━ Scope Gate Result ━━━"
echo "Verdict: ${VERDICT}"
echo "Lines: ${TOTAL} (+${ADDITIONS}/-${DELETIONS})"
echo "Files: ${FILE_COUNT}"
echo "Workstreams: ${MEANINGFUL_COUNT} ($(IFS=', '; echo "${MEANINGFUL_NAMES[*]}"))"
if [ ${#FINDINGS[@]} -gt 0 ]; then
  echo ""
  echo "Findings:"
  printf '  %s\n' "${FINDINGS[@]}"
fi
if [ ${#WARNINGS[@]} -gt 0 ]; then
  echo ""
  echo "Warnings:"
  printf '  %s\n' "${WARNINGS[@]}"
fi
if [ -n "$COMMENT_BODY" ]; then
  echo ""
  echo "━━━ Comment ━━━"
  echo "$COMMENT_BODY"
fi

# Exit code: 1 for block, 0 for pass/warn
if [ "$VERDICT" = "block" ]; then
  exit 1
fi
exit 0
