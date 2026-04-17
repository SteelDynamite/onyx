/**
 * Derive a workspace display name from a folder path picked via the file
 * dialog. Handles both `/` and `\` separators and tolerates trailing
 * separators (e.g. `"/home/me/Tasks/"` → `"Tasks"`, not `"workspace"`).
 */
export function workspaceNameFromPath(folder: string): string {
  const parts = folder.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || "workspace";
}
