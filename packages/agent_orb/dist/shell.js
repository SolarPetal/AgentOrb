import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
export function commandExists(command) {
    return findCommand(command) !== undefined;
}
export function findCommand(command) {
    const pathEnv = getPathEnv();
    const searchDirs = pathEnv.split(process.platform === 'win32' ? ';' : ':').filter(Boolean);
    const candidates = process.platform === 'win32'
        ? commandCandidates(command)
        : [command];
    for (const dir of searchDirs) {
        for (const candidate of candidates) {
            const fullPath = path.join(dir, candidate);
            if (isExecutableFile(fullPath))
                return fullPath;
        }
    }
    return undefined;
}
export function getPathEnv() {
    if (process.platform !== 'win32')
        return process.env.PATH ?? '';
    const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path');
    return pathKey ? process.env[pathKey] ?? '' : process.env.PATH ?? '';
}
export function setPathEnv(value) {
    if (process.platform !== 'win32') {
        process.env.PATH = value;
        return;
    }
    const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path') ?? 'Path';
    process.env[pathKey] = value;
    process.env.PATH = value;
}
export function run(command, args, options = {}) {
    const result = spawnSync(command, args, {
        cwd: options.cwd,
        env: options.env,
        stdio: options.stdio ?? 'pipe',
        encoding: 'utf8',
    });
    if (result.error)
        throw result.error;
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
export function spawnDetached(command, args, cwd) {
    const child = spawn(command, args, {
        cwd,
        detached: true,
        stdio: 'ignore',
        windowsHide: true,
    });
    child.unref();
    return child.pid;
}
function commandCandidates(command) {
    if (path.extname(command))
        return [command];
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
function isExecutableFile(filePath) {
    try {
        const stat = fs.statSync(filePath);
        if (!stat.isFile())
            return false;
        if (process.platform === 'win32')
            return true;
        fs.accessSync(filePath, fs.constants.X_OK);
        return true;
    }
    catch {
        return false;
    }
}
