// Saved agent profiles and workspace templates: pure library (no Svelte/Tauri).
import {
  MAX_LAYOUT_BYTES,
  MAX_LAYOUT_ENTITIES,
  findPane,
  newId,
  sanitizeLayout,
  type Layout,
  type LayoutNode,
  type PaneNode,
  type Workspace,
} from "./layout.ts";

export const SAVED_SETUPS_VERSION = 1;

export interface AgentProfile {
  id: string;
  name: string;
  cwd: string;
  cmd: string[];
}

export interface WorkspaceTemplate {
  id: string;
  name: string;
  workspace: Workspace;
}

export interface SavedSetups {
  version: 1;
  profiles: AgentProfile[];
  templates: WorkspaceTemplate[];
}

export function emptySavedSetups(): SavedSetups {
  return { version: 1 as const, profiles: [], templates: [] };
}

function rejectNul(value: string, label: string): void {
  if (value.includes("\0")) throw new Error(`Invalid saved setups: ${label} must not contain NUL.`);
}

function sanitizeArgv(raw: unknown, label: string): string[] {
  if (!Array.isArray(raw) || raw.length === 0) {
    throw new Error(`Invalid saved setups: ${label} requires a program with arguments.`);
  }
  if (!raw.every((part) => typeof part === "string")) {
    throw new Error(`Invalid saved setups: ${label} must be strings.`);
  }
  const parts = raw as string[];
  const program = parts[0].trim();
  if (program.length === 0) {
    throw new Error(`Invalid saved setups: ${label} requires a nonblank program.`);
  }
  rejectNul(program, `${label} program`);
  for (const arg of parts.slice(1)) rejectNul(arg, `${label} argument`);
  return [program, ...parts.slice(1)];
}

function sanitizeProfile(v: unknown): AgentProfile {
  if (typeof v !== "object" || v === null || Array.isArray(v)) {
    throw new Error("Invalid saved setups: profile must be an object.");
  }
  const record = v as Record<string, unknown>;
  if (typeof record["id"] !== "string" || (record["id"] as string).trim().length === 0) {
    throw new Error("Invalid saved setups: profile id must be a nonblank string.");
  }
  if (typeof record["name"] !== "string" || (record["name"] as string).trim().length === 0) {
    throw new Error("Invalid saved setups: profile name must be a nonblank string.");
  }
  if (typeof record["cwd"] !== "string" || (record["cwd"] as string).trim().length === 0) {
    throw new Error("Invalid saved setups: profile folder is required.");
  }
  rejectNul(record["cwd"] as string, "profile folder");
  return {
    id: record["id"] as string,
    name: (record["name"] as string).trim(),
    cwd: record["cwd"] as string,
    cmd: sanitizeArgv(record["cmd"], "profile command"),
  };
}

function sanitizeTemplateNode(node: LayoutNode): LayoutNode {
  if (node.kind === "pane") {
    const out: PaneNode = { kind: "pane", id: node.id };
    if (node.title !== undefined) out.title = node.title;
    if (node.cwd !== undefined) {
      if (typeof node.cwd !== "string") throw new Error("Invalid saved setups: pane folder must be a string.");
      if (node.cwd.length !== 0) {
        rejectNul(node.cwd, "pane folder");
        out.cwd = node.cwd;
      }
    }
    if (node.cmd !== undefined) {
      if (!Array.isArray(node.cmd) || node.cmd.length !== 0) {
        out.cmd = sanitizeArgv(node.cmd, "pane command");
      }
    }
    if (out.cmd !== undefined) {
      if (node.cmdOnRestore !== undefined) {
        if (typeof node.cmdOnRestore !== "boolean") {
          throw new Error("Invalid saved pane command restore policy.");
        }
        out.cmdOnRestore = node.cmdOnRestore;
      } else {
        out.cmdOnRestore = false;
      }
    }
    return out;
  }
  return {
    kind: "split",
    id: node.id,
    dir: node.dir,
    sizes: [node.sizes[0], node.sizes[1]],
    first: sanitizeTemplateNode(node.first),
    second: sanitizeTemplateNode(node.second),
  };
}

function sanitizeTemplate(v: unknown): WorkspaceTemplate {
  if (typeof v !== "object" || v === null || Array.isArray(v)) {
    throw new Error("Invalid saved setups: template must be an object.");
  }
  const record = v as Record<string, unknown>;
  if (typeof record["id"] !== "string" || (record["id"] as string).trim().length === 0) {
    throw new Error("Invalid saved setups: template id must be a nonblank string.");
  }
  if (typeof record["name"] !== "string" || (record["name"] as string).trim().length === 0) {
    throw new Error("Invalid saved setups: template name must be a nonblank string.");
  }
  if (typeof record["workspace"] !== "object" || record["workspace"] === null || Array.isArray(record["workspace"])) {
    throw new Error("Invalid saved setups: template workspace must be an object.");
  }
  const workspaceRecord = record["workspace"] as Record<string, unknown>;
  const wrapped = {
    version: 2,
    workspaces: [record["workspace"]],
    activeWorkspaceId: workspaceRecord["id"],
  };
  const checked = sanitizeLayout(wrapped);
  const workspace = checked.workspaces[0];
  const normalizedTabs = workspace.tabs.map((tab) => ({
    ...tab,
    root: sanitizeTemplateNode(tab.root),
  }));
  const normalized: Workspace = { ...workspace, tabs: normalizedTabs };
  sanitizeLayout({ version: 2, workspaces: [normalized], activeWorkspaceId: normalized.id });
  return { id: record["id"] as string, name: (record["name"] as string).trim(), workspace: normalized };
}

