<script lang="ts">
  import { onMount } from "svelte";
  import { platform } from "@tauri-apps/plugin-os";
  import { app } from "./lib/stores/app.svelte";
  import SetupScreen from "./lib/screens/SetupScreen.svelte";
  import TasksScreen from "./lib/screens/TasksScreen.svelte";

  const isLinux = platform() === "linux";

  onMount(() => {
    app.loadConfig();
  });
</script>

<div class={app.isDark ? "dark" : ""} data-theme={app.currentTheme ?? ""}>
  <div class="h-screen w-screen" class:p-2={isLinux}>
    <div
      class="relative h-full w-full overflow-hidden bg-surface-light text-text-light dark:bg-surface-dark dark:text-text-dark"
      class:rounded-xl={isLinux}
      class:linux-window-border={isLinux}
      style="container-type: inline-size"
    >
      {#if app.error}
        <div
          class="absolute top-0 left-0 right-0 z-50 flex items-center justify-between bg-danger px-4 py-2 text-sm text-white"
        >
          <span>{app.error}</span>
          <button onclick={() => app.clearError()} class="ml-2 font-bold">✕</button>
        </div>
      {/if}

      {#if app.screen === "missing"}
        <div class="flex h-full items-center justify-center p-6">
          <div class="w-full max-w-sm rounded-2xl bg-card-light p-8 shadow-lg dark:bg-card-dark">
            <h1 class="mb-1 text-2xl font-bold">Workspace Not Found</h1>
            <p class="mb-2 text-sm text-text-secondary-light dark:text-text-secondary-dark">
              The workspace <strong>{app.missingWorkspace}</strong> could not be opened. Its folder may have been moved or deleted.
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
        <TasksScreen />
      {/if}
    </div>
  </div>
</div>
