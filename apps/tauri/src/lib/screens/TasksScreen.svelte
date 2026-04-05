<script lang="ts">
  import { app } from "../stores/app.svelte";
  import TaskItem from "../components/TaskItem.svelte";
  import TaskDetailView from "../components/TaskDetailView.svelte";
  import NewTaskInput, { newTaskState } from "../components/NewTaskInput.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import SettingsScreen from "./SettingsScreen.svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { platform } from "@tauri-apps/plugin-os";
  import type { Task } from "../types";

  const appWindow = getCurrentWindow();
  const currentPlatform = platform();
  const isDesktop = currentPlatform === "linux" || currentPlatform === "windows";
  const isWindows = currentPlatform === "windows";

  let taskStack = $state<string[]>([]);
  let parentTask = $derived(taskStack.length >= 1 ? app.tasks.find(t => t.id === taskStack[0]) ?? null : null);
  let subtaskDetail = $derived(taskStack.length >= 2 ? app.tasks.find(t => t.id === taskStack[1]) ?? null : null);

  function openTask(task: Task) {
    taskStack = [task.id];
  }

  function pushTask(task: Task) {
    taskStack = [...taskStack, task.id];
  }

  function closeDetail() {
    if (taskStack.length > 1)
      taskStack = taskStack.slice(0, -1);
    else
      taskStack = [];
  }

  let showDrawer = $state(false);
  let showSettings = $state(false);
  let settingsWorkspace = $state<string | null>(null);
  let showNewList = $state(false);
  let showWorkspacePicker = $state(false);
  let workspacePickerEl = $state<HTMLDivElement | null>(null);

  function handleWindowClick(e: MouseEvent) {
    if (showWorkspacePicker && workspacePickerEl && !workspacePickerEl.contains(e.target as Node))
      showWorkspacePicker = false;
    if (showListMenu && listMenuEl && !listMenuEl.contains(e.target as Node))
      showListMenu = false;
  }

  let newListName = $state("");
  let showCompleted = $state(false);
  let completedVisible = $state(false);
  let renamingListId = $state<string | null>(null);
  let renameValue = $state("");
  let showListMenu = $state(false);
  let listMenuEl = $state<HTMLDivElement | null>(null);
  let confirmDeleteList = $state(false);
  let confirmDeleteCompleted = $state(false);
  let confirmRemoveWorkspace = $state<string | null>(null);
  let dragId = $state<string | null>(null);
  let dragOverId = $state<string | null>(null);
  let resizing = $state(false);
  let resizeTimer: ReturnType<typeof setTimeout>;

  $effect(() => {
    window.addEventListener("mousedown", handleWindowClick);
    const handleResize = () => {
      resizing = true;
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => (resizing = false), 150);
    };
    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("mousedown", handleWindowClick);
      window.removeEventListener("resize", handleResize);
    };
  });

  async function handleNewList() {
    if (!newListName.trim()) return;
    await app.createList(newListName.trim());
    newListName = "";
    showNewList = false;
  }

  function promptDeleteCompleted() {
    showListMenu = false;
    confirmDeleteCompleted = true;
  }

  async function executeDeleteCompleted() {
    confirmDeleteCompleted = false;
    for (var t of app.completedTasks) await app.deleteTask(t.id);
  }

  function promptDeleteList() {
    showListMenu = false;
    confirmDeleteList = true;
  }

  async function executeDeleteList() {
    confirmDeleteList = false;
    if (app.activeListId) await app.deleteList(app.activeListId);
  }

  function startRenameList() {
    showListMenu = false;
    if (!app.activeListId) return;
    var list = app.lists.find(l => l.id === app.activeListId);
    if (!list) return;
    renamingListId = app.activeListId;
    renameValue = list.title;
  }

  async function handleRenameList() {
    if (!renamingListId || !renameValue.trim()) { renamingListId = null; return; }
    var list = app.lists.find(l => l.id === renamingListId);
    if (renameValue.trim() !== list?.title)
      await app.renameList(renamingListId, renameValue.trim());
    renamingListId = null;
  }

  async function handleToggleGroupByDueDate() {
    showListMenu = false;
    if (!app.activeListId) return;
    var list = app.lists.find(l => l.id === app.activeListId);
    if (!list) return;
    await app.setGroupByDueDate(app.activeListId, !list.group_by_due_date);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (showSettings) { showSettings = false; return; }
    if (taskStack.length > 0) { closeDetail(); return; }
    if (showListMenu) { showListMenu = false; return; }
    if (showDrawer) { closeDrawer(); return; }
    if (showWorkspacePicker) { showWorkspacePicker = false; return; }
  }

  function handleDragStart(e: DragEvent, taskId: string) {
    dragId = taskId;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", taskId);
      const el = (e.target as HTMLElement).closest("[draggable]") as HTMLElement;
      if (el) {
        const clone = el.cloneNode(true) as HTMLElement;
        clone.style.width = `${el.offsetWidth}px`;
        clone.style.position = "absolute";
        clone.style.top = "-9999px";
        clone.style.left = "-9999px";
        if (app.isDark) {
          clone.classList.add("dark");
          clone.style.backgroundColor = "var(--color-surface-dark)";
          clone.style.color = "var(--color-text-dark)";
        }
        clone.style.opacity = "0.85";
        clone.style.borderRadius = "8px";
        clone.style.overflow = "hidden";
        clone.style.boxShadow = "0 4px 12px rgba(0,0,0,0.3)";
        document.body.appendChild(clone);
        e.dataTransfer.setDragImage(clone, e.offsetX, e.offsetY);
        requestAnimationFrame(() => clone.remove());
      }
    }
  }

  function handleDragOver(e: DragEvent, taskId: string) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    dragOverId = taskId;
  }

  function handleDragEnd() {
    dragId = null;
    dragOverId = null;
  }

  async function handleDrop(e: DragEvent, targetId: string) {
    e.preventDefault();
    if (!dragId || dragId === targetId) { handleDragEnd(); return; }
    const targetIndex = app.pendingTasks.findIndex((t) => t.id === targetId);
    if (targetIndex >= 0) await app.reorderTask(dragId, targetIndex);
    handleDragEnd();
  }

  function closeDrawer() {
    showDrawer = false;
    showNewList = false;
  }

  function closeSettings() {
    showSettings = false;
    settingsWorkspace = null;
  }

  function handleHeaderMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    if (isDesktop) appWindow.startDragging();
  }

  let workspaceIds = $derived(app.config ? Object.keys(app.config.workspaces).sort((a, b) => (app.config!.workspaces[a].name).localeCompare(app.config!.workspaces[b].name)) : []);
  let translateX = $derived(showDrawer ? '0' : '-80cqi');
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Viewport clip -->
<div class="h-full w-full overflow-hidden">
<!-- Sliding container: left drawer + main content -->
<div
  class="flex h-full ease-out {resizing ? '' : 'transition-transform duration-250'}"
  style="width: calc(100cqi + 80cqi); transform: translateX({translateX})"
