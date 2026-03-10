pub mod daemon;

use clap::{Parser, Subcommand};
use pi_daemon_kernel::config::{
    config_path, load_config, read_daemon_info, remove_daemon_info, write_daemon_info,
};
use pi_daemon_types::config::DaemonInfo;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "pi-daemon", version, about = "Agent kernel daemon for pi")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(long, short)]
        foreground: bool,
        /// Override listen address
        #[arg(long)]
        listen: Option<String>,
    },
    /// Stop a running daemon
    Stop,
    /// Show daemon status
    Status,
    /// Interactive terminal chat
    Chat {
        /// Agent name or ID to chat with (default: first available)
        #[arg(long, short)]
        agent: Option<String>,
        /// Model to use
        #[arg(long, short)]
        model: Option<String>,
    },
    /// Print version
    Version,
    /// Show configuration (secrets redacted)
    Config,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { foreground, listen } => cmd_start(foreground, listen).await,
        Commands::Stop => cmd_stop().await,
        Commands::Status => cmd_status().await,
        Commands::Chat { agent, model } => cmd_chat(agent, model).await,
        Commands::Version => {
            println!("pi-daemon v{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Commands::Config => cmd_config().await,
    }
}

async fn cmd_start(foreground: bool, listen_override: Option<String>) -> anyhow::Result<()> {
    // Load config first (before daemonizing to catch config errors early)
    let mut config = load_config()?;
    if let Some(addr) = listen_override {
        config.listen_addr = addr;
    }

    // Check if daemon is already running (before daemonizing)
    if let Ok(info) = read_daemon_info() {
        // Verify the process is actually running (not just a stale PID file)
        #[cfg(unix)]
        {
            use std::process::Command;
            let is_running = Command::new("kill")
                .args(["-0", &info.pid.to_string()])
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);

            if !is_running {
                tracing::warn!(
                    pid = info.pid,
                    "Stale daemon.json found (process not running), removing"
                );
                remove_daemon_info();
            } else {
                eprintln!(
                    "Daemon already running (PID {}) at {}",
                    info.pid, info.listen_addr
                );
                eprintln!("Run `pi-daemon stop` first.");
                std::process::exit(1);
            }
        }

        #[cfg(not(unix))]
        {
            eprintln!(
                "Daemon already running (PID {}) at {}",
                info.pid, info.listen_addr
            );
            eprintln!(
                "Run `pi-daemon stop` first, or delete ~/.pi-daemon/daemon.json if the process is dead."
            );
            std::process::exit(1);
        }
    }

    // Handle backgrounding differently - use process spawning instead of forking
    if !foreground {
        println!("pi-daemon starting in background mode...");
        println!("  Address:  http://{}", config.listen_addr);
        println!("  Webchat:  http://{}", config.listen_addr);
        println!();
        println!("Use `pi-daemon status` to check status");
        println!("Use `pi-daemon chat` for terminal chat");
        println!("Use `pi-daemon stop` to stop the daemon");
        println!();
        
        // For background mode, spawn a new process and exit this one
        return spawn_daemon_process(&config.listen_addr);
    }

    // Initialize tracing AFTER daemonizing 
    if foreground {
        // In foreground mode, log to terminal with full verbosity
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "pi_daemon=info,tower_http=info".into()),
            )
            .init();
    } else {
        // In daemon mode, use minimal console logging since stdout/stderr are detached
        // This will primarily log to the daemon log file via our custom logging
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "pi_daemon=error,tower_http=error".into()), // Only errors in daemon mode
            )
            .with_ansi(false) // No ANSI colors in daemon mode
            .init();
    }

    // Create kernel
    let kernel = Arc::new(pi_daemon_kernel::PiDaemonKernel::new());
    kernel.init().await;

    // Write daemon info (with correct PID after daemonizing)
    let daemon_info = DaemonInfo {
        pid: std::process::id(),
        listen_addr: config.listen_addr.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    write_daemon_info(&daemon_info)?;

    // Log daemon startup to file if in daemon mode
    if !foreground {
        let log_msg = format!(
            "pi-daemon v{} started in background mode (PID {}) listening on {}",
            env!("CARGO_PKG_VERSION"),
            daemon_info.pid,
            daemon_info.listen_addr
        );
        let _ = daemon::write_daemon_log(&log_msg);
    }

    // Verify GitHub auth if configured
    if !config.github.personal_access_token.is_empty() {
        match pi_daemon_kernel::github::verify_github_auth(&config.github).await {
            Ok(user) => tracing::info!(user = %user.login, "GitHub authenticated"),
            Err(e) => tracing::warn!("GitHub auth failed: {e} (continuing without GitHub)"),
        }
    }

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        listen = %config.listen_addr,
        foreground = foreground,
        "pi-daemon starting"
    );

    // Handle Ctrl+C (mainly for foreground mode, daemon mode won't receive this)
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Received Ctrl+C, shutting down...");
        let _ = daemon::write_daemon_log("pi-daemon stopped via Ctrl+C");
        remove_daemon_info();
        std::process::exit(0);
    });

    // Run the API server (blocks until shutdown)
    pi_daemon_api::server::run_daemon(kernel, config).await?;

    // Cleanup
    let _ = daemon::write_daemon_log("pi-daemon stopped normally");
    remove_daemon_info();
    Ok(())
}

