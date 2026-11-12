#!/usr/bin/env node
// Stagehand access for the extensions workflow.
//
// Owns the Stagehand runtime (installed in RUNTIME_DIR from runtime/package.json),
// the AI model name and API key (this skill's .env, never printed), and opening
// a visible session on the shared CDP endpoint with the dedicated profile.
//
// Module:
//   import { openStagehand } from "/home/tree/.codex/skills/access-stagehand/scripts/stagehand.mjs";
//   const session = await openStagehand({ url, extensions: [dir] });
//   await session.stagehand.act("...");  // needs STAGEHAND_API_KEY
//   await session.close();               // disconnects; the browser stays open
//
// CLI:
//   node stagehand.mjs status
//   node stagehand.mjs install
//   node stagehand.mjs open [--url URL] [--extension DIR[,DIR]] [--port 9222]
//   node stagehand.mjs doctor [--restart-runtime] [--close-hung] [--port 9222]
//   node stagehand.mjs stats [--last N]
import { appendFileSync, closeSync, copyFileSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, realpathSync, statSync, unlinkSync, utimesSync, writeFileSync } from "node:fs";
import { spawn, spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { homedir, hostname, tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createConnection, createServer } from "node:net";

const SKILL_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const RUNTIME_DIR = process.env.STAGEHAND_RUNTIME_DIR || "/home/tree/.local/share/access-stagehand";
export const DEFAULT_PORT = 9222;
export const PROFILE_DIR = "/home/tree/.config/codex-run-cdp";
export const CHROMIUM = "/home/tree/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome";
export const STAGEHAND_EXTENSION_DIR = join(RUNTIME_DIR, "node_modules", "@browserbasehq", "stagehand", "dist", "extension");
const pause = (ms) => new Promise((done) => setTimeout(done, ms));

export function cleanupBrowserLogs(dir = tmpdir(), now = Date.now()) {
  for (const name of readdirSync(dir)) {
    if (!/^run_cdp-\d+\.log$/.test(name)) continue;
    const file = join(dir, name);
    try { if (now - statSync(file).mtimeMs > 14 * 24 * 3600 * 1000) unlinkSync(file); }
    catch (error) { if (error?.code !== "ENOENT") throw error; }
  }
}

const record = (operation, ack, error = "") => process.stderr.write(`[record-log] 00119805791 | ${operation} | attempt=1 ack=${ack}${error ? ` error=${error}` : ""}\n`);
export function optionalStateWrite(file, operation, { warn = (line) => process.stderr.write(`${line}\n`) } = {}) {
  try { operation(); return true; }
  catch (error) {
    if (error?.code !== "EROFS" && error?.code !== "EACCES") throw error;
    warn(`[stagehand-step] state-write-skipped (${basename(file)}: ${error.code})`);
    return false;
  }
}
export const browserArgs = ({ port = DEFAULT_PORT, profileDir = PROFILE_DIR, extensions = [], url = "about:blank", headless = false }) => {
  const dirs = [...new Set(extensions.map((dir) => resolve(dir)))];
  const csv = dirs.join(",");
  return [
    ...(dirs.length ? [`--load-extension=${csv}`] : []),
    ...(headless ? ["--headless=new"] : []),
    "--remote-debugging-address=127.0.0.1", `--remote-debugging-port=${port}`,
    "--remote-allow-origins=*", "--disable-features=DisableLoadExtensionCommandLineSwitch",
    "--enable-unsafe-extension-debugging", `--user-data-dir=${profileDir}`,
    "--no-first-run", "--no-default-browser-check", "--disable-dev-shm-usage",
    "--no-sandbox", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", url,
  ];
};

export function browserFlags(port, { processList = () => spawnSync("ps", ["-eo", "args="], { encoding: "utf8" }).stdout || "" } = {}) {
  const line = processList().split("\n").find((args) => args.includes(`--remote-debugging-port=${port}`) && !args.includes("--type="));
  if (!line) return null;
  const list = (name) => {
    const match = line.match(new RegExp(`--${name}=(\\S+)`, "u"));
    return match ? match[1].split(",").filter(Boolean).map((dir) => resolve(dir)) : null;
  };
  return { loaded: list("load-extension") || [], allowOnly: list("disable-extensions-except"), unsafeDebugging: line.includes("--enable-unsafe-extension-debugging") };
}

export function extensionState(profileDir = PROFILE_DIR, requested = [], { runtimeDir = STAGEHAND_EXTENSION_DIR, noExtension = false } = {}) {
  const stateFile = join(profileDir, "run_cdp-extensions");
  const previous = !noExtension && existsSync(stateFile) ? readFileSync(stateFile, "utf8").split("\n").filter(Boolean) : [];
  const candidates = [...new Set([...requested, ...previous].map((dir) => resolve(dir)))];
  for (const dir of requested) if (!existsSync(join(dir, "manifest.json"))) throw new Error(`extension-manifest-absent: ${dir}`);
  const names = new Set();
  const valid = noExtension ? [] : candidates.filter((dir) => {
    if (!existsSync(join(dir, "manifest.json"))) return false;
    const name = JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8")).name;
    if (names.has(name)) return false;
    names.add(name);
    return true;
  });
  return { stateFile, userDirs: valid, launchDirs: [...valid, ...(existsSync(join(runtimeDir, "manifest.json")) ? [resolve(runtimeDir)] : [])] };
}

const profilePids = (profileDir, processList) => processList().split("\n").flatMap((line) => {
  const match = line.trim().match(/^(\d+)\s+(.+)$/);
  return match && match[2].includes(`--user-data-dir=${profileDir}`) && !match[2].includes("--type=") ? [Number(match[1])] : [];
});

export function enableProfileExtensions(profileDir = PROFILE_DIR) {
  const prefsFile = join(profileDir, "Default", "Preferences");
  if (!existsSync(prefsFile)) return false;
  try {
    const prefs = JSON.parse(readFileSync(prefsFile, "utf8"));
    const settings = prefs?.extensions?.settings;
    if (!settings || typeof settings !== "object") return false;
    let changed = false;
    for (const entry of Object.values(settings)) {
      if (!entry || typeof entry !== "object") continue;
      if (Array.isArray(entry.disable_reasons) && entry.disable_reasons.length > 0) {
        entry.disable_reasons = [];
        changed = true;
      }
      if (entry.state === 0) {
        entry.state = 1;
        changed = true;
      }
    }
    if (changed) optionalStateWrite(prefsFile, () => writeFileSync(prefsFile, JSON.stringify(prefs)));
    return changed;
  } catch {
    return false;
  }
}

export async function ensureBrowser({ port = DEFAULT_PORT, extensions = [], replace = false, url = "about:blank", profileDir = PROFILE_DIR, chromium = CHROMIUM, headless = false, noExtension = false, lock, endpoint = endpointVersion, processList = () => spawnSync("ps", ["-eo", "pid=,args="], { encoding: "utf8" }).stdout || "", cdp, cdpFactory = browserCdp, spawnBrowser = spawn, kill = process.kill, sleep = pause, writeState = writeFileSync, warnState } = {}) {
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("invalid-browser-port");
  if (url !== "about:blank" && !/^https?:\/\/\S+$/.test(url)) throw new Error("invalid-browser-url");
  const state = extensionState(profileDir, extensions, { noExtension });
  const existing = await endpoint(port);
  if (existing && !replace) {
    const flags = browserFlags(port, { processList: () => processList().replace(/^\s*\d+\s+/gm, "") });
    let versions = {};
    if (state.userDirs.length) {
      if (flags && !flags.unsafeDebugging) {
        throw new Error(`stagehand-extension-unloadable: the browser at port ${port} lacks --enable-unsafe-extension-debugging, so Stagehand cannot load its runtime; restart it with \`run_cdp --replace -e ${state.userDirs.join(",")}\`, which loads the Stagehand Runtime too`);
      }
      const connection = cdp || await cdpFactory(port, { version: existing });
      try { ({ versions } = await ensureExtensions(connection, state.userDirs, { flags, sleep })); }
      finally { if (!cdp) connection.close(); }
    }
    const content = state.userDirs.length ? `${state.userDirs.join("\n")}\n` : "";
    const oldContent = existsSync(state.stateFile) ? readFileSync(state.stateFile, "utf8") : "";
    if (content !== oldContent) {
      optionalStateWrite(state.stateFile, () => {
        mkdirSync(profileDir, { recursive: true });
        writeState(state.stateFile, content);
      }, { warn: warnState });
    }
    return { version: existing, launched: false, flags, versions };
  }
  let ownedLock;
  if (replace && !lock) {
    ownedLock = await acquireRuntimeLock(port);
  }
  try {
    if (existing) {
      const connection = cdp || await cdpFactory(port, { version: existing });
      try {
        try { await connection.call("Browser.close"); }
        catch (error) { if (!/cdp-socket-closed/.test(String(error?.message))) throw error; }
      } finally { if (!cdp) connection.close(); }
      let stopped = false;
      for (let attempt = 0; attempt < 20; attempt += 1) {
        if (!await endpoint(port)) { stopped = true; break; }
        await sleep(100);
      }
      if (!stopped) throw new Error("browser-endpoint-still-running");
    } else if (profilePids(profileDir, processList).length) {
      throw new Error("browser-profile-still-running");
    }
    if (!existsSync(chromium)) throw new Error("browser-executable-absent");
    mkdirSync(profileDir, { recursive: true });
    enableProfileExtensions(profileDir);
    cleanupBrowserLogs();
    const logFile = `/tmp/run_cdp-${port}.log`;
    const fd = openSync(logFile, "w");
    let child;
    try {
      child = spawnBrowser(chromium, browserArgs({ port, profileDir, extensions: state.launchDirs, url, headless }), { detached: true, stdio: ["ignore", fd, fd] });
    } finally { closeSync(fd); }
    child.unref?.();
    for (let attempt = 0; attempt < 20; attempt++) {
      await sleep(500);
      const version = await endpoint(port);
      if (version) {
        if (state.userDirs.length) optionalStateWrite(state.stateFile, () => writeState(state.stateFile, `${state.userDirs.join("\n")}\n`), { warn: warnState });
        record("ensureBrowser", "ready");
        return { version, launched: true, pid: child.pid, flags: { loaded: state.launchDirs, allowOnly: null, unsafeDebugging: true } };
      }
    }
    try { kill(child.pid, "SIGTERM"); } catch {}
    throw new Error("browser-endpoint-not-ready");
  } catch (error) { record("ensureBrowser", "error", String(error.message).split(":", 1)[0]); throw error; }
  finally { if (ownedLock) await ownedLock.release(); }
}

async function extensionTarget(cdp, id) {
  const targets = (await cdp.call("Target.getTargets")).result?.targetInfos || [];
  const owned = targets.filter((item) => item.url?.startsWith(`chrome-extension://${id}/`));
  return owned.find((item) => item.type === "service_worker") || owned.find((item) => ["background_page", "page"].includes(item.type)) || null;
}

async function loadedVersion(cdp, id, sleep = pause) {
  const targets = (await cdp.call("Target.getTargets")).result?.targetInfos || [];
  const worker = targets.find((item) => item.type === "service_worker" && item.url?.startsWith(`chrome-extension://${id}/`));
  const source = "page";
  const sw = worker?.url || "none";
  const created = await cdp.call("Target.createTarget", { url: `chrome-extension://${id}/manifest.json` });
  const targetId = created.result?.targetId;
  if (!targetId) return { version: null, sw, source };
  try {
    const attached = await cdp.call("Target.attachToTarget", { targetId, flatten: true });
    const sessionId = attached.result?.sessionId;
    if (!sessionId) return { version: null, sw, source };
    try {
      for (let attempt = 0; attempt < 10; attempt += 1) {
        const answer = await cdp.call("Runtime.evaluate", { expression: "chrome.runtime.getManifest().version", returnByValue: true }, sessionId).catch(() => null);
        const version = answer?.result?.result?.value;
        if (typeof version === "string" && version.length) return { version, sw, source };
        await sleep(100);
      }
      return { version: null, sw, source };
    } finally { await cdp.call("Target.detachFromTarget", { sessionId }).catch(() => {}); }
  } finally { await cdp.call("Target.closeTarget", { targetId }).catch(() => {}); }
}

async function reloadExtension(cdp, id, manifest, sleep) {
  let target = await extensionTarget(cdp, id);
  let createdTargetId;
  if (!target) {
    const page = manifest.action?.default_popup || manifest.options_page || "manifest.json";
    const created = await cdp.call("Target.createTarget", { url: `chrome-extension://${id}/${page}` });
    createdTargetId = created.result?.targetId;
    if (!createdTargetId) return false;
    target = { targetId: createdTargetId };
  }
  const oldTargetId = target.type === "service_worker" ? target.targetId : null;
  try {
    const attached = await cdp.call("Target.attachToTarget", { targetId: target.targetId, flatten: true });
    const sessionId = attached.result?.sessionId;
    if (!sessionId) return false;
    try {
      const result = await cdp.call("Runtime.evaluate", { expression: "setTimeout(() => chrome.runtime.reload(), 50), true", returnByValue: true }, sessionId);
      if (result.error || result.result?.exceptionDetails) return false;
    } finally { await cdp.call("Target.detachFromTarget", { sessionId }).catch(() => {}); }
    if (!manifest.background?.service_worker) { await sleep(100); return true; }
    for (let attempt = 0; attempt < 30; attempt += 1) {
      await sleep(100);
      const targets = (await cdp.call("Target.getTargets")).result?.targetInfos || [];
      if (targets.some((item) => item.type === "service_worker" && item.url?.startsWith(`chrome-extension://${id}/`) && item.targetId !== oldTargetId)) return true;
    }
    return false;
  } finally { if (createdTargetId) await cdp.call("Target.closeTarget", { targetId: createdTargetId }).catch(() => {}); }
}

function matchesContentScript(url, patterns) {
  return patterns.some((pattern) => {
    if (pattern === "<all_urls>") return /^(https?|file|ftp):\/\//.test(url);
    const match = pattern.match(/^(\*|http|https|file|ftp):\/\/([^/]*)(\/.*)$/);
    if (!match) return false;
    const [, scheme, host, path] = match;
    const escape = (part) => part.replace(/[|\\{}()[\]^$+?.]/g, "\\$&").replace(/\*/g, ".*");
    const schemes = scheme === "*" ? "https?" : scheme;
    const hosts = host.startsWith("*.") ? `(?:[^/]+\\.)?${escape(host.slice(2))}` : escape(host);
    return new RegExp(`^${schemes}:\\/\\/${hosts}${escape(path)}$`).test(url);
  });
}

async function reloadContentScriptTabs(cdp, manifest) {
  const patterns = manifest.content_scripts?.flatMap((script) => script.matches || []) || [];
  if (!patterns.length) return;
  const targets = (await cdp.call("Target.getTargets")).result?.targetInfos || [];
  for (const target of targets) {
    if (target.type !== "page" || !matchesContentScript(target.url || "", patterns)) continue;
    const attached = await cdp.call("Target.attachToTarget", { targetId: target.targetId, flatten: true });
    const sessionId = attached.result?.sessionId;
    if (!sessionId) throw new Error(`content-script-tab-attach-failed: ${target.targetId}`);
    try { await cdp.call("Page.reload", {}, sessionId); }
    finally { await cdp.call("Target.detachFromTarget", { sessionId }).catch(() => {}); }
  }
}

export async function ensureExtensions(cdp, dirs, { flags = null, sleep = pause } = {}) {
  const unique = [...new Set(dirs.map((dir) => resolve(dir)))];
  if (flags && (!flags.unsafeDebugging || (flags.allowOnly && unique.some((dir) => !flags.allowOnly.includes(dir))))) {
    throw new Error("extension-debugging-unavailable: browser flags do not permit extension loading");
  }
  const versions = {};
  for (const dir of unique) {
    const manifest = JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8"));
    const expected = manifest.version;
    const id = extensionIdFor(dir);
    let loadedId = "none";
    let observed = await loadedVersion(cdp, id, sleep);
    if (observed.version !== expected) {
      const result = await cdp.call("Extensions.loadUnpacked", { path: dir });
      if (!result.result?.id) { record("ensureExtensions", "error", "extension-load-failed"); throw new Error(`extension-load-failed: ${result.error?.message || "no id"}`); }
      loadedId = result.result.id;
      observed = await loadedVersion(cdp, loadedId, sleep);
      if (observed.version !== expected) {
        await reloadExtension(cdp, loadedId, manifest, sleep);
        observed = await loadedVersion(cdp, loadedId, sleep);
      }
      if (observed.version !== expected) throw new Error(`extension-version-stale: ${dir} expected=${expected} actual=${observed.version ?? "none"} sw=${observed.sw} source=${observed.source} id=${id} loadedId=${loadedId}`);
      await reloadContentScriptTabs(cdp, manifest);
    }
    versions[dir] = observed.version;
  }
  record("ensureExtensions", "ok");
  return { restarted: false, versions };
}

export const runtimeLockDir = (env = process.env, tempRoot = tmpdir()) => env.STAGEHAND_LOCK_DIR || join(tempRoot, "access-stagehand-locks");
export const sessionLogFile = (env = process.env) => env.STAGEHAND_SESSION_LOG_FILE || join(homedir(), ".local", "state", "access-stagehand", "sessions.jsonl");

const metricKeys = ["lockWaitMs", "ensureMs", "doctorMs", "connectMs", "createMs", "pageMs"];
const emptyMetrics = () => Object.fromEntries(metricKeys.map((key) => [key, 0]));
export const formatSessionSummary = (metrics, retries, tabs, mode) => `[stagehand-summary] ${JSON.stringify({ ...metrics, retries, tabs, mode })}`;
const boundaryError = (boundary, error) => new Error(`${boundary}: ${error instanceof Error ? error.message : String(error)}`, { cause: error });
const errorBoundary = (error) => String(error?.message || error).split(":", 1)[0];
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export async function connectAndCreate({ connect, create, refreshTarget, restartRuntime, doctor, onRetry = () => {}, onCreateDuration = () => {}, sleep = wait, createTimeoutMs = 20_000, createSettleTimeoutMs = 65_000, totalTimeoutMs = 60_000, now = Date.now }) {
  const started = now();
  let retries = 0;
  let targetRetries = 0;
  let initRestarted = false;
  let handleReconnected = false;
  let connected = false;
  let browser;
  const deadline = () => {
    if (now() - started >= totalTimeoutMs) throw new Error("stagehand-total-timeout");
  };
  for (;;) {
    deadline();
    let phase = connected ? "create" : "connect";
    try {
      if (!connected) {
        browser = await connect();
        connected = true;
      }
      phase = "create";
      let timer;
      const createStartedAt = now();
      const limit = Math.min(createTimeoutMs, Math.max(1, totalTimeoutMs - (now() - started)));
      const pending = Promise.resolve().then(() => create(browser, retries + 1));
      // A create that finishes after the deadline must not leave its session alive.
      pending.then((late) => { if (timer === null) void late?.close?.().catch(() => {}); }).catch(() => {});
      try {
        const result = await Promise.race([pending, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("stagehand-create-timeout")), limit); })]);
        clearTimeout(timer);
        return { stagehand: result, browser, retries };
      } catch (error) {
        clearTimeout(timer);
        if (error?.message === "stagehand-create-timeout") {
          timer = null;
          // Promise.race does not cancel Stagehand.create. Keep the lock and do not
          // start another create until the timed-out call has actually settled.
          const settled = await Promise.race([
            pending.then(() => true, () => true),
            sleep(createSettleTimeoutMs).then(() => false),
          ]);
          if (!settled) throw new Error("stagehand-create-unsettled", { cause: error });
        }
        throw error;
      } finally {
        onCreateDuration(now() - createStartedAt);
      }
    } catch (error) {
      const message = String(error?.message || error);
      const noTarget = message.includes("No target with given id") || message.includes("target-disappeared");
      // A failed create may have claimed or invalidated this handle. A fresh connect
      // gets a fresh CDP client; browser.close() would close the shared browser.
      if (phase === "create") {
        connected = false;
        browser = undefined;
      }
      if (noTarget && targetRetries < 2) {
        targetRetries += 1;
        retries += 1;
        onRetry(retries);
        await refreshTarget();
        await sleep(300);
        continue;
      }
      if (/already initialized|initialization is already in progress/u.test(message) && !initRestarted) {
        initRestarted = true;
        retries += 1;
        onRetry(retries);
        await restartRuntime();
        continue;
      }
      if (message.includes("already attached to a Stagehand instance") && !handleReconnected) {
        handleReconnected = true;
        retries += 1;
        onRetry(retries);
        continue;
      }
      const boundary = noTarget ? `stagehand-${phase}-target` : message.startsWith("stagehand-create-unsettled") ? "stagehand-create-unsettled" : message.startsWith("stagehand-create-timeout") ? "stagehand-create-timeout" : /already initialized|initialization is already in progress/u.test(message) ? "runtime-already-initialized" : message.includes("already attached to a Stagehand instance") ? "browser-handle-attached" : `stagehand-${phase}-failed`;
      throw boundaryError(boundary, error);
    }
  }
}

