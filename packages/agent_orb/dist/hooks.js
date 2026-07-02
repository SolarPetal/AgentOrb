import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
// Marker embedded in every hook command Agent Orb installs, so we can find and
// replace our own entries idempotently without touching the user's other hooks.
const HOOK_MARKER = 'agent_orb';
const HOOK_SUBCOMMAND = 'hook';
// Claude Code hook events Agent Orb subscribes to, mapped to orb state by the
// `agent_orb hook` subcommand. See https://code.claude.com/docs/en/hooks
const CLAUDE_HOOK_EVENTS = [
    'SessionStart',
    'UserPromptSubmit',
    'PreToolUse',
    'PostToolUse',
    'Notification',
    'PreCompact',
    'PostCompact',
    'Stop',
];
export function claudeConfigDir() {
    return process.env.CLAUDE_CONFIG_DIR?.trim() || path.join(os.homedir(), '.claude');
}
export function claudeSettingsPath() {
    return path.join(claudeConfigDir(), 'settings.json');
}
/**
 * Merge Agent Orb hooks into the user's Claude Code settings.json.
 *
 * Safety contract:
 * - The original file is copied to a timestamped backup BEFORE any write.
 * - Existing hooks are preserved; only Agent Orb's own entries are refreshed.
 * - Malformed JSON aborts without writing, so we never clobber a file we cannot
 *   parse.
 */
export function installClaudeHooks(agentOrbExe) {
    const settingsPath = claudeSettingsPath();
    const command = hookCommand(agentOrbExe);
    const existing = readSettings(settingsPath);
    const settings = existing.value ?? {};
    const nextHooks = mergeHooks(isRecord(settings.hooks) ? settings.hooks : {}, command);
    settings.hooks = nextHooks;
    // Back up the original file before writing (only when a real file existed).
    let backupPath;
    if (existing.raw !== undefined) {
        backupPath = `${settingsPath}.agent-orb-backup-${timestamp()}`;
        fs.writeFileSync(backupPath, existing.raw, 'utf8');
    }
    fs.mkdirSync(path.dirname(settingsPath), { recursive: true });
    fs.writeFileSync(settingsPath, `${JSON.stringify(settings, null, 2)}\n`, 'utf8');
    return { settingsPath, backupPath, changed: true };
}
/**
 * Remove only Agent Orb's hook entries, leaving the user's other hooks intact.
 * Backs up before writing, same as install.
 */
export function removeClaudeHooks() {
    const settingsPath = claudeSettingsPath();
    const existing = readSettings(settingsPath);
    if (existing.raw === undefined || !existing.value) {
        return { settingsPath, changed: false };
    }
    const settings = existing.value;
    if (!isRecord(settings.hooks)) {
        return { settingsPath, changed: false };
    }
    const cleaned = stripAgentOrbHooks(settings.hooks);
    if (Object.keys(cleaned).length > 0) {
        settings.hooks = cleaned;
    }
    else {
        delete settings.hooks;
    }
    const backupPath = `${settingsPath}.agent-orb-backup-${timestamp()}`;
    fs.writeFileSync(backupPath, existing.raw, 'utf8');
    fs.writeFileSync(settingsPath, `${JSON.stringify(settings, null, 2)}\n`, 'utf8');
    return { settingsPath, backupPath, changed: true };
}
export function claudeHooksInstalled() {
    const existing = readSettings(claudeSettingsPath());
    if (!existing.value || !isRecord(existing.value.hooks))
        return false;
    return Object.values(existing.value.hooks).some((groups) => Array.isArray(groups) && groups.some(isAgentOrbGroup));
}
function hookCommand(agentOrbExe) {
    // Double-quote the path so spaces survive on both cmd.exe and POSIX shells.
    return `"${agentOrbExe}" ${HOOK_SUBCOMMAND} --adapter claude`;
}
function mergeHooks(currentHooks, command) {
    // Start from the user's hooks with any prior Agent Orb entries stripped, so
    // re-running setup is idempotent instead of appending duplicates.
    const merged = stripAgentOrbHooks(currentHooks);
    for (const event of CLAUDE_HOOK_EVENTS) {
        const groups = Array.isArray(merged[event]) ? merged[event] : [];
        groups.push({ hooks: [{ type: 'command', command }] });
        merged[event] = groups;
    }
    return merged;
}
function stripAgentOrbHooks(hooks) {
    const result = {};
    for (const [event, groups] of Object.entries(hooks)) {
        if (!Array.isArray(groups)) {
            result[event] = groups;
            continue;
        }
        const kept = groups.filter((group) => !isAgentOrbGroup(group));
        if (kept.length > 0)
            result[event] = kept;
    }
    return result;
}
function isAgentOrbGroup(group) {
    if (!isRecord(group) || !Array.isArray(group.hooks))
        return false;
    return group.hooks.some((hook) => {
        if (!isRecord(hook))
            return false;
        const command = typeof hook.command === 'string' ? hook.command : '';
        return command.includes(HOOK_MARKER) && command.includes(HOOK_SUBCOMMAND);
    });
}
function readSettings(settingsPath) {
    let raw;
    try {
        raw = fs.readFileSync(settingsPath, 'utf8');
    }
    catch {
        return {};
    }
    const trimmed = raw.replace(/^﻿/, '').trim();
    if (!trimmed)
        return { raw, value: {} };
    try {
        const parsed = JSON.parse(trimmed);
        if (!isRecord(parsed)) {
            throw new Error('settings.json is not a JSON object');
        }
        return { raw, value: parsed };
    }
    catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`Refusing to modify ${settingsPath}: it is not valid JSON (${message}). ` +
            'Fix or move the file, then rerun.');
    }
}
function isRecord(value) {
    return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function timestamp() {
    return new Date().toISOString().replace(/[:.]/g, '-');
}
