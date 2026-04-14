<script lang="ts">
  import type { Task } from "../types";
  import { app } from "../stores/app.svelte";
  import DateTimePicker from "./DateTimePicker.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { platform } from "@tauri-apps/plugin-os";

  const appWindow = getCurrentWindow();
  const currentPlatform = platform();
  const isDesktop = currentPlatform === "linux" || currentPlatform === "windows";

  let { task, onback, onopen }: { task: Task; onback: () => void; onopen?: (task: Task) => void } = $props();

  let title = $state(task.title);
  let description = $state(task.description);
  let showMenu = $state(false);
  let menuEl = $state<HTMLDivElement | null>(null);
  let showDatePicker = $state(false);
  let saveTimer: ReturnType<typeof setTimeout>;
  let confirmDelete = $state(false);

  $effect(() => {
    return () => clearTimeout(saveTimer);
  });

  let otherLists = $derived(app.lists.filter((l) => l.id !== app.activeListId));

  function handleHeaderMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    if (isDesktop) appWindow.startDragging();
  }

  function debouncedSave(fields: Partial<Task>) {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      app.updateTask({ ...task, ...fields });
    }, 400);
  }

  function handleTitleInput() {
    debouncedSave({ title: title.trim() || task.title });
  }

  function handleDescInput() {
    debouncedSave({ description });
  }

  function handleDateChange(iso: string | null, hasTime: boolean = false) {
    app.updateTask({ ...task, date: iso, has_time: hasTime });
  }

  async function handleToggle() {
    await app.toggleTask(task.id);
    onback();
  }

  function promptDelete() {
    showMenu = false;
    confirmDelete = true;
  }

  async function executeDelete() {
    confirmDelete = false;
    // Cascade: delete subtasks first
    for (const s of subtasks) await app.deleteTask(s.id);
    await app.deleteTask(task.id);
    onback();
  }

  function handleMenuClickOutside(e: MouseEvent) {
    if (showMenu && menuEl && !menuEl.contains(e.target as Node))
      showMenu = false;
  }

  $effect(() => {
    if (showMenu) {
      window.addEventListener("mousedown", handleMenuClickOutside);
      return () => window.removeEventListener("mousedown", handleMenuClickOutside);
    }
  });

  let isCompleted = $derived(task.status === "completed");
  let isSubtask = $derived(!!task.parent_id);
  let subtasks = $derived(app.getSubtasks(task.id));
  let pendingSubtasks = $derived(subtasks.filter(s => s.status !== "completed"));
  let completedSubtasks = $derived(subtasks.filter(s => s.status === "completed"));
  let addingSubtask = $state(false);
  let subtaskTitle = $state("");
  let showSubtaskMenu = $state(false);
  let subtaskMenuEl = $state<HTMLDivElement | null>(null);
  let showCompletedSubtasks = $state(false);
  let completedSubtasksVisible = $state(false);
  let confirmDeleteCompleted = $state(false);

  async function handleAddSubtask() {
    if (!subtaskTitle.trim()) return;
    await app.createTask(subtaskTitle.trim(), undefined, task.id);
    subtaskTitle = "";
  }

  async function executeDeleteCompletedSubtasks() {
    confirmDeleteCompleted = false;
    showSubtaskMenu = false;
    for (const s of completedSubtasks) await app.deleteTask(s.id);
  }

  function handleSubtaskMenuClickOutside(e: MouseEvent) {
    if (showSubtaskMenu && subtaskMenuEl && !subtaskMenuEl.contains(e.target as Node))
      showSubtaskMenu = false;
  }

  $effect(() => {
    if (showSubtaskMenu) {
      window.addEventListener("mousedown", handleSubtaskMenuClickOutside);
      return () => window.removeEventListener("mousedown", handleSubtaskMenuClickOutside);
    }
  });

  function formatDateChip(iso: string): string {
    const d = new Date(iso);
    const today = new Date();
    const dayNames = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const day = dayNames[d.getDay()];
    const pad = (n: number) => String(n).padStart(2, "0");
    const hasTime = task.has_time;
    const timePart = hasTime ? `, ${pad(d.getHours())}:${pad(d.getMinutes())}` : "";
    if (d.toDateString() === today.toDateString()) return `Today${timePart}`;
    return `${day}, ${pad(d.getDate())}/${pad(d.getMonth() + 1)}${timePart}`;
  }
