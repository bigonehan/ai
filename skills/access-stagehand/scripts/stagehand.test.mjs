import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runDoctor, extensionIdFor, STAGEHAND_EXTENSION_DIR } from "./stagehand.mjs";

const SKILL_DIR = new URL("..", import.meta.url);
const runtimeId = extensionIdFor(STAGEHAND_EXTENSION_DIR);

// Fake browser CDP: `hung` targets never answer Runtime.evaluate until reloaded.
function fakeCdp(targets, hung) {
  const frozen = new Set(hung);
  const calls = [];
  const sessions = new Map();
  let nextSession = 0;
  return {
    calls,
    async call(method, params = {}, sessionId) {
      calls.push(`${method}${sessionId ? `@${sessions.get(sessionId)}` : ""}`);
      if (method === "Target.getTargets") return { result: { targetInfos: targets } };
      if (method === "Target.attachToTarget") { const id = `s${nextSession += 1}`; sessions.set(id, params.targetId); return { result: { sessionId: id } }; }
      if (method === "Target.detachFromTarget") return { result: {} };
      const target = sessions.get(sessionId);
      if (method === "Page.reload") { frozen.delete(target); return { result: {} }; }
      if (method === "Runtime.evaluate") return frozen.has(target) ? new Promise(() => {}) : { result: { result: { value: "complete" } } };
      return { result: {} };
    },
  };
}

const targets = [
  { targetId: "A", type: "page", url: "http://localhost:8484/o/docs/doc/x/p/47" },
  { targetId: "B", type: "page", url: "http://localhost:5184/" },
  { targetId: "C", type: "page", url: "http://localhost:8484/o/docs/" },
  { targetId: "W", type: "service_worker", url: `chrome-extension://${runtimeId}/service-worker.js` },
];

test("doctor reloads unresponsive page targets and restarts the Stagehand Runtime", async () => {
  const cdp = fakeCdp(targets, ["B", "C"]);
  const report = await runDoctor(cdp, { probeTimeoutMs: 20, settleMs: 0, sleep: async () => {} });
  console.log(JSON.stringify(report));
  assert.deepEqual(report.hung.map((target) => target.url), ["http://localhost:5184/", "http://localhost:8484/o/docs/"]);
  assert.deepEqual(report.stillHung, []);
  assert.equal(report.runtimeRestarted, true);
  assert.equal(cdp.calls.filter((call) => call.startsWith("Page.reload")).length, 2);
  assert.ok(cdp.calls.includes("Runtime.evaluate@W"));
});

test("doctor only reports when every page answers and no runtime restart is asked", async () => {
  const cdp = fakeCdp(targets, []);
  const report = await runDoctor(cdp, { probeTimeoutMs: 20, settleMs: 0, sleep: async () => {} });
  console.log(JSON.stringify(report));
  assert.deepEqual(report.hung, []);
  assert.equal(report.runtimeRestarted, false);
  assert.equal(report.checked, 3);
  assert.equal(cdp.calls.some((call) => call.startsWith("Page.reload") || call.endsWith("@W")), false);
});

test("the attach notes record the stuck-runtime case and SKILL.md makes agents read and update them", () => {
  let notes = "";
  try { notes = readFileSync(new URL("references/attach-notes.md", SKILL_DIR), "utf8"); } catch {}
  const skill = readFileSync(new URL("SKILL.md", SKILL_DIR), "utf8");
  for (const text of ["A Stagehand instance is already initialized", "Stagehand initialization is already in progress", "doctor"]) assert.ok(notes.includes(text), text);
  assert.ok(skill.includes("references/attach-notes.md"));
  assert.ok(/doctor/u.test(skill));
});

test("doctor closes pages that stay unresponsive after reload only when asked", async () => {
  const stuck = fakeCdp(targets, ["B"]);
  const original = stuck.call;
  stuck.call = async (method, params, sessionId) => (method === "Page.reload" ? { result: {} } : original(method, params, sessionId));
  const kept = await runDoctor(stuck, { probeTimeoutMs: 20, settleMs: 0, sleep: async () => {} });
  const closed = await runDoctor(stuck, { probeTimeoutMs: 20, settleMs: 0, sleep: async () => {}, closeHung: true });
  console.log(JSON.stringify(kept), JSON.stringify(closed));
  assert.deepEqual(kept.stillHung.map((target) => target.targetId), ["B"]);
  assert.deepEqual(kept.closed, []);
  assert.deepEqual(closed.closed.map((target) => target.targetId), ["B"]);
  assert.equal(stuck.calls.filter((call) => call === "Target.closeTarget").length, 1);
});
