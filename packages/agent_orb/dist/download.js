import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseChecksums, verifyChecksum } from './checksum.js';
import { hasPrebuiltRuntimeBundle } from './platform.js';
import { commandExists, run } from './shell.js';
class RuntimeBundleNotFoundError extends Error {
}
export async function installRuntimeBundle(platform, options = {}) {
    if (!options.force && runtimeMatchesExpectedVersion(platform)) {
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
        try {
            await downloadFile(joinUrl(baseUrl, platform.bundleName), bundlePath);
            await downloadFile(joinUrl(baseUrl, 'checksums.txt'), checksumsPath);
        }
        catch (error) {
            if (error instanceof RuntimeBundleNotFoundError) {
                console.log(`· No prebuilt runtime bundle found for ${platform.platform}/${platform.arch}.`);
                return false;
            }
            throw error;
        }
        const checksums = parseChecksums(fs.readFileSync(checksumsPath, 'utf8'));
        const expected = checksums.get(platform.bundleName);
        if (!expected) {
            throw new Error(`checksums.txt does not contain an entry for ${platform.bundleName}`);
        }
        verifyChecksum(bundlePath, expected);
        console.log(`✓ checksum verified: ${platform.bundleName}`);
        cleanupInstalledRuntime(platform);
        extractBundle(bundlePath, tempDir, platform);
        assertRuntimeInstalled(platform);
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
    return requiredRuntimeFiles(platform).every((file) => fs.existsSync(file));
}
/**
 * True only when the installed agent_orb binary exists AND reports the version
 * this bootstrapper expects. A stale binary (files present but older version)
 * returns false so setup/upgrade force a fresh download — otherwise the new
 * setup could register hooks the old binary cannot serve (e.g. the `hook`
 * subcommand), which blocks Claude.
 */
export function runtimeMatchesExpectedVersion(platform) {
    if (!runtimeLooksInstalled(platform))
        return false;
    const expected = readPackageVersion();
    if (!expected)
        return runtimeLooksInstalled(platform);
    const installed = installedRuntimeVersion(platform);
    if (!installed)
        return false;
    return normalizeVersion(installed) === normalizeVersion(expected);
}
export function installedRuntimeVersion(platform) {
    const exe = path.join(platform.runtimeDir, `agent_orb${platform.exeSuffix}`);
    if (!fs.existsSync(exe))
        return undefined;
    try {
        const result = run(exe, ['--version'], { allowFailure: true });
        if (result.status !== 0)
            return undefined;
        // clap prints e.g. "agent_orb 0.1.18"; take the last whitespace token.
        const match = result.stdout.trim().split(/\s+/).pop();
        return match || undefined;
    }
    catch {
        return undefined;
    }
}
/** Whether the installed binary supports a given subcommand (probe its help). */
export function runtimeSupportsSubcommand(platform, subcommand) {
    const exe = path.join(platform.runtimeDir, `agent_orb${platform.exeSuffix}`);
    if (!fs.existsSync(exe))
        return false;
    try {
        const result = run(exe, ['help'], { allowFailure: true });
        const text = `${result.stdout}\n${result.stderr}`;
        return text.includes(subcommand);
    }
    catch {
        return false;
    }
}
function normalizeVersion(value) {
    return value.trim().replace(/^v/i, '');
}
function assertRuntimeInstalled(platform) {
    const missing = requiredRuntimeFiles(platform).filter((file) => !fs.existsSync(file));
    if (missing.length === 0)
        return;
    const installed = fs.existsSync(platform.runtimeDir)
        ? fs.readdirSync(platform.runtimeDir).sort().join(', ')
        : '<runtime dir missing>';
    throw new Error(`Runtime install incomplete; missing ${missing.join(', ')}. Installed files: ${installed}`);
}
function requiredRuntimeFiles(platform) {
    return ['agent_orb', 'agent_orbd', 'agent-orb-ui'].map((name) => path.join(platform.runtimeDir, `${name}${platform.exeSuffix}`));
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
    // Keep the default download path aligned with the assets built in release.yml.
    // Custom release URLs/directories above may still provide additional targets.
    if (!hasPrebuiltRuntimeBundle(platform.platform, platform.arch))
        return undefined;
    const version = process.env.AGENT_ORB_VERSION ?? defaultReleaseVersion() ?? 'v0.1.18';
    const repo = githubRepository();
    if (repo)
        return `https://github.com/${repo}/releases/download/${version}`;
    return undefined;
}
function defaultReleaseVersion() {
    const version = readPackageVersion();
    if (!version)
        return undefined;
    return version.startsWith('v') ? version : `v${version}`;
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
    const packageJson = readPackageJson();
    const repo = packageJson?.config?.github_repository;
    return typeof repo === 'string' && repo.trim() ? repo.trim() : undefined;
}
function readPackageVersion() {
    const version = readPackageJson()?.version;
    return typeof version === 'string' && version.trim() ? version.trim() : undefined;
}
function readPackageJson() {
    try {
        const packageJsonPath = fileURLToPath(new URL('../package.json', import.meta.url));
        return JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
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
        try {
            fs.copyFileSync(fileURLToPath(url), dest);
        }
        catch (error) {
            if (isMissingFileError(error)) {
                throw new RuntimeBundleNotFoundError(`Release asset not found: ${url}`);
            }
            throw error;
        }
        return;
    }
    const response = await fetch(url);
    if (response.status === 404 || response.status === 410) {
        throw new RuntimeBundleNotFoundError(`Release asset not found: ${url}`);
    }
    if (!response.ok) {
        throw new Error(`Download failed (${response.status} ${response.statusText}): ${url}`);
    }
    const bytes = Buffer.from(await response.arrayBuffer());
    fs.writeFileSync(dest, bytes);
}
function isMissingFileError(error) {
    return error instanceof Error && 'code' in error && error.code === 'ENOENT';
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
