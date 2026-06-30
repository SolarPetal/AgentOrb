# agent-orb-cli

Rust command-line wrapper for observing AI CLI runtime activity.

## Why it exists

Users should run their normal CLI command while Agent Orb observes stdout, stderr, exit status, prompt-like output, and idle time.

## Responsibilities

- Run `agent_orb run -- <command>`.
- Spawn and preserve target process stdin/stdout/stderr behavior.
- Emit normalized events to the local daemon.
- Auto-start `agent_orbd` when possible.
- Detect Codex / Claude / generic sources from command names.

## Dependencies

- Upstream: `agent-orb-core`, `tokio`, `clap`, `uuid`, `time`.
- Downstream: npm bootstrapper and adapter shims.

## Usage example

```bash
agent_orb run -- echo hello
agent_orb run -- codex
agent_orb run -- claude
```

## Verify

```bash
cargo test -p agent-orb-cli
cargo run -p agent-orb-cli -- run -- echo hello
```
