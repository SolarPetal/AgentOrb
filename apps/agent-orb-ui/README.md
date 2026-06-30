# agent-orb-ui

Tauri desktop floating orb for Agent Orb runtime status.

## Why it exists

Users need a low-distraction desktop signal that shows whether an AI CLI is active, silent, waiting, complete, failed, or stuck.

## Responsibilities

- Start a transparent always-on-top orb window.
- Poll daemon status through Tauri commands.
- Render colors and animations from visual status.
- Read user config for colors, size, position, opacity, and click-through.
- Clear completed/failed status on click.

## Dependencies

- Upstream: `agent-orb-core`, Tauri v2, Vite, TypeScript.
- Downstream: runtime bundles and npx setup.

## Usage example

```bash
cd apps/agent-orb-ui
npm run build
npm run tauri dev
```

## Verify

```bash
npm run build
cd src-tauri && cargo test
```
