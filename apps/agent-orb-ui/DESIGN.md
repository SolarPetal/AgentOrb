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

The Rust bridge validates the daemon host against the shared loopback policy before reading/sending the bearer token, and applies bounded connect and I/O timeouts. This keeps a tampered `config.toml` from redirecting the token to a remote host or hanging the orb indefinitely.

### CSS variables from config

The frontend maps `config.toml` colors and dimensions into CSS variables.

Alternative: generate CSS files. Less dynamic and harder to refresh.

### Polling every second

Simple and reliable for ambient status.

Alternative: WebSocket stream. Lower latency but more lifecycle complexity.

### Click-to-expand panel

The orb starts as a compact always-on-top window and expands into a small details panel when clicked. The frontend asks the Rust command `set_panel_open` to resize and re-anchor the native Tauri window against the configured screen corner; doing this in Rust avoids production permission failures from frontend-only `setSize` / `setPosition` calls. The orb button is explicitly marked as `no-drag`; otherwise the frameless-window drag region can swallow click events on desktop webviews.

Alternative: separate dashboard window. More room, but too heavy for the MVP ambient-orb workflow.

## Known limitations

- Native window size/position is mostly static; CSS applies visual size and inner positioning for MVP.
- Click-through can make the orb non-interactive, so users should keep it disabled if they need click-to-clear.

## Change history

- 2026-06-30: Added config-driven colors/size/opacity, daemon status polling, and click-to-clear.
- 2026-07-01: Added click-to-expand details panel, moved native resize/positioning into a Rust command, and removed the drag-region click blocker from the orb button.
