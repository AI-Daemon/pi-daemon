# Configuration

## Config File

pi-daemon reads configuration from `~/.pi-daemon/config.toml`. A default file is automatically created on first run with commented defaults.

```
~/.pi-daemon/
├── config.toml        # Main configuration
├── daemon.json        # Runtime info (PID, address) — auto-managed
└── data/              # Sessions, memory, cron jobs, etc.
```

## Full Config Reference

The current implementation supports these configuration sections:

```toml
# pi-daemon configuration

# HTTP server listen address
listen_addr = "127.0.0.1:4200"

# API key for authenticating requests (empty = no auth)
api_key = ""

# Default LLM model
default_model = "claude-sonnet-4-20250514"

# Data directory path (will be created if it doesn't exist)
data_dir = "/home/user/.pi-daemon/data"

[providers]
# Anthropic
anthropic_api_key = ""
anthropic_base_url = "https://api.anthropic.com"
# OpenAI
openai_api_key = ""
openai_base_url = "https://api.openai.com"
# OpenRouter
openrouter_api_key = ""
# Ollama (local)
ollama_base_url = "http://localhost:11434"

[github]
# Personal Access Token — needed for private repo access
# Scopes: repo, read:org
# Set via config or GITHUB_TOKEN / GH_TOKEN env var
personal_access_token = ""
api_base_url = "https://api.github.com"
default_owner = ""
```

> **Note:** Future phases will add `[wire]`, `[channels]`, and other sections.

## Environment Variable Overrides

Environment variables take precedence over the config file:

| Variable | Config field |
|----------|-------------|
| `PI_DAEMON_LISTEN_ADDR` | `listen_addr` |
| `PI_DAEMON_API_KEY` | `api_key` |
| `PI_DAEMON_DEFAULT_MODEL` | `default_model` |
| `ANTHROPIC_API_KEY` | `providers.anthropic_api_key` |
| `OPENAI_API_KEY` | `providers.openai_api_key` |
| `OPENROUTER_API_KEY` | `providers.openrouter_api_key` |
| `GITHUB_TOKEN` or `GH_TOKEN` | `github.personal_access_token` |

## GitHub Authentication

To access private repositories, set a GitHub Personal Access Token:

1. Go to **https://github.com/settings/tokens**
2. Create a token with scopes: `repo`, `read:org`
3. Set it via config or environment:
   ```bash
   export GITHUB_TOKEN="ghp_your_token_here"
   ```

Verify it works:
```bash
pi-daemon start --foreground
# Look for: "GitHub authenticated as your-username"
```

The kernel provides these GitHub APIs:
- `github::verify_github_auth()` — validate PAT and get user info
- `github::list_repos()` — list accessible private repositories

## Security Notes

- API keys in `config.toml` are stored in plain text — protect the file with appropriate permissions (`chmod 600`)
- When `api_key` is set, all API endpoints require `Authorization: Bearer <key>` or `X-API-Key: <key>`
- The webchat page (`/`) and health check (`/api/health`) are accessible without auth even when `api_key` is set
- Sensitive values are never logged — tracing output redacts API keys
