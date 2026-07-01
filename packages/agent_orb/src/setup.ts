import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline/promises';
import { stdin as input, stdout as output } from 'node:process';
import { fileURLToPath } from 'node:url';
import { detectAdapters, type AdapterProfile } from './adapter.js';
import { runtimeConfigFromEnv, writeConfig, type RuntimeConfig } from './config.js';
import { cleanupInstalledRuntime, installRuntimeBundle } from './download.js';
import { detectPlatform, type PlatformInfo } from './platform.js';
import { commandExists, getPathEnv, run, setPathEnv, spawnDetached } from './shell.js';

interface SetupOptions {
  yes?: boolean;
  doctorOnly?: boolean;
  smoke?: boolean;
  force?: boolean;
  buildFromSource?: boolean;
  releaseBaseUrl?: string;
  releaseDir?: string;
}

export async function setup(options: SetupOptions = {}): Promise<void> {
  const platform = detectPlatform();
  const runtime = runtimeConfigFromEnv();
  printHeader(platform);
  assertSupported(platform);

  if (options.doctorOnly) {
    await doctor(platform, runtime);
    return;
  }

  console.log(`Runtime dir: ${platform.runtimeDir}`);
  console.log(`Config dir:  ${platform.configDir}`);

  const detectedAdapters = detectAdapters();
  printDetectedAdapters(detectedAdapters);
  const selectedAdapters = await selectAdapters(detectedAdapters, options.yes);

  if (options.buildFromSource) {
    installRuntimeFromSource(platform);
  } else {
    const installedFromBundle = await installRuntimeBundle(platform, {
      force: options.force,
      releaseBaseUrl: options.releaseBaseUrl,
      releaseDir: options.releaseDir,
    });
    if (!installedFromBundle) {
      console.log('\n· No matching native runtime bundle was found for this platform.');
      console.log('· Falling back to local source build.');
      installRuntimeFromSource(platform);
    }
  }

  const configPath = writeConfig(platform.configDir, selectedAdapters, runtime);
  createAdapterShims(platform, selectedAdapters);
  ensurePathConfigured(platform);
  await ensureDaemon(platform, runtime);
  const orbStarted = startOrbUiIfAvailable(platform);

  if (options.smoke ?? true) {
    smokeTest(platform);
  }

  console.log('\n✓ Agent Orb setup complete');
  console.log(`Config: ${configPath}`);
  console.log(`Try:    agent_orb run -- ${platform.platform === 'windows' ? 'cmd /C echo hello' : 'echo hello'}`);
  if (selectedAdapters.some((adapter) => adapter.name === 'codex')) {
    console.log('Codex:  agent_orb-codex');
  }
  if (selectedAdapters.some((adapter) => adapter.name === 'claude')) {
    console.log('Claude: agent_orb-claude');
  }
  const orb = runtimeExe(platform, 'agent-orb-ui');
  if (fs.existsSync(orb)) {
    console.log(`Orb:    ${orbStarted ? 'started' : orb}`);
  }
}

function installRuntimeFromSource(platform: PlatformInfo): void {
  console.log('\n==> Building runtime from source');
  ensureBuildTools();
  const repoRoot = findRepoRoot();
  console.log(`Repository: ${repoRoot}`);
  buildRuntime(repoRoot);
  cleanupInstalledRuntime(platform);
  installRuntime(repoRoot, platform);
}

export async function doctor(platform = detectPlatform(), runtime = runtimeConfigFromEnv()): Promise<void> {
  printHeader(platform);
  assertSupported(platform);
  const binaries = ['agent_orb', 'agent_orbd'].map((name) => runtimeExe(platform, name));
  for (const binary of binaries) {
    console.log(`${fs.existsSync(binary) ? '✓' : '✗'} ${binary}`);
  }
  console.log(`${fs.existsSync(path.join(platform.configDir, 'token')) ? '✓' : '·'} token: ${path.join(platform.configDir, 'token')}`);
  console.log(`${await health(runtime) ? '✓' : '·'} daemon: http://${runtime.daemonHost}:${runtime.daemonPort}/health`);
  printDetectedAdapters(detectAdapters());
}

