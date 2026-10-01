<script lang="ts">
  import { PUBLIC_POSTHOG_HOST, PUBLIC_POSTHOG_PROJECT_TOKEN } from "$env/static/public";
  import posthog from "posthog-js";
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import { agent } from "./agent.svelte";
  import {
    FLEET_CELEBRATED_KEY,
    FLEET_PROMPT_SETTLE_MS,
    FLEET_PROMPT_TIMEOUT_MS,
  } from "./fleet";
  import { findPane, findTabByPane } from "./layout";
  import { posthogLogs } from "./posthogLogs";
  import { store } from "./store.svelte";
  import { toasts } from "./toasts.svelte.ts";

  /** launchedAt of the fleet currently tracked; re-arms on change. */
  let armedAt = 0;
  /** Fleet panes still awaiting their starter prompt. */
  let pending = new Set<string>();
  let timers: ReturnType<typeof setTimeout>[] = [];
  let celebratedThisSession = false;

  function clearTimers(): void {
    for (const timer of timers) clearTimeout(timer);
    timers = [];
  }

  onDestroy(clearTimers);

  function paneAlive(nodeId: string): boolean {
    return !!store.layout && !!findTabByPane(store.layout, nodeId);
  }

  function paneLabel(nodeId: string): string {
    const found = store.layout && findTabByPane(store.layout, nodeId);
    const pane = found ? findPane(found.tab.root, nodeId) : null;
    return (
      agent.paneAgentLabel(nodeId) ??
      pane?.agentCli?.trim().split(/\s+/)[0] ??
      "Agent"
    );
  }

  /** Reserve a booted pane's prompt and send it after the settle delay. */
  function scheduleSend(nodeId: string): void {
    const prompt = store.takeFleetPrompt(nodeId);
    pending.delete(nodeId);
    if (!prompt) return;
    timers.push(
      setTimeout(() => {
        const liveId = agent.liveIdForNode(nodeId);
        if (!paneAlive(nodeId) || liveId === undefined) return;
        invoke("pty_write", { id: liveId, data: `${prompt}\r` }).catch((error: unknown) => {
          console.error("ubra: failed to send fleet starter prompt", error);
          toasts.push(
            `${paneLabel(nodeId)} needs attention`,
            "The starter task didn't send — type it in the terminal.",
            nodeId,
          );
        });
      }, FLEET_PROMPT_SETTLE_MS),
    );
  }

  /** Sweep panes whose CLI never booted; the user drives those by hand. */
  function sweepUnbooted(): void {
    for (const nodeId of [...pending]) {
      pending.delete(nodeId);
      store.takeFleetPrompt(nodeId);
      if (!paneAlive(nodeId)) continue;
      toasts.push(
        `${paneLabel(nodeId)} needs attention`,
        "It didn't start on its own — check the terminal, then type the task yourself.",
        nodeId,
      );
    }
    maybeDisarm();
  }

  function celebrate(finisher: string, launchedAt: number): void {
    celebratedThisSession = true;
    let firstEver = false;
    try {
      firstEver = window.localStorage.getItem(FLEET_CELEBRATED_KEY) === null;
      if (firstEver) window.localStorage.setItem(FLEET_CELEBRATED_KEY, "1");
    } catch (error: unknown) {
      console.error("ubra: fleet celebration flag failed", error);
    }
    if (firstEver) {
      const seconds = Math.max(1, Math.round((Date.now() - launchedAt) / 1000));
      toasts.push("Your fleet finished its first task", "Click to review", finisher, {
        dismissMs: 12000,
      });
      if (PUBLIC_POSTHOG_PROJECT_TOKEN && PUBLIC_POSTHOG_HOST) {
        posthog.capture("fleet_first_completed", { seconds });
        posthogLogs.fleetFirstCompleted(seconds);
      }
    }
    maybeDisarm();
  }

  function maybeDisarm(): void {
    if (pending.size > 0) return;
    if (!celebratedThisSession && [...(store.fleet?.nodeIds ?? [])].some(paneAlive)) {
      return;
    }
    clearTimers();
    armedAt = 0;
    store.disarmFleet();
  }

  $effect(() => {
    const fleet = store.fleet;
    if (!fleet) {
      clearTimers();
      pending = new Set();
      armedAt = 0;
      return;
    }
    if (fleet.launchedAt !== armedAt) {
      clearTimers();
      pending = new Set(fleet.nodeIds);
      armedAt = fleet.launchedAt;
      timers.push(setTimeout(sweepUnbooted, FLEET_PROMPT_TIMEOUT_MS));
    }
    for (const nodeId of [...pending]) {
      if (!paneAlive(nodeId)) {
        pending.delete(nodeId);
        store.takeFleetPrompt(nodeId);
        continue;
      }
      // An observed agent means the CLI booted past any splash/auth gate
      // the detector recognizes; unrecognized CLIs fall to the timeout sweep.
      if (agent.paneAgentLabel(nodeId)) scheduleSend(nodeId);
    }
    if (!celebratedThisSession) {
      const finisher = fleet.nodeIds.find(
        (nodeId) => agent.paneState(nodeId)?.state === "done",
      );
      if (finisher) celebrate(finisher, fleet.launchedAt);
      else maybeDisarm();
    } else {
      maybeDisarm();
    }
  });
</script>