export function sessionStats(file = sessionLogFile(), last = Infinity) {
  let lines;
  try { lines = readFileSync(file, "utf8").split("\n").filter(Boolean); } catch (error) { if (error.code !== "ENOENT") throw error; lines = []; }
  const records = lines.slice(-last).flatMap((line) => { try { return [JSON.parse(line)]; } catch { return []; } });
  const stages = Object.fromEntries(metricKeys.map((key) => {
    const values = records.map((row) => row[key]).filter((value) => Number.isFinite(value)).sort((a, b) => a - b);
    const percentile = (p) => values.length ? values[Math.ceil(p * values.length) - 1] : null;
    return [key, { p50: percentile(0.5), p90: percentile(0.9) }];
  }));
  const counts = new Map();
  for (const row of records) if (row.result && row.result !== "ok") counts.set(row.result, (counts.get(row.result) || 0) + 1);
  const errors = [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, 5).map(([boundary, count]) => ({ boundary, count }));
  return { count: records.length, stages, errors };
}

/** Chrome's id for an unpacked extension: sha256 of its absolute path, first 32 hex digits mapped 0-f → a-p. */
export function extensionIdFor(dir) {
  const hex = createHash("sha256").update(resolve(dir)).digest("hex").slice(0, 32);
  return [...hex].map((digit) => String.fromCharCode(97 + parseInt(digit, 16))).join("");
}

