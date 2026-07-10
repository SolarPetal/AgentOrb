import os from 'node:os';

export type PlatformName = 'windows' | 'macos' | 'linux' | 'unsupported';
export type ArchName = 'x64' | 'arm64' | 'unsupported';

export interface PlatformInfo {
  platform: PlatformName;
  arch: ArchName;
  exeSuffix: string;
  pathDelimiter: string;
  runtimeDir: string;
  configDir: string;
  bundleName: string;
}

export function detectPlatform(env = process.env): PlatformInfo {
  const platform = detectPlatformName();
  const arch = detectArchName();
  const exeSuffix = platform === 'windows' ? '.exe' : '';
  const pathDelimiter = platform === 'windows' ? ';' : ':';
  const runtimeDir = defaultRuntimeDir(platform, env);
  const configDir = defaultConfigDir(platform, env);
  const bundleName = `agent-orb-${platform}-${arch}${platform === 'windows' ? '.zip' : '.tar.gz'}`;

  return { platform, arch, exeSuffix, pathDelimiter, runtimeDir, configDir, bundleName };
}

/** Runtime bundles currently produced by `.github/workflows/release.yml`. */
export function hasPrebuiltRuntimeBundle(platform: PlatformName, arch: ArchName): boolean {
  return arch === 'x64' && (platform === 'linux' || platform === 'windows');
}

function detectPlatformName(): PlatformName {
  switch (process.platform) {
    case 'win32':
      return 'windows';
    case 'darwin':
      return 'macos';
    case 'linux':
      return 'linux';
    default:
      return 'unsupported';
  }
}

function detectArchName(): ArchName {
  switch (os.arch()) {
    case 'x64':
      return 'x64';
    case 'arm64':
      return 'arm64';
    default:
      return 'unsupported';
  }
}

function defaultRuntimeDir(platform: PlatformName, env: NodeJS.ProcessEnv): string {
  if (env.AGENT_ORB_BIN_DIR) return env.AGENT_ORB_BIN_DIR;

  if (platform === 'windows') {
    const localAppData = env.LOCALAPPDATA ?? env.USERPROFILE ?? process.cwd();
    return `${localAppData}\\agent-orb\\bin`;
  }
  if (platform === 'macos') {
    const home = env.HOME ?? process.cwd();
    return `${home}/Library/Application Support/agent-orb/bin`;
  }
  const home = env.HOME ?? process.cwd();
  return `${home}/.local/share/agent-orb/bin`;
}

function defaultConfigDir(platform: PlatformName, env: NodeJS.ProcessEnv): string {
  if (env.AGENT_ORB_CONFIG_DIR) return env.AGENT_ORB_CONFIG_DIR;

  if (platform === 'windows') {
    const appData = env.APPDATA ?? env.USERPROFILE ?? process.cwd();
    return `${appData}\\agent-orb`;
  }
  if (platform === 'macos') {
    const home = env.HOME ?? process.cwd();
    return `${home}/Library/Application Support/agent-orb`;
  }
  const xdg = env.XDG_CONFIG_HOME;
  const home = env.HOME ?? process.cwd();
  return `${xdg ?? `${home}/.config`}/agent-orb`;
}
