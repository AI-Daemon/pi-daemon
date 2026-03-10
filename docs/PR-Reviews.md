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
| **Commit Message Secrets** | Custom script | ✅ | ✅ | Scans all PR commit messages for secret patterns and env dumps. AI agents are prone to dumping `env` output into commit messages. See `commit-msg-scan.yml`. |
| **SSRF / Private IP** | Custom grep | ⚠️ | ❌ | Scans for hardcoded private IPs, cloud metadata URLs, or localhost in non-test code. Advisory only — some localhost refs are intentional. |

### 📦 Supply Chain & Dependencies

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **License Compliance** | cargo-deny | ✅ | ✅ | Ensures all deps use approved licenses (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0). Blocks copyleft (GPL). Config: `deny.toml`. |
| **Unused Dependencies** | cargo-machete | ⚠️ | ✅ | Detects deps in Cargo.toml that are never used in code. Warning only — false positives with proc macros. |
| **Outdated Dependencies** | cargo outdated / Dependabot | ❌ | ❌ | Reports deps with newer versions. Advisory only. |
| **Security Audit** | cargo-audit | ✅ | ❌ | Checks deps against RustSec advisory database. Standard output in logs. |
| **npm Security Audit** | npm audit / yarn audit | ✅ | ✅ | Checks JavaScript/TypeScript dependencies for known vulnerabilities. Supports npm, Yarn, and pnpm projects. Auto-detects package managers. |

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
| **PR Pipeline** | `pr-pipeline.yml` | ✅ | ❌ | Orchestrator that calls reusable workflows in dependency order. Scope gate runs first; lint depends on it via `needs:`. |

The PR Pipeline (`pr-pipeline.yml`) is the orchestrator for PR checks. It calls reusable workflows using `uses: ./.github/workflows/_*.yml` and enforces ordering via `needs:`. If the scope gate blocks, all downstream jobs are automatically skipped.

**Current pipeline (Phase 1):**
```
scope-gate → lint-format (clippy + fmt + docs compile)
```

