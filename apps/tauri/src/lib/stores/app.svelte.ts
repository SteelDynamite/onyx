import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  AppConfig,
  Task,
  TaskList,
  Screen,
  SyncResult,
} from "../types";
import { groupTasksByDate, type TaskGroup } from "../grouping";

// Listen for file system changes from the backend watcher.
listen("fs-changed", () => {
  loadLists();
  // Debounced sync for WebDAV workspaces on local file changes
  if (isSyncedWorkspace) debouncedSync();
});

// ── Reactive state ───────────────────────────────────────────────────

const LS_DECORATIONS_KEY = "windowDecorations";
let windowDecorations = $state<"custom" | "none" | "system">(
  (localStorage.getItem(LS_DECORATIONS_KEY) as "custom" | "none" | "system") ?? "custom"
);
if (windowDecorations === "system") getCurrentWindow().setDecorations(true);
if (windowDecorations === "none") document.documentElement.classList.add("decorations-none");

let screen = $state<Screen>("setup");
let config = $state<AppConfig | null>(null);
let lists = $state<TaskList[]>([]);
let activeListId = $state<string | null>(null);
let tasks = $state<Task[]>([]);
let osDark = globalThis.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
let syncing = $state(false);
let initialSync = $state(false);
let syncStatus = $state<"idle" | "synced" | "error" | "offline">("idle");
let lastSyncResult = $state<SyncResult | null>(null);
let error = $state<string | null>(null);
let missingWorkspace = $state<string | null>(null);
let lastSyncTime = 0;
let _syncInterval: ReturnType<typeof setInterval> | null = null;
let _syncDebounce: ReturnType<typeof setTimeout> | null = null;
let _focusUnlisten: (() => void) | null = null;
const DEFAULT_SYNC_INTERVAL_SECS = 60;
const DEFAULT_SYNC_INTERVAL_UNFOCUSED_SECS = 600;
const SYNC_DEBOUNCE_MS = 5_000;
let _appFocused = true;

// ── Derived ──────────────────────────────────────────────────────────

let activeList = $derived(lists.find((l) => l.id === activeListId) ?? null);
let pendingTasks = $derived(tasks.filter((t) => t.status === "backlog" && !t.parent_id));
let completedTasks = $derived(tasks.filter((t) => t.status === "completed" && !t.parent_id));

let groupedPendingTasks = $derived.by((): TaskGroup[] | null => {
  if (!activeList?.group_by_date) return null;
  return groupTasksByDate(pendingTasks);
});

// Build a map of parent_id -> children for subtask hierarchy
let childrenMap = $derived.by(() => {
  const map = new Map<string, Task[]>();
  for (const t of tasks) {
    if (t.parent_id) {
      const siblings = map.get(t.parent_id);
      if (siblings) siblings.push(t);
      else map.set(t.parent_id, [t]);
    }
  }
  return map;
});

function getSubtasks(parentId: string): Task[] {
  return childrenMap.get(parentId) ?? [];
}
let hasWorkspace = $derived(
  config !== null &&
    config.current_workspace !== null &&
    Object.keys(config.workspaces).length > 0,
);

const DARK_THEMES = new Set(["dark", "nord", "dracula", "solarized", "onyx"]);
let currentTheme = $derived(
  config?.current_workspace
    ? config.workspaces[config.current_workspace]?.theme ?? null
    : null,
);
let isDark = $derived(
  currentTheme ? DARK_THEMES.has(currentTheme) : osDark,
);
let isWebdav = $derived(
  config?.current_workspace
    ? config.workspaces[config.current_workspace]?.mode === "webdav"
    : false,
);
let isGoogleTasks = $derived(
  config?.current_workspace
    ? config.workspaces[config.current_workspace]?.mode === "googletasks"
    : false,
);
let isSyncedWorkspace = $derived(isWebdav || isGoogleTasks);
let syncIntervalSecs = $derived(
  config?.current_workspace
    ? config.workspaces[config.current_workspace]?.sync_interval_secs ?? DEFAULT_SYNC_INTERVAL_SECS
    : DEFAULT_SYNC_INTERVAL_SECS,
);
let syncIntervalUnfocusedSecs = $derived(
  config?.current_workspace
    ? config.workspaces[config.current_workspace]?.sync_interval_unfocused_secs ?? DEFAULT_SYNC_INTERVAL_UNFOCUSED_SECS
    : DEFAULT_SYNC_INTERVAL_UNFOCUSED_SECS,
);

