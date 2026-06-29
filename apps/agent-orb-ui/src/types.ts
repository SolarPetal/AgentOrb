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
