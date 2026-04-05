import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppConfig,
  Task,
  TaskList,
  Screen,
  SyncResult,
} from "../types";

// Listen for file system changes from the backend watcher.
listen("fs-changed", () => {
  loadLists();
});

// ── Reactive state ───────────────────────────────────────────────────

let screen = $state<Screen>("setup");
let config = $state<AppConfig | null>(null);
let lists = $state<TaskList[]>([]);
let activeListId = $state<string | null>(null);
let tasks = $state<Task[]>([]);
let osDark = globalThis.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
let syncing = $state(false);
let syncMode = $state<"full" | "push" | "pull">("full");
let lastSyncResult = $state<SyncResult | null>(null);
let error = $state<string | null>(null);
let missingWorkspace = $state<string | null>(null);

// ── Derived ──────────────────────────────────────────────────────────

let activeList = $derived(lists.find((l) => l.id === activeListId) ?? null);
let pendingTasks = $derived(tasks.filter((t) => t.status === "backlog" && !t.parent_id));
let completedTasks = $derived(tasks.filter((t) => t.status === "completed" && !t.parent_id));

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

const DARK_THEMES = new Set(["dark", "nord", "dracula", "solarized"]);
let currentTheme = $derived(
  config?.current_workspace
    ? config.workspaces[config.current_workspace]?.theme ?? null
    : null,
);
let isDark = $derived(
  currentTheme ? DARK_THEMES.has(currentTheme) : osDark,
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

async function switchWorkspace(name: string) {
  try {
    await invoke("set_current_workspace", { name });
    config = await invoke<AppConfig>("get_config");
    activeListId = null;
    await loadLists();
    const ws = config?.workspaces[name];
    if (ws) invoke("watch_workspace", { path: ws.path }).catch((e) => console.warn("File watcher failed:", e));
    error = null;
  } catch (e) {
    error = String(e);
  }
}

async function renameWorkspace(oldName: string, newName: string) {
  try {
    await invoke("rename_workspace", { oldName, newName });
    config = await invoke<AppConfig>("get_config");
    error = null;
  } catch (e) {
    error = String(e);
  }
}

async function removeWorkspace(name: string) {
  try {
    await invoke("remove_workspace", { name });
    config = await invoke<AppConfig>("get_config");
    if (!hasWorkspace) {
      screen = "setup";
      lists = [];
      tasks = [];
      activeListId = null;
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
    tasks = await invoke<Task[]>("list_tasks", { listId: activeListId });
  } catch (e) {
    error = String(e);
  }
}

async function selectList(id: string) {
  activeListId = id;
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

async function createTask(title: string, description?: string, parentId?: string): Promise<Task | null> {
  if (!activeListId) return null;
  try {
    const task = await invoke<Task>("create_task", {
      listId: activeListId,
      title,
      description: description ?? "",
      parentId: parentId ?? null,
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

async function deleteTask(taskId: string) {
  if (!activeListId) return;
  try {
    await invoke("delete_task", { listId: activeListId, taskId });
    tasks = tasks.filter((t) => t.id !== taskId);
  } catch (e) {
    error = String(e);
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

async function setGroupByDueDate(listId: string, enabled: boolean) {
  try {
    await invoke("set_group_by_due_date", { listId, enabled });
    lists = lists.map((l) =>
      l.id === listId ? { ...l, group_by_due_date: enabled } : l,
    );
    if (listId === activeListId) await loadTasks();
  } catch (e) {
    error = String(e);
  }
}

async function triggerSync() {
  if (!config?.current_workspace) return;
  syncing = true;
  error = null;
  try {
    const result = await invoke<SyncResult>("sync_workspace", {
      workspaceName: config.current_workspace,
      mode: syncMode,
    });
    lastSyncResult = result;
    if (result.errors.length > 0) {
      error = result.errors.join("; ");
    }
    config = await invoke<AppConfig>("get_config");
    await loadLists();
  } catch (e) {
    error = String(e);
  } finally {
    syncing = false;
  }
}

function setSyncMode(mode: "full" | "push" | "pull") {
  syncMode = mode;
}

async function setTheme(theme: string | null) {
  if (!config?.current_workspace) return;
  try {
    await invoke("set_workspace_theme", {
      workspaceName: config.current_workspace,
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
    await loadLists();
    const ws = config?.workspaces[name];
    if (ws) invoke("watch_workspace", { path: ws.path }).catch((e) => console.warn("File watcher failed:", e));
    screen = "tasks";
    error = null;
  } catch (e) {
    error = String(e);
  }
}

async function forgetMissingWorkspace() {
  if (!missingWorkspace) return;
  await removeWorkspace(missingWorkspace);
  missingWorkspace = null;
  config = await invoke<AppConfig>("get_config");
  if (hasWorkspace) {
    // Switch to the next available workspace
    const nextName = Object.keys(config!.workspaces)[0];
    if (nextName) {
      await switchWorkspace(nextName);
      screen = "tasks";
      return;
    }
  }
  screen = "setup";
  lists = [];
  tasks = [];
  activeListId = null;
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
  get syncMode() {
    return syncMode;
  },
  get lastSyncResult() {
    return lastSyncResult;
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
  setGroupByDueDate,
  triggerSync,
  setSyncMode,
  setTheme,
  addWebdavWorkspace,
  forgetMissingWorkspace,
  setScreen,
  clearError,
};
