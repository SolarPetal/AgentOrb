# agent-orb-core Design

## Design goal

Keep the runtime protocol independent from transport, UI, and process spawning details.

## Non-goals

- No HTTP server or client.
- No desktop UI behavior.
- No CLI process management.

## Choices and trade-offs

### Enum-based event/status protocol

Chosen for compile-time exhaustiveness and predictable serde names.

Alternative: stringly typed maps. More flexible but would push errors to runtime and make state-machine tests weaker.

### Deterministic state-machine function

`transition(current, event)` is pure and easy to test.

Alternative: store state and timers inside core. That would couple core to async runtime concerns. MVP keeps timers in wrapper/daemon layers.

### Config defaults in core

All components can share the same MVP defaults.

Alternative: duplicate defaults per crate. Faster initially but creates documentation drift.

## Known limitations

- Timestamp fields are strings at the protocol layer; validation happens where needed.
- Unknown event types currently fail deserialization instead of becoming an extension variant.

## Change history

- 2026-06-30: Added shared config loader and MVP event/status/state models.
