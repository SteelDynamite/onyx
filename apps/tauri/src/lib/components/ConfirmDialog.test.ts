import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import ConfirmDialog, { isConfirmDialogOpen } from "./ConfirmDialog.svelte";

beforeEach(() => {
  cleanup();
});

describe("ConfirmDialog", () => {
  it("renders the message, detail and custom confirm label", () => {
    render(ConfirmDialog, {
      message: "Delete task?",
      detail: "This cannot be undone.",
      confirmText: "Delete",
      onconfirm: vi.fn(),
      oncancel: vi.fn(),
    });
    expect(screen.getByText("Delete task?")).toBeInTheDocument();
    expect(screen.getByText("This cannot be undone.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("fires oncancel when Cancel is clicked", async () => {
    const user = userEvent.setup();
    const oncancel = vi.fn();
    render(ConfirmDialog, {
      message: "Delete?",
      onconfirm: vi.fn(),
      oncancel,
    });
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(oncancel).toHaveBeenCalledTimes(1);
  });

  it("fires onconfirm when Confirm is clicked and not oncancel", async () => {
    const user = userEvent.setup();
    const onconfirm = vi.fn();
    const oncancel = vi.fn();
    render(ConfirmDialog, {
      message: "Delete?",
      confirmText: "Delete",
      onconfirm,
      oncancel,
    });
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(onconfirm).toHaveBeenCalledTimes(1);
    expect(oncancel).not.toHaveBeenCalled();
  });

  it("cancels and stops propagation on Escape (regression: used to bubble and pop task detail)", async () => {
    const oncancel = vi.fn();
    // An outer bubble-phase listener emulates TasksScreen's svelte:window
    // Escape handler. If the dialog leaks Escape, this spy fires too.
    const outer = vi.fn();
    window.addEventListener("keydown", outer);
    try {
      render(ConfirmDialog, {
        message: "Delete?",
        onconfirm: vi.fn(),
        oncancel,
      });
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      expect(oncancel).toHaveBeenCalledTimes(1);
      expect(outer).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener("keydown", outer);
    }
  });

  it("ignores non-Escape keydowns", async () => {
    const oncancel = vi.fn();
    render(ConfirmDialog, {
      message: "Delete?",
      onconfirm: vi.fn(),
      oncancel,
    });
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(oncancel).not.toHaveBeenCalled();
  });

  it("increments the open-count singleton so parent Escape handlers can defer", () => {
    expect(isConfirmDialogOpen()).toBe(false);
    const { unmount } = render(ConfirmDialog, {
      message: "Delete?",
      onconfirm: vi.fn(),
      oncancel: vi.fn(),
    });
    expect(isConfirmDialogOpen()).toBe(true);
    unmount();
    expect(isConfirmDialogOpen()).toBe(false);
  });

  it("tracks multiple concurrently-mounted dialogs and releases on unmount", () => {
    const a = render(ConfirmDialog, { message: "A?", onconfirm: vi.fn(), oncancel: vi.fn() });
    const b = render(ConfirmDialog, { message: "B?", onconfirm: vi.fn(), oncancel: vi.fn() });
    expect(isConfirmDialogOpen()).toBe(true);
    a.unmount();
    expect(isConfirmDialogOpen()).toBe(true);
    b.unmount();
    expect(isConfirmDialogOpen()).toBe(false);
  });
});
