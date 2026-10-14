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
import { copyFileSync, existsSync, mkdirSync, readFileSync, realpathSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const SKILL_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const RUNTIME_DIR = process.env.STAGEHAND_RUNTIME_DIR || "/home/tree/.local/share/access-stagehand";
export const DEFAULT_PORT = 9222;
export const PROFILE_DIR = "/home/tree/.config/codex-run-cdp";
export const CHROMIUM = "/home/tree/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome";
// Stagehand's own "Stagehand Runtime" extension; its service worker must run in the browser.
export const STAGEHAND_EXTENSION_DIR = join(RUNTIME_DIR, "node_modules", "@browserbasehq", "stagehand", "dist", "extension");

/** Chrome's id for an unpacked extension: sha256 of its absolute path, first 32 hex digits mapped 0-f → a-p. */
export function extensionIdFor(dir) {
  const hex = createHash("sha256").update(resolve(dir)).digest("hex").slice(0, 32);
  return [...hex].map((digit) => String.fromCharCode(97 + parseInt(digit, 16))).join("");
}

/** Extension flags of the browser process serving `port`, or null when it cannot be read. */
export function browserFlags(port) {
  const ps = spawnSync("ps", ["-eo", "args="], { encoding: "utf8" });
  const line = (ps.stdout || "").split("\n").find((args) => args.includes(`--remote-debugging-port=${port}`) && !args.includes("--type="));
  if (!line) return null;
  const list = (name) => {
    const match = line.match(new RegExp(`--${name}=(\\S+)`, "u"));
    return match ? match[1].split(",").filter(Boolean).map((dir) => resolve(dir)) : null;
  };
  return {
    loaded: list("load-extension") || [],
    allowOnly: list("disable-extensions-except"),
    unsafeDebugging: line.includes("--enable-unsafe-extension-debugging"),
  };
}

/** How to reach the Stagehand Runtime in an attached browser; throws at once instead of the 60s init timeout. */
function attachExtension(port, extensions) {
  if (process.env.STAGEHAND_EXTENSION_ID) return { extensionId: process.env.STAGEHAND_EXTENSION_ID };
  const flags = browserFlags(port);
  if (!flags) return {};
  const restart = `restart it with \`run_cdp --replace${extensions.length ? ` -e ${extensions.join(",")}` : ""}\`, which loads the Stagehand Runtime too`;
  const missing = extensions.map((dir) => resolve(dir)).filter((dir) => !flags.loaded.includes(dir));
  if (missing.length) {
    throw new Error(`extension-not-loaded: the browser at port ${port} did not load ${missing.join(", ")}; ${restart}`);
  }
  if (flags.loaded.includes(resolve(STAGEHAND_EXTENSION_DIR))) return { extensionId: extensionIdFor(STAGEHAND_EXTENSION_DIR) };
  if (flags.allowOnly) {
    throw new Error(`stagehand-extension-blocked: the browser at port ${port} runs with --disable-extensions-except=${flags.allowOnly.join(",")}, which disables the Stagehand Runtime; ${restart}`);
  }
  if (!flags.unsafeDebugging) {
    throw new Error(`stagehand-extension-unloadable: the browser at port ${port} lacks --enable-unsafe-extension-debugging, so Stagehand cannot load its runtime; ${restart}`);
  }
  return {};
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

export async function openStagehand({ url, extensions = [], port = DEFAULT_PORT } = {}) {
  const env = loadEnv();
  const entry = runtimeEntry();
  const { localBrowser, Stagehand } = await import(pathToFileURL(entry.path).href);
  let browser;
  let mode;
  if (await endpointVersion(port)) {
    const extension = attachExtension(port, extensions);
    // Stagehand loads its helper extension on connect; the first attempts can
    // race its service worker ("No target with given id").
    for (let attempt = 1; ; attempt += 1) {
      try {
        browser = await localBrowser.connect({ cdpUrl: `http://127.0.0.1:${port}`, ...extension });
        break;
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (!message.includes("No target with given id") || attempt === 4) throw error;
        await new Promise((accept) => setTimeout(accept, 1000));
      }
    }
    mode = "attach";
  } else {
    // The allow-list must keep the Stagehand Runtime, or its service worker never starts.
    const allowed = [...extensions.map((dir) => resolve(dir)), STAGEHAND_EXTENSION_DIR].join(",");
    const args = extensions.length ? [`--load-extension=${allowed}`, `--disable-extensions-except=${allowed}`] : [];
    browser = await localBrowser.launch({
      headless: false,
      executablePath: CHROMIUM,
      userDataDir: PROFILE_DIR,
      preserveUserDataDir: true,
      port,
      keepAlive: true,
      args,
    });
    mode = "launch";
  }
  const model = env.STAGEHAND_MODEL && env.STAGEHAND_API_KEY
    ? { model: { modelName: env.STAGEHAND_MODEL, apiKey: env.STAGEHAND_API_KEY } }
    : {};
  const stagehand = await Stagehand.create({ browser, ...model, logging: { level: "off" } });
  let page;
  if (url) {
    // Reuse a blank tab or one already on this URL; page URLs may be async.
    for (const candidate of await stagehand.browser.context.pages()) {
      const current = await candidate.url();
      if (current === "about:blank" || current === url) {
        page = candidate;
        break;
      }
    }
    page = page || (await stagehand.browser.context.newPage());
    if ((await page.url()) !== url) await page.goto(url);
  }
  const version = await endpointVersion(port);
  return {
    stagehand,
    page,
    mode,
    aiEnabled: Boolean(model.model),
    endpoint: `http://127.0.0.1:${port}`,
    browserVersion: version?.Browser,
    headless: /Headless/i.test(version?.["User-Agent"] || ""),
    stagehandVersion: entry.version,
    close: () => stagehand.close(),
  };
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
    else if (flag === "--extension") { options.extensions = value.split(",").filter(Boolean); index += 1; }
    else if (flag === "--port") { options.port = Number(value); index += 1; }
    else { console.error(JSON.stringify({ ok: false, error: `unknown option ${flag}` })); process.exit(2); }
  }
  try {
    if (command === "status") {
      console.log(JSON.stringify(status()));
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
