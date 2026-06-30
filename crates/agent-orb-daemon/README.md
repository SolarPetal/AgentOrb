# agent-orb-daemon

Local HTTP daemon that stores Agent Orb session state and exposes status to the UI.

## Why it exists

The wrapper and UI run as separate processes. The daemon is the local coordination point between process events and visual status.

## Responsibilities

- Bind a loopback HTTP API.
- Receive authenticated events at `/v1/events`.
- Serve authenticated status at `/v1/status`.
- Clear completed/failed statuses at `/v1/status/clear`.
- Persist and validate a local bearer token.

## Dependencies

- Upstream: `agent-orb-core`, `axum`, `tokio`.
- Downstream: CLI wrapper, Tauri UI, npm bootstrapper.

## Usage example

```bash
agent_orbd
curl http://127.0.0.1:17321/health
```

Authenticated status requires the token file in the platform config directory.

## Verify

```bash
cargo test -p agent-orb-daemon
```
