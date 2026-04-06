<script lang="ts">
  import type { Snippet } from "svelte";
  let { onclose, children }: { onclose: () => void; children: Snippet } = $props();
</script>

<!-- Backdrop -->
<div
  class="fixed inset-0 z-40 bg-black/40"
  role="button"
  tabindex="-1"
  aria-label="Close sheet"
  onclick={onclose}
  onkeydown={(e) => { if (e.key === "Escape") onclose(); }}
></div>

<!-- Sheet -->
<div
  role="dialog"
  aria-modal="true"
  class="fixed bottom-0 left-0 right-0 z-50 max-h-[70vh] overflow-y-auto rounded-t-2xl bg-surface-light shadow-xl dark:bg-card-dark animate-slide-up"
>
  <!-- Drag handle -->
  <div class="flex justify-center py-2">
    <div class="h-1 w-8 rounded-full bg-gray-300 dark:bg-gray-600"></div>
  </div>
  {@render children()}
  <div class="h-[env(safe-area-inset-bottom)]"></div>
</div>

<style>
  @keyframes slide-up {
    from {
      transform: translateY(100%);
    }
    to {
      transform: translateY(0);
    }
  }
  .animate-slide-up {
    animation: slide-up 0.25s ease-out;
  }
</style>