/** Extension flags of the browser process serving `port`, or null when it cannot be read. */
/** How to reach the Stagehand Runtime in an attached browser; throws at once instead of the 60s init timeout. */
export async function attachExtension(port, extensions, { flags = browserFlags(port), cdp: sharedCdp, cdpFactory = browserCdp, sleep = (ms) => new Promise((accept) => setTimeout(accept, ms)) } = {}) {
  if (process.env.STAGEHAND_EXTENSION_ID) return { extensionId: process.env.STAGEHAND_EXTENSION_ID };
  const restart = `restart it with \`run_cdp --replace${extensions.length ? ` -e ${extensions.join(",")}` : ""}\`, which loads the Stagehand Runtime too`;
  if (flags?.loaded.includes(resolve(STAGEHAND_EXTENSION_DIR))) return { extensionId: extensionIdFor(STAGEHAND_EXTENSION_DIR) };
  if (flags?.allowOnly) {
    throw new Error(`stagehand-extension-blocked: the browser at port ${port} runs with --disable-extensions-except=${flags.allowOnly.join(",")}, which disables the Stagehand Runtime; ${restart}`);
  }
  const cdp = sharedCdp || await cdpFactory(port);
  try {
    return await resolveRuntimeExtension(cdp, { sleep, beforeLoad: () => {
      if (flags && !flags.unsafeDebugging) {
        throw new Error(`stagehand-extension-unloadable: the browser at port ${port} lacks --enable-unsafe-extension-debugging, so Stagehand cannot load its runtime; ${restart}`);
      }
    } });
  } finally {
    if (!sharedCdp) cdp.close();
  }
}

