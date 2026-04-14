<script lang="ts">
  import { onMount } from "svelte";
  import { platform } from "@tauri-apps/plugin-os";
  import { app } from "./lib/stores/app.svelte";
  import SetupScreen from "./lib/screens/SetupScreen.svelte";
  import TasksScreen from "./lib/screens/TasksScreen.svelte";

  const isLinux = platform() === "linux";
  const isMobile = platform() === "android" || platform() === "ios";

  onMount(() => {
    app.loadConfig();
  });

  $effect(() => {
    document.documentElement.classList.toggle("decorations-none", app.windowDecorations === "none");
  });
</script>

<div class={app.isDark ? "dark" : ""} data-theme={app.currentTheme ?? ""} data-decorations={app.windowDecorations}>
  <div class="h-screen w-screen" class:p-2={isLinux && app.windowDecorations === "custom"}>
    <div
      class="relative h-full w-full overflow-hidden bg-surface-light text-text-light dark:bg-surface-dark dark:text-text-dark"
      class:rounded-xl={isLinux && app.windowDecorations === "custom"}
      class:linux-window-border={isLinux && app.windowDecorations !== "system"}
      style="container-type: inline-size"
    >
      {#if app.error}
        <div
          class="absolute top-0 left-0 right-0 z-50 flex items-center justify-between bg-danger px-4 py-2 text-sm text-white"
          style="top: env(safe-area-inset-top)"
        >
          <span>{app.error}</span>
          <button onclick={() => app.clearError()} class="ml-2 font-bold">✕</button>
        </div>
      {/if}

      {#if app.initialSync}
        <div class="flex h-full flex-col items-center justify-center gap-4">
          <svg class="h-8 w-8 animate-spin text-primary" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" opacity="0.25" />
            <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" stroke-width="3" stroke-linecap="round" />
          </svg>
          <p class="text-sm text-text-secondary-light dark:text-text-secondary-dark">Syncing workspace&hellip;</p>
        </div>
      {:else if app.screen === "missing"}
        <div class="flex h-full items-center justify-center p-6">
          <div class="w-full max-w-sm rounded-2xl bg-card-light p-8 shadow-lg dark:bg-card-dark">
            <h1 class="mb-1 text-2xl font-bold">Workspace Not Found</h1>
            <p class="mb-2 text-sm text-text-secondary-light dark:text-text-secondary-dark">
              The workspace <strong>{app.missingWorkspace && app.config?.workspaces[app.missingWorkspace]?.name || "Unknown"}</strong> could not be opened. Its folder may have been moved or deleted.
            </p>
            <p class="mb-6 text-sm text-text-secondary-light dark:text-text-secondary-dark">
              It will be removed from your workspace list. You can re-add it if the folder becomes available again.
            </p>
            <button
              onclick={() => app.forgetMissingWorkspace()}
              class="w-full rounded-lg bg-primary py-2.5 text-sm font-medium text-white hover:bg-primary-hover"
            >
              Continue
            </button>
          </div>
        </div>
      {:else if app.screen === "setup"}
        <SetupScreen cancellable={app.hasWorkspace} />
      {:else}
        {#key app.config?.current_workspace}
          <TasksScreen />
        {/key}
      {/if}
    </div>
  </div>
</div>
