mod json;
#[path = "../../prompt-decision.rs"]
mod prompt_decision;
use prompt_decision::{decision, Label};
#[path = "../../runtime-kind.rs"]
mod runtime_kind;
use runtime_kind::{runtime_kind, RuntimeKind};
#[path = "../../analysis-report.rs"]
mod analysis_report;
use analysis_report::analysis_report_problem;

use json::{obj_set, s, Value};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicBool, Ordering};
use std::os::fd::AsRawFd;

const MAIN_ALLOWED: &[&str] = &["완료", "확인"];
const AGENT_ALLOWED: &[&str] = &["검증", "완료"];
const VERIFIER_ALLOWED: &[&str] = &["초안", "작업", "교정", "진단", "검증", "확인", "완료"];
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

/// Why a `resolve-deadlock` analysis verifier may not stop yet: its report must be complete.
fn analysis_stop_reason(path: &str) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => analysis_report_problem(&text).map(|problem| format!("분석 보고서 {path}: {problem}. resolve-deadlock 형식으로 고친 뒤 종료하세요.")),
        Err(_) => Some(format!("분석 보고서 {path}가 없어요. resolve-deadlock 형식으로 쓴 뒤 종료하세요.")),
    }
}

fn verifier_recorded(project_dir: &Path, row_id: i64) -> bool {
    grist::gate::gate_load(project_dir, row_id)
        .get("verifier_recorded").and_then(|v| v.as_bool()) == Some(true)
}

fn verifier_execution(project_dir: &Path, row_id: i64, row: &Value) -> Result<(), String> {
    let n8n_marked = project_dir.join(format!(".ai/gate/{row_id}.n8n")).is_file();
    if user_action_reason(row) && !n8n_marked { return Ok(()); }
    let kind = runtime_kind(project_dir)?;
    let log_path = project_dir.join(format!(".ai/dispatch/{row_id}.verify.log"));
    let log = std::fs::read_to_string(log_path)
        .map_err(|_| format!("검증자 #{row_id}: 이번 실행 verify.log가 없어요"))?;
    let log = log.rfind("\nOpenAI Codex v").map(|index| &log[index + 1..]).unwrap_or(&log);
    let lines: Vec<&str> = log.lines().collect();
    if n8n_marked {
        let latest = lines.iter().rev().find(|line| line.starts_with("[verify-step] loss-check "));
        let result = latest.ok_or_else(|| format!("검증자 #{row_id}: 이번 verify.log에 손실 대조 줄이 없어요. ~/ai/skills/role-verifier/references/flow.md의 `[verify-step] loss-check source=<N> recorded=<N> excluded=<N> deleted=<N> unexplained=0`을 기록하세요."))?;
        validate_loss_check_line(result).map_err(|reason| format!("검증자 #{row_id}: 손실 대조 {reason}. ~/ai/skills/role-verifier/references/flow.md의 loss-check 규칙을 확인하세요."))?;
        validate_site_coverage(&lines).map_err(|reason| format!("검증자 #{row_id}: 사이트 대조 {reason}. 유닛 테스트·n8n 자체 수집 결과가 아니라 각 사이트 마감순 목록(사이트 응답)과 Grist API 행을 대조한 `[verify-step] site-coverage site=<사이트> listed=<N> in_grist=<N> missing=0` 줄이 필요해요(~/ai/skills/role-verifier/references/flow.md)."))?;
    }
    // 261114 #944: 검증자가 대행자 유닛 테스트를 다시 돌려 토큰만 쓰고 실제 누락은 못 찾았다. 유닛 테스트는 대행자 몫이다.
    if let Some(line) = lines.iter().find(|line| (line.starts_with("/usr/sbin/bash -lc ") || line.starts_with("/bin/bash -lc "))
        && ["vitest", "cargo test", "npm test", "npm run test", "node --test", "pytest", "jest"].iter().any(|runner| line.contains(runner))) {
        return Err(format!("검증자 #{row_id}: 대행자 유닛 테스트를 다시 실행했어요({}). 검증자는 유닛 테스트를 돌리지 않고 실제 실행 결과와 최종 소비자(Grist API 등)만 대조해요(~/ai/skills/role-verifier/references/flow.md).", line.chars().take(80).collect::<String>()));
    }
    let command = lines.iter().any(|line| line.starts_with("/usr/sbin/bash -lc ") || line.starts_with("/bin/bash -lc "));
    let completed = lines.iter().any(|line| line.trim_start().starts_with("succeeded in "));
    let executed = match kind {
        RuntimeKind::Web => lines.iter().any(|line| line.starts_with("[stagehand-step] cdp-attach-done "))
            || lines.iter().any(|line| line.starts_with("[verify-step] relay op=") && line.contains(" ok=true"))
            || lines.iter().any(|line| line.starts_with("[verify-step] screenshot=") && line.contains(".png")),
        RuntimeKind::App => command && completed && lines.iter().any(|line| line.starts_with("[verify-step] runtime-start cmd=")),
        RuntimeKind::Cli => command && completed && lines.iter().any(|line| line.starts_with("[verify-step] cli-run cmd=")),
    };
    if !executed { return Err(format!("검증자 #{row_id}: {:?} 실제 실행 기록이 이번 verify.log에 없어요", kind)); }
    let readback = lines.iter().any(|line| line.strip_prefix("[verify-step] readback consumer=").is_some_and(|rest| rest.contains(" value=") && !rest.trim_end().ends_with("value=")));
    if !readback { return Err(format!("검증자 #{row_id}: 최종 소비자 readback(`[verify-step] readback consumer=<소비자> value=<직접 읽은 값>`)이 이번 verify.log에 없어요 — ACK·로그 줄만으로 통과할 수 없어요")); }
    // 261110 #813: mock 하니스 결과만으로 `확인`이 됨 → 실제 데이터 흐름 기록을 필수로 하고 mock 표기 readback을 거부한다.
    let mock_word = |line: &str| { let lower = line.to_lowercase(); ["mock", "가짜", "stub", "스텁", "fake"].iter().any(|word| lower.contains(word)) };
    if lines.iter().any(|line| line.starts_with("[verify-step] readback ") && mock_word(line)) {
        return Err(format!("검증자 #{row_id}: readback이 mock·가짜 결과예요 — 실제 흐름에서 읽은 값만 쓸 수 있어요"));
    }
    let range_lines: Vec<&str> = lines.iter().copied().filter(|line| line.starts_with("[verify-step] range-check ")).collect();
    if !range_lines.is_empty() && !range_lines.iter().copied().any(valid_range_check_line) {
        return Err(format!("검증자 #{row_id}: 범위 검증(`[verify-step] range-check start=<시작> mid=<중간> end=<끝>`)은 기본 시작(0·1)이나 시작·끝 한쪽만 보면 안 되고 서로 다른 시작·중간·끝 랜덤 범위여야 해요"));
    }
    // 기존 흐름이 정상이라는 근거(이전 확인 행·배포 버전)가 있고 표시·변형만 바뀐 행은 바뀐 부분 readback으로 충분하다.
    if lines.iter().any(|line| valid_partial_scope_line(line)) { return Ok(()); }
    let real = lines.iter().any(|line| line.strip_prefix("[verify-step] real-flow ").is_some_and(|rest| !rest.trim().is_empty() && !mock_word(rest)));
    if real { Ok(()) } else { Err(format!("검증자 #{row_id}: 실제 데이터 흐름 기록(`[verify-step] real-flow <실제 시작 동작→외부 연동→최종 소비자>`)이 이번 verify.log에 없어요 — mock 사례만으로 통과할 수 없어요")) }
}

/// 261114: 유닛 테스트·n8n 자체 수집 결과만으로는 사이트에 있는데 빠진 공모전을 못 잡았다.
/// n8n 표식 행은 사이트가 직접 내준 목록과 Grist API 행을 사이트마다 대조한 줄이 있어야 한다.
fn validate_site_coverage(lines: &[&str]) -> Result<(), &'static str> {
    let rows: Vec<&&str> = lines.iter().filter(|line| line.starts_with("[verify-step] site-coverage ")).collect();
    if rows.is_empty() { return Err("site-coverage 줄이 없어요"); }
    for line in rows {
        let field = |name: &str| line.split_whitespace().find_map(|word| word.strip_prefix(name)).map(str::to_string);
        if field("site=").filter(|site| !site.is_empty()).is_none() { return Err("site 값이 없어요"); }
        let num = |name: &str| field(name).and_then(|value| value.parse::<u64>().ok());
        let (Some(listed), Some(in_grist), Some(missing)) = (num("listed="), num("in_grist="), num("missing=")) else { return Err("수치 형식이 올바르지 않아요") };
        if listed == 0 { return Err("listed가 0이에요"); }
        if missing != 0 { return Err("missing이 0이 아니에요"); }
        if in_grist > listed { return Err("in_grist가 listed보다 커요"); }
    }
    Ok(())
}

fn validate_loss_check_line(line: &str) -> Result<(), &'static str> {
    let rest = line.strip_prefix("[verify-step] loss-check ").ok_or("형식이 올바르지 않아요")?;
    let mut words = rest.split_whitespace();
    let mut number = |field: &str| -> Result<u64, &'static str> {
        words.next().and_then(|word| word.strip_prefix(field))
            .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse::<u64>().ok()).ok_or("수치 형식이 올바르지 않아요")
    };
    let source = number("source=")?;
    let recorded = number("recorded=")?;
    let excluded = number("excluded=")?;
    let deleted = number("deleted=")?;
    let unexplained = number("unexplained=")?;
    if words.next().is_some() { return Err("추가 필드가 있어요"); }
    if unexplained != 0 { return Err("unexplained가 0이 아니에요"); }
    if recorded.checked_add(excluded).and_then(|sum| sum.checked_add(deleted)) != Some(source) {
        return Err("source와 recorded + excluded + deleted가 일치하지 않아요");
    }
    Ok(())
}

/// `[verify-step] scope=display|variant base=<#행 또는 vX.Y.Z> changed=<바뀐 부분>`:
/// 기존 흐름의 정상 근거(이전 확인 행·배포 버전)와 바뀐 부분이 모두 있어야 부분 검증으로 인정한다.
fn valid_partial_scope_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("[verify-step] scope=") else { return false };
    let field = |key: &str| rest.split_whitespace().find_map(|word| word.strip_prefix(key)).unwrap_or("");
    let kind = rest.split_whitespace().next().unwrap_or("");
    let base = field("base=");
    let base_ok = base.strip_prefix('#').is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
        || base.strip_prefix('v').is_some_and(|ver| ver.split('.').count() >= 2 && ver.split('.').all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())));
    matches!(kind, "display" | "variant") && base_ok && !field("changed=").is_empty()
}

