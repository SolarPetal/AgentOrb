import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline/promises';
import { stdin as input, stdout as output } from 'node:process';
import { fileURLToPath } from 'node:url';
import { detectAdapters } from './adapter.js';
import { runtimeConfigFromEnv, writeConfig } from './config.js';
import { installRuntimeBundle } from './download.js';
import { detectPlatform } from './platform.js';
import { commandExists, run, spawnDetached } from './shell.js';
export async function setup(options = {}) {
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
    }
    else {
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
    ensurePathHint(platform);
    await ensureDaemon(platform, runtime);
    if (options.smoke ?? true) {
        smokeTest(platform);
    }
    console.log('\n✓ Agent Orb setup complete');
    console.log(`Config: ${configPath}`);
    console.log(`Try:    ${runtimeExe(platform, 'agent_orb')} run -- ${platform.platform === 'windows' ? 'cmd /C echo hello' : 'echo hello'}`);
    const orb = runtimeExe(platform, 'agent-orb-ui');
    if (fs.existsSync(orb)) {
        console.log(`Orb:    ${orb}`);
    }
}
function installRuntimeFromSource(platform) {
    console.log('\n==> Building runtime from source');
    ensureBuildTools();
    const repoRoot = findRepoRoot();
    console.log(`Repository: ${repoRoot}`);
    buildRuntime(repoRoot);
    installRuntime(repoRoot, platform);
}
export async function doctor(platform = detectPlatform(), runtime = runtimeConfigFromEnv()) {
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
function printHeader(platform) {
    console.log('Agent Orb Setup');
    console.log(`Platform: ${platform.platform}/${platform.arch}`);
    console.log(`Bundle:   ${platform.bundleName}`);
}
function assertSupported(platform) {
    if (platform.platform === 'unsupported' || platform.arch === 'unsupported') {
        throw new Error(`Unsupported platform: ${process.platform}/${process.arch}`);
    }
}
function ensureBuildTools() {
    if (!commandExists('cargo') || !commandExists('rustc')) {
        const install = process.platform === 'win32'
            ? 'winget install --id Rustlang.Rustup -e'
            : 'curl https://sh.rustup.rs -sSf | sh';
        throw new Error(`Rust toolchain is required for local npx setup. Install it first:\n  ${install}`);
    }
}
function buildRuntime(repoRoot) {
    console.log('\n==> Building Agent Orb runtime');
    run('cargo', ['build', '--release', '-p', 'agent-orb-cli', '-p', 'agent-orb-daemon'], {
        cwd: repoRoot,
        stdio: 'inherit',
    });
}
function installRuntime(repoRoot, platform) {
    console.log('\n==> Installing runtime');
    fs.mkdirSync(platform.runtimeDir, { recursive: true });
    copyRequired(repoRoot, platform, 'agent_orb');
    copyRequired(repoRoot, platform, 'agent_orbd');
    const uiBinary = path.join(repoRoot, 'apps', 'agent-orb-ui', 'src-tauri', 'target', 'release', `agent-orb-ui${platform.exeSuffix}`);
    if (fs.existsSync(uiBinary)) {
        fs.copyFileSync(uiBinary, path.join(platform.runtimeDir, `agent-orb-ui${platform.exeSuffix}`));
        console.log(`✓ installed agent-orb-ui${platform.exeSuffix}`);
    }
    else {
        console.log('· UI binary not found, skipping UI install for now');
    }
}
function copyRequired(repoRoot, platform, name) {
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
function sameFileContent(left, right) {
    if (!fs.existsSync(left) || !fs.existsSync(right))
        return false;
    const leftStat = fs.statSync(left);
    const rightStat = fs.statSync(right);
    if (leftStat.size !== rightStat.size)
        return false;
    return fs.readFileSync(left).equals(fs.readFileSync(right));
}
function copyBinaryReplacingExisting(source, dest, platform) {
    const temp = path.join(path.dirname(dest), `.${path.basename(dest)}.${process.pid}.tmp`);
    fs.copyFileSync(source, temp);
    if (platform.platform !== 'windows')
        fs.chmodSync(temp, 0o755);
    try {
        fs.renameSync(temp, dest);
    }
    catch (error) {
        fs.rmSync(temp, { force: true });
        if (platform.platform === 'windows') {
            throw new Error(`Could not replace ${dest}. If Agent Orb is already running, stop agent_orbd.exe / agent-orb-ui.exe and rerun npx agent_orb. Original error: ${formatError(error)}`);
        }
        throw error;
    }
}
function formatError(error) {
    return error instanceof Error ? error.message : String(error);
}
async function selectAdapters(adapters, yes = false) {
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
    if (!answer.trim())
        return found;
    const selectedIndexes = new Set(answer.split(',').map((part) => Number.parseInt(part.trim(), 10) - 1));
    return found.filter((_, index) => selectedIndexes.has(index));
}
function printDetectedAdapters(adapters) {
    console.log('\nDetected adapters:');
    for (const adapter of adapters) {
        if (adapter.foundBinary) {
            console.log(`  ✓ ${adapter.displayName}: ${adapter.foundBinary}`);
        }
        else {
            console.log(`  · ${adapter.displayName}: not found`);
        }
    }
}
function createAdapterShims(platform, adapters) {
    if (adapters.length === 0)
        return;
    console.log('\n==> Creating adapter shims');
    for (const adapter of adapters) {
        const shimPath = path.join(platform.runtimeDir, adapter.wrapperCommand);
        if (platform.platform === 'windows') {
            const target = adapter.name;
            fs.writeFileSync(shimPath, `@echo off\r\n"%~dp0agent_orb.exe" run -- ${target} %*\r\n`, 'ascii');
        }
        else {
            fs.writeFileSync(shimPath, `#!/usr/bin/env sh\n"$(dirname "$0")/agent_orb" run -- ${adapter.name} "$@"\n`, 'utf8');
            fs.chmodSync(shimPath, 0o755);
        }
        console.log(`✓ ${shimPath}`);
    }
}
function ensurePathHint(platform) {
    const currentPath = process.env.PATH ?? '';
    const parts = currentPath.split(platform.pathDelimiter).filter(Boolean);
    if (parts.includes(platform.runtimeDir))
        return;
    console.log('\nPATH note:');
    if (platform.platform === 'windows') {
        console.log(`  Add to user PATH if needed: ${platform.runtimeDir}`);
        console.log('  Or open a new terminal if installer/script already added it.');
    }
    else {
        console.log(`  export PATH="${platform.runtimeDir}:$PATH"`);
    }
}
async function ensureDaemon(platform, runtime) {
    console.log('\n==> Starting daemon');
    const tokenPath = path.join(platform.configDir, 'token');
    if (fs.existsSync(tokenPath) && await authenticatedStatus(tokenPath, runtime)) {
        console.log(`✓ daemon already healthy at http://${runtime.daemonHost}:${runtime.daemonPort}/health`);
        return;
    }
    const daemon = runtimeExe(platform, 'agent_orbd');
    const daemonAlreadyHealthy = await health(runtime);
    if (daemonAlreadyHealthy) {
        throw new Error(`agent_orbd is already running on ${runtime.daemonHost}:${runtime.daemonPort}, but it does not accept the token at ${tokenPath}. Stop the existing agent_orbd process and rerun setup.`);
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
async function health(runtime) {
    try {
        const response = await fetch(`http://${runtime.daemonHost}:${runtime.daemonPort}/health`);
        return response.ok;
    }
    catch {
        return false;
    }
}
async function authenticatedStatus(tokenPath, runtime) {
    try {
        const token = fs.readFileSync(tokenPath, 'utf8').trim();
        if (!token)
            return false;
        const response = await fetch(`http://${runtime.daemonHost}:${runtime.daemonPort}/v1/status`, {
            headers: {
                Authorization: `Bearer ${token}`,
            },
        });
        return response.ok;
    }
    catch {
        return false;
    }
}
function smokeTest(platform) {
    console.log('\n==> Smoke test');
    const agentOrb = runtimeExe(platform, 'agent_orb');
    if (platform.platform === 'windows') {
        run(agentOrb, ['run', '--', 'cmd', '/C', 'echo', 'hello'], { stdio: 'inherit' });
    }
    else {
        run(agentOrb, ['run', '--', 'echo', 'hello'], { stdio: 'inherit' });
    }
}
function runtimeExe(platform, name) {
    return path.join(platform.runtimeDir, `${name}${platform.exeSuffix}`);
}
function findRepoRoot() {
    if (process.env.AGENT_ORB_REPO)
        return process.env.AGENT_ORB_REPO;
    let current = process.cwd();
    for (;;) {
        if (fs.existsSync(path.join(current, 'Cargo.toml')) && fs.existsSync(path.join(current, 'crates'))) {
            return current;
        }
        const parent = path.dirname(current);
        if (parent === current)
            break;
        current = parent;
    }
    const fromPackage = path.resolve(fileURLToPath(new URL('../../..', import.meta.url)));
    if (fs.existsSync(path.join(fromPackage, 'Cargo.toml')))
        return fromPackage;
    throw new Error('Could not find AgentOrb repo root. Set AGENT_ORB_REPO to the repo path.');
}
