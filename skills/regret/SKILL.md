---
name: regret
description: "When the assistant used the token '잘못', append a regret record and improvement action to references/report.md and report execution."
---

# Regret Skill

## Trigger
- Use this skill whenever the assistant output includes the token `잘못`.

## Steps
1. Open `/home/tree/ai/skills/regret/references/report.md`.
2. Append one bullet under `# 잘못한점` describing what went wrong.
3. Append one bullet under `# 개선할점` with one concrete preventive action.
4. Keep entries concise and timestamped (`YYYY-MM-DD HH:MM KST`).
5. Save the file and mention that regret logging was executed.

## Output Rule
- Do not skip logging when the trigger token appears.
- Write exactly one new item to each section per trigger occurrence.
