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
| **Secrets Scan** | TruffleHog | ✅ | ✅ | Scans full diff for leaked API keys, tokens, passwords, private keys. Runs via `_security.yml`. |
| **Hardcoded Credentials** | gitleaks / custom grep | ✅ | ✅ | Regex patterns for `sk-ant-`, `ghp_`, `Bearer `, `password = "`, etc. Runs via `_security.yml`. |
| **Commit Message Secrets** | Custom script | ✅ | ✅ | Scans all PR commit messages for secret patterns and env dumps. Runs via `_hygiene.yml`. |
| **SSRF / Private IP** | Custom grep | ⚠️ | ❌ | Scans for hardcoded private IPs, cloud metadata URLs, or localhost in non-test code. Advisory only — some localhost refs are intentional. |

### 📦 Supply Chain & Dependencies

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **License Compliance** | cargo-deny | ✅ | ✅ | Ensures all deps use approved licenses (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0). Blocks copyleft (GPL). Config: `deny.toml`. Runs via `_security.yml`. |
| **Unused Dependencies** | cargo-machete | ⚠️ | ✅ | Detects deps in Cargo.toml that are never used in code. Warning only — false positives with proc macros. |
| **Outdated Dependencies** | cargo outdated / Dependabot | ❌ | ❌ | Reports deps with newer versions. Advisory only. |
| **Security Audit** | cargo-audit | ✅ | ❌ | Checks deps against RustSec advisory database. Runs via `_security.yml`. |
| **npm Security Audit** | npm audit / yarn audit | ✅ | ✅ | Checks JavaScript/TypeScript dependencies for known vulnerabilities. Runs via `_security.yml`. |

### 🔍 Code Quality & Correctness

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Clippy** | cargo clippy | ✅ | ❌ | Zero warnings policy (`-D warnings`). Standard output in logs. |
| **Rustfmt** | cargo fmt | ✅ | ❌ | Formatting check. Self-explanatory — run `cargo fmt`. |
| **Unsafe Code Detection** | grep / cargo geiger | ✅ | ✅ | pi-daemon should be 100% safe Rust. Any new `unsafe` requires justification. Runs via `_security.yml`. |
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
| **Docs Drift Detection** | Custom script | ✅ | ✅ | Fails if routes → `API-Reference.md`, `Cargo.toml` members → `Architecture.md`, `config.rs` → `Configuration.md`, or workflow files → `PR-Reviews.md` are changed without updating the corresponding docs. |
| **Changelog Enforcement** | Custom script | ✅ | ❌ | Fails if `.rs`, workflow (`.yml`), or `Cargo.toml` files changed but `CHANGELOG.md` wasn't updated. |
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

### 📝 PR Template & Local Test Enforcement

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Template Sync** | template-sync.yml | ❌ | ❌ | Validates PR template structure matches current crates/workflows. Runs on push to main + weekly cron. |
| **Local Test Evidence** | Custom script | ⚠️ | ❌ | Warns if PR description lacks evidence of local `scripts/test-local.sh` execution or crate checkboxes. |

### 🔀 PR Pipeline (Orchestrator)

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **PR Pipeline** | `pr-pipeline.yml` | ✅ | ❌ | Orchestrator that calls reusable workflows in dependency order. Scope gate runs first; lint, test, and security depend on it via `needs:`. |

The PR Pipeline (`pr-pipeline.yml`) is the sole orchestrator for all PR checks. It calls reusable workflows using `uses: ./.github/workflows/_*.yml` and enforces ordering via `needs:`. If the scope gate blocks, all downstream jobs are automatically skipped.

**Pipeline dependency graph:**
```
scope-gate ──→ classify ──→ lint-format ──┬──→ test ──────────→ code-review (LLM reviews)
                       │                  │
                       │                  └──→ build ─────────→ sandbox (integration)
                       │
                       ├──→ security (parallel with lint)
                       │
                       └──→ hygiene (parallel with lint)
```

