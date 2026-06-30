# agent-orb-daemon Design

## Design goal

Provide a small, local, authenticated state API that decouples wrappers from UI renderers.

## Non-goals

- No remote network binding in MVP.
- No database persistence.
- No multi-user access control.

## Choices and trade-offs

### In-memory session store

Chosen because MVP only needs current runtime status.

Alternative: persistent database. Useful for history, but unnecessary for ambient status and increases privacy exposure.

### Token-protected localhost API

Chosen to reduce local abuse risk while keeping setup simple.

Alternative: unauthenticated localhost API. Simpler but unsafe for write endpoints.

### Priority-based global status

MVP shows one orb. The daemon selects the most important current status: failed and waiting-input outrank active, active outranks silent, etc.

Alternative: per-session UI. More accurate for parallel work, but out of MVP scope.

## Known limitations

- No WebSocket stream yet; UI polls `/v1/status`.
- Completed status expiry is calculated on status read, not via a background sweep.

## Change history

- 2026-06-30: Added config-backed hold time, loopback guard, token-protected API, and status priority store.
