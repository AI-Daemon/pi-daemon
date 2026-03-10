## Description
<!-- What does this PR do? Why? Reference the issue. -->

Closes #<!-- issue number -->

## Crates Modified
<!-- Check the crates you modified in this PR. -->
- [ ] `pi-daemon-types`
- [ ] `pi-daemon-kernel`
- [ ] `pi-daemon-api`
- [ ] `pi-daemon-cli`
- [ ] `pi-daemon-test-utils`
<!-- CRATE_CHECKLIST_MARKER — auto-updated by template-sync workflow -->

## Local Test Verification

> **⚠️ REQUIRED: All tests must pass locally before pushing.**
> Run `scripts/test-local.sh` and paste the final output line below.

**Local test output:**
```
<!-- PASTE OUTPUT HERE, e.g.: ✅ All local tests passed — safe to push -->
```

## Per-Crate Test Checklist

> Check only the crates you modified. **Every checked crate must have passing tests.**

#### If you modified `pi-daemon-types`:
- [ ] Unit tests pass: `cargo test -p pi-daemon-types`
- [ ] Serialization roundtrip tests for any new types
- [ ] `Display` impl tests for any new ID types

#### If you modified `pi-daemon-kernel`:
- [ ] Unit tests pass: `cargo test -p pi-daemon-kernel`
- [ ] Integration tests pass: `cargo test -p pi-daemon-kernel --test '*'`
- [ ] Event bus tests cover new event types (if any)
- [ ] Registry tests cover new operations (if any)
- [ ] Concurrent access tests for any new shared state

#### If you modified `pi-daemon-api`:
- [ ] Unit tests pass: `cargo test -p pi-daemon-api`
- [ ] Integration tests pass: `cargo test -p pi-daemon-api --test '*'`
- [ ] New endpoints have integration tests using `FullTestServer`
- [ ] Error responses tested (400, 404, 422 paths)
- [ ] OpenAI compat tests updated if `/v1/` routes changed

#### If you modified `pi-daemon-cli`:
- [ ] Unit tests pass: `cargo test -p pi-daemon-cli`
- [ ] CLI arg parsing tests for new subcommands/flags
- [ ] `assert_cmd` tests for new CLI surface area

#### If you modified `pi-daemon-test-utils`:
- [ ] Self-tests pass: `cargo test -p pi-daemon-test-utils`
- [ ] Downstream crate tests still pass (changes are backward-compatible)
- [ ] New helpers have doc comments with usage examples

<!-- CRATE_TEST_SECTION_MARKER — auto-updated by template-sync workflow -->

## Test Quality Standards

- [ ] **No `#[ignore]`** — All tests run. If a test needs a running daemon, use `FullTestServer`
- [ ] **No `start_test_server()` duplication** — Use `FullTestServer::new()` from test-utils
- [ ] **No raw `unwrap()` in assertions** — Use `expect("descriptive message")` or assertion macros
- [ ] **Test naming** — `test_<thing>_<behavior>` for unit, `test_<feature>_<scenario>_<result>` for edge cases
- [ ] **Test isolation** — No shared mutable state between tests. Use `TestKernel` for isolated environments
- [ ] **Edge cases** — New code has tests for error paths, not just happy paths

## Architecture Compliance

- [ ] **Dependency graph** — No new cross-crate deps that violate `types → kernel → api → cli` ordering
- [ ] **Concurrency** — Uses `DashMap` (not `Mutex<HashMap>`), `broadcast` for pub/sub, `Arc` for sharing
- [ ] **Error handling** — `thiserror` for typed errors, `anyhow` at boundaries, no `unwrap()` in lib code
- [ ] **Logging** — `tracing` macros only (`info!`, `warn!`, `debug!`), never `println!`

## Documentation

- [ ] Doc comments (`///`) on all new public items
- [ ] `docs/Architecture.md` updated if new crate or subsystem added
- [ ] `docs/API-Reference.md` updated if new routes added
- [ ] `CHANGELOG.md` updated with entry under `[Unreleased]`

## CI Checks (automated — do not manually check)
<!-- These run automatically. Listed for reference only. -->
- Lint (fmt + clippy + doc warnings)
- Unit tests (per-crate matrix)
- Integration tests
- Coverage report (posted as comment)
- Security audit (cargo-audit + TruffleHog + credential patterns)
- License & supply chain (cargo-deny)
- Binary size tracking
- Architectural review (Gemini 2.5 Flash via OpenRouter)
- Test quality review (Gemini 2.5 Flash via OpenRouter)
- Configuration review (Gemini 2.5 Flash via OpenRouter)
- Sandbox integration (real binary lifecycle)
- PR hygiene (size, commit messages, description)
- Documentation (sidebar sync, link check, drift detection)
<!-- CI_CHECKLIST_MARKER — auto-updated by template-sync workflow -->
