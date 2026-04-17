<script lang="ts" module>
  // Shared state accessible from outside
  export const newTaskState = $state({ open: false });
</script>

<script lang="ts">
  import { app } from "../stores/app.svelte";
  import { formatDateChip } from "../dateFormat";
  import DateTimePicker from "./DateTimePicker.svelte";

  let title = $state("");
  let description = $state("");
  let date = $state<string | null>(null);
  let dateHasTime = $state(false);
  let inputEl = $state<HTMLInputElement | null>(null);
  let showDatePicker = $state(false);

  async function handleSubmit() {
    if (!title.trim()) return;
    // Pass date/has_time into createTask directly so the date can't be lost
    // if a second round-trip to update() failed after the create succeeded.
    await app.createTask(
      title.trim(),
      description.trim() || undefined,
      undefined,
      date,
      dateHasTime,
    );
    title = "";
    description = "";
    date = null;
    dateHasTime = false;
    newTaskState.open = false;
  }

  function handleClose() {
    newTaskState.open = false;
    title = "";
    description = "";
    date = null;
    dateHasTime = false;
    showDatePicker = false;
  }

  function handleDateChange(iso: string | null, hasTime: boolean = false) {
    date = iso;
    dateHasTime = hasTime;
  }


  $effect(() => {
    if (newTaskState.open) {
      requestAnimationFrame(() => inputEl?.focus());
    }
  });
</script>

<!-- Backdrop + sheet wrapper -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="absolute inset-0 z-50 flex flex-col justify-end overflow-hidden transition-opacity duration-250 ease-out {newTaskState.open ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'}"
  style="background: rgba(0,0,0,0.4)"
  onclick={handleClose}
  onkeydown={(e) => { if (e.key === "Escape") handleClose(); }}
>
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="rounded-t-2xl bg-surface-light shadow-xl transition-transform duration-250 ease-out dark:bg-card-dark {newTaskState.open ? 'translate-y-0' : 'translate-y-full'}"
  onclick={(e) => e.stopPropagation()}
>
  <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }} class="px-4 pt-4">
    <!-- Title -->
    <input
      bind:this={inputEl}
      type="text"
      bind:value={title}
      placeholder="Task title"
      class="w-full bg-transparent text-xl font-bold outline-none placeholder:opacity-30"
      onkeydown={(e) => { if (e.key === "Escape") handleClose(); }}
    />

    <!-- Description -->
    <div class="mt-4 flex items-start gap-3">
      <svg class="mt-0.5 h-5 w-5 shrink-0 opacity-40" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M4 4a1 1 0 011-1h10a1 1 0 110 2H5a1 1 0 01-1-1zm0 4a1 1 0 011-1h10a1 1 0 110 2H5a1 1 0 01-1-1zm0 4a1 1 0 011-1h7a1 1 0 110 2H5a1 1 0 01-1-1z" />
      </svg>
      <textarea
        bind:value={description}
        placeholder="Add details"
        rows="3"
        class="w-full flex-1 resize-none bg-transparent text-sm outline-none placeholder:opacity-40"
        onkeydown={(e) => { if (e.key === "Escape") handleClose(); }}
      ></textarea>
    </div>

    <!-- Date/time -->
    <div class="mt-4 flex items-center gap-3">
      <svg class="h-5 w-5 shrink-0 opacity-40" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm1-12a1 1 0 10-2 0v4a1 1 0 00.293.707l2.828 2.829a1 1 0 101.415-1.415L11 9.586V6z" clip-rule="evenodd" />
      </svg>
      {#if date}
        <div class="flex items-center gap-1.5 rounded-full border border-border-light bg-black/5 px-3 py-1 text-sm dark:border-border-dark dark:bg-white/10">
          <button type="button" onclick={() => (showDatePicker = true)} class="hover:opacity-70">
            {formatDateChip(date, dateHasTime)}
          </button>
          <button type="button" onclick={() => (date = null)} class="opacity-40 hover:opacity-80">
            <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
              <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
            </svg>
          </button>
        </div>
      {:else}
        <button
          type="button"
          onclick={() => (showDatePicker = true)}
          class="text-sm opacity-40 hover:opacity-70"
        >
          Add date/time
        </button>
      {/if}
    </div>
  </form>

  <!-- Save button -->
  <div class="border-t border-border-light px-4 py-3 mt-4 dark:border-border-dark">
    <button
      onclick={handleSubmit}
      disabled={!title.trim()}
      class="w-full text-center text-sm font-medium text-primary hover:opacity-70 disabled:opacity-30"
    >
      Save
    </button>
  </div>

  <!-- Date picker overlay -->
  {#if showDatePicker}
    <DateTimePicker
      value={date}
      has_time={dateHasTime}
      onchange={handleDateChange}
      onclose={() => (showDatePicker = false)}
    />
  {/if}
</div>
</div>
