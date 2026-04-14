export interface Task {
  id: string;
  title: string;
  description: string;
  status: "backlog" | "completed";
  date: string | null;
  has_time: boolean;
  version: number;
  parent_id: string | null;
}

export interface TaskList {
  id: string;
  title: string;
  tasks: Task[];
  created_at: string;
  updated_at: string;
  group_by_date: boolean;
}

export type WorkspaceMode = "local" | "webdav" | "googletasks";

export interface WorkspaceConfig {
  name: string;
  path: string;
  mode: WorkspaceMode;
  webdav_url: string | null;
  webdav_path: string | null;
  google_account: string | null;
  last_sync: string | null;
  theme: string | null;
  sync_interval_secs: number | null;
  sync_interval_unfocused_secs: number | null;
}

export interface AppConfig {
  workspaces: Record<string, WorkspaceConfig>;
  current_workspace: string | null;
}

export interface SyncResult {
  uploaded: number;
  downloaded: number;
  deleted_local: number;
  deleted_remote: number;
  conflicts: number;
  errors: string[];
}

export type Screen = "setup" | "tasks" | "settings" | "missing";
