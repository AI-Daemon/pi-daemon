# Bug Report Template

> Copy this template when filing a new bug. Fill in each section and delete the guidance comments (`<!-- ... -->`). Remove optional sections if they don't apply.

---

## Bug Description

<!-- 2-3 sentences: What is broken? What do you observe? -->

## Evidence

<!-- Paste logs, CI output, error messages, or screenshots that prove the bug exists.
     Wrap long output in a code fence. Link to CI runs when available. -->

```
<!-- paste here -->
```

<!-- Optional: link to CI run, PR, or screenshot -->
- **CI run**: <!-- e.g., https://github.com/AI-Daemon/pi-daemon/actions/runs/XXXXX -->
- **Related PR**: <!-- e.g., #NNN -->

## Root Cause

<!-- Why does this happen? Reference specific files and lines when possible.
     If unknown, say "Unknown — needs investigation" and describe what you've ruled out. -->

**File(s):** `<!-- e.g., crates/pi-daemon-api/src/openai_compat.rs -->`
**Line(s):** `<!-- e.g., ~240-310 -->`

## Impact

<!-- What is affected? Be specific:
     - Is the feature completely broken or partially degraded?
     - Does it affect CI, the API, the webchat UI, developer experience?
     - Is data lost or silently corrupted?
     - How many users/agents are affected? -->

## Steps to Reproduce

<!-- Minimal steps to trigger the bug. Include commands, config, and expected setup. -->

1. <!-- step 1 -->
2. <!-- step 2 -->
3. <!-- step 3 -->

**Actual result:** <!-- what happens -->
**Expected result:** <!-- what should happen -->

## Proposed Fix

<!-- Concrete approach to fix the bug. "Fix it" is not sufficient.
     Reference the specific code change, pattern, or approach. -->

## Affected Code

<!-- List the files and functions involved. -->

| File | Function / Area |
|------|----------------|
| `<!-- path -->` | `<!-- function or section -->` |

## Related Issues

<!-- Cross-reference related bugs, enhancements, or PRs. -->

- <!-- #NNN — description -->

## Environment *(optional)*

<!-- Only if environment-specific. -->

| Field | Value |
|-------|-------|
| OS | <!-- e.g., Ubuntu 24.04, macOS 15, iOS 18 --> |
| Rust | <!-- e.g., 1.94.0 --> |
| Browser | <!-- e.g., Safari on iOS --> |
| pi-daemon version | <!-- e.g., commit SHA or tag --> |
