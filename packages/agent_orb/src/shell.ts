import { spawn, spawnSync } from 'node:child_process';

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
  const checker = process.platform === 'win32' ? 'where' : 'command';
  const args = process.platform === 'win32' ? [command] : ['-v', command];
  const result = process.platform === 'win32'
    ? spawnSync(checker, args, { encoding: 'utf8' })
    : spawnSync('sh', ['-lc', `${checker} ${quote(command)}`], { encoding: 'utf8' });

  if (result.status !== 0) return undefined;
  const firstLine = (result.stdout ?? '').split(/\r?\n/).find((line) => line.trim().length > 0);
  return firstLine?.trim();
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

function quote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}
