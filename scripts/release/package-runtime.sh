#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-$ROOT/dist/release}"
mkdir -p "$OUT_DIR"

sha_file="$OUT_DIR/checksums.txt"
: > "$sha_file"

package_linux_x64() {
  echo "==> Building linux/x64 runtime"
  cargo build --release -p agent-orb-cli -p agent-orb-daemon --manifest-path "$ROOT/Cargo.toml"

  local staging
  staging="$(mktemp -d)"
  mkdir -p "$staging/agent-orb/bin"
  cp "$ROOT/target/release/agent_orb" "$staging/agent-orb/bin/agent_orb"
  cp "$ROOT/target/release/agent_orbd" "$staging/agent-orb/bin/agent_orbd"
  if [[ -x "$ROOT/apps/agent-orb-ui/src-tauri/target/release/agent-orb-ui" ]]; then
    cp "$ROOT/apps/agent-orb-ui/src-tauri/target/release/agent-orb-ui" "$staging/agent-orb/bin/agent-orb-ui"
  fi
  chmod +x "$staging/agent-orb/bin"/*

  local bundle="$OUT_DIR/agent-orb-linux-x64.tar.gz"
  tar -C "$staging" -czf "$bundle" agent-orb
  rm -rf "$staging"
  add_checksum "$bundle"
}

package_windows_x64_if_available() {
  if [[ "${AGENT_ORB_BUILD_WINDOWS:-0}" == "1" ]] && (command -v cargo-xwin >/dev/null 2>&1 || cargo xwin --version >/dev/null 2>&1); then
    echo "==> Building windows/x64 runtime with cargo-xwin"
    rustup target add x86_64-pc-windows-msvc >/dev/null
    cargo xwin build --release --target x86_64-pc-windows-msvc -p agent-orb-cli -p agent-orb-daemon --manifest-path "$ROOT/Cargo.toml"
  fi

  local target_dir="$ROOT/target/x86_64-pc-windows-msvc/release"
  local cli="$target_dir/agent_orb.exe"
  local daemon="$target_dir/agent_orbd.exe"
  if [[ ! -f "$cli" || ! -f "$daemon" ]]; then
    target_dir="$ROOT/target/x86_64-pc-windows-gnullvm/release"
    cli="$target_dir/agent_orb.exe"
    daemon="$target_dir/agent_orbd.exe"
  fi
  if [[ ! -f "$cli" || ! -f "$daemon" ]]; then
    target_dir="$ROOT/target/x86_64-pc-windows-gnu/release"
    cli="$target_dir/agent_orb.exe"
    daemon="$target_dir/agent_orbd.exe"
  fi

  if [[ ! -f "$cli" || ! -f "$daemon" ]]; then
    echo "· Windows runtime not found; skipping agent-orb-windows-x64.zip" >&2
    echo "  Build it in CI, on Windows, or rerun with AGENT_ORB_BUILD_WINDOWS=1 when cargo-xwin is ready." >&2
    return 0
  fi

  echo "==> Packaging windows/x64 runtime"
  local staging
  staging="$(mktemp -d)"
  mkdir -p "$staging/agent-orb/bin"
  cp "$cli" "$staging/agent-orb/bin/agent_orb.exe"
  cp "$daemon" "$staging/agent-orb/bin/agent_orbd.exe"
  if [[ -f "$ROOT/apps/agent-orb-ui/src-tauri/target/release/agent-orb-ui.exe" ]]; then
    cp "$ROOT/apps/agent-orb-ui/src-tauri/target/release/agent-orb-ui.exe" "$staging/agent-orb/bin/agent-orb-ui.exe"
  fi

  local bundle="$OUT_DIR/agent-orb-windows-x64.zip"
  rm -f "$bundle"
  if command -v powershell.exe >/dev/null 2>&1; then
    local staging_win bundle_win
    staging_win="$(wslpath -w "$staging/agent-orb")"
    bundle_win="$(wslpath -w "$bundle")"
    powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Compress-Archive -Path '$staging_win' -DestinationPath '$bundle_win' -Force" >/dev/null
  elif command -v zip >/dev/null 2>&1; then
    (cd "$staging" && zip -qr "$bundle" agent-orb)
  else
    tar -C "$staging" -cf "$bundle" agent-orb
  fi
  rm -rf "$staging"
  add_checksum "$bundle"
}

add_checksum() {
  local bundle="$1"
  local name
  name="$(basename "$bundle")"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$bundle" | awk -v name="$name" '{print $1 "  " name}' >> "$sha_file"
  else
    shasum -a 256 "$bundle" | awk -v name="$name" '{print $1 "  " name}' >> "$sha_file"
  fi
  echo "✓ $(basename "$bundle")"
}

package_linux_x64
package_windows_x64_if_available

echo "==> Checksums"
cat "$sha_file"
echo "Release dir: $OUT_DIR"
