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

## Development quick start

```bash
cargo check --workspace
cargo run -p agent-orb-cli -- run -- echo hello
cargo run -p agent-orb-daemon
```

## Planned user setup

```bash
npx agent_orb
```

The bootstrapper will let users choose Codex CLI, Claude Code CLI, or both.
