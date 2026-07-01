#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OWN_PACK_DIR=0
OWN_BIN_DIR=0
OWN_CONFIG_DIR=0

if [[ -z "${PACK_DIR:-}" ]]; then
  OWN_PACK_DIR=1
fi
if [[ -z "${AGENT_ORB_SMOKE_BIN_DIR:-}" ]]; then
  OWN_BIN_DIR=1
fi
if [[ -z "${AGENT_ORB_SMOKE_CONFIG_DIR:-}" ]]; then
  OWN_CONFIG_DIR=1
fi

PACK_DIR="${PACK_DIR:-$(mktemp -d)}"
BIN_DIR="${AGENT_ORB_SMOKE_BIN_DIR:-$(mktemp -d)}"
CONFIG_DIR="${AGENT_ORB_SMOKE_CONFIG_DIR:-$(mktemp -d)}"
SMOKE_PORT="${AGENT_ORB_SMOKE_PORT:-$((23000 + RANDOM % 10000))}"
FAKE_ADAPTER_DIR="$(mktemp -d)"

cat > "$FAKE_ADAPTER_DIR/codex" <<'SH'
#!/usr/bin/env sh
echo fake-codex-ok
SH
cat > "$FAKE_ADAPTER_DIR/claude" <<'SH'
#!/usr/bin/env sh
echo "continue? yes/no"
SH
chmod +x "$FAKE_ADAPTER_DIR/codex" "$FAKE_ADAPTER_DIR/claude"

cleanup() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${CONFIG_DIR:-}" && -f "$CONFIG_DIR/daemon.pid" ]]; then
    kill "$(cat "$CONFIG_DIR/daemon.pid")" >/dev/null 2>&1 || true
  fi
  rm -rf "$FAKE_ADAPTER_DIR"
  if [[ "${AGENT_ORB_SMOKE_KEEP:-0}" == "1" ]]; then
    echo "Keeping smoke config: $CONFIG_DIR"
    echo "Keeping smoke bin:    $BIN_DIR"
    echo "Keeping smoke pack:   $PACK_DIR"
    return
  fi
  if [[ "$OWN_CONFIG_DIR" == "1" ]]; then
    rm -rf "$CONFIG_DIR"
  fi
  if [[ "$OWN_BIN_DIR" == "1" ]]; then
    rm -rf "$BIN_DIR"
  fi
  if [[ "$OWN_PACK_DIR" == "1" ]]; then
    rm -rf "$PACK_DIR"
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
TARBALL="$PACK_DIR/$(tail -n 1 /tmp/agent-orb-npm-pack.out)"

echo "==> Install via npx-compatible npm exec"
PATH="$FAKE_ADAPTER_DIR:$PATH" \
AGENT_ORB_BIN_DIR="$BIN_DIR" \
AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" \
AGENT_ORB_DAEMON_PORT="$SMOKE_PORT" \
npm exec --yes --package "$TARBALL" -- agent_orb setup --yes --no-smoke --release-dir "$RELEASE_DIR"

echo "==> Upgrade smoke"
PATH="$FAKE_ADAPTER_DIR:$PATH" \
AGENT_ORB_BIN_DIR="$BIN_DIR" \
AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" \
AGENT_ORB_DAEMON_PORT="$SMOKE_PORT" \
npm exec --yes --package "$TARBALL" -- agent_orb upgrade --yes --no-smoke --release-dir "$RELEASE_DIR"

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
AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/agent_orb" run -- echo npx-smoke-ok

echo "==> Adapter wrapper smoke"
PATH="$FAKE_ADAPTER_DIR:$PATH" AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/agent_orb" run -- codex
PATH="$FAKE_ADAPTER_DIR:$PATH" AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/agent_orb" run -- claude
PATH="$FAKE_ADAPTER_DIR:$PATH" AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/agent_orb-codex"
PATH="$FAKE_ADAPTER_DIR:$PATH" AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/agent_orb-claude"
PATH="$FAKE_ADAPTER_DIR:$PATH" AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/codex-orb"
PATH="$FAKE_ADAPTER_DIR:$PATH" AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/claude-orb"

echo "==> Installed files"
find "$BIN_DIR" -maxdepth 1 -type f -print | sort

echo "✓ npx local smoke passed"