export function sanitizeSavedSetups(raw: unknown): SavedSetups {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    throw new Error("Invalid saved setups: document must be an object.");
  }
  const value = raw as Record<string, unknown>;
  if (value["version"] !== SAVED_SETUPS_VERSION) {
    throw new Error(`Unsupported saved setups version: ${String(value["version"])}.`);
  }
  if (!Array.isArray(value["profiles"]) || !Array.isArray(value["templates"])) {
    throw new Error("Invalid saved setups: profiles and templates must be arrays.");
  }
  let bytes: number;
  try {
    bytes = new TextEncoder().encode(JSON.stringify(raw)).byteLength;
  } catch {
    throw new Error("Invalid saved setups: document cannot be serialized safely.");
  }
  if (bytes > MAX_LAYOUT_BYTES) {
    throw new Error(`Saved setups exceed the ${MAX_LAYOUT_BYTES} byte limit.`);
  }
  const profiles = (value["profiles"] as unknown[]).map(sanitizeProfile);
  const templates = (value["templates"] as unknown[]).map(sanitizeTemplate);
  if (profiles.length + templates.length > MAX_LAYOUT_ENTITIES) {
    throw new Error(`Saved setups exceed the ${MAX_LAYOUT_ENTITIES} entry limit.`);
  }
  const ids = new Set<string>();
  for (const entry of [...profiles, ...templates]) {
    if (ids.has(entry.id)) throw new Error("Invalid saved setups: duplicate saved entry id.");
    ids.add(entry.id);
  }
  const profileNames = new Set<string>();
  for (const p of profiles) {
    const key = p.name.toLowerCase();
    if (profileNames.has(key)) throw new Error("A profile with this name already exists.");
    profileNames.add(key);
  }
  const templateNames = new Set<string>();
  for (const t of templates) {
    const key = t.name.toLowerCase();
    if (templateNames.has(key)) throw new Error("A template with this name already exists.");
    templateNames.add(key);
  }
  return { version: 1, profiles, templates };
}

function copyWorkspaceFresh(workspace: Workspace): Workspace {
  const paneMap = new Map<string, string>();
  const mapNode = (node: LayoutNode): LayoutNode => {
    if (node.kind === "pane") {
      const id = newId("pane");
      paneMap.set(node.id, id);
      const out: PaneNode = { kind: "pane", id };
      if (node.title !== undefined) out.title = node.title;
      if (node.cwd !== undefined && node.cwd.length > 0) out.cwd = node.cwd;
      if (node.cmd !== undefined && node.cmd.length > 0) {
        out.cmd = [...node.cmd];
        out.cmdOnRestore = false;
      }
      return out;
    }
    return {
      kind: "split",
      id: newId("split"),
      dir: node.dir,
      sizes: [node.sizes[0], node.sizes[1]],
      first: mapNode(node.first),
      second: mapNode(node.second),
    };
  };
  const tabIds = new Map<string, string>();
  const tabs = workspace.tabs.map((tab) => {
    const id = newId("tab");
    tabIds.set(tab.id, id);
    const root = mapNode(tab.root);
    let zoomedPaneId: string | undefined;
    if (tab.zoomedPaneId) {
      const mapped = paneMap.get(tab.zoomedPaneId);
      if (mapped && findPane(root, mapped)) zoomedPaneId = mapped;
    }
    return { id, name: tab.name, root, ...(zoomedPaneId ? { zoomedPaneId } : {}) };
  });
  const activeTabId = tabIds.get(workspace.activeTabId) ?? tabs[0]?.id ?? newId("tab");
  return { id: newId("ws"), name: workspace.name, tabs, activeTabId };
}

export function captureWorkspaceTemplate(workspace: Workspace, name: string): WorkspaceTemplate {
  const trimmed = name.trim();
  if (trimmed.length === 0) throw new Error("Invalid saved setups: template name must be a nonblank string.");
  const detached = JSON.parse(JSON.stringify(workspace)) as Workspace;
  const stripEmpty = (node: LayoutNode): void => {
    if (node.kind === "pane") {
      if (node.cmd !== undefined && node.cmd.length === 0) delete node.cmd;
      if (node.cmd === undefined) delete node.cmdOnRestore;
      else node.cmdOnRestore = false;
      return;
    }
    stripEmpty(node.first);
    stripEmpty(node.second);
  };
  for (const tab of detached.tabs) stripEmpty(tab.root);
  const fresh = copyWorkspaceFresh(detached);
  fresh.name = detached.name;
  return { id: newId("template"), name: trimmed, workspace: fresh };
}

export function instantiateProfile(profile: AgentProfile): Workspace {
  const tabId = newId("tab");
  const paneId = newId("pane");
  return {
    id: newId("ws"),
    name: profile.name,
    tabs: [
      {
        id: tabId,
        name: profile.name,
        root: {
          kind: "pane",
          id: paneId,
          title: profile.name,
          cwd: profile.cwd,
          cmd: [...profile.cmd],
          cmdOnRestore: false,
        },
      },
    ],
    activeTabId: tabId,
  };
}

export function instantiateTemplate(template: WorkspaceTemplate): Workspace {
  const detached = JSON.parse(JSON.stringify(template.workspace)) as Workspace;
  const workspace = copyWorkspaceFresh(detached);
  workspace.name = template.workspace.name;
  sanitizeLayout({ version: 2, workspaces: [workspace], activeWorkspaceId: workspace.id });
  return workspace;
}

export function validateSetupInsertion(layout: Layout, workspace: Workspace): void {
  sanitizeLayout({
    version: 2,
    workspaces: [...layout.workspaces, workspace],
    activeWorkspaceId: workspace.id,
  });
}
