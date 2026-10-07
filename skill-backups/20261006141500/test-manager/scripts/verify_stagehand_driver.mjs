#!/usr/bin/env node
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";

function validateEvidence(evidence) {
  const errors = [];
  const receipts = Array.isArray(evidence?.receipts) ? evidence.receipts : [];
  const runtime = receipts
    .map((receipt) => receipt?.observation)
    .filter((observation) => observation?.phase === "post_fix" && observation?.observation_level === "runtime");
  if (runtime.length === 0) errors.push("Stagehand gate requires at least one post_fix runtime observation");
  for (const [index, observation] of runtime.entries()) {
    const field = `runtime[${index}]`;
    if (observation?.input_fidelity?.driver !== "Stagehand v4 observe/act/extract") {
      errors.push(`${field} driver must be Stagehand v4 observe/act/extract`);
    }
    if (observation?.runtime_provenance?.automation !== "Stagehand v4") {
      errors.push(`${field} provenance must identify Stagehand v4`);
    }
    if (observation?.mocked !== false || observation?.entry_stage !== "original_input") {
      errors.push(`${field} must be non-mocked and start at original_input`);
    }
    if (!Array.isArray(observation?.substitutions) || observation.substitutions.length !== 0) {
      errors.push(`${field} must contain no substitutions`);
    }
    const sourceCapture = observation?.driver_source_capture;
    if (!sourceCapture?.path || !/^[a-f0-9]{64}$/.test(sourceCapture.sha256 ?? "")) {
      errors.push(`${field} must include a hashed Stagehand runner source capture`);
    } else {
      try {
        const source = readFileSync(sourceCapture.path, "utf8");
        const digest = createHash("sha256").update(source).digest("hex");
        if (digest !== sourceCapture.sha256) errors.push(`${field} Stagehand runner source hash does not match`);
        if (/\.goto\s*\(|\.url\s*\(|connectOverCDP|sendCommand\s*\(/.test(source)) {
          errors.push(`${field} Stagehand runner source contains raw page/CDP control`);
        }
        if (!source.includes("stagehand.observe(") || !source.includes("stagehand.act(") || !source.includes("stagehand.extract(")) {
          errors.push(`${field} Stagehand runner source does not use observe/act/extract`);
        }
      } catch (error) {
        errors.push(`${field} Stagehand runner source is unreadable: ${error.message}`);
      }
    }
    const serialized = JSON.stringify({
      driver: observation?.input_fidelity?.driver,
      automation: observation?.runtime_provenance?.automation,
      automationTrace: observation?.runtime_provenance?.automation_trace
    }).toLowerCase();
    if (serialized.includes("playwright") || serialized.includes("direct cdp") || serialized.includes("connectovercdp")) {
      errors.push(`${field} contains a forbidden Playwright/direct-CDP automation driver`);
    }
  }
  return errors;
}

function selfTest() {
  const sourcePath = "/home/tree/extensions/jw_downloader/scripts/stagehand-runtime.mjs";
  const sourceHash = createHash("sha256").update(readFileSync(sourcePath)).digest("hex");
  const observation = {
    phase: "post_fix",
    observation_level: "runtime",
    mocked: false,
    entry_stage: "original_input",
    substitutions: [],
    driver_source_capture: { path: sourcePath, sha256: sourceHash },
    input_fidelity: { driver: "Stagehand v4 observe/act/extract" },
    runtime_provenance: { automation: "Stagehand v4" }
  };
  const evidence = { version: 6, receipts: [{ observation }] };
  const checks = [
    ["Stagehand runtime accepted", evidence, true],
    ["Playwright mutant rejected", { version: 6, receipts: [{ observation: { ...observation, input_fidelity: { driver: "Playwright" } } }] }, false],
    ["direct CDP mutant rejected", { version: 6, receipts: [{ observation: { ...observation, runtime_provenance: { automation: "direct CDP" } } }] }, false],
    ["mocked mutant rejected", { version: 6, receipts: [{ observation: { ...observation, mocked: true } }] }, false],
    ["middle-entry mutant rejected", { version: 6, receipts: [{ observation: { ...observation, entry_stage: "handler" } }] }, false]
  ];
  const results = checks.map(([name, value, expected]) => ({ name, passed: (validateEvidence(value).length === 0) === expected }));
  const ok = results.every((item) => item.passed);
  process.stdout.write(`${JSON.stringify({ ok, checks: results }, null, 2)}\n`);
  return ok ? 0 : 1;
}

async function main() {
  const [command, path] = process.argv.slice(2);
  if (command === "self-test") process.exit(selfTest());
  if (command !== "validate" || !path) {
    process.stderr.write("usage: verify_stagehand_driver.mjs self-test | validate <evidence.json>\n");
    process.exit(2);
  }
  const evidence = JSON.parse(await readFile(path, "utf8"));
  const errors = validateEvidence(evidence);
  process.stdout.write(`${JSON.stringify({ ok: errors.length === 0, errors }, null, 2)}\n`);
  process.exit(errors.length === 0 ? 0 : 2);
}

export { validateEvidence };
await main();
