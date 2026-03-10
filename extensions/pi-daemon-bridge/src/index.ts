import type { ExtensionContext } from "@mariozechner/pi-coding-agent";

// Configuration — read from env or ~/.pi-daemon/daemon.json
const DAEMON_URL = process.env.PI_DAEMON_URL || null;
const HEARTBEAT_INTERVAL_MS = 30_000;

interface DaemonInfo {
  pid: number;
  listen_addr: string;
  started_at: string;
  version: string;
}

interface AgentRegistration {
  agent_id: string;
  name: string;
}

export async function activate(context: ExtensionContext & {
  log?: (message: string) => void;
  on?: (event: string, handler: (data: any) => void) => void;
}) {
  // 1. Find the daemon
  const daemonUrl = await findDaemon();
  if (!daemonUrl) {
    context.log?.("pi-daemon-bridge: daemon not running, bridge inactive");
    return;
  }

  context.log?.(`pi-daemon-bridge: connecting to ${daemonUrl}`);

  // 2. Register this pi instance as an agent
  const agentName = `pi-${process.pid}`;
  let agentId: string | null = null;

  try {
    const resp = await fetch(`${daemonUrl}/api/agents`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: agentName,
        kind: "pi_instance",
        model: null, // Will be set when pi starts a conversation
      }),
    });

    if (resp.ok) {
      const data: AgentRegistration = await resp.json();
      agentId = data.agent_id;
      context.log?.(`pi-daemon-bridge: registered as ${agentName} (${agentId})`);
    } else {
      context.log?.(`pi-daemon-bridge: registration failed (${resp.status})`);
      return;
    }
  } catch (e) {
    context.log?.(`pi-daemon-bridge: connection failed: ${e}`);
    return;
  }

  // 3. Start heartbeat
  const heartbeatInterval = setInterval(async () => {
    try {
      await fetch(`${daemonUrl}/api/agents/${agentId}/heartbeat`, {
        method: "POST",
      });
    } catch {
      // Daemon may have stopped — will reconnect on next event
    }
  }, HEARTBEAT_INTERVAL_MS);

  // 4. Forward events to daemon
  // When pi completes a message exchange, notify the daemon
  context.on?.("message_end", async (event: any) => {
    if (!agentId) return;
    try {
      // Send the conversation event to the daemon
      // This allows webchat viewers to see what pi is doing
      await fetch(`${daemonUrl}/api/agents/${agentId}/events`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          type: "message_end",
          content: event.content || "",
          role: event.role || "assistant",
          tokens: event.usage || null,
        }),
      });
    } catch {
      // Best-effort — don't crash pi if daemon is down
    }
  });

  // 5. Forward tool executions
  context.on?.("tool_result", async (event: any) => {
    if (!agentId) return;
    try {
      await fetch(`${daemonUrl}/api/agents/${agentId}/events`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          type: "tool_result",
          tool_name: event.name || "unknown",
          success: !event.is_error,
        }),
      });
    } catch {}
  });

  // 6. Cleanup on shutdown
  context.on?.("deactivate", async () => {
    clearInterval(heartbeatInterval);
    if (agentId) {
      try {
        await fetch(`${daemonUrl}/api/agents/${agentId}`, {
          method: "DELETE",
        });
        context.log?.("pi-daemon-bridge: unregistered");
      } catch {}
    }
  });
}

/// Find the running daemon by reading ~/.pi-daemon/daemon.json
async function findDaemon(): Promise<string | null> {
  // Check env var first
  if (DAEMON_URL) return DAEMON_URL;

  // Read daemon.json
  try {
    const { readFile } = await import("fs/promises");
    const { homedir } = await import("os");
    const path = await import("path");

    const infoPath = path.join(homedir(), ".pi-daemon", "daemon.json");
    const content = await readFile(infoPath, "utf-8");
    const info: DaemonInfo = JSON.parse(content);

    // Verify daemon is actually responding
    const url = `http://${info.listen_addr}`;
    const resp = await fetch(`${url}/api/health`, { signal: AbortSignal.timeout(2000) });
    if (resp.ok) return url;
  } catch {
    // Daemon not running or not reachable
  }

  return null;
}