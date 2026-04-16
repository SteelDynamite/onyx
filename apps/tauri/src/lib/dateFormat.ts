const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const pad = (n: number) => String(n).padStart(2, "0");

/** Format a date for display in chips (detail view, new task input). */
export function formatDateChip(iso: string, hasTime: boolean): string {
  const d = new Date(iso);
  const today = new Date();
  const day = DAY_NAMES[d.getDay()];
  const timePart = hasTime ? `, ${pad(d.getHours())}:${pad(d.getMinutes())}` : "";
  if (d.toDateString() === today.toDateString()) return `Today${timePart}`;
  return `${day}, ${pad(d.getDate())}/${pad(d.getMonth() + 1)}${timePart}`;
}

/** Format a date for compact display in task list items. */
export function formatDateLabel(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  if (d.toDateString() === today.toDateString()) return "Today";
  const tomorrow = new Date(today);
  tomorrow.setDate(today.getDate() + 1);
  if (d.toDateString() === tomorrow.toDateString()) return "Tomorrow";
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}