// ── Actions ──────────────────────────────────────────────────────────

async function loadConfig() {
  try {
    config = await invoke<AppConfig>("get_config");
    if (hasWorkspace) {
      // Try loading lists — if the workspace path is gone, get_lists will fail
      lists = [];
      try {
        lists = await invoke<TaskList[]>("get_lists");
      } catch {
        missingWorkspace = config!.current_workspace;
        screen = "missing";
        return;
      }
      if (lists.length > 0 && !activeListId) activeListId = lists[0].id;
      if (activeListId) await loadTasks();
      screen = "tasks";
      if (isSyncedWorkspace) startAutoSync();
    } else {
      screen = "setup";
    }
  } catch (e) {
    config = { workspaces: {}, current_workspace: null };
    screen = "setup";
  }
}

async function addWorkspace(name: string, path: string) {
  try {
    await invoke("init_workspace", { path });
    await invoke("add_workspace", { name, path });
    config = await invoke<AppConfig>("get_config");
    await loadLists();
    invoke("watch_workspace", { path }).catch((e) => console.warn("File watcher failed:", e));
    screen = "tasks";
    error = null;
  } catch (e) {
    error = String(e);
  }
}

async function switchWorkspace(id: string) {
  try {
    await invoke("set_current_workspace", { id });
    config = await invoke<AppConfig>("get_config");
    activeListId = null;
    tasks = [];
    await loadLists();
    const ws = config?.workspaces[id];
    if (ws) invoke("watch_workspace", { path: ws.path }).catch((e) => console.warn("File watcher failed:", e));
    if (isSyncedWorkspace) startAutoSync(); else stopAutoSync();
    error = null;
  } catch (e) {
    error = String(e);
  }
}

async function renameWorkspace(id: string, newName: string) {
  try {
    await invoke("rename_workspace", { id, newName });
    config = await invoke<AppConfig>("get_config");
    error = null;
  } catch (e) {
    error = String(e);
  }
}

async function removeWorkspace(id: string) {
  stopAutoSync();
  try {
    await invoke("remove_workspace", { id });
    config = await invoke<AppConfig>("get_config");
    activeListId = null;
    tasks = [];
    lists = [];
    // Switch to the next available workspace rather than dumping the user
    // to the setup screen when they still have other workspaces.
    const remaining = Object.keys(config?.workspaces ?? {});
    if (remaining.length > 0) {
      await switchWorkspace(remaining[0]);
      screen = "tasks";
    } else {
      screen = "setup";
    }
  } catch (e) {
    error = String(e);
  }
}

async function loadLists() {
  try {
    lists = await invoke<TaskList[]>("get_lists");
    if (lists.length > 0 && !activeListId) {
      activeListId = lists[0].id;
    }
    if (activeListId) await loadTasks();
  } catch (e) {
    error = String(e);
  }
}

async function loadTasks() {
  if (!activeListId) return;
  try {
    const loaded = await invoke<Task[]>("list_tasks", { listId: activeListId });
    // Deduplicate by task ID — sync conflicts can produce files with the same UUID
    const seen = new Set<string>();
    tasks = loaded.filter((t) => {
      if (seen.has(t.id)) return false;
      seen.add(t.id);
      return true;
    });
  } catch (e) {
    error = String(e);
  }
}

