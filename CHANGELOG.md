# Changelog

All notable changes to pi-daemon will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Fix sandbox broken pipe error and add dynamic step summary (#153)
  - Replace `echo "$CONTENT" | grep -q` and `echo "$CONTENT" | wc -c` with here-strings in webchat step to avoid SIGPIPE on large (~128KB) output
  - Summary step no longer hardcodes ✅ for every phase; uses `$GITHUB_ENV` markers set by each phase's final step
  - Summary renders a dynamic markdown table with ✅/❌ per phase and writes to `$GITHUB_STEP_SUMMARY`
- Fix test, build, and sandbox jobs receiving false classify outputs due to missing `needs: [classify]` (#151)
  - `test`, `build`, and `sandbox` jobs in `pr-pipeline.yml` referenced `needs.classify.outputs.*` but did not include `classify` in their `needs` arrays
  - GitHub Actions only allows output access from jobs in the `needs` array, so classify flags (`has_rust`, `has_deps`, etc.) evaluated to null/false
  - All inner jobs (unit tests, integration tests, coverage, release build, binary size, MSRV, sandbox) were unconditionally skipped on every PR
  - Fix: add `classify` to `needs` arrays — `test: [lint-format, classify]`, `build: [lint-format, classify]`, `sandbox: [build, classify]`
  - No execution order change — `classify` already runs before `lint-format` transitively
  - Updated pipeline dependency graph in `pr-pipeline.yml` and `PR-Reviews.md`
- Missing `/v1/models` endpoint for OpenAI API compatibility (#115)
  - Added `GET /v1/models` endpoint that returns available models in OpenAI-compatible format
  - Dynamic model discovery from daemon configuration, registered agents, and provider API keys
  - Robust model validation (filters empty, whitespace-only, and overly long model names)
  - Smart ownership inference (anthropic, openai, meta, etc.) with deduplication
  - Production performance: 8ms single request, 66ms for 100 concurrent requests
  - Complete test coverage: 18 comprehensive tests covering all scenarios
  - Full API documentation and configuration examples added
- Suppress clean-PASS LLM code reviews from PR conversation (#148)
  - All 3 review types (architectural, test quality, configuration) now check for actionable findings before posting
  - Clean PASS (verdict PASS + zero inline comments + zero body actions + zero body issues) writes to `$GITHUB_STEP_SUMMARY` instead of `pulls.createReview()`
  - Stale reviews from previous pushes that had findings are still dismissed (existing dedup logic runs first)
  - FAIL verdicts continue to post `createReview()` with `REQUEST_CHANGES` — no change
  - PASS with findings (file+line annotations) continues to post `createReview()` with `COMMENT` — no change
  - Eliminates the most common source of PR comment noise: config review PASS on every PR touching .yml or .md files

### Added
- PR Output Layer Phase 4: Single auto-updating PR dashboard comment (#142)
  - `update-dashboard` job added to `pr-pipeline.yml` with `if: always()` and `needs: [all 9 stages]`
  - Aggregates results from all pipeline stages into a single `<!-- pi-daemon-dashboard -->` comment
  - Upsert logic: creates comment on first push, updates on subsequent pushes
  - Reads `needs.*.result` for job statuses (success/failure/skipped/cancelled)
  - Reads `repos.listCommitStatusesForRef()` for metrics (coverage %, binary size)
  - Reads `pulls.listReviews()` for code review verdicts (arch/test/config PASS/FAIL)
  - Reads `checks.listForRef()` for annotation counts
  - Shows blocking issues prominently with emoji + stage name summary
  - Scope gate row shows one-line summary + link (no detail duplication)
  - Dashboard job always exits 0 — wrapped in try/catch, failures emit `core.warning()` only
  - `auto-approve.yml`: `Update Dashboard` added to `SELF_NAMES` exclusion list alongside `Check Gate`
  - Links to Actions run for drill-down

### Changed
- PR Output Layer Phase 3: Convert security and hygiene warnings to inline file annotations (#141)
  - Secrets Scan (`_security.yml`): TruffleHog findings posted as `::error file=X,line=Y` annotations (up to 10 inline, overflow to step summary)
  - Credential Patterns (`_security.yml`): Hardcoded credential findings posted as `::error file=X,line=Y` annotations with actual file/line context from changed files
  - npm Security Audit (`_security.yml`): Vulnerability findings posted as `::warning` annotation + `$GITHUB_STEP_SUMMARY` (no file context — advisory-level findings)
  - Commit Message Secrets (`_hygiene.yml`): Secret patterns posted as `::error` annotations without file context (commit messages are not files)
  - Sidebar Sync (`_hygiene.yml`): Mismatches posted as `::warning file=X` annotations on orphaned doc pages and `_Sidebar.md`
  - All 5 `issues.createComment()` / `issues.updateComment()` upsert sites removed
  - Annotation overflow handled: first 10 findings inline, remaining aggregated in `$GITHUB_STEP_SUMMARY`
  - Blocking behavior preserved (`exit 1` on security failures)
  - `pull-requests: write` permission removed from `_hygiene.yml` (no longer needed — annotations use workflow commands, not API calls)
  - No permissions changes needed for `_security.yml` (annotations are workflow commands)
- PR Output Layer Phase 2: Convert metrics (coverage, binary size) to commit statuses (#140)
  - Coverage metric (`_test.yml`) now posts `repos.createCommitStatus()` with context `coverage` instead of a PR comment
  - Binary size metric (`_build.yml`) now posts `repos.createCommitStatus()` with context `binary-size` instead of a PR comment
  - Binary size >50MB now sets `state: 'failure'` (previously only a `::warning`)
  - Full metric detail moved to `$GITHUB_STEP_SUMMARY` (accessible via the Actions run link)
  - `statuses: write` permission added to `pr-pipeline.yml` (top-level, `test`, and `build` callers)
  - Legacy metric PR comments are auto-deleted on first run (one-time migration cleanup)
  - `auto-approve.yml` unaffected — it reads check runs via `checks.listForRef()`, not commit statuses
  - `description` field truncated to 140 chars with graceful ellipsis
- PR Output Layer Phase 1: Convert LLM code reviews to native Pull Request Reviews with inline annotations (#139)
  - All 3 review types (architectural, test quality, configuration) now use `pulls.createReview()` instead of `issues.createComment()`
  - LLM prompt schemas extended with optional `file` and `line` fields in `actions` and `issues` arrays
  - Inline review comments appear on specific files/lines in the "Files changed" tab
  - FAIL verdicts use `REQUEST_CHANGES` event; PASS verdicts use `COMMENT` event
  - Previous bot reviews are dismissed before posting new ones (dedup via HTML comment markers)
  - Actions/issues without `file`/`line` gracefully fall back to the top-level review body
  - Skip notifications (3 sites) moved from PR comments to `$GITHUB_STEP_SUMMARY`
  - `auto-approve.yml` APPROVE reviews are unaffected (dismiss filters by body marker, not user)

### Added
- Scope Gate Phase 3: LLM-assisted split suggestions (#121)
  - When the mechanical gate BLOCKs a PR, calls Gemini 2.5 Flash via OpenRouter to suggest how to split it
  - LLM receives file list + categories + issue body only (no diffs — tiny context, ~$0.01 per blocked PR)
  - Split suggestion appended to the existing BLOCK comment with proposed issue titles and merge order
  - Only fires on BLOCK verdicts — PASS and WARN never trigger an LLM call ($0.00 cost)
  - Graceful degradation: missing API key, API errors, or unparseable responses fall back to Phase 1+2 comment
  - `_scope-gate.yml` accepts `OPENROUTER_API_KEY` as optional secret (same pattern as `_code-review.yml`)
  - `pr-pipeline.yml` passes `OPENROUTER_API_KEY` to scope gate
  - 8 new test cases (54 total); all 46 Phase 1+2 tests still pass
  - Version stamp updated to `Scope Gate v3 · Phase 1+2+3`
- Scope Gate Phase 2: issue alignment validation (#120)
  - Check 4: Issue scope detection — blocks PRs when the referenced issue has 3+ pillars/phases/steps/sections; warns when 15+ acceptance criteria span 5+ sections
  - Check 5: Workstream vs issue alignment — warns when PR touches file categories (workflows, docs, templates, scripts, test-utils) not mentioned in the issue
  - Phase 2 checks skip gracefully when issue metadata is unavailable
  - 19 new test cases (46 total); all 27 Phase 1 tests still pass
  - Version stamp updated to `Scope Gate v2 · Phase 1+2`
  - `AGENT_PROMPT.md` updated with Step 4.5: Scope Check guidance
- Path-aware CI pipeline — skip irrelevant checks based on changed file types (#133)
- `classify` job in `pr-pipeline.yml` — categorizes changed files into 7 boolean flags (`has_rust`, `has_ts`, `has_docs`, `has_deps`, `has_workflows`, `has_scripts`, `has_npm`) (#133)
- All 7 reusable workflows accept optional boolean `workflow_call` inputs with `default: true` (fail-open) (#133)
- Jobs inside reusable workflows use `if:` to skip when their flag is false — skipped checks register as `conclusion: skipped`, satisfying the Check Gate (#133)

### Fixed
- Caller jobs in `pr-pipeline.yml` cascade-skip when all inner reusable-workflow jobs are skipped — added `if: !failure() && !cancelled()` on `test`, `build`, `code-review`, `sandbox` (#135)

### Changed
- `pr-pipeline.yml` — classify job runs after scope-gate; all `uses:` calls pass relevant flags (#133)
- `_lint-format.yml` — `has_rust` input gates clippy, fmt, docs-compile (#133)
- `_test.yml` — `has_rust` input gates unit tests, integration tests, coverage (#133)
- `_build.yml` — `has_rust`/`has_deps`/`has_ts`/`has_npm` inputs gate build-release, binary-size, msrv, test-bridge (#133)
- `_sandbox.yml` — `has_rust`/`has_deps` inputs gate sandbox integration (#133)
- `_security.yml` — `has_rust`/`has_deps`/`has_ts`/`has_npm` inputs gate license-check, unsafe-check, cargo-audit, npm-security; secrets-scan and credential-patterns always run (#133)
- `_hygiene.yml` — `has_rust`/`has_deps`/`has_docs` inputs gate sidebar-sync, markdown-lint, link-check, unused-deps, crate-doc-sync, todo-tracker; commit-msg-scan, commit-lint, pr-description, docs-drift, changelog always run (#133)
- `_code-review.yml` — `has_rust`/`has_ts`/`has_workflows`/`has_docs`/`has_deps` inputs gate architectural-review, test-quality-review, configuration-review; classify and code-review gate always run (#133)
- `docs/PR-Reviews.md` — documented change classification system, per-workflow skip matrix, PR type examples (#133)
- `ci-main.yml` unchanged — calls workflows without inputs, so all jobs run post-merge (inputs default to `true`) (#133)

### Previously Added
- CI Orchestrator Phase 4 — hygiene consolidation, auto-approve update, cleanup (#128)
- `_hygiene.yml` — reusable hygiene workflow consolidating commit-msg-scan, docs-check, pr-hygiene, and remaining ci.yml jobs (#128)
- `ci-main.yml` — post-merge CI on main, reuses `_test.yml` and `_build.yml` (#128)
- Hygiene depends on scope-gate via `needs:` in orchestrator — runs parallel with lint and security (#128)

### Changed
- `pr-pipeline.yml` — added hygiene stage, now orchestrates all PR checks (#128)
- `auto-approve.yml` — simplified to watch only "PR Pipeline" (all PR checks are under it) (#128)
- `docs/PR-Reviews.md` — updated pipeline architecture, workflow file listing, check tables (#128)
- `AGENT_PROMPT.md` — updated CI section with pipeline structure (#128)

### Removed
- `ci.yml` — all jobs migrated to reusable workflows; push trigger replaced by `ci-main.yml` (#128)
- `commit-msg-scan.yml` — merged into `_hygiene.yml` (#128)
- `docs-check.yml` — merged into `_hygiene.yml` (#128)
- `pr-hygiene.yml` — merged into `_hygiene.yml` (#128)

### Previously Added
- CI Orchestrator Phase 3 — code review, build, sandbox as reusable workflows under the orchestrator (#127)
- `_code-review.yml` — reusable code review workflow: file classification, architectural/test-quality/configuration LLM reviews (#127)
- `_build.yml` — reusable build workflow: release builds (multi-target matrix), binary size, MSRV, bridge extension (#127)
- `_sandbox.yml` — reusable sandbox workflow: real binary lifecycle, concurrency, crash recovery, shutdown testing (#127)
- Code review depends on lint-format + test via `needs:` in orchestrator — LLM reviews only code that compiles and passes tests (#127)
- Build depends on lint-format via `needs:` — build only runs after lint passes (#127)
- Sandbox depends on build via `needs:` — sandbox only runs after build passes (#127)

### Changed
- `OPENROUTER_API_KEY` secret passed explicitly from orchestrator to `_code-review.yml` via `workflow_call` secrets (#127)
- `auto-approve.yml` no longer watches "Architectural Review" (now part of "PR Pipeline") (#127)
- `ci.yml` report job slimmed to only track remaining hygiene checks (unused-deps, crate-doc-sync, todo-tracker, docs-drift, changelog) (#127)

### Removed
- `code-review.yml` standalone workflow — replaced by `_code-review.yml` reusable workflow (#127)
- `sandbox-test.yml` standalone workflow — replaced by `_sandbox.yml` reusable workflow (#127)
- `build-release`, `binary-size`, `build` (gate), `test-bridge`, `msrv` jobs removed from `ci.yml` — now run via `_build.yml` (#127)

### Previously Added
- CI Orchestrator Phase 2 — tests + security as reusable workflows under the orchestrator (#126)
- `_test.yml` — reusable test workflow: unit tests (per-crate matrix), integration tests, coverage (#126)
- `_security.yml` — reusable security workflow: secrets scan, credential patterns, cargo-audit, license compliance, unsafe code detection, npm audit (#126)
- Tests depend on lint-format via `needs:` in orchestrator — lint failure skips tests (#126)
- Security runs parallel with lint after scope gate — `needs: [scope-gate]` (#126)
- CI Orchestrator Phase 1 — `pr-pipeline.yml` orchestrates reusable workflows with `needs:` ordering (#125)
- `_scope-gate.yml` — reusable scope gate workflow (`on: workflow_call`) with `pr_number`/`pr_body` inputs (#125)
- `_lint-format.yml` — reusable lint/format workflow: Clippy, Rustfmt, docs compile (#125)
- Scope Gate Phase 1 workflow (`.github/workflows/scope-gate.yml`) — mechanical PR size + workstream checks (#119)
- `scripts/scope-gate.sh` — standalone scope gate logic with 3 checks: issue reference, size thresholds, workstream cohesion (#119)
- `scripts/test-scope-gate.sh` — 27 test cases for scope gate covering all thresholds and edge cases (#119)

### Changed
- Scope gate blocks lint via `needs:` — if scope gate fails, lint is skipped (#125)
- Lint job extracted from `ci.yml` into `_lint-format.yml` reusable workflow (#125)
- `auto-approve.yml` now watches "PR Pipeline" in addition to existing workflows (#125)
- PR size check removed from `pr-hygiene.yml` and consolidated into scope gate (#119)

### Removed
- `security.yml` standalone workflow — replaced by `_security.yml` reusable workflow (#126)
- `test-unit`, `test-integration`, `coverage` jobs removed from `ci.yml` — now run via `_test.yml` (#126)
- `security`, `license-check`, `unsafe-check` jobs removed from `ci.yml` — now run via `_security.yml` (#126)
- `"Security"` removed from `auto-approve.yml` watched workflows (now part of "PR Pipeline") (#126)
- `scope-gate.yml` standalone workflow — replaced by `_scope-gate.yml` reusable workflow (#125)
- `lint` job removed from `ci.yml` — now runs via `_lint-format.yml` (#125)

### Previously Added
- `FullTestServer` in test-utils — centralized API test server replacing duplicated boilerplate (#116)
- Enhanced `TestClient` with `put_json`, `patch_json`, `post_raw`, `get_concurrent`, `post_json_expect` methods (#116)
- New assertion macros: `assert_header!`, `assert_json_contains!`, `assert_openai_completion!`, `assert_events_contain!` (#116)
- `scripts/test-local.sh` — local test runner mirroring CI (lint + test + integration) (#116)
- Self-updating PR template with per-crate test checklists and local test enforcement (#116)
- Template sync workflow (`.github/workflows/template-sync.yml`) — auto-validates template structure (#116)
- Edge case tests: concurrent mutations, unicode content, double-delete idempotency, WebSocket flood, rapid pings (#116)

### Changed
- All API integration tests refactored to use `FullTestServer::new()` — zero duplicated `start_test_server()` (#116)
- LLM review prompts now inject real Architecture.md, Testing.md, and PR-Reviews.md as context (#116)
- Test quality review prompt enforces `FullTestServer` and project assertion macros (#116)
- Removed `#[ignore]` from CLI `test_daemon_lifecycle` — all tests now run (#116)
- PR template overhauled with per-crate checklists, architecture compliance, and local test evidence (#116)

### Previously Added
- Core types crate (`pi-daemon-types`) with agent, message, event, and error types (#4)
- Kernel crate (`pi-daemon-kernel`) with agent registry and event bus (#5)
- Config system with TOML files and environment variable overrides (#6)
- GitHub PAT authentication for private repo access (#6)
- API server (`pi-daemon-api`) with Axum HTTP routes and shared state (#7)
- WebSocket streaming chat handler with real-time messaging (#8)
- Embedded webchat SPA with Alpine.js and responsive design (#9)
- OpenAI-compatible `/v1/chat/completions` endpoint with streaming support (#10)
- CLI daemon lifecycle commands: start/stop/status/chat/config (#11)
- Sandbox integration test workflow for real binary lifecycle validation (#37)
- Unified code review system with intelligent file classification and multi-review support (#98, #101, #100, #104)
- Eliminate duplicate GitHub checks for each review type (#108)
- All review jobs now always comment with explanatory skip reasons (#106)
- Implement real LLM analysis for Architectural and Test Quality reviews (#110)
- Pi bridge extension (TypeScript) — connects pi TUI instances to pi-daemon kernel (#12)
- npm security audit for JavaScript/TypeScript dependencies (#99)
- Comprehensive CI/CD pipeline with 25+ automated checks (#24)
- Supply chain security checks with cargo-deny (#34)
- Code quality checks: unsafe detection, TODO tracking, docs drift, binary size (#35)
- Dynamic Check Gate system — discovers all PR checks automatically, no hardcoded names (#63/#65)
- Manual re-trigger for Check Gate via `workflow_dispatch` (#60/#61)

### Fixed
- Daemon no longer runs in foreground mode when `--foreground` flag is not provided (#113/#114)
- Implemented proper Unix background daemonization with terminal detachment via setsid
- Added daemon logging to `~/.pi-daemon/daemon.log` for background operations
- Process spawning approach maintains async runtime compatibility
- Background daemon processes are properly detached from TTY (show `?` in ps output)
- Sandbox integration test: fix hanging recovery test and broken pipe errors (#83)
- CLI detects stale daemon.json after crash and auto-cleans instead of refusing to start (#83)
- Recovery test properly waits for port to clear after kill -9 (#83)
- Health endpoint now includes timestamp for cache-busting (#83)
- Daemon no longer becomes unresponsive under sustained HTTP load (#86)
- Added concurrency limit (256 max in-flight requests) to prevent tokio runtime exhaustion (#86)
- Added 30s HTTP request timeout to drop stalled connections (#86)
- Enabled SO_REUSEADDR for faster port re-binding after crash/restart (#86)
- CI workflow permissions: added `pull-requests: write` so coverage, binary size, and PR report comments can post (#90)
- WebSocket connections now cleaned up promptly on abrupt client disconnect (#87)
- Added RAII `ConnectionGuard` so per-IP connection count is always decremented on drop (#87)
- Server-initiated WebSocket ping every 15s detects dead connections (#87)
- Read timeout (60s inactivity) closes zombie connections instead of waiting 30 min (#87)
- Idle timeout now correctly persists across loop iterations (pinned sleep) (#87)
- Docs Drift check now covers workflow file changes and fails instead of only warning (#69)
- Changelog check now covers workflow and Cargo.toml changes, not just `.rs` files (#69)
- Check quality checks use `exit 1` instead of `::warning::` so they actually block PRs (#69)
- Sandbox test memory monitoring showing unrealistic 1MB usage (#85)

### Infrastructure
- Workspace-based Rust project structure with 5 crates
- GitHub Actions workflows for security, testing, and quality assurance
- Branch protection with required status checks and reviews

## [0.1.0] - 2026-03-09

### Added
- Initial project structure and workspace setup (#3)
- Basic crate scaffolding for types, kernel, API, CLI, and test utilities
