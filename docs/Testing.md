# Testing

## Overview

pi-daemon uses a multi-tier testing strategy enforced by CI on every PR. No merge without green checks. Every test must run locally via `scripts/test-local.sh` before pushing.

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

---

## Running Tests Locally

**Every contributor must run tests locally before pushing.** Use the provided script:

```bash
# Full CI-equivalent local run (required before every push)
scripts/test-local.sh

# Single crate (for fast iteration)
cargo test -p pi-daemon-kernel

# With output
cargo test -- --nocapture

# Only integration tests
cargo test --all --test '*'

# Lint check (same as CI)
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

The PR template requires you to paste the output of `scripts/test-local.sh`. If you skip this, the `Local Test Evidence` CI check will flag your PR.

---

## Test Helpers (pi-daemon-test-utils)

All API integration tests **must** use the shared helpers in `pi-daemon-test-utils`. Do not duplicate test infrastructure.

### FullTestServer

The primary test helper. Boots a complete pi-daemon API server with a real kernel on a random port.

```rust
use pi_daemon_test_utils::FullTestServer;

// Default — random port, no auth, real kernel
let server = FullTestServer::new().await;
let client = server.client();

let resp = client.get("/api/health").await;
assert_eq!(resp.status(), 200);

// WebSocket URL
let ws_url = server.ws_url("my-agent");

// Custom config (auth tests, etc.)
let server = FullTestServer::with_config(DaemonConfig {
    api_key: "test-key".to_string(),
    ..Default::default()
}).await;
```

### TestClient

HTTP client with convenience methods. Use instead of raw `reqwest::Client`.

```rust
let client = server.client(); // or TestClient::new(&base_url)

// Standard verbs
client.get("/api/health").await;
client.post_json("/api/agents", &json!({"name": "a", "kind": "api_client"})).await;
client.put_json("/path", &body).await;
client.patch_json("/path", &body).await;
client.delete("/api/agents/id").await;

// Malformed input testing
client.post_raw("/path", "not json", "text/plain").await;

// Concurrency testing
let responses = client.get_concurrent("/api/status", 50).await;

// POST + assert status + parse JSON in one call
let json = client.post_json_expect("/api/agents", &body, 201).await;
```

### Assertion Macros

Use these instead of manual assertion chains:

```rust
use pi_daemon_test_utils::{
    assert_status, assert_json_ok, assert_header,
    assert_json_contains, assert_openai_completion, assert_events_contain,
};

// Status code
assert_status!(resp, 200);

// JSON key exists + parse
let json = assert_json_ok!(resp, "status");

// Header value contains substring
assert_header!(resp, "content-type", "application/json");

// JSON subset match
assert_json_contains!(resp, json!({"status": "ok"}));

// Full OpenAI response schema validation
assert_openai_completion!(body);

// Event type ordering in a list
assert_events_contain!(events, "System", "AgentRegistered");
```

### TestKernel

Isolated filesystem for kernel-level tests:

```rust
use pi_daemon_test_utils::TestKernel;

