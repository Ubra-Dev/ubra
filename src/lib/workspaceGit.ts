import type { LayoutNode, Workspace } from "./layout";

/**
 * The directory git status is read from for a workspace: the first pane
 * cwd in tab order. Null when no pane has a working directory.
 */
export function workspaceDir(ws: Workspace): string | null {
  for (const tab of ws.tabs) {
    const cwd = firstPaneCwd(tab.root);
    if (cwd !== null) return cwd;
  }
  return null;
}

function firstPaneCwd(node: LayoutNode): string | null {
  if (node.kind === "pane") {
    return node.cwd !== undefined && node.cwd.trim() !== "" ? node.cwd : null;
  }
  return firstPaneCwd(node.first) ?? firstPaneCwd(node.second);
}
