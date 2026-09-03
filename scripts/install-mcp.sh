#!/usr/bin/env bash
# Register the passvalet MCP server with Cursor / Claude Code / Codex.
# Usage: scripts/install-mcp.sh [/path/to/passvalet]   (defaults to the bundled or PATH binary)
set -euo pipefail

CLI="${1:-}"
if [[ -z "$CLI" ]]; then
  if [[ -x "/Applications/PassValet.app/Contents/MacOS/passvalet" ]]; then
    CLI="/Applications/PassValet.app/Contents/MacOS/passvalet"
  elif command -v passvalet >/dev/null 2>&1; then
    CLI="$(command -v passvalet)"
  else
    CLI="$(cd "$(dirname "$0")/.." && pwd)/target/release/passvalet"
  fi
fi
[[ -x "$CLI" ]] || { echo "passvalet binary not found at $CLI"; exit 1; }
echo "using $CLI"

# Cursor
mkdir -p ~/.cursor
python3 - "$CLI" <<'EOF'
import json, os, sys
p = os.path.expanduser("~/.cursor/mcp.json")
cfg = {}
if os.path.exists(p):
    try: cfg = json.load(open(p))
    except Exception: cfg = {}
cfg.setdefault("mcpServers", {})["passvalet"] = {"command": sys.argv[1], "args": ["mcp"]}
json.dump(cfg, open(p, "w"), indent=2)
print("cursor: wrote", p)
EOF

# Claude Code
if command -v claude >/dev/null 2>&1; then
  claude mcp remove passvalet >/dev/null 2>&1 || true
  claude mcp add passvalet -- "$CLI" mcp && echo "claude code: registered"
fi

# Codex
if [[ -f ~/.codex/config.toml ]] && ! grep -q "mcp_servers.passvalet" ~/.codex/config.toml; then
  printf '\n[mcp_servers.passvalet]\ncommand = "%s"\nargs = ["mcp"]\n' "$CLI" >> ~/.codex/config.toml
  echo "codex: appended to ~/.codex/config.toml"
fi