let kernel = TestKernel::new();
// kernel.data_dir is an isolated temp directory, cleaned up on drop
```

---

## Naming Conventions

| Pattern | When | Example |
|---------|------|---------|
| `test_<thing>_<behavior>` | Unit tests | `test_agent_id_new_is_unique` |
| `test_<thing>_<scenario>_<result>` | Edge cases | `test_config_load_missing_file_creates_default` |
| `test_<feature>_<flow>` | Integration | `test_agent_crud_lifecycle` |
| `e2e_<user_action>` | End-to-end | `e2e_register_agent_via_api` |

### Good Names

```rust
test_agent_id_new_is_unique
test_double_delete_agent_is_idempotent
test_heartbeat_nonexistent_agent_returns_404
test_concurrent_agent_register_delete_consistency
test_websocket_rapid_ping_flood
test_unicode_content_handling
test_streaming_chunk_ids_are_consistent
```

### Bad Names

```rust
test_it_works           // What works? Be specific
test_1                  // Numbers are not descriptions
test_api                // Too vague — which endpoint? What behavior?
test_agent_test         // Redundant — test is already in the name
check_stuff             // Not prefixed with test_
```

---

## What Good Tests Look Like

### Unit Test — testing a single function

```rust
#[test]
fn test_agent_kind_serialization() {
    let kind = AgentKind::PiInstance;
    let json = serde_json::to_string(&kind).unwrap();
    assert_eq!(json, "\"pi_instance\"");

    let roundtrip: AgentKind = serde_json::from_str(&json).unwrap();
    assert_eq!(kind, roundtrip);
}
```

Why it's good: Tests one thing (serde roundtrip), clear assertion, descriptive name.

### Integration Test — testing API behavior end-to-end

```rust
#[tokio::test]
async fn test_heartbeat_nonexistent_agent_returns_404() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let fake_id = "00000000-0000-0000-0000-000000000000";
    let resp = client
        .post_json(&format!("/api/agents/{fake_id}/heartbeat"), &json!({}))
        .await;
    assert_eq!(resp.status(), 404);
}
```

Why it's good: Uses `FullTestServer`, tests error path not just happy path, descriptive name encodes the expected outcome.

### Concurrency Test — validating thread safety

```rust
#[tokio::test]
async fn test_concurrent_agent_register_delete_consistency() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let mut handles = Vec::new();
    for i in 0..20 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            c.post_json("/api/agents", &json!({
                "name": format!("concurrent-{i}"), "kind": "api_client"
            })).await
        }));
    }
    let results = futures::future::join_all(handles).await;
    for result in &results {
        assert_eq!(result.as_ref().unwrap().status(), 201);
    }

    let resp = client.get("/api/agents").await;
    let agents: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(agents.as_array().unwrap().len(), 20);
}
```

Why it's good: Tests actual concurrency (not sequential), verifies consistency after concurrent operations.

### Edge Case Test — testing boundaries

```rust
#[tokio::test]
async fn test_unicode_content_handling() {
    let server = FullTestServer::new().await;
    let body = server.client().post_json_expect(
        "/v1/chat/completions",
        &json!({
            "model": "test",
            "messages": [{"role": "user", "content": "こんにちは 🌍 مرحبا"}],
            "stream": false
        }),
        200,
    ).await;
    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(content.contains("こんにちは"));
}
```

Why it's good: Tests a real edge case (multi-script unicode), validates the content roundtrips correctly.

---

## What Bad Tests Look Like

### Bad: Duplicated server boilerplate

```rust
// ❌ DO NOT DO THIS — use FullTestServer
async fn start_test_server() -> String {
    let kernel = Arc::new(PiDaemonKernel::new());
    kernel.init().await;
    let config = DaemonConfig { listen_addr: "127.0.0.1:0".to_string(), ..Default::default() };
    let (router, _) = build_router(kernel, config);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap() });
    tokio::time::sleep(Duration::from_millis(100)).await;
    format!("http://127.0.0.1:{}", addr.port())
}
```

Fix: `let server = FullTestServer::new().await;`

### Bad: No error path testing

```rust
// ❌ Only tests the happy path
#[tokio::test]
async fn test_register_agent() {
    let server = FullTestServer::new().await;
    let resp = server.client().post_json("/api/agents", &json!({"name": "a", "kind": "api_client"})).await;
    assert_eq!(resp.status(), 201);
}
```

Fix: Also test what happens with missing fields, invalid kind, empty name, duplicate registration.

### Bad: Ignored test

```rust
// ❌ DO NOT USE #[ignore] — restructure the test instead
#[tokio::test]
#[ignore] // Requires actual daemon running
async fn test_daemon_lifecycle() { }
```

Fix: Use `FullTestServer` to create an in-process server. If it truly needs a binary, put it in the sandbox workflow.

### Bad: Timing-dependent assertions

```rust
// ❌ Will be flaky in CI
#[tokio::test]
async fn test_something() {
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(something_happened());
}
```

Fix: Use a loop with timeout polling, or test state directly instead of relying on wall-clock time.

### Bad: Bare unwrap in assertions

```rust
// ❌ Panic message gives zero context
let val = resp.json::<Value>().await.unwrap();
```

Fix: Use `expect("health response should be valid JSON")` or assertion macros.

---

## Dos and Don'ts

### Do

- ✅ Use `FullTestServer::new()` for all API/WebSocket tests
- ✅ Use `TestClient` and assertion macros from `pi-daemon-test-utils`
- ✅ Test both success and error paths for every endpoint
- ✅ Test edge cases: empty input, unicode, huge payloads, concurrent access
- ✅ Name tests `test_<thing>_<behavior>` — the name should describe the assertion
- ✅ Run `scripts/test-local.sh` before every push
- ✅ Add integration tests in `crates/<crate>/tests/` for cross-module behavior
- ✅ Use `expect("message")` instead of `unwrap()` in test code
- ✅ Keep tests isolated — no shared mutable state, use `TestKernel` for temp dirs
- ✅ Test concurrent access for any new shared state (DashMap, broadcast, etc.)

### Don't

- ❌ Don't call real external APIs (GitHub, Telegram, LLM providers) in tests
- ❌ Don't write to `~/.pi-daemon/` — use `tempfile::TempDir` or `TestKernel`
- ❌ Don't use `tokio::time::sleep` for timing — poll with timeout or test state directly
- ❌ Don't use `#[ignore]` — restructure the test to not need it
- ❌ Don't duplicate `start_test_server()` boilerplate — use `FullTestServer`
- ❌ Don't use raw `reqwest::Client` — use `TestClient` from test-utils
- ❌ Don't test only the happy path — every new endpoint needs error case tests
- ❌ Don't use `println!` for debugging tests — use `tracing` or `--nocapture`
- ❌ Don't add `#[allow(dead_code)]` or `#[allow(unused)]` to suppress warnings