/// Spawn the daemon process in the background and exit the parent
fn spawn_daemon_process(listen_addr: &str) -> anyhow::Result<()> {
    use std::process::{Command, Stdio};
    
    // Get the current executable path
    let exe_path = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Failed to get current executable path: {}", e))?;
    
    // Spawn a new process with --foreground flag but detached from terminal
    let mut cmd = Command::new(exe_path);
    cmd.args(["start", "--foreground", "--listen", listen_addr])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    
    // On Unix, use process groups to detach from terminal
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                // Create new session to detach from terminal
                libc::setsid();
                Ok(())
            });
        }
    }
    
    let child = cmd.spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn daemon process: {}", e))?;
    
    // Don't wait for the child - let it run independently
    std::mem::forget(child);
    
    // Give the daemon a moment to start before exiting
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    Ok(())
}

async fn cmd_stop() -> anyhow::Result<()> {
    let info = read_daemon_info().map_err(|_| anyhow::anyhow!("Daemon is not running"))?;

    println!("Stopping daemon (PID {})...", info.pid);

    // Try graceful shutdown via API first
    let client = reqwest::Client::new();
    let shutdown_result = client
        .post(format!("http://{}/api/shutdown", info.listen_addr))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match shutdown_result {
        Ok(_) => {
            // Wait a moment for graceful shutdown
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(_) => {
            // API didn't respond, try SIGTERM on Unix
            #[cfg(unix)]
            {
                use std::process::Command;
                let _ = Command::new("kill")
                    .args(["-TERM", &info.pid.to_string()])
                    .output();
            }
        }
    }

    // Remove daemon info
    remove_daemon_info();
    println!("Daemon stopped.");
    Ok(())
}

async fn cmd_status() -> anyhow::Result<()> {
    let info = match read_daemon_info() {
        Ok(info) => info,
        Err(_) => {
            println!("pi-daemon is not running.");
            println!("Start with: pi-daemon start");
            return Ok(());
        }
    };

    println!("pi-daemon v{}", info.version);
    println!("  PID:      {}", info.pid);
    println!("  Address:  http://{}", info.listen_addr);
    println!("  Started:  {}", info.started_at);

    // Fetch live status from API
    let client = reqwest::Client::new();
    match client
        .get(format!("http://{}/api/status", info.listen_addr))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let status: serde_json::Value = resp.json().await?;
            let uptime = status["uptime_secs"].as_u64().unwrap_or(0);
            let agent_count = status["agent_count"].as_u64().unwrap_or(0);

            println!("  Uptime:   {}", format_duration(uptime));
            println!("  Agents:   {agent_count}");

            // List agents if any
            if agent_count > 0 {
                if let Ok(agents_resp) = client
                    .get(format!("http://{}/api/agents", info.listen_addr))
                    .send()
                    .await
                {
                    if let Ok(agents) = agents_resp.json::<Vec<serde_json::Value>>().await {
                        println!("\n  Active agents:");
                        for agent in &agents {
                            let name = agent["name"].as_str().unwrap_or("?");
                            let kind = agent["kind"].as_str().unwrap_or("?");
                            let status = agent["status"].as_str().unwrap_or("?");
                            let model = agent["model"].as_str().unwrap_or("");

                            if model.is_empty() {
                                println!("    - {name} ({kind}) [{status}]");
                            } else {
                                println!("    - {name} ({kind}) [{status}] — {model}");
                            }
                        }
                    }
                }
            }

            println!("\n  Webchat:  http://{}", info.listen_addr);
            println!("  API:      http://{}/api", info.listen_addr);
        }
        _ => {
            println!("  Status:   ❌ Not responding");
            println!();
            println!(
                "Daemon appears to be running (PID {}) but is not responding.",
                info.pid
            );
            println!("It may have crashed. Run `pi-daemon stop` to clean up.");
        }
    }

    Ok(())
}

