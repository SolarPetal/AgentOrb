# agent_orb npx bootstrapper

Local development bootstrapper for Agent Orb. It is intended to become:

```bash
npx @solar_orb/agent_orb
```

after the package is published to npm.

## Local development

From the repo root:

```bash
./scripts/release/smoke-npx-local.sh
```

For manual testing:

```bash
npm --prefix packages/agent_orb run package-runtime
npx --yes ./packages/agent_orb setup --yes
```

If `packages/agent_orb/releases` contains a matching native bundle, setup installs that bundle directly with SHA256 verification. Otherwise it falls back to source build. On Windows, setup also adds the runtime bin directory to the user PATH so a new terminal can run `agent_orb-codex`, `agent_orb-claude`, `agent_orb`, `codex-orb`, and `claude-orb` directly. The adapter launchers call `agent_orb launch`, which starts the orb UI and daemon only for that CLI session. When the adapter CLI exits, the session-local `agent_orbd` and `agent-orb-ui` processes are stopped so the desktop orb does not linger.

Upgrade or repair an existing install:

```bash
npx @solar_orb/agent_orb upgrade --yes
```

The upgrade flow verifies the new bundle before stopping the old daemon/orb UI and removing old runtime files.

If Windows cannot auto-detect Codex CLI, set an explicit path before setup:

```powershell
$env:AGENT_ORB_CODEX_PATH = "C:\nvm4w\nodejs\codex.cmd"
npx --yes @solar_orb/agent_orb@0.1.13 upgrade --yes
```

## Windows local path

```powershell
cd C:\path\to\AgentOrb
npx --yes .\packages\agent_orb
```

If no Windows bundle is present and Rust is not installed on Windows yet, setup will print:

```powershell
winget install --id Rustlang.Rustup -e
```

For the intended no-Rust Windows path, publish or place `agent-orb-windows-x64.zip` plus `checksums.txt` in the release endpoint/package releases directory.

## Windows + WSL repo caveat

Windows npm may fail with `ERR_INVALID_URL` when installing a local package directly from a UNC path like:

```powershell
\\wsl.localhost\Ubuntu\home\...\AgentOrb
```

For Windows-host testing, prefer either:

1. run from a Windows-local clone, or
2. use a packed tarball from a Windows-local directory:

```powershell
cd $env:TEMP\agent-orb-npx
npx --yes .\solar_orb-agent_orb-0.1.13.tgz --help
```
