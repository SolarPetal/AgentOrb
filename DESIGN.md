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

### Choice 5: One shared loopback host policy with bounded HTTP calls

Core normalizes `localhost` to `127.0.0.1` and accepts only loopback IP literals. The CLI, daemon, and Tauri bridge use that same policy, and CLI/UI HTTP operations have separate connect and I/O timeouts.

Alternative: let every component resolve arbitrary hostnames. That creates bind/connect inconsistencies and risks sending the local bearer token to a non-loopback host after config tampering.

### Choice 6: Prebuilt matrix before source fallback

Official release assets currently cover Linux x64 and Windows x64. Custom release locations may add other targets; otherwise a missing asset returns control to setup so a source checkout can build locally. Published npm installs without a matching asset now fail with an explicit source-checkout/custom-bundle instruction instead of an HTTP 404.

Alternative: advertise every platform recognized by Node as prebuilt. That hid the actual release matrix and made fallback unreachable when GitHub returned a missing-asset response.

## Key decisions

- Daemon refuses non-loopback binding for MVP safety.
- `localhost` is normalized consistently, and daemon HTTP clients fail closed on connection or I/O timeout.
- Full stdout/stderr is not recorded by default; output samples are bounded by `privacy.max_sample_chars` and disabled unless explicitly configured, except short prompt samples needed for prompt detection.
- Completed status expires after `behavior.completed_hold_seconds`; failed status requires explicit clear.
- Adapter shims (`codex-orb`, `claude-orb`) call `agent_orb run -- <adapter>` and never replace original CLI binaries.
- Adapter state is driven by installed CLI hooks rather than terminal scraping. Claude Code hooks report the full six-state set; Codex hooks report executing (`PreToolUse`) and completed (`Stop`), which is enough for a coarse running/done signal. Codex's hooks engine is enabled via `codex features enable hooks`; output scraping (`AGENT_ORB_OBSERVE_PTY=1`) remains an opt-in fallback.

## Known limitations

- Real Codex CLI / Claude Code CLI behavior still depends on their installed versions and terminal expectations.
- Codex hooks are experimental, not available on Windows, and must be trusted once via `/hooks` inside Codex before they fire. `PreToolUse` currently intercepts the Bash tool reliably, so text-only turns may skip the executing state and go straight to completed.
- Prompt detection is heuristic and may produce false positives or miss custom prompts.
- UI dynamic positioning is applied inside the orb window; native window geometry is still mostly static in MVP.
- Official CI currently publishes only Linux x64 and Windows x64; macOS and arm64 require source builds or externally supplied bundles.
- Repository smoke tests isolate `CLAUDE_CONFIG_DIR` and `CODEX_HOME`; they must never modify a developer's real adapter hooks or feature configuration.

## Change history

- 2026-06-30: MVP first-version implementation completed for core, daemon, wrapper, Tauri UI, bootstrapper, config, and local smoke.
- 2026-07-09: Added Codex CLI hook integration (executing/completed) so Codex orb state no longer depends on the opt-in PTY output scraper.
- 2026-07-10: Isolated smoke adapter configs, aligned prebuilt bundle fallback with the release matrix, normalized loopback hosts, and bounded CLI/UI daemon HTTP calls.