**Key ordering guarantees:**
- Code review only fires after lint + tests pass — the LLM never reviews broken code (#127)
- Sandbox only runs after build passes — no point testing a binary that doesn't compile (#127)
- Build runs in parallel with tests (both depend on lint) for faster pipeline completion
- Security and hygiene run in parallel with lint (only depend on scope gate) (#128)

### 🔬 Scope Gate

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Scope Gate** | `scripts/scope-gate.sh` | ✅ | ✅ | Mechanical PR scope check. Pure bash, no LLM, <15 seconds. See `_scope-gate.yml`. |

The Scope Gate evaluates whether a PR is focused enough to review reliably. It runs six checks across three phases:

**Phase 1: Mechanical Checks**

1. **Issue Reference (required):** Every PR must reference an issue (`Closes #N`, `Fixes #N`, `Refs #N`, or `Implements #N`). Missing = blocked.
2. **Size Thresholds:** >1500 lines → blocked. >800 lines → warning.
3. **Workstream Cohesion:** Files are categorized into workstreams (`source`, `test-code`, `test-infra`, `ci-workflows`, `docs`, `pr-template`, `scripts`, `other`). `deps` (Cargo.toml/Cargo.lock) and `changelog` are always expected and not counted. `source` + `test-code` always count as one workstream (tests belong with their source per Google eng-practices). 4+ workstreams → blocked. 3 workstreams at 500+ lines → warning.

**Phase 2: Issue Alignment Validation (#120)**

4. **Issue Scope Detection:** The referenced issue's body is scanned for structural signals of multi-concern issues. Headings matching `Pillar N`, `Phase N`, `Part N`, `Section N`, `Step N` patterns are counted. 3+ pillars/phases → blocked ("split the issue first"). Issues with >15 acceptance criteria (`- [ ]`) across >5 `## ` sections → warning (likely too broad).
5. **Workstream vs Issue Alignment:** Compares the PR's changed file categories against keywords in the issue title/body. If the PR touches workflows, docs, templates, scripts, or test-utils but the issue doesn't mention the corresponding category, a warning is raised. This catches accidental scope creep — not blocks, just flags for reviewer attention.

Phase 2 checks are skipped gracefully when issue metadata is unavailable (e.g., `gh issue view` fails or the issue has no body).

**Phase 3: LLM-Assisted Split Suggestions (#121)**

6. **Smart Split Suggestions:** When the mechanical gate BLOCKs a PR, an LLM (Gemini 2.5 Flash via OpenRouter) is called with the file list, workstream categories, and issue body (no diffs — tiny context) to suggest how to split the PR into focused, reviewable pieces. Each suggestion includes a proposed issue title, file grouping, and merge order. The suggestion is appended to the existing BLOCK comment. Cost: $0.00 for clean PRs, ~$0.01 for blocked PRs. Degrades gracefully if `OPENROUTER_API_KEY` is missing or the LLM call fails — the BLOCK verdict is unchanged, only the split suggestion is omitted.

On block/warn, a PR comment is posted with the workstream breakdown and guidance on how to split. On pass, no comment (no clutter). If a previously-blocked PR is fixed and now passes, the stale comment is deleted.

**Architecture:** The logic lives in `scripts/scope-gate.sh` — a standalone bash script testable locally via `scripts/test-scope-gate.sh` (54 test cases: 27 Phase 1, 19 Phase 2, 8 Phase 3). The workflow (`_scope-gate.yml`) is a thin reusable wrapper that gathers PR metadata (including issue title/body for Phase 2 and `OPENROUTER_API_KEY` for Phase 3) and calls the script.

### 🔀 Change Classification (#133)

After the scope gate passes, a lightweight **classify** job (~5s) categorizes changed files into boolean flags. These flags are passed as `workflow_call` inputs to every reusable workflow. Jobs inside each workflow use `if:` to skip when their flag is false.

**Why this approach:**
- Every reusable workflow is always *called* (check runs always register), but individual jobs may be skipped.
- Skipped jobs produce `conclusion: skipped` which the Check Gate treats as terminal — no merge blocking.
- All inputs default to `true`, so `ci-main.yml` (post-merge) runs everything without changes.
- Adding a new change category requires: 1 grep line in classify + 1 input in the consuming workflow + 1 `if:` on the job.

**Classification flags:**

| Flag | Pattern | Example files |
|------|---------|--------------|
| `has_rust` | `\.rs$` | `crates/pi-daemon-kernel/src/lib.rs` |
| `has_ts` | `\.(ts\|js)$` | `extensions/pi-daemon-bridge/src/index.ts` |
| `has_docs` | `^docs/` or `\.md$` | `docs/Architecture.md`, `CHANGELOG.md` |
| `has_deps` | `Cargo\.toml` or `Cargo\.lock` | `Cargo.toml`, `crates/*/Cargo.toml` |
| `has_workflows` | `^\.github/workflows/` | `.github/workflows/pr-pipeline.yml` |
| `has_scripts` | `^scripts/` | `scripts/scope-gate.sh` |
| `has_npm` | `package\.json`, lockfiles | `extensions/pi-daemon-bridge/package.json` |

**Per-workflow skip matrix:**

| Workflow | Inputs | Jobs that skip | Condition |
|----------|--------|---------------|-----------|
| `_lint-format.yml` | `has_rust` | clippy, fmt, docs-compile | `!has_rust` |
| `_test.yml` | `has_rust` | test-unit, test-integration, coverage | `!has_rust` |
| `_build.yml` | `has_rust`, `has_deps`, `has_ts`, `has_npm` | build-release, binary-size, msrv | `!has_rust && !has_deps` |
| | | test-bridge | `!has_ts && !has_npm` |
| `_sandbox.yml` | `has_rust`, `has_deps` | sandbox | `!has_rust && !has_deps` |
| `_security.yml` | `has_rust`, `has_deps`, `has_ts`, `has_npm` | license-check, cargo-audit | `!has_rust && !has_deps` |
| | | unsafe-check | `!has_rust` |
| | | npm-security | `!has_npm && !has_ts` |
| | | secrets-scan, credential-patterns | *(never skip)* |
| `_hygiene.yml` | `has_rust`, `has_deps`, `has_docs` | sidebar-sync, markdown-lint, link-check | `!has_docs` |
| | | unused-deps | `!has_rust && !has_deps` |
| | | crate-doc-sync | `!has_rust && !has_deps && !has_docs` |
| | | todo-tracker | `!has_rust && !has_docs` |
| | | commit-msg-scan, commit-lint, pr-description, docs-drift, changelog | *(never skip)* |
| `_code-review.yml` | `has_rust`, `has_ts`, `has_workflows`, `has_docs`, `has_deps` | architectural-review | `!has_rust && !has_ts` |
| | | test-quality-review | `!has_rust` |
| | | configuration-review | `!has_workflows && !has_docs && !has_deps` |
| | | classify, code-review (gate) | *(never skip)* |

**PR type examples:**

| PR Type | Flags set | What runs | ~Time |
|---------|-----------|-----------|-------|
| Docs-only (`docs/*.md`, `CHANGELOG.md`) | `has_docs` | scope-gate, classify, secrets-scan, credential-patterns, commit-msg-scan, commit-lint, pr-description, docs-drift, changelog, sidebar-sync, markdown-lint, link-check, todo-tracker, crate-doc-sync, classify+config-review+gate | ~30-45s |
| CI-workflow-only (`.github/workflows/*.yml`) | `has_workflows` | scope-gate, classify, secrets-scan, credential-patterns, commit-msg-scan, commit-lint, pr-description, docs-drift, changelog, classify+config-review+gate | ~30-45s |
| Rust code (`crates/*.rs`, `Cargo.toml`, docs) | `has_rust`, `has_deps`, `has_docs` | Everything | ~20 min |
| TypeScript-only (`extensions/pi-daemon-bridge/*`) | `has_ts`, `has_npm` | scope-gate, classify, test-bridge, secrets-scan, credential-patterns, npm-security, commit-msg-scan, commit-lint, pr-description, docs-drift, changelog, classify+arch-review+gate | ~1-2 min |

### 🧹 Lint & Format

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Clippy** | cargo clippy | ✅ | ❌ | Zero warnings policy (`-D warnings`). Runs via `_lint-format.yml`. |
| **Rustfmt** | cargo fmt | ✅ | ❌ | Formatting check. Runs via `_lint-format.yml`. |
| **Docs Compile** | cargo doc | ✅ | ❌ | Ensures docs compile without warnings. Runs via `_lint-format.yml`. |

Lint and format checks run as a reusable workflow (`_lint-format.yml`) called by the PR Pipeline orchestrator. They only run after the scope gate passes.

### 🧪 Tests

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Unit Tests** | cargo test (per-crate matrix) | ✅ | ❌ | Parallel per-crate. Runs via `_test.yml`. |
| **Integration Tests** | cargo test --test '*' | ✅ | ❌ | Cross-module tests. Runs via `_test.yml`. |
| **Coverage** | cargo-llvm-cov | ❌ | ✅ | Posts coverage breakdown as PR comment. Needs unit tests. Runs via `_test.yml`. |

Test jobs run as a reusable workflow (`_test.yml`) called by the PR Pipeline orchestrator. Tests only run after lint passes (`needs: [lint-format]` in orchestrator). Coverage depends on unit tests internally (`needs: [test-unit]`).

### 🔐 Security

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Secrets Scan** | TruffleHog | ✅ | ✅ | Scans for leaked API keys, tokens, passwords. |
| **Credential Patterns** | Custom grep | ✅ | ✅ | Regex patterns for `sk-ant-`, `ghp_`, etc. |
| **License Compliance** | cargo-deny | ✅ | ✅ | Approved licenses only. Config: `deny.toml`. |
| **Unsafe Code** | grep | ✅ | ✅ | pi-daemon must be 100% safe Rust. |
| **Security Audit** | cargo-audit | ✅ | ❌ | Checks deps against RustSec advisory DB. |
| **npm Audit** | npm/yarn audit | ✅ | ✅ | JS/TS dependency vulnerabilities. |

Security checks run as a reusable workflow (`_security.yml`) called by the PR Pipeline orchestrator. Security runs in parallel with lint after scope gate passes (`needs: [scope-gate]` in orchestrator).

### 🧹 PR Hygiene

All hygiene checks run as a reusable workflow (`_hygiene.yml`) called by the PR Pipeline orchestrator after scope gate passes (`needs: [scope-gate]`). This consolidates what was previously spread across `commit-msg-scan.yml`, `docs-check.yml`, `pr-hygiene.yml`, and the remaining `ci.yml` jobs.

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Commit Message Lint** | Custom regex | ⚠️ | ❌ | Conventional Commits format: `feat:`, `fix:`, `docs:`, `test:`, `ci:`, `refactor:`, `chore:`. Runs via `_hygiene.yml`. |
| **PR Description** | Custom script | ⚠️ | ❌ | Verifies PR body isn't empty and references an issue. Runs via `_hygiene.yml`. |
| **Commit Message Secrets** | Custom script | ✅ | ✅ | Scans commit messages for secret patterns and env dumps. Runs via `_hygiene.yml`. |
| **Sidebar Sync** | Custom bash | ✅ | ✅ | Verifies `docs/_Sidebar.md` matches `docs/*.md` files. Runs via `_hygiene.yml`. |
| **Markdown Lint** | markdownlint | ⚠️ | ❌ | Heading structure, formatting. Runs via `_hygiene.yml`. |
| **Link Check** | lychee | ⚠️ | ❌ | Validates links in markdown files. Runs via `_hygiene.yml`. |
| **Unused Dependencies** | cargo-machete | ⚠️ | ✅ | Detects deps never used in code. Warning only. Runs via `_hygiene.yml`. |
| **Crate Docs Sync** | Custom script | ✅ | ❌ | Verifies all workspace crates appear in `Architecture.md`. Runs via `_hygiene.yml`. |
| **TODO Tracker** | Custom grep | ⚠️ | ✅ | New TODOs must reference a GitHub issue. Runs via `_hygiene.yml`. |
| **Docs Drift** | Custom script | ✅ | ✅ | Fails if source/config/workflow changes lack corresponding doc updates. Runs via `_hygiene.yml`. |
| **Changelog** | Custom script | ✅ | ❌ | Fails if meaningful changes lack CHANGELOG.md update. Runs via `_hygiene.yml`. |

### 🏗️ Build & Release

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Release Build** | cargo build --release | ✅ | ❌ | Must compile for Linux x86_64 and macOS ARM64. Runs via `_build.yml`. |
| **Binary Size** | stat + GitHub Script | ⚠️ | ✅ | Reports size diff. Warns if >50MB. Runs via `_build.yml`. |
| **MSRV** | cargo check on Rust 1.94 | ✅ | ❌ | Minimum Supported Rust Version. Runs via `_build.yml`. |
| **Pi Bridge Extension** | TypeScript compilation check | ✅ | ❌ | Compiles the TypeScript bridge extension. Type-check only. Runs via `_build.yml`. |

Build checks run as a reusable workflow (`_build.yml`) called by the PR Pipeline orchestrator. Build only runs after lint passes (`needs: [lint-format]` in orchestrator).

### 🧪 Tests & Coverage

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Unit Tests** | cargo test (per-crate matrix) | ✅ | ❌ | Parallel per-crate. Runs via `_test.yml`. |
| **Integration Tests** | cargo test --test '*' | ✅ | ❌ | Cross-module tests. Runs via `_test.yml` after lint passes. |
| **E2E Tests** | tests/e2e/ | ✅ | ❌ | Full daemon boot, HTTP/WebSocket flows. |
| **Coverage** | cargo-llvm-cov | ❌ | ✅ | Posts per-crate coverage breakdown as PR comment. Advisory only. Runs via `_test.yml`. |

### 🏖️ Sandbox Integration

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Sandbox Integration** | Real binary lifecycle testing | ✅ | ❌ | Builds release binary, runs as actual daemon process, tests concurrency, memory, crash recovery, graceful shutdown. Runs via `_sandbox.yml`. |

Sandbox tests run as a reusable workflow (`_sandbox.yml`) called by the PR Pipeline orchestrator. Sandbox only runs after build passes (`needs: [build]` in orchestrator).

---

## Sandbox Integration Testing

The **Sandbox Integration Test** (`_sandbox.yml`) is a reusable workflow that tests the actual `pi-daemon` binary in a real CI environment, catching deployment issues that in-process tests cannot detect. Called by the PR Pipeline orchestrator after build passes (`needs: [build]`).

### What Makes It Different
- **Real Binary**: Builds `cargo build --release` and executes actual `pi-daemon` command
- **Real Process**: Daemon runs as separate process with PID tracking and signal handling
- **Real Ports**: Tests port 4200 binding (not random test ports)  
- **Real Configuration**: Loads actual TOML config files from disk
- **Real Concurrency**: Multiple HTTP clients + WebSocket connections + sustained load

### Test Phases

#### Phase 1: Smoke Testing
- Binary startup and health endpoint readiness
- All API endpoints functional (health, status, agents, events, shutdown, OpenAI)
- Webchat SPA loads with full content
- PID file management (creation, tracking, cleanup)

#### Phase 2: Concurrency & Load
- 50 concurrent HTTP requests to `/api/status`
- 20 concurrent agent registrations with verification  
- 5 WebSocket connections within per-IP limits
- Memory usage monitoring with multi-method measurement (warns if >200MB)

#### Phase 3: Stress & Recovery  
- 30-second sustained load with memory growth tracking
- Kill -9 crash simulation followed by restart verification
- Memory leak detection with robust measurement (warns if >50MB growth during load)

#### Phase 4: Graceful Shutdown
- API shutdown endpoint testing
- Process exit verification and timing
- PID file cleanup validation
- Port release confirmation

### Critical Gaps Addressed

| In-Process Tests Miss | Real Deployment Bug | Sandbox Catches |
|-----------------------|-------------------|-----------------|
| Binary panics on startup | Missing runtime init | ✅ Real daemon startup |
| PID file management | Written but never cleaned up | ✅ File lifecycle testing |
| Stale PID file after crash | daemon.json blocks restart after kill -9 | ✅ CLI auto-cleans stale PID files |
| Port binding issues | SO_REUSEADDR conflicts | ✅ Standard port binding |
| Port TIME_WAIT after crash | Port stuck after kill -9 | ✅ Recovery test waits for port to clear |
| Signal handling | Ctrl+C/SIGTERM cleanup | ✅ Signal testing |
| Memory leaks | Only visible under sustained use | ✅ Load testing + monitoring |
| WebSocket limits | Per-IP enforcement | ✅ Connection limit validation |

### Memory Monitoring

The sandbox test includes comprehensive memory monitoring to detect leaks and validate realistic usage:

- **Multiple measurement methods**: `ps -o rss=` (portable), `/proc/PID/status VmRSS` (Linux, more accurate), process tree totals
- **Decimal precision**: Uses `bc` for accurate MB calculations instead of integer truncation
- **Realistic validation**: Fails if memory <5MB (unrealistic for Rust daemon with web framework and assets)
- **PID validation**: Ensures daemon process exists before attempting measurement
- **Expected range**: 20-50MB for idle daemon (Rust binary + Axum + tokio + embedded assets)

### When It Runs
- **Trigger**: Called by PR Pipeline orchestrator after build passes (`needs: [build]`)
- **Skip**: Skipped if build fails or scope gate blocks
- **Timeout**: 10 minutes (prevents hung processes from blocking CI)

### Future Enhancements
- **#77**: Persistence testing (after SQLite substrate #13)
- **#78**: Supervisor stress testing (after supervisor #17) 
- **#79**: Scheduler validation (after cron engine #16)

---

## 🎯 Unified Code Review System

Comprehensive code review system with intelligent file classification and specialized review workflows. Uses **Gemini 2.5 Flash** via OpenRouter for AI-powered analysis. Runs as a reusable workflow (`_code-review.yml`) called by the PR Pipeline orchestrator. Code review only runs after lint + tests pass (`needs: [lint-format, test]` in orchestrator), ensuring the LLM never reviews code that doesn't compile or pass tests (#127).

**🔍 Intelligent File Classification:**
- **🏗️ Architectural Review:** Source code files (.rs, .ts, .js) containing architectural decisions
- **🧪 Test Quality Review:** Test files (test., spec., tests/) for testing patterns and quality  
- **⚙️ Configuration Review:** Config files (.yml, .toml, .md) for DevOps and documentation standards
- **❌ Auto-Exclude:** Generated files (package-lock.json, node_modules/, dist/) to prevent token overflow
- **🚀 Performance:** 75-97% faster execution through focused, conditional review execution
- **🎯 Single Check per Review:** Job status shows pass/fail, detailed analysis in native PR reviews (no duplicate checks)
- **📝 Native PR Reviews:** All 3 review types post native GitHub PR reviews with inline annotations on specific files/lines (not timeline comments)
- **⏭️ Skip → Step Summary:** When a review type has no relevant files, a skip message goes to `$GITHUB_STEP_SUMMARY` instead of a PR comment — zero timeline noise

**🎯 Clean Single-Check Architecture:**

### 🏗️ Architectural Review (for source code)
**Dual-Layer System:**
1. **Technical Compliance:** Crate structure, concurrency patterns, error handling, security, logging
2. **Architectural Judgment:** System design impact, consistency with vision, maintainability

### 🧪 Test Quality Review (for test files)  
**Dual-Layer System:**
1. **Testing Compliance:** Test naming, organization, assertions, isolation, test utilities usage
2. **Test Quality Analysis:** Coverage strategy, maintainability, test architecture patterns

### ⚙️ Configuration Review (for config files)  
**Dual-Layer System:**
1. **Technical Compliance:** YAML syntax, CI/CD naming, documentation format, changelog standards
2. **DevOps Expert Analysis:** Performance impact, maintainability, documentation clarity, best practices

**🔒 Universal Pass/Fail Logic:** 
- ✅ **PASS:** Both compliance and expert layers must approve for each review type
- ❌ **FAIL:** Either layer can fail any review - neither can override into passing  
- **Conditional Execution:** Only runs reviews for relevant file types (architectural for source, test quality for tests, etc.)

**📊 Native Review Output (#139):**
- **File Classification:** Clear breakdown of what files triggered which reviews
- **Multi-Review Results:** Separate native PR reviews for each type (🏗️ Architectural, 🧪 Test Quality, ⚙️ Configuration)
- **Inline Annotations:** LLM findings with `file` + `line` appear as inline comments in the "Files changed" tab
- **Verdict Events:** `PASS` → `COMMENT` review, `FAIL` → `REQUEST_CHANGES` review (shows in merge box)
- **Dedup:** Previous bot reviews for each type are dismissed before posting new ones (keyed by HTML comment markers like `<!-- pi-daemon:arch-review -->`)
- **Backward Compat:** Actions/issues without file/line fall back to the top-level review body
- **Skip Handling:** Reviews with no relevant files write to `$GITHUB_STEP_SUMMARY` instead of PR comments
- **Auto-Pass Logic:** PRs with only generated files skip all reviews

**💰 Cost:** ~$0.01–0.05 per PR review (Gemini 2.5 Flash pricing), token usage optimized through intelligent filtering.

**🔧 Implementation:** Full OpenRouter + Gemini 2.5 Flash integration with dual-layer analysis framework, size-based fallbacks, and comprehensive error handling for both Architectural and Test Quality reviews. Configuration Review has been LLM-powered since initial implementation. As of #139, all reviews use `pulls.createReview()` with inline annotations instead of `issues.createComment()`.

---

## Branch Protection & Check Gate

The `main` branch is protected with the following rules:

- **Require a pull request** — no direct pushes to `main`
- **Require 1 approving review** — satisfied by the Check Gate bot (see below)
- **Require status checks to pass** — security scans and commit-message scan must pass
- **No force-push** — prevents history rewriting on `main`
- **No deletions** — prevents accidental branch deletion

### Check Gate (`auto-approve.yml`)

The Check Gate is a **dynamic auto-approve system** that discovers and tracks all PR checks automatically — no hardcoded check names. After Phase 4 (#128), it watches only the **"PR Pipeline"** workflow since all PR checks now live under that single orchestrator.

**How it works:**

1. When the "PR Pipeline" workflow completes (`workflow_run` event), the Gate fires
2. It fetches **all** check runs for the PR head SHA using the paginated Checks API
3. It self-excludes its own `Check Gate` check run to prevent loops
4. It classifies every check: pass, fail, skip, running, pending, cancelled
5. **Decision:**
   - Any still running/pending → wait (exit, will re-trigger on next workflow completion)
   - Any failed/cancelled → deny (log details, do not approve)
   - All terminal + none failed + ≥20 checks present → approve with summary
6. If approved, submits an approving review as `github-actions[bot]`

**Key properties:**
- **Zero hardcoded check names** — discovers checks dynamically via SHA
- **Minimum threshold (20)** — prevents premature approval when few checks have registered; this is the only tunable
- **Single trigger** — watches only "PR Pipeline" (all PR checks are under it)
- **Concurrency groups** — prevents race conditions from simultaneous triggers
- **Manual re-trigger** — escape hatch when event-driven triggers fail:

```bash
gh workflow run auto-approve.yml -f pr_number=66
```

**Adding new CI checks:** Just add them to any workflow file. The Gate discovers them automatically. No changes to `auto-approve.yml` needed.

If any check fails, the bot does not approve and the PR cannot be merged.

### Pre-commit Hooks

Local secret scanning is available via [pre-commit](https://pre-commit.com/):

```bash
pip install pre-commit
pre-commit install --hook-type pre-commit --hook-type commit-msg
```

This installs:
- **gitleaks** — scans staged file content for secrets
- **check-commit-msg.sh** — scans commit messages for secret patterns and env dumps

Agents can bypass with `--no-verify`, which is why CI scanning is the real backstop.

---

## Workflow Permissions

The PR Pipeline orchestrator (`pr-pipeline.yml`) uses a top-level `permissions` block and passes them down to reusable workflows via `permissions:` on each `uses:` call:

```yaml
permissions:
  contents: read          # checkout code
  pull-requests: write    # post/update PR comments
  checks: write           # report check results
```

**Why this matters:** Several jobs (`coverage`, `binary-size`, hygiene checks) use `actions/github-script` to post PR comments or reviews. Code review jobs use `github.rest.pulls.createReview()` for native PR reviews with inline annotations, and `pulls.dismissReview()` to remove stale reviews on re-push. Without `pull-requests: write`, these calls fail with `403 Resource not accessible by integration`.

**When adding new checks:** Add the job to the appropriate reusable `_*.yml` workflow. Permissions are granted at the orchestrator level when calling each reusable workflow. If a new workflow needs additional scopes, add them to the `permissions:` block on the corresponding `uses:` entry in `pr-pipeline.yml`.

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
- **New crate not tested:** Add it to the test matrix in `.github/workflows/_test.yml`
- **All checks skipped:** Scope gate may have blocked — check the scope-gate output in the PR Pipeline run

---

## Adding a New Check

1. **Determine category** — where does it fit in the tables above?
2. **Decide: comment or check-only** — is the output dynamic and unique per PR? → Comment. Standard tooling output? → Check only.
3. **Decide: blocking or advisory** — can the developer safely ignore it sometimes? → Advisory (⚠️). Is it a correctness/security issue? → Blocking (✅).
4. **Implement** — add a job to the appropriate reusable workflow (`_hygiene.yml`, `_security.yml`, `_test.yml`, `_build.yml`, etc.). If ordering matters, add a `needs:` entry in `pr-pipeline.yml`.
5. **Update this doc** — add the check to the appropriate table above
6. **Update the issue** — reference the implementation PR

**Workflow file layout (after Phase 4):**

| File | Purpose | Trigger |
|------|---------|---------|
| `pr-pipeline.yml` | Orchestrator — calls all reusable workflows | `pull_request` |
| `_scope-gate.yml` | Scope gate (Phase 1: size, workstreams, issue ref; Phase 2: issue scope + alignment; Phase 3: LLM split suggestions) | `workflow_call` |
| `_lint-format.yml` | Clippy + rustfmt + doc compile | `workflow_call` |
| `_test.yml` | Unit + integration tests + coverage | `workflow_call` |
| `_security.yml` | Secrets, license, unsafe, audit | `workflow_call` |
| `_code-review.yml` | LLM reviews (arch, test, config) | `workflow_call` |
| `_build.yml` | Release build, binary size, MSRV, bridge | `workflow_call` |
| `_hygiene.yml` | PR hygiene, docs, commit-msg, TODOs, drift | `workflow_call` |
| `_sandbox.yml` | Full sandbox integration test | `workflow_call` |
| `auto-approve.yml` | Check Gate (dynamic auto-approve) | `workflow_run` |
| `ci-main.yml` | Post-merge CI on main | `push` to main |
| `template-sync.yml` | PR template sync | `push` to main |
| `wiki-sync.yml` | Wiki sync | `push` to main |