async fn cmd_chat(agent: Option<String>, _model: Option<String>) -> anyhow::Result<()> {
    let info = read_daemon_info()
        .map_err(|_| anyhow::anyhow!("Daemon is not running. Start it with `pi-daemon start`"))?;

    // Use provided agent or default to "webchat"
    let agent_id = agent.unwrap_or_else(|| "webchat".to_string());
    let ws_url = format!("ws://{}/ws/{agent_id}", info.listen_addr);

    println!("🤖 pi-daemon chat (agent: {agent_id})");
    println!("Connected to: {}", info.listen_addr);
    println!("Type a message and press Enter. Type /quit to exit.\n");

    // Connect WebSocket
    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to WebSocket: {e}"))?;

    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (mut writer, mut reader) = ws_stream.split();

    // Spawn task to handle WebSocket messages
    let print_handle = tokio::spawn(async move {
        while let Some(msg_result) = reader.next().await {
            match msg_result {
                Ok(Message::Text(text)) => {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                        match parsed["type"].as_str() {
                            Some("typing") => {
                                let state = parsed["state"].as_str().unwrap_or("");
                                match state {
                                    "start" => {
                                        print!("\n🤔 ");
                                        flush_stdout();
                                    }
                                    "tool" => {
                                        let tool = parsed["tool_name"].as_str().unwrap_or("tool");
                                        print!("\n🛠️  Running {tool}...");
                                        flush_stdout();
                                    }
                                    "stop" => {
                                        // Just continue, response will come
                                    }
                                    _ => {}
                                }
                            }
                            Some("text_delta") => {
                                if let Some(content) = parsed["content"].as_str() {
                                    print!("{content}");
                                    flush_stdout();
                                }
                            }
                            Some("response") => {
                                // Final response - add newline
                                println!();
                                if let Some(input_tokens) = parsed["input_tokens"].as_u64() {
                                    if let Some(output_tokens) = parsed["output_tokens"].as_u64() {
                                        println!("📊 {input_tokens} in, {output_tokens} out");
                                    }
                                }
                            }
                            Some("error") => {
                                if let Some(content) = parsed["content"].as_str() {
                                    eprintln!("\n❌ Error: {content}");
                                }
                            }
                            Some("pong") => {
                                // Ignore keepalive responses
                            }
                            _ => {
                                // Unknown message type, ignore
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    println!("\n🔌 Connection closed by server");
                    break;
                }
                Err(e) => {
                    eprintln!("\n❌ WebSocket error: {e}");
                    break;
                }
                _ => {
                    // Ignore other message types (binary, ping, pong)
                }
            }
        }
    });

    // Read from stdin and send messages
    let stdin = tokio::io::stdin();
    let mut stdin_reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        print!("> ");
        flush_stdout();

        line.clear();
        match stdin_reader.read_line(&mut line).await {
            Ok(0) => {
                // EOF (Ctrl+D)
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "/quit" || trimmed == "/exit" {
                    break;
                }
                if trimmed == "/help" {
                    print_chat_help();
                    continue;
                }
                if trimmed.starts_with('/') {
                    println!("Unknown command: {trimmed}");
                    println!("Type /help for available commands");
                    continue;
                }

                let msg = serde_json::json!({
                    "type": "message",
                    "content": trimmed
                });

                if let Err(e) = writer.send(Message::Text(msg.to_string())).await {
                    eprintln!("❌ Failed to send message: {e}");
                    break;
                }
            }
            Err(e) => {
                eprintln!("❌ Error reading input: {e}");
                break;
            }
        }
    }

    // Close WebSocket gracefully
    let _ = writer.send(Message::Close(None)).await;

    // Wait for print task to finish
    print_handle.abort();

    println!("👋 Chat ended");
    Ok(())
}

async fn cmd_config() -> anyhow::Result<()> {
    let config = load_config()?;

    // Function to redact sensitive values
    let redact = |s: &str| -> String {
        if s.is_empty() {
            "(not set)".to_string()
        } else if s.len() <= 8 {
            "****".to_string()
        } else {
            format!("{}...{}", &s[..4], &s[s.len() - 4..])
        }
    };

    println!("pi-daemon configuration");
    println!("  Config file: {}", config_path().display());
    println!();
    println!("  listen_addr:     {}", config.listen_addr);
    println!("  api_key:         {}", redact(&config.api_key));
    println!("  default_model:   {}", config.default_model);
    println!("  data_dir:        {}", config.data_dir.display());
    println!();
    println!("  [providers]");
    println!(
        "  anthropic:       {}",
        redact(&config.providers.anthropic_api_key)
    );
    println!(
        "  openai:          {}",
        redact(&config.providers.openai_api_key)
    );
    println!(
        "  openrouter:      {}",
        redact(&config.providers.openrouter_api_key)
    );
    println!("  ollama:          {}", config.providers.ollama_base_url);
    println!();
    println!("  [github]");
    println!(
        "  pat:             {}",
        redact(&config.github.personal_access_token)
    );
    println!(
        "  default_owner:   {}",
        if config.github.default_owner.is_empty() {
            "(not set)"
        } else {
            &config.github.default_owner
        }
    );

    Ok(())
}

fn print_chat_help() {
    println!("Chat Commands:");
    println!("  /help    - Show this help");
    println!("  /quit    - Exit chat");
    println!("  /exit    - Exit chat");
    println!("  Ctrl+D   - Exit chat");
    println!("  Ctrl+C   - Exit chat");
    println!();
    println!("Just type a message and press Enter to chat!");
}

fn flush_stdout() {
    use std::io::Write;
    std::io::stdout().flush().ok();
}

fn format_duration(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_valid_semver() {
        let version = env!("CARGO_PKG_VERSION");

        // Basic semver format check (x.y.z)
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(parts.len(), 3, "Version should have format x.y.z");

        // Each part should be a number
        for part in parts {
            part.parse::<u32>()
                .expect("Version parts should be numbers");
        }
    }

    #[test]
    fn test_version_output() {
        // Simple test to verify basic functionality
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3661), "1h 1m");
        assert_eq!(format_duration(90061), "1d 1h 1m");
    }
}
