#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PACK_DIR="${PACK_DIR:-$(mktemp -d)}"
BIN_DIR="${AGENT_ORB_SMOKE_BIN_DIR:-$(mktemp -d)}"
CONFIG_DIR="${AGENT_ORB_SMOKE_CONFIG_DIR:-}"

cleanup() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

echo "==> Build bootstrapper"
npm --prefix "$ROOT/packages/agent_orb" run check
npm --prefix "$ROOT/packages/agent_orb" run build
npm --prefix "$ROOT/packages/agent_orb" audit

echo "==> Package runtime bundle"
RELEASE_DIR="$ROOT/dist/release-smoke"
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"
"$ROOT/scripts/release/package-runtime.sh" "$RELEASE_DIR"

echo "==> Pack npm tarball"
(cd "$ROOT/packages/agent_orb" && npm pack --pack-destination "$PACK_DIR") >/tmp/agent-orb-npm-pack.out
cat /tmp/agent-orb-npm-pack.out
TARBALL="$PACK_DIR/agent_orb-0.1.0.tgz"

echo "==> Install via npx-compatible npm exec"
if [[ -n "$CONFIG_DIR" ]]; then
  AGENT_ORB_BIN_DIR="$BIN_DIR" \
  AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" \
  npm exec --yes --package "$TARBALL" -- agent_orb setup --yes --no-smoke --release-dir "$RELEASE_DIR"
else
  AGENT_ORB_BIN_DIR="$BIN_DIR" \
  npm exec --yes --package "$TARBALL" -- agent_orb setup --yes --no-smoke --release-dir "$RELEASE_DIR"
fi

if [[ -n "$CONFIG_DIR" && ! -f "$CONFIG_DIR/token" ]]; then
  echo "token was not created; starting isolated smoke daemon" >&2
  AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/agent_orbd" >/tmp/agent-orb-smoke-daemon.log 2>&1 &
  DAEMON_PID=$!
  for _ in $(seq 1 40); do
    [[ -s "$CONFIG_DIR/token" ]] && break
    sleep 0.25
  done
fi

echo "==> Runtime smoke"
if [[ -n "$CONFIG_DIR" ]]; then
  AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/agent_orb" run -- echo npx-smoke-ok
else
  "$BIN_DIR/agent_orb" run -- echo npx-smoke-ok
fi

echo "==> Installed files"
find "$BIN_DIR" -maxdepth 1 -type f -print | sort

echo "✓ npx local smoke passed"
