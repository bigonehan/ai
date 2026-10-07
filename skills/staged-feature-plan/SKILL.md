---
name: staged-feature-plan
description: Convert an approved feature, fix, build, or configuration plan into dependency-ordered implementation stages with explicit user-intervention gates and evidence before advancing. Use in Plan mode and when executing an approved staged plan.
---

# Staged Feature Plan

Keep implementation and verification coupled by stage. Do not change product code for a later stage until the current stage has met its completion contract.

## Daily plan file

When an approved plan begins execution, make the first project file mutation the daily plan at `.ai/plan/YYMMDD.요약.md`:

- Use the current Asia/Seoul date and a Korean summary of 1–9 characters.
- Use one plan file per project per day. Add separate `# 요청명` sections for different requests on the same day.
- For a follow-up to the same request, update that request section and append its change history instead of creating another daily file.
- Start from [assets/stage-plan-template.md](assets/stage-plan-template.md). Remove unused guidance text rather than copying placeholders into a live plan.

## Stage contract

Split the work into the smallest dependency-ordered goals that independently deliver and verify a meaningful boundary. Every stage records:

- unchecked or checked status and one of `pending`, `in-progress`, `complete`, or `blocked`;
- user intervention `[n]` or `[y]`;
- goal, allowed scope, forbidden adjacent scope, implementation contract, test contract, completion conditions, and completion evidence;
- the authoritative consumer that determines completion.

Use `[n]` only when the agent can exercise the production boundary and read the authoritative consumer itself. Use `[y]` when a user-controlled runtime action is necessary. Before asking for that action, prepare correlated logs and the baseline state, follow the applicable input-notification and user-action gates, and include the exact action and evidence to read afterward.

## End-to-end feature flow gate

For every user-visible feature or fix, bind the plan to one or more complete vertical flows before product implementation. A flow starts at the exact user action and ends only after the authoritative consumer and visible result have been read.

Each affected stage must record:

- the rendered control or production entry action;
- required pre-state and lifecycle state, including missing, stale, loading, reconnected, or partially initialized context when applicable;
- the ordered production path through handler, validation, state/context acquisition, transforms, transports, external consumers, persistence, and UI reconciliation;
- the success feedback visible at the initiating surface;
- the failure feedback visible at that same surface, including the first failed boundary and retry safety;
- adjacent entry paths and effects that must remain unchanged.

Do not treat helper tests, API tests, build success, or separately verified downstream functions as evidence that the initiating control works. The stage test contract must exercise the production handler from the user action through every required downstream consumer and verify the final UI or persisted state. Add focused helper tests only as lower-level evidence.

Before implementation, inspect whether overlays, dialogs, disabled states, loading states, error boundaries, or navigation can hide feedback or detach the handler from its consumer. A status message outside the active surface does not satisfy visible failure feedback when that surface covers or replaces it.

If the complete vertical flow cannot be exercised by the agent, mark the stage `[y]` before implementation and prepare correlated logging for every boundary. Do not downgrade the stage to helper-level completion.

## External and multi-source provenance gate

When a stage relays, joins, compares, selects, or matches data from an external API, bridge, host application, table, file system, or another runtime, its implementation contract and test contract must define diagnostics before product code changes. This applies even when every upstream request succeeds and the visible failure occurs only during a later comparison or selection.

Record, under one stable operation ID and a distinct boundary or refresh ID:

- every source system, owning boundary, operation, and source scope such as table, folder, collection, or endpoint;
- the observed raw value type and safe count, length, stable identifier, or SHA-256 needed to distinguish inputs without storing private raw values;
- source-set counts and fingerprints at the exact values consumed by the comparison, not only HTTP status or an earlier helper result;
- candidate counts, the contract-authorized comparison or selection criteria, and each `matched`, `unmatched`, `collision`, `ignored`, or invalid-type outcome;
- the first failing owner/stage and the authoritative consumer readback, including an explicit zero result where zero is the complaint.

Forbid credentials, authorization data, raw payloads, private file content, full personal paths, and private display values in diagnostics. Prefer typed metadata, counts, identifiers, lengths, and digests. If a digest could expose a small predictable secret domain, use a keyed or runtime-local correlation identifier instead.

The test contract must prove that diagnostics are disabled when developer mode is off, that all source and decision records correlate when it is on, and that the same visible result caused by different source or decision failures identifies different owners. Add a regression mutant that removes or mislabels one required source or decision boundary. A plan cannot mark the stage complete when logs show only transport success, a generic message, or aggregate input counts that cannot explain the consumer decision.

## Execution

1. Mark only the current stage `in-progress`.
2. Implement only its allowed scope.
3. Run its unit, boundary, build-artifact, and runtime checks required by the test contract.
4. Mark it `complete` only when the checkbox, state, completion conditions, and evidence agree. Otherwise keep it incomplete or mark it blocked with the precise boundary.
5. Begin the next stage only after the prior stage is complete. Independent `[n]` checks may run together only when neither changes later-stage product code.

If an `[n]` stage discovers a user-only runtime boundary, change the stage to `[y]`; do not skip or replace that verification. If the approved plan or requirement changes, append a dated change record and update affected later stages before further product mutation.

## Validation without a custom script

This Skill intentionally has no project-specific validator. Review the live daily plan directly and reject it when any of these conditions holds:

- filename date or 1–9 character summary is invalid;
- more than one daily plan file was created for the same project and date;
- a stage lacks `[y/n]`, allowed scope, completion conditions, test contract, authoritative consumer, or evidence;
- a user-visible stage omits its initiating control, pre-state/context variants, ordered production path, success feedback, failure feedback, or adjacent-flow preservation;
- an external or multi-source stage omits its source scopes, safe consumed-value fingerprints, candidate/decision outcomes, first-failure owner, developer-mode boundary, or authoritative consumer readback;
- a stage claims the initiating control works using only helper, API, mock, build, or separately tested consumer evidence;
- a later product stage started while an earlier dependency is not complete;
- a completed checkbox conflicts with the written state or evidence;
- a `[y]` request was made before logging, baseline, and applicable action gate were ready.

Use existing repository validators and Test Manager tools when their own rules apply; do not create a Python validator merely to validate this document format.