function printHeader(platform: PlatformInfo): void {
  console.log('Agent Orb Setup');
  console.log(`Platform: ${platform.platform}/${platform.arch}`);
  console.log(`Bundle:   ${platform.bundleName}`);
}

function assertSupported(platform: PlatformInfo): void {
  if (platform.platform === 'unsupported' || platform.arch === 'unsupported') {
    throw new Error(`Unsupported platform: ${process.platform}/${process.arch}`);
  }
}

function ensureBuildTools(): void {
  if (!commandExists('cargo') || !commandExists('rustc')) {
    const install = process.platform === 'win32'
      ? 'winget install --id Rustlang.Rustup -e'
      : 'curl https://sh.rustup.rs -sSf | sh';
    throw new Error(`Rust toolchain is required for local npx setup. Install it first:\n  ${install}`);
  }
}

function buildRuntime(repoRoot: string): void {
  console.log('\n==> Building Agent Orb runtime');
  run('cargo', ['build', '--release', '-p', 'agent-orb-cli', '-p', 'agent-orb-daemon'], {
    cwd: repoRoot,
    stdio: 'inherit',
  });
}

function installRuntime(repoRoot: string, platform: PlatformInfo): void {
  console.log('\n==> Installing runtime');
  fs.mkdirSync(platform.runtimeDir, { recursive: true });
  copyRequired(repoRoot, platform, 'agent_orb');
  copyRequired(repoRoot, platform, 'agent_orbd');

  const uiBinary = path.join(repoRoot, 'apps', 'agent-orb-ui', 'src-tauri', 'target', 'release', `agent-orb-ui${platform.exeSuffix}`);
  if (fs.existsSync(uiBinary)) {
    fs.copyFileSync(uiBinary, path.join(platform.runtimeDir, `agent-orb-ui${platform.exeSuffix}`));
    console.log(`✓ installed agent-orb-ui${platform.exeSuffix}`);
  } else {
    console.log('· UI binary not found, skipping UI install for now');
  }
}

function copyRequired(repoRoot: string, platform: PlatformInfo, name: string): void {
  const source = path.join(repoRoot, 'target', 'release', `${name}${platform.exeSuffix}`);
  if (!fs.existsSync(source)) {
    throw new Error(`Build output not found: ${source}`);
  }
  const dest = path.join(platform.runtimeDir, `${name}${platform.exeSuffix}`);

  if (sameFileContent(source, dest)) {
    console.log(`✓ ${dest} already up to date`);
    return;
  }

  copyBinaryReplacingExisting(source, dest, platform);
  console.log(`✓ installed ${dest}`);
}

function sameFileContent(left: string, right: string): boolean {
  if (!fs.existsSync(left) || !fs.existsSync(right)) return false;

  const leftStat = fs.statSync(left);
  const rightStat = fs.statSync(right);
  if (leftStat.size !== rightStat.size) return false;

  return fs.readFileSync(left).equals(fs.readFileSync(right));
}

