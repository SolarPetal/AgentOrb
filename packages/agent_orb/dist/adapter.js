import { findCommand } from './shell.js';
export const adapters = [
    {
        name: 'codex',
        displayName: 'Codex CLI',
        binaryCandidates: process.platform === 'win32' ? ['codex.exe', 'codex'] : ['codex'],
        wrapperCommand: process.platform === 'win32' ? 'codex-orb.cmd' : 'codex-orb',
        promptPatterns: ['approve', 'permission', 'continue?', 'yes/no'],
    },
    {
        name: 'claude',
        displayName: 'Claude Code CLI',
        binaryCandidates: process.platform === 'win32' ? ['claude.exe', 'claude'] : ['claude'],
        wrapperCommand: process.platform === 'win32' ? 'claude-orb.cmd' : 'claude-orb',
        promptPatterns: ['continue?', 'permission', 'press enter', 'approve'],
    },
];
export function detectAdapters() {
    return adapters.map((adapter) => {
        const found = adapter.binaryCandidates
            .map((candidate) => findCommand(candidate))
            .find((candidate) => candidate !== undefined);
        return { ...adapter, foundBinary: found };
    });
}
