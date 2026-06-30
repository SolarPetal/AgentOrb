import { invoke } from '@tauri-apps/api/core';
import type { StatusSnapshot, UiConfig } from './types';

const disconnectedStatus: StatusSnapshot = {
  status: 'disconnected',
  visual: 'disconnected',
  message: 'Agent Orb daemon is disconnected',
};

export async function getStatus(): Promise<StatusSnapshot> {
  try {
    return await invoke<StatusSnapshot>('get_status');
  } catch (error) {
    console.warn('failed to get daemon status', error);
    return disconnectedStatus;
  }
}

export async function clearStatus(): Promise<void> {
  await invoke('clear_status');
}

export async function getConfig(): Promise<UiConfig> {
  return await invoke<UiConfig>('get_config');
}