async function selectList(id: string) {
  activeListId = id;
  tasks = [];
  await loadTasks();
}

async function createList(name: string) {
  try {
    const list = await invoke<TaskList>("create_list", { name });
    lists = [...lists, list];
    activeListId = list.id;
    tasks = [];
    error = null;
  } catch (e) {
    error = String(e);
  }
}

async function deleteList(id: string) {
  try {
    await invoke("delete_list", { listId: id });
    lists = lists.filter((l) => l.id !== id);
    if (activeListId === id) {
      activeListId = lists.length > 0 ? lists[0].id : null;
      if (activeListId) await loadTasks();
      else tasks = [];
    }
  } catch (e) {
    error = String(e);
  }
}

async function createTask(
  title: string,
  description?: string,
  parentId?: string,
  date?: string | null,
  hasTime?: boolean,
): Promise<Task | null> {
  if (!activeListId) return null;
  try {
    const task = await invoke<Task>("create_task", {
      listId: activeListId,
      title,
      description: description ?? "",
      parentId: parentId ?? null,
      date: date ?? null,
      hasTime: hasTime ?? false,
    });
    tasks = parentId ? [task, ...tasks] : [...tasks, task];
    error = null;
    return task;
  } catch (e) {
    error = String(e);
    return null;
  }
}

async function toggleTask(taskId: string) {
  if (!activeListId) return;
  try {
    const updated = await invoke<Task>("toggle_task", {
      listId: activeListId,
      taskId,
    });
    // Move to top of list locally, then persist order in background
    if (updated.status === "backlog") {
      tasks = [updated, ...tasks.filter((t) => t.id !== taskId)];
      invoke("reorder_task", { listId: activeListId, taskId, newPosition: 0 }).catch((e) => { error = String(e); });
    } else {
      tasks = tasks.map((t) => (t.id === taskId ? updated : t));
    }
  } catch (e) {
    error = String(e);
  }
}

async function updateTask(task: Task) {
  if (!activeListId) return;
  try {
    await invoke("update_task", { listId: activeListId, task });
    tasks = tasks.map((t) => (t.id === task.id ? task : t));
  } catch (e) {
    error = String(e);
  }
}

async function reorderTask(taskId: string, newPosition: number) {
  if (!activeListId) return;
  try {
    await invoke("reorder_task", { listId: activeListId, taskId, newPosition });
    await loadTasks();
  } catch (e) {
    error = String(e);
  }
}

async function deleteTask(taskId: string): Promise<boolean> {
  if (!activeListId) return false;
  try {
    await invoke("delete_task", { listId: activeListId, taskId });
    tasks = tasks.filter((t) => t.id !== taskId);
    return true;
  } catch (e) {
    error = String(e);
    return false;
  }
}

async function moveTask(taskId: string, targetListId: string) {
  if (!activeListId) return;
  try {
    await invoke("move_task", {
      fromListId: activeListId,
      toListId: targetListId,
      taskId,
    });
    tasks = tasks.filter((t) => t.id !== taskId);
  } catch (e) {
    error = String(e);
  }
}

async function renameList(listId: string, newName: string) {
  try {
    await invoke("rename_list", { listId, newName });
    lists = lists.map((l) =>
      l.id === listId ? { ...l, title: newName } : l,
    );
  } catch (e) {
    error = String(e);
  }
}

async function setGroupByDate(listId: string, enabled: boolean) {
  try {
    await invoke("set_group_by_date", { listId, enabled });
    lists = lists.map((l) =>
      l.id === listId ? { ...l, group_by_date: enabled } : l,
    );
    if (listId === activeListId) await loadTasks();
  } catch (e) {
    error = String(e);
  }
}

