export type InternalStatus =
  | 'disconnected'
  | 'idle'
  | 'starting'
  | 'active'
  | 'silent'
  | 'waiting_input'
  | 'completed'
  | 'failed'
  | 'stuck'
  | 'cancelled';

export type VisualStatus =
  | 'disconnected'
  | 'idle'
  | 'starting'
  | 'blue_spinning'
  | 'purple_spinning'
  | 'yellow_pulse'
  | 'green_done'
  | 'red_error'
  | 'orange_warning'
  | 'cancelled';

export interface StatusSnapshot {
  status: InternalStatus;
  visual: VisualStatus;
  source?: 'codex' | 'claude' | 'generic';
  workspace?: string;
  session_id?: string;
  started_at?: string;
  updated_at?: string;
  message: string;
}

export interface UiConfig {
  daemon: {
    host: string;
    port: number;
  };
  orb: {
    position: string;
    size: number;
    opacity: number;
    always_on_top: boolean;
    click_through: boolean;
  };
  colors: {
    disconnected: string;
    idle: string;
    starting: string;
    active: string;
    thinking_like: string;
    waiting_input: string;
    completed: string;
    error: string;
    warning: string;
  };
  behavior: {
    silent_threshold_seconds: number;
    stuck_threshold_seconds: number;
    completed_hold_seconds: number;
  };
}
