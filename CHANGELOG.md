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
- Comprehensive CI/CD pipeline with 25+ automated checks (#24)
- Supply chain security checks with cargo-deny (#34)
- Code quality checks: unsafe detection, TODO tracking, docs drift, binary size (#35)
- Dynamic Check Gate system — discovers all PR checks automatically, no hardcoded names (#63/#65)
- Manual re-trigger for Check Gate via `workflow_dispatch` (#60/#61)

### Fixed
- Docs Drift check now covers workflow file changes and fails instead of only warning (#69)
- Changelog check now covers workflow and Cargo.toml changes, not just `.rs` files (#69)
- Check quality checks use `exit 1` instead of `::warning::` so they actually block PRs (#69)

### Infrastructure
- Workspace-based Rust project structure with 5 crates
- GitHub Actions workflows for security, testing, and quality assurance
- Branch protection with required status checks and reviews

## [0.1.0] - 2026-03-09

### Added
- Initial project structure and workspace setup (#3)
- Basic crate scaffolding for types, kernel, API, CLI, and test utilities
