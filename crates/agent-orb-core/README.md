# agent-orb-core

Core protocol, state, visual mapping, and configuration model for Agent Orb.

## Why it exists

The CLI wrapper, daemon, and UI need one shared vocabulary for runtime events and status. This crate keeps that vocabulary stable and testable.

## Responsibilities

- Define `Source`, `EventType`, and `EventEnvelope`.
- Define `InternalStatus` and `VisualStatus`.
- Convert events through the state machine.
- Load MVP TOML config with defaults.

## Dependencies

- Upstream: `serde`, `serde_json`, `toml`, `uuid`, `time`.
- Downstream: `agent-orb-cli`, `agent-orb-daemon`, and the Tauri UI.

## Usage example

```rust
use agent_orb_core::{event::EventEnvelope, state_machine::StateMachine};

let event = EventEnvelope::from_json_str(include_str!("../../../examples/events/session-started.json"))?;
let mut machine = StateMachine::new();
machine.apply(&event);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Verify

```bash
cargo test -p agent-orb-core
```