fn valid_range_check_line(line: &str) -> bool {
    let mut start = None;
    let mut mid = None;
    let mut end = None;
    for token in line.split_whitespace() {
        if let Some(v) = token.strip_prefix("start=").filter(|v| !v.is_empty()) { start = Some(v); }
        else if let Some(v) = token.strip_prefix("mid=").filter(|v| !v.is_empty()) { mid = Some(v); }
        else if let Some(v) = token.strip_prefix("end=").filter(|v| !v.is_empty()) { end = Some(v); }
    }
    let (Some(s), Some(m), Some(e)) = (start, mid, end) else { return false };
    if s == m || m == e || s == e { return false; }
    if let (Ok(sn), Ok(mn), Ok(en)) = (s.parse::<i64>(), m.parse::<i64>(), e.parse::<i64>()) {
        return sn > 1 && sn < mn && mn < en;
    }
    !matches!(s, "0" | "1")
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

fn process_owned_rows(owned: Vec<String>, projects: &[(String, String)], process: bool) -> Vec<String> {
    if !process { return owned; }
    owned.into_iter().filter(|id| projects.iter().any(|(row, path)| row == id && path == "/home/tree/extensions")).collect()
}

fn unfinished_process_rows(owned: &[String], rows: &[Value]) -> Vec<String> {
    owned.iter().filter(|id| rows.iter().any(|row| {
        row.get("id").is_some_and(|value| value.py_str() == **id)
            && row.get("H").and_then(Value::as_str)
                .is_some_and(|stage| !matches!(stage.trim(), "확인" | "완료"))
    })).cloned().collect()
}

const PROCESS_BLOCK: &str = "과정 수정 미완료(manage-intervention): ";
static PROCESS_STOP: AtomicBool = AtomicBool::new(false);

/// A process turn whose only rows are ones already closed before the prompt has no evidence of its own.
fn process_rows_only_prior_closed(rows: &[Value], owned: &[String], prior: &[String]) -> bool {
    !owned.is_empty() && owned.iter().all(|id| prior.contains(id) && rows.iter()
        .find(|row| row.get("id").is_some_and(|value| value.py_str() == *id))
        .and_then(|row| row.get("H")).and_then(Value::as_str)
        .is_some_and(|stage| matches!(stage.trim(), "확인" | "완료")))
}

fn session_project_map(thread: &str) -> Vec<(String, String)> {
    let path = state_dir().join("session_projects").join(format!("{}.json", sanitize(thread)));
    std::fs::read_to_string(path).ok().and_then(|text| json::parse(&text).ok())
        .and_then(|value| match value { Value::Obj(items) => Some(items), _ => None })
        .unwrap_or_default().into_iter().filter_map(|(id, path)| Some((id, path.as_str()?.to_string()))).collect()
}

fn rows_for_session_projects_with(owned: &[String], projects: &[(String, String)], cwd: &str, base: Vec<Value>, mut fetch: impl FnMut(&str) -> Result<Vec<Value>, String>) -> Result<Vec<Value>, String> {
    let mut rows = session_rows(base, owned);
    let mut queried = std::collections::BTreeSet::new();
    for (_, project) in projects {
        if project == cwd || !queried.insert(project.as_str()) { continue; }
        for row in session_rows(fetch(project)?, owned) {
            if !rows.iter().any(|existing| existing.get("id") == row.get("id")) { rows.push(row); }
        }
    }
    Ok(rows)
}

fn rows_for_session_projects(owned: &[String], projects: &[(String, String)], cwd: &str, base: Vec<Value>) -> Result<Vec<Value>, String> {
    let mut rows = rows_for_session_projects_with(owned, projects, cwd, base, |project| {
        let (sync_code, _, _) = grist(&["sync", "--cwd", project]);
        if sync_code != 0 { return Err(format!("등록된 프로젝트 grist sync 실패(exit {sync_code})")); }
        let (code, output, _) = grist(&["gate", "--cwd", project]);
        if code != 0 { return Err(format!("등록된 프로젝트 grist gate 실패(exit {code})")); }
        let status = json::parse(&output).map_err(|_| "등록된 프로젝트 grist gate JSON 오류".to_string())?;
        match status.get("rows") { Some(Value::Arr(items)) => Ok(items.clone()), _ => Err("등록된 프로젝트 grist gate rows 누락".into()) }
    })?;
    for id in owned {
        if rows.iter().any(|row| row.get("id").is_some_and(|value| value.py_str() == *id)) { continue; }
        let project = projects.iter().find(|(registered, _)| registered == id)
            .map(|(_, path)| path.as_str()).ok_or_else(|| format!("세션 행 #{id} 소유 프로젝트 경로 누락"))?;
        let (code, output, _) = grist(&["gate", "--cwd", project, "--row", id]);
        if code != 0 { return Err(format!("세션 행 #{id} grist gate 실패(exit {code})")); }
        let status = json::parse(&output).map_err(|_| format!("세션 행 #{id} grist gate JSON 오류"))?;
        let row = match status.get("rows") {
            Some(Value::Arr(items)) => items.iter().find(|row| row.get("id").is_some_and(|value| value.py_str() == *id)),
            _ => None,
        }.ok_or_else(|| format!("세션 행 #{id} grist gate 결과 누락"))?;
        rows.push(row.clone());
    }
    Ok(rows)
}

fn check_session_rows_with(rows: &[Value], projects: &[(String, String)], cwd: &str, mut check: impl FnMut(&str, &Value) -> Result<(), String>) -> Result<(), String> {
    rows.iter().try_for_each(|row| {
        let project = projects.iter().find(|(id, _)| row.get("id").is_some_and(|value| value.py_str() == *id))
            .map(|(_, path)| path.as_str()).unwrap_or(cwd);
        check(project, row)
    })
}

fn check_session_requests(rows: &[Value], projects: &[(String, String)], cwd: &str) -> Result<(), String> {
    check_session_requests_with(rows, projects, cwd, |project, ids| {
        grist::task::check_request(project, ids).map_err(|e| grist::task::exit_message(&e))
    })
}

fn check_session_requests_with(rows: &[Value], projects: &[(String, String)], cwd: &str, mut check: impl FnMut(&Path, &[i64]) -> Result<(), String>) -> Result<(), String> {
    let mut by_project: std::collections::BTreeMap<String, Vec<i64>> = std::collections::BTreeMap::new();
    for row in rows {
        if row.get("H").and_then(Value::as_str) == Some("완료") { continue; }
        let Some(id) = row.get("id").and_then(Value::as_i64) else { continue };
        let project = projects.iter().find(|(registered, _)| registered == &id.to_string())
            .map(|(_, path)| path.as_str()).unwrap_or(cwd);
        by_project.entry(project.to_string()).or_default().push(id);
    }
    for (project, ids) in by_project {
        check(Path::new(&project), &ids)?;
    }
    Ok(())
}

fn claude_session_path_at(state: &Path, cwd: &Path) -> PathBuf {
    let project = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    state.join("claude_project_session").join(format!("{}.json", sanitize(&project.to_string_lossy())))
}

fn record_claude_session_at(state: &Path, cwd: &Path, session_id: &str) -> std::io::Result<()> {
    let path = claude_session_path_at(state, cwd);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let project = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let data = Value::Obj(vec![("cwd".into(), s(project.to_string_lossy())), ("session_id".into(), s(session_id))]);
    std::fs::write(path, json::dumps(&data))?;
    Ok(())
}

fn record_claude_session(cwd: &Path, session_id: &str) -> std::io::Result<()> {
    record_claude_session_at(&state_dir(), cwd, session_id)
}

fn session_id_from(event: &Value, codex_thread: Option<&str>) -> Option<String> {
    codex_thread.filter(|id| !id.is_empty()).map(str::to_string)
        .or_else(|| event.get("session_id").filter(|v| v.truthy()).and_then(Value::as_str).map(str::to_string))
}

fn hook_session_id(event: &Value) -> Option<String> {
    let codex_thread = std::env::var("CODEX_THREAD_ID").ok();
    session_id_from(event, codex_thread.as_deref())
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
    if row.get("H").and_then(Value::as_str) != Some("완료") {
        ensure_structure_matches(Path::new(project)).map_err(|e| format!("#{id} {e}"))?;
    }
    grist::task::check(Path::new(project), raw_id)
        .map(|_| ())
        .map_err(|e| format!("#{id} {}", grist::task::exit_message(&e)))?;
    grist::records::blackbox("complete", raw_id, None);
    Ok(())
}

/// 개선자 세션의 `improve-entry done` 완료 표시를 확인한다.
fn improvement_entry_done(project: &Path, raw_id: i64) -> Result<bool, String> {
    let record = grist::records::item_file(project, raw_id).ok_or("changes 기록 없음")?;
    let text = std::fs::read_to_string(&record).map_err(|e| format!("changes 읽기 실패: {e}"))?;
    let marker = format!("- [x] [#{raw_id}] ");
    Ok(grist::records::improve_lines(&text).is_some_and(|lines| lines.iter().any(|line| line.starts_with(&marker))))
}

const PROCESS_TIMEOUT_NOTE: &str = "[🚥] 과정 수정 20분 초과";

/// A per-row lock makes process notes idempotent across concurrent Stop invocations.
fn process_overdue_notes_with(
    lock_dir: &Path, owned: &[String], prior: &[String], started: u64, now: u64,
    mut exists: impl FnMut(i64) -> Result<bool, String>,
    mut note: impl FnMut(i64, &str) -> Result<(), String>,
) -> Result<usize, String> {
    let elapsed = now.saturating_sub(started);
    if elapsed < 20 * 60 { return Ok(0); }
    std::fs::create_dir_all(lock_dir).map_err(|e| format!("과정 기록 잠금 디렉터리: {e}"))?;
    let mut recorded = 0;
    for id in owned.iter().filter(|id| !prior.contains(id)) {
        let raw_id: i64 = id.parse().map_err(|_| format!("잘못된 과정 행 ID: {id}"))?;
        let lock_path = lock_dir.join(format!("{raw_id}.lock"));
        let lock = std::fs::OpenOptions::new().create(true).read(true).write(true).open(lock_path)
            .map_err(|e| format!("과정 기록 잠금 파일: {e}"))?;
        if unsafe { flock(lock.as_raw_fd(), 2) } != 0 { return Err(format!("과정 기록 잠금 실패: {}", std::io::Error::last_os_error())); }
        let result: Result<bool, String> = (|| {
            if exists(raw_id)? { return Ok(false); }
            let message = format!("{PROCESS_TIMEOUT_NOTE} — 경과 {}m", elapsed / 60);
            note(raw_id, &message)?;
            Ok(true)
        })();
        unsafe { flock(lock.as_raw_fd(), 8); }
        if result? { recorded += 1; }
    }
    Ok(recorded)
}

fn process_overdue_notes(owned: &[String], prior: &[String], started: u64, now: u64) -> Result<usize, String> {
    let project = Path::new("/home/tree/extensions");
    let lock_dir = state_dir().join("process_note_locks");
    process_overdue_notes_with(&lock_dir, owned, prior, started, now,
        |id| Ok(lock_dir.join(format!("{id}.recorded")).is_file()),
        |id, message| {
            let result = std::process::Command::new("/home/tree/extensions/manage_improve/scripts/bin/manage_improve")
                .args(["note", "--from", project.to_str().unwrap(), "--row", &id.to_string(), "--target", "hook:task-complete", "--sign", "🚥", "--text", message])
                .output();
            match result {
                Ok(output) if output.status.success() => {
                    if let Err(error) = std::fs::write(lock_dir.join(format!("{id}.recorded")), "1") {
                        eprintln!("task-complete: 개선 기록 상태 저장 실패: {error}");
                    }
                }
                Ok(output) => eprintln!("task-complete: 개선 기록 실패(exit {:?})", output.status.code()),
                Err(error) => eprintln!("task-complete: 개선 기록 실행 실패: {error}"),
            }
            Ok(())
        })
}

/// 개선자(`T26_ROLE=improver`) Stop: 자기 행의 완료 항목만 본다.
fn improver_stop(project: &Path, row: Option<&str>) -> Result<(), String> {
    let raw_id: i64 = row.and_then(|value| value.parse().ok()).ok_or("개선자 T26_ROW가 없거나 숫자가 아니에요.")?;
    match improvement_entry_done(project, raw_id)? {
        true => Ok(()),
        false => Err(format!("개선자 종료 차단: #{raw_id}의 개선을 `grist improve-entry done {raw_id} --project <dir> --text ...`로 기록하세요")),
    }
}

fn ensure_structure_matches(project: &Path) -> Result<(), String> {
    let doc = std::fs::read_to_string(project.join(".ai/docs/architecture.md"))
        .map_err(|e| format!("architecture.md 읽기 실패: {e}"))?;
    let difference = read_architecture::structure_difference(project, &doc)?;
    if difference.is_empty() {
        return Ok(());
    }
    Err(format!(
        "구조 차이: 문서에만 있음 {} / 문서에 없음 {} — .ai/docs/architecture.md를 갱신하세요",
        difference.documented_only.join(", "),
        difference.undocumented.join(", ")
    ))
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

/// Consecutive hook-forced continuations allowed before a stop goes through.
/// Claude Code overrides a Stop hook after 9 blocks, so the gate yields first with its own note.
const MAX_STOP_STREAK: i64 = 3;

static STOP_STREAK: std::sync::OnceLock<(bool, PathBuf)> = std::sync::OnceLock::new();

/// 차단 사유에서 매번 바뀌는 앞쪽 `[N] ` 횟수를 뺀 비교용 키.
fn stop_reason_key(reason: &str) -> String {
    let trimmed = reason.trim();
    match trimmed.strip_prefix('[').and_then(|rest| rest.split_once("] ")) {
        Some((count, rest)) if count.chars().all(|c| c.is_ascii_digit()) => rest.to_string(),
        _ => trimmed.to_string(),
    }
}

fn cap_stop_streak(output: Vec<(String, Value)>, stop_hook_active: bool, streak_file: &Path) -> Vec<(String, Value)> {
    // 이미 한 번 양보한 같은 사유는 다음 턴에도 다시 막지 않는다: 매 턴 3회씩 같은 차단을 반복하던 문제(261114 #952 [17]회).
    let yielded_file = streak_file.with_extension("yielded");
    let blocking = output.iter().any(|(k, v)| k == "decision" && v.as_str() == Some("block"));
    if !blocking {
        let _ = std::fs::remove_file(streak_file);
        let _ = std::fs::remove_file(&yielded_file);
        return output;
    }
    let reason = output.iter().find(|(k, _)| k == "reason").and_then(|(_, v)| v.as_str()).unwrap_or("");
    let key = stop_reason_key(reason);
    if std::fs::read_to_string(&yielded_file).is_ok_and(|seen| seen == key) {
        return vec![];
    }
    let previous = if stop_hook_active {
        std::fs::read_to_string(streak_file).ok().and_then(|t| t.trim().parse::<i64>().ok()).unwrap_or(0)
    } else {
        0
    };
    if previous >= MAX_STOP_STREAK {
        let _ = std::fs::remove_file(streak_file);
        let _ = std::fs::write(&yielded_file, &key);
        let head: String = reason.chars().take(120).collect();
        return vec![("systemMessage".into(), s(format!(
            "task-complete: {MAX_STOP_STREAK}회 연속 차단 뒤 종료를 허용했어요. 같은 사유로는 다시 막지 않아요. 마지막 사유: {head}"
        )))];
    }
    if let Some(dir) = streak_file.parent() { let _ = std::fs::create_dir_all(dir); }
    let _ = std::fs::write(streak_file, (previous + 1).to_string());
    output
}

fn process_stop_prefix(mut output: Vec<(String, Value)>, process: bool) -> Vec<(String, Value)> {
    if process
        && output.iter().any(|(key, value)| key == "decision" && value.as_str() == Some("block")) {
        if let Some((_, reason)) = output.iter_mut().find(|(key, _)| key == "reason") {
            if let Some(message) = reason.as_str() {
                if !message.starts_with(PROCESS_BLOCK) { *reason = s(format!("{PROCESS_BLOCK}{message}")); }
            }
        }
    }
    output
}

fn emit(output: Vec<(String, Value)>, sync_note: &str) -> ! {
    let output = process_stop_prefix(output, PROCESS_STOP.load(Ordering::Relaxed));
    let mut output = match STOP_STREAK.get() {
        Some((active, file)) => cap_stop_streak(output, *active, file),
        None => output,
    };
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
        && judgment.get("mode").and_then(Value::as_str) == Some("enforce")
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
    let message = last_assistant_message_from(event);
    worker_question(&message)
        || message.lines().any(|line| line.contains("진입점:"))
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
    !rows.is_empty() && running_dispatch(project_dir)
}

fn dispatch_wait_stop_allowed(hook_main: bool, rows: &[Value], project_dir: &Path) -> bool {
    hook_main && dispatch_wait_allowed(rows, project_dir)
}

fn session_dispatch_wait_stop_allowed(hook_main: bool, rows: &[Value], projects: &[(String, String)], cwd: &str) -> bool {
    if !hook_main || rows.is_empty() { return false; }
    // A process-repair row may be confirmed while the original work is still open.
    // In that mixed state the original row must reach the completion judgment.
    if rows.iter().any(|row| matches!(row.get("H").and_then(Value::as_str), Some("확인" | "완료"))) {
        return false;
    }
    rows.iter().filter(|row| !matches!(row.get("H").and_then(Value::as_str), Some("확인" | "완료")))
        .try_fold(false, |_, row| {
            let id = row.get("id").map(Value::py_str)?;
            let project = projects.iter().find(|(owned_id, _)| *owned_id == id)
                .map(|(_, path)| path.as_str()).unwrap_or(cwd);
            dispatch_wait_stop_allowed(hook_main, std::slice::from_ref(row), Path::new(project)).then_some(true)
        }).unwrap_or(false)
}

fn user_action_reason(row: &Value) -> bool {
    let d = row.get("D").and_then(Value::as_str).unwrap_or("").trim();
    d.starts_with("[🏷]") && d.contains("사용자")
        && (d.contains("대기") || d.contains("확인 중") || d.contains("검증 중단"))
}

fn user_action_wait_rows(pending: &[Value]) -> Option<Value> {
    if pending.is_empty() || !pending.iter().all(user_action_reason) {
        return None;
    }
    Some(Value::Arr(pending.iter().map(|row| Value::Obj(vec![
        ("id".into(), row.get("id").cloned().unwrap_or(Value::Null)),
        ("H".into(), row.get("H").cloned().unwrap_or(Value::Null)),
        ("D".into(), row.get("D").cloned().unwrap_or(Value::Null)),
    ])).collect()))
}

fn completion_user_action_wait_rows(rows: &[Value], pending: &[Value], completion_error: Option<&str>) -> Option<Value> {
    let mut waiting = pending.to_vec();
    if let Some(error) = completion_error {
        waiting.extend(rows.iter().filter(|row| {
            row.get("id").is_some_and(|id| error.starts_with(&format!("#{} ", id.py_str())))
                && !pending.iter().any(|pending_row| pending_row.get("id") == row.get("id"))
        }).cloned());
    }
    user_action_wait_rows(&waiting)
}

fn completion_error_detail(error: &str) -> String {
    let Some(id) = error.split_whitespace().next().filter(|part| part.starts_with('#')) else {
        return error.to_string();
    };
    let evidence = if error.contains("flow-wait") { "flow-wait 결과" }
        else if error.contains("확인") || error.contains("confirmation") || error.contains("evidence") { "확인 블록" }
        else { "완료 증거" };
    format!("막힌 행 {id}, 빠진 증거: {evidence}. {error}")
}

fn repeated_user_action_wait(previous: &Value, waiting: &Value) -> bool {
    previous.get("wait_rows").is_some_and(|rows| rows.py_eq(waiting))
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
    let step = if has_stage(&["진단"]) {
        "진단 행은 검증 실패(`$I`)를 같은 대행자 세션에 `dispatch-agent --role agent --continue`로 전달하세요(새 대행자·개선자·분석 검증자 금지). 재제출·재배포 뒤 검증자도 `--continue`로 멈춘 단계부터 이어요.".to_string()
    } else if has_stage(&["초안"]) {
        "`초안` 행의 코드 변경은 파일 수와 상관없이 `dispatch-agent` 대행자에게 맡기고, 규칙·Skill 문서 행만 메인이 직접 처리하세요(`manage-project` `references/plan.md`).".to_string()
    } else if has_stage(&["작업", "교정"]) {
        "구현자 결과를 기다리세요. 끝난 구현자의 행이 `검증`이 아니면 같은 행으로 다음 교정을 맡기세요.".to_string()
    } else {
        format!(
            "모든 행이 `검증`이에요. 배포 뒤 검증자를 흐름에 한 번 부르고(흐름 세션이 있으면 `--continue`, 새 검증자 금지), 통과했으면 행마다 `{CMD} task-confirm <id> --project {project_dir} --summary \"[표시 기호] <입력 또는 이전 단계> ⇒ <구현 결과>\" --check \"<목적>\" --detail \"<실제값 한 줄>\" --evidence <파일: 목표 줄마다 \"N. <근거>\">`로 확인을 기록한 뒤 \
`{CMD} flow-wait --project {project_dir}`를 실행하세요. `.ai/changes`는 직접 열거나 쓰지 않고, 기록 확인은 `{CMD} task-show <id> --project {project_dir} --section 확인`으로 하세요."
        )
    };
    format!(
        "종료 차단: Table26 {stages} (작업 폴더 {project_dir}). {step} \
사용자 입력(로그인·결제·비가역 외부 전송·OS 앱 조작)만 남았으면 각 미완 행의 `$D`에 `[🏷]`와 `사용자`, `대기`·`확인 중`·`검증 중단` 중 하나를 포함한 사유를 기록하세요. 같은 대기 행 상태의 두 번째 Stop은 허용돼요. `$H`는 직접 바꾸지 않고, 사용자가 Grist에서 `완료`로 처리하면 종료돼요."
    )
}

fn pending_reason_with_error(reason: String, completion_error: Option<&str>) -> String {
    match completion_error {
        Some(error) => format!("{}. {reason}", completion_error_detail(error)),
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

fn needs_j2(has_rows: bool, agent_path: &str, worker_row: Option<&str>, cli_cwd: Option<&str>, cli_row: Option<&str>) -> bool {
    has_rows && agent_path.is_empty() && worker_row.is_none() && cli_cwd.is_none() && cli_row.is_none()
}

fn should_run_j2(rows: &[Value], pending: &[Value], uninitialized: &str, completion_error: Option<&str>, agent_path: &str, worker_row: Option<&str>, cli_cwd: Option<&str>, cli_row: Option<&str>) -> bool {
    needs_j2(rows.iter().any(|row| !user_done(row)), agent_path, worker_row, cli_cwd, cli_row)
        && pending.is_empty() && uninitialized.is_empty() && completion_error.is_none()
}

fn last_assistant_message_from(event: &Value) -> String {
    if let Some(msg) = event.get("last_assistant_message").filter(|v| v.truthy()).and_then(Value::as_str) {
        if !msg.trim().is_empty() {
            return msg.to_string();
        }
    }
    let Some(path) = event.get("transcript_path").filter(|v| v.truthy()).and_then(Value::as_str) else {
        return String::new();
    };
    let Ok(mut text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    if text.lines().rev().filter_map(|line| json::parse(line).ok())
        .find(|entry| entry.get("type").and_then(Value::as_str) == Some("PLANNER_RESPONSE"))
        .is_some_and(|entry| entry.get("truncated_fields").is_some()) {
        let full = Path::new(path).with_file_name("transcript_full.jsonl");
        if let Ok(untruncated) = std::fs::read_to_string(full) { text = untruncated; }
    }
    text.lines()
        .rev()
        .filter_map(|line| json::parse(line).ok())
        .find_map(|entry| match entry.get("type").and_then(Value::as_str)? {
            "response_item" if entry.get("payload").and_then(|p| p.get("role")).and_then(Value::as_str) == Some("assistant") => {
                let Value::Arr(parts) = entry.get("payload")?.get("content")? else { return None };
                let joined: Vec<&str> = parts.iter()
                    .filter(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect();
                (!joined.is_empty()).then(|| joined.join("\n"))
            }
            "assistant" => {
                let Some(Value::Arr(parts)) = entry.get("message").and_then(|m| m.get("content")) else {
                    return None;
                };
                let joined: Vec<&str> = parts
                    .iter()
                    .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect();
                (!joined.is_empty()).then(|| joined.join("\n"))
            }
            "PLANNER_RESPONSE" => {
                let content = entry.get("content").and_then(Value::as_str)?.trim();
                (!content.is_empty()).then(|| content.to_string())
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn main_asked_user(event: &Value) -> bool {
    let Some(path) = event.get("transcript_path").and_then(Value::as_str) else { return false };
    let Ok(text) = std::fs::read_to_string(path) else { return false };
    let mut awaiting = false;
    for line in text.lines() {
        let Ok(entry) = json::parse(line) else { continue };
        match entry.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                let Some(Value::Arr(parts)) = entry.get("message").and_then(|m| m.get("content")) else { continue };
                for part in parts {
                    if part.get("type").and_then(Value::as_str) == Some("tool_use") {
                        awaiting = part.get("name").and_then(Value::as_str) == Some("AskUserQuestion");
                    }
                }
            }
            Some("user") => {
                let tool_result_only = matches!(entry.get("message").and_then(|m| m.get("content")), Some(Value::Arr(parts))
                    if !parts.is_empty() && parts.iter().all(|part| part.get("type").and_then(Value::as_str) == Some("tool_result")));
                if !tool_result_only { awaiting = false; }
            }
            Some("PLANNER_RESPONSE") => {
                let calls = entry.get("tool_calls").or_else(|| entry.get("toolCalls"));
                if let Some(Value::Arr(calls)) = calls {
                    if !calls.is_empty() {
                        awaiting = calls.iter().any(|call| call.get("name").and_then(Value::as_str) == Some("ask_question"));
                    }
                }
            }
            Some("USER_INPUT") => awaiting = false,
            _ => {}
        }
    }
    awaiting
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

extern "C" { fn flock(fd: std::os::raw::c_int, operation: std::os::raw::c_int) -> std::os::raw::c_int; }

fn remove_expired(dir: &Path, max_age: Duration, keep: &str, include: impl Fn(&str) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == keep || !include(&name) { continue; }
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue; };
        if meta.file_type().is_symlink() || !meta.modified().ok().and_then(|time| SystemTime::now().duration_since(time).ok()).is_some_and(|age| age > max_age) { continue; }
        if meta.is_dir() { let _ = std::fs::remove_dir_all(&path); continue; }
        let Ok(file) = std::fs::OpenOptions::new().read(true).open(&path) else { continue; };
        if unsafe { flock(file.as_raw_fd(), 2 | 4) } != 0 { continue; } // LOCK_EX | LOCK_NB
        let _ = std::fs::remove_file(&path);
        unsafe { flock(file.as_raw_fd(), 8); }
    }
}

fn cleanup_expired_state(state: &Path, home: &Path, tmp: &Path, thread: &str) {
    let fortnight = Duration::from_secs(14 * 24 * 3600);
    let day = Duration::from_secs(24 * 3600);
    let current = sanitize(thread);
    remove_expired(&state.join("session_start"), fortnight, "", |name| {
        (name.ends_with(".json") || name.ends_with(".blocked") || name.ends_with(".defect")) && !name.starts_with(&format!("{current}."))
    });
    remove_expired(state, fortnight, "", |name| name.starts_with("j2-") && name.ends_with(".json") && name != format!("j2-{current}.json"));
    remove_expired(&state.join("session_rows"), fortnight, &format!("{current}.json"), |name| name.ends_with(".json"));
    remove_expired(&home.join(".local/state/main-edit-gate"), fortnight, &format!("images-{current}.json"), |_| true);
    remove_expired(tmp, day, "", |name| name.starts_with("dispatch-agent-") && name.ends_with(".lock") || name.starts_with("grist-") || name.starts_with("tc-"));
}

fn git_root(dir: &Path) -> Option<PathBuf> {
    let target = dir.ancestors().find(|p| p.exists()).unwrap_or(dir);
    let out = Command::new("git").arg("-C").arg(target).args(["rev-parse", "--show-toplevel"])
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

/// Blocks once per session: a concurrent session keeps growing the change list, so re-blocking on
/// every new file would keep this one open (261109 grist_mindmap edits from another session).
/// `~/ai` rules·Skills·hooks·tools are main-session edits that need no row (CLAUDE.md §2), so only product repos count.
/// This session's own edits come from its transcript instead (`own_changes`) and block until a row exists.
fn own_changes(transcript: &str) -> Vec<String> {
    const MUTATING: [&str; 9] = ["cp ", "mv ", "rm ", "sed -i", "tee ", "grist update", "grist add", "grist delete", ",'x'"];
    // `~/plan.md` is the Claude→Codex hand-off plan (memory claude-plans-codex-implements), not product data.
    const FREE: [&str; 7] = ["~/ai/", "/home/tree/ai/", "~/.claude/", "/home/tree/.claude/", "/tmp/", "~/plan.md", "/home/tree/plan.md"];
    let free = |text: &str| FREE.iter().any(|prefix| text.trim_start_matches(['"', '\'']).starts_with(prefix));
    let mut changed: Vec<String> = Vec::new();
    for entry in transcript.lines().filter_map(|line| json::parse(line).ok()) {
        if entry.get("type").and_then(Value::as_str) != Some("assistant") { continue }
        let Some(Value::Arr(parts)) = entry.get("message").and_then(|m| m.get("content")) else { continue };
        for part in parts.iter().filter(|part| part.get("type").and_then(Value::as_str) == Some("tool_use")) {
            let input = |key: &str| part.get("input").and_then(|i| i.get(key)).and_then(Value::as_str).unwrap_or("").to_string();
            let item = match part.get("name").and_then(Value::as_str) {
                Some("Edit" | "Write" | "NotebookEdit") => Some(display_dir(&input("file_path"))).filter(|path| !path.is_empty() && !free(path)),
                Some("Bash") => {
                    let command = input("command");
                    let body = bash_targets_text(&command);
                    // Only segments that run a mutating command name write targets; `ls ~/.codex/x 2>/dev/null` and `ls /mnt/h/x 2>&1` read.
                    let has_redir = |s: &str| [">&1", ">&2", "2>/dev/null", ">/dev/null"].iter().fold(s.to_string(), |s, fd| s.replace(fd, "")).contains('>');
                    let masked = bash_unquoted(&body).replace("2>&1", "    ").replace(">&1", "   ").replace(">&2", "   ");
                    let mut cd_free = entry.get("cwd").and_then(Value::as_str).is_some_and(|dir| free(&format!("{}/", dir.trim_end_matches('/'))));
                    let mut changed = false;
                    let mut start = 0;
                    for (pos, ch) in masked.char_indices().filter(|(_, ch)| matches!(ch, '\n' | ';' | '|' | '&')).chain(std::iter::once((body.len(), '\n'))) {
                        let segment = body[start..pos].trim();
                        let code = masked[start..pos].trim();
                        start = pos + ch.len_utf8();
                        if let Some(rest) = segment.strip_prefix("cd ") {
                            let dir = rest.split_whitespace().next().unwrap_or("").trim_matches(['"', '\'']);
                            cd_free = free(&format!("{}/", dir.trim_end_matches('/')))
                                || (cd_free && !dir.starts_with(['/', '~', '$']));
                            continue;
                        }
                        if !MUTATING.iter().any(|m| code.contains(m)) && !has_redir(code) { continue; }
                        let targets = segment.split_whitespace()
                            .filter(|word| (word.contains('/') || word.starts_with("Table")) && !word.ends_with("/dev/null"));
                        let target_free = |word: &str| {
                            let bare = word.trim_matches(['"', '\'', '(', ')', ';']);
                            let bare = match bare.split_once('=') {
                                Some((name, value)) if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') => value.trim_matches(['"', '\'']),
                                _ => bare,
                            };
                            free(bare) || (cd_free && !bare.starts_with(['/', '~', '$']) && !bare.starts_with("Table"))
                        };
                        if !targets.into_iter().all(target_free) { changed = true; break; }
                    }
                    changed.then(|| format!("$ {}", command.lines().next().unwrap_or("").chars().take(80).collect::<String>()))
                }
                _ => None,
            };
            if let Some(item) = item { if !changed.contains(&item) { changed.push(item); } }
        }
    }
    changed
}

/// A Bash command reduced to the text that names write targets: heredoc bodies are data, not paths,
/// `NAME=/path`·`set NAME /path` variables are expanded before relative paths are resolved
/// (261110 a `cd ~/ai/... && python3 - <<EOF` edit and `D=/tmp/...` files blocked as product changes).
fn bash_targets_text(command: &str) -> String {
    let mut lines = Vec::new();
    let mut delimiter: Option<String> = None;
    for line in command.lines() {
        if let Some(end) = &delimiter {
            if line.trim() == end { delimiter = None; }
            continue;
        }
        if let Some(pos) = line.find("<<") {
            let word = line[pos + 2..].trim_start_matches(['-', '~']).trim_start();
            let word: String = word.trim_start_matches(['\'', '"']).chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if !word.is_empty() { delimiter = Some(word); }
        }
        lines.push(line);
    }
    let mut body = lines.join("\n");
    let words: Vec<String> = body.split(|c: char| c.is_whitespace() || c == ';').map(str::to_string).collect();
    let mut vars: Vec<(String, String)> = Vec::new();
    for (i, word) in words.iter().enumerate() {
        if let Some((name, value)) = word.split_once('=') {
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                // `d=$(mktemp -d)` names a fresh temp dir, so its paths are scratch like `/tmp/`.
                let value = value.trim_matches(['"', '\'']);
                let value = if value.starts_with("$(mktemp") { "/tmp/mktemp" } else { value };
                vars.push((name.to_string(), value.to_string()));
            }
        } else if word == "set" {
            let rest: Vec<&String> = words[i + 1..].iter().filter(|w| !w.is_empty()).take(2).collect();
            if let [name, value] = rest[..] {
                if !name.starts_with('-') && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    vars.push((name.clone(), value.trim_matches(['"', '\'']).to_string()));
                }
            }
        }
    }
    for (name, value) in &vars {
        for pattern in [format!("${{{name}:?}}"), format!("${{{name}}}"), format!("${name}")] {
            body = body.replace(&pattern, value);
        }
    }
    // A `sed -i '<script>'` script is data, not paths (261112 a sed edit of `~/ai` code quoting `/mnt/h/x` blocked).
    let mut from = 0;
    while let Some(pos) = body[from..].find("sed -i") {
        let start = from + pos + "sed -i".len();
        let rest = &body[start..];
        let quote = start + rest.len() - rest.trim_start_matches([' ', 'e', '-']).len();
        from = match body[quote..].starts_with('\'').then(|| body[quote + 1..].find('\'')).flatten() {
            Some(end) => { body.replace_range(quote..quote + end + 2, "''"); quote + 2 }
            None => start,
        };
    }
    body
}

/// Keep byte offsets so command operators can be located without interpreting quoted data as code.
fn bash_unquoted(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut quote = None;
    let mut escaped = false;
    for ch in body.chars() {
        if escaped {
            out.extend(std::iter::repeat_n(' ', ch.len_utf8()));
            escaped = false;
        } else if ch == '\\' {
            out.push(' ');
            escaped = true;
        } else if quote == Some(ch) {
            out.push(' ');
            quote = None;
        } else if quote.is_some() {
            out.extend(std::iter::repeat_n(' ', ch.len_utf8()));
        } else if ch == '\'' || ch == '"' {
            out.push(' ');
            quote = Some(ch);
        } else {
            out.push(ch);
        }
    }
    out
}

fn own_change_reason(changed: &[String]) -> Option<String> {
    if changed.is_empty() { return None; }
    let shown = changed.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    Some(format!(
        "종료 차단: 이 세션이 Table26 행 없이 제품·데이터를 바꿨어요({shown}). 메인은 직접 고치고 자체 확인으로 끝내지 않아요. \
`manage-project` 1단계(request-record → task-add → task-goal)로 행을 만들고, 구현은 대행자, 확인은 검증자(`dispatch-agent --role verifier`)에게 맡기세요. \
행이 생길 때까지 계속 막아요."
    ))
}

fn unregistered_reason(thread: &str, changed: &[String]) -> Option<String> {
    let changed = changed.iter().filter(|file| !file.starts_with("~/ai/")).cloned().collect::<Vec<_>>();
    if changed.is_empty() { return None; }
    let marker = state_dir().join("session_start").join(format!("{}.blocked", sanitize(thread)));
    let key = changed.join("\n");
    if marker.exists() { return None; }
    let _ = std::fs::write(&marker, &key);
    let shown = changed.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
    let more = if changed.len() > 8 { format!(" 외 {}개", changed.len() - 8) } else { String::new() };
    Some(format!(
        "종료 차단: 이 세션에 등록된 Table26 행이 없는데 파일이 바뀌었어요({shown}{more}). \
이 세션이 바꾼 것이면 `manage-project` 1단계(request-record → task-add → task-goal)로 행을 만들고 검증하세요. \
다른 세션의 변경이면 그대로 종료하면 돼요. 같은 변경 목록으로는 다시 막지 않아요."
    ))
}

const TOOL_DEFECT_MARK: &str = "작업 도구 결함:";

/// A tool fix must reach the user as a defect, not only as a change log. Once per distinct change list, like above.
fn tool_defect_reason(thread: &str, changed: &[String], message: &str) -> Option<String> {
    let tools = changed.iter().filter(|file| file.starts_with("~/ai/scripts/") || file.starts_with("~/ai/codex/")).cloned().collect::<Vec<_>>();
    if tools.is_empty() { return None; }
    let marker = state_dir().join("session_start").join(format!("{}.defect", sanitize(thread)));
    let key = tools.join("\n");
    let prev = std::fs::read_to_string(&marker).ok();
    if message.contains(TOOL_DEFECT_MARK) {
        let _ = std::fs::write(&marker, &key);
        return None;
    }
    if prev.as_deref() == Some("*") || prev.as_deref() == Some(key.as_str()) { return None; }
    let _ = std::fs::write(&marker, "*");
    let shown = tools.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
    Some(format!(
        "종료 차단: 작업 도구가 바뀌었는데 최종 보고에 결함 보고가 없어요({shown}). \
이 세션이나 대행자가 고친 것이면 `{TOOL_DEFECT_MARK} <증상> → <수정>` 줄을 최종 보고에 넣으세요. \
다른 세션의 변경이면 그대로 종료하면 돼요. 같은 변경 목록으로는 다시 막지 않아요."
    ))
}

fn extract_user_prompt_text(raw: &str) -> Option<String> {
    if let Some((_, rest)) = raw.split_once("<USER_REQUEST>") {
        let body = rest.split_once("</USER_REQUEST>").map(|(b, _)| b).unwrap_or(rest).trim();
        return (!body.is_empty()).then(|| body.to_string());
    }
    let trimmed = raw.trim_start();
    (!trimmed.is_empty() && !trimmed.starts_with('<') && !trimmed.starts_with("Stop hook feedback"))
        .then(|| raw.to_string())
}

fn codex_user_prompt(entry: &Value) -> Option<String> {
    if entry.get("type").and_then(Value::as_str) != Some("response_item")
        || entry.get("payload").and_then(|p| p.get("role")).and_then(Value::as_str) != Some("user") { return None; }
    let Value::Arr(parts) = entry.get("payload")?.get("content")? else { return None; };
    parts.iter().find(|part| part.get("type").and_then(Value::as_str) == Some("input_text"))
        .and_then(|part| part.get("text")).and_then(Value::as_str).and_then(extract_user_prompt_text)
}

fn read_transcript_for_user(path: &str) -> Option<String> {
    let mut text = std::fs::read_to_string(path).ok()?;
    if text.lines().rev().filter_map(|line| json::parse(line).ok())
        .find(|entry| entry.get("type").and_then(Value::as_str) == Some("USER_INPUT"))
        .is_some_and(|entry| entry.get("truncated_fields").is_some()) {
        let full = Path::new(path).with_file_name("transcript_full.jsonl");
        if let Ok(untruncated) = std::fs::read_to_string(full) { text = untruncated; }
    }
    Some(text)
}

fn last_user_prompt_from(event: &Value) -> String {
    let Some(path) = event.get("transcript_path").and_then(Value::as_str) else { return String::new() };
    let Some(text) = read_transcript_for_user(path) else { return String::new() };
    text.lines().rev().filter_map(|line| json::parse(line).ok()).find_map(|entry| {
        if let Some(prompt) = codex_user_prompt(&entry) { return Some(prompt); }
        match entry.get("type").and_then(Value::as_str)? {
            "user" if !entry.get("isMeta").is_some_and(|v| v.truthy()) => {
                let raw = match entry.get("message").and_then(|m| m.get("content"))? {
                    Value::Str(text) => text.clone(),
                    Value::Arr(parts) => parts.iter()
                        .find(|part| part.get("type").and_then(Value::as_str) == Some("text"))
                        .and_then(|part| part.get("text")).and_then(Value::as_str)?.to_string(),
                    _ => return None,
                };
                extract_user_prompt_text(&raw)
            }
            "USER_INPUT" => entry.get("content").and_then(Value::as_str).and_then(extract_user_prompt_text),
            _ => None,
        }
    }).unwrap_or_default()
}

fn explanation_suppressed_by_user(event: &Value) -> bool {
    let Some(path) = event.get("transcript_path").and_then(Value::as_str) else { return false };
    let Some(text) = read_transcript_for_user(path) else { return false };
    let mut suppressed = false;
    for entry in text.lines().filter_map(|line| json::parse(line).ok()) {
        let raw = match entry.get("type").and_then(Value::as_str) {
            Some("USER_INPUT") => entry.get("content").and_then(Value::as_str),
            Some("user") => entry.get("message").and_then(|message| message.get("content")).and_then(Value::as_str),
            _ => None,
        };
        let Some(prompt) = raw.and_then(extract_user_prompt_text).or_else(|| codex_user_prompt(&entry)) else { continue };
        if ["자세히 설명해", "이유를 설명해", "원인을 설명해"].iter().any(|phrase| prompt.contains(phrase))
            && !prompt.contains("설명하지") { suppressed = false; }
        if ["설명하지", "설명 말", "설명 그만", "그딴걸 설명", "이유설명으로 끝", "문제가 뭔지는 나도"]
            .iter().any(|phrase| prompt.contains(phrase)) { suppressed = true; }
    }
    suppressed
}

fn suppressed_explanation_reason(message: &str) -> Option<&'static str> {
    let first = message.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
    let cause_first = ["원인은", "이유는", "왜냐하면", "제가 잘못", "제가 마지막", "제가 이미"]
        .iter().any(|phrase| first.contains(phrase));
    if cause_first || message.chars().count() > 450 {
        Some("사용자가 원인 설명을 중단하라고 했어요. 원인 반복을 빼고 실행한 수정·검증 결과만 짧게 보고하세요.")
    } else { None }
}

fn dismissed_fix_reason(thread: &str, prompt: &str, message: &str) -> Option<&'static str> {
    let correction = matches!(decision(thread, prompt).label, Some(Label::Improvement | Label::Feature | Label::Resolution));
    let response = unquoted(message);
    (correction && ["그럴 필요 없", "할 필요 없", "고칠 필요 없", "수정할 필요 없", "필요 없습니다", "불필요해요"]
        .iter().any(|term| response.contains(term)))
        .then_some("종료 차단: 사용자의 수정 요청을 필요 없다고 거절했어요. 요청한 수정을 실행하고 검증 결과를 보고하세요.")
}

fn last_turn_had_fix_action(event: &Value) -> bool {
    last_turn_fix_action(event).unwrap_or(false)
}

/// Whether the latest user turn edited something; None when the turn cannot be read from the transcript.
fn last_turn_fix_action(event: &Value) -> Option<bool> {
    let path = event.get("transcript_path").and_then(Value::as_str)?;
    let text = read_transcript_for_user(path)?;
    let mut after_user = false;
    let mut fixed = false;
    for entry in text.lines().filter_map(|line| json::parse(line).ok()) {
        if codex_user_prompt(&entry).is_some() {
            after_user = true;
            fixed = false;
            continue;
        }
        if after_user && entry.get("type").and_then(Value::as_str) == Some("response_item")
            && entry.get("payload").and_then(|p| p.get("type")).and_then(Value::as_str) == Some("custom_tool_call") {
            let input = entry.get("payload").and_then(|p| p.get("input")).map(json::dumps).unwrap_or_default();
            if ["apply_patch", "agent-rules", "sed -i", "--role improver"].iter().any(|term| input.contains(term)) { fixed = true; }
            continue;
        }
        match entry.get("type").and_then(Value::as_str) {
            Some("user") if !entry.get("isMeta").is_some_and(|v| v.truthy()) => {
                let raw = match entry.get("message").and_then(|m| m.get("content")) {
                    Some(Value::Str(t)) => Some(t.as_str()),
                    Some(Value::Arr(parts)) => parts.iter()
                        .find(|p| p.get("type").and_then(Value::as_str) == Some("text"))
                        .and_then(|p| p.get("text")).and_then(Value::as_str),
                    _ => None,
                };
                if raw.and_then(extract_user_prompt_text).is_some() {
                    after_user = true;
                    fixed = false;
                }
            }
            Some("USER_INPUT") => {
                if entry.get("content").and_then(Value::as_str).and_then(extract_user_prompt_text).is_some() {
                    after_user = true;
                    fixed = false;
                }
            }
            Some("assistant") if after_user => {
                if let Some(Value::Arr(parts)) = entry.get("message").and_then(|m| m.get("content")) {
                    for part in parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("tool_use")) {
                        match part.get("name").and_then(Value::as_str) {
                            Some("Edit" | "Write" | "MultiEdit" | "NotebookEdit") => fixed = true,
                            Some("Bash") => {
                                let cmd = part.get("input").and_then(|i| i.get("command")).and_then(Value::as_str).unwrap_or("");
                                if ["dispatch-agent", "agent-rules", "sed -i"].iter().any(|k| cmd.contains(k)) { fixed = true; }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some("PLANNER_RESPONSE") if after_user => {
                if let Some(Value::Arr(calls)) = entry.get("tool_calls") {
                    for call in calls {
                        let name = call.get("name").or_else(|| call.get("tool_name")).and_then(Value::as_str).unwrap_or("");
                        if matches!(name, "edit_file" | "write_file" | "replace_file_content" | "multi_replace_file_content") {
                            fixed = true;
                        } else if name == "run_command" {
                            let cmd = json::dumps(call);
                            if ["dispatch-agent", "agent-rules", "sed -i"].iter().any(|k| cmd.contains(k)) { fixed = true; }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    after_user.then_some(fixed)
}

/// A hook execution or focused Rust test in the latest user turn verifies a process edit.
/// An improver handoff owns this verification and also counts as evidence.
fn last_turn_hook_verification_action(event: &Value) -> Option<bool> {
    let path = event.get("transcript_path").and_then(Value::as_str)?;
    let transcript = read_transcript_for_user(path)?;
    let mut after_user = false;
    let mut verified = false;
    let command_verifies = |command: &str| {
        ["bin/main-edit-gate", "bin/task-complete", "cargo test", "--role improver"]
            .iter().any(|needle| command.contains(needle))
    };
    for entry in transcript.lines().filter_map(|line| json::parse(line).ok()) {
        if codex_user_prompt(&entry).is_some()
            || (entry.get("type").and_then(Value::as_str) == Some("USER_INPUT")
                && entry.get("content").and_then(Value::as_str).and_then(extract_user_prompt_text).is_some())
            || (entry.get("type").and_then(Value::as_str) == Some("user")
                && !matches!(entry.get("message").and_then(|m| m.get("content")), Some(Value::Arr(parts))
                    if !parts.is_empty() && parts.iter().all(|part| part.get("type").and_then(Value::as_str) == Some("tool_result")))) {
            after_user = true;
            verified = false;
            continue;
        }
        if !after_user { continue; }
        match entry.get("type").and_then(Value::as_str) {
            Some("response_item") if entry.get("payload").and_then(|p| p.get("type")).and_then(Value::as_str) == Some("custom_tool_call") => {
                let input = entry.get("payload").and_then(|p| p.get("input")).map(json::dumps).unwrap_or_default();
                if command_verifies(&input) { verified = true; }
            }
            Some("assistant") => {
                if let Some(Value::Arr(parts)) = entry.get("message").and_then(|m| m.get("content")) {
                    for part in parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("tool_use")) {
                        if part.get("name").and_then(Value::as_str) == Some("Bash") {
                            let command = part.get("input").and_then(|input| input.get("command")).and_then(Value::as_str).unwrap_or("");
                            if command_verifies(command) { verified = true; }
                        }
                    }
                }
            }
            Some("PLANNER_RESPONSE") => {
                if let Some(Value::Arr(calls)) = entry.get("tool_calls") {
                    for call in calls {
                        let name = call.get("name").or_else(|| call.get("tool_name")).and_then(Value::as_str);
                        if name == Some("run_command") && command_verifies(&json::dumps(call)) { verified = true; }
                    }
                }
            }
            _ => {}
        }
    }
    after_user.then_some(verified)
}

fn session_had_any_fix_action(event: &Value) -> bool {
    let Some(path) = event.get("transcript_path").and_then(Value::as_str) else { return true };
    let Some(text) = read_transcript_for_user(path) else { return true };
    const WRITE_CMDS: [&str; 11] = [
        "dispatch-agent", "agent-rules", "sed -i", "tee ", "cp ", "mv ", "rm ",
        "cargo ", "rustc", "python", "patch",
    ];
    let has_redir = |s: &str| [">&1", ">&2", "2>/dev/null", ">/dev/null"].iter().fold(s.to_string(), |s, fd| s.replace(fd, "")).contains('>');
    for entry in text.lines().filter_map(|line| json::parse(line).ok()) {
        match entry.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                if let Some(Value::Arr(parts)) = entry.get("message").and_then(|m| m.get("content")) {
                    for part in parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("tool_use")) {
                        match part.get("name").and_then(Value::as_str) {
                            Some("Edit" | "Write" | "MultiEdit" | "NotebookEdit") => return true,
                            Some("Bash") => {
                                let cmd = part.get("input").and_then(|i| i.get("command")).and_then(Value::as_str).unwrap_or("");
                                if WRITE_CMDS.iter().any(|k| cmd.contains(k)) || has_redir(cmd) { return true; }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some("PLANNER_RESPONSE") => {
                if let Some(Value::Arr(calls)) = entry.get("tool_calls") {
                    for call in calls {
                        let name = call.get("name").or_else(|| call.get("tool_name")).and_then(Value::as_str).unwrap_or("");
                        if matches!(name, "edit_file" | "write_file" | "replace_file_content" | "multi_replace_file_content") {
                            return true;
                        } else if name == "run_command" {
                            let cmd = call.get("args").or_else(|| call.get("arguments"))
                                .and_then(|a| a.get("CommandLine").or_else(|| a.get("command")))
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            if WRITE_CMDS.iter().any(|k| cmd.contains(k)) || has_redir(cmd) { return true; }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn status_question_stop_allowed(thread: &str, prompt: &str, work_in_progress: bool) -> bool {
    status_question_stop_allowed_at(&state_dir(), thread, prompt, work_in_progress)
}

fn status_question_stop_allowed_at(state: &Path, thread: &str, prompt: &str, work_in_progress: bool) -> bool {
    if !work_in_progress || prompt_decision::decision_at(state, thread, prompt).label != Some(Label::Question) { return false; }
    let marker = state.join("session_start").join(format!("{}.statusq", sanitize(thread)));
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(prompt) { return false; }
    if let Some(parent) = marker.parent() { let _ = std::fs::create_dir_all(parent); }
    let _ = std::fs::write(&marker, prompt);
    true
}

const FIX_OFFERS: [&str; 10] = ["원하시면", "드릴까요", "할까요", "드릴게요", "드리면", "고칠 수 있어요", "수정할 수 있어요", "만들 수 있어요", "정해 주", "알려 주"];

fn offer_or_talk_only_reason(thread: &str, prompt: &str, message: &str, had_fix: bool, process: bool) -> Option<String> {
    let fix_requested = matches!(decision(thread, prompt).label, Some(Label::Improvement | Label::Feature | Label::Resolution));
    if !fix_requested { return None; }
    let offered = FIX_OFFERS.iter().any(|offer| message.contains(offer));
    let talk_only = !had_fix && !message.contains("실행한 수정");
    if !offered && !talk_only { return None; }
    let marker = state_dir().join("session_start").join(format!("{}.offer", sanitize(thread)));
    if !process && std::fs::read_to_string(&marker).ok().as_deref() == Some(prompt) { return None; }
    if let Some(parent) = marker.parent() { let _ = std::fs::create_dir_all(parent); }
    let _ = std::fs::write(&marker, prompt);
    let reason = "종료 차단: 오동작·무행동 지적은 설명·인정이 아니라 수정 요청인데 실제 코드·규칙·hook 수정 없이 제안하거나 응답만 하고 끝내려 해요. \
원인 규칙·Skill·hook·코드를 지금 고치고 `실행한 수정 → 검증 결과 → 남은 경계`로 보고하세요. \
결과가 정말 갈리는 결정만 남았으면 AskUserQuestion으로 물으세요.";
    Some(if process { format!("{PROCESS_BLOCK}{reason}") } else { format!("{reason} 같은 요청으로는 다시 막지 않아요.") })
}

/// Quoted text ("…", `…`) cites a phrase instead of saying it.
fn unquoted(text: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    for ch in text.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' || ch == '`' || ch == '“' => quote = Some(if ch == '“' { '”' } else { ch }),
            None => out.push(ch),
        }
    }
    out
}

const UNVERIFIED: [&str; 8] = ["확인하지 못", "확인 못 ", "검증하지 못", "검증 못 ", "단위 테스트로만", "유닛 테스트로만", "실행해 보지 않", "돌려 보지 않"];
const USER_ONLY: &str = "[🏷]";

/// A report that admits an unchecked part of its own change must check it before ending; only a part that
/// needs the user (login·payment·external send·OS app, marked `[🏷]` on that line) may stay unchecked.
fn unverified_report_reason(message: &str) -> Option<String> {
    let lines = message.lines()
        .filter(|line| { let bare = unquoted(line); UNVERIFIED.iter().any(|word| bare.contains(word)) && !line.contains(USER_ONLY) })
        .map(|line| line.trim().chars().take(80).collect::<String>())
        .collect::<Vec<_>>();
    if lines.is_empty() { return None; }
    Some(format!(
        "종료 차단: 보고에 확인하지 않은 부분이 있어요({}). 완료로 보고하지 말고 지금 실제 환경에서 확인하세요. \
사용자 몫(로그인·결제·외부 전송·OS 앱)이라 못 하는 것만 그 줄에 `{USER_ONLY}`를 붙여 남기세요.",
        lines.join(" / ")
    ))
}

const APPLY_ASKS: [&str; 7] = ["불러와 주", "Publish해 주", "Publish를 눌러 주", "게시해 주", "배포해 주", "적용해 주", "import해 주"];

/// Asking the user to apply (import·publish·deploy) while a row's cause is still open makes them apply twice
/// (261110 n8n: TS2O published with the Contest Korea year bug already diagnosed, then 261110-3 again).
fn early_apply_reason(rows: &[Value], message: &str) -> Option<String> {
    let said = message.lines().map(unquoted).collect::<Vec<_>>().join("\n");
    if !APPLY_ASKS.iter().any(|ask| said.contains(ask)) { return None; }
    let open = rows.iter().filter(|row| matches!(row.get("H").and_then(Value::as_str), Some("진단" | "작업" | "교정")))
        .filter_map(|row| row.get("id").map(Value::py_str)).collect::<Vec<_>>();
    if open.is_empty() { return None; }
    Some(format!(
        "종료 차단: 원인이 열린 행(#{})이 있는데 사용자에게 적용(불러오기·Publish·배포)을 요청해요. \
발견한 결함은 데이터만 고치지 말고 원인 노드·코드 수정을 먼저 맡기고, 열린 행이 모두 `검증`이 된 뒤 적용을 한 번만 요청하세요.",
        open.join(", #")
    ))
}

fn unverified_fix_report_reason(rows: &[Value], message: &str, process: bool) -> Option<String> {
    let said = message.lines().map(unquoted).collect::<Vec<_>>().join("\n");
    if process && !said.contains("복귀 #") { return None; }
    if !said.contains("실행한 수정") { return None; }
    let open = rows.iter().filter(|row| {
        matches!(row.get("H").and_then(Value::as_str), Some("진단" | "작업" | "교정"))
            && !row.get("D").and_then(Value::as_str).unwrap_or("").trim().starts_with("[⏸]")
    }).filter_map(|row| row.get("id").map(Value::py_str)).collect::<Vec<_>>();
    (!open.is_empty()).then(|| format!("종료 차단: 런타임 검증이 통과하지 않은 행(#{})이 있는데 완료된 수정처럼 `실행한 수정`으로 보고해요. 검증을 통과시키거나 미검증·검증 실패 상태로만 보고하세요.", open.join(", #")))
}

fn main() {
    let session_start = std::env::args().any(|a| a == "--session-start");
    let session_touch = std::env::args().any(|a| a == "--session-touch");
    if session_start || session_touch {
        let event = json::parse(&read_stdin_bounded()).unwrap_or(Value::Obj(vec![]));
        if let (Some(cwd), Some(session_id)) = (event.get("cwd").and_then(Value::as_str), event.get("session_id").and_then(Value::as_str)) {
            if !session_id.is_empty() && std::env::var("CODEX_THREAD_ID").ok().filter(|id| !id.is_empty()).is_none() {
                if let Err(error) = record_claude_session(Path::new(cwd), session_id) {
                    eprintln!("task-complete: Claude session pointer: {error}");
                    std::process::exit(1);
                }
            }
        }
        if session_start || (session_touch && !session_start_path(&hook_session_id(&event).unwrap_or_else(|| "unknown".into())).exists()) {
            let thread = hook_session_id(&event).unwrap_or_else(|| "unknown".into());
            if session_start { cleanup_expired_state(&state_dir(), &home_dir(), &std::env::temp_dir(), &thread); }
            record_session_start(&event, &thread);
        }
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
    let _ = STOP_STREAK.set((
        event.get("stop_hook_active") == Some(&Value::Bool(true)),
        s_dir.join(format!("streak-{}", sanitize(thread))),
    ));

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

    if std::env::var("T26_ROLE").ok().as_deref() == Some("improver") {
        match improver_stop(Path::new(cwd), std::env::var("T26_ROW").ok().as_deref()) {
            Ok(()) => emit(vec![], &sync_note),
            Err(reason) => emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note),
        }
    }
    let cli_row = parse_cli_row();
    let verifier_role = std::env::var("T26_ROLE").ok().as_deref() == Some("verifier");
    let verifier_row = grist::gate::verifier_row().map(|row| row.to_string());
    if verifier_role && verifier_row.is_none() {
        println!("{{\"decision\": \"block\", \"reason\": \"검증자 T26_ROW가 없거나 숫자가 아니에요.\"}}");
        std::process::exit(2);
    }
    let worker_row = grist::gate::worker_row().map(|row| row.to_string()).or_else(|| verifier_row.clone());
    if worker_row.is_some() && cli_row.is_some() {
        println!("{{\"decision\": \"block\", \"reason\": \"task-complete --row is run by the orchestrator, not a dispatch-agent worker or verifier\"}}");
        std::process::exit(2);
    }
    let (code, out, err, allowed) = if let Some(row_id) = cli_row.as_deref() {
        let (c, o, e) = grist(&["gate", "--cwd", cwd, "--row", row_id]);
        (c, o, e, MAIN_ALLOWED)
    } else if let Some(row_id) = worker_row.as_deref() {
        let (c, o, e) = grist(&["gate", "--cwd", cwd, "--row", row_id]);
        (c, o, e, if verifier_row.is_some() { VERIFIER_ALLOWED } else { AGENT_ALLOWED })
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
    let session_id = hook_session_id(&event).unwrap_or_else(|| thread.to_string());
    let session_owned = if hook_main { session_row_ids(&session_id) } else { vec![] };
    let projects = if hook_main { session_project_map(&session_id) } else { vec![] };
    let user_prompt = last_user_prompt_from(&event);
    let turn = if hook_main { Some(decision(&session_id, &user_prompt)) } else { None };
    let user_wait = main_asked_user(&event) || last_assistant_message_from(&event).contains(USER_ONLY);
    let turn_is_process = turn.as_ref().is_some_and(|turn| turn.process);
    let process_turn = turn_is_process && !user_wait;
    PROCESS_STOP.store(process_turn, Ordering::Relaxed);
    let owned = process_owned_rows(session_owned, &projects, turn_is_process);
    if process_turn {
        let prior = turn.as_ref().map(|turn| turn.prior_rows.as_slice()).unwrap_or_default();
        let started = turn.as_ref().map(|turn| turn.started).unwrap_or_default();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs()).unwrap_or_default();
        let unfinished = unfinished_process_rows(&owned, &gate_rows);
        if let Err(error) = process_overdue_notes(&unfinished, prior, started, now) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(error))], &sync_note);
        }
    }
    if hook_main {
        if let Some(reason) = dismissed_fix_reason(&session_id, &user_prompt, &last_assistant_message_from(&event)) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
    }
    if hook_main && explanation_suppressed_by_user(&event) {
        if let Some(reason) = suppressed_explanation_reason(&last_assistant_message_from(&event)) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
    }
    if process_turn && last_turn_fix_action(&event) != Some(true) {
        emit(vec![("decision".into(), s("block")), ("reason".into(), s(format!("{PROCESS_BLOCK}과정 수정 요청 뒤 이번 턴의 실제 수정(규칙·Skill·hook·스크립트 편집 또는 개선자 호출)이 확인되지 않았어요. 조회·테스트·이전 확인 행만으로 완료하지 말고 현재 지적을 고치세요.")))], &sync_note);
    }
    if process_turn && last_turn_hook_verification_action(&event) != Some(true) {
        emit(vec![("decision".into(), s("block")), ("reason".into(), s(format!("{PROCESS_BLOCK}훅 검증 없음. 이번 턴에 main-edit-gate·task-complete 훅 또는 cargo test를 실행해 수정 결과를 확인하세요.")))], &sync_note);
    }
    if process_turn && owned.is_empty() {
        emit(vec![("decision".into(), s("block")), ("reason".into(), s(format!("{PROCESS_BLOCK}과정 수정 행과 실제 훅 검증이 확인되지 않았어요. 작업관리기 행을 기록하고 개선자를 호출하세요.")))], &sync_note);
    }
    let initial_rows = if hook_main {
        match rows_for_session_projects(&owned, &projects, cwd, gate_rows) {
            Ok(rows) => rows,
            Err(error) => emit(vec![("decision".into(), s("block")), ("reason".into(), s(error))], &sync_note),
        }
    } else { gate_rows };
    if process_turn && process_rows_only_prior_closed(&initial_rows, &owned, &turn.as_ref().map(|t| t.prior_rows.clone()).unwrap_or_default()) {
        emit(vec![("decision".into(), s("block")), ("reason".into(), s(format!("{PROCESS_BLOCK}이번 지적 전에 이미 확인·완료된 행만 있어요. 이전 증거는 이번 지적의 처리 증거가 아니니 현재 지적을 작업관리기 새 행으로 기록하고 고치세요.")))], &sync_note);
    }
    if hook_main {
        if let Some(reason) = early_apply_reason(&initial_rows, &last_assistant_message_from(&event)) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
        if let Some(reason) = unverified_report_reason(&last_assistant_message_from(&event)) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
        if let Some(reason) = unverified_fix_report_reason(&initial_rows, &last_assistant_message_from(&event), turn_is_process) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
    }

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
    if let Some(row_id) = verifier_row.as_deref() {
        let row = initial_rows.iter().find(|row| row.get("id").is_some_and(|id| id.py_str() == row_id))
            .cloned().unwrap_or(Value::Obj(vec![]));
        if let Err(reason) = verifier_execution(Path::new(project_dir), row_id.parse().unwrap_or_default(), &row) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
        let analysis_report = std::env::var("T26_ANALYSIS_REPORT").ok();
        if let Some(reason) = analysis_report.as_deref().and_then(analysis_stop_reason) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
        let recorded = analysis_report.is_some() || row_id.parse::<i64>().ok().is_some_and(|id| verifier_recorded(Path::new(project_dir), id));
        if !recorded {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(format!("검증자 #{row_id}: task-hidden 또는 task-step 검증 기록을 남긴 뒤 종료하세요.")))], &sync_note);
        }
    }
    // The orchestrator alone runs the completion judgment; a t26 subagent or dispatch-agent worker ends at 검증.
    let orchestrator = agent_path.is_empty() && worker_row.is_none();
    let completion_checked = orchestrator && uninitialized.is_empty();
    if !process_turn && session_dispatch_wait_stop_allowed(hook_main, &initial_rows, &projects, project_dir) {
        emit(vec![], &sync_note);
    }
    let mut completion_error = if completion_checked {
        check_session_rows_with(&initial_rows, &projects, project_dir, complete_row)
            .and_then(|()| check_session_requests(&initial_rows, &projects, project_dir)).err()
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
                    rows = if hook_main {
                        match rows_for_session_projects(&owned, &projects, cwd, items.clone()) {
                            Ok(rows) => rows,
                            Err(error) => {
                                completion_error = Some(error);
                                rows.clone()
                            }
                        }
                    } else { items.clone() };
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
    if process_turn {
        for row in &rows {
            let id = row.get("id").and_then(Value::as_i64).unwrap_or_default();
            let confirmed = matches!(row.get("H").and_then(Value::as_str), Some("확인" | "완료"));
            if !confirmed {
                emit(vec![("decision".into(), s("block")), ("reason".into(), s(format!("#{id} 과정 수정 검증이 미완료예요.")))], &sync_note);
            }
        }
    }
    if orchestrator && uninitialized.is_empty() && !rows.is_empty() && rows_completed && pending.is_empty() {
        let mut done = completed_ids(project_dir);
        for (_, project) in &projects {
            if project != project_dir {
                for id in completed_ids(project) {
                    if !done.contains(&id) { done.push(id); }
                }
            }
        }
        if !complete_work_valid(&rows, &rows, &done) {
            completion_error = Some("COMPLETE_WORK: completed list omitted a task".into());
        }
    }

    // J2 only decides a stop the rows would allow, so a blocked stop skips the Jev call.
    let judgment_result = if should_run_j2(&rows, &pending, &uninitialized, completion_error.as_deref(), &agent_path, worker_row.as_deref(), cli_cwd.as_deref(), cli_row.as_deref()) {
        let last_msg = last_assistant_message_from(&event);
        jev_stop_cached(&s_dir.join(format!("j2-{}.json", sanitize(thread))), &last_msg, &rows)
    } else {
        Ok(Value::Obj(vec![]))
    };
    let j2_error = judgment_result.as_ref().err().map(|error| format!("J2 종료 판정 실패: {error}"));
    let judgment = judgment_result.unwrap_or(Value::Obj(vec![]));

    let asks_user = judgment.get("category").and_then(Value::as_str) == Some("사용자 질문")
        && judged(&judgment, "category", &["pass"]);
    let disallowed_question = asks_user && judged(&judgment, "allowed_action", &["fail"]);
    let stop_reason = j2_error.unwrap_or_else(|| early_stop(&judgment).to_string());
    let jev_block = !stop_reason.is_empty() || disallowed_question;

    if hook_main && owned.is_empty() && uninitialized.is_empty() {
        let transcript = event.get("transcript_path").and_then(Value::as_str).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        if let Some(reason) = own_change_reason(&own_changes(&transcript)) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
        if session_had_any_fix_action(&event) {
            if let Some(reason) = unregistered_reason(&session_id, &unregistered_changes(&session_id)) {
                emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
            }
        }
    }

    let active_work = !pending.is_empty() || running_dispatch(Path::new(project_dir))
        || projects.iter().any(|(_, path)| running_dispatch(Path::new(path)));
    if hook_main && !process_turn && uninitialized.is_empty() && active_work
        && (main_asked_user(&event) || status_question_stop_allowed(&session_id, &user_prompt, active_work))
    {
        let _ = std::fs::remove_file(&state_file);
        emit(vec![], &sync_note);
    }

    if hook_main && uninitialized.is_empty() && pending.is_empty() && !main_asked_user(&event) {
        let last_msg = last_assistant_message_from(&event);
        if let Some(reason) = offer_or_talk_only_reason(
            &session_id,
            &last_user_prompt_from(&event),
            &last_msg,
            last_turn_had_fix_action(&event),
            turn_is_process,
        ) {
            emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
        }
        if session_had_any_fix_action(&event) {
            if let Some(reason) = tool_defect_reason(&session_id, &unregistered_changes(&session_id), &last_msg) {
                emit(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], &sync_note);
            }
        }
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

    let wait_rows = if hook_main {
        completion_user_action_wait_rows(&rows, &pending, completion_error.as_deref())
    } else { None };
    if let Some(ref waiting) = wait_rows {
        if repeated_user_action_wait(&state, waiting) {
            emit(vec![], &sync_note);
        }
    }

    let blocks = if state.get("stages").map(|st| st.py_eq(&stages)).unwrap_or(false) {
        state.get("blocks").and_then(Value::as_i64).unwrap_or(0)
    } else {
        0
    };

    let project_dir = status
        .get("project_dir")
        .filter(|v| v.truthy())
        .and_then(Value::as_str);

    let reason = if (worker_row.is_some() || !agent_path.is_empty()) && !pending.is_empty() {
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
        format!("종료 차단: Table26 {stages_text}. {stop_reason} {q}{}", completion_error.as_deref().map(completion_error_detail).unwrap_or_default())
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
    if let Some(waiting) = wait_rows {
        obj_set(&mut new_state, "wait_rows", waiting);
    }
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
    #[test]
    fn request_check_groups_split_rows_once_and_skips_user_done() {
        let row = |id: i64, stage: &str| super::Value::Obj(vec![
            ("id".into(), super::Value::Num(id.to_string())),
            ("H".into(), super::s(stage)),
        ]);
        let rows = vec![row(7, "검증"), row(8, "검증"), row(9, "완료"), row(10, "검증")];
        let projects = vec![("7".into(), "/p".into()), ("8".into(), "/p".into()),
            ("9".into(), "/p".into()), ("10".into(), "/q".into())];
        let mut seen = Vec::new();
        super::check_session_requests_with(&rows, &projects, "/default", |path, ids| {
            seen.push((path.to_string_lossy().into_owned(), ids.to_vec()));
            Ok(())
        }).unwrap();
        assert_eq!(seen, vec![("/p".into(), vec![7, 8]), ("/q".into(), vec![10])]);
    }
    #[test]
    fn question_stop_gate_requires_active_work_and_label() {
        let dir = std::path::PathBuf::from("/home/tree/extensions/test/tmp").join(format!("tc-question-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let prompt = "상태를 알려줘";
        for (session, label, active, blocked) in [
            ("q23", Some(super::Label::Question), false, false),
            ("q45", Some(super::Label::Question), true, true),
            ("q67", Some(super::Label::Feature), true, false),
            ("q89", None, true, false),
        ] {
            super::prompt_decision::decision_with(&dir, session, prompt, |_| label);
            assert_eq!(super::status_question_stop_allowed_at(&dir, session, prompt, active), blocked);
            if blocked { assert!(!super::status_question_stop_allowed_at(&dir, session, prompt, active)); }
        }
        eprintln!("[verify-step] range-check start=23 mid=45 end=67");
        std::fs::remove_dir_all(dir).unwrap();
    }
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup { fn drop(&mut self) { if self.0.is_dir() { let _ = std::fs::remove_dir_all(&self.0); } else { let _ = std::fs::remove_file(&self.0); } } }


    #[test]
    fn session_start_sweeps_old_state_but_keeps_current_and_locked_files() {
        use std::time::{Duration, SystemTime};
        let root = std::env::temp_dir().join(format!("tc-sweep-{}", std::process::id()));
        let _cleanup = Cleanup(root.clone());
        let (state, home, tmp) = (root.join("state"), root.join("home"), root.join("tmp"));
        for dir in [state.join("session_start"), state.join("session_rows"), home.join(".local/state/main-edit-gate"), tmp.clone()] { std::fs::create_dir_all(dir).unwrap(); }
        let old = SystemTime::now() - Duration::from_secs(15 * 24 * 3600);
        let paths = [state.join("session_start/old.json"), state.join("session_start/old.blocked"), state.join("session_start/old.defect"), state.join("j2-old.json"), state.join("session_rows/old.json"), home.join(".local/state/main-edit-gate/images-old.json"), tmp.join("grist-old.lock"), tmp.join("tc-old")];
        for path in &paths { let file = std::fs::File::create(path).unwrap(); file.set_times(std::fs::FileTimes::new().set_modified(old)).unwrap(); }
        let current = state.join("session_start/current.json");
        let file = std::fs::File::create(&current).unwrap(); file.set_times(std::fs::FileTimes::new().set_modified(old)).unwrap();
        let locked = tmp.join("dispatch-agent-old.lock");
        let file = std::fs::File::create(&locked).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(2 * 24 * 3600))).unwrap();
        assert_eq!(unsafe { super::flock(std::os::fd::AsRawFd::as_raw_fd(&file), 2 | 4) }, 0);
        super::cleanup_expired_state(&state, &home, &tmp, "current");
        assert!(paths.iter().all(|path| !path.exists()));
        assert!(current.exists() && locked.exists());
    }
    use super::{complete_work_valid, dispatch_wait_allowed, main_reason, needs_j2, parse_j2_response, pending_reason_with_error, running_dispatch, session_row_ids_at, session_rows, worker_stop_allowed, json::s, Value, MAIN_ALLOWED};
    use std::path::{Path, PathBuf};
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

    #[test]
    fn improver_stop_needs_its_own_done_line() {
        let dir = std::env::temp_dir().join(format!("tc-improver-stop-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".ai/changes")).unwrap();
        let record = dir.join(".ai/changes/261109.a.md");
        std::fs::write(&record, "# 진행\n\n## [구현] [✒️] x (#9004)\n\n## 개선 요구\n- [#9004] 실패 2회\n").unwrap();
        assert!(super::improver_stop(&dir, Some("9004")).unwrap_err().contains("improve-entry done 9004"));
        assert!(super::improver_stop(&dir, None).is_err());
        std::fs::write(&record, "# 진행\n\n## [구현] [✒️] x (#9004)\n\n## 개선 요구\n- [x] [#9004] 실패 2회\n").unwrap();
        assert!(super::improver_stop(&dir, Some("9004")).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn stage_row(stage: &str) -> Value {
        Value::Obj(vec![("id".into(), Value::Num("359".into())), ("H".into(), s(stage))])
    }

    fn structure_fixture(doc: &str) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = PathBuf::from("/home/tree/extensions/blender_scene_creator/test/tmp").join(format!(
            "task-complete-structure-{}-{}",
            std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join(".ai/docs")).unwrap();
        std::fs::write(dir.join(".ai/docs/architecture.md"), doc).unwrap();
        dir
    }

    #[test]
    fn structure_difference_blocks_confirmation_with_folder_names() {
        let dir = structure_fixture("# 구조\n- documented/ : 문서에만 있음\n");
        std::fs::create_dir(dir.join("actual")).unwrap();
        let error = super::complete_row(dir.to_str().unwrap(), &stage_row("검증")).unwrap_err();
        assert_eq!(error, "#359 구조 차이: 문서에만 있음 documented / 문서에 없음 actual — .ai/docs/architecture.md를 갱신하세요");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn structure_match_allows_confirmation_path() {
        let dir = structure_fixture("# 구조\n- actual/ : 실제 폴더\n");
        std::fs::create_dir(dir.join("actual")).unwrap();
        assert!(super::ensure_structure_matches(&dir).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn structure_difference_reports_missing_nested_folder() {
        let dir = structure_fixture("# 구조\n- src/ : 소스\n  - src/components/ : 컴포넌트\n");
        std::fs::create_dir(dir.join("src")).unwrap();
        let error = super::ensure_structure_matches(&dir).unwrap_err();
        assert!(error.contains("문서에만 있음 src/components"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn analysis_stop_needs_a_complete_report() {
        let path = std::env::temp_dir().join(format!("analysis-stop-{}.md", std::process::id()));
        let path_text = path.to_str().unwrap();
        assert!(super::analysis_stop_reason(path_text).unwrap().contains("없어요"));
        std::fs::write(&path, "# 재현\n1회\n").unwrap();
        assert!(super::analysis_stop_reason(path_text).unwrap().contains("수치"));
        std::fs::write(&path, "# 재현\n1회\n# 수치\n3ms\n# 배제한 가설\n없음\n# 원인 후보\nA\n# 확신도\n중간\n# 해결책\nB\n# 확인 방법\nC\n").unwrap();
        assert_eq!(super::analysis_stop_reason(path_text), None);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn verifier_stop_needs_a_recorded_check() {
        let dir = dispatch_fixture();
        let _cleanup = Cleanup(dir.clone());
        assert!(!super::verifier_recorded(&dir, 504));
        let path = grist::gate::gate_path(&dir, 504);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "{\"verifier_recorded\":true}").unwrap();
        assert!(super::verifier_recorded(&dir, 504));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn verifier_stop_requires_runtime_execution_in_current_log() {
        let dir = dispatch_fixture();
        let _cleanup = Cleanup(dir.clone());
        std::fs::create_dir_all(dir.join(".ai/dispatch")).unwrap();
        let agents = dir.join("AGENTS.md");
        let log = dir.join(".ai/dispatch/629.verify.log");
        let row = Value::Obj(vec![("D".into(), s(""))]);
        let set_runtime = |value: &str| std::fs::write(&agents, format!("# 상태\n### 실행\nruntime : {value}\n")).unwrap();
        set_runtime("http://localhost:5175");
        assert!(super::verifier_execution(&dir, 629, &row).unwrap_err().contains("verify.log"));
        std::fs::write(&log, "[stagehand-step] cdp-attach-start +10ms\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_err());
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).unwrap_err().contains("readback"));
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] readback consumer=x value=\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_err());
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] readback consumer=x value=1\n[verify-step] real-flow 명령 실행 → 실제 API → x\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_ok());
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] readback consumer=x value=1\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).unwrap_err().contains("real-flow"));
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] readback consumer=x value=가짜 requestUrl 1회\n[verify-step] real-flow x\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).unwrap_err().contains("mock"));
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] range-check start=1 mid=5 end=10\n[verify-step] readback consumer=x value=1\n[verify-step] real-flow x\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).unwrap_err().contains("range-check"));
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] range-check start=5 end=12\n[verify-step] readback consumer=x value=1\n[verify-step] real-flow x\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).unwrap_err().contains("range-check"));
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] range-check start=5 mid=9 end=14\n[verify-step] readback consumer=x value=1\n[verify-step] real-flow x\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_ok());
        // 표시·변형만 바뀐 행: 기존 정상 근거와 바뀐 부분이 있으면 real-flow 없이 통과, 근거가 없으면 거부.
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] scope=display base=#853 changed=저장버튼라벨\n[verify-step] readback consumer=화면 value=저장하기\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_ok());
        std::fs::write(&log, "[verify-step] screenshot=/tmp/settings.png\n[verify-step] scope=display base=#853 changed=선택칸\n[verify-step] readback consumer=화면 value=선택칸 있음\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_ok());
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] scope=variant base=v0.0.32 changed=종류선택지\n[verify-step] readback consumer=Table16 value=장면\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_ok());
        for weak in ["scope=display changed=라벨", "scope=display base=이전 changed=라벨", "scope=display base=#853", "scope=flow base=#853 changed=라벨"] {
            std::fs::write(&log, format!("[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] {weak}\n[verify-step] readback consumer=화면 value=저장하기\n")).unwrap();
            assert!(super::verifier_execution(&dir, 629, &row).unwrap_err().contains("real-flow"), "{weak}");
        }
        std::fs::write(&log, "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] scope=display base=#853 changed=라벨\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).unwrap_err().contains("readback"));
        set_runtime("Blender 5.2 애드온");
        assert!(super::verifier_execution(&dir, 629, &row).is_err());
        std::fs::write(&log, "/usr/sbin/bash -lc 'blender -b' in /tmp\n succeeded in 20ms:\n[verify-step] runtime-start cmd=blender -b\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_err());
        std::fs::write(&log, "/usr/sbin/bash -lc 'blender -b' in /tmp\n succeeded in 20ms:\n[verify-step] runtime-start cmd=blender -b\n[verify-step] readback consumer=x value=1\n[verify-step] real-flow 명령 실행 → 실제 API → x\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_ok());
        set_runtime("없음(로컬 CLI)");
        assert!(super::verifier_execution(&dir, 629, &row).is_err());
        std::fs::write(&log, "/usr/sbin/bash -lc '~/ai/scripts/bin/grist get Table7' in /tmp\n succeeded in 20ms:\n[verify-step] cli-run cmd=grist get Table7\n[verify-step] readback consumer=x value=1\n[verify-step] real-flow 명령 실행 → 실제 API → x\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).is_ok());
        // 261114 #944: 검증자가 대행자 유닛 테스트를 다시 돌리면 거부한다.
        std::fs::write(&log, "/usr/sbin/bash -lc 'npx vitest run test/a.test.ts' in /tmp\n succeeded in 20ms:\n[verify-step] cli-run cmd=grist get Table7\n[verify-step] readback consumer=x value=1\n[verify-step] real-flow 명령 실행 → 실제 API → x\n").unwrap();
        assert!(super::verifier_execution(&dir, 629, &row).unwrap_err().contains("유닛 테스트"));
        set_runtime("알 수 없는 방식");
        assert!(super::verifier_execution(&dir, 629, &row).is_err());
        let waiting = Value::Obj(vec![("D".into(), s("[🏷] 로그인 사용자 입력 대기"))]);
        assert!(super::verifier_execution(&dir, 629, &waiting).is_ok());
    }

    #[test]
    fn loss_check_requires_current_marked_row_log_with_complete_accounting() {
        let dir = dispatch_fixture();
        let _cleanup = Cleanup(dir.clone());
        std::fs::create_dir_all(dir.join(".ai/gate")).unwrap();
        std::fs::write(dir.join("AGENTS.md"), "# 상태\n### 실행\nruntime : http://localhost:5175\n").unwrap();
        let row = Value::Obj(vec![("D".into(), s(""))]);
        let base = "[stagehand-step] cdp-attach-done +20ms (mode=attach)\n[verify-step] readback consumer=Grist value=12\n[verify-step] real-flow n8n → Grist\n";
        let log = dir.join(".ai/dispatch/934.verify.log");
        std::fs::write(&log, base).unwrap();
        assert!(super::verifier_execution(&dir, 934, &row).is_ok()); // unmarked rows retain their behavior
        std::fs::write(dir.join(".ai/gate/934.n8n"), "261114\n").unwrap();
        let missing = super::verifier_execution(&dir, 934, &row).unwrap_err();
        assert!(missing.contains("flow.md") && missing.contains("loss-check"));
        for (line, expected) in [
            ("[verify-step] loss-check source=12 recorded=3 excluded=7 deleted=2 unexplained=1", "unexplained"),
            ("[verify-step] loss-check source=13 recorded=3 excluded=7 deleted=2 unexplained=0", "일치하지"),
            ("[verify-step] loss-check source=12 recorded=3 excluded=x deleted=2 unexplained=0", "수치 형식"),
            ("[verify-step] loss-check source=12 recorded=3 excluded=7 deleted=2", "수치 형식"),
        ] {
            std::fs::write(&log, format!("{base}{line}\n")).unwrap();
            let reason = super::verifier_execution(&dir, 934, &row).unwrap_err();
            assert!(reason.contains(expected) && reason.contains("flow.md"), "{reason}");
        }
        let valid = "[verify-step] loss-check source=12 recorded=3 excluded=7 deleted=2 unexplained=0";
        // 261114 #943: 손실 대조만으로는 부족하고 CDP로 연 사이트 목록 대조가 있어야 한다.
        std::fs::write(&log, format!("{base}{valid}\n")).unwrap();
        assert!(super::verifier_execution(&dir, 934, &row).unwrap_err().contains("site-coverage"));
        let site_missing = "[verify-step] site-coverage site=linkareer listed=30 in_grist=29 missing=1";
        std::fs::write(&log, format!("{base}{valid}\n{site_missing}\n")).unwrap();
        assert!(super::verifier_execution(&dir, 934, &row).unwrap_err().contains("missing"));
        let no_cdp = "[verify-step] cli-run cmd=x\n[verify-step] readback consumer=Grist value=12\n[verify-step] real-flow n8n → Grist\n";
        let site_ok = "[verify-step] site-coverage site=linkareer listed=30 in_grist=30 missing=0";
        std::fs::write(&log, format!("{no_cdp}{valid}\n{site_ok}\n")).unwrap();
        assert!(!super::verifier_execution(&dir, 934, &row).unwrap_err().contains("사이트 대조")); // 사이트 대조는 CDP 기록을 요구하지 않음(남은 오류는 웹 런타임 실행 기록)
        let valid = format!("{valid}\n{site_ok}\n[verify-step] site-coverage site=gomlab listed=50 in_grist=41 missing=0");
        std::fs::write(&log, format!("{base}{valid}\n")).unwrap();
        assert!(super::verifier_execution(&dir, 934, &row).is_ok());
        std::fs::write(&log, format!("{base}{valid}\n[verify-step] loss-check source=12 recorded=3 excluded=7 deleted=2 unexplained=1\n")).unwrap();
        assert!(super::verifier_execution(&dir, 934, &row).unwrap_err().contains("unexplained"));
        println!("[verify-step] range-check start=3 mid=7 end=12");
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
    fn stop_streak_yields_after_consecutive_blocks() {
        let file = std::env::temp_dir().join(format!("tc-streak-{}", std::process::id()));
        let _cleanup = Cleanup(file.clone());
        let block = || vec![("decision".into(), s("block")), ("reason".into(), s("미완료"))];
        assert_eq!(super::cap_stop_streak(block(), false, &file).len(), 2);
        for _ in 1..super::MAX_STOP_STREAK {
            assert_eq!(super::cap_stop_streak(block(), true, &file).len(), 2);
        }
        let allowed = super::cap_stop_streak(block(), true, &file);
        assert!(allowed.iter().all(|(k, _)| k != "decision"));
        assert!(!file.exists());
        // 양보한 같은 사유는 다음 턴(새 Stop)에도 막지 않고, 횟수 [N]만 다른 사유도 같은 사유로 본다.
        let yielded = file.with_extension("yielded");
        let _yielded_cleanup = Cleanup(yielded.clone());
        assert!(super::cap_stop_streak(block(), false, &file).is_empty());
        let counted = |n: u32| vec![("decision".into(), s("block")), ("reason".into(), s(format!("[{n}] 미완료")))];
        assert!(super::stop_reason_key("[17] 미완료") == "미완료");
        assert!(super::cap_stop_streak(counted(18), true, &file).is_empty());
        // 다른 사유는 다시 막고, 차단이 없는 Stop은 양보 기록을 지운다.
        let other = vec![("decision".into(), s("block")), ("reason".into(), s("다른 사유"))];
        assert_eq!(super::cap_stop_streak(other, false, &file).len(), 2);
        assert!(super::cap_stop_streak(vec![], false, &file).is_empty());
        assert!(!file.exists() && !yielded.exists());
        assert_eq!(super::cap_stop_streak(block(), false, &file).len(), 2);
        println!("[verify-step] range-check start=2 mid=3 end=18");
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
        let _cleanup = Cleanup(dir.clone());
        assert!(running_dispatch(&dir));
        assert!(dispatch_wait_allowed(&[stage_row("초안")], &dir));
        assert!(dispatch_wait_allowed(&[stage_row("초안"), stage_row("검증")], &dir));
        assert!(super::dispatch_wait_stop_allowed(true, &[stage_row("검증")], &dir));
        assert!(!super::dispatch_wait_stop_allowed(false, &[stage_row("검증")], &dir));
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
        let _cleanup = Cleanup(dir.clone());
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
        let _cleanup = Cleanup(dir.clone());
        std::fs::write(dir.join(".ai/dispatch/359.exit"), "0").unwrap();
        assert!(!running_dispatch(&dir));
        assert!(!dispatch_wait_allowed(&[stage_row("초안")], &dir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dispatch_wait_running_verifier_allows_main_stop_with_mixed_rows() {
        let dir = dispatch_fixture();
        let _cleanup = Cleanup(dir.clone());
        std::fs::rename(dir.join(".ai/dispatch/359.sh"), dir.join(".ai/dispatch/359-verifier.sh")).unwrap();
        std::fs::rename(dir.join(".ai/dispatch/359.log"), dir.join(".ai/dispatch/359-verifier.log")).unwrap();
        assert!(super::dispatch_wait_stop_allowed(true, &[stage_row("검증"), stage_row("확인")], &dir));
        std::fs::write(dir.join(".ai/dispatch/359-verifier.exit"), "0").unwrap();
        assert!(!super::dispatch_wait_stop_allowed(true, &[stage_row("검증")], &dir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn session_dispatch_wait_requires_dispatch_for_each_pending_project() {
        let root = dispatch_fixture();
        let _cleanup = Cleanup(root.clone());
        let product = root.join("product");
        std::fs::create_dir_all(&product).unwrap();
        let projects = vec![("860".into(), root.to_string_lossy().into_owned()), ("853".into(), product.to_string_lossy().into_owned())];
        let confirmed = Value::Obj(vec![("id".into(), Value::Num("860".into())), ("H".into(), s("확인"))]);
        let pending = Value::Obj(vec![("id".into(), Value::Num("853".into())), ("H".into(), s("검증"))]);
        assert!(!super::session_dispatch_wait_stop_allowed(true, &[confirmed.clone(), pending], &projects, root.to_str().unwrap()));
        let product_dispatch = dispatch_fixture();
        let _product_cleanup = Cleanup(product_dispatch.clone());
        let projects_with_dispatch = vec![("860".into(), root.to_string_lossy().into_owned()), ("853".into(), product_dispatch.to_string_lossy().into_owned())];
        let product_pending = Value::Obj(vec![("id".into(), Value::Num("853".into())), ("H".into(), s("검증"))]);
        assert!(!super::session_dispatch_wait_stop_allowed(true, &[confirmed.clone(), product_pending], &projects_with_dispatch, root.to_str().unwrap()));
        let root_pending = Value::Obj(vec![("id".into(), Value::Num("860".into())), ("H".into(), s("검증"))]);
        assert!(super::session_dispatch_wait_stop_allowed(true, &[root_pending], &projects, root.to_str().unwrap()));
        let product_confirmed = Value::Obj(vec![("id".into(), Value::Num("853".into())), ("H".into(), s("확인"))]);
        assert!(!super::session_dispatch_wait_stop_allowed(true, &[confirmed.clone(), product_confirmed], &projects, root.to_str().unwrap()));
        assert!(!super::session_dispatch_wait_stop_allowed(true, &[confirmed], &projects, root.to_str().unwrap()));
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
        let _cleanup = Cleanup(dir.clone());
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
        assert!(!needs_j2(false, "", None, None, None));
        assert!(!needs_j2(true, "t26_276", None, None, None));
        assert!(!needs_j2(true, "", Some("443"), None, None));
    }

    #[test]
    fn last_assistant_message_falls_back_to_transcript_path() {
        let path = std::env::temp_dir().join(format!("tc-transcript-{}.jsonl", std::process::id()));
        let _cleanup = Cleanup(path.clone());
        std::fs::write(
            &path,
            "{\"type\":\"USER_INPUT\",\"content\":\"질문\"}\n{\"type\":\"PLANNER_RESPONSE\",\"content\":\"최종 보고예요.\"}\n",
        )
        .unwrap();
        let event = Value::Obj(vec![("transcript_path".into(), s(path.to_string_lossy()))]);
        assert_eq!(super::last_assistant_message_from(&event), "최종 보고예요.");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn no_explanation_preference_persists_until_user_requests_detail() {
        let path = std::env::temp_dir().join(format!("tc-no-explain-{}.jsonl", std::process::id()));
        std::fs::write(&path, "{\"type\":\"USER_INPUT\",\"content\":\"문제는 알고 있으니 그딴걸 설명하지 말고 먼저 고쳐\"}\n{\"type\":\"USER_INPUT\",\"content\":\"결과 알려줘\"}\n").unwrap();
        let event = Value::Obj(vec![("transcript_path".into(), s(path.to_string_lossy()))]);
        assert!(super::explanation_suppressed_by_user(&event));
        assert!(super::suppressed_explanation_reason("원인은 제가 게이트를 잘못 골랐기 때문이에요.").is_some());
        assert!(super::suppressed_explanation_reason("수정했어요. 테스트가 통과했어요.").is_none());
        std::fs::write(&path, "{\"type\":\"USER_INPUT\",\"content\":\"설명하지 말고 고쳐\"}\n{\"type\":\"USER_INPUT\",\"content\":\"이번에는 자세히 설명해줘\"}\n").unwrap();
        assert!(!super::explanation_suppressed_by_user(&event));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agy_truncated_reply_uses_full_transcript_and_worker_report_ends() {
        let dir = Path::new("/home/tree/extensions/blender_scene_creator/test/tmp").join(format!("agy-reply-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("transcript.jsonl");
        std::fs::write(&path, "{\"type\":\"PLANNER_RESPONSE\",\"content\":\"잘림\",\"truncated_fields\":[\"content\"]}\n").unwrap();
        std::fs::write(dir.join("transcript_full.jsonl"), "{\"type\":\"PLANNER_RESPONSE\",\"content\":\"1. [기능] 진입점: /p:1 main — 조사 결과\"}\n").unwrap();
        let event = Value::Obj(vec![("transcript_path".into(), s(path.to_string_lossy()))]);
        assert!(super::last_assistant_message_from(&event).contains("조사 결과"));
        assert!(worker_stop_allowed(&event, &Value::Obj(vec![]), "미제출"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn agy_question_wait_tracks_tool_calls_and_user_input() {
        let dir = Path::new("/home/tree/extensions/blender_scene_creator/test/tmp").join(format!("agy-ask-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("transcript.jsonl");
        let event = Value::Obj(vec![("transcript_path".into(), s(path.to_string_lossy()))]);
        let ask = "{\"type\":\"PLANNER_RESPONSE\",\"tool_calls\":[{\"name\":\"ask_question\"}]}\n";
        std::fs::write(&path, ask).unwrap();
        assert!(super::main_asked_user(&event));
        std::fs::write(&path, format!("{ask}{{\"type\":\"PLANNER_RESPONSE\",\"toolCalls\":[{{\"name\":\"run_command\"}}]}}\n")).unwrap();
        assert!(!super::main_asked_user(&event));
        std::fs::write(&path, format!("{ask}{{\"type\":\"USER_INPUT\",\"content\":\"응답\"}}\n")).unwrap();
        assert!(!super::main_asked_user(&event));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn main_question_wait_tracks_transcript_tool_and_user_reply() {
        let path = std::env::temp_dir().join(format!("tc-ask-{}.jsonl", std::process::id()));
        let _cleanup = Cleanup(path.clone());
        let event = Value::Obj(vec![("transcript_path".into(), s(path.to_string_lossy()))]);
        let ask = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"AskUserQuestion\"}]}}\n";
        let result = "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"content\":\"\"}]}}\n";
        let text = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"답변을 기다려요.\"}]}}\n";
        std::fs::write(&path, text).unwrap();
        assert!(!super::main_asked_user(&event));
        std::fs::write(&path, ask).unwrap();
        assert!(super::main_asked_user(&event));
        std::fs::write(&path, format!("{ask}{result}{text}")).unwrap();
        assert!(super::main_asked_user(&event));
        std::fs::write(&path, format!("{ask}{result}{text}{{\"type\":\"USER_INPUT\",\"content\":\"답변\"}}\n")).unwrap();
        assert!(!super::main_asked_user(&event));
        std::fs::write(&path, format!("{ask}{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"OtherTool\"}}]}}}}\n")).unwrap();
        assert!(!super::main_asked_user(&event));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn user_action_wait_needs_every_pending_row_and_exact_state() {
        let row = |id: &str, d: &str| Value::Obj(vec![
            ("id".into(), Value::Num(id.into())), ("H".into(), s("검증")), ("D".into(), s(d)),
        ]);
        let waiting = row("514", "[🏷] 구현 결과 확인, 사용자에게 확인 중");
        let second = row("515", "[🏷] 로그인 사용자 입력 대기");
        let signature = super::user_action_wait_rows(&[waiting.clone(), second.clone()]).unwrap();
        assert!(signature.py_eq(&super::user_action_wait_rows(&[waiting, second]).unwrap()));
        let first_stop = Value::Obj(vec![]);
        let after_first_block = Value::Obj(vec![("wait_rows".into(), signature.clone())]);
        assert!(!super::repeated_user_action_wait(&first_stop, &signature));
        assert!(super::repeated_user_action_wait(&after_first_block, &signature));
        assert!(!signature.py_eq(&super::user_action_wait_rows(&[row("514", "[🏷] 다른 사용자 입력 대기"), row("515", "[🏷] 로그인 사용자 입력 대기")]).unwrap()));
        assert!(super::user_action_wait_rows(&[row("514", "[🏷] 사용자에게 확인 중"), row("515", "[🏷] 구현 중")]).is_none());
        assert!(super::user_action_wait_rows(&[row("514", "[🏷] 사용자 동작 필요")]).is_none());
        assert!(super::user_action_wait_rows(&[row("514", "사용자 입력 대기")]).is_none());
    }

    #[test]
    fn user_action_wait_uses_reason_for_each_stage() {
        for (stage, reason) in [
            ("초안", "[🏷] 사용자 입력 대기"),
            ("진단", "[🏷] 사용자에게 확인 중"),
            ("작업", "[🏷] 사용자 검증 중단"),
            ("검증", "[🏷] 사용자 입력 대기"),
            ("확인", "[🏷] 사용자에게 확인 중"),
        ] {
            let row = Value::Obj(vec![
                ("id".into(), Value::Num("593".into())),
                ("H".into(), s(stage)),
                ("D".into(), s(reason)),
            ]);
            let pending = if stage == "확인" { vec![] } else { vec![row.clone()] };
            let signature = super::completion_user_action_wait_rows(
                &[row.clone()], &pending, Some("#593 TEST: confirmation evidence missing"),
            ).unwrap();
            assert!(!super::repeated_user_action_wait(&Value::Obj(vec![]), &signature), "{stage}: first Stop");
            let first_block = Value::Obj(vec![("wait_rows".into(), signature.clone())]);
            assert!(super::repeated_user_action_wait(&first_block, &signature), "{stage}: second Stop");
            let changed = Value::Obj(vec![
                ("id".into(), Value::Num("593".into())),
                ("H".into(), s(stage)),
                ("D".into(), s("[🏷] 다른 사용자 입력 대기")),
            ]);
            let changed_pending = if stage == "확인" { vec![] } else { vec![changed.clone()] };
            let changed_signature = super::completion_user_action_wait_rows(
                &[changed], &changed_pending, Some("#593 TEST: confirmation evidence missing"),
            ).unwrap();
            assert!(!super::repeated_user_action_wait(&first_block, &changed_signature), "{stage}: changed reason");
            let no_reason = Value::Obj(vec![
                ("id".into(), Value::Num("593".into())),
                ("H".into(), s(stage)),
                ("D".into(), s("[🏗️] 구현 진행 중")),
            ]);
            let no_reason_pending = if stage == "확인" { vec![] } else { vec![no_reason.clone()] };
            assert!(super::completion_user_action_wait_rows(
                &[no_reason], &no_reason_pending, Some("#593 TEST: confirmation evidence missing"),
            ).is_none(), "{stage}: no wait reason");
        }
    }

    #[test]
    fn user_action_wait_accepts_mixed_stages_with_wait_reasons() {
        let row = |id: &str, stage: &str, reason: &str| Value::Obj(vec![
            ("id".into(), Value::Num(id.into())),
            ("H".into(), s(stage)),
            ("D".into(), s(reason)),
        ]);
        let draft = row("593", "초안", "[🏷] 사용자 입력 대기");
        let diagnosis = row("594", "진단", "[🏷] 사용자에게 확인 중");
        let verification = row("587", "검증", "[🏷] 사용자 검증 중단");
        let confirmed = row("537", "확인", "[🏷] 사용자 확인 중");
        let pending = [draft.clone(), diagnosis.clone(), verification.clone()];
        let rows = [draft, diagnosis, verification, confirmed];
        let signature = super::completion_user_action_wait_rows(
            &rows, &pending, Some("#537 TEST: flow-wait evidence missing"),
        ).unwrap();
        let first_block = Value::Obj(vec![("wait_rows".into(), signature.clone())]);
        assert!(super::repeated_user_action_wait(&first_block, &signature));
        let mut without_reason = pending.to_vec();
        without_reason[1] = row("594", "진단", "[🏗️] 원인 조사 중");
        assert!(super::completion_user_action_wait_rows(&without_reason, &without_reason, None).is_none());
    }

    #[test]
    fn user_action_wait_confirmed_evidence_error_allows_second_stop() {
        let confirmed = Value::Obj(vec![
            ("id".into(), Value::Num("537".into())),
            ("H".into(), s("확인")),
            ("D".into(), s("[🏷] 사용자 대기: 화면 확인 끝, Grist 완료 처리 대기")),
        ]);
        let error = "#537 TEST: #537 flow-wait evidence missing";
        let signature = super::completion_user_action_wait_rows(&[confirmed.clone()], &[], Some(error)).unwrap();
        assert!(!super::repeated_user_action_wait(&Value::Obj(vec![]), &signature));
        let first_block = Value::Obj(vec![("wait_rows".into(), signature.clone())]);
        let same = super::completion_user_action_wait_rows(&[confirmed.clone()], &[], Some(error)).unwrap();
        assert!(super::repeated_user_action_wait(&first_block, &same));
        assert!(super::completion_user_action_wait_rows(&[confirmed.clone()], &[], None).is_none());
        let changed = Value::Obj(vec![
            ("id".into(), Value::Num("537".into())),
            ("H".into(), s("확인")),
            ("D".into(), s("[🏷] 사용자 확인 중")),
        ]);
        let changed_signature = super::completion_user_action_wait_rows(&[changed], &[], Some(error)).unwrap();
        assert!(!super::repeated_user_action_wait(&first_block, &changed_signature));
    }

    #[test]
    fn user_action_wait_confirmed_nonwaiting_evidence_error_keeps_blocking() {
        let confirmed = Value::Obj(vec![
            ("id".into(), Value::Num("537".into())),
            ("H".into(), s("확인")),
            ("D".into(), s("[🏷] 완료 증거 오류 점검 중")),
        ]);
        let rows = [confirmed];
        let error = "#537 TEST: #537 flow-wait evidence missing";
        assert!(super::completion_user_action_wait_rows(&rows, &[], Some(error)).is_none());
        assert!(super::completion_user_action_wait_rows(&rows, &[], Some(error)).is_none());
    }

    #[test]
    fn user_action_wait_verified_error_with_nonwaiting_peer_keeps_blocking() {
        let waiting = Value::Obj(vec![("id".into(), Value::Num("544".into())), ("H".into(), s("검증")), ("D".into(), s("[🏷] 사용자 확인 중"))]);
        let peer = Value::Obj(vec![("id".into(), Value::Num("545".into())), ("H".into(), s("검증")), ("D".into(), s("[🏗️] 구현 완료"))]);
        let error = "#544 TEST: #544 flow-wait evidence missing";
        assert!(super::completion_user_action_wait_rows(&[waiting.clone(), peer.clone()], &[waiting, peer], Some(error)).is_none());
        assert!(super::pending_reason_with_error("대기".into(), Some(error)).contains("막힌 행 #544, 빠진 증거: flow-wait 결과"));
        let no_confirm = Value::Obj(vec![("id".into(), Value::Num("545".into())), ("H".into(), s("검증")), ("D".into(), s("[🏗️] 구현 완료"))]);
        let still_waiting = Value::Obj(vec![("id".into(), Value::Num("544".into())), ("H".into(), s("검증")), ("D".into(), s("[🏷] 사용자 검증 중단"))]);
        assert!(super::user_action_wait_rows(&[still_waiting, no_confirm]).is_none());
    }

    #[test]
    fn user_action_completion_error_names_missing_confirmation_block() {
        let reason = super::pending_reason_with_error("대기".into(), Some("#545 TEST: confirmation evidence missing"));
        assert!(reason.contains("막힌 행 #545, 빠진 증거: 확인 블록"));
    }

    #[test]
    fn j2_is_required_only_for_main_stop_hook() {
        assert!(needs_j2(true, "", None, None, None));
        assert!(!needs_j2(false, "", None, None, None));
        assert!(!needs_j2(true, "", None, Some("/project"), None));
        assert!(!needs_j2(true, "", None, Some("/project"), Some("276")));
        assert!(!needs_j2(true, "", None, None, Some("276")));
        assert!(!needs_j2(true, "t26_276", None, None, None));
        assert!(!needs_j2(true, "", Some("443"), None, None));
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
    fn claude_pointer_records_session_for_project() {
        let dir = std::env::temp_dir().join(format!("task-complete-claude-{}", std::process::id()));
        let project = dir.join("project");
        std::fs::create_dir_all(&project).unwrap();
        super::record_claude_session_at(&dir, &project, "claude-123").unwrap();
        let pointer = super::json::parse(&std::fs::read_to_string(super::claude_session_path_at(&dir, &project)).unwrap()).unwrap();
        assert_eq!(pointer.get("session_id").and_then(Value::as_str), Some("claude-123"));
        assert_eq!(pointer.get("cwd").and_then(Value::as_str), project.to_str());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn claude_stop_session_blocks_verification_and_allows_confirmed_row() {
        let event = super::json::parse(r#"{"session_id":"claude-123","cwd":"/project"}"#).unwrap();
        assert_eq!(super::session_id_from(&event, None).as_deref(), Some("claude-123"));
        assert_eq!(super::session_id_from(&event, Some("codex-456")).as_deref(), Some("codex-456"));
        let owned = vec!["410".to_string()];
        let verifying = Value::Obj(vec![("id".into(), Value::Num("410".into())), ("H".into(), s("검증"))]);
        let confirmed = Value::Obj(vec![("id".into(), Value::Num("410".into())), ("H".into(), s("확인"))]);
        let done = vec!["410".to_string()];
        let rows = session_rows(vec![verifying], &owned);
        assert!(!complete_work_valid(&rows, &rows, &done));
        let rows = session_rows(vec![confirmed], &owned);
        assert!(complete_work_valid(&rows, &rows, &done));
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
    fn cross_project_session_rows_reach_completion_gate() {
        let root = "/home/tree/extensions";
        let product = "/home/tree/extensions/web_save_current_page";
        let projects = vec![("860".into(), root.into()), ("853".into(), product.into())];
        let root_row = Value::Obj(vec![("id".into(), Value::Num("860".into())), ("H".into(), s("확인"))]);
        let product_row = Value::Obj(vec![("id".into(), Value::Num("853".into())), ("H".into(), s("검증"))]);
        let rows = super::rows_for_session_projects_with(&["860".into(), "853".into()], &projects, root, vec![root_row], |path| {
            assert_eq!(path, product);
            Ok(vec![product_row.clone()])
        }).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].get("id").unwrap().py_str(), "853");
        assert!(rows.iter().any(|row| row.get("H").and_then(Value::as_str) == Some("검증")));
        let mut checked = Vec::new();
        let result = super::check_session_rows_with(&rows, &projects, root, |path, row| {
            checked.push((path.to_string(), row.get("id").unwrap().py_str()));
            Ok(())
        });
        assert_eq!(result, Ok(()));
        assert_eq!(checked, vec![(root.into(), "860".into()), (product.into(), "853".into())]);
        let pending = rows.iter().filter(|row| row.get("H").and_then(Value::as_str) == Some("검증")).cloned().collect::<Vec<_>>();
        assert!(!super::should_run_j2(&rows, &pending, "", None, "", None, None, None));
        let confirmed = vec![
            Value::Obj(vec![("id".into(), Value::Num("860".into())), ("H".into(), s("확인"))]),
            Value::Obj(vec![("id".into(), Value::Num("853".into())), ("H".into(), s("확인"))]),
        ];
        assert!(super::should_run_j2(&confirmed, &[], "", None, "", None, None, None));
    }

    #[test]
    fn unfinished_session_rows_stay_owned_after_process_fix() {
        let rows = vec![
            Value::Obj(vec![("id".into(), Value::Num("860".into())), ("H".into(), s("초안"))]),
            Value::Obj(vec![("id".into(), Value::Num("861".into())), ("H".into(), s("초안"))]),
        ];
        let owned = vec!["860".into(), "861".into()];
        assert_eq!(super::session_rows(rows, &owned).len(), 2);
    }

    #[test]
    fn process_block_points_to_manage_intervention() {
        assert!(super::PROCESS_BLOCK.contains("manage-intervention"));
    }

    #[test]
    fn process_correction_scopes_stop_to_process_row_until_resume_is_explicit() {
        let projects = vec![("860".into(), "/home/tree/extensions".into()),
            ("853".into(), "/home/tree/extensions/web_save_current_page".into())];
        let owned = vec!["860".into(), "853".into()];
        assert_eq!(super::process_owned_rows(owned.clone(), &projects, true), vec!["860"]);
        assert_eq!(super::process_owned_rows(owned.clone(), &projects, false), owned);
    }

    #[test]
    fn process_timeout_only_uses_unfinished_rows() {
        let owned = vec!["2".into(), "5".into(), "9".into()];
        let row = |id: &str, stage: &str| Value::Obj(vec![
            ("id".into(), Value::Str(id.into())),
            ("H".into(), Value::Str(stage.into())),
        ]);
        let rows = vec![row("2", "확인"), row("5", "작업"), row("9", "완료")];
        assert_eq!(super::unfinished_process_rows(&owned, &rows), vec!["5"]);
        assert_eq!(super::unfinished_process_rows(&owned, &[row("2", "확인"), row("9", "완료")]), Vec::<String>::new());
    }

    #[test]
    fn prompt_label_overrides_previous_row_state() {
        let projects = vec![("860".into(), "/home/tree/extensions".into()),
            ("853".into(), "/home/tree/extensions/product".into())];
        let owned = vec!["860".into(), "853".into()];
        // The classifier can reject a keyword match or accept wording without a keyword.
        assert_eq!(super::process_owned_rows(owned.clone(), &projects, false), owned);
        assert_eq!(super::process_owned_rows(owned, &projects, true), vec!["860"]);
    }

    #[test]
    fn process_turn_requires_latest_turn_fix_and_hook_verification() {
        let dir = PathBuf::from("/home/tree/extensions/test/tmp").join(format!("tc-process-evidence-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join("transcript.jsonl");
        let event = Value::Obj(vec![("transcript_path".into(), Value::Str(transcript.to_string_lossy().into_owned()))]);
        assert!(!super::last_turn_had_fix_action(&event));
        assert_eq!(super::last_turn_hook_verification_action(&event), None);
        let user = "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"과정 수정\"}]}}\n";
        let call = |input: &str| format!("{{\"type\":\"response_item\",\"payload\":{{\"type\":\"custom_tool_call\",\"input\":{}}}}}\n", super::json::dumps(&Value::Str(input.into())));
        std::fs::write(&transcript, format!("{user}{}", call("await tools.apply_patch(patch)"))).unwrap();
        assert_eq!(super::last_turn_fix_action(&event), Some(true));
        assert_eq!(super::last_turn_hook_verification_action(&event), Some(false));
        std::fs::write(&transcript, format!("{user}{}{}", call("await tools.apply_patch(patch)"), call("cargo test -p task-complete process_turn_"))).unwrap();
        assert_eq!(super::last_turn_hook_verification_action(&event), Some(true));
        std::fs::write(&transcript, format!("{user}{}{}{}", call("await tools.apply_patch(patch)"), call("cargo test"), user)).unwrap();
        assert_eq!(super::last_turn_hook_verification_action(&event), Some(false));
        std::fs::write(&transcript, format!("{user}{}", call("dispatch-agent --role improver --detach"))).unwrap();
        assert_eq!(super::last_turn_fix_action(&event), Some(true));
        assert_eq!(super::last_turn_hook_verification_action(&event), Some(true));
        let row = |id: &str, stage: &str| Value::Obj(vec![("id".into(), Value::Str(id.into())), ("H".into(), Value::Str(stage.into()))]);
        assert!(!super::process_rows_only_prior_closed(&[row("860", "확인"), row("910", "확인")], &["910".into()], &["860".into()]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn codex_transcript_uses_latest_user_response_correction() {
        let dir = PathBuf::from("/home/tree/extensions/test/tmp").join(format!("tc-codex-prompt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join("transcript.jsonl");
        std::fs::write(&transcript, concat!(
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"이전 작업을 계속해\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"응답 방식을 고쳐\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call\",\"name\":\"exec\",\"input\":\"{\\\"cmd\\\":\\\"cargo test\\\"}\"}}\n"
        )).unwrap();
        let event = Value::Obj(vec![("transcript_path".into(), Value::Str(transcript.to_string_lossy().into_owned()))]);
        assert_eq!(super::last_user_prompt_from(&event), "응답 방식을 고쳐");
        // running tests is not a fix; an edit in the same turn is
        assert_eq!(super::last_turn_fix_action(&event), Some(false));
        let mut text = std::fs::read_to_string(&transcript).unwrap();
        text.push_str("{\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call\",\"name\":\"exec\",\"input\":\"await tools.apply_patch(patch)\"}}\n");
        std::fs::write(&transcript, text).unwrap();
        assert_eq!(super::last_turn_fix_action(&event), Some(true));
        // an unreadable turn is no evidence for a process fix
        let missing = Value::Obj(vec![("transcript_path".into(), Value::Str(dir.join("none.jsonl").to_string_lossy().into_owned()))]);
        assert_eq!(super::last_turn_fix_action(&missing), None);
        std::fs::remove_dir_all(dir).unwrap();
    }


    #[test]
    fn process_turn_rejects_prior_closed_rows_and_keeps_blocking() {
        let row = |id: &str, stage: &str| Value::Obj(vec![("id".into(), Value::Str(id.into())), ("H".into(), Value::Str(stage.into()))]);
        let prior = vec!["860".to_string(), "861".to_string()];
        let rows = vec![row("860", "확인"), row("861", "완료"), row("862", "초안")];
        assert!(super::process_rows_only_prior_closed(&rows, &["860".into(), "861".into()], &prior));
        assert!(!super::process_rows_only_prior_closed(&rows, &["860".into(), "862".into()], &prior));
        assert!(!super::process_rows_only_prior_closed(&[row("860", "작업")], &["860".into()], &prior));
        let dir = PathBuf::from("/home/tree/extensions/test/tmp").join(format!("tc-process-streak-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let streak = dir.join("streak");
        let block = |reason: String| vec![("decision".to_string(), Value::Str("block".into())), ("reason".to_string(), Value::Str(reason))];
        // 과정 수정 미완료도 같은 상한(3회) 뒤 종료를 허용한다(261114 무한 차단).
        let mut outs = vec![];
        for _ in 0..=super::MAX_STOP_STREAK { outs.push(super::cap_stop_streak(block(format!("{}수정 없음", super::PROCESS_BLOCK)), true, &streak)); }
        assert!(outs[..3].iter().all(|out| out.iter().any(|(k, v)| k == "decision" && v.as_str() == Some("block"))));
        assert!(!outs[3].iter().any(|(k, _)| k == "decision"));
        std::fs::remove_file(streak.with_extension("yielded")).ok();
        std::fs::remove_file(&streak).ok();
        let mut last = vec![];
        for _ in 0..4 { last = super::cap_stop_streak(block("다른 사유".into()), true, &streak); }
        assert!(!last.iter().any(|(k, _)| k == "decision"));
        std::env::set_var("TASK_COMPLETE_STATE_DIR", &dir);
        let prompt = "작업과정 결함을 왜 안 고쳤어?";
        super::prompt_decision::decision_with(&dir, "tp1", prompt, |_| Some(super::Label::Improvement));
        for _ in 0..3 { assert!(super::offer_or_talk_only_reason("tp1", prompt, "원인은 이래요.", false, true).is_some()); }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn process_stop_repeated_block_and_offer_keep_process_prefix() {
        let dir = PathBuf::from("/home/tree/extensions/test/tmp").join(format!("tc-process-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let streak = dir.join("streak");
        for reason in ["제안만", "설명만", "개선 기록 미완료"] {
            let output = super::process_stop_prefix(vec![("decision".into(), s("block")), ("reason".into(), s(reason))], true);
            let output = super::cap_stop_streak(output, true, &streak);
            assert_eq!(output.iter().find(|(key, _)| key == "decision").and_then(|(_, value)| value.as_str()), Some("block"));
            assert!(output.iter().find(|(key, _)| key == "reason").and_then(|(_, value)| value.as_str()).unwrap().starts_with(super::PROCESS_BLOCK));
        }
        // 4번째 연속 차단은 상한(3회)에 걸려 종료를 허용한다.
        let fourth = super::cap_stop_streak(super::process_stop_prefix(vec![("decision".into(), s("block")), ("reason".into(), s("개선자 호출"))], true), true, &streak);
        assert!(!fourth.iter().any(|(key, _)| key == "decision"));
        std::fs::remove_file(streak.with_extension("yielded")).ok();
        let thread = format!("tc-process-repeat-{}", std::process::id());
        super::prompt_decision::decision_with(&super::state_dir(), &thread, "왜 규칙을 안 고쳤어?", |_| Some(super::Label::Improvement));
        for _ in 0..2 {
            let reason = super::offer_or_talk_only_reason(&thread, "왜 규칙을 안 고쳤어?", "원하시면 고칠게요.", false, true).unwrap();
            assert!(reason.starts_with(super::PROCESS_BLOCK));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn process_stop_overdue_note_is_once_across_order_and_competing_stops() {
        use std::sync::{Arc, Mutex};
        let dir = PathBuf::from("/home/tree/extensions/test/tmp").join(format!("tc-process-timeout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let noted = Arc::new(Mutex::new(Vec::<(i64, String)>::new()));
        let owned = vec!["910".into(), "911".into()];
        let prior = vec!["910".into()];
        let run = |now: u64| {
            let read_notes = Arc::clone(&noted);
            let write_notes = Arc::clone(&noted);
            super::process_overdue_notes_with(&dir, &owned, &prior, 1_000, now,
                move |id| {
                    Ok(read_notes.lock().unwrap().iter().any(|(row, _)| *row == id))
                },
                move |id, note| {
                    write_notes.lock().unwrap().push((id, note.to_string()));
                    Ok(())
                })
        };
        assert_eq!(run(2_199).unwrap(), 0);
        std::thread::scope(|scope| {
            let first = scope.spawn(|| run(2_200));
            let second = scope.spawn(|| run(2_260));
            assert_eq!(first.join().unwrap().unwrap() + second.join().unwrap().unwrap(), 1);
        });
        assert_eq!(run(2_320).unwrap(), 0);
        let notes = noted.lock().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].0, 911);
        assert!(notes[0].1.starts_with("[🚥] 과정 수정 20분 초과 — 경과 "));
        drop(notes);
        std::fs::remove_dir_all(dir).unwrap();
    }


    #[test]
    fn j2_rejects_missing_or_malformed_judgment() {
        let pass = r#"{"ok":true,"mode":"enforce","command":"stop","verdict":"pass","category":"완료 보고","items":[{"name":"category","verdict":"pass"}]}"#;
        assert!(parse_j2_response(pass.as_bytes()).is_ok());
        for response in [
            "".to_string(), "{}".to_string(), "not json".to_string(),
            pass.replace("\"ok\":true", "\"ok\":false"),
            pass.replace("\"mode\":\"enforce\"", "\"mode\":\"other\""),
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
        assert!(msg.contains("--continue") && !msg.contains("전체 흐름을 테스트"));
    }

    #[test]
    fn diagnosis_stop_reason_sends_failure_to_the_same_agent_session() {
        let row = Value::Obj(vec![("id".into(), Value::Num("500".into())), ("H".into(), s("진단"))]);
        let msg = main_reason(&[row.clone()], &[row], "/home/tree/extensions");
        assert!(msg.contains("dispatch-agent --role agent --continue"), "{msg}");
        assert!(msg.contains("개선자") && msg.contains("금지"));
        assert!(!msg.contains("재현만 목표로"));
    }

    #[test]
    fn draft_stop_reason_assigns_code_to_dispatch_agent() {
        let draft = Value::Obj(vec![("id".into(), Value::Num("501".into())), ("H".into(), s("초안"))]);
        let msg = main_reason(&[draft.clone()], &[draft], "/home/tree/extensions");
        assert!(msg.contains("`초안` 행의 코드 변경은 파일 수와 상관없이 `dispatch-agent` 대행자에게 맡기고, 규칙·Skill 문서 행만 메인이 직접 처리하세요(`manage-project` `references/plan.md`)."));
        assert!(!msg.contains("수정 파일 1~2개"));
    }

    #[test]
    fn offer_after_complaint_blocks_once_and_passes_other_turns() {
        let base = std::env::temp_dir().join(format!("tc-offer-{}", std::process::id()));
        let _cleanup = Cleanup(base.clone());
        std::fs::create_dir_all(base.join("session_start")).unwrap();
        std::env::set_var("TASK_COMPLETE_STATE_DIR", &base);
        let complaint = "이걸 보고, 왜 새로 추가되는게 그걸 안지키고 있는지 확인해봐";
        let offer = "2일 처리가 없어요. 정해 주시면 JSON을 만들어 드릴게요.";
        super::prompt_decision::decision_with(&base, "o1", complaint, |_| Some(super::Label::Resolution));
        assert!(super::offer_or_talk_only_reason("o1", complaint, "고쳤어요. 검증 결과는 통과예요.", true, false).is_none());
        super::prompt_decision::decision_with(&base, "o1", "이 명령어 뜻이 뭐야", |_| Some(super::Label::Question));
        assert!(super::offer_or_talk_only_reason("o1", "이 명령어 뜻이 뭐야", offer, true, false).is_none());
        super::prompt_decision::decision_with(&base, "o1", complaint, |_| Some(super::Label::Resolution));
        assert!(super::offer_or_talk_only_reason("o1", complaint, offer, true, false).unwrap().contains("수정 요청"));
        assert!(super::offer_or_talk_only_reason("o1", complaint, offer, true, false).is_none());
        super::prompt_decision::decision_with(&base, "o1", "왜 또 안 고쳤어", |_| Some(super::Label::Resolution));
        assert!(super::offer_or_talk_only_reason("o1", "왜 또 안 고쳤어", offer, true, false).is_some());
        super::prompt_decision::decision_with(&base, "o2", "왜 중단되는지에 대한 질문을 먼저 하고 있잖아", |_| Some(super::Label::Question));
        assert!(super::offer_or_talk_only_reason("o2", "왜 중단되는지에 대한 질문을 먼저 하고 있잖아", "로그에서 중단 요청을 확인했어요.", false, false).is_none());
        super::prompt_decision::decision_with(&base, "o3", "왜 중단되는지 대답을 먼저 해", |_| Some(super::Label::Question));
        assert!(super::offer_or_talk_only_reason("o3", "왜 중단되는지 대답을 먼저 해", "로그에서 중단 요청을 확인했어요.", false, false).is_none());
        super::prompt_decision::decision_with(&base, "o4", "왜 중단되는지 대답을 먼저 하고 코드도 고쳐", |_| Some(super::Label::Resolution));
        assert!(super::offer_or_talk_only_reason("o4", "왜 중단되는지 대답을 먼저 하고 코드도 고쳐", "원인을 설명했어요.", false, false).is_some());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn last_user_prompt_skips_meta_tool_results_and_hook_feedback() {
        let path = std::env::temp_dir().join(format!("tc-prompt-{}.jsonl", std::process::id()));
        let _cleanup = Cleanup(path.clone());
        let event = Value::Obj(vec![("transcript_path".into(), s(path.to_string_lossy()))]);
        let lines = [
            "{\"type\":\"user\",\"message\":{\"content\":\"왜 안 돼\"}}",
            "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Base directory\"}]}}",
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"content\":\"\"}]}}",
            "{\"type\":\"user\",\"message\":{\"content\":\"Stop hook feedback: 차단\"}}",
            "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"답\"}]}}",
        ];
        std::fs::write(&path, lines.join("\n")).unwrap();
        assert_eq!(super::last_user_prompt_from(&event), "왜 안 돼");
    }

    #[test]
    fn own_changes_skip_tool_edits_and_catch_products_and_data() {
        let tool = |name: &str, key: &str, value: &str| format!("{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"{name}\",\"input\":{{\"{key}\":\"{value}\"}}}}]}}}}\n");
        let free = tool("Edit", "file_path", "/home/tree/ai/scripts/task-complete/src/main.rs")
            + &tool("Write", "file_path", "/home/tree/.claude/projects/x/memory/a.md")
            + &tool("Bash", "command", "cp /tmp/claude-1000/a.json /tmp/claude-1000/b.json")
            + &tool("Bash", "command", "rg -n grist update ~/ai/x")
            + &tool("Bash", "command", "cd /home/tree/ai/skills/access-jev && python3 - <<'EOF'\\np='SKILL.md'; s=s.replace('a', '`.ai/docs` jev/src/main.rs rm x')\\nEOF\\nrm -f /tmp/claude-1000/s/req.md")
            + &tool("Bash", "command", "D=/tmp/claude-1000/s\\nprintf a > \\\"$D/req.md\\\"\\nrm -f \\\"${D:?}\\\"/*.md")
            + &tool("Bash", "command", "set d /tmp/claude-1000/s; mkdir -p $d; rm -f $d/x.md")
            + &tool("Bash", "command", "cd /tmp; d=$(mktemp -d); printf '{}' > $d/a.json; rm -rf $d")
            + &tool("Bash", "command", "cd /home/tree/ai/x && python3 - <<'EOF'\\nprint(1)\\nEOF\\nrm -f /tmp/claude-1000/a.md; ls -la ~/.codex/skills/x 2>/dev/null | head -2")
            + &tool("Write", "file_path", "/home/tree/plan.md")
            + &tool("Bash", "command", "ls -la /mnt/h/code/x 2>&1 | head; rg -n version /mnt/h/code/x/manifest.json 2>&1")
            + &tool("Bash", "command", "cd ~/ai/scripts/x/src; sed -i 's|a > b|`ls /mnt/h/x 2>&1` read|' main.rs; sed -i -e 's#x#/home/tree/extensions/y#' main.rs");
        assert!(super::own_changes(&free).is_empty(), "{:?}", super::own_changes(&free));
        assert!(super::own_change_reason(&super::own_changes(&free)).is_none());
        let product = free + &tool("Bash", "command", "cp /tmp/claude-1000/p.json /mnt/c/Users/tende/OneDrive/backup/n8n/a.json")
            + &tool("Bash", "command", "~/ai/scripts/bin/grist update Table7 --id 1221")
            + &tool("Edit", "file_path", "/home/tree/extensions/grist_mindmap/src/main.js")
            + &tool("Bash", "command", "cd /home/tree/extensions/grist_mindmap && rm -f src/old.js")
            + &tool("Bash", "command", "cd ~/ai && cp a.txt /home/tree/extensions/x/a.txt")
            + &tool("Bash", "command", "ls ~/ai; echo a > /home/tree/extensions/x/b.txt");
        let changed = super::own_changes(&product);
        assert_eq!(changed.len(), 6, "{changed:?}");
        assert!(super::own_change_reason(&changed).unwrap().contains("--role verifier"));
    }

    #[test]
    fn own_changes_ignores_quoted_search_text_and_tracks_intermediate_cd() {
        let bash = |command: &str| {
            let escaped = command.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
            format!("{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"Bash\",\"input\":{{\"command\":\"{escaped}\"}}}}]}}}}\n")
        };
        let scratch = "set -e; S=/tmp/claude-1000/x/scratchpad/tokstat; mkdir -p $S/src; cat > $S/Cargo.toml <<'EOF'\n[package]\nname = 'test'\nEOF\ncd $S && cargo build --release --offline 2>&1 | tail -n 5; cd $S && sed -i 's/a/b/' src/main.rs";
        let free = bash("rg -n \"let tee|tee = \" dispatch-agent/src/main.rs | head -3")
            + &bash("awk 'NR>3' /tmp/claude-1000/x/data.txt")
            + &bash(scratch);
        assert!(super::own_changes(&free).is_empty(), "{:?}", super::own_changes(&free));
        assert!(super::bash_targets_text("set -e; S=/tmp/claude-1000/x; cd $S && sed -i 's/a/b/' src/main.rs").contains("cd /tmp/claude-1000/x"));
        let cwd_entry = "{\"type\":\"assistant\",\"cwd\":\"/tmp/claude-1000/x/scratchpad\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Bash\",\"input\":{\"command\":\"sed -i 's/a/b/' src/main.rs\"}}]}}\n";
        assert!(super::own_changes(cwd_entry).is_empty());
        let external = free + &bash("cd /tmp/claude-1000/x && cp src/main.rs /mnt/c/Users/tende/Desktop/main.rs");
        assert_eq!(super::own_changes(&external).len(), 1);
    }

    #[test]
    fn unverified_report_blocks_unless_user_only() {
        assert!(super::unverified_report_reason("실제 환경에서는 차단 사례를 확인하지 못했어요.").unwrap().contains("확인하지 못"));
        assert!(super::unverified_report_reason("열린 행 차단은 단위 테스트로만 확인했어요.").is_some());
        assert!(super::unverified_report_reason("- [🏷] 로그인이 필요해 확인하지 못했어요.").is_none());
        assert!(super::unverified_report_reason("hook 환경에서 거부·통과를 확인했어요.").is_none());
        assert!(super::unverified_report_reason("- `실제 환경에서는 확인하지 못했어요.`는 막혔어요.").is_none());
        assert!(super::unverified_report_reason("보고에 \"확인하지 못\", \"단위 테스트로만\" 같은 줄이 있으면 막아요.").is_none());
    }

    #[test]
    fn early_apply_blocks_while_cause_rows_are_open() {
        let row = |id: i64, h: &str| Value::Obj(vec![("id".into(), Value::Num(id.to_string())), ("H".into(), s(h))]);
        let ask = "261110-3 파일을 n8n에 불러와 Publish해 주세요.";
        assert!(super::early_apply_reason(&[row(816, "교정")], ask).unwrap().contains("#816"));
        assert!(super::early_apply_reason(&[row(816, "진단"), row(817, "검증")], ask).unwrap().contains("#816"));
        assert!(super::early_apply_reason(&[row(816, "검증")], ask).is_none());
        assert!(super::early_apply_reason(&[row(816, "교정")], "대행자 결과를 기다려요.").is_none());
        assert!(super::early_apply_reason(&[row(816, "교정")], "1. Obsidian을 다시 불러와요(`Ctrl+R`).").is_none());
        assert!(super::early_apply_reason(&[row(816, "교정")], "\"n8n에 불러와 주세요\": 막힘").is_none());
    }

    #[test]
    fn tool_defect_report_blocks_missing_line_once_and_ignores_other_paths() {
        let base = std::env::temp_dir().join(format!("tc-defect-{}", std::process::id()));
        let _cleanup = Cleanup(base.clone());
        std::fs::create_dir_all(base.join("session_start")).unwrap();
        std::env::set_var("TASK_COMPLETE_STATE_DIR", &base);
        let tool = vec!["~/ai/scripts/dispatch-agent/src/main.rs".to_string()];
        let product = vec!["~/extensions/grist_mindmap/a.ts".to_string()];

        assert!(super::tool_defect_reason("d1", &product, "완료").is_none());
        assert!(super::tool_defect_reason("d1", &tool, "완료").unwrap().contains("작업 도구 결함:"));
        assert!(super::tool_defect_reason("d1", &tool, "완료").is_none());
        let more = vec![tool[0].clone(), "~/ai/codex/hooks.json".to_string()];
        assert!(super::tool_defect_reason("d1", &more, "완료").is_none());
        assert!(super::tool_defect_reason("d2", &tool, "작업 도구 결함: 창 누적 → 종료 시 닫음").is_none());
        assert!(super::tool_defect_reason("d2", &tool, "다음 턴").is_none());
        assert!(super::tool_defect_reason("d2", &more, "다음 턴").is_some());
        let ro_tp = base.join("ro.jsonl");
        std::fs::write(&ro_tp, "{\"type\":\"PLANNER_RESPONSE\",\"tool_calls\":[{\"name\":\"run_command\",\"args\":{\"CommandLine\":\"rg -n foo .\"}}]}\n").unwrap();
        let ro_ev = Value::Obj(vec![("transcript_path".into(), s(ro_tp.to_string_lossy()))]);
        assert!(!super::session_had_any_fix_action(&ro_ev));
        let rw_tp = base.join("rw.jsonl");
        std::fs::write(&rw_tp, "{\"type\":\"PLANNER_RESPONSE\",\"tool_calls\":[{\"name\":\"edit_file\",\"args\":{\"Path\":\"/home/tree/ai/scripts/x.rs\"}}]}\n").unwrap();
        let rw_ev = Value::Obj(vec![("transcript_path".into(), s(rw_tp.to_string_lossy()))]);
        assert!(super::session_had_any_fix_action(&rw_ev));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn unregistered_changes_block_once_and_skip_record_files() {
        use std::process::Command;
        let base = std::env::temp_dir().join(format!("tc-unreg-{}", std::process::id()));
        let _cleanup = Cleanup(base.clone());
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
        let snapshot = Value::Obj(vec![("repos".into(), Value::Arr(vec![Value::Obj(vec![("dir".into(), s(&dir)), ("tree".into(), s(&tree))])]))]);
        std::fs::write(&start, super::json::dumps(&snapshot)).unwrap();

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
        assert!(super::unregistered_reason("t1", &super::unregistered_changes("t1")).is_none());
        assert!(super::unregistered_changes("no-start").is_empty());
        assert!(super::unregistered_reason("t2", &["~/ai/skills/x/SKILL.md".to_string()]).is_none());
        let _ = std::fs::remove_dir_all(&base);
    }


    #[test]
    fn unverified_fix_report_and_diagnosis_ready_blocks_open_row_fix_claim() {
        println!("[verify-step] range-check start=3 mid=6 end=9");
        let diag = vec![Value::Obj(vec![("id".into(), s("865")), ("H".into(), s("진단")), ("D".into(), s("[🏷] 사용자 대기 — 865"))])];
        let paused = vec![Value::Obj(vec![("id".into(), s("865")), ("H".into(), s("진단")), ("D".into(), s("[⏸] 개입 #926"))])];
        let verified = vec![Value::Obj(vec![("id".into(), s("865")), ("H".into(), s("확인")), ("D".into(), s("[⌨️] 확인"))])];
        assert!(super::unverified_fix_report_reason(&diag, "- **실행한 수정**: 고침", false).is_some());
        assert!(super::unverified_fix_report_reason(&diag, "- **검증 결과**: 실패 [🏷]", false).is_none());
        assert!(super::unverified_fix_report_reason(&paused, "- **실행한 수정**: 개입 수정", false).is_none());
        assert!(super::unverified_fix_report_reason(&diag, "- **실행한 수정**: 개입 수정", true).is_none());
        assert!(super::unverified_fix_report_reason(&diag, "- **실행한 수정**: 고침\n개입: #938 → 복귀 #865", true).is_some());
        assert!(super::unverified_fix_report_reason(&verified, "- **실행한 수정**: 고침", false).is_none());
    }

}