---

## PR Template Testing Requirements

The PR template (`.github/pull_request_template.md`) enforces testing standards on every PR. Contributors must:

1. **Check which crates they modified** — the template has per-crate checkboxes
2. **Complete the per-crate test checklist** — each crate section lists required test categories
3. **Paste `scripts/test-local.sh` output** — proves tests were run locally
4. **Confirm test quality standards** — no `#[ignore]`, no duplicated boilerplate, proper naming

### When the Template Must Be Updated

The template has marker comments (`CRATE_CHECKLIST_MARKER`, `CRATE_TEST_SECTION_MARKER`, `CI_CHECKLIST_MARKER`) for auto-sync. Update the template when:

| Trigger | What to Update | How |
|---------|---------------|-----|
| New crate added to workspace | Crate checklist + per-crate test section | Add a checkbox and test section for the new crate |
| New CI workflow added | CI checks reference list | Add the workflow name to the CI section |
| New test-utils helper added | Test quality standards section | Reference the new helper so reviewers know to use it |
| Test helper API changes | Per-crate test section | Update the command examples if test invocation changes |

The `template-sync.yml` workflow runs on push to main (when `Cargo.toml`, workflow files, or test-utils change) and weekly on Monday. It validates the template structure and warns if sections are missing or out of date.

### Manual Update Process

1. Edit `.github/pull_request_template.md`
2. Keep the marker comments intact — the sync workflow uses them
3. Ensure every workspace crate has a checkbox and a test section
4. Ensure the CI checklist matches the actual workflow jobs
5. Run `cargo test --all` to verify nothing is broken
6. PR the template change like any other code change

---

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
6. **Update the PR template** — add a crate checkbox and per-crate test section
7. Verify with `scripts/test-local.sh`

---

## Sandbox Integration Testing

The sandbox test (`sandbox-test.yml`) runs the actual compiled `pi-daemon` binary through its full lifecycle. It catches deployment bugs that in-process tests cannot detect:

- **Smoke Tests**: Health checks, API endpoints, webchat loading, PID management
- **Load Tests**: Concurrent HTTP requests, agent registrations, WebSocket connections
- **Memory Monitoring**: Multi-method measurement with realistic validation (expected: 20-50MB)
- **Stress Testing**: Sustained load testing with memory leak detection
- **Recovery Testing**: Kill -9 and graceful restart validation
- **CLI Testing**: Command behavior when daemon is/isn't running

### Memory Monitoring

```bash
# Multiple measurement methods for reliability
RSS_METHOD=$(ps -o rss= -p $DAEMON_PID | tr -d ' ')           # Portable
VMRSS_METHOD=$(grep "^VmRSS:" /proc/$PID/status | awk '{print $2}')  # Linux, accurate
TREE_METHOD=$(ps -o rss= --ppid $PID | awk '{sum+=$1} END {print sum+0}')  # Include children

# Fails if < 5MB (indicates measurement error, not actual efficiency)
# Warns if > 200MB
```

---

## LLM-Powered Test Quality Review

Every PR that modifies test files triggers an automated test quality review via Gemini 2.5 Flash. The review:

1. Receives the full `Testing.md` (this document) and the `pi-daemon-test-utils` API as context
2. Checks each test change against the standards documented above
3. Fails the PR if tests duplicate boilerplate, skip error paths, or violate naming conventions

The LLM review is not a replacement for human review — it catches mechanical violations. See [[PR-Reviews]] for the full check catalog.

---

## CI Pipeline

Every PR gets a comment with:
- ✅/❌ status for each CI job
- Code coverage report with per-crate breakdown
- Link to the full Actions run

See [[PR-Reviews]] for the full CI check breakdown.
