import { clearStatus, getConfig, getStatus, setPanelOpen as setNativePanelOpen } from './api';
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

  const container = document.createElement('div');
  container.className = 'orb-container';

  const shell = document.createElement('button');
  shell.className = 'orb-shell';
  shell.type = 'button';
  shell.setAttribute('aria-label', 'Agent Orb status');
  shell.setAttribute('aria-expanded', 'false');

  const orb = document.createElement('span');
  orb.className = 'orb';
  orb.setAttribute('aria-hidden', 'true');

  const glow = document.createElement('span');
  glow.className = 'orb__glow';
  const haloOuter = document.createElement('span');
  haloOuter.className = 'orb__halo orb__halo--outer';
  const haloInner = document.createElement('span');
  haloInner.className = 'orb__halo orb__halo--inner';
  const ring = document.createElement('span');
  ring.className = 'orb__ring';
  const core = document.createElement('span');
  core.className = 'orb__core';

  const compactHint = document.createElement('span');
  compactHint.className = 'orb-popover';
  compactHint.setAttribute('role', 'status');

  const panel = document.createElement('section');
  panel.className = 'orb-panel';
  panel.setAttribute('aria-label', 'Agent Orb details');
  panel.hidden = true;

  const panelHeader = document.createElement('div');
  panelHeader.className = 'orb-panel__header';

  const panelTitle = document.createElement('div');
  panelTitle.className = 'orb-panel__title';
  panelTitle.textContent = 'Agent Orb';

  const closeButton = document.createElement('button');
  closeButton.type = 'button';
  closeButton.className = 'orb-panel__close';
  closeButton.textContent = '×';
  closeButton.setAttribute('aria-label', 'Collapse Agent Orb panel');

  const statusBadge = document.createElement('div');
  statusBadge.className = 'orb-panel__badge';

  const message = document.createElement('div');
  message.className = 'orb-panel__message';

  const meta = document.createElement('dl');
  meta.className = 'orb-panel__meta';

  const clearButton = document.createElement('button');
  clearButton.type = 'button';
  clearButton.className = 'orb-panel__action';
  clearButton.textContent = 'Clear terminal status';

  orb.append(glow, haloOuter, haloInner, ring, core);
  shell.append(orb, compactHint);
  panelHeader.append(panelTitle, closeButton);
  panel.append(panelHeader, statusBadge, message, meta, clearButton);
  container.append(shell, panel);
  root.append(container);

  let currentStatus: StatusSnapshot | null = null;
  let currentConfig: UiConfig | null = null;
  let panelOpen = false;

  const render = (snapshot: StatusSnapshot) => {
    currentStatus = snapshot;
    const visualClass = visualClassMap[snapshot.visual];
    shell.className = `orb-shell ${visualClass}`;
    panel.className = `orb-panel ${visualClass}`;
    shell.setAttribute(
      'aria-label',
      `Agent Orb status: ${statusText[snapshot.status]}. ${snapshot.message}`,
    );
    shell.title = panelOpen ? 'Collapse Agent Orb details' : 'Open Agent Orb details';

    const compactMeta = [snapshot.source, snapshot.workspace]
      .filter(Boolean)
      .join(' · ');
    compactHint.textContent = compactMeta ? `${snapshot.message}\n${compactMeta}` : snapshot.message;

    statusBadge.textContent = statusText[snapshot.status];
    message.textContent = snapshot.message;
    meta.replaceChildren(
      detailRow('Source', snapshot.source ?? '—'),
      detailRow('Workspace', snapshot.workspace ?? '—'),
      detailRow('Session', snapshot.session_id ?? '—'),
      detailRow('Updated', snapshot.updated_at ?? '—'),
    );
    clearButton.hidden = !(snapshot.status === 'completed' || snapshot.status === 'failed');
  };

  const applyConfig = (config: UiConfig) => {
    currentConfig = config;
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
    void syncPanelWindow(panelOpen);
  };

  const setPanelOpenState = async (open: boolean) => {
    panelOpen = open;
    root.dataset.open = open ? 'true' : 'false';
    shell.setAttribute('aria-expanded', String(open));
    panel.hidden = !open;
    compactHint.hidden = open;
    shell.title = open ? 'Collapse Agent Orb details' : 'Open Agent Orb details';
    await syncPanelWindow(open);
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

  shell.addEventListener('click', () => {
    void setPanelOpenState(!panelOpen);
  });

  closeButton.addEventListener('click', () => {
    void setPanelOpenState(false);
  });

  clearButton.addEventListener('click', async () => {
    if (!currentStatus) return;
    try {
      await clearStatus();
      await poll();
    } catch (error) {
      console.warn('failed to clear daemon status', error);
    }
  });

  render({ status: 'disconnected', visual: 'disconnected', message: 'Connecting to daemon…' });
  void refreshConfig();
  void poll();
  window.setInterval(() => void poll(), POLL_INTERVAL_MS);
  window.setInterval(() => void refreshConfig(), 10_000);
}

function detailRow(label: string, value: string): HTMLElement {
  const row = document.createElement('div');
  row.className = 'orb-panel__row';

  const dt = document.createElement('dt');
  dt.textContent = label;
  const dd = document.createElement('dd');
  dd.textContent = value;

  row.append(dt, dd);
  return row;
}

async function syncPanelWindow(open: boolean): Promise<void> {
  try {
    await setNativePanelOpen(open);
  } catch (error) {
    console.warn('failed to resize Agent Orb window', error);
  }
}
