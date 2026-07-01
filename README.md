# Agent Orb

Agent Orb is a lightweight AI CLI runtime observer for Codex CLI and Claude Code CLI.

It wraps a target CLI process, sends normalized runtime events to a local daemon, and renders a low-distraction floating status orb on the desktop.

## MVP docs

- [MVP design](docs/AGENT_ORB_MVP.md)
- [MVP development plan](docs/AGENT_ORB_MVP_DEV_PLAN.md)

## Workspace layout

```text
agent-orb/
├── Cargo.toml
├── crates/
│   ├── agent-orb-core/
│   ├── agent-orb-cli/
│   └── agent-orb-daemon/
├── apps/
│   └── agent-orb-ui/
├── docs/
└── examples/
```

## MVP status

The first MVP version is implemented for local runtime observation:

- `agent_orb run -- <command>` wraps arbitrary CLIs.
- Codex / Claude adapter shims are supported through setup-generated `codex-orb` and `claude-orb` commands.
- The wrapper emits process, output, prompt, idle, stuck, and exit events.
- `agent_orbd` stores current local session state behind a bearer token.
- The Tauri orb polls daemon status and renders configured colors, size, opacity, and animation.
- The npm bootstrapper installs a portable runtime bundle or falls back to a local source build.

## Development quick start

```bash
cargo check --workspace
cargo run -p agent-orb-cli -- run -- echo hello
cargo run -p agent-orb-daemon
```

## User setup

```bash
npx @solar_orb/agent_orb
```

The bootstrapper detects Codex CLI and Claude Code CLI if installed, writes `config.toml`, installs native runtime binaries, starts the daemon, and creates optional adapter shims without replacing the original CLIs.

After setup:

```bash
agent_orb run -- echo hello
agent_orb run -- codex
agent_orb run -- claude
codex-orb   # if Codex CLI was detected during setup
claude-orb  # if Claude Code CLI was detected during setup
```

## Configuration

Agent Orb reads `config.toml` from the platform config directory:

- Linux: `$XDG_CONFIG_HOME/agent-orb` or `~/.config/agent-orb`
- macOS: `~/Library/Application Support/agent-orb`
- Windows: `%APPDATA%\\agent-orb`

Useful MVP settings:

```toml
[orb]
position = "top-right"
size = 36
opacity = 0.88
click_through = false

[behavior]
silent_threshold_seconds = 20
stuck_threshold_seconds = 180
completed_hold_seconds = 10

[privacy]
include_output_sample = false
max_sample_chars = 512
```

## Verification

```bash
cargo test --workspace
cd apps/agent-orb-ui && npm run build
cd apps/agent-orb-ui/src-tauri && cargo test
cd packages/agent_orb && npm run check && npm run build
AGENT_ORB_SMOKE_CONFIG_DIR="$(mktemp -d)" ./scripts/release/smoke-npx-local.sh
```

If real Codex CLI or Claude Code CLI binaries are installed, run the adapter smoke too:

```bash
AGENT_ORB_SKIP_UI_BUILD=1 ./scripts/smoke-real-adapters.sh
```

On a Windows host, run the matching PowerShell smoke after installing the Windows runtime:

```powershell
.\scripts\windows\install-agent-orb.ps1 -CreateAdapterShims
.\scripts\windows\smoke-real-adapters.ps1
```
