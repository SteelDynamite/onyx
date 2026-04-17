<script lang="ts" module>
  // Shared counter so sibling Escape handlers (e.g. TasksScreen's svelte:window
  // listener) can tell when a ConfirmDialog is open and defer to it instead of
  // popping the task-detail view behind the dialog.
  let openCount = $state(0);
  export function isConfirmDialogOpen(): boolean {
    return openCount > 0;
  }
</script>

<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";

  let { message, detail, confirmText = "Confirm", danger = false, onconfirm, oncancel }:
    { message: string; detail?: string; confirmText?: string; danger?: boolean; onconfirm: () => void; oncancel: () => void } = $props();

  let cancelBtn: HTMLButtonElement | undefined = $state();

  function handleGlobalKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    e.stopImmediatePropagation();
    e.preventDefault();
    oncancel();
  }

  onMount(() => {
    openCount += 1;
    // Focus Cancel so Escape/Enter go through the dialog's own keydown handler
    // (which cancels) instead of leaking to the global svelte:window listener
    // in TasksScreen (which would pop the task detail view).
    tick().then(() => cancelBtn?.focus());
    // Belt-and-suspenders: capture-phase listener dismisses even if focus
    // didn't land on Cancel (e.g. under test harnesses or headless compositors).
    window.addEventListener("keydown", handleGlobalKeydown, true);
  });
  onDestroy(() => {
    openCount -= 1;
    window.removeEventListener("keydown", handleGlobalKeydown, true);
  });
</script>

<div
  class="absolute inset-0 z-50 flex items-center justify-center"
  role="dialog"
  aria-modal="true"
  aria-label={message}
  onclick={oncancel}
  onkeydown={(e) => { if (e.key === "Escape") { e.stopPropagation(); oncancel(); } }}
>
  <div class="absolute inset-0 bg-black/40"></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="relative z-10 mx-6 w-full max-w-sm rounded-xl border border-border-light bg-surface-light p-5 shadow-xl dark:border-border-dark dark:bg-surface-dark"
    onclick={(e) => e.stopPropagation()}
  >
    <p class="text-sm font-medium">{message}</p>
    {#if detail}
      <p class="mt-2 text-xs opacity-50">{detail}</p>
    {/if}
    <div class="mt-4 flex justify-end gap-2">
      <button
        bind:this={cancelBtn}
        onclick={oncancel}
        class="rounded-lg px-4 py-2 text-sm hover:bg-black/5 dark:hover:bg-white/10"
      >
        Cancel
      </button>
      <button
        onclick={onconfirm}
        class="rounded-lg px-4 py-2 text-sm font-medium text-white {danger ? 'bg-danger hover:bg-danger/80' : 'bg-primary hover:bg-primary/80'}"
      >
        {confirmText}
      </button>
    </div>
  </div>
</div>
