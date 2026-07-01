import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseChecksums, verifyChecksum } from './checksum.js';
import { commandExists, run } from './shell.js';
export async function installRuntimeBundle(platform, options = {}) {
    if (!options.force && runtimeLooksInstalled(platform)) {
        console.log('\n==> Runtime bundle');
        console.log(`✓ existing runtime found at ${platform.runtimeDir}`);
        return true;
    }
    const baseUrl = releaseBaseUrl(platform, options);
    if (!baseUrl)
        return false;
    console.log('\n==> Downloading runtime bundle');
    console.log(`Release base: ${baseUrl}`);
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-orb-runtime-'));
    try {
        const bundlePath = path.join(tempDir, platform.bundleName);
        const checksumsPath = path.join(tempDir, 'checksums.txt');
        await downloadFile(joinUrl(baseUrl, platform.bundleName), bundlePath);
        await downloadFile(joinUrl(baseUrl, 'checksums.txt'), checksumsPath);
        const checksums = parseChecksums(fs.readFileSync(checksumsPath, 'utf8'));
        const expected = checksums.get(platform.bundleName);
        if (!expected) {
            throw new Error(`checksums.txt does not contain an entry for ${platform.bundleName}`);
        }
        verifyChecksum(bundlePath, expected);
        console.log(`✓ checksum verified: ${platform.bundleName}`);
        cleanupInstalledRuntime(platform);
        extractBundle(bundlePath, tempDir, platform);
        writeInstallManifest(platform, {
            bundle: platform.bundleName,
            sha256: expected,
            source: baseUrl,
        });
        console.log(`✓ installed runtime bundle into ${platform.runtimeDir}`);
        return true;
    }
    finally {
        fs.rmSync(tempDir, { recursive: true, force: true });
    }
}
export function runtimeLooksInstalled(platform) {
    const required = ['agent_orb', 'agent_orbd'].map((name) => path.join(platform.runtimeDir, `${name}${platform.exeSuffix}`));
    return required.every((file) => fs.existsSync(file));
}
export function cleanupInstalledRuntime(platform) {
    if (!fs.existsSync(platform.runtimeDir))
        return;
    console.log('\n==> Cleaning previous runtime files');
    stopRuntimeProcesses(platform);
    for (const filename of knownRuntimeFiles(platform)) {
        const filePath = path.join(platform.runtimeDir, filename);
        if (!fs.existsSync(filePath))
            continue;
        fs.rmSync(filePath, { force: true });
        console.log(`✓ removed ${filePath}`);
    }
}
function stopRuntimeProcesses(platform) {
    if (platform.platform === 'windows') {
        for (const imageName of ['agent_orbd.exe', 'agent-orb-ui.exe']) {
            run('taskkill', ['/F', '/T', '/IM', imageName], {
                allowFailure: true,
            });
        }
        return;
    }
    if (!commandExists('pkill'))
        return;
    for (const processName of ['agent_orbd', 'agent-orb-ui']) {
        run('pkill', ['-x', processName], {
            allowFailure: true,
        });
    }
}
function knownRuntimeFiles(platform) {
    const executableNames = [
        `agent_orb${platform.exeSuffix}`,
        `agent_orbd${platform.exeSuffix}`,
        `agent-orb-ui${platform.exeSuffix}`,
    ];
    return [
        ...executableNames,
        'agent-orb-runtime.json',
        'agent_orb-codex',
        'agent_orb-claude',
        'codex-orb',
        'claude-orb',
        'agent_orb-codex.cmd',
        'agent_orb-claude.cmd',
        'codex-orb.cmd',
        'claude-orb.cmd',
    ];
}
function releaseBaseUrl(platform, options) {
    if (options.releaseDir) {
        return pathToFileURL(path.resolve(options.releaseDir)).href;
    }
    if (options.releaseBaseUrl) {
        return normalizeBase(options.releaseBaseUrl);
    }
    if (process.env.AGENT_ORB_RELEASE_DIR) {
        return pathToFileURL(path.resolve(process.env.AGENT_ORB_RELEASE_DIR)).href;
    }
    const configured = process.env.AGENT_ORB_RELEASE_BASE_URL ?? process.env.AGENT_ORB_RELEASE_URL;
    if (configured?.trim())
        return configured.trim().replace(/\/+$/, '');
    const bundled = bundledReleaseBaseUrl(platform);
    if (bundled)
        return bundled;
    const version = process.env.AGENT_ORB_VERSION ?? 'v0.1.0';
    const repo = githubRepository();
    if (repo)
        return `https://github.com/${repo}/releases/download/${version}`;
    return undefined;
}
function githubRepository() {
    const configured = process.env.AGENT_ORB_GITHUB_REPOSITORY;
    if (configured?.trim())
        return configured.trim();
    const packageRepo = process.env.npm_package_config_github_repository;
    if (packageRepo?.trim())
        return packageRepo.trim();
    const ownPackageRepo = readPackageGithubRepository();
    if (ownPackageRepo)
        return ownPackageRepo;
    return undefined;
}
function readPackageGithubRepository() {
    try {
        const packageJsonPath = fileURLToPath(new URL('../package.json', import.meta.url));
        const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
        const repo = packageJson.config?.github_repository;
        return typeof repo === 'string' && repo.trim() ? repo.trim() : undefined;
    }
    catch {
        return undefined;
    }
}
function bundledReleaseBaseUrl(platform) {
    const releaseDir = fileURLToPath(new URL('../releases/', import.meta.url));
    if (fs.existsSync(path.join(releaseDir, platform.bundleName)) &&
        fs.existsSync(path.join(releaseDir, 'checksums.txt'))) {
        return pathToFileURL(releaseDir).href;
    }
    return undefined;
}
async function downloadFile(url, dest) {
    console.log(`↓ ${url}`);
    if (url.startsWith('file:')) {
        fs.copyFileSync(fileURLToPath(url), dest);
        return;
    }
    const response = await fetch(url);
    if (!response.ok) {
        throw new Error(`Download failed (${response.status} ${response.statusText}): ${url}`);
    }
    const bytes = Buffer.from(await response.arrayBuffer());
    fs.writeFileSync(dest, bytes);
}
function writeInstallManifest(platform, manifest) {
    fs.mkdirSync(platform.runtimeDir, { recursive: true });
    const manifestPath = path.join(platform.runtimeDir, 'agent-orb-runtime.json');
    fs.writeFileSync(manifestPath, `${JSON.stringify({
        install_method: 'npx',
        installed_at: new Date().toISOString(),
        ...manifest,
    }, null, 2)}\n`, 'utf8');
}
function extractBundle(bundlePath, tempDir, platform) {
    const extractDir = path.join(tempDir, 'extract');
    fs.mkdirSync(extractDir, { recursive: true });
    if (platform.platform === 'windows') {
        run('tar', ['-xf', bundlePath, '-C', extractDir]);
    }
    else {
        run('tar', ['-xzf', bundlePath, '-C', extractDir]);
    }
    fs.mkdirSync(platform.runtimeDir, { recursive: true });
    const binDir = findExtractedBinDir(extractDir, platform);
    copyRuntimeFile(binDir, platform.runtimeDir, `agent_orb${platform.exeSuffix}`, platform);
    copyRuntimeFile(binDir, platform.runtimeDir, `agent_orbd${platform.exeSuffix}`, platform);
    copyOptionalRuntimeFile(binDir, platform.runtimeDir, `agent-orb-ui${platform.exeSuffix}`, platform);
}
function findExtractedBinDir(extractDir, platform) {
    const candidates = [
        extractDir,
        path.join(extractDir, 'bin'),
        path.join(extractDir, 'agent-orb'),
        path.join(extractDir, 'agent-orb', 'bin'),
    ];
    for (const candidate of candidates) {
        if (fs.existsSync(path.join(candidate, `agent_orb${platform.exeSuffix}`))) {
            return candidate;
        }
    }
    throw new Error(`Bundle does not contain agent_orb${platform.exeSuffix}`);
}
function copyRuntimeFile(sourceDir, destDir, filename, platform) {
    const source = path.join(sourceDir, filename);
    if (!fs.existsSync(source))
        throw new Error(`Bundle is missing required file: ${filename}`);
    copyFile(source, path.join(destDir, filename), platform);
}
function copyOptionalRuntimeFile(sourceDir, destDir, filename, platform) {
    const source = path.join(sourceDir, filename);
    if (fs.existsSync(source))
        copyFile(source, path.join(destDir, filename), platform);
}
function copyFile(source, dest, platform) {
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
            throw new Error(`Could not replace ${dest}. Stop agent_orbd.exe / agent-orb-ui.exe and rerun npx agent_orb. Original error: ${formatError(error)}`);
        }
        throw error;
    }
}
function joinUrl(base, name) {
    const normalized = normalizeBase(base);
    if (normalized.startsWith('file:')) {
        return new URL(encodeURIComponent(name), ensureTrailingSlash(normalized)).href;
    }
    return `${normalized.replace(/\/+$/, '')}/${encodeURIComponent(name)}`;
}
function formatError(error) {
    return error instanceof Error ? error.message : String(error);
}
function normalizeBase(value) {
    const trimmed = value.trim();
    if (/^https?:\/\//i.test(trimmed) || /^file:\/\//i.test(trimmed)) {
        return trimmed.replace(/\/+$/, '');
    }
    if (fs.existsSync(trimmed)) {
        return pathToFileURL(path.resolve(trimmed)).href;
    }
    return trimmed.replace(/\/+$/, '');
}
function ensureTrailingSlash(value) {
    return value.endsWith('/') ? value : `${value}/`;
}
