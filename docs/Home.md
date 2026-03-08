# pi-daemon

A Rust-based agent kernel daemon that runs alongside [pi](https://github.com/mariozechner/pi-coding-agent). Single binary. Persistent agents. Webchat. API. Autonomous scheduling.

## Quick Links

- [[Architecture]] — System design, crate structure, dependency graph
- [[Getting-Started]] — Installation, first run, configuration
- [[Configuration]] — Config file reference, environment variables, GitHub auth
- [[API-Reference]] — REST endpoints, WebSocket protocol, OpenAI-compatible API
- [[Phases]] — Roadmap overview and build plan
- [[Testing]] — Test infrastructure, running tests, adding new tests
- [[Contributing]] — How to contribute, PR process, code style

## What is pi-daemon?

Pi stays as the interactive TUI coding agent you know. pi-daemon runs as a **background service** that pi connects to, adding:

| Feature | Description |
|---------|-------------|
| **Webchat UI** | Chat with agents from any browser at `http://localhost:4200` |
| **REST + WebSocket API** | Full agent management API, OpenAI-compatible `/v1/chat/completions` |
| **Agent Registry** | Track all running agent instances across pi, webchat, API clients |
| **Scheduler** | Cron-based autonomous agent execution |
| **Supervisor** | Health monitoring, auto-restart, resource limits |
| **Memory Substrate** | Fast embedded SQLite layer (complements Neo4j/Graphiti) |
| **Hands** | Autonomous capability packages that work for you on schedules |
| **Wire Protocol** | Agent-to-agent P2P networking across machines |
| **Channels** | Telegram, Slack, Discord adapters |

## Architecture at a Glance

```
User Browser ──HTTP/WS──→ pi-daemon (Rust, port 4200)
                              │
Pi TUI ────HTTP/Unix──────→   │──→ Agent Registry
                              │──→ Event Bus
Terminal ──`pi-daemon chat`──→│──→ Scheduler
                              │──→ Supervisor
                              ├──→ Memory Substrate (SQLite)
                              ├──→ Hands (autonomous agents)
                              └──→ LLM APIs (Anthropic/OpenAI/etc.)
```

## Status

🚧 Under construction. See the [Roadmap](https://github.com/users/Demonseed-jpg/projects/3) for the phased build plan.

---
*Last updated: 2026-03-08 18:59 UTC*
