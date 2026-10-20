mod json;

use json::{obj_set, s, Value};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const MAIN_ALLOWED: &[&str] = &["완료", "확인"];
const AGENT_ALLOWED: &[&str] = &["검증", "완료"];
const MAX_BLOCKS: i64 = 3;
const J2_MAX_BLOCKS: i64 = 2;
const CMD: &str = "~/ai/scripts/bin/grist";

const FAILURE_REASON: &str = "마지막 메시지가 실패를 결론으로 종료하고 있어요. CDP·runner·Stagehand 실패와 runtime-unverified는 종료 사유가 아니에요. \
최초 실패 경계를 `$I`에 쓰고 원인 수정 → 재빌드·배포 → 재연결 → 같은 흐름 재실행 → consumer readback을 계속하세요. ";
const QUESTION_REASON: &str = "사용자에게 넘길 수 있는 동작은 로그인·인증, 결제·비용, 비가역 외부 전송, 브라우저 밖 OS 앱 조작뿐이고, \
물을 수 있는 결정은 경로·이유를 밝힌 `.py` 동의와, 수정 전 결과가 갈리는 구체적 해석 선택뿐이에요. \
그 밖의 확인·클릭·재로드·테스트는 직접 실행하고 결과로 보고하세요.";
const PROGRESS_REASON: &str = "마지막 메시지가 진행 보고로 끝나고 있어요. `확인`은 사용자 확인을 기다리는 상태일 뿐 작업 중단 사유가 아니에요. \
새 사용자 메시지나 에이전트 대기 중단 결과도 중지 지시가 아니에요. Table26과 남은 실행 단계를 다시 읽고 계속하세요. ";

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn grist_bin() -> PathBuf {
    std::env::var_os("GRIST_BIN")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("ai/scripts/bin/grist"))
}

fn jev_bin() -> PathBuf {
    std::env::var_os("JEV_BIN")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("ai/scripts/bin/jev"))
}

