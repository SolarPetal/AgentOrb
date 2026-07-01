import { findCommand } from './shell.js';
export const adapters = [
    {
        name: 'codex',
        displayName: 'Codex CLI',
        binaryCandidates: process.platform === 'win32' ? ['codex.exe', 'codex'] : ['codex'],
        wrapperCommand: process.platform === 'win32' ? 'codex-orb.cmd' : 'codex-orb',
        launcherCommand: process.platform === 'win32' ? 'agent_orb-codex.cmd' : 'agent_orb-codex',
        promptPatterns: ['approve', 'permission', 'continue?', 'yes/no'],
    },
    {
        name: 'claude',
        displayName: 'Claude Code CLI',
        binaryCandidates: process.platform === 'win32' ? ['claude.exe', 'claude'] : ['claude'],
        wrapperCommand: process.platform === 'win32' ? 'claude-orb.cmd' : 'claude-orb',
        launcherCommand: process.platform === 'win32' ? 'agent_orb-claude.cmd' : 'agent_orb-claude',
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