</script>

<!-- Header -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<header
  onmousedown={handleHeaderMouseDown}
  class="flex h-11 items-center border-b border-border-light px-4 dark:border-border-dark"
>
  <button
    onclick={onback}
    class="rounded-lg p-1.5 hover:bg-black/5 dark:hover:bg-white/10"
  >
    <svg class="h-5 w-5 opacity-60" viewBox="0 0 20 20" fill="currentColor">
      <path fill-rule="evenodd" d="M12.79 5.23a.75.75 0 01-.02 1.06L8.832 10l3.938 3.71a.75.75 0 11-1.04 1.08l-4.5-4.25a.75.75 0 010-1.08l4.5-4.25a.75.75 0 011.06.02z" />
    </svg>
  </button>
</header>

<!-- Content -->
<main class="relative flex-1 overflow-y-auto px-4 pt-4" style="padding-bottom: max(2rem, var(--safe-bottom))">
  <!-- Kebab menu -->
  <div class="absolute right-3 top-2" bind:this={menuEl}>
    <button
      onclick={() => (showMenu = !showMenu)}
      class="rounded-lg p-1.5 opacity-50 hover:bg-black/5 hover:opacity-80 dark:hover:bg-white/10"
    >
      <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
        <path d="M10 6a2 2 0 110-4 2 2 0 010 4zM10 12a2 2 0 110-4 2 2 0 010 4zM10 18a2 2 0 110-4 2 2 0 010 4z" />
      </svg>
    </button>
    {#if showMenu}
      <div class="dropdown-menu absolute right-0 top-full z-40 mt-1 min-w-[200px] rounded-lg border border-border-light bg-surface-light py-1 menu-shadow dark:border-border-dark dark:bg-surface-dark">
        <button
          onclick={handleToggle}
          class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-black/5 dark:hover:bg-white/10"
        >
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            {#if isCompleted}
              <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd" />
            {:else}
              <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
            {/if}
          </svg>
          {isCompleted ? "Restore task" : "Mark as completed"}
        </button>
        <button
          onclick={promptDelete}
          class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-danger hover:bg-black/5 dark:hover:bg-white/10"
        >
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
          </svg>
          Delete
        </button>
        {#if otherLists.length > 0}
          <div class="my-1 border-t border-border-light dark:border-border-dark"></div>
          <p class="px-3 py-1.5 text-xs font-medium opacity-40">Move to...</p>
          {#each otherLists as list}
            <button
              onclick={async () => { showMenu = false; await app.moveTask(task.id, list.id); onback(); }}
              class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-black/5 dark:hover:bg-white/10"
            >
              <svg class="h-4 w-4 opacity-40" viewBox="0 0 20 20" fill="currentColor">
                <path d="M3 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h8a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1z" />
              </svg>
              {list.title}
            </button>
          {/each}
        {/if}
      </div>
    {/if}
  </div>
  <!-- Parent task indicator -->
  {#if task.parent_id}
    {@const parent = app.tasks.find(t => t.id === task.parent_id)}
    {#if parent}
      <p class="mb-2 text-xs opacity-40">Subtask of: {parent.title}</p>
    {/if}
  {/if}

  <!-- Title -->
  <input
    type="text"
    bind:value={title}
    oninput={handleTitleInput}
    placeholder="Task title"
    class="w-full bg-transparent text-xl font-bold outline-none placeholder:opacity-30"
  />

  <!-- Description -->
  <div class="mt-4 flex items-start gap-3">
    <svg class="mt-0.5 h-5 w-5 shrink-0 opacity-40" viewBox="0 0 20 20" fill="currentColor">
      <path fill-rule="evenodd" d="M4 4a1 1 0 011-1h10a1 1 0 110 2H5a1 1 0 01-1-1zm0 4a1 1 0 011-1h10a1 1 0 110 2H5a1 1 0 01-1-1zm0 4a1 1 0 011-1h7a1 1 0 110 2H5a1 1 0 01-1-1z" />
    </svg>
    <textarea
      bind:value={description}
      oninput={handleDescInput}
      placeholder="Add details"
      rows="3"
      class="w-full flex-1 resize-none bg-transparent text-sm outline-none placeholder:opacity-40"
    ></textarea>
  </div>

  <!-- Date/time -->
  <div class="mt-4 flex items-center gap-3">
    <svg class="h-5 w-5 shrink-0 opacity-40" viewBox="0 0 20 20" fill="currentColor">
      <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm1-12a1 1 0 10-2 0v4a1 1 0 00.293.707l2.828 2.829a1 1 0 101.415-1.415L11 9.586V6z" clip-rule="evenodd" />
    </svg>
    {#if task.date}
      <div class="flex items-center gap-1.5 rounded-full border border-border-light bg-black/5 px-3 py-1 text-sm dark:border-border-dark dark:bg-white/10">
        <button onclick={() => (showDatePicker = true)} class="hover:opacity-70">
          {formatDateChip(task.date)}
        </button>
        <button onclick={() => handleDateChange(null)} class="opacity-40 hover:opacity-80">
          <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
            <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
          </svg>
        </button>
      </div>
    {:else}
      <button
        onclick={() => (showDatePicker = true)}
        class="text-sm opacity-40 hover:opacity-70"
      >
        Add date/time
      </button>
    {/if}
  </div>

  <!-- Subtasks section (only for top-level tasks) -->
  {#if !isSubtask}
    <div class="mt-6 border-t border-border-light pt-4 dark:border-border-dark">
      <div class="flex items-center gap-2 mb-2">
        <svg class="h-5 w-5 shrink-0 opacity-40" viewBox="0 0 20 20" fill="currentColor">
          <path fill-rule="evenodd" d="M3 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm2 4a1 1 0 011-1h10a1 1 0 110 2H6a1 1 0 01-1-1zm2 4a1 1 0 011-1h8a1 1 0 110 2H8a1 1 0 01-1-1z" />
        </svg>
        <span class="text-sm font-medium opacity-60">Subtasks{subtasks.length > 0 ? ` (${completedSubtasks.length}/${subtasks.length})` : ""}</span>
        <!-- Subtasks kebab menu -->
        {#if completedSubtasks.length > 0}
          <div class="relative ml-auto" bind:this={subtaskMenuEl}>
            <button
              onclick={() => (showSubtaskMenu = !showSubtaskMenu)}
              class="rounded p-1 opacity-40 hover:opacity-70"
            >
              <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                <path d="M10 6a2 2 0 110-4 2 2 0 010 4zM10 12a2 2 0 110-4 2 2 0 010 4zM10 18a2 2 0 110-4 2 2 0 010 4z" />
              </svg>
            </button>
            {#if showSubtaskMenu}
              <div class="dropdown-menu absolute right-0 top-full z-40 mt-1 min-w-[240px] rounded-lg border border-border-light bg-surface-light py-1 menu-shadow dark:border-border-dark dark:bg-surface-dark">
                <button
                  onclick={() => { showSubtaskMenu = false; confirmDeleteCompleted = true; }}
                  class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-danger hover:bg-black/5 dark:hover:bg-white/10"
                >
                  <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                    <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
                  </svg>
                  Delete completed subtasks
                </button>
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Add subtask (top of list) -->
      {#if addingSubtask}
        <div class="flex items-center gap-2 px-2 py-1">
          <div class="h-4 w-4 shrink-0 rounded-full border-2 border-gray-400 dark:border-gray-500"></div>
          <input
            type="text"
            bind:value={subtaskTitle}
            placeholder="Subtask title"
            class="flex-1 bg-transparent text-sm outline-none placeholder:opacity-40"
            onkeydown={(e) => { if (e.key === "Enter") handleAddSubtask(); if (e.key === "Escape") { e.stopPropagation(); addingSubtask = false; subtaskTitle = ""; } }}
            onblur={async () => { if (subtaskTitle.trim()) { await handleAddSubtask(); addingSubtask = false; } else { addingSubtask = false; subtaskTitle = ""; } }}
            autofocus
          />
        </div>
      {:else}
        <button
          onclick={() => (addingSubtask = true)}
          class="flex w-full items-center gap-2 px-2 py-2 text-sm text-primary opacity-60 hover:opacity-100"
        >
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path d="M10 3a1 1 0 011 1v5h5a1 1 0 110 2h-5v5a1 1 0 11-2 0v-5H4a1 1 0 110-2h5V4a1 1 0 011-1z" />
          </svg>
          Add subtask
        </button>
      {/if}

      <!-- Pending subtasks -->
      {#each pendingSubtasks as subtask (subtask.id)}
        <button
          onclick={() => onopen?.(subtask)}
          class="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-black/5 dark:hover:bg-white/10"
        >
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            onclick={(e) => { e.stopPropagation(); app.toggleTask(subtask.id); }}
            class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full border-2 border-gray-400 dark:border-gray-500"
          >
          </div>
          <span class="text-sm">{subtask.title}</span>
        </button>
      {/each}

      <!-- Completed subtasks (collapsible) -->
      {#if completedSubtasks.length > 0}
        <button
          onclick={() => {
            if (showCompletedSubtasks) {
              showCompletedSubtasks = false;
              setTimeout(() => (completedSubtasksVisible = false), 200);
            } else {
              completedSubtasksVisible = true;
              requestAnimationFrame(() => (showCompletedSubtasks = true));
            }
          }}
          class="mt-2 flex w-full items-center gap-2 rounded-lg px-2 py-2 text-sm opacity-50 hover:bg-black/5 dark:hover:bg-white/10"
        >
          <svg
            class="h-3.5 w-3.5 transition-transform {showCompletedSubtasks ? 'rotate-90' : ''}"
            viewBox="0 0 20 20"
            fill="currentColor"
          >
            <path fill-rule="evenodd" d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z" />
          </svg>
          Completed ({completedSubtasks.length})
        </button>
        {#if completedSubtasksVisible}
          <div class="transition-all duration-200 ease-out {showCompletedSubtasks ? 'opacity-100 translate-y-0' : 'opacity-0 -translate-y-2'}">
            {#each completedSubtasks as subtask (subtask.id)}
              <button
                onclick={() => onopen?.(subtask)}
                class="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-black/5 dark:hover:bg-white/10"
              >
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  onclick={(e) => { e.stopPropagation(); app.toggleTask(subtask.id); }}
                  class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full border-2 border-primary bg-primary"
                >
                  <svg class="h-2.5 w-2.5 text-white" viewBox="0 0 20 20" fill="currentColor">
                    <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" />
                  </svg>
                </div>
                <span class="text-sm line-through opacity-50">{subtask.title}</span>
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</main>

<!-- Date picker overlay -->
{#if showDatePicker}
  <DateTimePicker
    value={task.date}
    has_time={task.has_time}
    onchange={handleDateChange}
    onclose={() => (showDatePicker = false)}
  />
{/if}

<!-- Delete confirmation -->
{#if confirmDelete}
  <ConfirmDialog
    message='Delete task "{task.title}"?'
    detail={subtasks.length > 0 ? `This will also delete ${subtasks.length} subtask${subtasks.length === 1 ? '' : 's'}.` : undefined}
    confirmText="Delete"
    danger
    onconfirm={executeDelete}
    oncancel={() => (confirmDelete = false)}
  />
{/if}

<!-- Delete completed subtasks confirmation -->
{#if confirmDeleteCompleted}
  <ConfirmDialog
    message="Delete {completedSubtasks.length} completed subtask{completedSubtasks.length === 1 ? '' : 's'}?"
    confirmText="Delete"
    danger
    onconfirm={executeDeleteCompletedSubtasks}
    oncancel={() => (confirmDeleteCompleted = false)}
  />
{/if}
