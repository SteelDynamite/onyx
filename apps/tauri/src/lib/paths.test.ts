import { describe, it, expect } from "vitest";
import { workspaceNameFromPath } from "./paths";

describe("workspaceNameFromPath", () => {
  it("returns the last path component of a POSIX path", () => {
    expect(workspaceNameFromPath("/home/me/Tasks")).toBe("Tasks");
  });

  it("strips a trailing slash (regression: used to fall back to 'workspace')", () => {
    expect(workspaceNameFromPath("/home/me/Tasks/")).toBe("Tasks");
  });

  it("strips multiple trailing slashes", () => {
    expect(workspaceNameFromPath("/home/me/Tasks///")).toBe("Tasks");
  });

  it("handles Windows-style backslash paths", () => {
    expect(workspaceNameFromPath("C:\\Users\\me\\Tasks")).toBe("Tasks");
  });

  it("strips a trailing backslash on Windows paths", () => {
    expect(workspaceNameFromPath("C:\\Users\\me\\Tasks\\")).toBe("Tasks");
  });

  it("handles mixed separators", () => {
    expect(workspaceNameFromPath("C:\\Users/me\\Tasks")).toBe("Tasks");
  });

  it("falls back to 'workspace' when the path has no usable tail", () => {
    expect(workspaceNameFromPath("/")).toBe("workspace");
    expect(workspaceNameFromPath("\\")).toBe("workspace");
    expect(workspaceNameFromPath("")).toBe("workspace");
  });

  it("preserves names with spaces", () => {
    expect(workspaceNameFromPath("/home/me/My Tasks/")).toBe("My Tasks");
  });
});
