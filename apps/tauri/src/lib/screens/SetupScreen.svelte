<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { app } from "../stores/app.svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { platform } from "@tauri-apps/plugin-os";

  let { cancellable = false }: { cancellable?: boolean } = $props();

  const appWindow = getCurrentWindow();
  const currentPlatform = platform();
  const isDesktop = currentPlatform === "linux" || currentPlatform === "windows";
  const isWindows = currentPlatform === "windows";
  const isMobile = currentPlatform === "android" || currentPlatform === "ios";

  let mode = $state<"local" | "webdav" | null>(isMobile ? "webdav" : null);
  let name = $state("");
  let path = $state("");
  let webdavUrl = $state("");
  let webdavUser = $state("");
  let webdavPass = $state("");
  let testStatus = $state<"idle" | "testing" | "ok" | "fail">("idle");

  async function pickFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected) path = selected as string;
  }

  async function handleCreate() {
    if (!name.trim() || !path.trim()) return;
    const sep = path.includes("\\") ? "\\" : "/";
    const fullPath = path.trimEnd().replace(/[\\/]+$/, "") + sep + name.trim();
    await app.addWorkspace(name.trim(), fullPath);
  }

  async function handleOpen() {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;
    const folder = selected as string;
    const parts = folder.replace(/\\/g, "/").split("/");
    const wsName = parts[parts.length - 1] || "workspace";
    await app.addWorkspace(wsName, folder);
  }

  async function testConnection() {
    testStatus = "testing";
    try {
      await invoke("test_webdav_connection", {
        url: webdavUrl,
        username: webdavUser,
        password: webdavPass,
      });
      testStatus = "ok";
    } catch {
      testStatus = "fail";
    }
  }

  async function handleCreateWebdav() {
    if (!name.trim() || !webdavUrl.trim()) return;
    await app.addWebdavWorkspace(name.trim(), webdavUrl.trim(), webdavUser, webdavPass);
  }

  function handleDrag(e: MouseEvent) {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button, input")) return;
    if (isDesktop) appWindow.startDragging();
  }

  function goBack() {
    mode = null;
    name = "";
    path = "";
    webdavUrl = "";
    webdavUser = "";
    webdavPass = "";
    testStatus = "idle";
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex h-full flex-col" onmousedown={handleDrag}>
  <!-- Title bar area with window controls -->
  <header class="flex h-11 shrink-0 items-center justify-between px-2">
    <div>
      {#if cancellable}
        <button
          onclick={() => app.setScreen("tasks")}
          class="rounded-lg p-1.5 opacity-50 hover:bg-black/10 hover:opacity-80 dark:hover:bg-white/10"
        >
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path fill-rule="evenodd" d="M17 10a.75.75 0 01-.75.75H5.612l4.158 3.96a.75.75 0 11-1.04 1.08l-5.5-5.25a.75.75 0 010-1.08l5.5-5.25a.75.75 0 111.04 1.08L5.612 9.25H16.25A.75.75 0 0117 10z" />
          </svg>
        </button>
      {/if}
    </div>
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

  <div class="flex flex-1 items-center justify-center p-6">
    <div
      class="w-full max-w-sm rounded-2xl bg-card-light p-8 shadow-lg dark:bg-card-dark"
    >
      <h1 class="mb-1 text-2xl font-bold">Onyx</h1>

      {#if mode === null}
        <!-- Step 1: Choose mode -->
        <p class="mb-6 text-sm text-text-secondary-light dark:text-text-secondary-dark">
          How would you like to store your tasks?
        </p>

        <button
          onclick={() => (mode = "local")}
          class="mb-3 w-full rounded-xl border border-border-light p-4 text-left hover:bg-black/5 dark:border-border-dark dark:hover:bg-white/10"
        >
          <p class="text-sm font-semibold">Local Folder</p>
          <p class="mt-0.5 text-xs text-text-secondary-light dark:text-text-secondary-dark">
            Pick a folder on your computer. Files stay local.
          </p>
        </button>

        <button
          onclick={() => (mode = "webdav")}
          class="w-full rounded-xl border border-border-light p-4 text-left hover:bg-black/5 dark:border-border-dark dark:hover:bg-white/10"
        >
          <p class="text-sm font-semibold">WebDAV Server</p>
          <p class="mt-0.5 text-xs text-text-secondary-light dark:text-text-secondary-dark">
            Connect to a WebDAV server. The app manages local files automatically.
          </p>
        </button>

      {:else if mode === "local"}
        <!-- Step 2a: Local workspace -->
        <p class="mb-6 text-sm text-text-secondary-light dark:text-text-secondary-dark">
          Create a new workspace or open an existing one.
        </p>

        <label class="mb-1 block text-sm font-medium">
          Workspace name
          <input
            type="text"
            bind:value={name}
            placeholder="My Tasks"
            class="mt-1 mb-4 w-full rounded-lg border border-border-light bg-transparent px-3 py-2 text-sm font-normal outline-none focus:border-primary dark:border-border-dark"
          />
        </label>

        <!-- svelte-ignore a11y_label_has_associated_control -->
        <label class="mb-1 block text-sm font-medium">Folder</label>
        <div class="mb-6 flex gap-2">
          <input
            type="text"
            bind:value={path}
            readonly
            placeholder="Select a folder..."
            class="min-w-0 flex-1 rounded-lg border border-border-light bg-transparent px-3 py-2 text-sm dark:border-border-dark"
          />
          <button
            onclick={pickFolder}
            class="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-white hover:bg-primary-hover"
          >
            Browse
          </button>
        </div>

        <button
          onclick={handleCreate}
          disabled={!name.trim() || !path.trim()}
          class="w-full rounded-lg bg-primary py-2.5 text-sm font-medium text-white hover:bg-primary-hover disabled:opacity-40"
        >
          Create Workspace
        </button>

        <div class="my-4 flex items-center gap-3">
          <div class="h-px flex-1 bg-border-light dark:bg-border-dark"></div>
          <span class="text-xs opacity-40">or</span>
          <div class="h-px flex-1 bg-border-light dark:bg-border-dark"></div>
        </div>

        <button
          onclick={handleOpen}
          class="mb-3 w-full rounded-lg border border-border-light py-2.5 text-sm font-medium hover:bg-black/5 dark:border-border-dark dark:hover:bg-white/10"
        >
          Open Existing Folder
        </button>

        {#if !isMobile}
          <button
            onclick={goBack}
            class="w-full rounded-lg py-2 text-sm opacity-50 hover:opacity-80"
          >
            Back
          </button>
        {/if}

      {:else}
        <!-- Step 2b: WebDAV workspace -->
        <p class="mb-6 text-sm text-text-secondary-light dark:text-text-secondary-dark">
          Connect to a WebDAV server for cloud-synced tasks.
        </p>

        <label class="mb-1 block text-sm font-medium">
          Workspace name
          <input
            type="text"
            bind:value={name}
            placeholder="My Tasks"
            class="mt-1 mb-4 w-full rounded-lg border border-border-light bg-transparent px-3 py-2 text-sm font-normal outline-none focus:border-primary dark:border-border-dark"
          />
        </label>

        <label class="mb-1 block text-xs font-medium opacity-60">Server URL</label>
        <input
          type="url"
          bind:value={webdavUrl}
          placeholder="https://dav.example.com/tasks/"
          class="mb-3 w-full rounded-lg border border-border-light bg-transparent px-3 py-2 text-sm outline-none focus:border-primary dark:border-border-dark"
        />

        <label class="mb-1 block text-xs font-medium opacity-60">Username</label>
        <input
          type="text"
          bind:value={webdavUser}
          class="mb-3 w-full rounded-lg border border-border-light bg-transparent px-3 py-2 text-sm outline-none focus:border-primary dark:border-border-dark"
        />

        <label class="mb-1 block text-xs font-medium opacity-60">Password</label>
        <input
          type="password"
          bind:value={webdavPass}
          class="mb-4 w-full rounded-lg border border-border-light bg-transparent px-3 py-2 text-sm outline-none focus:border-primary dark:border-border-dark"
        />

        <div class="mb-4 flex gap-2">
          <button
            onclick={testConnection}
            disabled={!webdavUrl.trim()}
            class="rounded-lg border border-border-light px-4 py-2 text-sm font-medium hover:bg-black/5 disabled:opacity-40 dark:border-border-dark dark:hover:bg-white/10"
          >
            {testStatus === "testing" ? "Testing..." : testStatus === "ok" ? "Connected" : testStatus === "fail" ? "Failed -- Retry" : "Test Connection"}
          </button>
        </div>

        <button
          onclick={handleCreateWebdav}
          disabled={!name.trim() || !webdavUrl.trim()}
          class="w-full rounded-lg bg-primary py-2.5 text-sm font-medium text-white hover:bg-primary-hover disabled:opacity-40"
        >
          Create Workspace
        </button>

        {#if !isMobile}
          <button
            onclick={goBack}
            class="mt-3 w-full rounded-lg py-2 text-sm opacity-50 hover:opacity-80"
          >
            Back
          </button>
        {/if}
      {/if}
    </div>
  </div>
</div>
