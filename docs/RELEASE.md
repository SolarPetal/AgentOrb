# Agent Orb release

## GitHub Releases runtime assets

Runtime bundles are published as GitHub Release assets:

- `agent-orb-linux-x64.tar.gz`
- `agent-orb-windows-x64.zip`
- `checksums.txt`

The npm package `agent_orb` stays lightweight and downloads the matching runtime bundle during setup.

## Create a release

1. Update versions if needed.
2. Push a tag:

```bash
VERSION=$(node -p "require('./packages/agent_orb/package.json').version")
git tag "v$VERSION"
git push origin "v$VERSION"
```

3. GitHub Actions workflow `.github/workflows/release.yml` builds and uploads release assets.

## Configure the npm bootstrapper

Before publishing to npm, set the repository in `packages/agent_orb/package.json`:

```json
"config": {
  "github_repository": "OWNER/REPO"
}
```

Or users can override at runtime:

```bash
AGENT_ORB_GITHUB_REPOSITORY=OWNER/REPO npx @solar_orb/agent_orb
AGENT_ORB_VERSION=v0.1.11 npx @solar_orb/agent_orb
```

By default, the bootstrapper downloads the GitHub Release tag matching its own npm package version, for example npm `0.1.11` downloads release `v0.1.11`. If `github_repository` is empty and no override is supplied, setup will fall back to local bundled assets or source build.

## Local smoke

```bash
./scripts/release/smoke-npx-local.sh
```

The smoke covers the npm-compatible install/upgrade path and adapter launchers. For status color regressions, also verify an observed adapter session reaches:

- `silent` / `yellow_thinking`
- `active` / `blue_spinning`
- `waiting_input` / `red_waiting`
- `completed` / `green_done`
- `compacting` / `purple_compacting`

## Real adapter smoke

When Codex CLI or Claude Code CLI is available on `PATH`, verify the installed runtime and generated shims against the real binaries:

```bash
AGENT_ORB_SKIP_UI_BUILD=1 ./scripts/smoke-real-adapters.sh
```

The script installs into temporary isolated runtime/config directories, runs:

- `agent_orb run -- codex --version`
- `codex-orb --version`
- `agent_orb run -- claude --version`
- `claude-orb --version`

Only adapters present on `PATH` are tested. Set `AGENT_ORB_REQUIRE_REAL_ADAPTERS=1` to fail when no real adapter is available.

On a Windows host, use the PowerShell equivalent after installing the Windows runtime:

```powershell
.\scripts\windows\install-agent-orb.ps1 -CreateAdapterShims
.\scripts\windows\smoke-real-adapters.ps1
```

The Windows smoke also uses an isolated temporary config directory and random local daemon port by default.
