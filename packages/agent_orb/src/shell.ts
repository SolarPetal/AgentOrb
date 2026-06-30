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
  const pathEnv = process.env.PATH ?? '';
  const searchDirs = pathEnv.split(process.platform === 'win32' ? ';' : ':').filter(Boolean);
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
