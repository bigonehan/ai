#!/usr/bin/env python3
"""Codex Stop/SubagentStop hook: a turn may end only when Table26 $H allows it.

Agent (subagent spawned with task_name `t26_<RawID>`): may stop when its row is
검증 (submitted with `grist.py task-submit`) or 완료. Other subagents are not gated.
Main session: may stop when every row named by the nearest AGENTS.md
`- Table26 task Raw ID:` line is 대기 or 완료, or when it names none.

After MAX_BLOCKS consecutive blocks for one thread with no $H change the stop is allowed, recorded
in $D and announced with nf so the loop cannot burn tokens forever. A Grist
failure never locks the session; it is reported through systemMessage.

Main-session stops also get the access-jev J2 judgment of last_assistant_message
(skipped when jev is missing or failing). With JEV_MODE=enforce a question that
asks the user for something other than the four allowed actions is blocked even
in 대기/완료, and a stop that presents a failure as the outcome gets a sharper reason.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

GRIST = Path.home() / ".codex/skills/access-grist/scripts/grist.py"
STATE_DIR = Path.home() / ".codex/tmp/completion_gate"
MAIN_ALLOWED = ("완료", "대기")
AGENT_ALLOWED = ("검증", "완료")
AGENT_TASK = re.compile(r"(?:^|/)t26_(\d+)$")
MAX_BLOCKS = 3
CMD = "python3 ~/.codex/skills/access-grist/scripts/grist.py"
JEV = Path.home() / ".codex/skills/access-jev/scripts/jev.py"
FAILURE_REASON = ("마지막 메시지가 실패를 결론으로 종료하고 있어요. CDP·runner·Stagehand 실패와 runtime-unverified는 종료 사유가 아니에요. "
                  "최초 실패 경계를 `$I`에 쓰고 원인 수정 → 재빌드·배포 → 재연결 → 같은 흐름 재실행 → consumer readback을 계속하세요. ")
QUESTION_REASON = ("사용자에게 넘길 수 있는 동작은 로그인·인증, 결제·비용, 비가역 외부 전송, 브라우저 밖 OS 앱 조작뿐이에요. "
                   "그 밖의 확인·클릭·재로드·테스트는 직접 실행하고 결과로 보고하세요.")


def grist(*args: str) -> tuple[int, str, str]:
    proc = subprocess.run(["python3", str(GRIST), *args], capture_output=True, text=True, timeout=40)
    return proc.returncode, proc.stdout, proc.stderr


def emit(output: dict) -> None:
    print(json.dumps(output, ensure_ascii=False))
    sys.exit(0)


def next_number(memo: str | None) -> int:
    match = re.match(r"\s*(\d+)\.", memo or "")
    return int(match.group(1)) + 1 if match else 1


def thread_meta(event: dict) -> dict:
    """session_meta of this thread's rollout; a subagent's carries `agent_path` (/root/<task_name>)."""
    path = event.get("agent_transcript_path") or event.get("transcript_path")
    try:
        with open(path, encoding="utf-8") as rollout:
            return json.loads(rollout.readline()).get("payload", {})
    except (TypeError, OSError, ValueError):
        return {}


def jev_stop(message: str) -> dict:
    """J2 judgment; {} when jev is missing, failing, in shadow mode or the message is empty."""
    if not message.strip() or not JEV.is_file():
        return {}
    with tempfile.NamedTemporaryFile("w", suffix=".txt", encoding="utf-8") as handle:
        handle.write(message)
        handle.flush()
        try:
            proc = subprocess.run(["python3", str(JEV), "stop", "--message-file", handle.name],
                                  capture_output=True, text=True, timeout=20)
        except (OSError, subprocess.TimeoutExpired):
            return {}
    if proc.returncode != 0:
        return {}
    result = json.loads(proc.stdout)
    return result if result.get("mode") == "enforce" else {}


def judged(result: dict, name: str, verdict: str) -> bool:
    return any(item["name"] == name and item["verdict"] == verdict for item in result.get("items", []))


def agent_reason(row: dict, project_dir: str | None) -> str:
    return (
        f"Table26 #{row['id']} $H={row['H']}: 담당 작업이 아직 제출되지 않았어요. $D 최신 항목: {row.get('D') or '-'}. "
        "완료 조건을 확인하는 단위 테스트를 만들고 통과시킨 뒤 "
        f"`{CMD} task-submit {row['id']} --project {project_dir or '<프로젝트>'} --test \"<테스트 명령>\"`로 제출하세요. "
        "실패하면 `$H=교정`, `$I`·`$J`를 기록하고 원인을 고치세요. 사용자에게 질문하지 말고 막힌 경계는 결과 텍스트로 메인에 보고하세요."
    )


def main_reason(rows: list[dict], project_dir: str) -> str:
    stages = ", ".join(f"#{r['id']}={r['H']}" for r in rows)
    pending = [r for r in rows if r["H"] not in MAIN_ALLOWED]
    by_stage = {r["H"] for r in pending}
    if by_stage & {"초안"}:
        step = "`초안` 행을 spawn_agent(task_name `t26_<RawID>`)로 하나씩 할당하세요."
    elif by_stage & {"작업", "교정", "진단"}:
        step = "에이전트 결과를 기다리세요(wait). 끝난 에이전트의 행이 `검증`이 아니면 같은 행으로 다음 교정을 할당하세요."
    else:
        step = (f"모든 행이 `검증`이에요. 전체 흐름을 테스트하고 당일 `.ai/changes`에 `## 흐름 판정`(`- [x] 항목` + `근거:`)을 쓴 뒤 "
                f"`{CMD} flow-wait --project {project_dir} --waiting \"<남은 사용자 테스트>\"`를 실행하세요.")
    return (f"종료 차단: Table26 {stages} (작업 폴더 {project_dir}). {step} "
            "사용자 입력(로그인·결제·비가역 외부 전송·OS 앱 조작)만 남았으면 `$H=대기`와 `$D` 사유를 기록한 뒤 질문하세요.")


def main() -> None:
    event = json.load(sys.stdin)
    meta = thread_meta(event)
    agent_path = meta.get("agent_path") or ""
    thread = event.get("agent_id") or meta.get("id") or event.get("session_id") or "unknown"
    state_file = STATE_DIR / f"{re.sub(r'[^A-Za-z0-9_-]', '_', thread)}.json"

    if agent_path:
        task = AGENT_TASK.search(agent_path)
        if not task:
            emit({})  # helper subagent without a Table26 row
        code, out, err = grist("gate", "--cwd", event.get("cwd") or ".", "--row", task.group(1))
        allowed = AGENT_ALLOWED
    else:
        code, out, err = grist("gate", "--cwd", event.get("cwd") or ".")
        allowed = MAIN_ALLOWED
    if code != 0:
        emit({"systemMessage": f"completion_gate: grist.py gate 실패(exit {code}) — {err.strip()[:300]}"})
    status = json.loads(out)
    rows = status["rows"]
    pending = [r for r in rows if r["H"] not in allowed]
    judgment = jev_stop(event.get("last_assistant_message") or "") if rows and not agent_path else {}
    asks_user = judgment.get("category") == "사용자 질문" and judged(judgment, "category", "pass")
    disallowed_question = asks_user and judged(judgment, "allowed_action", "fail")
    if not pending and not disallowed_question:
        state_file.unlink(missing_ok=True)
        emit({})

    # Count only blocks without progress: any $H change since the last block restarts the count.
    stages = {str(r["id"]): r["H"] for r in rows}
    state = json.loads(state_file.read_text()) if state_file.exists() else {}
    blocks = state.get("blocks", 0) if state.get("stages") == stages else 0
    if blocks >= MAX_BLOCKS:
        state_file.unlink(missing_ok=True)
        for row in pending or rows:
            memo = f"{next_number(row.get('D'))}. 종료 게이트 {MAX_BLOCKS}회 차단 후 종료($H={row['H']})"
            grist("task-update", str(row["id"]), "--fields", json.dumps({"D": memo}, ensure_ascii=False))
        summary = ", ".join(f"#{r['id']} $H={r['H']}" for r in pending or rows)
        # nf takes ~30s; detach it and pass the text as an argument so fish does not expand `$H`.
        subprocess.Popen(["fish", "-c", "nf -m $argv[1]", f"Codex 종료 게이트 {MAX_BLOCKS}회 초과: Table26 {summary}"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        emit({"systemMessage": f"completion_gate: Table26 {summary} 상태로 {MAX_BLOCKS}회 차단 뒤 종료를 허용했어요."})

    STATE_DIR.mkdir(parents=True, exist_ok=True)
    state_file.write_text(json.dumps({"blocks": blocks + 1, "stages": stages}, ensure_ascii=False))
    if agent_path:
        reason = agent_reason(pending[0], status.get("project_dir"))
    elif not pending:
        reason = "종료 차단: 허용되지 않은 사용자 요청이에요. " + QUESTION_REASON
    else:
        failure = judgment.get("category") == "실패를 결론으로 종료" and judged(judgment, "category", "pass")
        reason = (FAILURE_REASON if failure else "") + main_reason(rows, status["project_dir"])
        if disallowed_question:
            reason += " " + QUESTION_REASON
    emit({"decision": "block", "reason": f"[{blocks + 1}/{MAX_BLOCKS}] {reason}"})


if __name__ == "__main__":
    main()
