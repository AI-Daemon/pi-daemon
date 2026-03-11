# Contributing

## Getting Started

1. Fork and clone the repo
2. Install Rust 1.94+: `rustup update stable`
3. Build: `cargo build`
4. Test: `cargo test --all`
5. Lint: `cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`

## Issue Templates

Use the templates in `docs/templates/` when filing new issues:

- **[Bug Report](templates/bug-report.md)** — for defects, regressions, and unexpected behavior
- **[Feature Request](templates/feature-request.md)** — for enhancements, new capabilities, and improvements

Copy the template, fill in each section, and delete the guidance comments.

## PR Process

1. Create a branch from `main`
2. Implement your changes with tests
3. Run the full test suite locally: `cargo test --all`
4. Run the lint check: `cargo clippy --all-targets --all-features -- -D warnings`
5. Push and open a PR against `main`
6. CI runs automatically — all checks must pass
7. A coverage report is posted as a PR comment
8. A summary of all CI job results is posted as a PR comment
9. Get review and merge

## PR Template

Every PR should include:
- Description of what changed
- Test checklist (unit, integration, E2E as applicable)
- Reference to the issue being addressed

## Code Style

- **Zero clippy warnings** — `-D warnings` is enforced in CI
- **Zero rustfmt violations** — `cargo fmt` is enforced in CI
- **Doc comments** on all public items — `RUSTDOCFLAGS="-D warnings"` is enforced
- **Error handling** — use `thiserror` for typed errors, `anyhow` at boundaries
- **Logging** — use `tracing` macros (`info!`, `warn!`, `debug!`), never `println!`
- **Concurrency** — prefer `DashMap` over `Mutex<HashMap>`, `broadcast` over `mpsc` for pub/sub
- **Naming** — snake_case for everything Rust, types match the module they're in

## Adding a New Crate

1. Create `crates/<name>/` with `Cargo.toml` and `src/lib.rs`
2. Add to workspace members in root `Cargo.toml`
3. Add to the CI test matrix in `.github/workflows/ci.yml`
4. Add test helpers to `pi-daemon-test-utils` if needed
5. Add documentation to `docs/`

## Documentation

All documentation lives in `docs/` and is automatically synced to the GitHub Wiki on push to `main`. Edit docs via PR — never edit the wiki directly.

See [[Testing]] for the full test infrastructure guide.