export async function resolveRuntimeExtension(cdp, { extensionDir = STAGEHAND_EXTENSION_DIR, sleep = (ms) => new Promise((accept) => setTimeout(accept, ms)), beforeLoad = () => {} } = {}) {
  const extensionId = extensionIdFor(extensionDir);
  const runtimeUrl = `chrome-extension://${extensionId}/service-worker.js`;
  const workerPresent = async () => ((await cdp.call("Target.getTargets")).result?.targetInfos || [])
    .some((target) => target.type === "service_worker" && target.url === runtimeUrl);
  if (await workerPresent()) return { extensionId };
  const wake = await cdp.call("Target.createTarget", { url: `chrome-extension://${extensionId}/wake-service-worker.html` });
  const wakeTargetId = wake.result?.targetId;
  try {
    if (wakeTargetId) {
      for (let attempt = 0; attempt < 20; attempt += 1) {
        if (await workerPresent()) return { extensionId };
        await sleep(100);
      }
    }
    beforeLoad();
    const loaded = await cdp.call("Extensions.loadUnpacked", { path: extensionDir });
    if (!loaded.result?.id) throw new Error(`runtime-extension-load-failed: ${loaded.error?.message || "no extension id"}`);
    return { extensionId: loaded.result.id };
  } finally {
    if (wakeTargetId) await cdp.call("Target.closeTarget", { targetId: wakeTargetId }).catch(() => {});
  }
}

/** Probe page targets over a browser CDP connection, reload unresponsive ones and optionally restart the Stagehand Runtime. */
// An unresponsive page (hung renderer) stalls the Runtime's page attach during init, and a session
// that ended without close() leaves the Runtime "initialized"; both surface as the two errors below.
export async function runDoctor(cdp, { probeTimeoutMs = 1000, settleMs = 200, restartRuntime = false, closeHung = true, runtimeExtensionId = extensionIdFor(STAGEHAND_EXTENSION_DIR), sleep = (ms) => new Promise((accept) => setTimeout(accept, ms)) } = {}) {
  const timed = async (promise) => {
    let timer;
    try { return await Promise.race([promise, new Promise((accept) => { timer = setTimeout(() => accept({ timeout: true }), probeTimeoutMs); })]); }
    catch { return { timeout: true }; }
    finally { clearTimeout(timer); }
  };
  const parallel = async (items, operation) => {
    let index = 0;
    const results = Array(items.length);
    await Promise.all(Array.from({ length: Math.min(8, items.length) }, async () => {
      while (index < items.length) { const at = index++; results[at] = await operation(items[at]); }
    }));
    return results;
  };
  const targets = (await cdp.call("Target.getTargets")).result?.targetInfos || [];
  const pages = targets.filter((target) => target.type === "page");
  const probe = async (target, beforeEval) => {
    const deadline = Date.now() + probeTimeoutMs;
    const remaining = () => Math.max(1, deadline - Date.now());
    const attached = await timed(cdp.call("Target.attachToTarget", { targetId: target.targetId, flatten: true }, undefined, { timeoutMs: remaining() }));
    const sessionId = attached.result?.sessionId;
    if (!sessionId) return false;
    try {
      if (beforeEval) await beforeEval(sessionId);
      if (remaining() <= 1) return false;
      const answer = await timed(cdp.call("Runtime.evaluate", { expression: "document.readyState", returnByValue: true }, sessionId, { timeoutMs: remaining() }));
      return !answer.timeout && !answer.error;
    } finally {
      void cdp.call("Target.detachFromTarget", { sessionId }, undefined, { timeoutMs: probeTimeoutMs }).catch(() => {});
    }
  };
  const firstPass = await parallel(pages, (target) => probe(target));
  const hung = pages.filter((_, index) => !firstPass[index]).map(({ targetId, url }) => ({ targetId, url }));
  const secondPass = await parallel(hung, async (target) => {
    const answered = await probe(target, async (sessionId) => {
      await timed(cdp.call("Page.reload", {}, sessionId, { timeoutMs: probeTimeoutMs }));
      await sleep(settleMs);
    });
    return answered;
  });
  const stillHung = hung.filter((_, index) => !secondPass[index]);
  // A busy-looping renderer ignores reload; closing it is the only fix, so it is opt-in.
  const closed = [];
  if (closeHung) {
    const outcomes = await parallel(stillHung, (target) => timed(cdp.call("Target.closeTarget", { targetId: target.targetId }, undefined, { timeoutMs: probeTimeoutMs })));
    outcomes.forEach((answer, index) => { if (!answer.timeout && !answer.error) closed.push(stillHung[index]); });
  }
  let runtimeRestarted = false;
  if (restartRuntime) {
    const runtimeUrl = `chrome-extension://${runtimeExtensionId}/service-worker.js`;
    let worker = targets.find((target) => target.type === "service_worker" && target.url === runtimeUrl);
    let wakeTargetId;
    try {
      if (!worker) {
        const wake = await timed(cdp.call("Target.createTarget", { url: `chrome-extension://${runtimeExtensionId}/wake-service-worker.html` }));
        wakeTargetId = wake.result?.targetId;
        if (wakeTargetId) {
          for (let attempt = 0; attempt < 20; attempt += 1) {
            const refreshed = (await timed(cdp.call("Target.getTargets")))?.result?.targetInfos || [];
            worker = refreshed.find((target) => target.type === "service_worker" && target.url === runtimeUrl);
            if (worker) break;
            await sleep(100);
          }
        }
      }
      const reloadTargetId = worker?.targetId || wakeTargetId;
      if (reloadTargetId) {
        const oldWorkerId = worker?.targetId || null;
        const attached = await timed(cdp.call("Target.attachToTarget", { targetId: reloadTargetId, flatten: true }));
        const sessionId = attached.result?.sessionId;
        if (sessionId) {
          // Reset any active Stagehand instance via RPC first, then schedule an extension reload.
          const resetExpr = `Promise.resolve(typeof __stagehandReceiveFromHost === "function" ? __stagehandReceiveFromHost(JSON.stringify({ jsonrpc: "2.0", id: 999999, method: "stagehand.close", params: {} })) : null).catch(() => {}).then(() => { setTimeout(() => chrome.runtime.reload(), 50); return true; })`;
          const answer = await timed(cdp.call("Runtime.evaluate", { expression: resetExpr, awaitPromise: true, returnByValue: true }, sessionId));
          runtimeRestarted = !answer.timeout && !answer.error;
          void cdp.call("Target.detachFromTarget", { sessionId }, undefined, { timeoutMs: probeTimeoutMs }).catch(() => {});
          if (runtimeRestarted) {
            await sleep(120);
            for (let i = 0; i < 30; i += 1) {
              const refreshed = (await timed(cdp.call("Target.getTargets")))?.result?.targetInfos || [];
              const nextWorker = refreshed.find((t) => t.type === "service_worker" && t.url === runtimeUrl && t.targetId !== oldWorkerId);
              if (nextWorker) {
                await sleep(150);
                break;
              }
              await sleep(100);
            }
          }
        }
      }
    } finally {
      if (wakeTargetId) void cdp.call("Target.closeTarget", { targetId: wakeTargetId }, undefined, { timeoutMs: probeTimeoutMs }).catch(() => {});
    }
  }
  return { checked: pages.length, hung, stillHung, closed, runtimeRestarted };
}

/** Browser-level CDP connection for runDoctor. */
export async function browserCdp(port, { version: knownVersion, WebSocketClass = WebSocket } = {}) {
  const version = knownVersion || await endpointVersion(port);
  if (!version) throw new Error(`endpoint-absent: no browser answers at http://127.0.0.1:${port}`);
  const socket = new WebSocketClass(version.webSocketDebuggerUrl);
  await new Promise((accept, reject) => { socket.onopen = accept; socket.onerror = () => reject(new Error("endpoint-unreachable: browser websocket failed")); });
  let nextId = 0;
  const pending = new Map();
  let ended = false;
  const rejectPending = (reason) => {
    ended = true;
    for (const { reject, timer } of pending.values()) { clearTimeout(timer); reject(new Error(reason)); }
    pending.clear();
  };
  socket.onclose = () => rejectPending("cdp-socket-closed");
  socket.onerror = () => rejectPending("cdp-socket-error");
  socket.onmessage = (event) => {
    let message;
    try { message = JSON.parse(event.data); } catch { return; }
    const entry = pending.get(message.id);
    if (entry) { clearTimeout(entry.timer); pending.delete(message.id); entry.accept(message); }
  };
  return {
    call: (method, params = {}, sessionId, { timeoutMs = 5000 } = {}) => new Promise((accept, reject) => {
      if (ended || socket.readyState !== WebSocketClass.OPEN) { reject(new Error("cdp-socket-closed")); return; }
      nextId += 1;
      const id = nextId;
      const timer = setTimeout(() => { pending.delete(id); reject(new Error(`cdp-timeout: ${method}`)); }, timeoutMs);
      pending.set(id, { accept, reject, timer });
      try { socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) })); }
      catch (error) { clearTimeout(timer); pending.delete(id); reject(error); }
    }),
    close: () => { rejectPending("cdp-socket-closed"); socket.close(); },
  };
}

