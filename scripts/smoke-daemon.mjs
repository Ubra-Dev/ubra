// Daemon launch/quit/reattach smoke: boots a real `ubra-daemon` on scratch
// dirs, attaches a pane, checkpoints, SIGKILLs the daemon (abrupt loss),
// reattaches after restart, and verifies recovery + closure semantics.
// Cross-platform (no shell-specific commands). Exits non-zero on failure.
import { spawn, execFileSync } from "node:child_process";
import { createConnection } from "node:net";
import { createHmac, randomBytes } from "node:crypto";
import { mkdtempSync, readFileSync, readdirSync, rmSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const srcTauri = join(root, "..", "src-tauri");
const exe = process.platform === "win32" ? ".exe" : "";
const profile = process.env.SMOKE_PROFILE ?? "debug";
const targetDir = process.env.CARGO_TARGET_DIR ?? join(srcTauri, "target");
const daemonBin = join(targetDir, profile, `ubra-daemon${exe}`);

function fail(message) {
  console.error(`smoke FAILED: ${message}`);
  process.exitCode = 1;
  throw new Error(message);
}

function assert(cond, message) {
  if (!cond) fail(message);
  console.log(`ok: ${message}`);
}

execFileSync("cargo", ["build", "--bins", ...(profile === "release" ? ["--release"] : [])], {
  cwd: srcTauri,
  stdio: "inherit",
});

const scratch = mkdtempSync(join(tmpdir(), "ubra-smoke-"));
const stateDir = join(scratch, "state");
const dataDir = join(scratch, "data");
let daemon = null;

function startDaemon() {
  daemon = spawn(daemonBin, ["--state-dir", stateDir], {
    env: { ...process.env, UBRA_DATA_DIR: dataDir },
    stdio: "ignore",
    detached: process.platform !== "win32",
  });
  daemon.unref();
}

function stopDaemon(signal) {
  if (!daemon) return;
  try {
    if (signal === "kill") daemon.kill("SIGKILL");
    else daemon.kill();
  } catch {}
  daemon = null;
}

async function waitForFile(path, timeoutMs) {
  const start = Date.now();
  for (;;) {
    try {
      return readFileSync(path, "utf8");
    } catch {
      if (Date.now() - start > timeoutMs) fail(`timed out waiting for ${path}`);
      await new Promise((r) => setTimeout(r, 50));
    }
  }
}

function proof(token, role, client, server) {
  return createHmac("sha256", token).update(`ubra-v3:${role}:${client}:${server}`).digest("hex");
}

class Client {
  constructor(port, token) {
    this.port = port;
    this.token = token;
    this.nextId = 1;
    this.buffer = "";
    this.waiters = [];
  }

  connect() {
    return new Promise((resolve, reject) => {
      this.socket = createConnection({ host: "127.0.0.1", port: this.port }, () => resolve());
      this.socket.on("data", (chunk) => this.onData(chunk));
      this.socket.on("error", reject);
    });
  }

  onData(chunk) {
    this.buffer += chunk.toString("utf8");
    let idx;
    while ((idx = this.buffer.indexOf("\n")) >= 0) {
      const line = this.buffer.slice(0, idx);
      this.buffer = this.buffer.slice(idx + 1);
      const waiter = this.waiters.shift();
      if (waiter) waiter(line);
    }
  }

  readLine() {
    return new Promise((resolve) => this.waiters.push(resolve));
  }

  async handshake() {
    const clientNonce = randomBytes(32).toString("hex");
    this.socket.write(JSON.stringify({ op: "hello", id: 0, nonce: clientNonce }) + "\n");
    const challenge = JSON.parse(await this.readLine());
    assert(challenge.protocol === 3, `daemon speaks protocol 3 (got ${challenge.protocol})`);
    const serverNonce = challenge.nonce;
    const expected = proof(this.token, "server", clientNonce, serverNonce);
    assert(challenge.proof === expected, "daemon identity proof verifies");
    this.socket.write(
      JSON.stringify({ op: "auth", id: 0, proof: proof(this.token, "client", clientNonce, serverNonce) }) + "\n",
    );
    const reply = JSON.parse(await this.readLine());
    assert(reply.ok === true && reply.protocol === 3, "client authentication accepted");
  }

  async request(op) {
    const id = this.nextId++;
    this.socket.write(JSON.stringify({ ...op, id }) + "\n");
    for (;;) {
      const value = JSON.parse(await this.readLine());
      if (value.event || value.id !== id) continue;
      return value;
    }
  }

  close() {
    try {
      this.socket.destroy();
    } catch {}
  }
}

async function openClient() {
  const portFile = JSON.parse(await waitForFile(join(stateDir, "daemon.json"), 10000));
  const token = (await waitForFile(join(stateDir, "daemon.auth"), 10000)).trim();
  const client = new Client(portFile.port, token);
  await client.connect();
  await client.handshake();
  return client;
}

try {
  // --- Lifetime 1: attach, run output, checkpoint.
  startDaemon();
  let client = await openClient();
  const ping = await client.request({ op: "ping" });
  assert(ping.ok && typeof ping.epoch === "number", "ping reports epoch");
  assert(Number.isSafeInteger(ping.epoch) && ping.epoch >= 1, "epoch is a JS-safe integer");
  const epoch1 = ping.epoch;

  const attach = await client.request({
    op: "pty_attach",
    key: "pane-smoke",
    cols: 80,
    rows: 24,
    frontend: true,
  });
  assert(attach.ok && attach.attached === "created", `fresh key creates (got ${attach.attached})`);
  const pane = attach.pane;
  assert(pane > 0 && attach.epoch === epoch1, "attach carries epoch + pane id");
  const incarnation = attach.incarnation;
  assert(Number.isSafeInteger(incarnation) && incarnation >= 1, "attach carries incarnation");

  const write = await client.request({ op: "pty_write", pane, data: "echo smoke-marker-42\n" });
  assert(write.ok, "write accepted");

  // Wait for the marker to land, then snapshot + flush.
  let seen = false;
  for (let i = 0; i < 50; i++) {
    const read = await client.request({ op: "pty_read", pane });
    if (read.ok && read.screen.includes("smoke-marker-42")) {
      seen = true;
      break;
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  assert(seen, "pane ran the command");

  // Mirror the frontend: echo the attach-time epoch/incarnation back so the
  // freshness check runs exactly as it does for a real pane.
  const snap = await client.request({ op: "pty_snapshot", pane, epoch: epoch1, incarnation });
  assert(snap.ok && snap.pages >= 1 && snap.data.length > 0, "paged snapshot serves content");
  if (snap.pages > 1) {
    const page = await client.request({ op: "pty_snapshot_page", pane, page: 1, epoch: epoch1, incarnation });
    assert(page.ok && page.page === 1, "snapshot page 1 serves");
  }

  const flush = await client.request({ op: "flush" });
  assert(flush.ok && flush.panes >= 1, "explicit flush checkpoints panes");
  const records = readdirSync(join(dataDir, "recovery"));
  assert(records.length === 1, `one recovery record persisted (got ${records.length})`);

  // Stale incarnation must not affect the live pane.
  const stale = await client.request({ op: "pty_write", pane, data: "x", incarnation: 999999 });
  assert(!stale.ok && /stale/.test(stale.error), "stale incarnation rejected");

  // --- Abrupt daemon loss + restart: recovery without rerunning.
  client.close();
  stopDaemon("kill");
  await new Promise((r) => setTimeout(r, 500));
  // A killed daemon leaves stale runtime files; drop them so the client
  // below waits for the restarted daemon's fresh port + credentials.
  for (const name of ["daemon.json", "daemon.auth"]) {
    try {
      unlinkSync(join(stateDir, name));
    } catch {}
  }
  startDaemon();
  client = await openClient();
  const ping2 = await client.request({ op: "ping" });
  assert(ping2.ok && ping2.epoch !== epoch1, "restarted daemon has a new epoch");

  const reattach = await client.request({
    op: "pty_attach",
    key: "pane-smoke",
    cols: 80,
    rows: 24,
    frontend: true,
  });
  assert(
    reattach.ok && reattach.attached === "recovered" && reattach.resumed === false,
    `restart recovers shell without rerunning (got ${reattach.attached}/${reattach.resumed})`,
  );
  assert(typeof reattach.note === "string" || reattach.note == null, "recovery note present or absent cleanly");

  // Old pane id + epoch must not touch the replacement.
  const crossWrite = await client.request({ op: "pty_write", pane, data: "x", epoch: epoch1 });
  assert(!crossWrite.ok, "cross-epoch write rejected");

  // --- Closure removes runtime + recovery state.
  const commit = await client.request({ op: "layout_commit", layout: { version: 2, workspaces: [] } });
  assert(commit.ok && commit.closed.includes("pane-smoke"), "layout removal closes the pane");
  const recordsAfter = readdirSync(join(dataDir, "recovery"));
  assert(recordsAfter.length === 0, "layout removal drops the recovery record");

  const reattachClosed = await client.request({ op: "pty_attach", key: "pane-smoke", cols: 80, rows: 24 });
  assert(reattachClosed.ok && reattachClosed.attached === "created", "closed pane never returns; reattach is fresh");

  const stopAll = await client.request({ op: "stop_all" });
  assert(stopAll.ok && stopAll.stopped >= 1, "stop-all terminates sessions");

  const shutdown = await client.request({ op: "shutdown" });
  assert(shutdown.ok && shutdown.shutdown === true, "orderly shutdown acknowledged");
  client.close();
  daemon = null;
  console.log("smoke PASSED");
} finally {
  stopDaemon("kill");
  rmSync(scratch, { recursive: true, force: true });
}
