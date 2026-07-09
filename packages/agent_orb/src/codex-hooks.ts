import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { run } from './shell.js';

// Marker embedded in every hook command Agent Orb installs, so we can find and
// replace our own entries idempotently without touching the user's other hooks.
const HOOK_MARKER = 'agent_orb';
const HOOK_SUBCOMMAND = 'hook';

// Codex CLI lifecycle events Agent Orb subscribes to. Codex only needs a coarse
// running/done signal, so we intentionally register just two events instead of
// Claude's full six-state set:
// - PreToolUse fires when Codex is about to run a tool  -> executing (blue)
// - Stop fires when Codex finishes a turn               -> completed (green)
// The `agent_orb hook --adapter codex` subcommand maps these to orb state.
// See https://developers.openai.com/codex/hooks
const CODEX_HOOK_EVENTS = ['PreToolUse', 'Stop'];

// Experimental feature flag that gates Codex's hooks engine. Off by default;
// without it Codex silently ignores hooks.json. `codex_hooks` is a deprecated
// alias for `hooks`.
const CODEX_HOOKS_FEATURE = 'hooks';

export function codexHome(): string {
  return process.env.CODEX_HOME?.trim() || path.join(os.homedir(), '.codex');
}

export function codexHooksPath(): string {
  return path.join(codexHome(), 'hooks.json');
}

export interface HookInstallResult {
  hooksPath: string;
  backupPath?: string;
  changed: boolean;
}

/**
 * Merge Agent Orb hooks into the user's Codex hooks.json.
 *
 * Safety contract mirrors installClaudeHooks:
 * - The original file is copied to a timestamped backup BEFORE any write.
 * - Existing hooks are preserved; only Agent Orb's own entries are refreshed.
 * - Malformed JSON aborts without writing, so we never clobber a file we cannot
 *   parse.
 */
export function installCodexHooks(agentOrbExe: string): HookInstallResult {
  const hooksPath = codexHooksPath();
  const command = hookCommand(agentOrbExe);

  const existing = readHooksFile(hooksPath);
  const root = existing.value ?? {};

  const nextHooks = mergeHooks(isRecord(root.hooks) ? root.hooks : {}, command);
  root.hooks = nextHooks;

  // Back up the original file before writing (only when a real file existed).
  let backupPath: string | undefined;
  if (existing.raw !== undefined) {
    backupPath = `${hooksPath}.agent-orb-backup-${timestamp()}`;
    fs.writeFileSync(backupPath, existing.raw, 'utf8');
  }

  fs.mkdirSync(path.dirname(hooksPath), { recursive: true });
  fs.writeFileSync(hooksPath, `${JSON.stringify(root, null, 2)}\n`, 'utf8');

  return { hooksPath, backupPath, changed: true };
}

/**
 * Remove only Agent Orb's hook entries, leaving the user's other hooks intact.
 * Backs up before writing, same as install.
 */
export function removeCodexHooks(): HookInstallResult {
  const hooksPath = codexHooksPath();
  const existing = readHooksFile(hooksPath);
  if (existing.raw === undefined || !existing.value) {
    return { hooksPath, changed: false };
  }

  const root = existing.value;
  if (!isRecord(root.hooks)) {
    return { hooksPath, changed: false };
  }

  const cleaned = stripAgentOrbHooks(root.hooks);
  if (Object.keys(cleaned).length > 0) {
    root.hooks = cleaned;
  } else {
    delete root.hooks;
  }

  const backupPath = `${hooksPath}.agent-orb-backup-${timestamp()}`;
  fs.writeFileSync(backupPath, existing.raw, 'utf8');
  fs.writeFileSync(hooksPath, `${JSON.stringify(root, null, 2)}\n`, 'utf8');

  return { hooksPath, backupPath, changed: true };
}

export function codexHooksInstalled(): boolean {
  const existing = readHooksFile(codexHooksPath());
  if (!existing.value || !isRecord(existing.value.hooks)) return false;
  return Object.values(existing.value.hooks).some(
    (groups) => Array.isArray(groups) && groups.some(isAgentOrbGroup),
  );
}

/**
 * Enable Codex's experimental hooks engine through the official CLI, which
 * writes `[features] hooks = true` into $CODEX_HOME/config.toml safely. We shell
 * out instead of hand-editing TOML so we never corrupt the user's config (which
 * may hold `[projects.*]` trust tables). Returns true on success.
 */
export function enableCodexHooksFeature(codexExe: string): boolean {
  try {
    const result = run(codexExe, ['features', 'enable', CODEX_HOOKS_FEATURE], {
      allowFailure: true,
    });
    return result.status === 0;
  } catch {
    return false;
  }
}

function hookCommand(agentOrbExe: string): string {
  // Double-quote the path so spaces survive on both cmd.exe and POSIX shells.
  return `"${agentOrbExe}" ${HOOK_SUBCOMMAND} --adapter codex`;
}

function mergeHooks(
  currentHooks: Record<string, unknown>,
  command: string,
): Record<string, unknown> {
  // Start from the user's hooks with any prior Agent Orb entries stripped, so
  // re-running setup is idempotent instead of appending duplicates.
  const merged = stripAgentOrbHooks(currentHooks);

  for (const event of CODEX_HOOK_EVENTS) {
    const groups = Array.isArray(merged[event]) ? (merged[event] as unknown[]) : [];
    groups.push({ hooks: [{ type: 'command', command }] });
    merged[event] = groups;
  }

  return merged;
}

function stripAgentOrbHooks(hooks: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [event, groups] of Object.entries(hooks)) {
    if (!Array.isArray(groups)) {
      result[event] = groups;
      continue;
    }
    const kept = groups.filter((group) => !isAgentOrbGroup(group));
    if (kept.length > 0) result[event] = kept;
  }
  return result;
}

function isAgentOrbGroup(group: unknown): boolean {
  if (!isRecord(group) || !Array.isArray(group.hooks)) return false;
  return group.hooks.some((hook) => {
    if (!isRecord(hook)) return false;
    const command = typeof hook.command === 'string' ? hook.command : '';
    return command.includes(HOOK_MARKER) && command.includes(HOOK_SUBCOMMAND);
  });
}

interface ReadHooksFile {
  raw?: string;
  value?: Record<string, any>;
}

function readHooksFile(hooksPath: string): ReadHooksFile {
  let raw: string;
  try {
    raw = fs.readFileSync(hooksPath, 'utf8');
  } catch {
    return {};
  }

  const trimmed = raw.replace(/^﻿/, '').trim();
  if (!trimmed) return { raw, value: {} };

  try {
    const parsed = JSON.parse(trimmed);
    if (!isRecord(parsed)) {
      throw new Error('hooks.json is not a JSON object');
    }
    return { raw, value: parsed };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(
      `Refusing to modify ${hooksPath}: it is not valid JSON (${message}). ` +
        'Fix or move the file, then rerun.',
    );
  }
}

function isRecord(value: unknown): value is Record<string, any> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function timestamp(): string {
  return new Date().toISOString().replace(/[:.]/g, '-');
}
