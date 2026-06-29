import { clearStatus, getStatus } from './api';
import type { StatusSnapshot, VisualStatus } from './types';

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
  root.innerHTML = `
    <button class="orb-shell" type="button" aria-label="Agent Orb status">
      <span class="orb" aria-hidden="true">
        <span class="orb__glow"></span>
        <span class="orb__ring"></span>
        <span class="orb__core"></span>
      </span>
      <span class="orb-popover" role="status"></span>
    </button>
  `;

  const shell = root.querySelector<HTMLButtonElement>('.orb-shell');
  const popover = root.querySelector<HTMLElement>('.orb-popover');
  if (!shell || !popover) {
    throw new Error('Orb DOM failed to mount');
  }

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

  const poll = async () => {
    const snapshot = await getStatus();
    render(snapshot);
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
  void poll();
  window.setInterval(() => void poll(), POLL_INTERVAL_MS);
}
