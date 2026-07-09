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
- Claude Code reports precise state (idle, thinking, executing, waiting, completed, compacting) through installed hooks.
- Codex CLI reports executing and completed state through its own installed hooks; other states fall back to the wrapper's start/exit/timeout signals.
- Adapters run connected to the real terminal by default so their full-screen TUIs render cleanly. Set `AGENT_ORB_OBSERVE_PTY=1` to re-enable the legacy observed-terminal path that scrapes output for state.
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

The bootstrapper detects Codex CLI and Claude Code CLI if installed, writes `config.toml`, installs native runtime binaries, adds the runtime bin directory to the user PATH on Windows, and creates adapter launchers without replacing the original CLIs. The launchers start a session-local daemon/orb UI on demand.

If Windows setup cannot detect an already installed Codex CLI, pass the absolute shim path explicitly:

```powershell
$env:AGENT_ORB_CODEX_PATH = "C:\nvm4w\nodejs\codex.cmd"
npx --yes @solar_orb/agent_orb@0.1.18 upgrade --yes
```

After setup, open a new terminal and run one command:

```bash
agent_orb-codex   # starts the orb UI + daemon, then runs Codex through Agent Orb
agent_orb-claude  # starts the orb UI + daemon, then runs Claude Code through Agent Orb
```

Lower-level commands remain available:

```bash
agent_orb run -- echo hello
agent_orb run -- codex
agent_orb run -- claude
agent_orb launch --adapter claude --
codex-orb   # compatibility alias
claude-orb  # compatibility alias
```

Upgrade or repair an existing installation:

```bash
npx @solar_orb/agent_orb upgrade --yes
```

`upgrade` verifies the new runtime bundle first, then stops the old local daemon/orb UI, removes old Agent Orb runtime files and shims, and recreates launchers. The next adapter launcher starts a fresh daemon/orb session and cleans it up on exit.

## Configuration

Agent Orb reads `config.toml` from the platform config directory:

- Linux: `$XDG_CONFIG_HOME/agent-orb` or `~/.config/agent-orb`
- macOS: `~/Library/Application Support/agent-orb`
- Windows: `%APPDATA%\\agent-orb`

Useful MVP settings:

```toml
[colors]
idle = "#9CA3AF"          # 待命 / gray
thinking_like = "#FACC15" # 思考 / yellow
active = "#3B82F6"        # 执行 / blue
waiting_input = "#EF4444" # 等待 / red
completed = "#22C55E"     # 完成 / green
compacting = "#A855F7"    # 压缩 / purple

[orb]
position = "top-right"
size = 48
opacity = 0.92
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
