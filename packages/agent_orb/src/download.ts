import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseChecksums, verifyChecksum } from './checksum.js';
import type { PlatformInfo } from './platform.js';
import { commandExists, run } from './shell.js';

export interface BundleInstallOptions {
  force?: boolean;
  releaseBaseUrl?: string;
  releaseDir?: string;
}

export async function installRuntimeBundle(platform: PlatformInfo, options: BundleInstallOptions = {}): Promise<boolean> {
  if (!options.force && runtimeLooksInstalled(platform)) {
    console.log('\n==> Runtime bundle');
    console.log(`✓ existing runtime found at ${platform.runtimeDir}`);
    return true;
  }

  const baseUrl = releaseBaseUrl(platform, options);
  if (!baseUrl) return false;

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
    assertRuntimeInstalled(platform);
    writeInstallManifest(platform, {
      bundle: platform.bundleName,
      sha256: expected,
      source: baseUrl,
    });
    console.log(`✓ installed runtime bundle into ${platform.runtimeDir}`);
    return true;
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

export function runtimeLooksInstalled(platform: PlatformInfo): boolean {
  return requiredRuntimeFiles(platform).every((file) => fs.existsSync(file));
}

function assertRuntimeInstalled(platform: PlatformInfo): void {
  const missing = requiredRuntimeFiles(platform).filter((file) => !fs.existsSync(file));
  if (missing.length === 0) return;

  const installed = fs.existsSync(platform.runtimeDir)
    ? fs.readdirSync(platform.runtimeDir).sort().join(', ')
    : '<runtime dir missing>';
  throw new Error(
    `Runtime install incomplete; missing ${missing.join(', ')}. Installed files: ${installed}`,
  );
}

function requiredRuntimeFiles(platform: PlatformInfo): string[] {
  return ['agent_orb', 'agent_orbd', 'agent-orb-ui'].map((name) => path.join(platform.runtimeDir, `${name}${platform.exeSuffix}`));
}

export function cleanupInstalledRuntime(platform: PlatformInfo): void {
  if (!fs.existsSync(platform.runtimeDir)) return;

  console.log('\n==> Cleaning previous runtime files');
  stopRuntimeProcesses(platform);

  for (const filename of knownRuntimeFiles(platform)) {
    const filePath = path.join(platform.runtimeDir, filename);
    if (!fs.existsSync(filePath)) continue;
    fs.rmSync(filePath, { force: true });
    console.log(`✓ removed ${filePath}`);
  }
}

function stopRuntimeProcesses(platform: PlatformInfo): void {
  if (platform.platform === 'windows') {
    for (const imageName of ['agent_orbd.exe', 'agent-orb-ui.exe']) {
      run('taskkill', ['/F', '/T', '/IM', imageName], {
        allowFailure: true,
      });
    }
    return;
  }

  if (!commandExists('pkill')) return;
  for (const processName of ['agent_orbd', 'agent-orb-ui']) {
    run('pkill', ['-x', processName], {
      allowFailure: true,
    });
  }
}

function knownRuntimeFiles(platform: PlatformInfo): string[] {
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

function releaseBaseUrl(platform: PlatformInfo, options: BundleInstallOptions): string | undefined {
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
  if (configured?.trim()) return configured.trim().replace(/\/+$/, '');

  const bundled = bundledReleaseBaseUrl(platform);
  if (bundled) return bundled;

  const version = process.env.AGENT_ORB_VERSION ?? defaultReleaseVersion() ?? 'v0.1.8';
  const repo = githubRepository();
  if (repo) return `https://github.com/${repo}/releases/download/${version}`;

  return undefined;
}

function defaultReleaseVersion(): string | undefined {
  const version = readPackageVersion();
  if (!version) return undefined;
  return version.startsWith('v') ? version : `v${version}`;
}

function githubRepository(): string | undefined {
  const configured = process.env.AGENT_ORB_GITHUB_REPOSITORY;
  if (configured?.trim()) return configured.trim();

  const packageRepo = process.env.npm_package_config_github_repository;
  if (packageRepo?.trim()) return packageRepo.trim();

  const ownPackageRepo = readPackageGithubRepository();
  if (ownPackageRepo) return ownPackageRepo;

  return undefined;
}

function readPackageGithubRepository(): string | undefined {
  const packageJson = readPackageJson();
  const repo = packageJson?.config?.github_repository;
  return typeof repo === 'string' && repo.trim() ? repo.trim() : undefined;
}

function readPackageVersion(): string | undefined {
  const version = readPackageJson()?.version;
  return typeof version === 'string' && version.trim() ? version.trim() : undefined;
}

function readPackageJson(): { version?: unknown; config?: { github_repository?: unknown } } | undefined {
  try {
    const packageJsonPath = fileURLToPath(new URL('../package.json', import.meta.url));
    return JSON.parse(fs.readFileSync(packageJsonPath, 'utf8')) as {
      version?: unknown;
      config?: { github_repository?: unknown };
    };
  } catch {
    return undefined;
  }
}

function bundledReleaseBaseUrl(platform: PlatformInfo): string | undefined {
  const releaseDir = fileURLToPath(new URL('../releases/', import.meta.url));
  if (
    fs.existsSync(path.join(releaseDir, platform.bundleName)) &&
    fs.existsSync(path.join(releaseDir, 'checksums.txt'))
  ) {
    return pathToFileURL(releaseDir).href;
  }
  return undefined;
}

async function downloadFile(url: string, dest: string): Promise<void> {
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

function writeInstallManifest(
  platform: PlatformInfo,
  manifest: { bundle: string; sha256: string; source: string },
): void {
  fs.mkdirSync(platform.runtimeDir, { recursive: true });
  const manifestPath = path.join(platform.runtimeDir, 'agent-orb-runtime.json');
  fs.writeFileSync(
    manifestPath,
    `${JSON.stringify(
      {
        install_method: 'npx',
        installed_at: new Date().toISOString(),
        ...manifest,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
}

function extractBundle(bundlePath: string, tempDir: string, platform: PlatformInfo): void {
  const extractDir = path.join(tempDir, 'extract');
  fs.mkdirSync(extractDir, { recursive: true });

  if (platform.platform === 'windows') {
    run('tar', ['-xf', bundlePath, '-C', extractDir]);
  } else {
    run('tar', ['-xzf', bundlePath, '-C', extractDir]);
  }

  fs.mkdirSync(platform.runtimeDir, { recursive: true });

  const binDir = findExtractedBinDir(extractDir, platform);
  copyRuntimeFile(binDir, platform.runtimeDir, `agent_orb${platform.exeSuffix}`, platform);
  copyRuntimeFile(binDir, platform.runtimeDir, `agent_orbd${platform.exeSuffix}`, platform);
  copyOptionalRuntimeFile(binDir, platform.runtimeDir, `agent-orb-ui${platform.exeSuffix}`, platform);
}

function findExtractedBinDir(extractDir: string, platform: PlatformInfo): string {
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

function copyRuntimeFile(sourceDir: string, destDir: string, filename: string, platform: PlatformInfo): void {
  const source = path.join(sourceDir, filename);
  if (!fs.existsSync(source)) throw new Error(`Bundle is missing required file: ${filename}`);
  copyFile(source, path.join(destDir, filename), platform);
}

function copyOptionalRuntimeFile(sourceDir: string, destDir: string, filename: string, platform: PlatformInfo): void {
  const source = path.join(sourceDir, filename);
  if (fs.existsSync(source)) copyFile(source, path.join(destDir, filename), platform);
}

function copyFile(source: string, dest: string, platform: PlatformInfo): void {
  const temp = path.join(path.dirname(dest), `.${path.basename(dest)}.${process.pid}.tmp`);
  fs.copyFileSync(source, temp);
  if (platform.platform !== 'windows') fs.chmodSync(temp, 0o755);

  try {
    fs.renameSync(temp, dest);
  } catch (error) {
    fs.rmSync(temp, { force: true });
    if (platform.platform === 'windows') {
      throw new Error(
        `Could not replace ${dest}. Stop agent_orbd.exe / agent-orb-ui.exe and rerun npx agent_orb. Original error: ${formatError(error)}`,
      );
    }
    throw error;
  }
}

function joinUrl(base: string, name: string): string {
  const normalized = normalizeBase(base);
  if (normalized.startsWith('file:')) {
    return new URL(encodeURIComponent(name), ensureTrailingSlash(normalized)).href;
  }
  return `${normalized.replace(/\/+$/, '')}/${encodeURIComponent(name)}`;
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function normalizeBase(value: string): string {
  const trimmed = value.trim();
  if (/^https?:\/\//i.test(trimmed) || /^file:\/\//i.test(trimmed)) {
    return trimmed.replace(/\/+$/, '');
  }
  if (fs.existsSync(trimmed)) {
    return pathToFileURL(path.resolve(trimmed)).href;
  }
  return trimmed.replace(/\/+$/, '');
}

function ensureTrailingSlash(value: string): string {
  return value.endsWith('/') ? value : `${value}/`;
}
