<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { platform } from "@tauri-apps/plugin-os";
  import { app } from "../stores/app.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";

  const isLinux = platform() === "linux";

  let { onclose, workspaceId, ondelete }: { onclose?: () => void; workspaceId: string; ondelete?: (id: string) => void } = $props();

  let ws = $derived(app.config?.workspaces[workspaceId]);
  let isWebdav = $derived(ws?.mode === "webdav");

  let webdavUrl = $state("");
  let webdavUser = $state("");
  let webdavPass = $state("");
  let testStatus = $state<"idle" | "testing" | "ok" | "fail">("idle");

  let renaming = $state(false);
  let renameValue = $state("");
  let showKebab = $state(false);
  let confirmRename = $state(false);

  $effect(() => {
    if (!ws?.webdav_url) return;
    webdavUrl = ws.webdav_url;
    try {
      const domain = new URL(ws.webdav_url).hostname;
      invoke<[string, string]>("load_credentials", { domain }).then(([u, p]) => {
        webdavUser = u;
        webdavPass = p;
      }).catch((e) => {
        console.warn("Failed to load credentials:", e);
      });
    } catch {}
  });

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

  async function saveWebdav() {
    if (!webdavUrl.trim()) return;
    await invoke("set_webdav_config", {
      workspaceId,
      webdavUrl: webdavUrl.trim(),
    });
    if (webdavUser && webdavPass) {
      const domain = new URL(webdavUrl).hostname;
      await invoke("store_credentials", {
        domain,
        username: webdavUser,
        password: webdavPass,
      });
    }
    await app.loadConfig();
  }

  function startRename() {
    showKebab = false;
    renaming = true;
    renameValue = ws?.name ?? "";
  }

  async function handleRename() {
    if (!renaming) return;
    renaming = false;
    var trimmed = renameValue.trim();
    if (!trimmed || trimmed === ws?.name) return;
    confirmRename = true;
  }

  async function doRename() {
    confirmRename = false;
    var trimmed = renameValue.trim();
    if (!trimmed) return;
    await app.renameWorkspace(workspaceId, trimmed);
  }
  function handleWindowClick(e: MouseEvent) {
    if (showKebab && !(e.target as HTMLElement).closest("[data-settings-kebab]")) showKebab = false;
  }
</script>

<svelte:window onclick={handleWindowClick} />

<header
  data-tauri-drag-region
  class="flex items-center justify-between border-b border-border-light px-4 py-3 dark:border-border-dark"
>
  <h1 class="text-lg font-bold" data-tauri-drag-region>Workspace Settings</h1>
  <button
    onclick={() => onclose?.()}
    class="rounded-lg p-1.5 hover:bg-black/5 dark:hover:bg-white/10"
  >
    <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
      <path
        d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z"
      />
    </svg>
  </button>
</header>

