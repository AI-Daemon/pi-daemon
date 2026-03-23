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

## Configuration

### Network Access

By default, pi-daemon binds to `0.0.0.0:4200` for network access from mobile devices and other machines on your network.

**Security Note**: This allows any device on your network to access the daemon. For localhost-only access, set:

```bash
export PI_DAEMON_LISTEN_ADDR="127.0.0.1:4200"
```

Or edit `~/.pi-daemon/config.toml`:
```toml
listen_addr = "127.0.0.1:4200"
```

### Environment Variables

- `PI_DAEMON_LISTEN_ADDR` — Override listen address (default: `0.0.0.0:4200`)
- `PI_DAEMON_API_KEY` — Set API key for authentication
- `ANTHROPIC_API_KEY` — Anthropic API key
- `OPENAI_API_KEY` — OpenAI API key
- `GITHUB_TOKEN` — GitHub Personal Access Token

### Troubleshooting Network Access

If mobile devices can't connect even with `0.0.0.0:4200`:

1. **Check your machine's IP address**: Use `hostname -I` (Linux) or `ipconfig` (Windows)
2. **Test the port**: Verify the daemon is actually listening with `netstat -tlnp | grep 4200`
3. **Firewall**: Ensure port 4200 is allowed through your firewall:
   - **Linux**: `sudo ufw allow 4200` or `sudo firewall-cmd --add-port=4200/tcp`
   - **macOS**: System Preferences → Security & Privacy → Firewall → Options
   - **Windows**: Windows Defender Firewall → Advanced Settings → Inbound Rules
4. **Network connectivity**: Test basic connectivity with `ping [machine-ip]` from mobile device
5. **Alternative port**: Try a different port like 8080: `PI_DAEMON_LISTEN_ADDR=0.0.0.0:8080`
6. **iOS Local Network**: iOS 14+ requires apps to request local network permissions

## Status

🚧 Under construction. See [Issues](../../issues) for the phased build plan.

## License

MIT
