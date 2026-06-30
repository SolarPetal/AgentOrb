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

### Piped stdout/stderr with inherited stdin

Chosen because it is portable and keeps MVP simple while preserving visible output.

Alternative: PTY. Better for rich interactive CLIs, but cross-platform PTY behavior is more complex.

### Timer task for idle/stuck detection

The wrapper sends `idle.timeout` and `stuck.timeout` based on time since last output.

Alternative: daemon-side timer. This would centralize timing but requires background session sweeps. Wrapper already knows process lifetime, so MVP keeps timeout emission there.

### Bounded prompt sample

The wrapper inspects bounded output samples to detect prompts. Privacy config controls whether normal samples are sent to daemon.

Alternative: send all output and detect in daemon. Easier to debug, but violates MVP privacy defaults.

## Known limitations

- Prompt detection is heuristic.
- Some interactive CLIs may require a PTY for perfect behavior.
- Auto-start assumes `agent_orbd` is either next to `agent_orb`, on PATH, or pointed to by `AGENT_ORB_DAEMON`.

## Change history

- 2026-06-30: Added config-backed daemon connection, idle/stuck timeout events, prompt detection, and loopback host guard.
- 2026-06-30: Split the CLI implementation into focused modules without changing runtime behavior.
