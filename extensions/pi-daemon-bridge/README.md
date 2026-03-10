# Pi Daemon Bridge Extension

A pi extension that connects a running pi instance to the pi-daemon kernel. When pi starts with this extension installed, it automatically registers itself as an agent and forwards activity to the daemon.

## Installation

```bash
# From source (development)
pi install /path/to/pi-daemon/extensions/pi-daemon-bridge

# From npm (future)
pi install @demonseed-jpg/pi-daemon-bridge
```

## How It Works

When pi starts with this extension:

1. **Discovery**: Looks for a running pi-daemon via `~/.pi-daemon/daemon.json` or `PI_DAEMON_URL` env var
2. **Registration**: Registers this pi instance as an agent with the daemon (name: `pi-{pid}`)
3. **Heartbeats**: Sends heartbeats every 30 seconds to keep the connection alive
4. **Event Forwarding**: Forwards pi's message completions and tool executions to the daemon
5. **Cleanup**: Unregisters cleanly when pi shuts down

## User Experience

**With daemon running:**
```
$ pi "fix the auth bug"
[pi-daemon-bridge: connecting to http://127.0.0.1:4200]
[pi-daemon-bridge: registered as pi-12345 (agent_abc123)]

# Meanwhile, webchat users see pi-12345 in the agent list
# and can observe pi's activity in real-time
```

**Without daemon:**
```
$ pi "fix the auth bug"
[pi-daemon-bridge: daemon not running, bridge inactive]

# Pi works exactly as before - no interference
```

## Configuration

- `PI_DAEMON_URL`: Override daemon URL (default: auto-discover from `~/.pi-daemon/daemon.json`)
- Heartbeat interval: 30 seconds (hardcoded)

## Events Forwarded

- `message_end`: When pi completes an LLM response
- `tool_result`: When pi finishes executing a tool (bash, read, edit, etc.)

All daemon communication is best-effort - if the daemon is unreachable, pi continues working normally without errors.

## Future Enhancements

- Bidirectional: webchat users can send messages TO a pi instance
- Session sync: pi sessions visible in daemon's session browser  
- Model negotiation: daemon tells pi which model to use
- Real-time streaming: pi's token-by-token output forwarded to webchat