/** Hold a DevTools attachment to the Runtime worker for one Stagehand.create attempt. */
export async function holdRuntimeWorker(cdp, { extensionDir = STAGEHAND_EXTENSION_DIR, extensionId = extensionIdFor(extensionDir), sleep = (ms) => new Promise((accept) => setTimeout(accept, ms)) } = {}) {
  const runtimeUrl = `chrome-extension://${extensionId}/service-worker.js`;
  const targetList = async () => (await cdp.call("Target.getTargets")).result?.targetInfos || [];
  let wakeTargetId;
  let worker;
  let sessionId;
  let observedAt;
  const describe = (target) => target && ({ targetId: target.targetId, type: target.type, url: target.url, attached: target.attached, browserContextId: target.browserContextId ?? null });
  const diagnosis = async () => {
    const after = await targetList();
    const same = after.find((target) => target.targetId === worker?.targetId);
    return { before: describe(worker), beforeAt: observedAt, after: describe(same), afterAt: new Date().toISOString(), state: same ? "attach-unavailable" : "target-disappeared" };
  };
  const release = async () => {
    if (sessionId) await cdp.call("Target.detachFromTarget", { sessionId }).catch(() => {});
    if (wakeTargetId) await cdp.call("Target.closeTarget", { targetId: wakeTargetId }).catch(() => {});
  };
  try {
    for (let attempt = 0; attempt < 20; attempt += 1) {
      worker = (await targetList()).find((target) => target.type === "service_worker" && target.url === runtimeUrl);
      if (worker) break;
      if (!wakeTargetId) {
        const answer = await cdp.call("Target.createTarget", { url: `chrome-extension://${extensionId}/wake-service-worker.html` });
        wakeTargetId = answer.result?.targetId;
        if (!wakeTargetId) throw new Error(`runtime-wake-failed: ${answer.error?.message || "no targetId"}`);
      }
      await sleep(100);
    }
    if (!worker) throw new Error("runtime-worker-absent: service worker did not appear after wake");
    observedAt = new Date().toISOString();
    const attached = await cdp.call("Target.attachToTarget", { targetId: worker.targetId, flatten: true });
    sessionId = attached.result?.sessionId;
    if (!sessionId) {
      const detail = await diagnosis();
      throw new Error(`runtime-worker-attach-failed: ${JSON.stringify(detail)}; ${attached.error?.message || "no sessionId"}`);
    }
    return { release, diagnosis };
  } catch (error) {
    await release();
    throw error;
  }
}

export async function createWithRuntimeWorker(cdp, create, options) {
  const keepAlive = await holdRuntimeWorker(cdp, options);
  try {
    return await create(keepAlive);
  } finally {
    await keepAlive.release();
  }
}

function loadEnv() {
  const values = {};
  const envFile = process.env.STAGEHAND_ENV_FILE || join(SKILL_DIR, ".env");
  if (existsSync(envFile)) {
    for (const line of readFileSync(envFile, "utf8").split(/\r?\n/u)) {
      const match = line.match(/^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$/u);
      if (match) values[match[1]] = match[2].replace(/^(["'])(.*)\1$/u, "$2");
    }
  }
  for (const key of ["STAGEHAND_MODEL", "STAGEHAND_API_KEY"]) {
    if (process.env[key]) values[key] = process.env[key];
  }
  return values;
}

function runtimeEntry() {
  const manifest = join(RUNTIME_DIR, "node_modules", "@browserbasehq", "stagehand", "package.json");
  if (!existsSync(manifest)) {
    throw new Error(`Stagehand runtime is not installed in ${RUNTIME_DIR}; run: node ${join(SKILL_DIR, "scripts", "stagehand.mjs")} install`);
  }
  const pkg = JSON.parse(readFileSync(manifest, "utf8"));
  const entry = pkg.exports?.["."]?.import?.default || pkg.exports?.["."]?.import || "dist/index.mjs";
  return { path: join(dirname(manifest), typeof entry === "string" ? entry : "dist/index.mjs"), version: pkg.version };
}

/** zod from the same runtime, for extract() schemas. */
export async function loadZod() {
  return import(pathToFileURL(join(RUNTIME_DIR, "node_modules", "zod", "index.js")).href).catch(
    () => import(pathToFileURL(join(RUNTIME_DIR, "node_modules", "zod", "v4", "index.js")).href),
  );
}

async function endpointVersion(port) {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(1000) });
    const version = await response.json();
    return version.webSocketDebuggerUrl ? version : null;
  } catch {
    return null;
  }
}

export function status() {
  const env = loadEnv();
  let runtime = null;
  try {
    runtime = runtimeEntry().version;
  } catch {
    runtime = null;
  }
  return {
    runtimeDir: RUNTIME_DIR,
    stagehand: runtime,
    model: env.STAGEHAND_MODEL || null,
    apiKeyConfigured: Boolean(env.STAGEHAND_API_KEY),
  };
}

let activeInProcessSession = null;

/** Serialize Runtime use across processes on one CDP port. */
export async function acquireRuntimeLock(port, {
  lockDir = runtimeLockDir(),
  pid = process.pid,
  now = Date.now,
  script = process.argv[1] || "unknown",
  stage = () => "active",
  maxWaitMs = 240_000,
  waitLogMs = 30_000,
  onWait = () => {},
} = {}) {
  mkdirSync(lockDir, { recursive: true });
  const path = join(lockDir, `${port}.sock`);
  const startedAt = now();
  let recoveredStaleOwner = false;
  while (true) {
    const clients = new Set();
    const server = createServer((client) => {
      clients.add(client);
      client.on("close", () => clients.delete(client));
      client.write(`${JSON.stringify({ pid, script, stage: stage(), since: startedAt })}\n`);
    });
    try {
      await new Promise((resolve, reject) => {
        server.once("error", reject);
        server.listen(path, () => { server.removeListener("error", reject); resolve(); });
      });
      let releasePromise;
      return {
        recoveredStaleOwner,
        release: () => {
          if (!releasePromise) releasePromise = new Promise((resolve) => {
            for (const client of clients) client.destroy();
            server.close(resolve);
          });
          return releasePromise;
        },
      };
    } catch (error) {
      if (error?.code !== "EADDRINUSE") throw error;
    }
    const outcome = await new Promise((resolve, reject) => {
      const client = createConnection(path);
      let owner = { pid: "unknown", stage: "unknown" };
      let data = "";
      let connected = false;
      let done = false;
      const finish = (value) => { if (done) return; done = true; clearInterval(logTimer); clearTimeout(timeoutTimer); client.destroy(); resolve(value); };
      const remaining = maxWaitMs - (now() - startedAt);
      const report = () => onWait(owner, now() - startedAt);
      const logTimer = setInterval(report, waitLogMs);
      const timeoutTimer = setTimeout(() => finish({ timeout: true, owner }), Math.max(0, remaining));
      client.on("connect", () => { connected = true; report(); });
      client.on("data", (chunk) => {
        data += chunk;
        if (data.includes("\n")) {
          try { owner = JSON.parse(data.slice(0, data.indexOf("\n"))); } catch {}
          report();
        }
      });
      client.on("error", (error) => {
        if (error.code === "ECONNREFUSED" || error.code === "ENOENT") finish({ stale: error.code === "ECONNREFUSED" });
        else { clearInterval(logTimer); clearTimeout(timeoutTimer); client.destroy(); reject(error); }
      });
      client.on("close", () => { if (connected) finish({}); });
    });
    if (outcome.timeout) throw new Error(`runtime-lock-timeout: port=${port} owner=${outcome.owner.pid} stage=${outcome.owner.stage}`);
    if (outcome.stale) {
      try { unlinkSync(path); recoveredStaleOwner = true; } catch (error) { if (error.code !== "ENOENT") throw error; }
    }
  }
}

