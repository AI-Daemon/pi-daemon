#!/usr/bin/env bash
# ============================================================
# check-commit-msg.sh — Pre-commit hook (commit-msg stage)
#
# Scans the commit message for leaked secrets, API keys, tokens,
# passwords, and env dump patterns. Blocks the commit if found.
#
# Used by: .pre-commit-config.yaml (commit-msg stage)
# Usage:   scripts/check-commit-msg.sh <commit-msg-file>
# ============================================================
set -euo pipefail

MSG_FILE="${1:?Usage: check-commit-msg.sh <commit-msg-file>}"

if [ ! -f "$MSG_FILE" ]; then
  echo "Error: commit message file not found: $MSG_FILE"
  exit 1
fi

# ── Secret patterns ──────────────────────────────────────────
# Each pattern is a grep -E regex. Add new patterns here.
PATTERNS=(
  # GitHub tokens
  'ghp_[a-zA-Z0-9]{36}'
  'gho_[a-zA-Z0-9]{36}'
  'github_pat_[a-zA-Z0-9]{22}_[a-zA-Z0-9]{59}'
  # OpenRouter / OpenAI
  'sk-or-v1-[a-f0-9]{64}'
  'sk-ant-[a-zA-Z0-9]{20,}'
  'sk-proj-[a-zA-Z0-9]{20,}'
  'sk-[a-zA-Z0-9]{48}'
  # AWS
  'AKIA[0-9A-Z]{16}'
  # GitLab
  'glpat-[a-zA-Z0-9\-]{20,}'
  # Slack
  'xoxb-[0-9]{10,}'
  'xoxp-[0-9]{10,}'
  # Generic env dump indicators (KEY=value on its own line)
  '^(GH_TOKEN|OPENROUTER_API_KEY|NEO4J_PASSWORD|AWS_SECRET_ACCESS_KEY|GITHUB_TOKEN)=.+'
  # Inline password assignments
  'password\s*=\s*["\x27][^"\x27]{4,}'
)

COMBINED_PATTERN=$(IFS='|'; echo "${PATTERNS[*]}")

# ── Scan ─────────────────────────────────────────────────────
MATCHES=$(grep -nE "$COMBINED_PATTERN" "$MSG_FILE" 2>/dev/null || true)

if [ -n "$MATCHES" ]; then
  echo ""
  echo "🚨 COMMIT BLOCKED — Potential secrets detected in commit message!"
  echo ""
  echo "Matches found:"
  echo "$MATCHES" | while IFS= read -r line; do
    echo "  ⚠️  $line"
  done
  echo ""
  echo "If these are false positives (e.g., documentation about secret patterns),"
  echo "you can bypass with: git commit --no-verify"
  echo ""
  echo "Otherwise, edit your commit message to remove the secrets."
  echo ""
  exit 1
fi

exit 0
