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
  const pathEnv = getPathEnv();
  const pathDirs = pathEnv.split(process.platform === 'win32' ? ';' : ':').filter(Boolean);
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
    const normalized = process.platform === 'win32' ? value.trim().toLowerCase() : value.trim();
    if (!normalized || seen.has(normalized)) continue;
    seen.add(normalized);
    result.push(value);
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
  add(process.env.APPDATA ? path.join(process.env.APPDATA, 'npm') : undefined);
  add(process.env.LOCALAPPDATA ? path.join(process.env.LOCALAPPDATA, 'Programs', 'nodejs') : undefined);
  add(path.dirname(process.execPath));
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
  if (path.extname(command)) return [command];

  const pathExt = process.env.PATHEXT ?? '.COM;.EXE;.BAT;.CMD';
  return [
    command,
    ...pathExt
      .split(';')
      .filter(Boolean)
      .map((extension) => `${command}${extension.toLowerCase()}`),
    ...pathExt
      .split(';')
      .filter(Boolean)
      .map((extension) => `${command}${extension.toUpperCase()}`),
  ];
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
