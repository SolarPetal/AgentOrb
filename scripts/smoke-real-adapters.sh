#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OWN_CONFIG_DIR=0
OWN_BIN_DIR=0
OWN_PACK_DIR=0

if [[ -z "${AGENT_ORB_REAL_SMOKE_CONFIG_DIR:-}" ]]; then
  OWN_CONFIG_DIR=1
fi
if [[ -z "${AGENT_ORB_REAL_SMOKE_BIN_DIR:-}" ]]; then
  OWN_BIN_DIR=1
fi
if [[ -z "${PACK_DIR:-}" ]]; then
  OWN_PACK_DIR=1
fi

CONFIG_DIR="${AGENT_ORB_REAL_SMOKE_CONFIG_DIR:-$(mktemp -d)}"
BIN_DIR="${AGENT_ORB_REAL_SMOKE_BIN_DIR:-$(mktemp -d)}"
PACK_DIR="${PACK_DIR:-$(mktemp -d)}"
RELEASE_DIR="${AGENT_ORB_REAL_SMOKE_RELEASE_DIR:-$ROOT/dist/release-real-smoke}"
SMOKE_PORT="${AGENT_ORB_REAL_SMOKE_PORT:-$((34000 + RANDOM % 10000))}"
HOST="127.0.0.1"

# `agent_orb setup --yes` installs hooks and enables Codex's hooks feature.
# Isolate both adapter configuration roots alongside the temporary orb config.
export CLAUDE_CONFIG_DIR="$CONFIG_DIR/claude"
export CODEX_HOME="$CONFIG_DIR/codex"

cleanup() {
  if [[ "${AGENT_ORB_REAL_SMOKE_KEEP:-0}" == "1" ]]; then
    echo "Keeping smoke config: $CONFIG_DIR"
    echo "Keeping smoke bin:    $BIN_DIR"
    return
  fi

  if [[ -f "$CONFIG_DIR/daemon.pid" ]]; then
    kill "$(cat "$CONFIG_DIR/daemon.pid")" >/dev/null 2>&1 || true
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

log() {
  echo "==> $*"
}

warn() {
  echo "· $*" >&2
}

adapter_available() {
  command -v "$1" >/dev/null 2>&1
}

detected_adapters=()
for adapter in codex claude; do
  if adapter_available "$adapter"; then
    detected_adapters+=("$adapter")
  fi
done

if [[ ${#detected_adapters[@]} -eq 0 ]]; then
  warn "No real Codex or Claude CLI found on PATH."
  if [[ "${AGENT_ORB_REQUIRE_REAL_ADAPTERS:-0}" == "1" ]]; then
    exit 1
  fi
  exit 0
fi

log "Real adapters detected: ${detected_adapters[*]}"

log "Build bootstrapper"
npm --prefix "$ROOT/packages/agent_orb" run check
npm --prefix "$ROOT/packages/agent_orb" run build

log "Package runtime bundle"
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"
AGENT_ORB_SKIP_UI_BUILD="${AGENT_ORB_SKIP_UI_BUILD:-1}" \
  "$ROOT/scripts/release/package-runtime.sh" "$RELEASE_DIR"

log "Pack npm tarball"
(cd "$ROOT/packages/agent_orb" && npm pack --pack-destination "$PACK_DIR") \
  >/tmp/agent-orb-real-adapter-pack.out
cat /tmp/agent-orb-real-adapter-pack.out
TARBALL="$PACK_DIR/$(tail -n 1 /tmp/agent-orb-real-adapter-pack.out)"

log "Install isolated Agent Orb runtime"
AGENT_ORB_BIN_DIR="$BIN_DIR" \
AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" \
AGENT_ORB_DAEMON_PORT="$SMOKE_PORT" \
npm exec --yes --package "$TARBALL" -- agent_orb setup --yes --no-smoke --release-dir "$RELEASE_DIR"

for adapter in "${detected_adapters[@]}"; do
  case "$adapter" in
    claude) test -f "$CLAUDE_CONFIG_DIR/settings.json" ;;
    codex) test -f "$CODEX_HOME/hooks.json" ;;
  esac
done

read_token() {
  tr -d '\r\n' < "$CONFIG_DIR/token"
}

assert_status() {
  local expected_source="$1"
  local expected_status="$2"
  local token
  token="$(read_token)"

  node - "$expected_source" "$expected_status" "$HOST" "$SMOKE_PORT" "$token" <<'NODE'
const [expectedSource, expectedStatus, host, port, token] = process.argv.slice(2);

(async () => {
  const response = await fetch(`http://${host}:${port}/v1/status`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) {
    throw new Error(`status endpoint returned ${response.status}`);
  }

  const snapshot = await response.json();
  if (snapshot.status !== expectedStatus || snapshot.source !== expectedSource) {
    throw new Error(
      `unexpected status snapshot: expected ${expectedSource}/${expectedStatus}, got ${JSON.stringify(snapshot)}`,
    );
  }

  console.log(`✓ daemon status: ${snapshot.source}/${snapshot.status}`);
})().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
NODE
}

run_adapter() {
  local adapter="$1"
  local wrapper="${adapter}-orb"

  if ! adapter_available "$adapter"; then
    warn "$adapter not found; skipping"
    return
  fi

  log "agent_orb run -- $adapter --version"
  AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/agent_orb" run -- "$adapter" --version
  assert_status "$adapter" "completed"

  if [[ -x "$BIN_DIR/$wrapper" ]]; then
    log "$wrapper --version"
    AGENT_ORB_CONFIG_DIR="$CONFIG_DIR" "$BIN_DIR/$wrapper" --version
    assert_status "$adapter" "completed"
  else
    warn "$wrapper was not created; check adapter detection during setup"
    if [[ "${AGENT_ORB_REQUIRE_REAL_ADAPTERS:-0}" == "1" ]]; then
      exit 1
    fi
  fi
}

for adapter in "${detected_adapters[@]}"; do
  run_adapter "$adapter"
done

log "Installed files"
find "$BIN_DIR" -maxdepth 1 -type f -print | sort

echo "✓ real adapter smoke passed"
