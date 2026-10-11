#!/usr/bin/env node
/**
 * Side-effect-free policy boundary for run_cdp.  It deliberately does not
 * probe CDP, start a browser, or kill a process.  The only mutating mode is
 * --reserve-launch, called by run_cdp immediately before Chrome is spawned.
 * Initial state schema:
 * {"operations":{"<operation_id>":{"endpoint_owner":"<owner>","endpoint_pid":"<pid>","launch_count":0,"launch_reserved":false,"retry_used":false}}}
 */
import fs from 'node:fs';
import path from 'node:path';

const REQUIRED = [
  'operation_id', 'decision', 'origins', 'artifact_digests', 'start_action',
  'authoritative_consumer', 'auth_state', 'effect_class',
  'action_ledger_status', 'endpoint_owner', 'attempt', 'elapsed_seconds',
  'first_failure_boundary', 'endpoint_pid', 'browser_visibility',
];
const SAFE_EFFECTS = new Set(['read-only', 'reversible']);
const PERMITTED_LEDGER = new Set(['permitted', 'reserved']);
const ALLOWED_DECISIONS = new Set(['launch-allowed', 'attach-only']);

function fail(message) {
  process.stderr.write(`run_cdp preflight: ${message}\n`);
  process.exit(1);
}
function valueAfter(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}
function requiredActiveProjectTestTmpRoot(root) {
  if (!root || !path.isAbsolute(root)) fail('--active-project-test-tmp must be an absolute path');
  const normal = path.normalize(root);
  if (!normal.endsWith(`${path.sep}test${path.sep}tmp`)) fail('--active-project-test-tmp must name the active project test/tmp directory');
  return normal;
}
function requiredAbsoluteTestTmp(file, flag, activeProjectTestTmpRoot) {
  if (!file || !path.isAbsolute(file) || path.extname(file) !== '.json') fail(`${flag} must be an absolute .json path`);
  const normal = path.normalize(file);
  if (normal === activeProjectTestTmpRoot || !normal.startsWith(`${activeProjectTestTmpRoot}${path.sep}`)) fail(`${flag} must be inside the supplied active project's test/tmp directory`);
  return normal;
}
function readJson(file, label) {
  try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch (error) { fail(`cannot read ${label}: ${error.message}`); }
}
function nonEmptyString(value) { return typeof value === 'string' && value.trim().length > 0; }
function endpointPidString(value, label) {
  if (!nonEmptyString(value) || !/^[1-9][0-9]*$/.test(value)) fail(`${label} must be a positive-integer string PID`);
  return value;
}
function validateReceipt(receipt) {
  if (!receipt || typeof receipt !== 'object' || Array.isArray(receipt)) fail('receipt must be a JSON object');
  for (const field of REQUIRED) {
    const value = receipt[field];
    if (field === 'origins' || field === 'artifact_digests') {
      if (!Array.isArray(value) || value.length === 0 || value.some((item) => !nonEmptyString(item))) fail(`receipt.${field} must be a non-empty string array`);
    } else if (field === 'endpoint_pid') {
      endpointPidString(value, `receipt.${field}`);
    } else if (field === 'attempt' || field === 'elapsed_seconds') {
      if (!Number.isFinite(value)) fail(`receipt.${field} must be a finite number`);
    } else if (!nonEmptyString(value)) fail(`receipt.${field} must be a non-empty string`);
  }
  if (!ALLOWED_DECISIONS.has(receipt.decision)) fail(`decision ${JSON.stringify(receipt.decision)} is not launchable or attachable`);
  if (receipt.auth_state !== 'ready') fail('auth_state must be ready');
  if (!SAFE_EFFECTS.has(receipt.effect_class)) fail('effect_class must be read-only or reversible');
  if (!PERMITTED_LEDGER.has(receipt.action_ledger_status)) fail('action_ledger_status must be permitted or reserved');
  if (!Number.isInteger(receipt.attempt) || receipt.attempt < 1 || receipt.attempt > 2) fail('attempt must be 1 or 2');
  if (receipt.elapsed_seconds < 0 || receipt.elapsed_seconds > 300) fail('elapsed_seconds exceeds the 300 second browser-work budget');
  if (!new Set(['visible', 'headless-only']).has(receipt.browser_visibility)) fail('browser_visibility must be visible or headless-only');
  if (receipt.browser_visibility === 'visible' && receipt.headless_requested === true) fail('visible preflight cannot request headless');
}
function operationState(state, receipt) {
  if (!state || typeof state !== 'object' || Array.isArray(state) || !state.operations || typeof state.operations !== 'object') fail('state.operations must be an object keyed by operation_id');
  const operation = state.operations[receipt.operation_id];
  if (!operation || typeof operation !== 'object' || Array.isArray(operation)) fail(`state has no operation record for ${receipt.operation_id}`);
  if (operation.endpoint_owner !== receipt.endpoint_owner) fail('state endpoint_owner does not match receipt endpoint_owner');
  if (endpointPidString(operation.endpoint_pid, 'state endpoint_pid') !== endpointPidString(receipt.endpoint_pid, 'receipt.endpoint_pid')) fail('state endpoint_pid does not match receipt endpoint_pid');
  if (!Number.isInteger(operation.launch_count) || operation.launch_count < 0) fail('state launch_count must be a non-negative integer');
  if (typeof operation.launch_reserved !== 'boolean') fail('state launch_reserved must be boolean');
  if (typeof operation.retry_used !== 'boolean') fail('state retry_used must be boolean');
  return operation;
}
function classify(receipt, state, replacement, currentEndpointPid) {
  const operation = operationState(state, receipt);
  const fresh = nonEmptyString(receipt.fresh_boundary_evidence);
  if (replacement) {
    if (!nonEmptyString(receipt.stale_artifact_evidence)) fail('replacement requires stale_artifact_evidence');
    if (receipt.attempt !== 1 || operation.launch_count !== 0 || operation.launch_reserved || operation.retry_used) fail('replacement is only allowed before this operation has launched, reserved, or retried');
    if (receipt.decision !== 'launch-allowed') fail('replacement requires decision=launch-allowed');
    if (currentEndpointPid !== undefined && currentEndpointPid !== receipt.endpoint_pid) fail('replacement current shared-listener PID does not exactly match receipt/state endpoint_pid');
    return 'launch';
  }
  if (receipt.attempt === 2) {
    if (!fresh) fail('retry requires non-empty fresh_boundary_evidence');
    if (operation.launch_count !== 1 || !operation.launch_reserved || operation.retry_used) fail('retry requires exactly one reserved prior launch and retry_used=false');
    return 'retry-attach-only';
  }
  if (receipt.decision === 'attach-only') return 'attach-only';
  if (operation.launch_count !== 0 || operation.launch_reserved || operation.retry_used) fail('a new browser launch is limited to the first unreserved launch of an operation');
  return 'launch';
}

