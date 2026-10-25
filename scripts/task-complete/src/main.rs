mod json;

use json::{obj_set, s, Value};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::path::Path;
use std::time::{Duration, SystemTime};

const MAIN_ALLOWED: &[&str] = &["완료", "확인"];
const AGENT_ALLOWED: &[&str] = &["검증", "완료"];
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
    std::env::var_os("TASK_COMPLETE_STATE_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".codex/tmp/task_complete"))
}

fn session_row_ids_at(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path).ok()
        .and_then(|text| json::parse(&text).ok())
        .and_then(|value| match value {
            Value::Arr(rows) => Some(rows.into_iter().map(|row| row.py_str()).collect()),
            _ => None,
        })
        .unwrap_or_default()
}

fn session_row_ids(thread: &str) -> Vec<String> {
    session_row_ids_at(&state_dir().join("session_rows").join(format!("{}.json", sanitize(thread))))
}

fn session_rows(rows: Vec<Value>, owned: &[String]) -> Vec<Value> {
    rows.into_iter().filter(|row| row.get("id")
        .is_some_and(|id| owned.contains(&id.py_str()))).collect()
}

fn grist(args: &[&str]) -> (i32, String, String) {
    let mut command = Command::new(grist_bin());
    command.args(args).stdin(Stdio::null());
    match command.output() {
        Ok(out) => (
            out.status.code().unwrap_or(1),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ),
        Err(e) => (1, String::new(), e.to_string()),
    }
}

/// One row's completion through the shared grist check chain (evidence → J1 → changes list → `$H=확인`).
fn complete_row(project: &str, row: &Value) -> Result<(), String> {
    let id = row.get("id").map(Value::py_str).ok_or("missing row id")?;
    let raw_id: i64 = id.parse().map_err(|_| format!("invalid row id {id}"))?;
    grist::task::check(Path::new(project), raw_id)
        .map(|_| ())
        .map_err(|e| format!("#{id} {}", grist::task::exit_message(&e)))
}

fn completed_ids(project: &str) -> Vec<String> {
    grist::task::completed_ids(Path::new(project)).iter().map(i64::to_string).collect()
}

/// The user's Grist `$H=완료` overrides every other record: such a row needs no list entry or judgment.
fn user_done(row: &Value) -> bool {
    row.get("H").and_then(Value::as_str) == Some("완료")
}

fn complete_work_valid(rows: &[Value], listed: &[Value], done: &[String]) -> bool {
    listed.iter().all(|item| matches!(item.get("H").and_then(Value::as_str), Some("확인" | "완료")))
        && rows.iter().all(|row| {
            let id = row.get("id").map(Value::py_str);
            user_done(row)
                || (id.as_ref().is_some_and(|id| done.contains(id))
                    && listed.iter().any(|item| item.get("id").map(Value::py_str) == id))
        })
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

fn jev_stop(message: &str) -> Result<Value, &'static str> {
    let j_bin = jev_bin();
    if message.trim().is_empty() || !j_bin.is_file() {
        return Err("J2 input or executable missing");
    }
    let tmp_path = std::env::temp_dir().join(format!(
        "task-complete-jev-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    if std::fs::write(&tmp_path, message).is_err() {
        return Err("J2 input write failed");
    }
    let out = Command::new(&j_bin)
        .args(["stop", "--message-file", &tmp_path.to_string_lossy()])
        .stdin(Stdio::null())
        .output();
    let _ = std::fs::remove_file(&tmp_path);
    let Ok(out) = out else {
        return Err("J2 process failed");
    };
    if !out.status.success() {
        return Err("J2 process failed");
    }
    parse_j2_response(&out.stdout)
}

/// Key of one J2 input: the final message and the row stages it is judged against.
fn j2_cache_key(message: &str, rows: &[Value]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    message.hash(&mut h);
    for r in rows {
        r.get("id").map(Value::py_str).unwrap_or_default().hash(&mut h);
        r.get("H").map(Value::py_str).unwrap_or_default().hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

/// Reuses the last J2 judgment when the message and row stages are unchanged.
fn jev_stop_cached(cache: &Path, message: &str, rows: &[Value]) -> Result<Value, &'static str> {
    let key = j2_cache_key(message, rows);
    let saved = std::fs::read_to_string(cache).ok().and_then(|t| json::parse(&t).ok());
    if let Some(saved) = saved.filter(|v| v.get("key").and_then(Value::as_str) == Some(key.as_str())) {
        if let Some(judgment) = saved.get("judgment") {
            return Ok(judgment.clone());
        }
    }
    let judgment = jev_stop(message)?;
    if let Some(dir) = cache.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(cache, json::dumps(&Value::Obj(vec![("key".into(), s(key)), ("judgment".into(), judgment.clone())])));
    Ok(judgment)
}

fn parse_j2_response(raw: &[u8]) -> Result<Value, &'static str> {
    let text = std::str::from_utf8(raw).map_err(|_| "invalid J2 response")?;
    let judgment = json::parse(text).map_err(|_| "invalid J2 response")?;
    let valid_shape = judgment.get("ok") == Some(&Value::Bool(true))
        && judgment.get("command").and_then(Value::as_str) == Some("stop")
        && matches!(judgment.get("mode").and_then(Value::as_str), Some("shadow" | "enforce"))
        && matches!(judgment.get("verdict").and_then(Value::as_str), Some("pass" | "uncertain" | "fail"))
        && judgment.get("category").and_then(Value::as_str).is_some_and(|category| !category.is_empty());
    let Some(Value::Arr(items)) = judgment.get("items") else { return Err("invalid J2 response"); };
    if !valid_shape || !items.iter().any(|item| item.get("name").and_then(Value::as_str) == Some("category")) {
        return Err("invalid J2 response");
    }
    Ok(judgment)
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
실패하면 `$H=진단`, `$I`·`$J`를 기록하고 원인을 고치세요. 메인의 결정이 필요하면 질문으로 끝내세요."
    )
}

