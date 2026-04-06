<script lang="ts">
  let { value = null, has_time = false, onchange, onclose }: {
    value: string | null;
    has_time: boolean;
    onchange: (iso: string | null, has_time: boolean) => void;
    onclose: () => void;
  } = $props();

  const DAY_NAMES = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

  let now = new Date();
  let existing = value ? new Date(value) : null;
  let viewYear = $state(existing ? existing.getFullYear() : now.getFullYear());
  let viewMonth = $state(existing ? existing.getMonth() : now.getMonth());
  let selectedDay = $state(existing ? existing.getDate() : now.getDate());
  let includeTime = $state(has_time);
  let selectedHour = $state(existing ? existing.getHours() : now.getHours());
  let selectedMinute = $state(existing ? existing.getMinutes() : 0);
  let visible = $state(false);

  let todayStr = `${now.getFullYear()}-${now.getMonth()}-${now.getDate()}`;

  let daysInMonth = $derived(new Date(viewYear, viewMonth + 1, 0).getDate());
  let firstDayOfWeek = $derived(new Date(viewYear, viewMonth, 1).getDay());
  let monthLabel = $derived(new Date(viewYear, viewMonth).toLocaleDateString(undefined, { month: "long", year: "numeric" }));

  let calendarCells = $derived.by(() => {
    const cells: (number | null)[] = [];
    for (let i = 0; i < firstDayOfWeek; i++) cells.push(null);
    for (let d = 1; d <= daysInMonth; d++) cells.push(d);
    return cells;
  });

  requestAnimationFrame(() => { visible = true; });

  function dismiss() {
    visible = false;
    setTimeout(onclose, 200);
  }

  function prevMonth() {
    if (viewMonth === 0) { viewMonth = 11; viewYear--; }
    else viewMonth--;
  }

  function nextMonth() {
    if (viewMonth === 11) { viewMonth = 0; viewYear++; }
    else viewMonth++;
  }

  function selectDay(day: number) {
    selectedDay = day;
  }

  function isToday(day: number): boolean {
    return `${viewYear}-${viewMonth}-${day}` === todayStr;
  }

  function isSelected(day: number): boolean {
    return selectedDay === day && (!value || (() => {
      const v = new Date(value);
      return v.getFullYear() === viewYear && v.getMonth() === viewMonth;
    })());
  }

  function done() {
    const h = includeTime ? selectedHour : 0;
    const m = includeTime ? selectedMinute : 0;
    const iso = new Date(viewYear, viewMonth, selectedDay, h, m).toISOString();
    onchange(iso, includeTime);
    dismiss();
  }

  function clear() {
    onchange(null, false);
    dismiss();
  }
</script>

<!-- Wrapper: backdrop click dismisses, sheet click stops propagation -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="absolute inset-0 z-40 transition-opacity duration-200 {visible ? 'opacity-100' : 'opacity-0'}"
  style="background: rgba(0,0,0,0.4)"
  onclick={dismiss}
  onkeydown={(e) => { if (e.key === "Escape") dismiss(); }}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="absolute bottom-0 left-0 right-0 rounded-t-2xl bg-surface-light shadow-xl transition-transform duration-200 ease-out dark:bg-card-dark {visible ? 'translate-y-0' : 'translate-y-full'}"
    onclick={(e) => e.stopPropagation()}
  >
    <!-- Header -->
    <div class="flex items-center justify-between px-4 pt-3 pb-2">
      <span class="text-sm font-semibold">Date & Time</span>
      <button onclick={done} class="text-sm font-medium text-primary hover:opacity-70">Done</button>
    </div>

    <!-- Month navigation -->
    <div class="flex items-center justify-between px-4 py-2">
      <span class="text-sm font-medium">{monthLabel}</span>
      <div class="flex gap-1">
        <button onclick={prevMonth} class="rounded p-1 hover:bg-black/5 dark:hover:bg-white/10">
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path fill-rule="evenodd" d="M12.79 5.23a.75.75 0 01-.02 1.06L8.832 10l3.938 3.71a.75.75 0 11-1.04 1.08l-4.5-4.25a.75.75 0 010-1.08l4.5-4.25a.75.75 0 011.06.02z" />
          </svg>
        </button>
        <button onclick={nextMonth} class="rounded p-1 hover:bg-black/5 dark:hover:bg-white/10">
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path fill-rule="evenodd" d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Day names -->
    <div class="grid grid-cols-7 px-4">
      {#each DAY_NAMES as name}
        <div class="py-1 text-center text-xs font-medium opacity-40">{name}</div>
      {/each}
    </div>

    <!-- Calendar grid -->
    <div class="grid grid-cols-7 content-start px-4 pb-2" style="height: 192px;">
      {#each calendarCells as day}
        {#if day === null}
          <div></div>
        {:else}
          <button
            onclick={() => selectDay(day)}
            class="mx-auto flex h-8 w-8 items-center justify-center rounded-full text-sm transition-colors
              {selectedDay === day ? 'bg-primary text-white' : ''}
              {isToday(day) && selectedDay !== day ? 'font-bold text-primary' : ''}
              {selectedDay !== day && !isToday(day) ? 'hover:bg-black/5 dark:hover:bg-white/10' : ''}"
          >
            {day}
          </button>
        {/if}
      {/each}
    </div>

    <!-- Time section -->
    <div class="flex items-center gap-3 border-t border-border-light px-4 py-3 dark:border-border-dark">
      {#if includeTime}
        <span class="text-sm opacity-50">Time</span>
        <div class="flex items-center gap-1">
          <select
            bind:value={selectedHour}
            class="appearance-none rounded-lg border border-border-light bg-surface-light px-2 py-1 text-sm text-text-light outline-none dark:border-border-dark dark:bg-surface-dark dark:text-text-dark"
          >
            {#each Array(24) as _, h}
              <option value={h}>{String(h).padStart(2, "0")}</option>
            {/each}
          </select>
          <span class="text-sm opacity-40">:</span>
          <select
            bind:value={selectedMinute}
            class="appearance-none rounded-lg border border-border-light bg-surface-light px-2 py-1 text-sm text-text-light outline-none dark:border-border-dark dark:bg-surface-dark dark:text-text-dark"
          >
            {#each [0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55] as m}
              <option value={m}>{String(m).padStart(2, "0")}</option>
            {/each}
          </select>
        </div>
        <button onclick={() => (includeTime = false)} class="ml-auto opacity-40 hover:opacity-80">
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
          </svg>
        </button>
      {:else}
        <button
          onclick={() => (includeTime = true)}
          class="text-sm opacity-50 hover:opacity-80"
        >
          Set time
        </button>
      {/if}
    </div>

    <!-- Clear button -->
    {#if value}
      <div class="border-t border-border-light px-4 py-3 dark:border-border-dark">
        <button onclick={clear} class="text-sm text-danger hover:opacity-70">Clear date</button>
      </div>
    {/if}
    <div style="height: max(0.75rem, var(--safe-bottom))"></div>
  </div>
</div>
