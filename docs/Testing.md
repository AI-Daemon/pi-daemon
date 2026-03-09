# Testing

## Overview

pi-daemon uses a multi-tier testing strategy enforced by CI on every PR. No merge without green checks.

## Test Tiers

| Tier | Location | Runs In CI | What It Tests |
|------|----------|-----------|---------------|
| **Unit** | `#[cfg(test)] mod tests` inside modules | Per-crate matrix (parallel) | Individual functions, types, logic |
| **Doc** | `///` doc comments with examples | Per-crate | API contracts, usage examples |
| **Integration** | `crates/*/tests/*.rs` | After lint | Cross-module interactions within a crate |
| **E2E** | `tests/e2e/*.rs` | After unit | Full daemon boot, HTTP requests, WebSocket flows |
| **Coverage** | cargo-llvm-cov | After unit | Posted as PR comment |
| **Security** | cargo-audit | Always | Known vulnerability advisories |
| **Sandbox** | `.github/workflows/sandbox-test.yml` | On PR | Real binary lifecycle, stress testing, memory monitoring |

## Sandbox Integration Testing

The sandbox test runs the actual compiled `pi-daemon` binary through its full lifecycle:

- **Smoke Tests**: Health checks, API endpoints, webchat loading, PID management
- **Load Tests**: Concurrent HTTP requests, agent registrations, WebSocket connections  
- **Memory Monitoring**: Multi-method memory measurement with realistic validation
- **Stress Testing**: Sustained load testing with memory leak detection
- **Recovery Testing**: Kill -9 and graceful restart validation
- **CLI Testing**: Command behavior when daemon is/isn't running

### Memory Monitoring

The sandbox test includes comprehensive memory monitoring:

```bash
# Multiple measurement methods for reliability
RSS_METHOD=$(ps -o rss= -p $DAEMON_PID | tr -d ' ')           # Portable
VMRSS_METHOD=$(grep "^VmRSS:" /proc/$PID/status | awk '{print $2}')  # Linux, accurate
TREE_METHOD=$(ps -o rss= --ppid $PID | awk '{sum+=$1} END {print sum+0}')  # Include children

# Realistic validation
# Expected: 20-50MB for Rust daemon (binary + Axum + tokio + assets)
# Fails if < 5MB (indicates measurement error, not actual efficiency)
```

The test validates that memory usage is realistic for a Rust daemon with embedded assets and web framework, preventing false positives from measurement errors.

## Running Tests Locally

```bash
# All tests
cargo test --all

# Single crate
cargo test -p pi-daemon-kernel

# With output
cargo test -- --nocapture

# Only integration tests
cargo test --all --test '*'

# Only E2E
cargo test --test e2e

# Lint check (same as CI)
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

## Test Helpers

The `pi-daemon-test-utils` crate provides shared utilities:

```rust
use pi_daemon_test_utils::{test_kernel, test_server, TestClient};

// Create an isolated kernel with temp directory
let (kernel, _tmp) = test_kernel();

// Boot an ephemeral API server on random port
let (base_url, state) = test_server().await;

// HTTP client with assertion helpers
let client = TestClient::new(&base_url);
let resp = client.get("/api/health").await;
assert_status!(resp, 200);
let json = assert_json_ok!(resp, "status");
```

## Naming Conventions

```
test_<thing>_<behavior>           — unit test
test_<thing>_<scenario>_<result>  — edge case
integration_<feature>_<flow>      — integration test
e2e_<user_action>                 — end-to-end test
```

Examples:
```rust
#[test] fn test_agent_id_new_is_unique() { ... }
#[test] fn test_config_load_missing_file_creates_default() { ... }
#[tokio::test] async fn test_event_bus_broadcast_reaches_subscriber() { ... }
#[tokio::test] async fn e2e_register_agent_via_api() { ... }
```

## Adding Tests for a New Crate

1. Write unit tests inside each module (`#[cfg(test)] mod tests`)
2. Write integration tests in `crates/<your-crate>/tests/`
3. Add E2E tests in `tests/e2e/` if the feature has API endpoints
4. Add the crate to the CI matrix in `.github/workflows/ci.yml`:
   ```yaml
   matrix:
     crate:
       - pi-daemon-types
       - pi-daemon-kernel
       - your-new-crate  # ← add here
   ```
5. Add helpers to `pi-daemon-test-utils` if other crates will need them

## CI Pipeline

See [[CI-Pipeline]] for the full GitHub Actions workflow breakdown.

Every PR gets a comment with:
- ✅/❌ status for each CI job
- Code coverage report with per-crate breakdown
- Link to the full Actions run

## What Not To Do

- ❌ Don't call real external APIs (GitHub, Telegram, LLM providers) in tests
- ❌ Don't write to `~/.pi-daemon/` — use `tempfile::TempDir`
- ❌ Don't use `tokio::time::sleep` for timing tests — use controlled time or direct state manipulation
- ❌ Don't skip tests with `#[ignore]` unless they truly require a real daemon — prefer mocks