async function triggerSync() {
  if (!config?.current_workspace || syncing) return;
  syncing = true;
  try {
    const result = isGoogleTasks
      ? await invoke<SyncResult>("sync_google_tasks_workspace", {
          workspaceId: config.current_workspace,
        })
      : await invoke<SyncResult>("sync_workspace", {
          workspaceId: config.current_workspace,
          mode: "full",
        });
    lastSyncResult = result;
    lastSyncTime = Date.now();
    syncStatus = result.errors.length > 0 ? "error" : "synced";
    if (result.errors.length > 0) error = result.errors.join("; ");
    config = await invoke<AppConfig>("get_config");
    await loadLists();
  } catch (e) {
    const msg = String(e);
    // Narrow phrases so that a legitimate server-side error containing a
    // word like "network" or "refused" in its description isn't silently
    // swallowed as an offline blip. Only treat obvious connectivity failures
    // as transient.
    const isTransient = /(^|\W)(timed? out|timeout|connection (refused|reset|timed out|aborted)|connect error|network (is )?unreachable|no route to host|host (not found|is unreachable)|dns|enotfound|econnrefused|etimedout|ehostunreach|enetunreach)(\W|$)/i.test(msg);
    syncStatus = isTransient ? "offline" : "error";
    // Only show the error banner for non-transient failures; connectivity issues just update the status dot
    if (!isTransient) error = msg;
  } finally {
    syncing = false;
  }
}

function debouncedSync() {
  if (_syncDebounce) clearTimeout(_syncDebounce);
  _syncDebounce = setTimeout(() => { _syncDebounce = null; triggerSync(); }, SYNC_DEBOUNCE_MS);
}

function restartSyncInterval() {
  if (_syncInterval) clearInterval(_syncInterval);
  var secs = _appFocused ? syncIntervalSecs : syncIntervalUnfocusedSecs;
  _syncInterval = setInterval(triggerSync, secs * 1000);
}

function startAutoSync() {
  stopAutoSync();
  _appFocused = true;
  triggerSync();
  restartSyncInterval();
  getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    // Sync on re-focus if stale beyond the focused interval
    if (focused && !_appFocused && Date.now() - lastSyncTime > syncIntervalSecs * 1000)
      triggerSync();
    _appFocused = focused;
    restartSyncInterval();
  }).then((unlisten) => {
    if (!_syncInterval) unlisten();
    else _focusUnlisten = unlisten;
  }).catch((e) => {
    console.warn("Failed to set up focus listener:", e);
  });
}

function stopAutoSync() {
  if (_syncInterval) { clearInterval(_syncInterval); _syncInterval = null; }
  if (_syncDebounce) { clearTimeout(_syncDebounce); _syncDebounce = null; }
  if (_focusUnlisten) { _focusUnlisten(); _focusUnlisten = null; }
}

async function setSyncInterval(secs: number | null) {
  if (!config?.current_workspace) return;
  try {
    await invoke("set_sync_interval", {
      workspaceId: config.current_workspace,
      intervalSecs: secs,
    });
    config = await invoke<AppConfig>("get_config");
    if (isSyncedWorkspace) startAutoSync();
  } catch (e) {
    error = String(e);
  }
}

async function setSyncIntervalUnfocused(secs: number | null) {
  if (!config?.current_workspace) return;
  try {
    await invoke("set_sync_interval_unfocused", {
      workspaceId: config.current_workspace,
      intervalSecs: secs,
    });
    config = await invoke<AppConfig>("get_config");
    if (isSyncedWorkspace) startAutoSync();
  } catch (e) {
    error = String(e);
  }
}

function setWindowDecorations(value: "custom" | "none" | "system") {
  windowDecorations = value;
  localStorage.setItem(LS_DECORATIONS_KEY, value);
  getCurrentWindow().setDecorations(value === "system");
  document.documentElement.classList.toggle("decorations-none", value === "none");
}

async function setTheme(theme: string | null) {
  if (!config?.current_workspace) return;
  try {
    await invoke("set_workspace_theme", {
      workspaceId: config.current_workspace,
      theme,
    });
    config = await invoke<AppConfig>("get_config");
  } catch (e) {
    error = String(e);
  }
}

