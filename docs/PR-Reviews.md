# PR Reviews

Every pull request to `main` is reviewed by a suite of automated checks. This page documents what runs, why, and what to expect.

For the full rationale and design decisions, see [Issue #28](https://github.com/Demonseed-jpg/pi-daemon/issues/28).

## Overview

Checks fall into two categories:

- **Comment + Native Check** — Posts a PR comment with detailed analysis AND creates a GitHub status check. Used when the output is dynamic and the developer needs context to act.
- **Native Check Only** — Creates a GitHub status check. Output is in the Actions logs. Used when the failure is self-explanatory (standard tooling output).

## All Checks by Category

### 🔒 Security & Secrets

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Secrets Scan** | TruffleHog | ✅ | ✅ | Scans full diff for leaked API keys, tokens, passwords, private keys. AI agents are prone to committing secrets from context. |
| **Hardcoded Credentials** | gitleaks / custom grep | ✅ | ✅ | Regex patterns for `sk-ant-`, `ghp_`, `Bearer `, `password = "`, etc. Catches unverified patterns TruffleHog might miss. |
| **SSRF / Private IP** | Custom grep | ⚠️ | ❌ | Scans for hardcoded private IPs, cloud metadata URLs, or localhost in non-test code. Advisory only — some localhost refs are intentional. |

### 📦 Supply Chain & Dependencies

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **License Compliance** | cargo-deny | ✅ | ✅ | Ensures all deps use approved licenses (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0). Blocks copyleft (GPL). Config: `deny.toml`. |
| **Unused Dependencies** | cargo-machete | ⚠️ | ✅ | Detects deps in Cargo.toml that are never used in code. Warning only — false positives with proc macros. |
| **Outdated Dependencies** | cargo outdated / Dependabot | ❌ | ❌ | Reports deps with newer versions. Advisory only. |
| **Security Audit** | cargo-audit | ✅ | ❌ | Checks deps against RustSec advisory database. Standard output in logs. |

### 🔍 Code Quality & Correctness

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Clippy** | cargo clippy | ✅ | ❌ | Zero warnings policy (`-D warnings`). Standard output in logs. |
| **Rustfmt** | cargo fmt | ✅ | ❌ | Formatting check. Self-explanatory — run `cargo fmt`. |
| **Unsafe Code Detection** | grep / cargo geiger | ✅ | ✅ | pi-daemon should be 100% safe Rust. Any new `unsafe` requires justification. |
| **TODO/FIXME Tracker** | Custom grep | ⚠️ | ✅ | New TODOs must reference a GitHub issue (`TODO(#42): ...`). Lists orphaned TODOs. |
| **Dead Code / Unused Imports** | cargo clippy | ✅ | ❌ | Caught by clippy's `dead_code` and `unused_imports` lints. |
| **Complexity Analysis** | Custom / rust-code-analysis | ⚠️ | ❌ | Flags functions >100 lines or cyclomatic complexity >15. Advisory. |

### 📏 Binary & Performance

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Binary Size Tracking** | cargo build --release + stat | ⚠️ | ✅ | Reports size diff vs main. Warns if growth >10%. Hard warn at >50MB absolute. |
| **Compile Time** | cargo build --timings | ❌ | ❌ | Advisory only. Logs are sufficient. |

### 🤖 AI-Specific (folded into Arch Review LLM call)

These are checked by the [Architectural Review](#architectural-review-llm) as additional items in its checklist — no separate workflow.

| Check | Description |
|-------|-------------|
| **Hallucination Detection** | Imports of non-existent crates/modules, calls to fictional APIs |
| **Copy-Paste / Duplication** | Large duplicated code blocks that should be extracted |
| **Naming Consistency** | Error types named `*Error` (not `*Err`), consistent patterns across sessions |
| **Prompt Injection Leak** | System prompts, LLM instructions, or `"You are"` patterns in committed code |

### 📖 Documentation Architecture & Hygiene

Documentation is treated as a system — not just individual files. Checks cover structure (do all the pages connect?), coherence (do they agree with each other?), and completeness (is anything missing?).

Implemented as a **hybrid Documentation Architecture Validator** — a single workflow that runs tool-based structural checks first, then an LLM semantic review if `docs/` was modified.

#### Structural Checks (Tool-Based — Always Run)

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Sidebar ↔ Files Sync** | Custom bash | ✅ | ✅ | Every `.md` in `docs/` (excluding `_`-prefixed) must be in `_Sidebar.md`, and every sidebar entry must have a corresponding file. Catches orphaned pages and dangling sidebar links. |
| **Crate ↔ Architecture Sync** | Custom script | ⚠️ | ✅ | Parses `Cargo.toml` workspace members, verifies each crate appears in `Architecture.md`. Catches "added a crate but forgot to document it." |
| **Internal Link Validation** | lychee | ⚠️ | ❌ | Validates all `[[Wiki-Links]]` and `[text](relative/path.md)` links resolve to real files. |
| **Heading Structure Lint** | markdownlint | ⚠️ | ❌ | Consistent heading hierarchy: every page starts with `# Title`, uses `##` for sections, no level skipping. Config: `.markdownlint.json`. |
| **Docs Drift Detection** | Custom script | ⚠️ | ✅ | If `routes*.rs` changed, was `API-Reference.md` updated? If `Cargo.toml` members changed, was `Architecture.md` updated? |
| **Changelog Enforcement** | Custom script | ⚠️ | ❌ | Warns if `*.rs` files changed but `CHANGELOG.md` wasn't updated. |
| **Broken External Links** | lychee | ⚠️ | ❌ | Checks external URLs in markdown files for 404s. |

#### Semantic Checks (LLM-Based — Only When `docs/` Modified)

These run as part of a single Gemini 2.5 Flash call, only triggered when the PR touches files in `docs/`. All documentation pages are sent as context.

| Check | Blocking | Comment | Description |
|-------|:--------:|:-------:|-------------|
| **Terminology Consistency** | ⚠️ | ✅ | Are the same concepts named the same way across all pages? Catches "agent registry" vs "agent store" vs "agent manager" drift. |
| **Contradiction Detection** | ⚠️ | ✅ | Do any pages contradict each other? e.g., Architecture.md says "4 crates" but Configuration.md references a 5th. |
| **Completeness Check** | ⚠️ | ✅ | Does this PR introduce a concept (new crate, new API, new feature) that warrants a new documentation page or section that doesn't exist? |
| **Coherence Review** | ⚠️ | ✅ | Do the docs read well as a whole? Consistent tone, logical flow between pages, no orphaned references. |

#### Scheduled Checks (Weekly Cron — Not Per-PR)

| Check | Tool | Description |
|-------|------|-------------|
| **Stale Content Detection** | Gemini 2.5 Flash | Weekly scan: sends all docs + codebase state to LLM, asks "which docs are stale, outdated, or describe things that no longer exist?" Opens a GitHub issue with findings. |

#### Combined Output

When both structural and semantic checks run, a single PR comment is posted:

```
## 📖 Documentation Review

### Structure (automated)
✅ All pages in sidebar
✅ All internal links resolve
⚠️ Architecture.md missing crate `pi-daemon-memory`
✅ Heading structure valid

### Coherence (AI review)
✅ Terminology consistent across all pages
⚠️ Getting-Started.md still references echo-back behavior — update for LLM integration
✅ No contradictions between pages
✅ New feature adequately documented
```

### 🧹 PR Hygiene

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **PR Size** | gh pr diff --stat | ⚠️ | ✅ | <400 lines: silent. 400–800: info. 800+: warning. 1500+: strong warning. Never blocking. |
| **Commit Message Lint** | Custom regex | ⚠️ | ❌ | Conventional Commits format: `feat:`, `fix:`, `docs:`, `test:`, `ci:`, `refactor:`, `chore:`. |
| **PR Description** | Custom script | ⚠️ | ❌ | Verifies PR body isn't empty and references an issue. |

### 🏗️ Build & Test

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Unit Tests** | cargo test (per-crate matrix) | ✅ | ❌ | Parallel per-crate. Standard test output in logs. |
| **Integration Tests** | cargo test --test '*' | ✅ | ❌ | Cross-module tests. Runs after lint passes. |
| **E2E Tests** | tests/e2e/ | ✅ | ❌ | Full daemon boot, HTTP/WebSocket flows. |
| **Coverage** | cargo-llvm-cov | ❌ | ✅ | Posts per-crate coverage breakdown as PR comment. Advisory only. |
| **Release Build** | cargo build --release | ✅ | ❌ | Must compile for Linux x86_64 and macOS ARM64. |
| **MSRV** | cargo check on Rust 1.75 | ✅ | ❌ | Minimum Supported Rust Version. |

---

## Architectural Review (LLM)

The most unique check. Uses **Gemini 2.5 Flash** via OpenRouter to review every PR against the project's architecture documentation.

**What it checks (14-point checklist):**
1. Crate structure — correct crate, no dependency cycles
2. Concurrency — DashMap, broadcast channels, Arc
3. Error handling — thiserror/anyhow, no unwrap in library code
4. Naming conventions — snake_case, test naming patterns
5. API conventions — routes, extractors, status codes
6. Testing — unit tests, integration tests, test-utils usage
7. Security — no unwrap on user input, auth middleware, secrets not logged
8. Logging — tracing macros, not println
9. Documentation — doc comments, docs/ updates
10. General best practices — no dead code, no TODO without issue ref
11. Hallucination detection — non-existent crates/APIs *(AI-specific)*
12. Copy-paste detection — duplicated code blocks *(AI-specific)*
13. Naming consistency — consistent patterns across the codebase *(AI-specific)*
14. Prompt injection leaks — system prompts in source files *(AI-specific)*

**Output:** Comment with per-check pass/fail/skip table + issues list + native GitHub Check (pass/fail).

**Cost:** ~$0.01–0.05 per PR review (Gemini 2.5 Flash pricing).

---

## Interpreting Results

### ✅ All checks pass
Merge when ready (after human review if required).

### ⚠️ Warnings only
Review the warnings. Most are informational — decide if they warrant changes. Warnings do NOT block merge.

### ❌ Blocking failure
Must be fixed before merge. Common causes:
- Clippy warning introduced
- Formatting violation
- Test failure
- Secret detected in diff
- License violation in new dependency
- Unsafe code without justification

### Check not running?
- **Arch Review:** Requires `OPENROUTER_API_KEY` repo secret
- **Wiki Sync:** Requires `WIKI_TOKEN` repo secret
- **New crate not tested:** Add it to the CI matrix in `.github/workflows/ci.yml`

---

## Adding a New Check

1. **Determine category** — where does it fit in the tables above?
2. **Decide: comment or check-only** — is the output dynamic and unique per PR? → Comment. Standard tooling output? → Check only.
3. **Decide: blocking or advisory** — can the developer safely ignore it sometimes? → Advisory (⚠️). Is it a correctness/security issue? → Blocking (✅).
4. **Implement** — add a job to an existing workflow or create a new `.github/workflows/<name>.yml`
5. **Update this doc** — add the check to the appropriate table above
6. **Update the issue** — reference the implementation PR in [#28](https://github.com/Demonseed-jpg/pi-daemon/issues/28)
