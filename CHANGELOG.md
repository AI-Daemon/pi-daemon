# Changelog

All notable changes to pi-daemon will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
- Pi bridge extension (TypeScript) — connects pi TUI instances to pi-daemon kernel (#12)
- Comprehensive CI/CD pipeline with 25+ automated checks (#24)
- Supply chain security checks with cargo-deny (#34)
- Code quality checks: unsafe detection, TODO tracking, docs drift, binary size (#35)
- Dynamic Check Gate system — discovers all PR checks automatically, no hardcoded names (#63/#65)
- Manual re-trigger for Check Gate via `workflow_dispatch` (#60/#61)

### Fixed
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
