import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, fstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, rmdirSync, utimesSync, writeFileSync } from "node:fs";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { acquireRuntimeLock, attachExtension, browserArgs, browserCdp, cleanupBrowserLogs, closeRuntimeSession, connectAndCreate, createWithRuntimeWorker, enableProfileExtensions, enhanceStagehandPage, ensureBrowser, ensureExtensions, extensionIdFor, formatSessionSummary, holdRuntimeWorker, openStagehand, optionalStateWrite, runDoctor, runtimeLockDir, runtimeResetForRecoveredOwner, sessionStats, STAGEHAND_EXTENSION_DIR } from "./stagehand.mjs";

test("browser log sweep removes only expired port logs", () => {
  const dir = mkdtempSync(join(tmpdir(), "run-cdp-sweep-"));
  try {
    const old = join(dir, "run_cdp-9222.log");
    const recent = join(dir, "run_cdp-9333.log");
    const unrelated = join(dir, "run_cdp-other.log");
    for (const path of [old, recent, unrelated]) writeFileSync(path, "log");
    const now = Date.now();
    utimesSync(old, (now - 15 * 86400000) / 1000, (now - 15 * 86400000) / 1000);
    cleanupBrowserLogs(dir, now);
    assert.equal(existsSync(old), false);
    assert.equal(existsSync(recent), true);
    assert.equal(existsSync(unrelated), true);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

const runtimeUrl = `chrome-extension://${extensionIdFor(STAGEHAND_EXTENSION_DIR)}/service-worker.js`;
const worker = (targetId) => ({ targetId, type: "service_worker", url: runtimeUrl, attached: false, browserContextId: "context-1" });
const temporaryLockDir = () => {
  const root = join(tmpdir(), "access-stagehand-test");
  mkdirSync(root, { recursive: true });
  const dir = mkdtempSync(join(root, "runtime-lock-"));
  return { dir, clean: () => { rmSync(dir, { recursive: true, force: true }); try { rmdirSync(root); } catch {} } };
};

test("target failures re-query twice and reconnect with fresh browser handles", async () => {
  let connects = 0;
  let creates = 0;
  let refreshes = 0;
  const result = await connectAndCreate({
    connect: async () => { connects++; return {}; },
    create: async () => { if (++creates < 3) throw new Error("No target with given id"); return { ready: true }; },
    refreshTarget: async () => { refreshes++; },
    restartRuntime: async () => { throw new Error("unexpected restart"); },
    doctor: async () => {},
    sleep: async (ms) => { assert.equal(ms, 300); },
  });
  assert.deepEqual([connects, creates, refreshes, result.retries], [3, 3, 2, 2]);
  await assert.rejects(connectAndCreate({
    connect: async () => ({}), create: async () => { throw new Error("No target with given id"); },
    refreshTarget: async () => {}, restartRuntime: async () => {}, doctor: async () => {}, sleep: async () => {},
  }), /stagehand-create-target/);
});

test("already initialized restarts Runtime once and reports the final boundary", async () => {
  let restarts = 0;
  await assert.rejects(connectAndCreate({
    connect: async () => ({}), create: async () => { throw new Error("already initialized"); },
    refreshTarget: async () => {}, restartRuntime: async () => { restarts++; }, doctor: async () => {}, sleep: async () => {},
  }), /runtime-already-initialized/);
  assert.equal(restarts, 1);
});

test("already initialized restarts Runtime before creating on a fresh handle", async () => {
  const order = [];
  const handles = [];
  const result = await connectAndCreate({
    connect: async () => {
      const handle = { id: handles.length + 1 };
      handles.push(handle);
      order.push(`connect-${handle.id}`);
      return handle;
    },
    create: async (handle) => {
      order.push(`create-${handle.id}`);
      if (handle.id === 1) throw new Error("already initialized");
      return { browser: handle };
    },
    refreshTarget: async () => { throw new Error("unexpected refresh"); },
    restartRuntime: async () => { order.push("restart-runtime"); },
    doctor: async () => { throw new Error("unexpected doctor"); },
  });
  assert.deepEqual(order, ["connect-1", "create-1", "restart-runtime", "connect-2", "create-2"]);
  assert.equal(handles.length, 2);
  assert.notEqual(handles[0], handles[1]);
  assert.equal(result.stagehand.browser, handles[1]);
});

test("already attached to a Stagehand instance reconnects without a Runtime restart", async () => {
  let connects = 0;
  let restarts = 0;
  const result = await connectAndCreate({
    connect: async () => ({ id: ++connects }),
    create: async (handle) => {
      if (handle.id === 1) throw new Error("This browser is already attached to a Stagehand instance");
      return { browser: handle };
    },
    refreshTarget: async () => {}, restartRuntime: async () => { restarts++; }, doctor: async () => {},
  });
  assert.equal(connects, 2);
  assert.equal(restarts, 0);
  assert.equal(result.stagehand.browser.id, 2);
});

test("create deadline does not start another create while the first is unsettled", async () => {
  let creates = 0;
  let doctors = 0;
  let restarts = 0;
  await assert.rejects(connectAndCreate({
    connect: async () => ({}), create: async () => { creates++; return new Promise(() => {}); },
    refreshTarget: async () => {}, doctor: async () => { doctors++; }, restartRuntime: async () => { restarts++; },
    createTimeoutMs: 5, createSettleTimeoutMs: 5, totalTimeoutMs: 1000,
  }), /stagehand-create-unsettled/);
  assert.deepEqual([creates, doctors, restarts], [1, 0, 0]);
});

test("create timeout waits for the pending call to settle before returning", async () => {
  let settle;
  let finished = false;
  const result = connectAndCreate({
    connect: async () => ({}),
    create: async () => new Promise((resolve) => { settle = () => { finished = true; resolve({ close: async () => {} }); }; }),
    refreshTarget: async () => {}, doctor: async () => {}, restartRuntime: async () => {},
    createTimeoutMs: 5, createSettleTimeoutMs: 100, totalTimeoutMs: 1000,
  });
  setTimeout(() => settle(), 20);
  await assert.rejects(result, /stagehand-create-timeout/);
  assert.equal(finished, true);
});

test("internal browser pages are rejected before opening a Stagehand session", async () => {
  await assert.rejects(openStagehand({ url: "chrome://extensions" }), /stagehand-internal-page-unsupported: chrome:/);
});

test("session summary shape and stats percentiles are stable", () => {
  const { dir, clean } = temporaryLockDir();
  try {
    const file = join(dir, "sessions.jsonl");
    const keys = ["lockWaitMs", "ensureMs", "doctorMs", "connectMs", "createMs", "pageMs"];
    const rows = [1, 2, 3, 4, 5].map((number) => ({
      ts: "2026-10-31T00:00:00.000Z", ...Object.fromEntries(keys.map((key) => [key, number * 10])),
      retries: number - 1, tabs: number, mode: "attach", result: number > 3 ? "stagehand-create-timeout" : "ok",
    }));
    writeFileSync(file, rows.map((row) => JSON.stringify(row)).join("\n") + "\n");
    const metrics = Object.fromEntries(keys.map((key) => [key, 10]));
    assert.equal(formatSessionSummary(metrics, 0, 1, "attach"), '[stagehand-summary] {"lockWaitMs":10,"ensureMs":10,"doctorMs":10,"connectMs":10,"createMs":10,"pageMs":10,"retries":0,"tabs":1,"mode":"attach"}');
    assert.deepEqual(sessionStats(file, 3), {
      count: 3, stages: Object.fromEntries(keys.map((key) => [key, { p50: 40, p90: 50 }])),
      errors: [{ boundary: "stagehand-create-timeout", count: 2 }],
    });
  } finally { clean(); }
});

test("lock directory defaults to shared temp and honors the override", () => {
  assert.equal(runtimeLockDir({}, "/tmp"), "/tmp/access-stagehand-locks");
  assert.equal(runtimeLockDir({ STAGEHAND_LOCK_DIR: "/tmp/custom-locks" }, "/tmp"), "/tmp/custom-locks");
});

test("openStagehand returns and closes a session with no lease timeout", async () => {
  const { dir, clean } = temporaryLockDir();
  const previousLease = process.env.STAGEHAND_LEASE_TIMEOUT_MS;
  delete process.env.STAGEHAND_LEASE_TIMEOUT_MS;
  let session;
  let closes = 0;
  try {
    const page = { url: async () => "about:blank", goto: async () => {} };
    const context = { pages: async () => [page], newPage: async () => page };
    const browser = { context };
    const fakeStagehand = { browser, close: async () => { closes++; } };
    const cdp = {
      call: async (method) => method === "Target.getTargets"
        ? { result: { targetInfos: [{ type: "page", targetId: "blank", url: "about:blank" }] } }
        : { result: {} },
      close: () => {},
    };
    session = await openStagehand({ url: "about:blank", port: 19333, runtimeAdapters: {
      lockDir: dir,
      sessionLogFile: join(dir, "sessions.jsonl"),
      env: {},
      entry: { version: "test" },
      browserRuntime: { localBrowser: { connect: async () => browser }, Stagehand: { create: async () => fakeStagehand } },
      endpointVersion: async () => ({ Browser: "Fake", webSocketDebuggerUrl: "ws://fake" }),
      browserCdp: async () => cdp,
      ensureBrowser: async () => ({ version: { Browser: "Fake" }, flags: {} }),
      ensureExtensions: async () => ({ restarted: false }),
      attachExtension: async () => ({ extensionId: "fake" }),
      runDoctor: async () => ({}),
      createWithRuntimeWorker: async (_cdp, create) => create({}),
    } });
    assert.equal(session.mode, "attach");
    assert.equal(session.page, page);
    await session.close();
    assert.equal(closes, 1);
  } finally {
    if (session) await session.close();
    if (previousLease === undefined) delete process.env.STAGEHAND_LEASE_TIMEOUT_MS;
    else process.env.STAGEHAND_LEASE_TIMEOUT_MS = previousLease;
    clean();
  }
});

test("socket lock releases only after the session closes", async () => {
  const { dir, clean } = temporaryLockDir();
  try {
    const lock = await acquireRuntimeLock(9222, { lockDir: dir, pid: 111, now: () => 500 });
    let finishClose;
    const closing = closeRuntimeSession({ close: () => new Promise((resolve) => { finishClose = resolve; }) }, lock);
    let acquired = false;
    const nextPromise = acquireRuntimeLock(9222, { lockDir: dir, pid: 222, maxWaitMs: 2000 }).then((value) => { acquired = true; return value; });
    await new Promise((resolve) => setTimeout(resolve, 20));
    assert.equal(acquired, false);
    finishClose();
    await closing;
    const next = await nextPromise;
    await next.release();
  } finally { clean(); }
});

test("second session receives owner metadata and wakes on close", async () => {
  const { dir, clean } = temporaryLockDir();
  try {
    const first = await acquireRuntimeLock(9222, { lockDir: dir, pid: 111, stage: () => "create", now: () => 500 });
    const waits = [];
    const secondPromise = acquireRuntimeLock(9222, {
      lockDir: dir, pid: 222, onWait: (owner) => waits.push(owner), waitLogMs: 10, maxWaitMs: 2000,
    });
    await new Promise((resolve) => setTimeout(resolve, 30));
    assert.ok(waits.some((owner) => owner.pid === 111 && owner.stage === "create"));
    await closeRuntimeSession({ close: async () => {} }, first);
    const second = await secondPromise;
    await second.release();
  } finally { clean(); }
});

test("SIGKILL owner socket is reclaimed immediately", async () => {
  const { dir, clean } = temporaryLockDir();
  try {
    const child = spawn(process.execPath, ["--input-type=module", "-e", `import { acquireRuntimeLock } from ${JSON.stringify(new URL("./stagehand.mjs", import.meta.url).href)}; const lock = await acquireRuntimeLock(9222, { lockDir: ${JSON.stringify(dir)} }); process.stdout.write("ready\\n");`], { stdio: ["ignore", "pipe", "pipe"] });
    await once(child.stdout, "data");
    const exited = once(child, "exit");
    child.kill("SIGKILL");
    await exited;
    const started = Date.now();
    const lock = await acquireRuntimeLock(9222, { lockDir: dir, pid: 444 });
    assert.equal(lock.recoveredStaleOwner, true);
    assert.ok(Date.now() - started < 200);
    await lock.release();
  } finally { clean(); }
});

test("SIGTERM closes session before releasing socket", async () => {
  const { dir, clean } = temporaryLockDir();
  try {
    const child = spawn(process.execPath, ["--input-type=module", "-e", `import { acquireRuntimeLock, installRuntimeSignalHandlers } from ${JSON.stringify(new URL("./stagehand.mjs", import.meta.url).href)}; const lock = await acquireRuntimeLock(9222, { lockDir: ${JSON.stringify(dir)} }); installRuntimeSignalHandlers(() => ({ close: async () => { process.stdout.write("closed\\n"); } }), lock); process.stdout.write("ready\\n");`], { stdio: ["ignore", "pipe", "pipe"] });
    await once(child.stdout, "data");
    const exited = once(child, "exit");
    child.kill("SIGTERM");
    const [chunk] = await once(child.stdout, "data");
    assert.match(String(chunk), /closed/);
    await exited;
    const lock = await acquireRuntimeLock(9222, { lockDir: dir });
    assert.equal(lock.recoveredStaleOwner, false);
    await lock.release();
  } finally { clean(); }
});

test("blocked live owner keeps the socket lock", async () => {
  const { dir, clean } = temporaryLockDir();
  try {
    const child = spawn(process.execPath, ["--input-type=module", "-e", `import { acquireRuntimeLock } from ${JSON.stringify(new URL("./stagehand.mjs", import.meta.url).href)}; await acquireRuntimeLock(9222, { lockDir: ${JSON.stringify(dir)} }); process.stdout.write("ready\\n"); const until = Date.now() + 300; while (Date.now() < until) {}`], { stdio: ["ignore", "pipe", "pipe"] });
    await once(child.stdout, "data");
    await assert.rejects(acquireRuntimeLock(9222, { lockDir: dir, maxWaitMs: 80 }), /runtime-lock-timeout: port=9222/);
    const exited = once(child, "exit");
    child.kill("SIGKILL");
    await exited;
  } finally { clean(); }
});

test("recovered owner resets Runtime once before create", async () => {
  const order = [];
  const reset = runtimeResetForRecoveredOwner({ recoveredStaleOwner: true }, async () => { order.push("reset"); });
  await reset();
  order.push("create");
  await reset();
  assert.deepEqual(order, ["reset", "create"]);
});

test("existing Runtime worker connects by id without loading the extension", async () => {
  const calls = [];
  const cdp = { call: async (method) => {
    calls.push(method);
    if (method === "Target.getTargets") return { result: { targetInfos: [worker("existing")] } };
    return { result: {} };
  }, close: () => {} };
  const options = await attachExtension(9222, [], { flags: { loaded: [], allowOnly: null, unsafeDebugging: true }, cdpFactory: async () => cdp });
  assert.deepEqual(options, { extensionId: extensionIdFor(STAGEHAND_EXTENSION_DIR) });
  assert.equal(calls.filter((method) => method === "Extensions.loadUnpacked").length, 0);
});

test("waking an installed Runtime worker avoids reload", async () => {
  const calls = [];
  let woke = false;
  const cdp = { call: async (method) => {
    calls.push(method);
    if (method === "Target.getTargets") return { result: { targetInfos: woke ? [worker("woken")] : [] } };
    if (method === "Target.createTarget") { woke = true; return { result: { targetId: "wake-tab" } }; }
    return { result: {} };
  }, close: () => {} };
  const options = await attachExtension(9222, [], { flags: { loaded: [], allowOnly: null, unsafeDebugging: true }, cdpFactory: async () => cdp, sleep: async () => {} });
  assert.deepEqual(options, { extensionId: extensionIdFor(STAGEHAND_EXTENSION_DIR) });
  assert.equal(calls.filter((method) => method === "Extensions.loadUnpacked").length, 0);
  assert.equal(calls.filter((method) => method === "Target.closeTarget").length, 1);
});

test("missing Runtime loads once and uses the returned extension id", async () => {
  const calls = [];
  const cdp = { call: async (method) => {
    calls.push(method);
    if (method === "Target.getTargets") return { result: { targetInfos: [] } };
    if (method === "Target.createTarget") return { error: { message: "extension missing" } };
    if (method === "Extensions.loadUnpacked") return { result: { id: "different-runtime-id" } };
    return { result: {} };
  }, close: () => {} };
  const options = await attachExtension(9222, [], { flags: { loaded: [], allowOnly: null, unsafeDebugging: true }, cdpFactory: async () => cdp, sleep: async () => {} });
  assert.deepEqual(options, { extensionId: "different-runtime-id" });
  assert.equal(calls.filter((method) => method === "Extensions.loadUnpacked").length, 1);
});

test("create keeps the worker attached until completion and releases one session", async () => {
  const calls = [];
  const cdp = { call: async (method, params = {}) => {
    calls.push(method);
    if (method === "Target.getTargets") return { result: { targetInfos: [worker("sw-1")] } };
    if (method === "Target.attachToTarget") return { result: { sessionId: "session-1" } };
    return { result: {} };
  } };
  await createWithRuntimeWorker(cdp, async () => {
    calls.push("Stagehand.create");
    assert.equal(calls.at(-2), "Target.attachToTarget");
  });
  assert.deepEqual(calls, ["Target.getTargets", "Target.attachToTarget", "Stagehand.create", "Target.detachFromTarget"]);
  assert.equal(calls.filter((call) => call === "Target.detachFromTarget").length, 1);
});

test("wake absent worker and re-query a fresh target on a retry", async () => {
  const calls = [];
  let generation = 0;
  let wakes = 0;
  const cdp = { call: async (method, params = {}) => {
    calls.push([method, params]);
    if (method === "Target.getTargets") return { result: { targetInfos: wakes ? [worker(`sw-${generation}`)] : [] } };
    if (method === "Target.createTarget") { wakes++; generation++; return { result: { targetId: "wake-1" } }; }
    if (method === "Target.attachToTarget") return { result: { sessionId: `session-${generation}` } };
    return { result: {} };
  } };
  await assert.rejects(createWithRuntimeWorker(cdp, async () => { throw new Error("No target with given id found"); }, { sleep: async () => {} }), /No target/);
  generation++;
  await createWithRuntimeWorker(cdp, async () => {});
  assert.deepEqual(calls.filter(([method]) => method === "Target.attachToTarget").map(([, params]) => params.targetId), ["sw-1", "sw-2"]);
  assert.equal(calls.filter(([method]) => method === "Target.createTarget").length, 1);
  assert.equal(calls.filter(([method]) => method === "Target.detachFromTarget").length, 2);
  assert.equal(calls.filter(([method]) => method === "Target.closeTarget").length, 1);
});

test("attach failure diagnoses the target disappearing after discovery", async () => {
  let reads = 0;
  const cdp = { call: async (method) => {
    if (method === "Target.getTargets") return { result: { targetInfos: ++reads === 1 ? [worker("gone")]: [] } };
    if (method === "Target.attachToTarget") return { error: { message: "No target with given id found" } };
    return { result: {} };
  } };
  await assert.rejects(holdRuntimeWorker(cdp), (error) => {
    assert.match(error.message, /target-disappeared/);
    assert.match(error.message, /"targetId":"gone"/);
    assert.match(error.message, /"type":"service_worker"/);
    assert.match(error.message, /"url":"chrome-extension:\/\//);
    assert.match(error.message, /"attached":false/);
    assert.match(error.message, /"browserContextId":"context-1"/);
    return true;
  });
});

test("ordinary doctor probe does not restart Runtime", async () => {
  const calls = [];
  const cdp = { call: async (method) => {
    calls.push(method);
    if (method === "Target.getTargets") return { result: { targetInfos: [] } };
    return { result: {} };
  } };
  const result = await runDoctor(cdp, { restartRuntime: false });
  assert.equal(result.runtimeRestarted, false);
  assert.equal(calls.filter((method) => method === "Runtime.evaluate").length, 0);
});

test("doctor restartRuntime wakes an idle Runtime service worker before reloading", async () => {
  let woke = false;
  let reloaded = false;
  const calls = [];
  const cdp = { call: async (method, params = {}) => {
    calls.push(method);
    if (method === "Target.getTargets") {
      return { result: { targetInfos: reloaded ? [worker("sw-2")] : woke ? [worker("sw-1")] : [] } };
    }
    if (method === "Target.createTarget") { woke = true; return { result: { targetId: "wake-1" } }; }
    if (method === "Target.attachToTarget") return { result: { sessionId: `session-${params.targetId}` } };
    if (method === "Runtime.evaluate") { reloaded = true; return { result: { result: { value: true } } }; }
    return { result: {} };
  } };
  const result = await runDoctor(cdp, { restartRuntime: true, sleep: async () => {} });
  assert.equal(result.runtimeRestarted, true);
  assert.equal(calls.includes("Target.createTarget"), true);
  assert.equal(calls.includes("Target.closeTarget"), true);
});

test("CDP rejects a silent call and every pending call when its socket closes", async () => {
  class FakeSocket {
    static OPEN = 1;
    readyState = 1;
    constructor() { FakeSocket.instance = this; queueMicrotask(() => this.onopen()); }
    send() {}
    close() { this.readyState = 3; this.onclose(); }
  }
  const cdp = await browserCdp(9222, { version: { webSocketDebuggerUrl: "ws://fake" }, WebSocketClass: FakeSocket });
  await assert.rejects(cdp.call("Target.getTargets", {}, undefined, { timeoutMs: 20 }), /cdp-timeout: Target.getTargets/);
  const first = cdp.call("Runtime.evaluate");
  const second = cdp.call("Page.reload");
  const results = Promise.allSettled([first, second]);
  FakeSocket.instance.close();
  assert.deepEqual((await results).map((result) => result.reason.message), ["cdp-socket-closed", "cdp-socket-closed"]);
});

test("doctor probes twenty pages with two hung renderers in bounded time", async () => {
  const calls = [];
  const pages = Array.from({ length: 20 }, (_, index) => ({ targetId: `page-${index}`, type: "page", url: `https://example.com/${index}` }));
  const cdp = { call: async (method, params = {}, sessionId) => {
    calls.push([method, params.targetId, sessionId]);
    if (method === "Target.getTargets") return { result: { targetInfos: pages } };
    if (method === "Target.attachToTarget") return { result: { sessionId: params.targetId } };
    if (method === "Runtime.evaluate" && ["page-3", "page-17"].includes(sessionId)) return new Promise(() => {});
    return { result: {} };
  } };
  const start = Date.now();
  const result = await runDoctor(cdp, { probeTimeoutMs: 400, settleMs: 10 });
  assert.ok(Date.now() - start < 2500);
  assert.equal(result.checked, 20);
  assert.equal(result.hung.length, 2);
  assert.equal(result.closed.length, 2);
  assert.equal(calls.filter(([method]) => method === "Page.reload").length, 2);
  assert.equal(calls.filter(([method]) => method === "Target.closeTarget").length, 2);
});

const fixture = () => {
  const root = join(tmpdir(), "access-stagehand-test");
  mkdirSync(root, { recursive: true });
  const dir = mkdtempSync(join(root, "browser-"));
  const ext = join(dir, "ext");
  mkdirSync(ext);
  writeFileSync(join(ext, "manifest.json"), JSON.stringify({ manifest_version: 3, name: "Fixture", version: "2.0.0" }));
  return { dir, ext, clean: () => rmSync(dir, { recursive: true, force: true }) };
};

test("browser args carry the shared CDP and extension debugging flags", () => {
  const args = browserArgs({ port: 9333, profileDir: "/tmp/profile", extensions: ["/tmp/ext"] });
  for (const flag of ["--remote-debugging-address=127.0.0.1", "--remote-debugging-port=9333", "--remote-allow-origins=*", "--disable-features=DisableLoadExtensionCommandLineSwitch", "--enable-unsafe-extension-debugging", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--load-extension=/tmp/ext"]) assert.ok(args.includes(flag), flag);
  assert.equal(args.some((flag) => flag.startsWith("--disable-extensions-except=")), false);
});

test("enableProfileExtensions clears disable_reasons and disabled state before browser launch", () => {
  const { dir, clean } = fixture();
  try {
    const profileDir = join(dir, "profile");
    mkdirSync(join(profileDir, "Default"), { recursive: true });
    const prefsFile = join(profileDir, "Default", "Preferences");
    writeFileSync(prefsFile, JSON.stringify({
      extensions: {
        settings: {
          ext1: { path: "/mnt/h/code/save_current_page", state: 0, disable_reasons: [1024] },
          ext2: { path: "/mnt/h/code/web_prompt_bridge", state: 1, disable_reasons: [] },
        },
      },
    }));
    assert.equal(enableProfileExtensions(profileDir), true);
    const updated = JSON.parse(readFileSync(prefsFile, "utf8"));
    assert.deepEqual(updated.extensions.settings.ext1, { path: "/mnt/h/code/save_current_page", state: 1, disable_reasons: [] });
    assert.equal(enableProfileExtensions(profileDir), false);
  } finally { clean(); }
});

test("absent endpoint launches detached and persists the extension state", async () => {
  const { dir, ext, clean } = fixture();
  try {
    let launched = false;
    let captured;
    const result = await ensureBrowser({ port: 9333, profileDir: join(dir, "profile"), chromium: join(ext, "manifest.json"), extensions: [ext],
      endpoint: async () => launched ? { webSocketDebuggerUrl: "ws://example", Browser: "Chromium" } : null,
      processList: () => "", sleep: async () => {},
      spawnBrowser: (_exe, args, options) => { captured = { args, options }; launched = true; return { pid: 123, unref() {} }; },
    });
    assert.equal(result.launched, true);
    assert.equal(result.pid, 123);
    assert.equal(captured.options.detached, true);
    assert.ok(captured.args.includes("--enable-unsafe-extension-debugging"));
    assert.equal(readFileSync(join(dir, "profile", "run_cdp-extensions"), "utf8").trim(), ext);
  } finally { clean(); }
});

test("spawn failure closes its log descriptor and leaves extension state untouched", async () => {
  const { dir, ext, clean } = fixture();
  const profileDir = join(dir, "profile");
  const stateFile = join(profileDir, "run_cdp-extensions");
  try {
    let descriptor;
    await assert.rejects(ensureBrowser({ port: 9334, profileDir, chromium: join(ext, "manifest.json"), extensions: [ext],
      endpoint: async () => null, processList: () => "",
      spawnBrowser: (_exe, _args, options) => { descriptor = options.stdio[1]; throw new Error("spawn-failed"); },
    }), /spawn-failed/);
    assert.throws(() => fstatSync(descriptor), { code: "EBADF" });
    assert.equal(existsSync(stateFile), false);
  } finally { clean(); }
});

test("endpoint timeout terminates browser without persisting extension state", async () => {
  const { dir, ext, clean } = fixture();
  const profileDir = join(dir, "profile");
  const killed = [];
  try {
    await assert.rejects(ensureBrowser({ port: 9335, profileDir, chromium: join(ext, "manifest.json"), extensions: [ext],
      endpoint: async () => null, processList: () => "", sleep: async () => {},
      spawnBrowser: () => ({ pid: 12345, unref() {} }), kill: (pid, signal) => killed.push([pid, signal]),
    }), /browser-endpoint-not-ready/);
    assert.deepEqual(killed, [[12345, "SIGTERM"]]);
    assert.equal(existsSync(join(profileDir, "run_cdp-extensions")), false);
  } finally { clean(); }
});

test("attached endpoint continues when changed profile state is read only", async () => {
  const { dir, ext, clean } = fixture();
  try {
    const profileDir = join(dir, "profile");
    const warnings = [];
    const readOnly = () => { throw Object.assign(new Error("read only"), { code: "EROFS" }); };
    const cdp = { call: async (method) => {
      if (method === "Target.getTargets") return { result: { targetInfos: [] } };
      if (method === "Target.createTarget") return { result: { targetId: "manifest-page" } };
      if (method === "Target.attachToTarget") return { result: { sessionId: "session" } };
      if (method === "Runtime.evaluate") return { result: { result: { value: "2.0.0" } } };
      return { result: {} };
    } };
    const options = { port: 9333, profileDir, extensions: [ext], endpoint: async () => ({ webSocketDebuggerUrl: "ws://ready" }), processList: () => `123 --remote-debugging-port=9333 --enable-unsafe-extension-debugging --load-extension=${ext}`, cdp, writeState: readOnly, warnState: (line) => warnings.push(line) };
    const result = await ensureBrowser(options);
    assert.equal(result.launched, false);
    assert.deepEqual(result.versions, { [ext]: "2.0.0" });
    assert.deepEqual(warnings, ["[stagehand-step] state-write-skipped (run_cdp-extensions: EROFS)"]);
    mkdirSync(profileDir, { recursive: true });
    writeFileSync(join(profileDir, "run_cdp-extensions"), `${ext}\n`);
    warnings.length = 0;
    await ensureBrowser(options);
    assert.deepEqual(warnings, []);
  } finally { clean(); }
});

test("attached browser reloads a stale extension in place and keeps its process", async () => {
  const { dir, ext, clean } = fixture();
  try {
    const id = extensionIdFor(ext);
    let installed = "1.0.0";
    let loads = 0;
    let closed = 0;
    const cdp = { call: async (method) => {
      if (method === "Target.getTargets") return { result: { targetInfos: [] } };
      if (method === "Target.createTarget") return { result: { targetId: "manifest-page" } };
      if (method === "Target.attachToTarget") return { result: { sessionId: "session" } };
      if (method === "Runtime.evaluate") return { result: { result: { value: installed } } };
      if (method === "Extensions.loadUnpacked") { loads++; installed = "2.0.0"; return { result: { id } }; }
      if (method === "Browser.close") { closed++; throw new Error("browser must stay open"); }
      return { result: {} };
    }, close() { closed++; } };
    let connections = 0;
    const options = { port: 9333, profileDir: join(dir, "profile"), extensions: [ext],
      endpoint: async () => ({ webSocketDebuggerUrl: "ws://ready", Browser: "Chromium" }),
      processList: () => `123 --remote-debugging-port=9333 --enable-unsafe-extension-debugging --load-extension=${ext}`,
      cdpFactory: async () => { connections++; return cdp; },
      spawnBrowser: () => { throw new Error("browser must stay open"); }, sleep: async () => {},
    };
    const first = await ensureBrowser(options);
    const second = await ensureBrowser(options);
    assert.deepEqual([first.launched, first.versions[ext], second.versions[ext], loads, connections, closed], [false, "2.0.0", "2.0.0", 1, 2, 2]);
  } finally { clean(); }
});

test("requested extension replaces same-name state path without loading the duplicate", async () => {
  const { dir, ext, clean } = fixture();
  try {
    const duplicate = join(dir, "repo-dist");
    const profileDir = join(dir, "profile");
    mkdirSync(duplicate);
    mkdirSync(profileDir);
    writeFileSync(join(duplicate, "manifest.json"), JSON.stringify({ manifest_version: 3, name: "Fixture", version: "1.0.0" }));
    writeFileSync(join(profileDir, "run_cdp-extensions"), `${duplicate}\n`);
    const calls = [];
    const cdp = { call: async (method, params) => {
      calls.push([method, params]);
      if (method === "Target.getTargets") return { result: { targetInfos: [] } };
      if (method === "Target.createTarget") return { result: { targetId: "manifest-page" } };
      if (method === "Target.attachToTarget") return { result: { sessionId: "session" } };
      if (method === "Runtime.evaluate") return { result: { result: { value: "2.0.0" } } };
      return { result: {} };
    } };
    const result = await ensureBrowser({ port: 9333, profileDir, extensions: [ext],
      endpoint: async () => ({ webSocketDebuggerUrl: "ws://ready" }),
      processList: () => `123 --remote-debugging-port=9333 --enable-unsafe-extension-debugging --load-extension=${ext}`,
      cdp, spawnBrowser: () => { throw new Error("browser replaced"); },
    });
    assert.equal(result.launched, false);
    assert.deepEqual(result.versions, { [ext]: "2.0.0" });
    assert.equal(readFileSync(join(profileDir, "run_cdp-extensions"), "utf8"), `${ext}\n`);
    assert.equal(calls.some(([method, params]) => method === "Extensions.loadUnpacked" && params.path === duplicate), false);
  } finally { clean(); }
});

test("attached browser without unsafe extension debugging retains its error", async () => {
  const { dir, ext, clean } = fixture();
  try {
    await assert.rejects(ensureBrowser({ port: 9333, profileDir: join(dir, "profile"), extensions: [ext],
      endpoint: async () => ({ webSocketDebuggerUrl: "ws://ready" }),
      processList: () => "123 --remote-debugging-port=9333",
      cdpFactory: async () => { throw new Error("CDP must not connect"); },
      spawnBrowser: () => { throw new Error("browser must stay open"); },
    }), /stagehand-extension-unloadable/);
  } finally { clean(); }
});

test("session and page diagnostics skip permission failures but propagate other errors", () => {
  const warnings = [];
  const denied = (code) => () => { throw Object.assign(new Error(code), { code }); };
  for (const file of ["/tmp/sessions.jsonl", "/tmp/9333.pages.json"]) {
    assert.equal(optionalStateWrite(file, denied("EROFS"), { warn: (line) => warnings.push(line) }), false);
    assert.equal(optionalStateWrite(file, denied("EACCES"), { warn: (line) => warnings.push(line) }), false);
  }
  assert.deepEqual(warnings, [
    "[stagehand-step] state-write-skipped (sessions.jsonl: EROFS)",
    "[stagehand-step] state-write-skipped (sessions.jsonl: EACCES)",
    "[stagehand-step] state-write-skipped (9333.pages.json: EROFS)",
    "[stagehand-step] state-write-skipped (9333.pages.json: EACCES)",
  ]);
  assert.throws(() => optionalStateWrite("/tmp/sessions.jsonl", denied("EIO")), { code: "EIO" });
});

test("replace closes the browser through CDP before launching again", async () => {
  const { dir, ext, clean } = fixture();
  try {
    const calls = [];
    let running = true;
    let launched = false;
    const result = await ensureBrowser({ port: 9333, profileDir: join(dir, "profile"), chromium: join(ext, "manifest.json"), replace: true, lock: { release() { throw new Error("caller lock released"); } },
      endpoint: async () => running ? { webSocketDebuggerUrl: "ws://old" } : launched ? { webSocketDebuggerUrl: "ws://new" } : null,
      processList: () => { throw new Error("PID lookup used with a live endpoint"); },
      cdp: { call: async (method) => { calls.push(method); running = false; return { result: {} }; } },
      sleep: async () => {}, spawnBrowser: () => { calls.push("spawn"); launched = true; return { pid: 333, unref() {} }; },
    });
    assert.deepEqual(calls, ["Browser.close", "spawn"]);
    assert.equal(result.version.webSocketDebuggerUrl, "ws://new");
  } finally { clean(); }
});

test("missing extension loads unpacked and reads back its version", async () => {
  const { ext, clean } = fixture();
  try {
    const id = extensionIdFor(ext);
    let actual = null;
    let loads = 0;
    const cdp = { call: async (method) => {
      if (method === "Target.getTargets") return { result: { targetInfos: actual ? [{ type: "service_worker", targetId: "worker", url: `chrome-extension://${id}/background.js` }] : [] } };
      if (method === "Target.createTarget") return { result: { targetId: "manifest-page" } };
      if (method === "Target.attachToTarget") return { result: { sessionId: "session" } };
      if (method === "Runtime.evaluate") return { result: { result: { value: actual } } };
      if (method === "Extensions.loadUnpacked") { loads++; actual = "2.0.0"; return { result: { id } }; }
      return { result: {} };
    } };
    const result = await ensureExtensions(cdp, [ext], { sleep: async () => {} });
    assert.deepEqual([loads, result.versions[ext]], [1, "2.0.0"]);
  } finally { clean(); }
});

test("updated extension reloads matching open tabs after runtime version readback", async () => {
  const { ext, clean } = fixture();
  try {
    writeFileSync(join(ext, "manifest.json"), JSON.stringify({ manifest_version: 3, name: "Fixture", version: "2.0.0", content_scripts: [{ matches: ["https://*.chatgpt.com/*"] }] }));
    const id = extensionIdFor(ext);
    let version = "1.0.0";
    const calls = [];
    const cdp = { call: async (method, params = {}, sessionId) => {
      calls.push([method, params, sessionId]);
      if (method === "Target.getTargets") return { result: { targetInfos: [
        { type: "page", targetId: "chat", url: "https://chatgpt.com/c/1" },
        { type: "page", targetId: "other", url: "https://example.com/" },
      ] } };
      if (method === "Target.createTarget") return { result: { targetId: "manifest-page" } };
      if (method === "Target.attachToTarget") return { result: { sessionId: params.targetId } };
      if (method === "Runtime.evaluate") return { result: { result: { value: version } } };
      if (method === "Extensions.loadUnpacked") { version = "2.0.0"; return { result: { id } }; }
      return { result: {} };
    } };
    const result = await ensureExtensions(cdp, [ext], { sleep: async () => {} });
    assert.equal(result.versions[ext], "2.0.0");
    assert.deepEqual(calls.filter(([method]) => method === "Page.reload").map(([, , session]) => session), ["chat"]);
    assert.ok(calls.findIndex(([method]) => method === "Page.reload") > calls.findLastIndex(([method]) => method === "Runtime.evaluate"));
  } finally { clean(); }
});

test("stale runtime version prevents content script tab reload", async () => {
  const { ext, clean } = fixture();
  try {
    writeFileSync(join(ext, "manifest.json"), JSON.stringify({ manifest_version: 3, name: "Fixture", version: "2.0.0", content_scripts: [{ matches: ["https://chatgpt.com/*"] }] }));
    const id = extensionIdFor(ext);
    const calls = [];
    const cdp = { call: async (method, params = {}) => {
      calls.push(method);
      if (method === "Target.getTargets") return { result: { targetInfos: [{ type: "page", targetId: "chat", url: "https://chatgpt.com/" }] } };
      if (method === "Target.createTarget") return { result: { targetId: "manifest-page" } };
      if (method === "Target.attachToTarget") return { result: { sessionId: "session" } };
      if (method === "Runtime.evaluate") return { result: { result: { value: "1.0.0" } } };
      if (method === "Extensions.loadUnpacked") return { result: { id } };
      return { result: {} };
    } };
    await assert.rejects(ensureExtensions(cdp, [ext], { sleep: async () => {} }), /extension-version-stale/);
    assert.equal(calls.includes("Page.reload"), false);
  } finally { clean(); }
});

test("loaded stale extension is reloaded after same-path loadUnpacked stays stale", async () => {
  const { ext, clean } = fixture();
  try {
    const id = extensionIdFor(ext);
    let version = "1.0.0";
    let worker = "old-worker";
    const calls = [];
    const cdp = { call: async (method, params = {}) => {
      calls.push(method);
      if (method === "Target.getTargets") return { result: { targetInfos: [{ type: "service_worker", targetId: worker, url: `chrome-extension://${id}/background.js` }] } };
      if (method === "Target.createTarget") return { result: { targetId: "manifest-page" } };
      if (method === "Target.attachToTarget") return { result: { sessionId: "session" } };
      if (method === "Runtime.evaluate") {
        if (params.expression.includes("runtime.reload")) { worker = "new-worker"; version = "2.0.0"; return { result: { result: { value: true } } }; }
        return { result: { result: { value: version } } };
      }
      if (method === "Extensions.loadUnpacked") return { result: { id } };
      return { result: {} };
    } };
    const result = await ensureExtensions(cdp, [ext], { sleep: async () => {} });
    assert.equal(result.versions[ext], "2.0.0");
    assert.equal(calls.filter((method) => method === "Extensions.loadUnpacked").length, 1);
    assert.ok(calls.filter((method) => method === "Runtime.evaluate").length >= 4);
  } finally { clean(); }
});

test("stale version after reload reports source without closing the browser", async () => {
  const { ext, clean } = fixture();
  try {
    const id = extensionIdFor(ext);
    let worker = "old-worker";
    const calls = [];
    const cdp = { call: async (method, params = {}) => {
      calls.push(method);
      if (method === "Target.getTargets") return { result: { targetInfos: [{ type: "service_worker", targetId: worker, url: `chrome-extension://${id}/background.js` }] } };
      if (method === "Target.createTarget") return { result: { targetId: "manifest-page" } };
      if (method === "Target.attachToTarget") return { result: { sessionId: "session" } };
      if (method === "Runtime.evaluate") {
        if (params.expression.includes("runtime.reload")) { worker = "new-worker"; return { result: { result: { value: true } } }; }
        return { result: { result: { value: "1.0.0" } } };
      }
      if (method === "Extensions.loadUnpacked") return { result: { id } };
      return { result: {} };
    } };
    await assert.rejects(ensureExtensions(cdp, [ext], { sleep: async () => {} }), (error) => error.message.includes(`extension-version-stale: ${ext} expected=2.0.0 actual=1.0.0 sw=chrome-extension://${id}/background.js source=page id=${id} loadedId=${id}`));
    assert.equal(calls.filter((method) => method === "Browser.close").length, 0);
    assert.equal(calls.filter((method) => method === "Extensions.loadUnpacked").length, 1);
  } finally { clean(); }
});

test("content-script extension reads installed version from a temporary manifest page", async () => {
  const { ext, clean } = fixture();
  try {
    writeFileSync(join(ext, "manifest.json"), JSON.stringify({ manifest_version: 3, name: "Fixture", version: "2.0.0", background: { service_worker: "background.js" } }));
    const id = extensionIdFor(ext);
    const calls = [];
    const cdp = { call: async (method, params = {}) => {
      calls.push([method, params]);
      if (method === "Target.getTargets") return { result: { targetInfos: [] } };
      if (method === "Target.createTarget") return { result: { targetId: "manifest-page" } };
      if (method === "Target.attachToTarget") return { result: { sessionId: "manifest-session" } };
      if (method === "Runtime.evaluate") return { result: { result: { value: "2.0.0" } } };
      return { result: {} };
    } };
    const result = await ensureExtensions(cdp, [ext]);
    assert.equal(result.versions[ext], "2.0.0");
    assert.deepEqual(calls.filter(([method]) => method === "Target.createTarget").map(([, params]) => params.url), [`chrome-extension://${id}/manifest.json`]);
    assert.equal(calls.filter(([method]) => method === "Target.closeTarget").length, 1);
    assert.equal(calls.filter(([method]) => method === "Extensions.loadUnpacked").length, 0);
  } finally { clean(); }
});

test("loadUnpacked returned id is used when it differs from the calculated id", async () => {
  const { ext, clean } = fixture();
  try {
    const calculated = extensionIdFor(ext);
    const loadedId = "abcdefghijklmnopabcdefghijklmnop";
    const pages = [];
    const cdp = { call: async (method, params = {}, sessionId) => {
      if (method === "Target.getTargets") return { result: { targetInfos: [] } };
      if (method === "Target.createTarget") { pages.push(params.url); return { result: { targetId: `page-${pages.length}` } }; }
      if (method === "Target.attachToTarget") return { result: { sessionId: params.targetId } };
      if (method === "Runtime.evaluate") return { result: { result: { value: sessionId === "page-2" ? "2.0.0" : null } } };
      if (method === "Extensions.loadUnpacked") return { result: { id: loadedId } };
      return { result: {} };
    } };
    const result = await ensureExtensions(cdp, [ext], { sleep: async () => {} });
    assert.equal(result.versions[ext], "2.0.0");
    assert.deepEqual(pages, [`chrome-extension://${calculated}/manifest.json`, `chrome-extension://${loadedId}/manifest.json`]);
  } finally { clean(); }
});

test("insufficient flags and ineffective first load fail without browser replacement", async () => {
  const { ext, clean } = fixture();
  try {
    const calls = [];
    const cdp = { call: async (method) => { calls.push(method); return method === "Target.getTargets" ? { result: { targetInfos: [] } } : { result: { id: extensionIdFor(ext) } }; } };
    await assert.rejects(ensureExtensions(cdp, [ext], { flags: { unsafeDebugging: false, allowOnly: null } }), /extension-debugging-unavailable/);
    await assert.rejects(ensureExtensions(cdp, [ext], { sleep: async () => {} }), /extension-version-stale/);
    assert.equal(calls.filter((method) => method === "Browser.close").length, 0);
  } finally { clean(); }
});

test("enhanceStagehandPage polyfills locator.evaluate and evaluateAll including cross-origin iframe fallback", async () => {
  const calls = [];
  const makeRawLocator = (selector, nth) => ({
    descriptor: { selector, ...(nth === undefined ? {} : { nth }) },
    innerHtml: async () => `<div class="video-maker-init-overlay-backdrop"><p>초기화를 해주세요</p><button type="button">초기화</button></div>`,
    first: () => makeRawLocator(selector, 0),
    nth: (index) => makeRawLocator(selector, index),
  });
  const fakePage = {
    locator: (selector) => makeRawLocator(selector),
    evaluate: async (_fn, arg) => {
      calls.push(arg);
      if (arg.parts && arg.parts[0] === "iframe.custom_view") {
        return { status: "cross-origin" };
      }
      if (arg.html) {
        const fn = (0, eval)(`(${arg.fnText})`);
        const fakeEl = { className: "video-maker-init-overlay-backdrop", textContent: "초기화를 해주세요" };
        return arg.evalAll ? fn([fakeEl], arg.fnArg) : fn(fakeEl, arg.fnArg);
      }
      const fn = (0, eval)(`(${arg.fnText})`);
      const fakeEl = { tagName: "BUTTON", textContent: "초기화", nth: arg.nthIndex };
      return { status: "ok", value: arg.evalAll ? fn([fakeEl], arg.fnArg) : fn(fakeEl, arg.fnArg) };
    },
  };
  const enhanced = enhanceStagehandPage(fakePage);
  assert.equal(enhanced, fakePage);
  const sameOrigin = await enhanced.locator("button").nth(1).evaluate((el, suffix) => `${el.textContent}:${el.nth}:${suffix}`, "ok");
  assert.equal(sameOrigin, "초기화:1:ok");
  const crossOrigin = await enhanced.locator("iframe.custom_view >> .video-maker-init-overlay-backdrop").evaluate((el) => el.className);
  assert.equal(crossOrigin, "video-maker-init-overlay-backdrop");
  const allResult = await enhanced.locator("button").evaluateAll((els) => els.length);
  assert.equal(allResult, 1);
});