const activeProjectTestTmpRoot = requiredActiveProjectTestTmpRoot(valueAfter('--active-project-test-tmp'));
const receiptFile = requiredAbsoluteTestTmp(valueAfter('--preflight-receipt'), '--preflight-receipt', activeProjectTestTmpRoot);
const stateFile = requiredAbsoluteTestTmp(valueAfter('--state-file'), '--state-file', activeProjectTestTmpRoot);
const expectedOperationId = valueAfter('--expected-operation-id');
const reserveLaunch = process.argv.includes('--reserve-launch');
const recordRetry = process.argv.includes('--record-retry');
const replacement = process.argv.includes('--replacement');
const currentEndpointPid = valueAfter('--current-endpoint-pid');
if (currentEndpointPid !== undefined && !replacement) fail('--current-endpoint-pid requires --replacement');
if (currentEndpointPid !== undefined) endpointPidString(currentEndpointPid, '--current-endpoint-pid');
const receipt = readJson(receiptFile, 'preflight receipt');
const state = readJson(stateFile, 'state file');
validateReceipt(receipt);
if (expectedOperationId !== undefined && receipt.operation_id !== expectedOperationId) fail('receipt operation_id does not match --operation-id');
const action = classify(receipt, state, replacement, currentEndpointPid);
if (reserveLaunch) {
  if (action !== 'launch') fail('--reserve-launch is only valid for a first permitted launch');
  state.operations[receipt.operation_id].launch_count = 1;
  state.operations[receipt.operation_id].launch_reserved = true;
  state.operations[receipt.operation_id].retry_used = false;
  fs.writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`, 'utf8');
}
if (recordRetry) {
  if (action !== 'retry-attach-only') fail('--record-retry is only valid for an attach-only retry');
  state.operations[receipt.operation_id].retry_used = true;
  fs.writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`, 'utf8');
}
process.stdout.write(`${JSON.stringify({ operation_id: receipt.operation_id, action, decision: receipt.decision })}\n`);
