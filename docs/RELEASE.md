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
git tag v0.1.0
git push origin v0.1.0
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
AGENT_ORB_GITHUB_REPOSITORY=OWNER/REPO npx agent_orb
AGENT_ORB_VERSION=v0.1.0 npx agent_orb
```

If `github_repository` is empty and no override is supplied, setup will fall back to local bundled assets or source build.

## Local smoke

```bash
./scripts/release/smoke-npx-local.sh
```
