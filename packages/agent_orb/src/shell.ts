import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

export interface RunOptions {
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  stdio?: 'inherit' | 'pipe';
  allowFailure?: boolean;
}

export interface RunResult {
  status: number;
  stdout: string;
  stderr: string;
}

export function commandExists(command: string): boolean {
  return findCommand(command) !== undefined;
}

export function findCommand(command: string): string | undefined {
  const direct = findDirectCommand(command);
  if (direct) return direct;

  const fromShell = findCommandFromShell(command);
  if (fromShell) return fromShell;

  const pathEnv = getPathEnv();
  const pathDirs = pathEnv
    .split(process.platform === 'win32' ? ';' : ':')
    .map(cleanPathEntry)
    .filter(Boolean);
  const searchDirs = uniquePaths([...pathDirs, ...extraCommandSearchDirs()]);
  const candidates = process.platform === 'win32'
    ? commandCandidates(command)
    : [command];

  for (const dir of searchDirs) {
    for (const candidate of candidates) {
      const fullPath = path.join(dir, candidate);
      if (isExecutableFile(fullPath)) return fullPath;
    }
  }

  return undefined;
}

export function getPathEnv(): string {
  if (process.platform !== 'win32') return process.env.PATH ?? '';

  const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path');
  return pathKey ? process.env[pathKey] ?? '' : process.env.PATH ?? '';
}

export function setPathEnv(value: string): void {
  if (process.platform !== 'win32') {
    process.env.PATH = value;
    return;
  }

  const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path') ?? 'Path';
  process.env[pathKey] = value;
  process.env.PATH = value;
}

export function run(command: string, args: string[], options: RunOptions = {}): RunResult {
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: options.stdio ?? 'pipe',
    encoding: 'utf8',
  });

  if (result.error) throw result.error;
  const status = result.status ?? 1;
  if (status !== 0 && !options.allowFailure) {
    throw new Error(`Command failed (${status}): ${command} ${args.join(' ')}\n${result.stderr ?? ''}`);
  }

  return {
    status,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
  };
}

export function spawnDetached(command: string, args: string[], cwd?: string): number | undefined {
  const child = spawn(command, args, {
    cwd,
    detached: true,
    stdio: 'ignore',
    windowsHide: true,
  });
  child.unref();
  return child.pid;
}

function uniquePaths(values: string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const value of values) {
    const cleaned = cleanPathEntry(value);
    const normalized = process.platform === 'win32' ? cleaned.toLowerCase() : cleaned;
    if (!normalized || seen.has(normalized)) continue;
    seen.add(normalized);
    result.push(cleaned);
  }
  return result;
}

function extraCommandSearchDirs(): string[] {
  if (process.platform !== 'win32') return [];

  const dirs = new Set<string>();
  const add = (value: string | undefined) => {
    if (value?.trim()) dirs.add(value.trim());
  };

  add(process.env.NVM_SYMLINK);
  add(process.env.NVM_HOME);
  add(process.env.npm_config_prefix);
  add(process.env.APPDATA ? path.join(process.env.APPDATA, 'npm') : undefined);
  add(process.env.LOCALAPPDATA ? path.join(process.env.LOCALAPPDATA, 'Programs', 'nodejs') : undefined);
  // Common per-user install targets for globally-installed CLIs on Windows.
  add(process.env.LOCALAPPDATA ? path.join(process.env.LOCALAPPDATA, 'Programs', 'codex') : undefined);
  add(process.env.USERPROFILE ? path.join(process.env.USERPROFILE, '.local', 'bin') : undefined);
  add(process.env.USERPROFILE ? path.join(process.env.USERPROFILE, 'scoop', 'shims') : undefined);
  add(process.env.ProgramData ? path.join(process.env.ProgramData, 'chocolatey', 'bin') : undefined);
  add(process.env.ProgramData ? path.join(process.env.ProgramData, 'scoop', 'shims') : undefined);
  add(path.dirname(process.execPath));
  for (const dir of npmPrefixDirs()) add(dir);
  add('C:\\nvm4w\\nodejs');
  add('C:\\Program Files\\nodejs');
  add('C:\\Program Files (x86)\\nodejs');

  return [...dirs].filter((dir) => {
    try {
      return fs.statSync(dir).isDirectory();
    } catch {
      return false;
    }
  });
}

function commandCandidates(command: string): string[] {
  if (process.platform !== 'win32') return [command];
  if (path.extname(command)) return [command];

  const pathExt = process.env.PATHEXT ?? '.COM;.EXE;.BAT;.CMD';
  return uniqueStrings([
    command,
    `${command}.cmd`,
    `${command}.exe`,
    `${command}.bat`,
    `${command}.com`,
    ...pathExt
      .split(';')
      .filter(Boolean)
      .map((extension) => `${command}${extension.toLowerCase()}`),
    ...pathExt
      .split(';')
      .filter(Boolean)
      .map((extension) => `${command}${extension.toUpperCase()}`),
  ]);
}

function isExecutableFile(filePath: string): boolean {
  try {
    const stat = fs.statSync(filePath);
    if (!stat.isFile()) return false;
    if (process.platform === 'win32') return true;
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function findDirectCommand(command: string): string | undefined {
  if (!hasPathSeparator(command) && !path.isAbsolute(command)) return undefined;

  return commandCandidates(command).find(isExecutableFile);
}

function findCommandFromShell(command: string): string | undefined {
  if (process.platform !== 'win32' || hasPathSeparator(command)) return undefined;

  try {
    const result = spawnSync('where.exe', [command], {
      encoding: 'utf8',
      windowsHide: true,
    });
    if (result.status !== 0) return undefined;

    return (result.stdout ?? '')
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find(isExecutableFile);
  } catch {
    return undefined;
  }
}

function npmPrefixDirs(): string[] {
  const dirs: string[] = [];
  // On Windows the launcher is `npm.cmd`; spawning bare `npm` fails with ENOENT,
  // which previously hid every npm-global install location from detection.
  const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
  try {
    const result = spawnSync(npmCommand, ['config', 'get', 'prefix'], {
      encoding: 'utf8',
      windowsHide: true,
      shell: process.platform === 'win32',
    });
    const prefix = result.status === 0 ? result.stdout.trim() : '';
    if (prefix && !prefix.startsWith('undefined')) {
      dirs.push(prefix);
      dirs.push(path.join(prefix, 'bin'));
    }
  } catch {
    // npm may not be on PATH in source-build scenarios; PATH scanning still runs.
  }
  try {
    const result = spawnSync(npmCommand, ['root', '-g'], {
      encoding: 'utf8',
      windowsHide: true,
      shell: process.platform === 'win32',
    });
    const root = result.status === 0 ? result.stdout.trim() : '';
    // `npm root -g` returns <prefix>/node_modules; global bins live next to it.
    if (root && !root.startsWith('undefined')) {
      dirs.push(path.dirname(root));
    }
  } catch {
    // Non-fatal; other search dirs still apply.
  }
  return dirs;
}

function cleanPathEntry(value: string): string {
  let cleaned = value.trim();
  if (process.platform === 'win32') {
    cleaned = cleaned.replace(/^"+|"+$/g, '');
    cleaned = cleaned.replace(/%([^%]+)%/g, (_, name: string) => process.env[name] ?? `%${name}%`);
  }
  return cleaned;
}

function hasPathSeparator(value: string): boolean {
  return value.includes('/') || value.includes('\\');
}

function uniqueStrings(values: string[]): string[] {
  return [...new Set(values)];
}