<!-- Workspace name + kebab -->
<div class="flex items-center gap-2 px-4 py-3">
  <div class="min-w-0 flex-1">
    {#if renaming}
      <input
        type="text"
        bind:value={renameValue}
        class="w-full bg-transparent text-xl font-bold outline-none"
        onkeydown={(e) => { if (e.key === "Enter") handleRename(); if (e.key === "Escape") { renaming = false; } }}
        onblur={handleRename}
        autofocus
      />
    {:else}
      <p class="text-xl font-bold">{ws?.name}</p>
    {/if}
  </div>
  <div class="relative shrink-0" data-settings-kebab>
    <button
      onclick={() => showKebab = !showKebab}
      class="rounded-lg p-1.5 hover:bg-black/5 dark:hover:bg-white/10"
    >
      <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
        <path d="M10 6a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm0 5.5a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm0 5.5a1.5 1.5 0 110-3 1.5 1.5 0 010 3z" />
      </svg>
    </button>
    {#if showKebab}
      <div class="dropdown-menu absolute right-0 top-full z-10 mt-1 w-40 rounded-xl border border-border-light bg-surface-light py-1 menu-shadow dark:border-border-dark dark:bg-surface-dark">
        <button
          onclick={startRename}
          class="flex w-full items-center gap-2 px-4 py-2 text-sm hover:bg-black/5 dark:hover:bg-white/10"
        >
          <svg class="h-4 w-4 opacity-60" viewBox="0 0 20 20" fill="currentColor">
            <path d="M2.695 14.763l-1.262 3.154a.5.5 0 00.65.65l3.155-1.262a4 4 0 001.343-.885L17.5 5.5a2.121 2.121 0 00-3-3L3.58 13.42a4 4 0 00-.885 1.343z" />
          </svg>
          Rename
        </button>
        <button
          onclick={() => { showKebab = false; ondelete?.(workspaceId); }}
          class="flex w-full items-center gap-2 px-4 py-2 text-sm text-danger hover:bg-black/5 dark:hover:bg-white/10"
        >
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
          </svg>
          Delete
        </button>
      </div>
    {/if}
  </div>
</div>

<main class="flex-1 overflow-y-auto p-4">
  <!-- WebDAV Sync (only for webdav workspaces) -->
  {#if isWebdav}
    <section class="mb-6">
      <h2 class="mb-3 text-sm font-semibold uppercase tracking-wide opacity-50">
        WebDAV Sync
      </h2>
      <div class="rounded-xl border border-border-light p-4 dark:border-border-dark">
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

        <div class="flex gap-2">
          <button
            onclick={testConnection}
            disabled={!webdavUrl.trim()}
            class="rounded-lg border border-border-light px-4 py-2 text-sm font-medium hover:bg-black/5 disabled:opacity-40 dark:border-border-dark dark:hover:bg-white/10"
          >
            {testStatus === "testing" ? "Testing..." : testStatus === "ok" ? "Connected" : testStatus === "fail" ? "Failed -- Retry" : "Test Connection"}
          </button>
          <button
            onclick={saveWebdav}
            disabled={!webdavUrl.trim()}
            class="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-white hover:bg-primary-hover disabled:opacity-40"
          >
            Save
          </button>
        </div>
      </div>

      <div class="mt-3">
        <label class="mb-1 block text-xs font-medium opacity-60">Sync interval (focused)</label>
        <select
          value={String(app.syncIntervalSecs)}
          onchange={(e) => {
            const val = parseInt((e.target as HTMLSelectElement).value);
            app.setSyncInterval(val === 60 ? null : val);
          }}
          class="w-full appearance-none rounded-lg border border-border-light bg-surface-light px-3 py-2 text-sm text-text-light outline-none focus:border-primary dark:border-border-dark dark:bg-surface-dark dark:text-text-dark"
        >
          <option value="30">30 seconds</option>
          <option value="60">1 minute</option>
          <option value="120">2 minutes</option>
          <option value="300">5 minutes</option>
          <option value="600">10 minutes</option>
        </select>
      </div>

      <div class="mt-3">
        <label class="mb-1 block text-xs font-medium opacity-60">Sync interval (background)</label>
        <select
          value={String(app.syncIntervalUnfocusedSecs)}
          onchange={(e) => {
            const val = parseInt((e.target as HTMLSelectElement).value);
            app.setSyncIntervalUnfocused(val === 600 ? null : val);
          }}
          class="w-full appearance-none rounded-lg border border-border-light bg-surface-light px-3 py-2 text-sm text-text-light outline-none focus:border-primary dark:border-border-dark dark:bg-surface-dark dark:text-text-dark"
        >
          <option value="60">1 minute</option>
          <option value="120">2 minutes</option>
          <option value="300">5 minutes</option>
          <option value="600">10 minutes</option>
          <option value="1800">30 minutes</option>
        </select>
      </div>
    </section>
  {/if}

  <!-- Theme -->
  <section>
    <label class="mb-1 block text-xs font-medium opacity-60">Theme</label>
    <select
      value={ws?.theme ?? ""}
      onchange={(e) => {
        const val = (e.target as HTMLSelectElement).value;
        app.setTheme(val || null);
      }}
      class="w-full appearance-none rounded-lg border border-border-light bg-surface-light px-3 py-2 text-sm text-text-light outline-none focus:border-primary dark:border-border-dark dark:bg-surface-dark dark:text-text-dark"
    >
      <option value="">System default</option>
      <option value="light">Light</option>
      <option value="dark">Dark</option>
      <option value="nord">Nord</option>
      <option value="dracula">Dracula</option>
      <option value="solarized">Solarized Dark</option>
      <option value="onyx">Black and Gold</option>
      <option value="ink">Ink</option>
    </select>
  </section>

  {#if isLinux}
    <!-- Window decorations (Linux only) -->
    <section class="mt-6">
      <label class="mb-1 block text-xs font-medium opacity-60">Window decorations</label>
      <select
        value={app.windowDecorations}
        onchange={(e) => app.setWindowDecorations((e.target as HTMLSelectElement).value as "custom" | "none" | "system")}
        class="w-full appearance-none rounded-lg border border-border-light bg-surface-light px-3 py-2 text-sm text-text-light outline-none focus:border-primary dark:border-border-dark dark:bg-surface-dark dark:text-text-dark"
      >
        <option value="custom">Custom border</option>
        <option value="none">Borderless</option>
        <option value="system">System title bar</option>
      </select>
    </section>
  {/if}

  <p class="mt-8 text-center text-xs opacity-30">Tauri v2 + Svelte</p>
</main>

{#if confirmRename}
  <ConfirmDialog
    message="Rename workspace to '{renameValue.trim()}'?"
    detail={isWebdav ? "This will rename the folder on the WebDAV server." : "This will rename the folder on disk."}
    confirmText="Rename"
    onconfirm={doRename}
    oncancel={() => confirmRename = false}
  />
{/if}
