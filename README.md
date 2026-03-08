# pi-daemon

A Rust-based agent kernel daemon that runs alongside [pi](https://github.com/mariozechner/pi-coding-agent). Provides persistent agent lifecycle management, autonomous scheduling, webchat UI, and an API layer — all compiled into a single binary.

## Architecture

Pi stays as the interactive TUI coding agent. pi-daemon runs as a background service that pi connects to, adding:

- **Webchat UI** — Chat with agents from any browser
- **REST + WebSocket API** — OpenAI-compatible `/v1/chat/completions`
- **Agent Registry** — Track all running agent instances
- **Scheduler** — Cron-based autonomous agent execution
- **Supervisor** — Health monitoring, auto-restart, resource limits
- **Memory Substrate** — Fast embedded SQLite layer (complements Neo4j/Graphiti)
- **Hands** — Autonomous capability packages that work for you on schedules
- **Wire Protocol** — Agent-to-agent P2P networking

## Status

🚧 Under construction. See [Issues](../../issues) for the phased build plan.

## License

MIT
