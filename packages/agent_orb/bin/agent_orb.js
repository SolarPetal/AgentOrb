#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const packageDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const entry = path.join(packageDir, 'dist', 'index.js');

if (!existsSync(entry)) {
  console.error('[agent_orb] bootstrapper build output is missing; building it now...');
  const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';

  const install = spawnSync(npm, ['install'], {
    cwd: packageDir,
    stdio: 'inherit',
  });
  if (install.status !== 0) process.exit(install.status ?? 1);

  const build = spawnSync(npm, ['run', 'build'], {
    cwd: packageDir,
    stdio: 'inherit',
  });
  if (build.status !== 0) process.exit(build.status ?? 1);
}

await import(pathToFileURL(entry).href);
