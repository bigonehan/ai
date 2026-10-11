#!/usr/bin/env node
import assert from 'node:assert/strict';
import { mkdirSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const helper = path.join(here, 'run_cdp_preflight.mjs');
const testTmpRoot = process.env.RUN_CDP_TEST_TMP_ROOT;
if (!testTmpRoot || !path.isAbsolute(testTmpRoot) || !path.normalize(testTmpRoot).endsWith(`${path.sep}test${path.sep}tmp`)) {
  throw new Error('RUN_CDP_TEST_TMP_ROOT must be an absolute active-project test/tmp directory');
}
const root = path.join(testTmpRoot, `run-cdp-policy-${process.pid}`);
const tmp = root;
const receiptPath = path.join(tmp, 'receipt.json');
const statePath = path.join(tmp, 'state.json');
mkdirSync(tmp, { recursive: true });

const baseReceipt = {
  operation_id: 'op-safe', decision: 'launch-allowed', origins: ['https://example.test'],
  artifact_digests: ['sha256:test'], start_action: 'inspect', authoritative_consumer: 'consumer',
  auth_state: 'ready', effect_class: 'read-only', action_ledger_status: 'permitted',
  endpoint_owner: 'owner-safe', endpoint_pid: '4242', attempt: 1, elapsed_seconds: 0, first_failure_boundary: 'none',
};
const baseState = { operations: { 'op-safe': { endpoint_owner: 'owner-safe', endpoint_pid: '4242', launch_count: 0, launch_reserved: false, retry_used: false } } };
function write(receipt = baseReceipt, state = baseState) {
  writeFileSync(receiptPath, JSON.stringify(receipt));
  writeFileSync(statePath, JSON.stringify(state));
}
function run(...args) { return spawnSync(process.execPath, [helper, '--active-project-test-tmp', testTmpRoot, '--preflight-receipt', receiptPath, '--state-file', statePath, ...args], { encoding: 'utf8' }); }
function allowed(result, text) { assert.equal(result.status, 0, `${text}: ${result.stderr}`); }
function denied(result, text) { assert.notEqual(result.status, 0, `${text}: unexpectedly accepted`); }

try {
  write();
  allowed(run(), 'first safe launch');
  assert.match(run().stdout, /"action":"launch"/);
  for (const field of Object.keys(baseReceipt)) {
    const missing = { ...baseReceipt };
    delete missing[field];
    write(missing);
    denied(run(), `missing required receipt field ${field}`);
  }
  write({ ...baseReceipt, auth_state: 'required' }); denied(run(), 'authentication boundary');
  write({ ...baseReceipt, effect_class: 'cost' }); denied(run(), 'cost boundary');
  write({ ...baseReceipt, action_ledger_status: 'blocked' }); denied(run(), 'invalid action ledger');
  write({ ...baseReceipt, decision: 'handoff-required' }); denied(run(), 'handoff boundary');
  write({ ...baseReceipt, elapsed_seconds: 301 }); denied(run(), 'budget boundary');
  const withoutPid = { ...baseReceipt }; delete withoutPid.endpoint_pid;
  write(withoutPid); denied(run(), 'endpoint PID is required');
  write({ ...baseReceipt, endpoint_pid: 'not-a-pid' }); denied(run(), 'receipt endpoint PID format');
  write(baseReceipt, { operations: { 'op-safe': { ...baseState.operations['op-safe'], endpoint_pid: '4243' } } }); denied(run(), 'state and receipt PID mismatch');
  write({ ...baseReceipt, attempt: 2, fresh_boundary_evidence: 'endpoint reconnect evidence' }, { operations: { 'op-safe': { ...baseState.operations['op-safe'], launch_count: 1, launch_reserved: true } } });
  const retry = run(); allowed(retry, 'retry is attach-only'); assert.match(retry.stdout, /"action":"retry-attach-only"/);
  allowed(run('--record-retry'), 'record attach-only retry');
  assert.equal(JSON.parse(readFileSync(statePath, 'utf8')).operations['op-safe'].retry_used, true);
  write({ ...baseReceipt, attempt: 2 }, { operations: { 'op-safe': { ...baseState.operations['op-safe'], launch_count: 1, launch_reserved: true } } }); denied(run(), 'retry without new evidence');
  write({ ...baseReceipt, stale_artifact_evidence: 'bundle digest changed' }); allowed(run('--replacement', '--current-endpoint-pid', '4242'), 'owned stale replacement');
  write({ ...baseReceipt, stale_artifact_evidence: 'bundle digest changed' }); denied(run('--replacement', '--current-endpoint-pid', '4243'), 'replacement listener PID mismatch');
  write({ ...baseReceipt, stale_artifact_evidence: 'bundle digest changed' }, { operations: { 'op-safe': { ...baseState.operations['op-safe'], endpoint_owner: 'other-owner' } } }); denied(run('--replacement', '--current-endpoint-pid', '4242'), 'replacement owner mismatch');
  write({ ...baseReceipt, stale_artifact_evidence: 'bundle digest changed' }); denied(run('--replacement', '--expected-operation-id', 'different-operation'), 'replacement operation mismatch');
  write(); allowed(run('--reserve-launch'), 'reserve permitted launch immediately before spawn');
  let reserved = JSON.parse(readFileSync(statePath, 'utf8')).operations['op-safe'];
  assert.equal(reserved.launch_count, 1); assert.equal(reserved.launch_reserved, true);
  denied(run(), 'second launch is denied after reservation even if Chrome failed before ready');
  denied(run('--reserve-launch'), 'second launch reservation');
  const outsideRoot = path.join(path.dirname(testTmpRoot), `${path.basename(testTmpRoot)}-neighbor`, 'test', 'tmp');
  const outsideReceipt = path.join(outsideRoot, 'receipt.json');
  const outsideState = path.join(outsideRoot, 'state.json');
  mkdirSync(outsideRoot, { recursive: true });
  writeFileSync(outsideReceipt, JSON.stringify(baseReceipt));
  writeFileSync(outsideState, JSON.stringify(baseState));
  const outside = spawnSync(process.execPath, [helper, '--active-project-test-tmp', testTmpRoot, '--preflight-receipt', outsideReceipt, '--state-file', outsideState], { encoding: 'utf8' });
  denied(outside, 'test/tmp-looking path outside supplied active project root');
  rmSync(path.join(path.dirname(testTmpRoot), `${path.basename(testTmpRoot)}-neighbor`), { recursive: true, force: true });
  console.log('run_cdp_preflight.test: pass (validation paths only; Chrome was not started)');
} finally { rmSync(root, { recursive: true, force: true }); }
