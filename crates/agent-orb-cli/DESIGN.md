# agent-orb-cli Design

## Design goal

Observe a target CLI with minimal disruption and send daemon events that drive the orb state.

## Non-goals

- No shell alias replacement by default.
- No full terminal emulator or PTY in MVP.
- No complete stdout/stderr logging by default.

## Choices and trade-offs

### Small modules over one large CLI file

The CLI crate is split by responsibility: entrypoint, runner, daemon client, HTTP, config/token, prompt detection, source detection, event helpers, shell formatting, and shared errors.

Alternative: keep all code in `main.rs`. That was fast for MVP delivery, but it made quality checks noisy and raised the cost of changing prompt, timeout, or daemon behavior independently.

### TTY passthrough for interactive AI CLIs

Codex and Claude are spawned with inherited stdin/stdout/stderr so they still see a real terminal. Some interactive CLIs change behavior when stdout/stderr are pipes; Claude Code, for example, can enter print-style non-interactive mode if the wrapper captures its streams.

Trade-off: Agent Orb receives lifecycle events for these interactive adapters, but not live output/prompt samples until a cross-platform PTY/tee layer is added.

### Piped stdout/stderr for generic commands

Generic commands still use piped stdout/stderr with inherited stdin. This is portable and keeps MVP prompt/idle detection useful for non-interactive smoke and diagnostics.

Alternative: PTY. Better for rich interactive CLIs, but cross-platform PTY behavior is more complex.

### Timer task for idle/stuck detection

The wrapper sends `idle.timeout` and `stuck.timeout` based on time since last output.

Alternative: daemon-side timer. This would centralize timing but requires background session sweeps. Wrapper already knows process lifetime, so MVP keeps timeout emission there.

### Bounded prompt sample

The wrapper inspects bounded output samples to detect prompts. Privacy config controls whether normal samples are sent to daemon.

Alternative: send all output and detect in daemon. Easier to debug, but violates MVP privacy defaults.

## Known limitations

- Prompt detection is heuristic.
- Codex and Claude passthrough preserves interactivity, but live output/prompt detection for those adapters waits for a future PTY/tee implementation.
- Auto-start assumes `agent_orbd` is either next to `agent_orb`, on PATH, or pointed to by `AGENT_ORB_DAEMON`.

## Change history

- 2026-06-30: Added config-backed daemon connection, idle/stuck timeout events, prompt detection, and loopback host guard.
- 2026-06-30: Split the CLI implementation into focused modules without changing runtime behavior.
- 2026-07-01: Added TTY passthrough for Codex and Claude so adapter launchers preserve interactive CLI behavior.
