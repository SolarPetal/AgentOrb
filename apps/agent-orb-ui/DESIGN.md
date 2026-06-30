# agent-orb-ui Design

## Design goal

Render daemon status as a small ambient orb with minimal interaction and no dashboard complexity.

## Non-goals

- No multi-session dashboard.
- No cloud sync or account UI.
- No WebSocket dependency in MVP.

## Choices and trade-offs

### Tauri commands instead of browser fetch

The UI frontend calls Rust commands, and the Rust side reads the token file and talks to the daemon.

Alternative: direct browser fetch. That would expose token handling to frontend code and is less aligned with desktop-local security boundaries.

### CSS variables from config

The frontend maps `config.toml` colors and dimensions into CSS variables.

Alternative: generate CSS files. Less dynamic and harder to refresh.

### Polling every second

Simple and reliable for ambient status.

Alternative: WebSocket stream. Lower latency but more lifecycle complexity.

## Known limitations

- Native window size/position is mostly static; CSS applies visual size and inner positioning for MVP.
- Click-through can make the orb non-interactive, so users should keep it disabled if they need click-to-clear.

## Change history

- 2026-06-30: Added config-driven colors/size/opacity, daemon status polling, and click-to-clear.
