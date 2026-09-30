import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  collectPaneIds,
  defaultWorkspace,
  findPane,
  gridTab,
  sanitizeLayout,
  type Workspace,
} from "../src/lib/layout.ts";
import {
  captureWorkspaceTemplate,
  emptySavedSetups,
  instantiateProfile,
  instantiateTemplate,
  sanitizeSavedSetups,
  validateSetupInsertion,
} from "../src/lib/savedSetups.ts";

function twoTabWorkspace(): Workspace {
  const ws = defaultWorkspace("Source");
  const second = gridTab("Second");
  ws.tabs.push(second);
  ws.activeTabId = second.id;
  const panes = collectPaneIds(second.root);
  second.zoomedPaneId = panes[1];
  const first = ws.tabs[0].root;
  if (first.kind === "pane") {
    first.cwd = "/tmp/project";
    first.cmd = ["node", "-e", "console.log(1)"];
  }
  return ws;
}

function allIds(ws: Workspace): string[] {
  return [
    ws.id,
    ...ws.tabs.flatMap((t) => [t.id, ...collectPaneIds(t.root)]),
  ];
}

describe("capture and instantiate", () => {
  it("keeps topology/selection across definition and two instances with disjoint identities", () => {
    const original = twoTabWorkspace();
    const template = captureWorkspaceTemplate(
      JSON.parse(JSON.stringify(original)),
      "  Smoke workspace  ",
    );
    assert.equal(template.name, "Smoke workspace");
    const first = instantiateTemplate(template);
    const second = instantiateTemplate(template);
    const groups = [
      allIds(original),
      allIds(template.workspace),
      allIds(first),
      allIds(second),
    ];
    const seen = new Set<string>();
    for (const group of groups) {
      for (const id of group) {
        assert.ok(!seen.has(id), `duplicate identity ${id}`);
        seen.add(id);
      }
    }
    for (const ws of [template.workspace, first, second]) {
      assert.equal(ws.tabs.length, 2);
      assert.equal(ws.tabs[1].name, "Second");
      assert.equal(ws.activeTabId, ws.tabs[1].id);
      const zoom = ws.tabs[1].zoomedPaneId;
      assert.ok(zoom);
      assert.ok(zoom && findPane(ws.tabs[1].root, zoom));
    }
    const defCmd = findPane(
      template.workspace.tabs[0].root,
      collectPaneIds(template.workspace.tabs[0].root)[0],
    );
    assert.equal(defCmd?.kind, "pane");
    if (defCmd?.kind === "pane") assert.equal(defCmd.cmdOnRestore, false);
  });

  it("isolates mutation between instances and the definition", () => {
    const original = twoTabWorkspace();
    const template = captureWorkspaceTemplate(
      JSON.parse(JSON.stringify(original)),
      "isolated",
    );
    const before = JSON.parse(JSON.stringify(template));
    const first = instantiateTemplate(template);
    const second = instantiateTemplate(template);
    const pane = findPane(first.tabs[0].root, collectPaneIds(first.tabs[0].root)[0]);
    assert.equal(pane?.kind, "pane");
    if (pane?.kind === "pane") {
      pane.cmd = ["changed"];
      pane.cwd = "/elsewhere";
    }
    const other = findPane(second.tabs[0].root, collectPaneIds(second.tabs[0].root)[0]);
    assert.notDeepEqual(other?.kind === "pane" ? other.cmd : null, ["changed"]);
    assert.deepEqual(template, before);
  });

  it("instantiates a profile as a manual one-pane workspace", () => {
    const ws = instantiateProfile({
      id: "profile-1",
      name: "Smoke agent",
      cwd: "/tmp/project",
      cmd: ["node", "-e", "run"],
    });
    assert.equal(ws.tabs.length, 1);
    const root = ws.tabs[0].root;
    assert.equal(root.kind, "pane");
    if (root.kind !== "pane") throw new Error("Expected pane");
    assert.equal(root.cwd, "/tmp/project");
    assert.deepEqual(root.cmd, ["node", "-e", "run"]);
    assert.equal(root.cmdOnRestore, false);
    assert.notEqual(ws.id, "profile-1");
  });
  it("rejects insertion beyond aggregate limits without mutating the layout", () => {
    const layout = sanitizeLayout(JSON.parse(JSON.stringify({
      version: 2,
      activeWorkspaceId: "w",
      workspaces: [{ id: "w", name: "w", activeTabId: "t", tabs: [{ id: "t", name: "t", root: { kind: "pane", id: "p" } }] }],
    })));
    const before = JSON.parse(JSON.stringify(layout));
    const outsider = defaultWorkspace("outsider");
    outsider.id = "p";
    assert.throws(() => validateSetupInsertion(layout, outsider), /duplicate identity/);
    assert.deepEqual(layout, before);
  });
});

