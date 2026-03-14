# Architecture

## Design Philosophy

pi-daemon is a **single Rust binary** that compiles 6+ crates into one executable. It runs as a background daemon that pi (and other clients) connect to. The key principles:

1. **Single binary** — no runtime dependencies, no Docker, no database servers
2. **Pi stays unchanged** — pi-daemon is additive, pi works fine without it
3. **Incremental** — each phase delivers standalone value
4. **Concurrent by default** — DashMap, tokio broadcast channels, Arc everywhere
5. **Test-first** — CI gates every PR with lint, unit, integration, E2E, coverage, security audit

## Crate Structure

```
pi-daemon/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── pi-daemon-types/          # Shared types, traits, errors — no logic
│   ├── pi-daemon-kernel/         # Agent registry, event bus, config, scheduler, supervisor
│   ├── pi-daemon-api/            # Axum HTTP + WebSocket server, webchat UI
│   ├── pi-daemon-cli/            # Binary entry point + CLI commands
│   ├── pi-daemon-test-utils/     # Shared test helpers
│   ├── pi-daemon-pi-manager/     # Managed Pi process lifecycle — discovery, install, spawn, health
│   ├── pi-daemon-provider/       # LLM provider clients — streaming completions (Anthropic, OpenAI, OpenRouter)
│   ├── pi-daemon-memory/         # SQLite WAL memory substrate (Phase 2)
│   ├── pi-daemon-hands/          # Autonomous capability packages (Phase 4)
│   ├── pi-daemon-wire/           # Agent-to-agent P2P networking (Phase 5)
│   └── pi-daemon-channels/       # Telegram, Slack, Discord adapters (Phase 5)
├── hands/                        # Bundled Hand manifests + system prompts
├── tests/e2e/                    # End-to-end tests
└── docs/                         # Documentation (synced to wiki)
```

## Dependency Graph

```
pi-daemon-cli
  ├── pi-daemon-api
  │     ├── pi-daemon-kernel
  │     │     ├── pi-daemon-types
  │     │     └── pi-daemon-memory
  │     ├── pi-daemon-types
  │     ├── pi-daemon-provider
  │     ├── pi-daemon-pi-manager
  │     ├── pi-daemon-hands
  │     └── pi-daemon-channels
  ├── pi-daemon-pi-manager
  └── pi-daemon-kernel
```

Strict rule: **no circular dependencies**. Types flows up, logic flows down.

## Core Subsystems

### Agent Registry
Concurrent `DashMap<AgentId, AgentEntry>` tracking all connected agents — pi instances, webchat sessions, API clients, Hands. Supports register, unregister, heartbeat, status updates, find-by-name.

### Event Bus
`tokio::sync::broadcast` channel with per-agent targeted channels and a 1000-event ring buffer history. Every significant action (agent register, tool execution, status change) publishes an event. Triggers subscribe to the bus for reactive automation.

### Kernel
The `PiDaemonKernel` struct composes all subsystems:
- `registry: AgentRegistry`
- `event_bus: EventBus`
- `scheduler: CronScheduler`
- `supervisor: Supervisor`
- `triggers: TriggerEngine`
- `memory: Arc<MemorySubstrate>`
- `hands: HandRegistry`
- `approvals: ApprovalManager`

### Memory Substrate
Embedded SQLite in WAL mode. Four stores behind one interface:
- **StructuredStore** — per-agent key-value storage
- **SessionStore** — conversation sessions with messages
- **UsageStore** — token/cost tracking with daily aggregation
- **FragmentStore** — memory fragments with relevance decay

This is the **fast local tier**. Neo4j/Graphiti remains the **rich graph tier** for entity relationships and temporal reasoning.

### API Server
Axum HTTP server with:
- REST endpoints for agent management, sessions, usage, scheduler, approvals
- WebSocket endpoint for real-time streaming chat
- OpenAI-compatible `/v1/chat/completions` (streaming SSE + non-streaming)
- Embedded webchat SPA (compiled into binary via `include_str!()`)
- Auth middleware (API key), CORS, compression, tracing

### Hands
Autonomous capability packages defined by `HAND.toml` manifests. Each Hand has:
- A schedule (cron/interval/daily)
- A multi-phase system prompt
- Required tools and capabilities
- Approval gates for sensitive actions
- Persistent state and metrics

### Wire Protocol
TCP-based JSON-RPC framing with HMAC-SHA256 authentication for agent-to-agent communication across machines.

## Data Flow

```
User message (webchat/API/terminal/pi)
       │
       ▼
   API Server (Axum)
       │
       ├──→ Agent Registry (who handles this?)
       ├──→ Event Bus (publish UserMessage event)
       ├──→ Triggers (check for reactive automations)
       │
       ▼
   Agent Loop (LLM call → tool execution → response)
       │
       ├──→ Memory Substrate (save session, track usage)
       ├──→ Event Bus (publish AgentResponse event)
       │
       ▼
   Response streamed back to client (WebSocket text_delta / SSE chunks)
```

## Technology Choices

| Component | Choice | Why |
|-----------|--------|-----|
| Language | Rust | Single binary, memory safety, performance |
| Async runtime | Tokio | Industry standard, full-featured |
| HTTP server | Axum | Tower ecosystem, type-safe extractors |
| Database | SQLite (rusqlite, bundled) | Zero dependencies, embedded, WAL for concurrency |
| Concurrency | DashMap | Lock-free concurrent HashMap |
| Serialization | serde + serde_json + toml | Ecosystem standard |
| CLI | clap | Derive-based, feature-rich |
| Frontend | Alpine.js | Tiny, no build step, perfect for embedded SPA |
| Logging | tracing | Structured, async-compatible |
