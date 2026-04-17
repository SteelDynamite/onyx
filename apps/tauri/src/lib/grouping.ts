import type { Task } from "./types";

export type TaskGroup = { label: string; tasks: Task[]; date: Date | null };

/**
 * Group pending tasks into date buckets for the "group by date" view.
 *
 * Order:
 *   Overdue → Today → Tomorrow → future days (chronological) → No Date
 *
 * Within each dated bucket tasks sort by due date+time ascending, with the
 * original `pendingTasks` index as a stable tiebreaker. "No Date" preserves
 * the caller-supplied order.
 */
export function groupTasksByDate(pendingTasks: Task[], now: Date = new Date()): TaskGroup[] {
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const tomorrowStart = new Date(todayStart);
  tomorrowStart.setDate(todayStart.getDate() + 1);

  const overdue: Task[] = [];
  const today: Task[] = [];
  const tomorrow: Task[] = [];
  const futureByDay = new Map<string, { date: Date; tasks: Task[] }>();
  const noDate: Task[] = [];

  for (const task of pendingTasks) {
    if (!task.date) {
      noDate.push(task);
    } else {
      const d = new Date(task.date);
      const dayStart = new Date(d.getFullYear(), d.getMonth(), d.getDate());
      if (dayStart < todayStart) overdue.push(task);
      else if (dayStart.getTime() === todayStart.getTime()) today.push(task);
      else if (dayStart.getTime() === tomorrowStart.getTime()) tomorrow.push(task);
      else {
        const key = dayStart.toISOString();
        if (!futureByDay.has(key)) futureByDay.set(key, { date: dayStart, tasks: [] });
        futureByDay.get(key)!.tasks.push(task);
      }
    }
  }

  const taskOrderIndex = new Map(pendingTasks.map((t, i) => [t.id, i]));
  const sortByDue = (a: Task, b: Task) => {
    const dateDiff = new Date(a.date!).getTime() - new Date(b.date!).getTime();
    if (dateDiff !== 0) return dateDiff;
    return (taskOrderIndex.get(a.id) ?? 0) - (taskOrderIndex.get(b.id) ?? 0);
  };
  overdue.sort(sortByDue);
  today.sort(sortByDue);
  tomorrow.sort(sortByDue);

  const groups: TaskGroup[] = [];
  if (overdue.length) groups.push({ label: "Overdue", tasks: overdue, date: null });
  if (today.length) groups.push({ label: "Today", tasks: today, date: todayStart });
  if (tomorrow.length) groups.push({ label: "Tomorrow", tasks: tomorrow, date: tomorrowStart });

  const currentYear = now.getFullYear();
  for (const [, { date, tasks }] of [...futureByDay.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    tasks.sort(sortByDue);
    const opts: Intl.DateTimeFormatOptions = date.getFullYear() !== currentYear
      ? { weekday: "short", month: "short", day: "numeric", year: "numeric" }
      : { weekday: "short", month: "short", day: "numeric" };
    groups.push({ label: date.toLocaleDateString(undefined, opts), tasks, date });
  }

  if (noDate.length) groups.push({ label: "No Date", tasks: noDate, date: null });

  return groups;
}