fn state_dir() -> PathBuf {
    std::env::var_os("COMPLETION_GATE_STATE_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".codex/tmp/completion_gate"))
}

fn grist(args: &[&str]) -> (i32, String, String) {
    match Command::new(grist_bin()).args(args).stdin(Stdio::null()).output() {
        Ok(out) => (
            out.status.code().unwrap_or(1),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ),
        Err(e) => (1, String::new(), e.to_string()),
    }
}

fn emit(mut output: Vec<(String, Value)>, sync_note: &str) -> ! {
    if !sync_note.is_empty() {
        let combined = match output.iter().find(|(k, _)| k == "systemMessage").and_then(|(_, v)| v.as_str()) {
            Some(existing) if !existing.is_empty() => format!("{existing} {sync_note}"),
            _ => sync_note.to_string(),
        };
        obj_set(&mut output, "systemMessage", s(combined));
    }
    println!("{}", json::dumps(&Value::Obj(output)));
    std::process::exit(0);
}

fn next_number(memo: Option<&str>) -> i64 {
    let trimmed = memo.unwrap_or("").trim_start();
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && trimmed[digits.len()..].starts_with('.') {
        digits.parse::<i64>().map(|n| n + 1).unwrap_or(1)
    } else {
        1
    }
}

fn thread_meta(event: &Value) -> Value {
    let path = event
        .get("agent_transcript_path")
        .filter(|v| v.truthy())
        .or_else(|| event.get("transcript_path").filter(|v| v.truthy()))
        .and_then(Value::as_str);
    let Some(path) = path else {
        return Value::Obj(vec![]);
    };
    let Ok(file) = std::fs::File::open(path) else {
        return Value::Obj(vec![]);
    };
    let mut first_line = String::new();
    if std::io::BufRead::read_line(&mut std::io::BufReader::new(file), &mut first_line)
        .ok()
        .filter(|&n| n > 0)
        .is_none()
    {
        return Value::Obj(vec![]);
    }
    let Ok(parsed) = json::parse(first_line.trim()) else {
        return Value::Obj(vec![]);
    };
    parsed.get("payload").cloned().unwrap_or(Value::Obj(vec![]))
}

fn extract_agent_task(agent_path: &str) -> Option<&str> {
    let tail = agent_path.rsplit('/').next().unwrap_or(agent_path);
    let digits = tail.strip_prefix("t26_")?;
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        Some(digits)
    } else {
        None
    }
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

fn jev_stop(message: &str) -> Value {
    let j_bin = jev_bin();
    if message.trim().is_empty() || !j_bin.is_file() {
        return Value::Obj(vec![]);
    }
    let tmp_path = std::env::temp_dir().join(format!(
        "completion-gate-jev-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    if std::fs::write(&tmp_path, message).is_err() {
        return Value::Obj(vec![]);
    }
    let out = Command::new(&j_bin)
        .args(["stop", "--message-file", &tmp_path.to_string_lossy()])
        .stdin(Stdio::null())
        .output();
    let _ = std::fs::remove_file(&tmp_path);
    let Ok(out) = out else {
        return Value::Obj(vec![]);
    };
    if !out.status.success() {
        return Value::Obj(vec![]);
    }
    json::parse(&String::from_utf8_lossy(&out.stdout)).unwrap_or(Value::Obj(vec![]))
}

fn judged(result: &Value, name: &str, verdicts: &[&str]) -> bool {
    let Some(Value::Arr(items)) = result.get("items") else {
        return false;
    };
    items.iter().any(|item| {
        item.get("name").and_then(Value::as_str) == Some(name)
            && item
                .get("verdict")
                .and_then(Value::as_str)
                .map(|v| verdicts.contains(&v))
                .unwrap_or(false)
    })
}

fn early_stop(result: &Value) -> &'static str {
    let Some(category) = result.get("category").and_then(Value::as_str) else {
        return "";
    };
    if !judged(result, "category", &["pass", "uncertain"]) {
        return "";
    }
    match category {
        "진행 보고" => PROGRESS_REASON,
        "실패를 결론으로 종료" => FAILURE_REASON,
        _ => "",
    }
}

fn agent_reason(row: &Value, project_dir: Option<&str>) -> String {
    let id = row.get("id").map(Value::py_str).unwrap_or_default();
    let h = row.get("H").map(Value::py_str).unwrap_or_default();
    let d = row
        .get("D")
        .filter(|v| v.truthy())
        .map(Value::py_str)
        .unwrap_or_else(|| "-".to_string());
    let proj = project_dir.filter(|s| !s.is_empty()).unwrap_or("<프로젝트>");
    format!(
        "Table26 #{id} $H={h}: 담당 작업이 아직 제출되지 않았어요. $D 최신 항목: {d}. \
### 목표를 확인하는 단위 테스트를 만들고 통과시킨 뒤 \
`{CMD} task-submit {id} --project {proj}`로 제출하세요. \
실패하면 `$H=진단`, `$I`·`$J`를 기록하고 원인을 고치세요. 사용자에게 질문하지 말고 막힌 경계는 결과 텍스트로 메인에 보고하세요."
    )
}

fn init_reason(project_dir: &str) -> String {
    format!(
        "종료 차단: {project_dir}는 `AGENTS.md`에 `# 상태`(`### grist` `ID :`) 헤더가 없는 미초기화 프로젝트인데 변경 파일이 있어요. \
`initialize-project` Skill로 Table7 행·Obsidian 문서·`# 상태` 헤더를 만들고, 현재 요청을 `request-record`·`task-add`·`task-goal`로 기록한 뒤 \
`work-project` 흐름을 이어가세요."
    )
}

fn main_reason(rows: &[Value], pending: &[Value], project_dir: &str) -> String {
    let stages = rows
        .iter()
        .map(|r| {
            format!(
                "#{}={}",
                r.get("id").map(Value::py_str).unwrap_or_default(),
                r.get("H").map(Value::py_str).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let has_stage = |names: &[&str]| {
        pending.iter().any(|r| {
            r.get("H")
                .and_then(Value::as_str)
                .map(|h| names.contains(&h))
                .unwrap_or(false)
        })
    };
    let step = if has_stage(&["초안"]) {
        "`초안` 행을 spawn_agent(task_name `t26_<RawID>`)로 하나씩 할당하세요.".to_string()
    } else if has_stage(&["작업", "교정", "진단"]) {
        "에이전트 결과를 기다리세요(wait). 끝난 에이전트의 행이 `검증`이 아니면 같은 행으로 다음 교정을 할당하세요.".to_string()
    } else {
        format!(
            "모든 행이 `검증`이에요. 전체 흐름을 테스트하고 당일 `.ai/changes`에 `#### 확인 (#<id>)`(`- 요약 : <입력 또는 이전 단계> ⇒ <구현 결과>` 및 `- [x] 목적: <목적>` 아래 `결과 : 성공`·`상세 : <실제값 한 줄>`)를 쓴 뒤 \
`{CMD} flow-wait --project {project_dir}`를 실행하세요."
        )
    };
    format!(
        "종료 차단: Table26 {stages} (작업 폴더 {project_dir}). {step} \
사용자 입력(로그인·결제·비가역 외부 전송·OS 앱 조작)만 남았으면 `$H=확인`과 `$D` 구현 결과를 기록한 뒤 질문하세요."
    )
}

fn parse_cli_cwd() -> Option<String> {
    let mut args = std::env::args().skip(1);
    let mut cwd = None;
    while let Some(arg) = args.next() {
        if arg == "--project" || arg == "--cwd" {
            if let Some(val) = args.next() {
                cwd = Some(val);
            }
        } else if let Some(val) = arg.strip_prefix("--project=").or_else(|| arg.strip_prefix("--cwd=")) {
            cwd = Some(val.to_string());
        }
    }
    cwd
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
}

fn stdin_ready(timeout_ms: i32) -> bool {
    let mut pfd = PollFd { fd: 0, events: 1, revents: 0 };
    unsafe { poll(&mut pfd, 1, timeout_ms) > 0 }
}

fn read_stdin_bounded() -> String {
    if std::io::stdin().is_terminal() || !stdin_ready(20) {
        return String::new();
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut stdin = std::io::stdin().lock();
    loop {
        match stdin.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&chunk[..n]);
                if !stdin_ready(5) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn main() {
    let cli_cwd = parse_cli_cwd();
    let input = if cli_cwd.is_none() {
        read_stdin_bounded()
    } else {
        String::new()
    };
    let event = json::parse(&input).unwrap_or(Value::Obj(vec![]));
    let meta = thread_meta(&event);
    let agent_path = meta
        .get("agent_path")
        .filter(|v| v.truthy())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let thread = event
        .get("agent_id")
        .filter(|v| v.truthy())
        .or_else(|| meta.get("id").filter(|v| v.truthy()))
        .or_else(|| event.get("session_id").filter(|v| v.truthy()))
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    let s_dir = state_dir();
    let state_file = s_dir.join(format!("{}.json", sanitize(thread)));

    let cwd = cli_cwd
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            event
                .get("cwd")
                .filter(|v| v.truthy())
                .and_then(Value::as_str)
        })
        .unwrap_or(".");

    let mut sync_note = String::new();
    let (code, _, err) = grist(&["sync", "--cwd", cwd]);
    if code != 0 {
        let trimmed: String = err.trim().chars().take(200).collect();
        sync_note = format!("completion_gate: grist sync 실패(exit {code}) — {trimmed}");
    }

    let (code, out, err, allowed) = if !agent_path.is_empty() {
        let Some(row_id) = extract_agent_task(&agent_path) else {
            emit(vec![], &sync_note);
        };
        let (c, o, e) = grist(&["gate", "--cwd", cwd, "--row", row_id]);
        (c, o, e, AGENT_ALLOWED)
    } else {
        let (c, o, e) = grist(&["gate", "--cwd", cwd]);
        (c, o, e, MAIN_ALLOWED)
    };

    if code != 0 {
        let trimmed: String = err.trim().chars().take(300).collect();
        let mut obj = Vec::new();
        obj_set(
            &mut obj,
            "systemMessage",
            s(format!("completion_gate: grist gate 실패(exit {code}) — {trimmed}")),
        );
        emit(obj, &sync_note);
    }

    let status = json::parse(&out).unwrap_or(Value::Obj(vec![]));
    let rows = match status.get("rows") {
        Some(Value::Arr(r)) => r.clone(),
        _ => vec![],
    };
    let pending: Vec<Value> = rows
        .iter()
        .filter(|r| {
            let h = r.get("H").and_then(Value::as_str).unwrap_or("");
            !allowed.contains(&h)
        })
        .cloned()
        .collect();

    let judgment = if !rows.is_empty() && agent_path.is_empty() {
        let last_msg = event
            .get("last_assistant_message")
            .filter(|v| v.truthy())
            .and_then(Value::as_str)
            .unwrap_or("");
        jev_stop(last_msg)
    } else {
        Value::Obj(vec![])
    };

    let asks_user = judgment.get("category").and_then(Value::as_str) == Some("사용자 질문")
        && judged(&judgment, "category", &["pass"]);
    let disallowed_question = asks_user
        && judged(&judgment, "allowed_action", &["fail"])
        && judgment.get("mode").and_then(Value::as_str) == Some("enforce");
    let stop_reason = early_stop(&judgment);
    let jev_block = !stop_reason.is_empty() || disallowed_question;
    let uninitialized = if !agent_path.is_empty() {
        ""
    } else {
        status
            .get("uninitialized")
            .filter(|v| v.truthy())
            .and_then(Value::as_str)
            .unwrap_or("")
    };

    if pending.is_empty() && !jev_block && uninitialized.is_empty() {
        let _ = std::fs::remove_file(&state_file);
        emit(vec![], &sync_note);
    }

    let mut stage_items = Vec::new();
    for r in &rows {
        let id = r.get("id").map(Value::py_str).unwrap_or_default();
        let h = r.get("H").map(Value::py_str).unwrap_or_default();
        obj_set(&mut stage_items, &id, s(h));
    }
    let stages = Value::Obj(stage_items);

    let state = std::fs::read_to_string(&state_file)
        .ok()
        .and_then(|t| json::parse(&t).ok())
        .unwrap_or(Value::Obj(vec![]));

    let blocks = if state.get("stages").map(|st| st.py_eq(&stages)).unwrap_or(false) {
        state.get("blocks").and_then(Value::as_i64).unwrap_or(0)
    } else {
        0
    };

    let limit = if jev_block { J2_MAX_BLOCKS } else { MAX_BLOCKS };
    let label = if jev_block {
        let cat = judgment
            .get("category")
            .map(Value::py_str)
            .unwrap_or_else(|| "None".to_string());
        format!("J2 {cat} ")
    } else {
        String::new()
    };

    if blocks >= limit {
        let _ = std::fs::remove_file(&state_file);
        let target_rows = if !pending.is_empty() { &pending } else { &rows };
        for row in target_rows {
            let num = next_number(row.get("D").and_then(Value::as_str));
            let h = row.get("H").map(Value::py_str).unwrap_or_default();
            let memo = format!("{num}. 종료 게이트 {label}{limit}회 차단 후 종료($H={h})");
            let mut fields = Vec::new();
            obj_set(&mut fields, "D", s(memo));
            let id = row.get("id").map(Value::py_str).unwrap_or_default();
            grist(&["task-update", &id, "--fields", &json::dumps(&Value::Obj(fields))]);
        }
        let summary_rows = target_rows
            .iter()
            .map(|r| {
                format!(
                    "#{} $H={}",
                    r.get("id").map(Value::py_str).unwrap_or_default(),
                    r.get("H").map(Value::py_str).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let summary = if !summary_rows.is_empty() {
            summary_rows
        } else {
            format!("미초기화 {uninitialized}")
        };
        let mut obj = Vec::new();
        obj_set(
            &mut obj,
            "systemMessage",
            s(format!(
                "completion_gate: Table26 {summary} 상태로 {label}{limit}회 차단 뒤 종료를 허용했어요."
            )),
        );
        emit(obj, &sync_note);
    }

    let _ = std::fs::create_dir_all(&s_dir);
    let mut new_state = Vec::new();
    obj_set(&mut new_state, "blocks", Value::Num((blocks + 1).to_string()));
    obj_set(&mut new_state, "stages", stages);
    let _ = std::fs::write(&state_file, json::dumps(&Value::Obj(new_state)));

    let project_dir = status
        .get("project_dir")
        .filter(|v| v.truthy())
        .and_then(Value::as_str);

    let reason = if !agent_path.is_empty() {
        agent_reason(&pending[0], project_dir)
    } else if !uninitialized.is_empty() {
        init_reason(uninitialized)
    } else if pending.is_empty() {
        let stages_text = rows
            .iter()
            .map(|r| {
                format!(
                    "#{}={}",
                    r.get("id").map(Value::py_str).unwrap_or_default(),
                    r.get("H").map(Value::py_str).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let q = if disallowed_question {
            format!("허용되지 않은 사용자 요청이에요. {QUESTION_REASON}")
        } else {
            String::new()
        };
        format!("종료 차단: Table26 {stages_text}. {stop_reason}{q}")
    } else {
        let mut r = format!(
            "{stop_reason}{}",
            main_reason(&rows, &pending, project_dir.unwrap_or(""))
        );
        if disallowed_question {
            r.push(' ');
            r.push_str(QUESTION_REASON);
        }
        r
    };

    let mut out_obj = Vec::new();
    obj_set(&mut out_obj, "decision", s("block"));
    obj_set(
        &mut out_obj,
        "reason",
        s(format!("[{}/{limit}] {}", blocks + 1, reason.trim())),
    );
    emit(out_obj, &sync_note);
}

#[cfg(test)]
mod tests {
    use super::{main_reason, json::s, Value, MAIN_ALLOWED};

    #[test]
    fn main_allowed_and_reason_use_current_stages_and_headings() {
        assert_eq!(MAIN_ALLOWED, ["완료", "확인"]);
        let row = Value::Obj(vec![("id".to_string(), Value::Num("192".to_string())), ("H".to_string(), s("검증"))]);
        let msg = main_reason(&[row.clone()], &[row], "/home/tree/extensions");
        assert!(msg.contains("#### 확인 (#<id>)"));
        assert!(msg.contains("$H=확인"));
    }
}