Future phases (#126, #127, #128) will add test, security, build, code-review, and sandbox stages.

### 🔬 Scope Gate

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Scope Gate** | `scripts/scope-gate.sh` | ✅ | ✅ | Mechanical PR scope check. Pure bash, no LLM, <15 seconds. See `_scope-gate.yml`. |

The Scope Gate evaluates whether a PR is focused enough to review reliably. It runs three checks:

1. **Issue Reference (required):** Every PR must reference an issue (`Closes #N`, `Fixes #N`, `Refs #N`, or `Implements #N`). Missing = blocked.
2. **Size Thresholds:** >1500 lines → blocked. >800 lines → warning.
3. **Workstream Cohesion:** Files are categorized into workstreams (`source`, `test-code`, `test-infra`, `ci-workflows`, `docs`, `pr-template`, `scripts`, `other`). `deps` (Cargo.toml/Cargo.lock) and `changelog` are always expected and not counted. `source` + `test-code` always count as one workstream (tests belong with their source per Google eng-practices). 4+ workstreams → blocked. 3 workstreams at 500+ lines → warning.

On block/warn, a PR comment is posted with the workstream breakdown and guidance on how to split. On pass, no comment (no clutter). If a previously-blocked PR is fixed and now passes, the stale comment is deleted.

**Architecture:** The logic lives in `scripts/scope-gate.sh` — a standalone bash script testable locally via `scripts/test-scope-gate.sh` (27 test cases). The workflow (`_scope-gate.yml`) is a thin reusable wrapper that gathers PR metadata and calls the script.

### 🧹 Lint & Format

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Clippy** | cargo clippy | ✅ | ❌ | Zero warnings policy (`-D warnings`). Runs via `_lint-format.yml`. |
| **Rustfmt** | cargo fmt | ✅ | ❌ | Formatting check. Runs via `_lint-format.yml`. |
| **Docs Compile** | cargo doc | ✅ | ❌ | Ensures docs compile without warnings. Runs via `_lint-format.yml`. |

Lint and format checks run as a reusable workflow (`_lint-format.yml`) called by the PR Pipeline orchestrator. They only run after the scope gate passes.

### 🧹 PR Hygiene

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Commit Message Lint** | Custom regex | ⚠️ | ❌ | Conventional Commits format: `feat:`, `fix:`, `docs:`, `test:`, `ci:`, `refactor:`, `chore:`. |
| **PR Description** | Custom script | ⚠️ | ❌ | Verifies PR body isn't empty and references an issue. |

### 🏗️ Build & Test

| Check | Tool | Blocking | Comment | Description |
|-------|------|:--------:|:-------:|-------------|
| **Unit Tests** | cargo test (per-crate matrix) | ✅ | ❌ | Parallel per-crate. Standard test output in logs. |
| **Integration Tests** | cargo test --test '*' | ✅ | ❌ | Cross-module tests. Runs after lint passes. |
| **E2E Tests** | tests/e2e/ | ✅ | ❌ | Full daemon boot, HTTP/WebSocket flows. |
| **Sandbox Integration** | Real binary lifecycle testing | ✅ | ❌ | Builds release binary, runs as actual daemon process, tests concurrency, memory, crash recovery, graceful shutdown. Only runs when core code changes. |
| **Pi Bridge Extension** | TypeScript compilation check | ✅ | ❌ | Compiles the TypeScript bridge extension that connects pi TUI instances to pi-daemon. Type-check only. |
| **Coverage** | cargo-llvm-cov | ❌ | ✅ | Posts per-crate coverage breakdown as PR comment. Advisory only. |
| **Release Build** | cargo build --release | ✅ | ❌ | Must compile for Linux x86_64 and macOS ARM64. |
| **MSRV** | cargo check on Rust 1.75 | ✅ | ❌ | Minimum Supported Rust Version. |

---

## Sandbox Integration Testing

The **Sandbox Integration Test** (`sandbox-test.yml`) is a unique workflow that tests the actual `pi-daemon` binary in a real CI environment, catching deployment issues that in-process tests cannot detect.

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
- **Trigger**: Pull requests that change `crates/**`, `Cargo.toml`, or `Cargo.lock`
- **Skip**: Documentation-only changes (no unnecessary overhead)
- **Timeout**: 10 minutes (prevents hung processes from blocking CI)

### Future Enhancements
- **#77**: Persistence testing (after SQLite substrate #13)
- **#78**: Supervisor stress testing (after supervisor #17) 
- **#79**: Scheduler validation (after cron engine #16)

---

## 🎯 Unified Code Review System

Comprehensive code review system with intelligent file classification and specialized review workflows. Uses **Gemini 2.5 Flash** via OpenRouter for AI-powered analysis.

**🔍 Intelligent File Classification:**
- **🏗️ Architectural Review:** Source code files (.rs, .ts, .js) containing architectural decisions
- **🧪 Test Quality Review:** Test files (test., spec., tests/) for testing patterns and quality  
- **⚙️ Configuration Review:** Config files (.yml, .toml, .md) for DevOps and documentation standards
- **❌ Auto-Exclude:** Generated files (package-lock.json, node_modules/, dist/) to prevent token overflow
- **🚀 Performance:** 75-97% faster execution through focused, conditional review execution
- **🎯 Single Check per Review:** Job status shows pass/fail, detailed analysis in PR comments (no duplicate checks)
- **💬 Always Comment:** Every review type comments — detailed analysis when files present, clear skip explanation when no relevant files

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

**📊 Unified Output:**
- **File Classification:** Clear breakdown of what files triggered which reviews
- **Multi-Review Results:** Separate GitHub checks for each review type (🏗️ Architectural, 🧪 Test Quality, ⚙️ Configuration)
- **Performance Optimization:** Focused context per review type, shared file classification
- **Auto-Pass Logic:** PRs with only generated files skip all reviews

**💰 Cost:** ~$0.01–0.05 per PR review (Gemini 2.5 Flash pricing), token usage optimized through intelligent filtering.

**🔧 Implementation:** Full OpenRouter + Gemini 2.5 Flash integration with dual-layer analysis framework, size-based fallbacks, and comprehensive error handling for both Architectural and Test Quality reviews. Configuration Review has been LLM-powered since initial implementation.

---

## Branch Protection & Check Gate

The `main` branch is protected with the following rules:

- **Require a pull request** — no direct pushes to `main`
- **Require 1 approving review** — satisfied by the Check Gate bot (see below)
- **Require status checks to pass** — security scans and commit-message scan must pass
- **No force-push** — prevents history rewriting on `main`
- **No deletions** — prevents accidental branch deletion

### Check Gate (`auto-approve.yml`)

The Check Gate is a **dynamic auto-approve system** that discovers and tracks all PR checks automatically — no hardcoded check names.

**How it works:**

1. Each time a CI workflow completes (`workflow_run` event), the Gate fires
2. It fetches **all** check runs for the PR head SHA using the paginated Checks API (`github.paginate`)
3. It self-excludes its own `Check Gate` check run to prevent loops
4. It classifies every check: pass, fail, skip, running, pending, cancelled
5. **Decision:**
   - Any still running/pending → wait (exit, will re-trigger on next workflow completion)
   - Any failed/cancelled → deny (log details, do not approve)
   - All terminal + none failed + ≥20 checks present → approve with summary
6. If approved, submits an approving review as `github-actions[bot]`

**Key properties:**
- **Zero hardcoded check names** — discovers checks dynamically
- **Minimum threshold (20)** — prevents premature approval when few checks have registered; this is the only tunable
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

The CI workflow (`ci.yml`) uses a top-level `permissions` block to grant the `GITHUB_TOKEN` the minimum scopes needed:

```yaml
permissions:
  contents: read          # checkout code
  pull-requests: write    # post/update PR comments
  checks: write           # report check results
```

**Why this matters:** Several jobs (`coverage`, `binary-size`, `report`) use `actions/github-script` to post PR comments via `github.rest.issues.createComment()`. Without `pull-requests: write`, these calls fail with `403 Resource not accessible by integration`.

**When adding new checks:** If a new job needs to post PR comments or interact with the PR API, it's already covered by the top-level permissions block. No per-job permissions needed unless a job requires *additional* scopes.

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