>
  <!-- Drawer panel -->
  <div class="flex h-full shrink-0 flex-col bg-surface-light dark:bg-surface-dark" style="width: 80cqi">
    <!-- Drawer header: workspace switcher + settings -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      onmousedown={handleHeaderMouseDown}
      class="flex h-11 shrink-0 items-center justify-between border-b border-border-light px-3 dark:border-border-dark"
    >
      <div class="relative min-w-0 flex-1" bind:this={workspacePickerEl}>
        <button
          onclick={() => (showWorkspacePicker = !showWorkspacePicker)}
          class="flex items-center gap-1.5 rounded-lg px-2 py-1 text-sm font-semibold hover:bg-black/5 dark:hover:bg-white/10"
        >
          <span class="truncate">{app.config?.current_workspace ? app.config.workspaces[app.config.current_workspace]?.name ?? "Workspace" : "Workspace"}</span>
          <svg class="h-3.5 w-3.5 shrink-0 transition-transform {showWorkspacePicker ? 'rotate-180' : ''}" viewBox="0 0 20 20" fill="currentColor">
            <path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" />
          </svg>
        </button>
        {#if showWorkspacePicker}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute left-0 top-full z-40 mt-1 w-full rounded-lg border border-border-light bg-surface-light py-1 shadow-lg dark:border-border-dark dark:bg-surface-dark"
          >
            {#each workspaceIds as wsId}
              {@const ws = app.config?.workspaces[wsId]}
              <div class="group flex items-center px-1 hover:bg-black/5 dark:hover:bg-white/10">
                <button
                  onclick={() => { if (wsId !== app.config?.current_workspace) app.switchWorkspace(wsId); showWorkspacePicker = false; }}
                  class="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left {wsId === app.config?.current_workspace ? 'font-bold' : ''}"
                >
                  {#if wsId === app.config?.current_workspace}
                    <svg class="h-4 w-4 shrink-0 opacity-50" viewBox="0 0 20 20" fill="currentColor">
                      <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" />
                    </svg>
                  {/if}
                  <div class="min-w-0 flex-1">
                    <p class="truncate text-sm">{ws?.name}</p>
                    <p class="truncate text-xs opacity-40">{ws?.mode === "webdav" ? ws.webdav_url ?? "WebDAV" : ws?.path?.replace(/\/[^/]+\/?$/, "") ?? ""}</p>
                  </div>
                </button>
                <button
                  onclick={(e) => { e.stopPropagation(); settingsWorkspace = wsId; showSettings = true; showWorkspacePicker = false; }}
                  class="shrink-0 rounded p-1 opacity-0 transition-opacity group-hover:opacity-40 hover:!opacity-80"
                >
                  <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
                    <path fill-rule="evenodd" d="M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.533 1.533 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.533 1.533 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z" clip-rule="evenodd" />
                  </svg>
                </button>
              </div>
            {/each}
            <div class="mt-1 border-t border-border-light px-1 pt-1 dark:border-border-dark">
              <button
                onclick={() => { showWorkspacePicker = false; app.setScreen("setup"); }}
                class="w-full rounded-md px-2 py-1.5 text-left text-sm text-primary hover:bg-primary/5"
              >
                + Add workspace
              </button>
            </div>
          </div>
        {/if}
      </div>

    </div>

    <!-- List items + new list button -->
    <div class="flex-1 overflow-y-auto py-2">
      {#each app.lists as list (list.id)}
        <button
          onclick={() => { app.selectList(list.id); closeDrawer(); }}
          class="group flex w-full items-center gap-2 px-5 py-2.5 text-left text-sm hover:bg-black/5 dark:hover:bg-white/10 {list.id === app.activeListId ? 'font-bold' : ''}"
        >
          {#if list.id === app.activeListId}
            <svg class="h-4 w-4 shrink-0 opacity-50" viewBox="0 0 20 20" fill="currentColor">
              <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" />
            </svg>
          {/if}
          <span class="flex-1">{list.title}</span>
          <svg class="h-4 w-4 shrink-0 opacity-0 transition-opacity group-hover:opacity-30" viewBox="0 0 20 20" fill="currentColor">
            <path fill-rule="evenodd" d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z" />
          </svg>
        </button>
      {/each}

      <!-- New list inline -->
      <div class="px-2 mt-1">
        {#if showNewList}
          <div class="flex gap-2 px-1">
            <input
              type="text"
              bind:value={newListName}
              placeholder="List name"
              class="min-w-0 flex-1 rounded-lg border border-border-light bg-transparent px-3 py-2 text-sm outline-none focus:border-primary dark:border-border-dark"
              onkeydown={(e) => { if (e.key === "Enter") handleNewList(); if (e.key === "Escape") { showNewList = false; newListName = ""; } }}
            />
            <button
              onclick={handleNewList}
              disabled={!newListName.trim()}
              class="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-white disabled:opacity-40"
            >
              Add
            </button>
          </div>
        {:else}
          <button
            onclick={() => (showNewList = true)}
            class="w-full rounded-lg px-3 py-2.5 text-left text-sm text-primary hover:bg-primary/5"
          >
            + New list
          </button>
        {/if}
      </div>
    </div>

    <!-- Drawer footer: sync status -->
    <div class="shrink-0 px-4 py-2.5">
      {#if app.isWebdav}
        <div class="flex items-center gap-2">
          <!-- Status dot -->
          <span
            class="inline-block h-2 w-2 rounded-full {app.syncing ? 'animate-pulse bg-primary' : app.syncStatus === 'synced' || app.syncStatus === 'idle' ? 'bg-green-500' : app.syncStatus === 'error' ? 'bg-red-500' : 'bg-gray-400'}"
          ></span>
          <span class="flex-1 text-xs opacity-60">
            {app.syncing ? "Syncing..." : app.syncStatus === "synced" || app.syncStatus === "idle" ? "Synced" : app.syncStatus === "error" ? "Sync error" : "Offline"}
          </span>
          <!-- Manual sync button -->
          <button
            onclick={() => app.triggerSync()}
            disabled={app.syncing}
            class="rounded-lg p-1.5 hover:bg-black/5 disabled:opacity-30 dark:hover:bg-white/10"
            title="Sync now"
          >
            <svg class="h-4 w-4" style={app.syncing ? 'animation: spin 1s linear infinite reverse' : ''} viewBox="0 0 20 20" fill="currentColor">
              <path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd" />
            </svg>
          </button>
        </div>
      {:else}
        <span class="text-xs opacity-40">Local workspace</span>
      {/if}
    </div>

  </div>

  <!-- Main content panel -->
  <div class="relative h-full shrink-0 overflow-hidden bg-surface-light dark:bg-surface-dark" style="width: 100cqi">
    <!-- Dim overlay when drawer is open -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="absolute inset-0 z-30 transition-opacity duration-250 ease-out {showDrawer ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'}"
      style="box-shadow: inset 8px 0 24px rgba(0,0,0,0.4); background: rgba(0,0,0,0.4)"
      onclick={closeDrawer}
      onkeydown={(e) => { if (e.key === "Escape") closeDrawer(); }}
    ></div>

    <!-- Sliding inner: task list + detail view -->
    <div
      class="flex h-full {resizing ? '' : 'transition-transform duration-250'} ease-out"
      style="width: 300%; transform: translateX({taskStack.length === 0 ? '0' : taskStack.length === 1 ? '-33.333%' : '-66.666%'})"
    >
      <!-- Sub-panel: Task list -->
      <div class="relative flex h-full w-1/3 flex-col">
        <!-- Header / drag region -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <header
          onmousedown={handleHeaderMouseDown}
          class="relative flex h-11 items-center border-b border-border-light px-4 dark:border-border-dark"
        >
          <!-- Drawer toggle (left) -->
          <button
            onclick={() => (showDrawer = !showDrawer)}
            class="rounded-lg p-1.5 hover:bg-black/5 dark:hover:bg-white/10"
          >
            <svg class="h-5 w-5 opacity-60" viewBox="0 0 20 20" fill="currentColor">
              <path d="M3 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h8a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1z" />
            </svg>
          </button>

          <div class="flex-1"></div>

          <!-- Window controls (right) -->
          {#if isDesktop}
            <div class="flex items-center gap-0.5">
              {#if isWindows}
                <button
                  onclick={() => appWindow.minimize()}
                  class="rounded p-1.5 opacity-50 hover:bg-black/10 hover:opacity-80 dark:hover:bg-white/10"
                >
                  <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
                    <path d="M4 10a1 1 0 011-1h10a1 1 0 110 2H5a1 1 0 01-1-1z" />
                  </svg>
                </button>
              {/if}
              <button
                onclick={() => appWindow.close()}
                class="rounded p-1.5 opacity-50 hover:bg-danger/20 hover:opacity-100 hover:text-danger dark:hover:bg-danger/20"
              >
                <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
                  <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
                </svg>
              </button>
            </div>
          {/if}
        </header>

        <!-- List name + kebab (below header bar, like task detail) -->
        <div class="relative px-4 pt-3 pb-2">
          {#if app.activeListId}
            <!-- Kebab menu -->
            <div class="absolute right-3 top-1" bind:this={listMenuEl}>
              <button
                onclick={() => (showListMenu = !showListMenu)}
                class="rounded-lg p-1.5 opacity-50 hover:bg-black/5 hover:opacity-80 dark:hover:bg-white/10"
              >
                <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
                  <path d="M10 6a2 2 0 110-4 2 2 0 010 4zM10 12a2 2 0 110-4 2 2 0 010 4zM10 18a2 2 0 110-4 2 2 0 010 4z" />
                </svg>
              </button>
              {#if showListMenu}
                <div class="absolute right-0 top-full z-40 mt-1 min-w-[200px] rounded-lg border border-border-light bg-surface-light py-1 shadow-lg dark:border-border-dark dark:bg-surface-dark">
                  <button
                    onclick={startRenameList}
                    class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-black/5 dark:hover:bg-white/10"
                  >
                    <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                      <path d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.38-8.379-2.83-2.828z" />
                    </svg>
                    Rename
                  </button>
                  <button
                    onclick={handleToggleGroupByDueDate}
                    class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-black/5 dark:hover:bg-white/10"
                  >
                    <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                      <path fill-rule="evenodd" d="M6 2a1 1 0 00-1 1v1H4a2 2 0 00-2 2v10a2 2 0 002 2h12a2 2 0 002-2V6a2 2 0 00-2-2h-1V3a1 1 0 10-2 0v1H7V3a1 1 0 00-1-1zm0 5a1 1 0 000 2h8a1 1 0 100-2H6z" clip-rule="evenodd" />
                    </svg>
                    Group by due date
                    {#if app.activeList?.group_by_due_date}
                      <svg class="ml-auto h-4 w-4 text-primary" viewBox="0 0 20 20" fill="currentColor">
                        <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" />
                      </svg>
                    {/if}
                  </button>
                  {#if app.completedTasks.length > 0}
                    <button
                      onclick={promptDeleteCompleted}
                      class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-danger hover:bg-black/5 dark:hover:bg-white/10"
                    >
                      <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                        <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
                      </svg>
                      Delete completed
                    </button>
                  {/if}
                  <button
                    onclick={promptDeleteList}
                    class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-danger hover:bg-black/5 dark:hover:bg-white/10"
                  >
                    <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                      <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
                    </svg>
                    Delete list
                  </button>
                </div>
              {/if}
            </div>
          {/if}
          {#if renamingListId === app.activeListId}
            <input
              type="text"
              bind:value={renameValue}
              class="w-full bg-transparent text-xl font-bold outline-none"
              onkeydown={(e) => { if (e.key === "Enter") handleRenameList(); if (e.key === "Escape") renamingListId = null; }}
              onblur={handleRenameList}
              autofocus
            />
          {:else}
            <p class="text-xl font-bold">{app.activeList?.title ?? "Tasks"}</p>
          {/if}
        </div>

        <!-- Task list -->
        <main class="flex-1 overflow-y-auto">
          {#if app.lists.length === 0}
            <div class="flex h-full flex-col items-center justify-center p-8 text-center">
              <p class="text-lg font-medium opacity-60">No lists yet</p>
              <p class="mt-1 text-sm opacity-40">Tap the list name above to create one</p>
            </div>
          {:else if !app.activeListId}
            <div class="flex h-full items-center justify-center opacity-40">
              Select a list
            </div>
          {:else}
            {#each app.pendingTasks as task (task.id)}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                draggable="true"
                ondragstart={(e) => handleDragStart(e, task.id)}
                ondragover={(e) => handleDragOver(e, task.id)}
                ondragend={handleDragEnd}
                ondrop={(e) => handleDrop(e, task.id)}
                class="{dragId === task.id ? 'opacity-30' : ''} {dragOverId === task.id && dragId !== task.id ? 'border-t-2 border-t-primary' : ''}"
              >
                <TaskItem {task} onopen={openTask} />
              </div>
            {/each}

            {#if app.pendingTasks.length === 0}
              <div class="p-8 text-center text-sm opacity-40">No tasks. Add one below.</div>
            {/if}

            {#if app.completedTasks.length > 0}
              <div class="h-4"></div>
              <button
                onclick={() => {
                  if (showCompleted) {
                    showCompleted = false;
                    setTimeout(() => (completedVisible = false), 300);
                  } else {
                    completedVisible = true;
                    requestAnimationFrame(() => (showCompleted = true));
                  }
                }}
                class="relative z-10 flex w-full items-center justify-between border-t border-border-light bg-surface-light px-4 py-3 text-sm font-medium text-text-secondary-light transition-colors hover:bg-black/5 dark:border-border-dark dark:bg-surface-dark dark:text-text-secondary-dark dark:hover:bg-white/5"
              >
                Completed ({app.completedTasks.length})
                <svg
                  class="h-4 w-4 transition-transform {showCompleted ? 'rotate-90' : ''}"
                  viewBox="0 0 20 20"
                  fill="currentColor"
                >
                  <path
                    fill-rule="evenodd"
                    d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z"
                  />
                </svg>
              </button>
              {#if completedVisible}
                <div class="transition-all duration-300 ease-out {showCompleted ? 'opacity-100 translate-y-0' : 'opacity-0 -translate-y-4'}">
                  {#each app.completedTasks as task (task.id)}
                    <TaskItem {task} onopen={openTask} />
                  {/each}
                </div>
              {/if}
            {/if}
          {/if}
        </main>

        <!-- FAB button -->
        <div
          class="pointer-events-none absolute bottom-6 left-0 right-0 z-20 flex justify-center transition-all duration-250 ease-out {newTaskState.open ? 'opacity-0 scale-75' : ''} {showDrawer || taskStack.length > 0 ? 'translate-y-24 opacity-0' : 'translate-y-0 opacity-100'}"
        >
          <button
            onclick={() => { if (app.activeListId) newTaskState.open = true; }}
            disabled={!app.activeListId}
            class="pointer-events-auto flex h-14 w-14 items-center justify-center rounded-full bg-primary text-white shadow-lg transition-transform hover:scale-105 active:scale-95 disabled:opacity-40 disabled:shadow-none"
          >
            <svg class="h-7 w-7" viewBox="0 0 20 20" fill="currentColor">
              <path d="M10 3a1 1 0 011 1v5h5a1 1 0 110 2h-5v5a1 1 0 11-2 0v-5H4a1 1 0 110-2h5V4a1 1 0 011-1z" />
            </svg>
          </button>
        </div>
      </div>

      <!-- Sub-panel: Task detail -->
      <div class="relative flex h-full w-1/3 flex-col bg-surface-light dark:bg-surface-dark">
        {#if parentTask}
          {#key parentTask.id}
            <TaskDetailView task={parentTask} onback={closeDetail} onopen={pushTask} />
          {/key}
        {/if}
      </div>

      <!-- Sub-panel: Subtask detail -->
      <div class="relative flex h-full w-1/3 flex-col bg-surface-light dark:bg-surface-dark">
        {#if subtaskDetail}
          {#key subtaskDetail.id}
            <TaskDetailView task={subtaskDetail} onback={closeDetail} />
          {/key}
        {/if}
      </div>
    </div>

    <!-- Sync status indicator -->
    {#if app.syncing}
      <div class="absolute bottom-4 right-4 z-20 h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent"></div>
    {:else if app.lastSyncResult}
      <div class="absolute bottom-4 right-4 z-20 flex items-center gap-1 rounded-full bg-black/10 px-2.5 py-1 text-xs opacity-60 dark:bg-white/10">
        <span>↑{app.lastSyncResult.uploaded}</span>
        <span>↓{app.lastSyncResult.downloaded}</span>
      </div>
    {/if}
  </div>
</div>
</div>

<!-- Settings popup overlay -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="absolute inset-0 z-50 flex transition-opacity duration-200 {showSettings ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'}"
  style="padding: 4%"
>
  <!-- Backdrop -->
  <div
    class="absolute inset-0 bg-black/50"
    onclick={closeSettings}
    onkeydown={(e) => { if (e.key === "Escape") closeSettings(); }}
  ></div>
  <!-- Settings card -->
  <div
    class="relative flex h-full w-full flex-col overflow-hidden rounded-2xl bg-surface-light transition-transform duration-200 dark:bg-surface-dark {showSettings ? 'scale-100' : 'scale-95'}"
    style="border: 1px solid rgba(255,255,255,0.1); box-shadow: 0 25px 60px rgba(0,0,0,0.7), 0 10px 20px rgba(0,0,0,0.5)"
  >
    <SettingsScreen onclose={closeSettings} workspaceId={settingsWorkspace ?? app.config?.current_workspace ?? ""} ondelete={(id) => { closeSettings(); confirmRemoveWorkspace = id; }} />
  </div>
</div>

<!-- Toast overlay (outside sliding container so it stays centered) -->
<div class="pointer-events-none absolute inset-0 z-50">
  <NewTaskInput />
</div>

<!-- Delete list confirmation -->
{#if confirmDeleteList}
  <ConfirmDialog
    message='Delete list "{app.activeList?.title}" and all its tasks?'
    confirmText="Delete"
    danger
    onconfirm={executeDeleteList}
    oncancel={() => (confirmDeleteList = false)}
  />
{/if}

<!-- Remove workspace confirmation -->
{#if confirmRemoveWorkspace}
  <ConfirmDialog
    message='Remove workspace "{app.config?.workspaces[confirmRemoveWorkspace]?.name ?? confirmRemoveWorkspace}"?'
    detail="Files remain on disk."
    confirmText="Remove"
    danger
    onconfirm={() => { const id = confirmRemoveWorkspace; confirmRemoveWorkspace = null; if (id) app.removeWorkspace(id); }}
    oncancel={() => (confirmRemoveWorkspace = null)}
  />
{/if}

<!-- Delete completed tasks confirmation -->
{#if confirmDeleteCompleted}
  <ConfirmDialog
    message="Delete {app.completedTasks.length} completed task{app.completedTasks.length === 1 ? '' : 's'}?"
    confirmText="Delete"
    danger
    onconfirm={executeDeleteCompleted}
    oncancel={() => (confirmDeleteCompleted = false)}
  />
{/if}