describe("saved setups validation", () => {
  it("keeps literal spaces and empty arguments while trimming program and name", () => {
    const doc = sanitizeSavedSetups({
      version: 1,
      profiles: [{
        id: "p1",
        name: "  agent  ",
        cwd: "/tmp",
        cmd: ["  node  ", "--", "argument with spaces", "$(not-a-shell-command)", ""],
      }],
      templates: [],
    });
    assert.equal(doc.profiles[0].name, "agent");
    assert.deepEqual(doc.profiles[0].cmd, ["node", "--", "argument with spaces", "$(not-a-shell-command)", ""]);
  });

  it("rejects bad commands, duplicate ids, name collisions, and bad versions", () => {
    const profile = { id: "p1", name: "agent", cwd: "/tmp", cmd: ["sh"] };
    assert.throws(() => sanitizeSavedSetups({
      version: 1,
      profiles: [{ ...profile, cmd: [] }],
      templates: [],
    }));
    assert.throws(() => sanitizeSavedSetups({
      version: 1,
      profiles: [profile, { ...profile, cwd: "/other" }],
      templates: [],
    }));
    assert.throws(
      () => sanitizeSavedSetups({
        version: 1,
        profiles: [profile, { id: "p2", name: "AGENT", cwd: "/tmp", cmd: ["sh"] }],
        templates: [],
      }),
      /profile with this name/,
    );
    const template = captureWorkspaceTemplate(twoTabWorkspace(), "t1");
    assert.throws(
      () =>
        sanitizeSavedSetups({
          version: 1,
          profiles: [profile],
          templates: [{ ...template, id: "p1" }],
        }),
      /duplicate saved entry id/,
    );
    assert.throws(() => sanitizeSavedSetups({ version: 2, profiles: [], templates: [] }), /Unsupported/);
    assert.deepEqual(sanitizeSavedSetups(emptySavedSetups()), emptySavedSetups());
  });

  it("rejects templates with bad active or zoom references", () => {
    const template = captureWorkspaceTemplate(twoTabWorkspace(), "bad");
    const broken = JSON.parse(JSON.stringify(template));
    broken.workspace.activeTabId = "missing";
    assert.throws(() => sanitizeSavedSetups({ version: 1, profiles: [], templates: [broken] }), /active tab/);
    const brokenZoom = JSON.parse(JSON.stringify(template));
    brokenZoom.workspace.tabs[1].zoomedPaneId = "missing";
    assert.throws(() => sanitizeSavedSetups({ version: 1, profiles: [], templates: [brokenZoom] }), /zoomed pane/);
  });

  it("treats an empty template cwd as the default directory", () => {
    const template = captureWorkspaceTemplate(twoTabWorkspace(), "cwd");
    const raw = JSON.parse(JSON.stringify(template));
    const paneId = collectPaneIds(raw.workspace.tabs[0].root)[0];
    const node = findPane(raw.workspace.tabs[0].root, paneId);
    assert.equal(node?.kind, "pane");
    if (node?.kind === "pane") node.cwd = "";
    const doc = sanitizeSavedSetups({ version: 1, profiles: [], templates: [raw] });
    const saved = findPane(
      doc.templates[0].workspace.tabs[0].root,
      collectPaneIds(doc.templates[0].workspace.tabs[0].root)[0],
    );
    assert.equal(saved?.kind, "pane");
    if (saved?.kind === "pane") assert.equal(saved.cwd, undefined);
  });
});
