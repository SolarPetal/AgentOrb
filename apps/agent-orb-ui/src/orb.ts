import { clearStatus, getConfig, getStatus } from './api';
import type { StatusSnapshot, UiConfig, VisualStatus } from './types';

const POLL_INTERVAL_MS = 1000;

const visualClassMap: Record<VisualStatus, string> = {
  disconnected: 'is-disconnected',
  idle: 'is-idle',
  starting: 'is-starting',
  blue_spinning: 'is-active',
  purple_spinning: 'is-silent',
  yellow_pulse: 'is-waiting-input',
  green_done: 'is-completed',
  red_error: 'is-failed',
  orange_warning: 'is-stuck',
  cancelled: 'is-cancelled',
};

const statusText: Record<StatusSnapshot['status'], string> = {
  disconnected: 'Disconnected',
  idle: 'Idle',
  starting: 'Starting',
  active: 'Active',
  silent: 'Silent',
  waiting_input: 'Waiting',
  completed: 'Done',
  failed: 'Failed',
  stuck: 'Stuck',
  cancelled: 'Cancelled',
};

export function mountOrb(root: HTMLElement): void {
  root.replaceChildren();

  const shell = document.createElement('button');
  shell.className = 'orb-shell';
  shell.type = 'button';
  shell.setAttribute('aria-label', 'Agent Orb status');

  const orb = document.createElement('span');
  orb.className = 'orb';
  orb.setAttribute('aria-hidden', 'true');

  const glow = document.createElement('span');
  glow.className = 'orb__glow';
  const ring = document.createElement('span');
  ring.className = 'orb__ring';
  const core = document.createElement('span');
  core.className = 'orb__core';

  const popover = document.createElement('span');
  popover.className = 'orb-popover';
  popover.setAttribute('role', 'status');

  orb.append(glow, ring, core);
  shell.append(orb, popover);
  root.append(shell);

  let currentStatus: StatusSnapshot | null = null;

  const render = (snapshot: StatusSnapshot) => {
    currentStatus = snapshot;
    shell.className = `orb-shell ${visualClassMap[snapshot.visual]}`;
    shell.setAttribute(
      'aria-label',
      `Agent Orb status: ${statusText[snapshot.status]}. ${snapshot.message}`,
    );
    shell.title = snapshot.message;

    const meta = [snapshot.source, snapshot.workspace]
      .filter(Boolean)
      .join(' · ');
    popover.textContent = meta ? `${snapshot.message}\n${meta}` : snapshot.message;
  };

  const applyConfig = (config: UiConfig) => {
    const size = `${config.orb.size}px`;
    const coreInset = `${Math.max(6, Math.round(config.orb.size * 0.22))}px`;
    root.style.setProperty('--orb-size', size);
    root.style.setProperty('--orb-core-inset', coreInset);
    root.style.setProperty('--orb-opacity', String(config.orb.opacity));
    root.style.setProperty('--color-disconnected', config.colors.disconnected);
    root.style.setProperty('--color-idle', config.colors.idle);
    root.style.setProperty('--color-starting', config.colors.starting);
    root.style.setProperty('--color-active', config.colors.active);
    root.style.setProperty('--color-silent', config.colors.thinking_like);
    root.style.setProperty('--color-waiting-input', config.colors.waiting_input);
    root.style.setProperty('--color-completed', config.colors.completed);
    root.style.setProperty('--color-failed', config.colors.error);
    root.style.setProperty('--color-stuck', config.colors.warning);
    root.dataset.position = config.orb.position;
  };

  const poll = async () => {
    const snapshot = await getStatus();
    render(snapshot);
  };

  const refreshConfig = async () => {
    try {
      applyConfig(await getConfig());
    } catch (error) {
      console.warn('failed to get Agent Orb config', error);
    }
  };

  shell.addEventListener('click', async () => {
    if (!currentStatus) {
      return;
    }

    if (currentStatus.status === 'completed' || currentStatus.status === 'failed') {
      try {
        await clearStatus();
        await poll();
      } catch (error) {
        console.warn('failed to clear daemon status', error);
      }
    }
  });

  render({ status: 'disconnected', visual: 'disconnected', message: 'Connecting to daemon…' });
  void refreshConfig();
  void poll();
  window.setInterval(() => void poll(), POLL_INTERVAL_MS);
  window.setInterval(() => void refreshConfig(), 10_000);
}