async function addWebdavWorkspace(name: string, webdavUrl: string, webdavPath: string, username: string, password: string) {
  try {
    await invoke("add_webdav_workspace", { name, webdavUrl, webdavPath, username, password });
    config = await invoke<AppConfig>("get_config");
    screen = "tasks";
    error = null;
    // Run initial sync before showing content so the workspace isn't empty
    initialSync = true;
    try {
      await triggerSync();
    } finally {
      initialSync = false;
    }
    await loadLists();
    if (config?.current_workspace) {
      const ws = config.workspaces[config.current_workspace];
      if (ws) invoke("watch_workspace", { path: ws.path }).catch((e) => console.warn("File watcher failed:", e));
    }
    if (isSyncedWorkspace) startAutoSync();
  } catch (e) {
    initialSync = false;
    error = String(e);
  }
}

async function addGoogleTasksWorkspace(
  name: string,
  accessToken: string,
  refreshToken: string,
  account: string,
) {
  try {
    await invoke("add_google_tasks_workspace", { name, accessToken, refreshToken, account });
    config = await invoke<AppConfig>("get_config");
    screen = "tasks";
    error = null;
    await loadLists();
    startAutoSync();
  } catch (e) {
    error = String(e);
  }
}

async function forgetMissingWorkspace() {
  if (!missingWorkspace) return;
  // removeWorkspace handles switching to the next available workspace (or
  // falling back to the setup screen when none remain); just delegate.
  await removeWorkspace(missingWorkspace);
  missingWorkspace = null;
}

function setScreen(s: Screen) {
  screen = s;
}

function clearError() {
  error = null;
}

// ── Exports ──────────────────────────────────────────────────────────

export const app = {
  get screen() {
    return screen;
  },
  get config() {
    return config;
  },
  get lists() {
    return lists;
  },
  get activeListId() {
    return activeListId;
  },
  get activeList() {
    return activeList;
  },
  get tasks() {
    return tasks;
  },
  get pendingTasks() {
    return pendingTasks;
  },
  get groupedPendingTasks() {
    return groupedPendingTasks;
  },
  get completedTasks() {
    return completedTasks;
  },
  get currentTheme() {
    return currentTheme;
  },
  get isDark() {
    return isDark;
  },
  get syncing() {
    return syncing;
  },
  get initialSync() {
    return initialSync;
  },
  get syncStatus() {
    return syncStatus;
  },
  get isWebdav() {
    return isWebdav;
  },
  get isGoogleTasks() {
    return isGoogleTasks;
  },
  get isSyncedWorkspace() {
    return isSyncedWorkspace;
  },
  get syncIntervalSecs() {
    return syncIntervalSecs;
  },
  get syncIntervalUnfocusedSecs() {
    return syncIntervalUnfocusedSecs;
  },
  get lastSyncResult() {
    return lastSyncResult;
  },
  get windowDecorations() {
    return windowDecorations;
  },
  get error() {
    return error;
  },
  get hasWorkspace() {
    return hasWorkspace;
  },
  get missingWorkspace() {
    return missingWorkspace;
  },
  getSubtasks,
  loadConfig,
  addWorkspace,
  switchWorkspace,
  renameWorkspace,
  removeWorkspace,
  loadLists,
  loadTasks,
  selectList,
  createList,
  deleteList,
  createTask,
  toggleTask,
  updateTask,
  reorderTask,
  deleteTask,
  moveTask,
  renameList,
  setGroupByDate,
  triggerSync,
  startAutoSync,
  stopAutoSync,
  setSyncInterval,
  setSyncIntervalUnfocused,
  setWindowDecorations,
  setTheme,
  addWebdavWorkspace,
  addGoogleTasksWorkspace,
  forgetMissingWorkspace,
  setScreen,
  clearError,
};