export async function closeRuntimeSession(stagehand, runtimeLock) {
  try { await stagehand.close(); }
  finally { await runtimeLock.release(); }
}

export function runtimeResetForRecoveredOwner(runtimeLock, restart) {
  let used = false;
  return async () => {
    if (!runtimeLock.recoveredStaleOwner || used) return;
    used = true;
    await restart();
  };
}

export function installRuntimeSignalHandlers(getStagehand, runtimeLock, { processRef = process, closeTimeoutMs = 2000 } = {}) {
  const signals = ["SIGINT", "SIGTERM", "SIGHUP"];
  let stopping = false;
  const remove = () => { for (const signal of signals) processRef.removeListener(signal, handlers.get(signal)); };
  const handlers = new Map(signals.map((signal) => [signal, async () => {
    if (stopping) return;
    stopping = true;
    try {
      const stagehand = getStagehand();
      if (stagehand) await Promise.race([
        stagehand.close(),
        new Promise((resolve) => setTimeout(resolve, closeTimeoutMs)),
      ]);
    } catch {}
    await runtimeLock.release();
    remove();
    processRef.kill(processRef.pid, signal);
  }]));
  for (const signal of signals) processRef.on(signal, handlers.get(signal));
  return remove;
}

export function enhanceStagehandPage(page) {
  if (!page || typeof page.locator !== "function" || page.__stagehandEnhanced) return page;
  const rawLocatorFactory = page.locator.bind(page);
  const evalOnLocator = async (selector, nth, pageFunction, arg, all = false) => {
    const fnSource = typeof pageFunction === "function" ? pageFunction.toString() : String(pageFunction);
    const parts = String(selector || "").split(">>").map((piece) => piece.trim()).filter(Boolean);
    if (typeof page.evaluate === "function") {
      const direct = await page.evaluate(
        ({ parts: hopParts, nthIndex, fnText, fnArg, evalAll }) => {
          const queryAll = (root, raw) => {
            const trimmed = raw.trim();
            if (/^text=/i.test(trimmed)) {
              const needle = trimmed.replace(/^text=/i, "").trim().replace(/^["']|["']$/g, "");
              return [...root.querySelectorAll("*")].filter((el) => (el.innerText || el.textContent || "").trim().includes(needle));
            }
            const css = trimmed.replace(/^css=/i, "");
            return [...root.querySelectorAll(css)];
          };
          let root = document;
          for (let i = 0; i < hopParts.length - 1; i += 1) {
            const frameEl = queryAll(root, hopParts[i])[0];
            if (!frameEl) return { status: "missing" };
            if (frameEl.tagName === "IFRAME") {
              try {
                if (!frameEl.contentDocument) return { status: "cross-origin" };
                root = frameEl.contentDocument;
              } catch {
                return { status: "cross-origin" };
              }
            } else {
              root = frameEl.shadowRoot || frameEl;
            }
          }
          const matches = queryAll(root, hopParts[hopParts.length - 1] || "*");
          const fn = (0, eval)(`(${fnText})`);
          if (evalAll) return { status: "ok", value: fn(matches, fnArg) };
          const target = matches[nthIndex];
          if (!target) return { status: "missing" };
          return { status: "ok", value: fn(target, fnArg) };
        },
        { parts, nthIndex: nth, fnText: fnSource, fnArg: arg, evalAll: all },
      );
      if (direct && direct.status === "ok") return direct.value;
      if (direct && direct.status === "cross-origin" && parts.length > 1) {
        const framePrefix = parts.slice(0, -1).join(" >> ");
        const tailSelector = parts[parts.length - 1];
        const bodyHtml = await rawLocatorFactory(`${framePrefix} >> body`).innerHtml();
        return await page.evaluate(
          ({ html, tail, nthIndex, fnText, fnArg, evalAll }) => {
            const template = document.createElement("template");
            template.innerHTML = html;
            const css = tail.trim().replace(/^css=/i, "");
            const matches = [...template.content.querySelectorAll(css)];
            const fn = (0, eval)(`(${fnText})`);
            if (evalAll) return fn(matches, fnArg);
            const target = matches[nthIndex];
            if (!target) throw new Error(`locator.evaluate: no element matching ${tail} (nth=${nthIndex})`);
            return fn(target, fnArg);
          },
          { html: bodyHtml, tail: tailSelector, nthIndex: nth, fnText: fnSource, fnArg: arg, evalAll: all },
        );
      }
      if (direct && direct.status === "missing") {
        throw new Error(`locator.evaluate: no element matching ${selector} (nth=${nth})`);
      }
    }
    throw new Error(`locator.evaluate is unavailable for selector: ${selector}`);
  };
  const wrapLocator = (loc, selector, nth = 0) => {
    if (!loc || typeof loc !== "object") return loc;
    if (typeof loc.evaluate !== "function") {
      loc.evaluate = (pageFunction, arg) => evalOnLocator(loc.descriptor?.selector ?? selector, loc.descriptor?.nth ?? nth, pageFunction, arg, false);
    }
    if (typeof loc.evaluateAll !== "function") {
      loc.evaluateAll = (pageFunction, arg) => evalOnLocator(loc.descriptor?.selector ?? selector, loc.descriptor?.nth ?? nth, pageFunction, arg, true);
    }
    if (typeof loc.first === "function") {
      const origFirst = loc.first.bind(loc);
      loc.first = () => wrapLocator(origFirst(), loc.descriptor?.selector ?? selector, 0);
    }
    if (typeof loc.nth === "function") {
      const origNth = loc.nth.bind(loc);
      loc.nth = (index) => wrapLocator(origNth(index), loc.descriptor?.selector ?? selector, index);
    }
    return loc;
  };
  page.locator = (selector) => wrapLocator(rawLocatorFactory(selector), selector, 0);
  Object.defineProperty(page, "__stagehandEnhanced", { value: true, configurable: true });
  return page;
}

export async function openStagehand({
  url,
  waitForSelector,
  waitForTimeoutMs = 15_000,
  extensions = [],
  port = DEFAULT_PORT,
  keepPages = false,
  logSteps = Boolean(process.env.STAGEHAND_STEP_LOG),
  leaseTimeoutMs = process.env.STAGEHAND_LEASE_TIMEOUT_MS ? Number(process.env.STAGEHAND_LEASE_TIMEOUT_MS) : 0,
  runtimeAdapters = {},
} = {}) {
  if (typeof url === "string" && /^(?:chrome|devtools):\/\//u.test(url)) {
    throw new Error(`stagehand-internal-page-unsupported: ${new URL(url).protocol}`);
  }
  const startedAt = Date.now();
  const metrics = emptyMetrics();
  let retries = 0;
  let mode = null;
  let summaryWritten = false;
  const recordSummary = (result, tabs = 0) => {
    if (summaryWritten) return;
    summaryWritten = true;
    const summary = { ...metrics, retries, tabs, mode };
    if (result === "ok") process.stderr.write(`${formatSessionSummary(metrics, retries, tabs, mode)}\n`);
    const file = runtimeAdapters.sessionLogFile || sessionLogFile();
    optionalStateWrite(file, () => {
      mkdirSync(dirname(file), { recursive: true });
      appendFileSync(file, `${JSON.stringify({ ts: new Date().toISOString(), ...summary, result })}\n`);
    });
  };
  let currentStage = "init";
  const logStep = (stage, extra = "", force = false) => {
    currentStage = stage;
    const elapsedMs = Date.now() - startedAt;
    if (force || logSteps || elapsedMs >= 3000) {
      process.stderr.write(`[stagehand-step] ${stage} +${elapsedMs}ms${extra ? ` (${extra})` : ""}\n`);
    }
  };
  const watchdog = setInterval(() => {
    const elapsedMs = Date.now() - startedAt;
    const handles = (process._getActiveHandles?.() || []).map((h) => h?.constructor?.name || "Handle");
    process.stderr.write(`[stagehand-bottleneck] stage=${currentStage} elapsed=${elapsedMs}ms activeHandles=${handles.join(",")}\n`);
  }, 60000);
  watchdog.unref?.();
  if (activeInProcessSession) {
    try {
      await activeInProcessSession.close();
    } catch {}
    activeInProcessSession = null;
  }
  let runtimeLock;
  try {
    runtimeLock = await acquireRuntimeLock(port, {
      lockDir: runtimeAdapters.lockDir || runtimeLockDir(),
      stage: () => currentStage,
      onWait: (owner, elapsedMs) => process.stderr.write(`[stagehand-step] runtime-lock-wait owner=${owner.pid} stage=${owner.stage} +${elapsedMs}ms\n`),
    });
  } catch (error) {
    metrics.lockWaitMs = Date.now() - startedAt;
    recordSummary(errorBoundary(error));
    clearInterval(watchdog);
    throw error;
  }
  metrics.lockWaitMs = Date.now() - startedAt;
  logStep("runtime-lock-acquired", `port=${port}`, true);
  let stagehand;
  let sharedCdp;
  const removeSignalHandlers = installRuntimeSignalHandlers(() => stagehand, runtimeLock);
  process.once("exit", runtimeLock.release);
  try {
  const env = runtimeAdapters.env || loadEnv();
  const entry = runtimeAdapters.entry || runtimeEntry();
  const { localBrowser, Stagehand } = runtimeAdapters.browserRuntime || await import(pathToFileURL(entry.path).href);
  let browser;
  const ensureStartedAt = Date.now();
  let runtimeExtensionId = extensionIdFor(STAGEHAND_EXTENSION_DIR);
  let knownVersion;
  let knownFlags;
  const getCdp = async () => {
    if (!sharedCdp) {
      knownVersion ??= await (runtimeAdapters.endpointVersion || endpointVersion)(port);
      sharedCdp = await (runtimeAdapters.browserCdp || browserCdp)(port, { version: knownVersion });
    }
    return sharedCdp;
  };
  const pagesFile = join(runtimeAdapters.lockDir || runtimeLockDir(), `${port}.pages.json`);
  const ownedTargets = new Set();
  const persistPages = () => {
    optionalStateWrite(pagesFile, () => {
      mkdirSync(dirname(pagesFile), { recursive: true });
      writeFileSync(pagesFile, JSON.stringify([...ownedTargets]));
    });
  };
  const pageTargets = async () => ((await (await getCdp()).call("Target.getTargets")).result?.targetInfos || []).filter((target) => target.type === "page");
  const closeRecordedPages = async () => {
    let old = [];
    try { old = JSON.parse(readFileSync(pagesFile, "utf8")); } catch {}
    const existing = new Set((await pageTargets()).map((target) => target.targetId));
    for (const targetId of old) if (existing.has(targetId)) await (await getCdp()).call("Target.closeTarget", { targetId }).catch(() => {});
    try { unlinkSync(pagesFile); } catch {}
  };
  const closeDuplicateBlanks = async () => {
    const blanks = (await pageTargets()).filter((target) => target.url === "about:blank");
    for (const target of blanks.slice(1)) await (await getCdp()).call("Target.closeTarget", { targetId: target.targetId }).catch(() => {});
  };
  let doctorRan = false;
  const runDoctorOnce = async (force = false) => {
    if (doctorRan && !force) return;
    doctorRan = true;
    const began = Date.now();
    try {
      await (runtimeAdapters.runDoctor || runDoctor)(await getCdp(), { restartRuntime: force, closeHung: true, runtimeExtensionId });
    } catch (error) { if (force) throw error; }
    finally { metrics.doctorMs += Date.now() - began; }
  };
  let connectionOptions;
  const connectOrLaunch = async () => {
    if (!connectionOptions) {
      const ensured = await (runtimeAdapters.ensureBrowser || ensureBrowser)({ port, extensions, lock: runtimeLock });
      knownVersion = ensured.version;
      knownFlags = ensured.flags;
      const required = [...extensions, STAGEHAND_EXTENSION_DIR].filter((dir) => existsSync(join(dir, "manifest.json")));
      await (runtimeAdapters.ensureExtensions || ensureExtensions)(await getCdp(), required, { flags: knownFlags });
      await closeRecordedPages();
      await closeDuplicateBlanks();
      connectionOptions = await (runtimeAdapters.attachExtension || attachExtension)(port, extensions, { flags: knownFlags, cdp: await getCdp() });
      runtimeExtensionId = connectionOptions.extensionId;
      if (!runtimeLock.recoveredStaleOwner) await runDoctorOnce();
    }
    logStep("cdp-attach-start", `port=${port}`);
    browser = await localBrowser.connect({ cdpUrl: `http://127.0.0.1:${port}`, ...connectionOptions });
    mode = "attach";
    logStep("cdp-attach-done", `mode=${mode}`);
    return browser;
  };
  const model = env.STAGEHAND_MODEL && env.STAGEHAND_API_KEY
    ? { model: { modelName: env.STAGEHAND_MODEL, apiKey: env.STAGEHAND_API_KEY } }
    : {};
  const resetRecoveredRuntime = runtimeResetForRecoveredOwner(runtimeLock, () => runDoctorOnce(true));
  metrics.ensureMs = Date.now() - ensureStartedAt;
  const retryResult = await connectAndCreate({
    onRetry: (count) => { retries = count; },
    onCreateDuration: (ms) => { metrics.createMs += ms; },
    connect: async () => {
      const began = Date.now();
      try { const connectedBrowser = await connectOrLaunch(); await resetRecoveredRuntime(); return connectedBrowser; }
      finally { metrics.connectMs += Date.now() - began; }
    },
    create: async (_browser, attempt) => {
      try {
        logStep("stagehand-create-start");
        process.stderr.write(`[record-log] 00119805781 | Stagehand.create | attempt=${attempt} ack=start\n`);
        const created = await (runtimeAdapters.createWithRuntimeWorker || createWithRuntimeWorker)(await getCdp(), async (keepAlive) => {
          try { return await Stagehand.create({ browser, ...model, logging: { level: "off" } }); }
          catch (error) {
            if (String(error?.message || error).includes("No target with given id")) {
              try { process.stderr.write(`[stagehand-target] ${JSON.stringify({ attempt, detail: await keepAlive.diagnosis() })}\n`); }
              catch (diagnosisError) { process.stderr.write(`[stagehand-target] ${JSON.stringify({ attempt, diagnosisError: String(diagnosisError) })}\n`); }
            }
            throw error;
          }
        }, { extensionId: runtimeExtensionId });
        logStep("stagehand-create-done");
        process.stderr.write(`[record-log] 00119805781 | Stagehand.create | attempt=${attempt} ack=ok\n`);
        return created;
      } catch (error) {
        process.stderr.write(`[record-log] 00119805781 | Stagehand.create | attempt=${attempt} ack=error error=${errorBoundary(error)}\n`);
        throw error;
      }
    },
    refreshTarget: async () => { await (await getCdp()).call("Target.getTargets"); },
    doctor: async () => {
      const began = Date.now();
      try { await (runtimeAdapters.runDoctor || runDoctor)(await getCdp(), { closeHung: true }); }
      finally { metrics.doctorMs += Date.now() - began; }
    },
    restartRuntime: () => runDoctorOnce(true),
    createTimeoutMs: Number(process.env.STAGEHAND_CREATE_TIMEOUT_MS) || 20_000,
  });
  stagehand = retryResult.stagehand;
  retries = retryResult.retries;
  const pageStartedAt = Date.now();
  const context = stagehand.browser.context;
  const originalNewPage = context.newPage.bind(context);
  context.newPage = async (...args) => {
    const before = new Set((await pageTargets()).map((target) => target.targetId));
    const created = enhanceStagehandPage(await originalNewPage(...args));
    for (const target of await pageTargets()) if (!before.has(target.targetId)) ownedTargets.add(target.targetId);
    persistPages();
    return created;
  };
  const pages = (await context.pages()).map((candidate) => enhanceStagehandPage(candidate));
  const pageUrls = await Promise.all(pages.map(async (candidate) => {
    let timer;
    try { return await Promise.race([candidate.url(), new Promise((resolve) => { timer = setTimeout(() => resolve(null), 1000); })]); }
    catch { return null; }
    finally { clearTimeout(timer); }
  }));
  let page;
  if (url) {
    logStep("page-resolve-start");
    // Reuse a blank tab or one already on this URL; page URLs may be async.
    for (const [index, candidate] of pages.entries()) {
      const current = pageUrls[index];
      if (current === "about:blank" || current === url || (current && url.endsWith("/") && current.startsWith(url))) {
        page = candidate;
        break;
      }
    }
    page = enhanceStagehandPage(page || (await stagehand.browser.context.newPage()));
    const currentUrl = pages.includes(page) ? pageUrls[pages.indexOf(page)] : "about:blank";
    if (currentUrl !== url && !(currentUrl && url.endsWith("/") && currentUrl.startsWith(url))) {
      await page.goto(url);
    }
    logStep("page-resolve-done");
  } else {
    for (const [index, candidate] of pages.entries()) {
      const current = pageUrls[index];
      if (current && !current.startsWith("chrome-extension://") && !current.startsWith("devtools://")) {
        page = candidate;
        break;
      }
    }
    page = enhanceStagehandPage(page || (await stagehand.browser.context.newPage()));
  }
  if (waitForSelector && page && typeof page.waitForSelector === "function") {
    logStep("page-wait-selector-start", waitForSelector);
    await page.waitForSelector(waitForSelector, { timeout: waitForTimeoutMs });
    logStep("page-wait-selector-done", waitForSelector);
  }
  const version = knownVersion || await (runtimeAdapters.endpointVersion || endpointVersion)(port);
  logStep("session-ready");
  metrics.pageMs = Date.now() - pageStartedAt;
  recordSummary("ok", pages.length);
  let closed = false;
  const closeSession = async () => {
    if (closed) return;
    closed = true;
    if (activeInProcessSession === session) {
      activeInProcessSession = null;
    }
    clearInterval(watchdog);
    clearInterval(idleTimer);
    if (leaseTimer) clearTimeout(leaseTimer);
    logStep("session-close-start");
    try {
      if (!keepPages) {
        for (const targetId of ownedTargets) await (await getCdp()).call("Target.closeTarget", { targetId }).catch(() => {});
      }
      try { unlinkSync(pagesFile); } catch {}
      await closeRuntimeSession(stagehand, runtimeLock);
    } catch (error) {
      closed = false;
      activeInProcessSession = session;
      process.stderr.write(`[stagehand-step] session-close-error (${error instanceof Error ? error.message : String(error)})\n`);
      throw error;
    }
    process.removeListener("exit", runtimeLock.release);
    removeSignalHandlers();
    sharedCdp?.close();
    logStep("session-close-done");
    for (const handle of process._getActiveHandles?.() || []) {
      if (handle && handle !== process.stdout && handle !== process.stderr && handle !== process.stdin) {
        try { handle.unref?.(); } catch {}
      }
    }
  };
  const idleTimer = setInterval(() => {
    process.stderr.write(`[stagehand-step] session-idle stage=${currentStage} +${Date.now() - startedAt}ms\n`);
  }, 300_000);
  idleTimer.unref?.();
  const leaseTimer = leaseTimeoutMs > 0 ? setTimeout(() => { void closeSession(); }, leaseTimeoutMs) : null;
  leaseTimer?.unref?.();
  const session = {
    stagehand,
    page,
    pages,
    mode,
    aiEnabled: Boolean(model.model),
    endpoint: `http://127.0.0.1:${port}`,
    browserVersion: version?.Browser,
    headless: /Headless/i.test(version?.["User-Agent"] || ""),
    stagehandVersion: entry.version,
    logStep,
    close: closeSession,
  };
  activeInProcessSession = session;
  return session;
  } catch (error) {
    recordSummary(errorBoundary(error));
    if (stagehand) {
      try { await stagehand.close(); } catch {}
    }
    sharedCdp?.close();
    await runtimeLock.release();
    process.removeListener("exit", runtimeLock.release);
    removeSignalHandlers();
    clearInterval(watchdog);
    throw error;
  }
}

// Run as a CLI also when started through a symlinked skill path.
const invokedPath = process.argv[1] ? realpathSync(process.argv[1]) : "";
if (invokedPath && realpathSync(fileURLToPath(import.meta.url)) === invokedPath) {
  const [command = "status", ...rest] = process.argv.slice(2);
  const options = {};
  for (let index = 0; index < rest.length; index += 1) {
    const flag = rest[index];
    const value = rest[index + 1];
    if (flag === "--url") { options.url = value; index += 1; }
    else if (flag === "--extension") { options.extensions = [...(options.extensions || []), ...value.split(",").filter(Boolean)]; index += 1; }
    else if (flag === "--port") { options.port = Number(value); index += 1; }
    else if (flag === "--data-dir") { options.profileDir = value; index += 1; }
    else if (flag === "--browser") { options.chromium = value; index += 1; }
    else if (flag === "--replace") { options.replace = true; }
    else if (flag === "--detach") { options.detach = true; }
    else if (flag === "--headless") { options.headless = true; }
    else if (flag === "--no-extension") { options.noExtension = true; }
    else if (flag === "--restart-runtime") { options.restartRuntime = true; }
    else if (flag === "--close-hung") { options.closeHung = true; }
    else if (flag === "--last") {
      options.last = Number(value);
      if (!Number.isSafeInteger(options.last) || options.last < 1) { console.error(JSON.stringify({ ok: false, error: "--last must be a positive integer" })); process.exit(2); }
      index += 1;
    }
    else { console.error(JSON.stringify({ ok: false, error: `unknown option ${flag}` })); process.exit(2); }
  }
  try {
    if (command === "browser") {
      const port = options.port || DEFAULT_PORT;
      const logBroken = existsSync(`/tmp/run_cdp-${port}.log`) && readFileSync(`/tmp/run_cdp-${port}.log`, "utf8").includes("DidStartWorkerFail");
      if (!options.replace && (browserFlags(port)?.allowOnly || logBroken)) options.replace = true;
      const result = await ensureBrowser(options);
      console.log(JSON.stringify({ endpoint: `http://127.0.0.1:${options.port || DEFAULT_PORT}`, launched: result.launched, pid: result.pid || null, browser: result.version.Browser || null, versions: result.versions || {} }));
      process.exit(0);
    } else if (command === "status") {
      console.log(JSON.stringify(status()));
    } else if (command === "stats") {
      console.log(JSON.stringify(sessionStats(sessionLogFile(), options.last ?? Infinity)));
    } else if (command === "install") {
      // The pinned versions live in this skill's runtime/package.json.
      mkdirSync(RUNTIME_DIR, { recursive: true });
      copyFileSync(join(SKILL_DIR, "runtime", "package.json"), join(RUNTIME_DIR, "package.json"));
      const result = spawnSync("npm", ["install", "--no-audit", "--no-fund", "--prefix", RUNTIME_DIR], {
        cwd: RUNTIME_DIR,
        stdio: "inherit",
        env: process.env,
      });
      if (result.status !== 0) process.exit(result.status ?? 1);
      console.log(JSON.stringify(status()));
    } else if (command === "doctor") {
      const cdp = await browserCdp(options.port || DEFAULT_PORT);
      try {
        console.log(JSON.stringify(await runDoctor(cdp, { restartRuntime: options.restartRuntime === true, closeHung: options.closeHung === true })));
      } finally {
        cdp.close();
      }
      process.exit(0);
    } else if (command === "open") {
      const session = await openStagehand(options);
      const title = session.page ? await session.page.title() : "";
      console.log(JSON.stringify({
        mode: session.mode,
        endpoint: session.endpoint,
        browser: session.browserVersion,
        headless: session.headless,
        stagehand: session.stagehandVersion,
        aiEnabled: session.aiEnabled,
        url: session.page ? String(await session.page.url()).split("?")[0] : null,
        title,
      }));
      await session.close();
      process.exit(0);
    } else {
      console.error(JSON.stringify({ ok: false, error: `unknown command ${command}` }));
      process.exit(2);
    }
  } catch (error) {
    console.error(JSON.stringify({ ok: false, error: error instanceof Error ? error.message : String(error) }));
    process.exit(1);
  }
}