function copyBinaryReplacingExisting(source: string, dest: string, platform: PlatformInfo): void {
  const temp = path.join(path.dirname(dest), `.${path.basename(dest)}.${process.pid}.tmp`);
  fs.copyFileSync(source, temp);
  if (platform.platform !== 'windows') fs.chmodSync(temp, 0o755);

  try {
    fs.renameSync(temp, dest);
  } catch (error) {
    fs.rmSync(temp, { force: true });
    if (platform.platform === 'windows') {
      throw new Error(
        `Could not replace ${dest}. If Agent Orb is already running, stop agent_orbd.exe / agent-orb-ui.exe and rerun npx agent_orb. Original error: ${formatError(error)}`,
      );
    }
    throw error;
  }
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function selectAdapters(adapters: AdapterProfile[], yes = false): Promise<AdapterProfile[]> {
  if (yes) {
    return adapters.filter((adapter) => adapter.foundBinary);
  }

  const found = adapters.filter((adapter) => adapter.foundBinary);
  if (found.length === 0) {
    console.log('No Codex/Claude CLI detected. Manual agent_orb run remains available.');
    return [];
  }
  if (!process.stdin.isTTY) {
    return found;
  }

  const rl = readline.createInterface({ input, output });
  const choices = found.map((adapter, index) => `${index + 1}) ${adapter.displayName}`).join('\n');
  const answer = await rl.question(`\nChoose adapters to configure (comma-separated, Enter = all):\n${choices}\n> `);
  rl.close();
  if (!answer.trim()) return found;

  const selectedIndexes = new Set(answer.split(',').map((part) => Number.parseInt(part.trim(), 10) - 1));
  return found.filter((_, index) => selectedIndexes.has(index));
}

function printDetectedAdapters(adapters: AdapterProfile[]): void {
  console.log('\nDetected adapters:');
  for (const adapter of adapters) {
    if (adapter.foundBinary) {
      console.log(`  ✓ ${adapter.displayName}: ${adapter.foundBinary}`);
    } else {
      console.log(`  · ${adapter.displayName}: not found`);
    }
  }
}

function createAdapterShims(platform: PlatformInfo, adapters: AdapterProfile[]): void {
  if (adapters.length === 0) return;
  console.log('\n==> Creating adapter shims');
  for (const adapter of adapters) {
    const commands = uniqueStrings([adapter.launcherCommand, adapter.wrapperCommand]);
    for (const command of commands) {
      const shimPath = path.join(platform.runtimeDir, command);
      if (platform.platform === 'windows') {
        fs.writeFileSync(shimPath, windowsAdapterShim(adapter.name, adapter.foundBinary), 'ascii');
      } else {
        fs.writeFileSync(shimPath, unixAdapterShim(adapter.name, adapter.foundBinary), 'utf8');
        fs.chmodSync(shimPath, 0o755);
      }
      console.log(`✓ ${shimPath}`);
    }
  }
}

function uniqueStrings(values: string[]): string[] {
  return [...new Set(values)];
}

function windowsAdapterShim(adapterName: AdapterProfile['name'], adapterBinary?: string): string {
  const adapterCommand = escapeWindowsCmdSetValue(adapterBinary ?? adapterName);
  return `@echo off\r\nsetlocal\r\nset "AGENT_ORB_EXE=%~dp0agent_orb.exe"\r\nif not exist "%AGENT_ORB_EXE%" (\r\n  for %%I in (agent_orb.exe) do set "AGENT_ORB_EXE=%%~$PATH:I"\r\n)\r\nif not exist "%AGENT_ORB_EXE%" (\r\n  echo agent_orb runtime is missing: %~dp0agent_orb.exe 1>&2\r\n  echo Run: npx --yes @solar_orb/agent_orb upgrade --yes 1>&2\r\n  exit /b 1\r\n)\r\nset "ADAPTER_CMD=${adapterCommand}"\r\nset "ORB_UI=%~dp0agent-orb-ui.exe"\r\nif exist "%ORB_UI%" (\r\n  tasklist /FI "IMAGENAME eq agent-orb-ui.exe" 2>NUL | find /I "agent-orb-ui.exe" >NUL\r\n  if errorlevel 1 start "" "%ORB_UI%"\r\n)\r\n"%AGENT_ORB_EXE%" run -- "%ADAPTER_CMD%" %*\r\nexit /b %ERRORLEVEL%\r\n`;
}

function unixAdapterShim(adapterName: AdapterProfile['name'], adapterBinary?: string): string {
  const adapterCommand = shellSingleQuote(adapterBinary ?? adapterName);
  return `#!/usr/bin/env sh\nset -eu\nDIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nAGENT_ORB_EXE="$DIR/agent_orb"\nif [ ! -x "$AGENT_ORB_EXE" ]; then\n  AGENT_ORB_EXE=$(command -v agent_orb || true)\nfi\nif [ -z "$AGENT_ORB_EXE" ] || [ ! -x "$AGENT_ORB_EXE" ]; then\n  echo "agent_orb runtime is missing: $DIR/agent_orb" >&2\n  echo "Run: npx --yes @solar_orb/agent_orb upgrade --yes" >&2\n  exit 1\nfi\nADAPTER_CMD=${adapterCommand}\nORB_UI="$DIR/agent-orb-ui"\nif [ -x "$ORB_UI" ] && { [ -n "\${DISPLAY:-}" ] || [ -n "\${WAYLAND_DISPLAY:-}" ]; }; then\n  running=0\n  if command -v pgrep >/dev/null 2>&1 && pgrep -x agent-orb-ui >/dev/null 2>&1; then\n    running=1\n  fi\n  if [ "$running" = "0" ]; then\n    "$ORB_UI" >/dev/null 2>&1 &\n  fi\nfi\nexec "$AGENT_ORB_EXE" run -- "$ADAPTER_CMD" "$@"\n`;
}

function escapeWindowsCmdSetValue(value: string): string {
  return value.replace(/[\^&|<>()%!\"]/g, (char) => `^${char}`);
}

function shellSingleQuote(value: string): string {
  return `'${value.replace(/'/g, `'"'"'`)}'`;
}

function ensurePathConfigured(platform: PlatformInfo): void {
  const currentPath = getPathEnv();
  const parts = currentPath.split(platform.pathDelimiter).filter(Boolean);
  if (pathPartsContain(parts, platform.runtimeDir, platform)) {
    console.log(`✓ runtime dir already on PATH: ${platform.runtimeDir}`);
    return;
  }

  setPathEnv(`${platform.runtimeDir}${platform.pathDelimiter}${currentPath}`);

  console.log('\nPATH note:');
  if (platform.platform === 'windows') {
    if (addWindowsUserPath(platform.runtimeDir)) {
      console.log(`  ✓ added runtime dir to user PATH: ${platform.runtimeDir}`);
      console.log('  Open a new terminal to use agent_orb-codex, agent_orb-claude, and agent_orb globally.');
    } else {
      console.log(`  Could not update user PATH automatically. Add manually if needed: ${platform.runtimeDir}`);
    }
  } else {
    console.log(`  export PATH="${platform.runtimeDir}:$PATH"`);
  }
}

function pathPartsContain(parts: string[], target: string, platform: PlatformInfo): boolean {
  const normalizedTarget = normalizePathForCompare(target, platform);
  return parts.some((part) => normalizePathForCompare(part, platform) === normalizedTarget);
}

function normalizePathForCompare(value: string, platform: PlatformInfo): string {
  const trimmed = value.trim().replace(/[\\/]+$/, '');
  return platform.platform === 'windows' ? trimmed.toLowerCase() : trimmed;
}

function addWindowsUserPath(targetDir: string): boolean {
  const script = [
    '$target = $env:AGENT_ORB_TARGET_PATH',
    "$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')",
    '$parts = @()',
    'if ($userPath) { $parts = $userPath -split \';\' | Where-Object { $_ } }',
    '$normalizedTarget = $target.TrimEnd("\\").ToLowerInvariant()',
    '$exists = $false',
    'foreach ($part in $parts) {',
    '  if ($part.TrimEnd("\\").ToLowerInvariant() -eq $normalizedTarget) { $exists = $true; break }',
    '}',
    'if (-not $exists) {',
    "  [Environment]::SetEnvironmentVariable('Path', (($parts + $target) -join ';'), 'User')",
    '}',
  ].join('; ');

  try {
    run('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', script], {
      env: {
        ...process.env,
        AGENT_ORB_TARGET_PATH: targetDir,
      },
    });
    return true;
  } catch {
    return false;
  }
}

function startOrbUiIfAvailable(platform: PlatformInfo): boolean {
  const orb = runtimeExe(platform, 'agent-orb-ui');
  if (!fs.existsSync(orb)) return false;

  if (platform.platform === 'windows') {
    if (isWindowsProcessRunning('agent-orb-ui.exe')) return true;
    spawnDetached(orb, []);
    return true;
  }

  if (!process.env.DISPLAY && !process.env.WAYLAND_DISPLAY) return false;
  if (isUnixProcessRunning('agent-orb-ui')) return true;
  spawnDetached(orb, []);
  return true;
}

function isWindowsProcessRunning(imageName: string): boolean {
  try {
    const result = run('tasklist', ['/FI', `IMAGENAME eq ${imageName}`], {
      allowFailure: true,
    });
    return result.stdout.toLowerCase().includes(imageName.toLowerCase());
  } catch {
    return false;
  }
}

function isUnixProcessRunning(processName: string): boolean {
  if (!commandExists('pgrep')) return false;
  try {
    const result = run('pgrep', ['-x', processName], {
      allowFailure: true,
    });
    return result.status === 0;
  } catch {
    return false;
  }
}

async function ensureDaemon(platform: PlatformInfo, runtime: RuntimeConfig): Promise<void> {
  console.log('\n==> Starting daemon');
  const tokenPath = path.join(platform.configDir, 'token');
  if (fs.existsSync(tokenPath) && await authenticatedStatus(tokenPath, runtime)) {
    console.log(`✓ daemon already healthy at http://${runtime.daemonHost}:${runtime.daemonPort}/health`);
    return;
  }

  const daemon = runtimeExe(platform, 'agent_orbd');
  const daemonAlreadyHealthy = await health(runtime);
  if (daemonAlreadyHealthy) {
    throw new Error(
      `agent_orbd is already running on ${runtime.daemonHost}:${runtime.daemonPort}, but it does not accept the token at ${tokenPath}. Stop the existing agent_orbd process and rerun setup.`,
    );
  }

  const pid = spawnDetached(daemon, []);
  if (pid) {
    fs.mkdirSync(platform.configDir, { recursive: true });
    fs.writeFileSync(path.join(platform.configDir, 'daemon.pid'), `${pid}\n`, 'utf8');
  }

  for (let i = 0; i < 40; i++) {
    if (fs.existsSync(tokenPath) && await authenticatedStatus(tokenPath, runtime)) {
      console.log(`✓ daemon healthy at http://${runtime.daemonHost}:${runtime.daemonPort}/health`);
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`daemon did not become healthy on ${runtime.daemonHost}:${runtime.daemonPort}`);
}

async function health(runtime: RuntimeConfig): Promise<boolean> {
  try {
    const response = await fetch(`http://${runtime.daemonHost}:${runtime.daemonPort}/health`);
    return response.ok;
  } catch {
    return false;
  }
}

async function authenticatedStatus(tokenPath: string, runtime: RuntimeConfig): Promise<boolean> {
  try {
    const token = fs.readFileSync(tokenPath, 'utf8').trim();
    if (!token) return false;

    const response = await fetch(`http://${runtime.daemonHost}:${runtime.daemonPort}/v1/status`, {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });
    return response.ok;
  } catch {
    return false;
  }
}

function smokeTest(platform: PlatformInfo): void {
  console.log('\n==> Smoke test');
  const agentOrb = runtimeExe(platform, 'agent_orb');
  if (platform.platform === 'windows') {
    run(agentOrb, ['run', '--', 'cmd', '/C', 'echo', 'hello'], { stdio: 'inherit' });
  } else {
    run(agentOrb, ['run', '--', 'echo', 'hello'], { stdio: 'inherit' });
  }
}

function runtimeExe(platform: PlatformInfo, name: string): string {
  return path.join(platform.runtimeDir, `${name}${platform.exeSuffix}`);
}

function findRepoRoot(): string {
  if (process.env.AGENT_ORB_REPO) return process.env.AGENT_ORB_REPO;

  let current = process.cwd();
  for (;;) {
    if (fs.existsSync(path.join(current, 'Cargo.toml')) && fs.existsSync(path.join(current, 'crates'))) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }

  const fromPackage = path.resolve(fileURLToPath(new URL('../../..', import.meta.url)));
  if (fs.existsSync(path.join(fromPackage, 'Cargo.toml'))) return fromPackage;

  throw new Error('Could not find AgentOrb repo root. Set AGENT_ORB_REPO to the repo path.');
}