fn worker_question(message: &str) -> bool {
    let Some(last) = message.lines().rev().map(str::trim).find(|line| !line.is_empty()) else {
        return false;
    };
    last.starts_with("[요청]")
        || last.ends_with('?')
        || last.ends_with('？')
        || last.ends_with("할까요")
}

fn worker_stop_allowed(event: &Value, previous: &Value, reason: &str) -> bool {
    let message = event.get("last_assistant_message").and_then(Value::as_str).unwrap_or("");
    worker_question(message)
        || (event.get("stop_hook_active") == Some(&Value::Bool(true))
            && previous.get("reason").and_then(Value::as_str) == Some(reason))
}

fn init_reason(project_dir: &str) -> String {
    format!(
        "종료 차단: {project_dir}는 `AGENTS.md`에 `# 상태`(`### grist` `ID :`) 헤더가 없는 미초기화 프로젝트인데 변경 파일이 있어요. \
`manage-project` 초기화 절차(`references/initialize.md`)로 Table7 행·Obsidian 문서·`# 상태` 헤더를 만들고, 현재 요청을 `request-record`·`task-add`·`task-goal`로 기록한 뒤 \
`manage-project` 흐름을 이어가세요."
    )
}

fn running_dispatch(project_dir: &Path) -> bool {
    let dispatch_dir = project_dir.join(".ai/dispatch");
    let Ok(entries) = std::fs::read_dir(&dispatch_dir) else { return false };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("sh") || !path.is_file() {
            return false;
        }
        let exit = path.with_extension("exit");
        let log = path.with_extension("log");
        !exit.exists() && log.metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age <= Duration::from_secs(60 * 60))
    })
}

