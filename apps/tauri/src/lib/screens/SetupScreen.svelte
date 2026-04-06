<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { documentDir } from "@tauri-apps/api/path";
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

  // ── Shared state ──────────────────────────────────────────────────
  let mode = $state<"local" | "webdav" | null>(isMobile ? "webdav" : null);
  let name = $state("Onyx");
  let path = $state("");

  documentDir().then((d) => { path = d; }).catch(() => {});

  // ── WebDAV state ──────────────────────────────────────────────────
  let webdavUrl = $state("");
  let webdavUser = $state("");
  let webdavPass = $state("");
  let testStatus = $state<"idle" | "testing" | "ok" | "fail">("idle");

  // WebDAV step: "connect" → "browse" → "preview" | "create"
  let webdavStep = $state<"connect" | "browse" | "preview" | "create">("connect");
  let browsePath = $state<string[]>([]); // stack of folder names for navigation
  let browseLoading = $state(false);
  let browseEntries = $state<{ name: string; is_workspace: boolean }[]>([]);
  let browseError = $state<string | null>(null);

  // Workspace preview state
  let previewName = $state("");
  let previewLists = $state<{ name: string; task_count: number }[]>([]);
  let previewLoading = $state(false);

  // Create workspace state
  let createName = $state("Onyx");
  let creating = $state(false);

  // ── Derived ───────────────────────────────────────────────────────
  let currentBrowsePath = $derived(browsePath.join("/"));

  // ── Local workspace handlers ──────────────────────────────────────

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

  // ── WebDAV handlers ───────────────────────────────────────────────

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

  async function connectAndBrowse() {
    testStatus = "testing";
    try {
      await invoke("test_webdav_connection", {
        url: webdavUrl,
        username: webdavUser,
        password: webdavPass,
      });
      testStatus = "ok";
      webdavStep = "browse";
      browsePath = [];
      await loadFolder();
    } catch {
      testStatus = "fail";
    }
  }

  async function loadFolder() {
    browseLoading = true;
    browseError = null;
    try {
      const entries: typeof browseEntries = await invoke("list_remote_folder", {
        url: webdavUrl,
        username: webdavUser,
        password: webdavPass,
        path: currentBrowsePath,
      });
      entries.sort((a, b) => (a.is_workspace === b.is_workspace ? 0 : a.is_workspace ? -1 : 1));
      browseEntries = entries;
    } catch (e) {
      browseError = String(e);
      browseEntries = [];
    } finally {
      browseLoading = false;
    }
  }

  async function navigateInto(folder: { name: string; is_workspace: boolean }) {
    if (folder.is_workspace) {
      previewName = folder.name;
      previewLoading = true;
      webdavStep = "preview";
      try {
        const wsPath = currentBrowsePath
          ? `${currentBrowsePath}/${folder.name}`
          : folder.name;
        previewLists = await invoke("inspect_remote_workspace", {
          url: webdavUrl,
          username: webdavUser,
          password: webdavPass,
          path: wsPath,
        });
      } catch (e) {
        browseError = String(e);
        webdavStep = "browse";
      } finally {
        previewLoading = false;
      }
    } else {
      browsePath = [...browsePath, folder.name];
      await loadFolder();
    }
  }

  function navigateUp() {
    browsePath = browsePath.slice(0, -1);
    loadFolder();
  }

  async function openExistingWorkspace() {
    const wsPath = currentBrowsePath
      ? `${currentBrowsePath}/${previewName}`
      : previewName;
    await app.addWebdavWorkspace(previewName, webdavUrl.trim(), wsPath, webdavUser, webdavPass);
  }

  function startCreate() {
    createName = "Onyx";
    webdavStep = "create";
  }

  async function handleCreateWebdav() {
    if (!createName.trim()) return;
    creating = true;
    try {
      const wsPath = currentBrowsePath
        ? `${currentBrowsePath}/${createName.trim()}`
        : createName.trim();
      await invoke("create_remote_workspace", {
        url: webdavUrl,
        username: webdavUser,
        password: webdavPass,
        path: wsPath,
      });
      await app.addWebdavWorkspace(createName.trim(), webdavUrl.trim(), wsPath, webdavUser, webdavPass);
    } catch (e) {
      browseError = String(e);
      creating = false;
    }
  }

  // ── Window dragging ───────────────────────────────────────────────

  function handleDrag(e: MouseEvent) {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button, input")) return;
    if (isDesktop) appWindow.startDragging();
  }

  function goBack() {
    mode = null;
    name = "Onyx";
    path = "";
    webdavUrl = "";
    webdavUser = "";
    webdavPass = "";
    testStatus = "idle";
    webdavStep = "connect";
    browsePath = [];
    browseEntries = [];
    browseError = null;
  }

  function webdavBack() {
    if (webdavStep === "preview" || webdavStep === "create") {
      webdavStep = "browse";
    } else if (webdavStep === "browse") {
      webdavStep = "connect";
      browsePath = [];
      browseEntries = [];
    } else {
      goBack();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex h-full flex-col" onmousedown={handleDrag}>
  <div class="shrink-0" style="height: var(--safe-top)"></div>
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

      {:else if webdavStep === "connect"}
        <!-- Step 2b: WebDAV connect -->
        <p class="mb-6 text-sm text-text-secondary-light dark:text-text-secondary-dark">
          Connect to a WebDAV server.
        </p>

        <label class="mb-1 block text-xs font-medium opacity-60">Server URL</label>
        <input
          type="url"
          bind:value={webdavUrl}
          placeholder="https://dav.example.com/remote.php/dav/files/user/"
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

        {#if testStatus === "fail"}
          <p class="mb-3 text-xs text-danger">Connection failed. Check your URL and credentials.</p>
        {/if}

        <button
          onclick={connectAndBrowse}
          disabled={!webdavUrl.trim() || testStatus === "testing"}
          class="w-full rounded-lg bg-primary py-2.5 text-sm font-medium text-white hover:bg-primary-hover disabled:opacity-40"
        >
          {testStatus === "testing" ? "Connecting..." : "Connect"}
        </button>

        {#if !isMobile}
          <button
            onclick={goBack}
            class="mt-3 w-full rounded-lg py-2 text-sm opacity-50 hover:opacity-80"
          >
            Back
          </button>
        {/if}

      {:else if webdavStep === "browse"}
        <!-- Step 3: Folder explorer -->
        <p class="mb-4 text-sm text-text-secondary-light dark:text-text-secondary-dark">
          Pick a folder or create a new workspace.
        </p>

        <!-- Breadcrumb / back navigation -->
        <div class="mb-3 flex items-center gap-1 text-xs text-text-secondary-light dark:text-text-secondary-dark">
          {#if browsePath.length > 0}
            <button onclick={navigateUp} class="flex items-center gap-0.5 hover:opacity-80">
              <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
                <path fill-rule="evenodd" d="M17 10a.75.75 0 01-.75.75H5.612l4.158 3.96a.75.75 0 11-1.04 1.08l-5.5-5.25a.75.75 0 010-1.08l5.5-5.25a.75.75 0 111.04 1.08L5.612 9.25H16.25A.75.75 0 0117 10z" />
              </svg>
            </button>
          {/if}
          <span class="truncate font-mono">/{currentBrowsePath}</span>
        </div>

        <!-- Folder list -->
        <div class="mb-4 max-h-48 overflow-y-auto rounded-lg border border-border-light dark:border-border-dark">
          {#if browseLoading}
            <div class="flex items-center justify-center py-6 text-xs opacity-50">Loading...</div>
          {:else if browseError}
            <div class="px-3 py-4 text-xs text-danger">{browseError}</div>
          {:else if browseEntries.length === 0}
            <div class="px-3 py-4 text-xs opacity-50">No folders found.</div>
          {:else}
            {#each browseEntries as entry}
              <button
                onclick={() => navigateInto(entry)}
                class="flex w-full items-center gap-2 border-b border-border-light px-3 py-2.5 text-left text-sm last:border-b-0 hover:bg-black/5 dark:border-border-dark dark:hover:bg-white/10"
              >
                {#if entry.is_workspace}
                  <!-- Workspace icon -->
                  <svg class="h-4 w-4 shrink-0 text-primary" viewBox="0 0 20 20" fill="currentColor">
                    <path d="M10.362 1.093a.75.75 0 00-.724 0L2.523 5.018 10 9.143l7.477-4.125-7.115-3.925zM18 6.443l-7.25 4v8.25l6.862-3.786A.75.75 0 0018 14.25V6.443zM9.25 18.693v-8.25l-7.25-4v7.807a.75.75 0 00.388.657l6.862 3.786z" />
                  </svg>
                {:else}
                  <!-- Folder icon -->
                  <svg class="h-4 w-4 shrink-0 opacity-40" viewBox="0 0 20 20" fill="currentColor">
                    <path d="M3.75 3A1.75 1.75 0 002 4.75v3.26a3.235 3.235 0 011.75-.51h12.5c.644 0 1.245.188 1.75.51V6.75A1.75 1.75 0 0016.25 5h-4.836a.25.25 0 01-.177-.073L9.823 3.513A1.75 1.75 0 008.586 3H3.75zM3.75 9A1.75 1.75 0 002 10.75v4.5c0 .966.784 1.75 1.75 1.75h12.5A1.75 1.75 0 0018 15.25v-4.5A1.75 1.75 0 0016.25 9H3.75z" />
                  </svg>
                {/if}
                <span class="truncate">{entry.name}</span>
                {#if entry.is_workspace}
                  <span class="ml-auto shrink-0 rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary">workspace</span>
                {:else}
                  <svg class="ml-auto h-3.5 w-3.5 shrink-0 opacity-30" viewBox="0 0 20 20" fill="currentColor">
                    <path fill-rule="evenodd" d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z" />
                  </svg>
                {/if}
              </button>
            {/each}
          {/if}
        </div>

        <button
          onclick={startCreate}
          class="w-full rounded-lg bg-primary py-2.5 text-sm font-medium text-white hover:bg-primary-hover"
        >
          Create Workspace
        </button>

        <button
          onclick={webdavBack}
          class="mt-3 w-full rounded-lg py-2 text-sm opacity-50 hover:opacity-80"
        >
          Back
        </button>

      {:else if webdavStep === "preview"}
        <!-- Step 4a: Workspace preview -->
        <div class="mb-4 flex items-center gap-2">
          <button onclick={() => (webdavStep = "browse")} class="rounded-lg p-1 opacity-50 hover:opacity-80">
            <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
              <path fill-rule="evenodd" d="M17 10a.75.75 0 01-.75.75H5.612l4.158 3.96a.75.75 0 11-1.04 1.08l-5.5-5.25a.75.75 0 010-1.08l5.5-5.25a.75.75 0 111.04 1.08L5.612 9.25H16.25A.75.75 0 0117 10z" />
            </svg>
          </button>
          <h2 class="text-lg font-semibold">{previewName}</h2>
        </div>

        {#if previewLoading}
          <div class="flex items-center justify-center py-8 text-xs opacity-50">Loading workspace...</div>
        {:else if previewLists.length === 0}
          <p class="mb-6 py-4 text-center text-xs opacity-50">No lists in this workspace yet.</p>
        {:else}
          <div class="mb-6 max-h-48 overflow-y-auto rounded-lg border border-border-light dark:border-border-dark">
            {#each previewLists as list}
              <div class="flex items-center justify-between border-b border-border-light px-3 py-2.5 text-sm last:border-b-0 dark:border-border-dark">
                <span class="truncate">{list.name}</span>
                <span class="shrink-0 rounded-full bg-black/5 px-2 py-0.5 text-xs tabular-nums dark:bg-white/10">
                  {list.task_count} {list.task_count === 1 ? "task" : "tasks"}
                </span>
              </div>
            {/each}
          </div>
        {/if}

        <button
          onclick={openExistingWorkspace}
          class="w-full rounded-lg bg-primary py-2.5 text-sm font-medium text-white hover:bg-primary-hover"
        >
          Open Workspace
        </button>

      {:else if webdavStep === "create"}
        <!-- Step 4b: Create workspace -->
        <div class="mb-4 flex items-center gap-2">
          <button onclick={() => (webdavStep = "browse")} class="rounded-lg p-1 opacity-50 hover:opacity-80">
            <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
              <path fill-rule="evenodd" d="M17 10a.75.75 0 01-.75.75H5.612l4.158 3.96a.75.75 0 11-1.04 1.08l-5.5-5.25a.75.75 0 010-1.08l5.5-5.25a.75.75 0 111.04 1.08L5.612 9.25H16.25A.75.75 0 0117 10z" />
            </svg>
          </button>
          <h2 class="text-lg font-semibold">New Workspace</h2>
        </div>

        <p class="mb-1 text-xs text-text-secondary-light dark:text-text-secondary-dark">
          Creating in: <span class="font-mono">/{currentBrowsePath}</span>
        </p>

        <label class="mb-1 block text-sm font-medium">
          Workspace name
          <input
            type="text"
            bind:value={createName}
            placeholder="My Tasks"
            class="mt-1 mb-4 w-full rounded-lg border border-border-light bg-transparent px-3 py-2 text-sm font-normal outline-none focus:border-primary dark:border-border-dark"
          />
        </label>

        {#if browseError}
          <p class="mb-3 text-xs text-danger">{browseError}</p>
        {/if}

        <button
          onclick={handleCreateWebdav}
          disabled={!createName.trim() || creating}
          class="w-full rounded-lg bg-primary py-2.5 text-sm font-medium text-white hover:bg-primary-hover disabled:opacity-40"
        >
          {creating ? "Creating..." : "Create Workspace"}
        </button>
      {/if}
    </div>
  </div>
</div>
