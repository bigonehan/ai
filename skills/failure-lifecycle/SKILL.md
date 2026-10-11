---
name: failure-lifecycle
description: Route a reported product error, regression, or failed feature into the work-project failure loop.
---

# Failure Lifecycle

Use this skill when a user reports an error, regression, or feature that did not work. Do not use it for a purely explanatory question.

This skill has no separate procedure. Read `/home/tree/ai/skills/work-project/SKILL.md` and follow its **Bounded implementation**, **Verification order**, and **Failure loop** sections. In order:

1. Read the nearest project `AGENTS.md`, project `Input.md`, and the Table26 task. Set `$H=진단`.
2. Dispatch one bounded implementation agent that reproduces the issue, repairs the first failing owner boundary, and runs focused, production-boundary, and build checks in the same run.
3. The main session compares the diff and test output with the `Input.md` requirement. If the requirement is unresolved, dispatch exactly one next bounded repair. Otherwise set `$C=완료`.
4. After all items are complete, run one project-level runtime whole flow through the authoritative consumer.

Do not call a separate review agent unless `work-project` requires one.