fn dispatch_wait_allowed(rows: &[Value], project_dir: &Path) -> bool {
    let pending: Vec<&str> = rows.iter()
        .filter_map(|row| row.get("H").and_then(Value::as_str))
        .filter(|stage| !MAIN_ALLOWED.contains(stage))
        .collect();
    !pending.is_empty()
        && pending.iter().all(|stage| ["초안", "작업", "교정", "진단"].contains(stage))
        && running_dispatch(project_dir)
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
        "`초안` 행은 수정 파일 1~2개면 메인이 직접 처리하고, 3개 이상 묶음만 행마다 구현자(`t26_<RawID>` 또는 `dispatch-agent`)에게 맡기세요(`manage-project` `references/plan.md`).".to_string()
    } else if has_stage(&["작업", "교정", "진단"]) {
        "구현자 결과를 기다리세요. 끝난 구현자의 행이 `검증`이 아니면 같은 행으로 다음 교정을 맡기세요.".to_string()
    } else {
        format!(
            "모든 행이 `검증`이에요. 전체 흐름을 테스트하고 행마다 `{CMD} task-confirm <id> --project {project_dir} --summary \"[표시 기호] <입력 또는 이전 단계> ⇒ <구현 결과>\" --check \"<목적>\" --detail \"<실제값 한 줄>\" --evidence <파일: 목표 줄마다 \"N. <근거>\">`로 확인을 기록한 뒤 \
`{CMD} flow-wait --project {project_dir}`를 실행하세요. `.ai/changes`는 직접 열거나 쓰지 않고, 기록 확인은 `{CMD} task-show <id> --project {project_dir} --section 확인`으로 하세요."
        )
    };
    format!(
        "종료 차단: Table26 {stages} (작업 폴더 {project_dir}). {step} \
사용자 입력(로그인·결제·비가역 외부 전송·OS 앱 조작)만 남았으면 `$D`에 구현 결과와 사유를 기록한 뒤 질문하세요. `$H`는 직접 바꾸지 않고, 사용자가 Grist에서 `완료`로 처리하면 종료돼요."
    )
}

fn pending_reason_with_error(reason: String, completion_error: Option<&str>) -> String {
    match completion_error {
        Some(error) => format!("{error}. {reason}"),
        None => reason,
    }
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

fn parse_cli_row() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--row" { return args.next(); }
        if let Some(value) = arg.strip_prefix("--row=") { return Some(value.to_string()); }
    }
    None
}

