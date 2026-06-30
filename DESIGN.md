# Agent Orb Design

## Purpose

Agent Orb is a local-first runtime observer for AI CLIs. It wraps a target command, converts process activity into normalized events, stores the latest session state in a localhost daemon, and renders a low-distraction desktop orb.

## Goals

- Preserve the original CLI stdout/stderr experience.
- Show active, silent, waiting-input, completed, failed, stuck, idle, and disconnected states.
- Keep all runtime data local by default.
- Protect daemon write/status APIs with a local bearer token.
- Support lightweight setup through `npx agent_orb` and portable native bundles.

## Non-goals for MVP

- Cloud sync, accounts, remote dashboards, or mobile push.
- Exact model-internal thinking detection.
- Multiple simultaneous desktop orbs.
- Default replacement of `codex` or `claude` binaries.

## Architecture choices and trade-offs

### Choice 1: Local daemon + CLI wrapper + Tauri orb

Chosen because it keeps the observer independent from any single AI CLI and gives the UI a stable local API.

Alternative: direct terminal plugin integration. That would provide richer terminal semantics but would require shell/editor-specific integrations and would not cover arbitrary CLIs as cleanly.

### Choice 2: Polling status API before WebSocket

MVP uses `/v1/status` polling from the UI. This is simpler, reliable, and enough for ambient visual feedback.

Alternative: WebSocket streaming. It would reduce latency but increases lifecycle and reconnect complexity. It remains a later enhancement.

### Choice 3: Heuristic prompt detection

The wrapper scans bounded output samples for common prompt text and emits `prompt.detected`. This is intentionally weak evidence.

Alternative: PTY-level parsing. It would better preserve interactive semantics, but adds cross-platform complexity. MVP keeps inherited stdin and piped stdout/stderr.

### Choice 4: Local token file for daemon auth

The daemon writes a random token in the platform config directory and requires `Authorization: Bearer <token>` for event/status APIs.

Alternative: unauthenticated localhost API. Easier, but unsafe against local cross-process abuse.

## Key decisions

- Daemon refuses non-loopback binding for MVP safety.
- Full stdout/stderr is not recorded by default; output samples are bounded by `privacy.max_sample_chars` and disabled unless explicitly configured, except short prompt samples needed for prompt detection.
- Completed status expires after `behavior.completed_hold_seconds`; failed status requires explicit clear.
- Adapter shims (`codex-orb`, `claude-orb`) call `agent_orb run -- <adapter>` and never replace original CLI binaries.

## Known limitations

- Real Codex CLI / Claude Code CLI behavior still depends on their installed versions and terminal expectations.
- Prompt detection is heuristic and may produce false positives or miss custom prompts.
- UI dynamic positioning is applied inside the orb window; native window geometry is still mostly static in MVP.
- Windows/macOS release assets are produced by CI or platform-specific hosts; local Linux smoke only proves Linux runtime packaging.

## Change history

- 2026-06-30: MVP first-version implementation completed for core, daemon, wrapper, Tauri UI, bootstrapper, config, and local smoke.
