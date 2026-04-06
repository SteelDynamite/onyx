<script lang="ts">
  let { message, detail, confirmText = "Confirm", danger = false, onconfirm, oncancel }:
    { message: string; detail?: string; confirmText?: string; danger?: boolean; onconfirm: () => void; oncancel: () => void } = $props();
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