fn needs_j2(has_rows: bool, agent_path: &str, cli_cwd: Option<&str>, cli_row: Option<&str>) -> bool {
    has_rows && agent_path.is_empty() && cli_cwd.is_none() && cli_row.is_none()
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

fn session_start_path(thread: &str) -> PathBuf {
    state_dir().join("session_start").join(format!("{}.json", sanitize(thread)))
}

fn git_root(dir: &Path) -> Option<PathBuf> {
    let out = Command::new("git").arg("-C").arg(dir).args(["rev-parse", "--show-toplevel"])
        .stdin(Stdio::null()).output().ok().filter(|o| o.status.success())?;
    let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

/// Git roots watched for unregistered changes: the session's working repo and the rules/Skill repo.
fn watched_roots(cwd: &str) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for dir in [PathBuf::from(cwd), home_dir().join("ai")] {
        if let Some(root) = git_root(&dir) {
            if !roots.contains(&root) { roots.push(root); }
        }
    }
    roots
}

/// SessionStart: store one working-tree snapshot per watched repo for this session.
fn record_session_start(event: &Value, thread: &str) {
    let cwd = event.get("cwd").filter(|v| v.truthy()).and_then(Value::as_str).unwrap_or(".");
    let repos = watched_roots(cwd).into_iter().filter_map(|root| {
        let tree = grist::gate::snapshot(&root)?;
        Some(Value::Obj(vec![("dir".into(), s(root.to_string_lossy())), ("tree".into(), s(tree))]))
    }).collect();
    let path = session_start_path(thread);
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let _ = std::fs::write(&path, json::dumps(&Value::Obj(vec![("repos".into(), Value::Arr(repos))])));
}

/// Files the record flow itself writes (`grist sync`, `request-record`), not session edits.
fn record_file(path: &str) -> bool {
    path.starts_with(".ai/") || path.contains("/.ai/")
}

fn display_dir(dir: &str) -> String {
    let home = home_dir().to_string_lossy().into_owned();
    match dir.strip_prefix(&home) { Some(rest) if !home.is_empty() => format!("~{rest}"), _ => dir.to_string() }
}

/// Files changed since this session's start snapshot, as `~/repo/path`. Empty without a snapshot.
fn unregistered_changes(thread: &str) -> Vec<String> {
    let Some(start) = std::fs::read_to_string(session_start_path(thread)).ok().and_then(|t| json::parse(&t).ok()) else {
        return vec![];
    };
    let Some(Value::Arr(repos)) = start.get("repos") else { return vec![] };
    let mut changed = Vec::new();
    for repo in repos {
        let (Some(dir), Some(tree)) = (repo.get("dir").and_then(Value::as_str), repo.get("tree").and_then(Value::as_str)) else { continue };
        for file in grist::gate::changed_since(Path::new(dir), tree) {
            if !record_file(&file) { changed.push(format!("{}/{file}", display_dir(dir))); }
        }
    }
    changed
}

/// Blocks once per distinct change list, so another session's edits never keep this one open.
fn unregistered_reason(thread: &str, changed: &[String]) -> Option<String> {
    if changed.is_empty() { return None; }
    let marker = state_dir().join("session_start").join(format!("{}.blocked", sanitize(thread)));
    let key = changed.join("\n");
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(key.as_str()) { return None; }
    let _ = std::fs::write(&marker, &key);
    let shown = changed.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
    let more = if changed.len() > 8 { format!(" 외 {}개", changed.len() - 8) } else { String::new() };
    Some(format!(
        "종료 차단: 이 세션에 등록된 Table26 행이 없는데 파일이 바뀌었어요({shown}{more}). \
이 세션이 바꾼 것이면 `manage-project` 1단계(request-record → task-add → task-goal)로 행을 만들고 검증하세요. \
다른 세션의 변경이면 그대로 종료하면 돼요. 같은 변경 목록으로는 다시 막지 않아요."
    ))
}

fn main() {
    if std::env::args().any(|a| a == "--session-start") {
        let event = json::parse(&read_stdin_bounded()).unwrap_or(Value::Obj(vec![]));
        let thread = std::env::var("CODEX_THREAD_ID").ok().filter(|id| !id.is_empty())
            .or_else(|| event.get("session_id").filter(|v| v.truthy()).and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| "unknown".into());
        record_session_start(&event, &thread);
        std::process::exit(0);
    }
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
        sync_note = format!("task-complete: grist sync 실패(exit {code}) — {trimmed}");
    }

    let cli_row = parse_cli_row();
    let worker_row = grist::gate::worker_row().map(|row| row.to_string());
    if worker_row.is_some() && cli_row.is_some() {
        println!("{{\"decision\": \"block\", \"reason\": \"task-complete --row is run by the orchestrator, not a dispatch-agent worker (T26_ROLE=worker)\"}}");
        std::process::exit(2);
    }
    let (code, out, err, allowed) = if let Some(row_id) = cli_row.as_deref() {
        let (c, o, e) = grist(&["gate", "--cwd", cwd, "--row", row_id]);
        (c, o, e, MAIN_ALLOWED)
    } else if let Some(row_id) = worker_row.as_deref() {
        let (c, o, e) = grist(&["gate", "--cwd", cwd, "--row", row_id]);
        (c, o, e, AGENT_ALLOWED)
    } else if !agent_path.is_empty() {
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
            s(format!("task-complete: grist gate 실패(exit {code}) — {trimmed}")),
        );
        emit(obj, &sync_note);
    }

    let status = json::parse(&out).unwrap_or(Value::Obj(vec![]));
    let gate_rows = match status.get("rows") {
        Some(Value::Arr(r)) => r.clone(),
        _ => vec![],
    };
    let hook_main = agent_path.is_empty() && worker_row.is_none() && cli_cwd.is_none() && cli_row.is_none();
    let session_id = std::env::var("CODEX_THREAD_ID").ok().filter(|id| !id.is_empty())
        .unwrap_or_else(|| thread.to_string());
    let owned = if hook_main { session_row_ids(&session_id) } else { vec![] };
    let initial_rows = if hook_main { session_rows(gate_rows, &owned) } else { gate_rows };

    let uninitialized = if !agent_path.is_empty() {
        ""
    } else {
        status
            .get("uninitialized")
            .filter(|v| v.truthy())
            .and_then(Value::as_str)
            .unwrap_or("")
    };

    let project_dir = status.get("project_dir").and_then(Value::as_str).unwrap_or(cwd);
    // The orchestrator alone runs the completion judgment; a t26 subagent or dispatch-agent worker ends at 검증.
    let orchestrator = agent_path.is_empty() && worker_row.is_none();
    let completion_checked = orchestrator && uninitialized.is_empty();
    let waiting_for_dispatch = hook_main && dispatch_wait_allowed(&initial_rows, Path::new(project_dir));
    let mut completion_error = if completion_checked {
        initial_rows.iter().try_for_each(|row| {
            if waiting_for_dispatch && row.get("H").and_then(Value::as_str)
                .is_some_and(|stage| ["초안", "작업", "교정", "진단"].contains(&stage)) {
                Ok(())
            } else {
                complete_row(project_dir, row)
            }
        }).err()
    } else { None };
    let rows_completed = completion_checked && completion_error.is_none();
    let mut rows = initial_rows;
    if orchestrator && uninitialized.is_empty() {
        let fresh = if let Some(id) = cli_row.as_deref() {
            grist(&["gate", "--cwd", cwd, "--row", id])
        } else {
            grist(&["gate", "--cwd", cwd])
        };
        if fresh.0 == 0 {
            if let Ok(refreshed) = json::parse(&fresh.1) {
                if let Some(Value::Arr(items)) = refreshed.get("rows") {
                    rows = if hook_main { session_rows(items.clone(), &owned) } else { items.clone() };
                }
            }
        } else if completion_error.is_none() {
            completion_error = Some("COMPLETE_WORK: row readback failed".into());
        }
    }
    let pending: Vec<Value> = rows.iter().filter(|r| {
        let h = r.get("H").and_then(Value::as_str).unwrap_or("");
        !allowed.contains(&h)
    }).cloned().collect();
    if orchestrator && uninitialized.is_empty() && !rows.is_empty() && rows_completed && pending.is_empty() {
        let done = completed_ids(project_dir);
        if !complete_work_valid(&rows, &rows, &done) {
            completion_error = Some("COMPLETE_WORK: completed list omitted a task".into());
        }
    }

    // J2 only decides a stop the rows would allow, so a blocked stop skips the Jev call.
    let judgment_result = if needs_j2(rows.iter().any(|row| !user_done(row)), &agent_path, cli_cwd.as_deref(), cli_row.as_deref())
        && pending.is_empty() && uninitialized.is_empty() && completion_error.is_none()
    {
        let last_msg = event
            .get("last_assistant_message")
            .filter(|v| v.truthy())
            .and_then(Value::as_str)
            .unwrap_or("");
        jev_stop_cached(&s_dir.join(format!("j2-{}.json", sanitize(thread))), last_msg, &rows)
    } else {
        Ok(Value::Obj(vec![]))
    };
    let j2_error = judgment_result.as_ref().err().map(|error| format!("J2 종료 판정 실패: {error}"));
    let judgment = judgment_result.unwrap_or(Value::Obj(vec![]));

    let asks_user = judgment.get("category").and_then(Value::as_str) == Some("사용자 질문")
        && judged(&judgment, "category", &["pass"]);
    let disallowed_question = asks_user
        && judged(&judgment, "allowed_action", &["fail"])
        && judgment.get("mode").and_then(Value::as_str) == Some("enforce");
    let stop_reason = j2_error.unwrap_or_else(|| early_stop(&judgment).to_string());
    let jev_block = !stop_reason.is_empty() || disallowed_question;

    if hook_main && owned.is_empty() && uninitialized.is_empty() {
        if let Some(reason) = unregistered_reason(&session_id, &unregistered_changes(&session_id)) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
    }

    if hook_main && uninitialized.is_empty() && completion_error.is_none()
        && dispatch_wait_allowed(&rows, Path::new(project_dir)) {
        let _ = std::fs::remove_file(&state_file);
        emit(vec![("systemMessage".into(), s("실행 중 구현자 대기: Stop hook 종료 허용"))], &sync_note);
    }

    if pending.is_empty() && !jev_block && uninitialized.is_empty() && completion_error.is_none() {
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

    let project_dir = status
        .get("project_dir")
        .filter(|v| v.truthy())
        .and_then(Value::as_str);

    let reason = if worker_row.is_some() || !agent_path.is_empty() {
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
        format!("종료 차단: Table26 {stages_text}. {stop_reason} {q}{}", completion_error.as_deref().unwrap_or(""))
    } else {
        let mut r = format!(
            "{stop_reason} {}",
            main_reason(&rows, &pending, project_dir.unwrap_or(""))
        );
        if disallowed_question {
            r.push(' ');
            r.push_str(QUESTION_REASON);
        }
        pending_reason_with_error(r, completion_error.as_deref())
    };

    if worker_row.is_some() && worker_stop_allowed(&event, &state, &reason) {
        let _ = std::fs::remove_file(&state_file);
        emit(vec![], &sync_note);
    }

    let _ = std::fs::create_dir_all(&s_dir);
    let mut new_state = Vec::new();
    obj_set(&mut new_state, "blocks", Value::Num((blocks + 1).to_string()));
    obj_set(&mut new_state, "stages", stages);
    obj_set(&mut new_state, "reason", s(&reason));
    let _ = std::fs::write(&state_file, json::dumps(&Value::Obj(new_state)));

    let mut out_obj = Vec::new();
    obj_set(&mut out_obj, "decision", s("block"));
    obj_set(
        &mut out_obj,
        "reason",
        s(format!("[{}] {}", blocks + 1, reason.trim())),
    );
    emit(out_obj, &sync_note);
}

