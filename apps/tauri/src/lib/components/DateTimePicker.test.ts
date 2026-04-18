import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import DateTimePicker from "./DateTimePicker.svelte";

beforeEach(() => {
  cleanup();
});

describe("DateTimePicker — selected highlight", () => {
  it("only marks the selected day in the month/year that was actually picked", async () => {
    const user = userEvent.setup();
    // Pick a date in the current month so the component opens on it.
    const now = new Date();
    const existing = new Date(now.getFullYear(), now.getMonth(), 15, 0, 0, 0).toISOString();

    render(DateTimePicker, {
      value: existing,
      has_time: false,
      onchange: vi.fn(),
      onclose: vi.fn(),
    });

    // The "15" button for the current month should be rendered with the
    // selected styling (bg-primary).
    const day15 = screen.getByRole("button", { name: "15" });
    expect(day15.className).toMatch(/bg-primary/);

    // Navigate one month forward. The same "15" cell must NOT be marked as
    // selected, because the user hasn't picked a day in that month yet.
    const nextMonthBtn = screen.getAllByRole("button").find((b) =>
      b.querySelector("svg path[d*='M7.21 14.77']"),
    ) as HTMLElement;
    await user.click(nextMonthBtn);

    const nextMonth15 = screen.getByRole("button", { name: "15" });
    expect(nextMonth15.className).not.toMatch(/bg-primary/);
  });

  it("commits based on the last-selected month, not the currently-viewed month", async () => {
    const user = userEvent.setup();
    const onchange = vi.fn();
    const onclose = vi.fn();

    // Start with April 10 selected (use a fixed month/year so the test is stable).
    const existing = new Date(2026, 3, 10, 0, 0, 0).toISOString();
    render(DateTimePicker, {
      value: existing,
      has_time: false,
      onchange,
      onclose,
    });

    // Pick the 20th while viewing April.
    await user.click(screen.getByRole("button", { name: "20" }));

    // Flip to May.
    const nextMonthBtn = screen.getAllByRole("button").find((b) =>
      b.querySelector("svg path[d*='M7.21 14.77']"),
    ) as HTMLElement;
    await user.click(nextMonthBtn);

    // Hit Done.
    await user.click(screen.getByRole("button", { name: "Done" }));

    expect(onchange).toHaveBeenCalled();
    const committed = new Date(onchange.mock.calls[0][0] as string);
    // April == month 3 (0-indexed). We navigated to May without reselecting,
    // so the committed date must still be April 20.
    expect(committed.getMonth()).toBe(3);
    expect(committed.getDate()).toBe(20);
    expect(committed.getFullYear()).toBe(2026);
  });
});
