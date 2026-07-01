# agent-orb-cli

Rust command-line wrapper for observing AI CLI runtime activity.

## Why it exists

Users should run their normal CLI command while Agent Orb preserves interactive terminal behavior and emits lifecycle events to the orb.

## Responsibilities

- Run `agent_orb run -- <command>`.
- Spawn target processes without breaking interactive terminal behavior.
- Emit normalized events to the local daemon.
- Auto-start `agent_orbd` when possible.
- Detect Codex / Claude / generic sources from command names.
- Preserve a real terminal for Codex and Claude launchers; monitor stdout/stderr for generic commands.

## Internal layout

```text
src/main.rs    # clap entrypoint only
src/runner.rs  # process orchestration and stream forwarding
src/daemon.rs  # daemon lifecycle and event client
src/http.rs    # minimal local HTTP client
src/config.rs  # config dir, token loading, loopback guard
src/prompt.rs  # prompt heuristics and output sample truncation
src/event.rs   # EventEnvelope helpers
src/source.rs  # Codex / Claude / generic source detection
src/shell.rs   # command payload formatting
src/error.rs   # shared CLI error type
```

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