#[cfg(test)]
mod tests {
    use super::{complete_work_valid, dispatch_wait_allowed, main_reason, needs_j2, parse_j2_response, pending_reason_with_error, running_dispatch, session_row_ids_at, session_rows, worker_stop_allowed, json::s, Value, MAIN_ALLOWED};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime};

    fn dispatch_fixture() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("tc-dispatch-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let dispatch = dir.join(".ai/dispatch");
        std::fs::create_dir_all(&dispatch).unwrap();
        std::fs::write(dispatch.join("359.sh"), "").unwrap();
        std::fs::write(dispatch.join("359.log"), "").unwrap();
        dir
    }

    fn stage_row(stage: &str) -> Value {
        Value::Obj(vec![("id".into(), Value::Num("359".into())), ("H".into(), s(stage))])
    }

    #[test]
    fn worker_question_ends_stop() {
        for message in ["진행 중이에요.\n메인이 답해 주실까요?", "[요청] 어느 경로를 사용할까요", "할까요"] {
            let event = Value::Obj(vec![("last_assistant_message".into(), s(message))]);
            assert!(worker_stop_allowed(&event, &Value::Obj(vec![]), "미제출"));
        }
    }

    #[test]
    fn worker_report_keeps_block() {
        let event = Value::Obj(vec![("last_assistant_message".into(), s("구현을 진행 중이에요."))]);
        assert!(!worker_stop_allowed(&event, &Value::Obj(vec![]), "미제출"));
        let earlier_question = Value::Obj(vec![("last_assistant_message".into(), s("할까요?\n구현을 진행 중이에요."))]);
        assert!(!worker_stop_allowed(&earlier_question, &Value::Obj(vec![]), "미제출"));
    }

    #[test]
    fn worker_same_reason_second_block_is_allowed() {
        let event = Value::Obj(vec![("stop_hook_active".into(), Value::Bool(true))]);
        let previous = Value::Obj(vec![("reason".into(), s("미제출"))]);
        assert!(worker_stop_allowed(&event, &previous, "미제출"));
        assert!(!worker_stop_allowed(&event, &previous, "다른 사유"));
        let first_stop = Value::Obj(vec![("stop_hook_active".into(), Value::Bool(false))]);
        assert!(!worker_stop_allowed(&first_stop, &previous, "미제출"));
    }

    #[test]
    fn dispatch_wait_allows_draft_with_running_worker() {
        let dir = dispatch_fixture();
        assert!(running_dispatch(&dir));
        assert!(dispatch_wait_allowed(&[stage_row("초안")], &dir));
        assert!(!dispatch_wait_allowed(&[stage_row("초안"), stage_row("검증")], &dir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dispatch_wait_blocks_draft_without_worker() {
        let dir = std::env::temp_dir().join(format!("tc-dispatch-none-{}", std::process::id()));
        assert!(!running_dispatch(&dir));
        assert!(!dispatch_wait_allowed(&[stage_row("초안")], &dir));
    }

    #[test]
    fn dispatch_wait_blocks_stale_log() {
        let dir = dispatch_fixture();
        let log = dir.join(".ai/dispatch/359.log");
        std::fs::File::open(log).unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(61 * 60)).unwrap();
        assert!(!running_dispatch(&dir));
        assert!(!dispatch_wait_allowed(&[stage_row("초안")], &dir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dispatch_wait_blocks_completed_worker() {
        let dir = dispatch_fixture();
        std::fs::write(dir.join(".ai/dispatch/359.exit"), "0").unwrap();
        assert!(!running_dispatch(&dir));
        assert!(!dispatch_wait_allowed(&[stage_row("초안")], &dir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn done_in(text: &str) -> Vec<String> {
        grist::task::section_ids_in_text(text, "# 완료").iter().map(i64::to_string).collect()
    }

    #[test]
    fn pending_reason_reports_completion_failure() {
        assert_eq!(pending_reason_with_error("H=검증".into(), Some("J1 goal/output mismatch")), "J1 goal/output mismatch. H=검증");
        assert_eq!(pending_reason_with_error("H=검증".into(), None), "H=검증");
    }

    #[test]
    fn jev_skip_reuses_cached_judgment_for_same_message_and_stages() {
        let dir = std::env::temp_dir().join(format!("tc-j2-{}", std::process::id()));
        let cache = dir.join("j2-t.json");
        let rows = vec![Value::Obj(vec![("id".into(), Value::Num("7".into())), ("H".into(), s("확인"))])];
        let judgment = Value::Obj(vec![("category".into(), s("완료 보고"))]);
        let key = super::j2_cache_key("끝", &rows);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&cache, super::json::dumps(&Value::Obj(vec![("key".into(), s(key)), ("judgment".into(), judgment)]))).unwrap();
        let got = super::jev_stop_cached(&cache, "끝", &rows).unwrap();
        println!("cached category ▷ {}", got.get("category").and_then(Value::as_str).unwrap_or(""));
        assert_eq!(got.get("category").and_then(Value::as_str), Some("완료 보고"));
        let changed = vec![Value::Obj(vec![("id".into(), Value::Num("7".into())), ("H".into(), s("검증"))])];
        assert_ne!(super::j2_cache_key("끝", &rows), super::j2_cache_key("끝", &changed));
        assert_ne!(super::j2_cache_key("끝", &rows), super::j2_cache_key("진행", &rows));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jev_skip_without_rows_or_for_subagent() {
        assert!(!needs_j2(false, "", None, None));
        assert!(!needs_j2(true, "t26_276", None, None));
    }

    #[test]
    fn j2_is_required_only_for_main_stop_hook() {
        assert!(needs_j2(true, "", None, None));
        assert!(!needs_j2(false, "", None, None));
        assert!(!needs_j2(true, "", Some("/project"), None));
        assert!(!needs_j2(true, "", Some("/project"), Some("276")));
        assert!(!needs_j2(true, "", None, Some("276")));
        assert!(!needs_j2(true, "t26_276", None, None));
        let done = Value::Obj(vec![("id".into(), Value::Num("7".into())), ("H".into(), s("완료"))]);
        assert!(!super::user_done(&Value::Obj(vec![("H".into(), s("확인"))])) && super::user_done(&done));
    }

    #[test]
    fn session_row_filter_ignores_previous_session_and_keeps_owned_row() {
        let path = std::env::temp_dir().join(format!("task-complete-session-row-{}.json", std::process::id()));
        std::fs::write(&path, "[277]").unwrap();
        let owned = session_row_ids_at(&path);
        let previous = Value::Obj(vec![("id".into(), Value::Num("272".into())), ("H".into(), s("진단"))]);
        let current = Value::Obj(vec![("id".into(), Value::Num("277".into())), ("H".into(), s("검증"))]);
        assert_eq!(session_rows(vec![previous.clone(), current.clone()], &owned), vec![current]);
        assert!(session_rows(vec![previous], &owned).is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn session_row_complete_work_excludes_previous_unfinished_row() {
        let previous = Value::Obj(vec![("id".into(), Value::Num("272".into())), ("H".into(), s("진단"))]);
        let current = Value::Obj(vec![("id".into(), Value::Num("277".into())), ("H".into(), s("확인"))]);
        let gate_rows = vec![previous, current];
        let completed = done_in("# 작업\n1. [🚿] 이전 세션 미완료 (#272)\n# 완료\n1. [🚿] 현재 세션 완료 (#277)\n# 진행\n");
        assert!(!complete_work_valid(&gate_rows, &gate_rows, &completed));
        let owned_rows = session_rows(gate_rows, &["277".into()]);
        assert!(complete_work_valid(&owned_rows, &owned_rows, &completed));
        assert!(!complete_work_valid(&owned_rows, &owned_rows, &[]));
        let previous_only = session_rows(vec![Value::Obj(vec![("id".into(), Value::Num("272".into())), ("H".into(), s("진단"))])], &["277".into()]);
        assert!(previous_only.is_empty());
    }

    #[test]
    fn j2_rejects_missing_or_malformed_judgment() {
        let pass = r#"{"ok":true,"mode":"shadow","command":"stop","verdict":"pass","category":"완료 보고","items":[{"name":"category","verdict":"pass"}]}"#;
        assert!(parse_j2_response(pass.as_bytes()).is_ok());
        for response in [
            "".to_string(), "{}".to_string(), "not json".to_string(),
            pass.replace("\"ok\":true", "\"ok\":false"),
            pass.replace("\"command\":\"stop\"", "\"command\":\"match\""),
            pass.replace("\"category\":\"완료 보고\"", "\"category\":null"),
            pass.replace("[{\"name\":\"category\",\"verdict\":\"pass\"}]", "[]"),
        ] {
            assert!(parse_j2_response(response.as_bytes()).is_err(), "{response}");
        }
    }

    #[test]
    fn user_done_stage_overrides_completed_list_but_confirmed_row_needs_it() {
        let text = "# 작업\n1. [✒️] 사용자 완료 (#7)\n# 완료\n1. [✒️] 이전 완료 (#8)\n# 진행\n";
        let done = done_in(text);
        assert_eq!(done, vec!["8"]);
        let active = Value::Obj(vec![("id".into(), Value::Num("7".into())), ("H".into(), s("완료"))]);
        let completed = Value::Obj(vec![("id".into(), Value::Num("8".into())), ("H".into(), s("완료"))]);
        assert!(complete_work_valid(&[active.clone(), completed.clone()], &[active, completed.clone()], &done));
        assert!(complete_work_valid(&[completed.clone()], &[completed], &done));
        let confirmed = Value::Obj(vec![("id".into(), Value::Num("7".into())), ("H".into(), s("확인"))]);
        assert!(!complete_work_valid(&[confirmed.clone()], &[confirmed], &done));
    }

    #[test]
    fn main_allowed_and_reason_use_current_stages_and_headings() {
        assert_eq!(MAIN_ALLOWED, ["완료", "확인"]);
        let row = Value::Obj(vec![("id".to_string(), Value::Num("192".to_string())), ("H".to_string(), s("검증"))]);
        let msg = main_reason(&[row.clone()], &[row], "/home/tree/extensions");
        assert!(msg.contains("task-confirm <id>") && msg.contains("--section 확인") && !msg.contains("당일 `.ai/changes`에"));
        assert!(!msg.contains("`$H=확인`과"));
        assert!(msg.contains("Grist에서 `완료`로 처리하면 종료돼요"));
    }

    #[test]
    fn unregistered_changes_block_once_and_skip_record_files() {
        use std::process::Command;
        let base = std::env::temp_dir().join(format!("tc-unreg-{}", std::process::id()));
        let repo = base.join("repo");
        std::fs::create_dir_all(repo.join(".ai/changes")).unwrap();
        std::fs::write(repo.join("a.txt"), "1").unwrap();
        let git = |args: &[&str]| { Command::new("git").arg("-C").arg(&repo).args(args).output().unwrap(); };
        git(&["init", "-q"]);
        let tree = grist::gate::snapshot(&repo).unwrap();
        std::env::set_var("TASK_COMPLETE_STATE_DIR", base.join("state"));
        let start = super::session_start_path("t1");
        std::fs::create_dir_all(start.parent().unwrap()).unwrap();
        let dir = repo.to_string_lossy().into_owned();
        std::fs::write(&start, format!("{{\"repos\":[{{\"dir\":\"{dir}\",\"tree\":\"{tree}\"}}]}}")).unwrap();

        std::fs::write(repo.join(".ai/changes/x.md"), "rec").unwrap();
        assert!(super::unregistered_changes("t1").is_empty());
        assert!(super::unregistered_reason("t1", &[]).is_none());

        std::fs::write(repo.join("a.txt"), "2").unwrap();
        let changed = super::unregistered_changes("t1");
        assert_eq!(changed.len(), 1);
        assert!(changed[0].ends_with("/repo/a.txt"));
        let reason = super::unregistered_reason("t1", &changed).unwrap();
        assert!(reason.contains("등록된 Table26 행이 없는데") && reason.contains("a.txt"));
        assert!(super::unregistered_reason("t1", &changed).is_none());
        std::fs::write(repo.join("b.txt"), "new").unwrap();
        assert!(super::unregistered_reason("t1", &super::unregistered_changes("t1")).is_some());
        assert!(super::unregistered_changes("no-start").is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
