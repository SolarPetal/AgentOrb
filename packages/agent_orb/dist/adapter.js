import { findCommand } from './shell.js';
export const adapters = [
    {
        name: 'codex',
        displayName: 'Codex CLI',
        pathEnvVar: 'AGENT_ORB_CODEX_PATH',
        binaryCandidates: ['codex'],
        wrapperCommand: process.platform === 'win32' ? 'codex-orb.cmd' : 'codex-orb',
        launcherCommand: process.platform === 'win32' ? 'agent_orb-codex.cmd' : 'agent_orb-codex',
        promptPatterns: ['approve', 'permission', 'continue?', 'yes/no'],
    },
    {
        name: 'claude',
        displayName: 'Claude Code CLI',
        pathEnvVar: 'AGENT_ORB_CLAUDE_PATH',
        binaryCandidates: process.platform === 'win32' ? ['claude.exe', 'claude'] : ['claude'],
        wrapperCommand: process.platform === 'win32' ? 'claude-orb.cmd' : 'claude-orb',
        launcherCommand: process.platform === 'win32' ? 'agent_orb-claude.cmd' : 'agent_orb-claude',
        promptPatterns: ['continue?', 'permission', 'press enter', 'approve'],
    },
];
export function detectAdapters() {
    return adapters.map((adapter) => {
        const found = findEnvOverride(adapter) ?? adapter.binaryCandidates
            .map((candidate) => findCommand(candidate))
            .find((candidate) => candidate !== undefined);
        return { ...adapter, foundBinary: found };
    });
}
function findEnvOverride(adapter) {
    const configured = process.env[adapter.pathEnvVar]?.trim();
    if (!configured)
        return undefined;
    return findCommand(configured) ?? configured;
}
