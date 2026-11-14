//! Stage gates (`gate`, `task-goal`, `task-red`, `task-ready`, `task-submit`, `task-hidden`, `flow-wait`).

use crate::api::{self, R, DONE_STAGE, GOAL_FIELD, SUBMITTED_STAGE, TASK_TABLE};
use crate::util;
use crate::records::{self, NEW_FEATURE};
use crate::task::{Step, TaskChain, TaskCtx};
use crate::task;
use crate::testrun;
use regex::{Regex, RegexBuilder};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const WAITING_STAGE: &str = "확인";
pub const DRAFT_STAGE: &str = "초안";
pub const ERROR_STAGE: &str = "진단";
pub const WORKING_STAGES: [&str; 3] = ["작업", "교정", "진단"];
pub const ALWAYS_EDITABLE: [&str; 3] = [".ai", "AGENTS.md", "test/tmp"];
pub const OUTPUT_TAIL: usize = 4000;
pub const PURPOSE_FORMAT: &str = "80자 이하의 결과 한 줄 (명령, 백틱, --옵션 제외)";
pub const PATHS_FORMAT: &str = "프로젝트 기준 최상위 경로 쉼표 구분 (`.` 제외)";
const BRIDGE_MARKERS: [&str; 7] = [
    "/api/bridge/", "localhost:41600", "127.0.0.1:41600", "/queries/",
    "/commands/", "eagle-data-bridge", "web_prompt_bridge",
];
const BRIDGE_SAMPLE_PATH: &str = "<bridge>/contracts/samples/<계약>/NN-<설명>.json";

fn confirmation_note(summary: &str) -> String {
    util::head(summary, 300)
}

/// The `$H=확인` write: the stage plus `$D` = the latest `#### 확인` summary (완료 표시 기호 한 줄) when one exists.
pub fn confirmed_fields(stage: &str, summary: Option<&str>) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("H".into(), Value::String(stage.into()));
    if let Some(summary) = summary {
        fields.insert("D".into(), Value::String(confirmation_note(summary)));
    }
    fields
}

pub(crate) fn flow_summary_from_text(text: &str, raw_id: i64) -> Option<String> {
    if matches!(task::row_status_in_text(text, raw_id), Some(status) if status != "검증") {
        return None;
    }
    let confirmed = format!("#### 확인 (#{raw_id})");
    let mut in_confirmed = false;
    let mut summary = None;
    for line in text.lines() {
        let line = line.trim();
        if line == confirmed {
            in_confirmed = true;
            summary = None;
        } else if line.starts_with('#') {
            in_confirmed = false;
        } else if in_confirmed {
            if let Some(value) = line.strip_prefix("- 요약 :") {
                let value = value.trim();
                if let Some(marked) = value.strip_prefix('[') {
                    if let Some((marker, path)) = marked.split_once(']') {
                        let path = path.trim();
                        if !marker.trim().is_empty()
                            && !path.is_empty()
                            && (path.contains('⇒') || path.contains('→') || path.contains("->"))
                        {
                            summary = Some(value.to_string());
                        }
                    }
                }
            }
        }
    }
    summary
}

pub fn flow_summary(project_dir: &Path, raw_id: i64) -> Option<String> {
    util::glob(&project_dir.join(".ai").join("changes"), "", ".md")
        .into_iter()
        .filter_map(|record| std::fs::read_to_string(record).ok())
        .filter_map(|text| flow_summary_from_text(&text, raw_id))
        .last()
}

#[derive(Clone, Debug)]
pub struct Row {
    pub id: i64,
    pub b: Value,
    pub h: Value,
    pub d: Value,
    pub i: Value,
    pub j: Value,
    pub k: String,
}

impl Row {
    pub fn h_str(&self) -> &str {
        self.h.as_str().unwrap_or("")
    }
    pub fn d_str(&self) -> Option<&str> {
        self.d.as_str()
    }
    pub fn i_str(&self) -> Option<&str> {
        self.i.as_str()
    }
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "H": self.h,
            "D": self.d,
            "I": self.i,
            "J": self.j,
            "K": self.k,
        })
    }
    pub fn to_gate_json(&self) -> Value {
        json!({
            "id": self.id,
            "H": self.h,
            "D": self.d,
            "I": self.i,
            "J": self.j,
        })
    }
}

/// Row of a `dispatch-agent` worker (`T26_ROLE=worker`, `T26_ROW=<id>`).
pub fn worker_row() -> Option<i64> {
    worker_row_from(std::env::var("T26_ROLE").ok().as_deref(), std::env::var("T26_ROW").ok().as_deref())
}

fn worker_row_from(role: Option<&str>, row: Option<&str>) -> Option<i64> {
    (role == Some("worker")).then_some(())?;
    row?.trim().parse().ok()
}

pub fn verifier_row() -> Option<i64> {
    verifier_row_from(std::env::var("T26_ROLE").ok().as_deref(), std::env::var("T26_ROW").ok().as_deref())
}

fn verifier_row_from(role: Option<&str>, row: Option<&str>) -> Option<i64> {
    (role == Some("verifier")).then_some(())?;
    row?.trim().parse().ok()
}

fn main_code_file(path: &str) -> bool {
    let path = Path::new(path);
    // The delegator owns workflow tools under ~/ai/scripts; only product code is delegated.
    if path.starts_with("/home/tree/ai/scripts") || path.starts_with("manage_decision/") || path.starts_with("manage_improve/") { return false; }
    if release_artifact(path) { return false; }
    if matches!(path.file_name().and_then(|name| name.to_str()), Some("Cargo.toml" | "Cargo.lock" | "package.json" | "manifest.json" | "blender_manifest.toml" | "runtime-contract.js" | "tauri.conf.json")) {
        return false;
    }
    let extension_source = !path.is_absolute() || records::workflow_root_of(path).is_some();
    extension_source
        && !path.components().any(|part| part.as_os_str() == ".ai")
        && path.file_name().and_then(|name| name.to_str()) != Some("AGENTS.md")
        && path.extension().and_then(|ext| ext.to_str()) != Some("md")
}

fn release_artifact(path: &Path) -> bool {
    path == Path::new("main.js") || path.starts_with("dist/")
}

fn delegate_boundary(role: Option<&str>, raw_id: i64, files: &[String]) -> Option<Value> {
    if role == Some("worker") || role == Some("verifier") || !files.iter().any(|file| main_code_file(file)) { return None; }
    Some(json!({
        "boundary": "delegate",
        "raw_id": raw_id,
        "changed": files.len(),
        "files": files,
        "reason": "main session changed a code file; delegate implementation with dispatch-agent"
    }))
}

pub fn actor() -> (String, Option<i64>) {
    if let Some(row) = worker_row() {
        return ("agent".into(), Some(row));
    }
    if let Some(row) = verifier_row() {
        return ("verifier".into(), Some(row));
    }
    if std::env::var("T26_ROLE").ok().as_deref() == Some("verifier") {
        return ("verifier".into(), None);
    }
    let Ok(thread) = std::env::var("CODEX_THREAD_ID") else {
        return ("main".into(), None);
    };
    if thread.is_empty() {
        return ("main".into(), None);
    }
    let sessions = util::home().join(".codex").join("sessions");
    let mut rollouts = Vec::new();
    let suffix = format!("-{thread}.jsonl");
    if let Ok(y_dirs) = std::fs::read_dir(&sessions) {
        for y in y_dirs.flatten() {
            if let Ok(m_dirs) = std::fs::read_dir(y.path()) {
                for m in m_dirs.flatten() {
                    if let Ok(d_dirs) = std::fs::read_dir(m.path()) {
                        for d in d_dirs.flatten() {
                            for f in util::glob(&d.path(), "rollout-", &suffix) {
                                rollouts.push(f);
                            }
                        }
                    }
                }
            }
        }
    }
    rollouts.sort();
    static AGENT_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let agent_re = AGENT_RE.get_or_init(|| Regex::new(r"(?:^|/)t26_(\d+)$").unwrap());
    for rollout in rollouts {
        let Ok(file) = std::fs::File::open(&rollout) else {
            continue;
        };
        let mut first_line = String::new();
        if std::io::BufRead::read_line(&mut std::io::BufReader::new(file), &mut first_line)
            .ok()
            .filter(|&n| n > 0)
            .is_none()
        {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Value>(first_line.trim()) else {
            continue;
        };
        let path = parsed
            .get("payload")
            .and_then(|p| p.get("agent_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(caps) = agent_re.captures(path) {
            if let Ok(id) = caps[1].parse::<i64>() {
                return ("agent".into(), Some(id));
            }
        }
        if !path.is_empty() {
            return ("helper".into(), None);
        }
    }
    ("main".into(), None)
}

pub fn require_main(command: &str) -> R<()> {
    let (role, row) = actor();
    if role != "main" {
        let suffix = row.map(|r| format!(" t26_{r}")).unwrap_or_default();
        return api::fail(
            "actor",
            format!("`{command}` is run by the main session only (caller: {role}{suffix})"),
        );
    }
    Ok(())
}

pub fn require_row(command: &str, raw_id: i64) -> R<()> {
    let (role, row) = actor();
    if !row_command_allowed(&role, row, std::env::var("T26_ROWS").ok().as_deref(), command, raw_id) {
        let suffix = row.map(|r| format!(" t26_{r}")).unwrap_or_default();
        return api::fail(
            "actor",
            format!("`{command} {raw_id}` is not allowed for caller: {role}{suffix}"),
        );
    }
    Ok(())
}

fn verifier_rows_contains(rows: Option<&str>, raw_id: i64) -> bool {
    rows.is_some_and(|rows| rows.split(',').any(|id| id.trim().parse::<i64>().ok() == Some(raw_id)))
}

fn row_command_allowed(role: &str, row: Option<i64>, rows: Option<&str>, command: &str, raw_id: i64) -> bool {
    if role == "helper" || (role == "agent" && row != Some(raw_id)) { return false; }
    if role == "verifier" {
        return (row == Some(raw_id) || verifier_rows_contains(rows, raw_id))
            && matches!(command, "task-hidden" | "task-step" | "task-show" | "goal" | "task");
    }
    true
}

pub fn worker_update_allowed(role: Option<&str>, row: Option<i64>, raw_id: i64, fields: &Map<String, Value>) -> bool {
    role != Some("worker") || (row == Some(raw_id) && fields.keys().all(|key| {
        matches!(key.as_str(), "C" | "D" | "I" | "J")
            || (key == "H" && matches!(fields.get(key).and_then(Value::as_str), Some("진단" | "교정")))
    }))
}

/// Ids among `ids` that still exist in Table26 (one lookup, no failure on missing rows).
fn existing_ids(ids: &[i64]) -> R<Vec<i64>> {
    if ids.is_empty() { return Ok(Vec::new()); }
    let raw = api::get(TASK_TABLE, Some(ids), None, Some(&["H".to_string()]))?;
    let present: std::collections::HashSet<i64> = api::rows_by_id(&raw).into_iter().map(|(id, _)| id).collect();
    Ok(ids.iter().copied().filter(|id| present.contains(id)).collect())
}

pub fn rows(ids: &[i64]) -> R<Vec<Row>> {
    let fields = vec!["B".into(), "H".into(), "D".into(), "I".into(), "J".into(), GOAL_FIELD.into()];
    let raw = api::get(TASK_TABLE, Some(ids), None, Some(&fields))?;
    let map: HashMap<i64, Map<String, Value>> = api::rows_by_id(&raw).into_iter().collect();
    let missing: Vec<i64> = ids.iter().copied().filter(|i| !map.contains_key(i)).collect();
    if !missing.is_empty() {
        let list_str = missing.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
        return api::fail("validation", format!("Table26 rows [{list_str}] do not exist"));
    }
    Ok(ids
        .iter()
        .map(|&i| {
            let f = &map[&i];
            let k = f
                .get(GOAL_FIELD)
                .filter(|v| util::truthy(Some(v)))
                .map(util::val_str)
                .unwrap_or_default();
            Row {
                id: i,
                b: f.get("B").cloned().unwrap_or(Value::Null),
                h: f.get("H").cloned().unwrap_or(Value::Null),
                d: f.get("D").cloned().unwrap_or(Value::Null),
                i: f.get("I").cloned().unwrap_or(Value::Null),
                j: f.get("J").cloned().unwrap_or(Value::Null),
                k,
            }
        })
        .collect())
}

pub fn git(project_dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-c")
        .arg("core.quotepath=off")
        .arg("-C")
        .arg(project_dir)
        .args(args)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

pub fn uninitialized(start: &Path) -> Option<PathBuf> {
    let root = records::workflow_root_of(&util::resolve(start)).unwrap_or_else(records::extensions_root);
    let ext = util::resolve(&root);
    let rel = start.strip_prefix(&ext).ok()?;
    let first = rel.components().next()?.as_os_str().to_string_lossy().into_owned();
    if first.is_empty() || first.starts_with('.') || first == "node_modules" || first == "test" {
        return None;
    }
    let project_dir = root.join(&first);
    let agents = project_dir.join("AGENTS.md");
    if !project_dir.is_dir() {
        return None;
    }
    if agents.is_file() {
        if let Ok(text) = std::fs::read_to_string(&agents) {
            if records::has_table7_id(&text).is_some() {
                return None;
            }
        }
    }
    let changed = git(&project_dir, &["status", "--porcelain", "--untracked-files=normal", "--", "."])?;
    if !util::strip(&changed).is_empty() {
        Some(project_dir)
    } else {
        None
    }
}

pub fn gate(cwd: &str, row: Option<i64>) -> R<Value> {
    let resolved_cwd = util::resolve(Path::new(cwd));
    let found = records::find_task_agents(&resolved_cwd);
    if let Some(r) = row {
        let fetched = rows(&[r])?;
        let stripped: Vec<Value> = fetched.iter().map(|x| x.to_gate_json()).collect();
        let proj_val = found.map(|(_, p)| Value::String(util::path_str(&p))).unwrap_or(Value::Null);
        return Ok(json!({
            "raw_ids": [r],
            "rows": stripped,
            "project_dir": proj_val
        }));
    }
    let Some((ids, project_dir)) = found else {
        let mut out = Map::new();
        out.insert("raw_ids".into(), json!([]));
        out.insert("rows".into(), json!([]));
        if let Some(u) = uninitialized(&resolved_cwd) {
            out.insert("uninitialized".into(), Value::String(util::path_str(&u)));
        }
        return Ok(Value::Object(out));
    };
    // 완료 뒤 Grist에서 지워진 행이 changes 작업 목록에 남아도 프로젝트 게이트 전체를 막지 않는다(261114 루트 #614·#926 등).
    let ids = existing_ids(&ids)?;
    let fetched = rows(&ids)?;
    let stripped: Vec<Value> = fetched.iter().map(|x| x.to_gate_json()).collect();
    Ok(json!({
        "raw_ids": ids,
        "rows": stripped,
        "project_dir": util::path_str(&project_dir)
    }))
}

// --- test records & gate json -----------------------------------------------

pub fn gate_path(project_dir: &Path, raw_id: i64) -> PathBuf {
    project_dir.join(".ai").join("gate").join(format!("{raw_id}.json"))
}

pub fn gate_load(project_dir: &Path, raw_id: i64) -> Map<String, Value> {
    std::fs::read_to_string(gate_path(project_dir, raw_id))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

pub fn gate_save(project_dir: &Path, raw_id: i64, data: &Map<String, Value>) -> R<()> {
    let path = gate_path(project_dir, raw_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(api::other)?;
    }
    std::fs::write(&path, format!("{}\n", util::pretty(&Value::Object(data.clone())))).map_err(api::other)?;
    Ok(())
}

pub fn check_purpose(text: &str) -> R<String> {
    let label = util::strip(text);
    let cmd_like = Regex::new(r"`|(?:^|\s)--?[A-Za-z]|[\n\r]").unwrap();
    if label.is_empty() || cmd_like.is_match(label) || label.chars().count() > 80 {
        return api::fail(
            "validation",
            format!("--purpose: {PURPOSE_FORMAT}"),
        );
    }
    Ok(label.to_string())
}

fn hidden_replay_command(command: &str) -> R<()> {
    // The hidden command runs again in flow-wait, in a fresh Stagehand session.
    // AI actions can depend on an extension world that was ready only in the
    // verifier's session; deterministic page actions can be replayed.
    if Regex::new(r"\.stagehand\s*\.\s*(?:act|observe|extract)\s*\(")
        .unwrap()
        .is_match(command)
    {
        return api::fail(
            "validation",
            "task-hidden --test: Stagehand AI actions are session-dependent; use session.page locator/click/evaluate for a replayable check",
        );
    }
    Ok(())
}

pub fn one_line(text: &str, project_dir: &Path) -> String {
    let proj_prefix = format!("{}/", util::path_str(project_dir));
    let home_str = util::path_str(&util::home());
    let replaced = text.replace(&proj_prefix, "").replace(&home_str, "~");
    let collapsed = util::collapse_space(&replaced);
    util::head(util::strip(&collapsed), 200)
}

pub fn failure_detail(output: &str, project_dir: &Path) -> String {
    let lines: Vec<String> = util::splitlines(output)
        .into_iter()
        .filter(|l| !util::strip(l).is_empty())
        .collect();
    let fail_at = [
        Regex::new(r"panicked at ([^\s:]+):(\d+):\d+:?\s*$").unwrap(),
        Regex::new(r#"File "([^"]+)", line (\d+)"#).unwrap(),
        Regex::new(r"\(?([^\s()]+\.[cm]?[jt]sx?):(\d+):\d+\)?").unwrap(),
        Regex::new(r"^([^\s:]+\.\w+):(\d+):").unwrap(),
    ];
    let fail_messages = [
        RegexBuilder::new(r"Assertion|assert").case_insensitive(true).build().unwrap(),
        RegexBuilder::new(r"Error|panicked|expected").case_insensitive(true).build().unwrap(),
        Regex::new(r"FAIL").unwrap(),
    ];
    let mut where_str = String::new();
    let mut message = String::new();
    for (index, line) in lines.iter().enumerate() {
        for (p_idx, pattern) in fail_at.iter().enumerate() {
            if let Some(caps) = pattern.captures(line) {
                where_str = format!("{}:{}", util::name_of(&caps[1]), &caps[2]);
                if p_idx == 0 && index + 1 < lines.len() {
                    message = lines[index + 1].clone();
                }
                break;
            }
        }
        if !message.is_empty() {
            break;
        }
    }
    if message.is_empty() {
        for pattern in &fail_messages {
            let found: Vec<&String> = lines
                .iter()
                .filter(|l| {
                    let ls = util::lstrip(l);
                    pattern.is_match(l) && !ls.starts_with("File ") && !ls.starts_with("Traceback")
                })
                .collect();
            if let Some(last) = found.last() {
                message = (*last).clone();
                break;
            }
        }
    }
    if message.is_empty() {
        message = lines.last().cloned().unwrap_or_else(|| "출력 없음".to_string());
    }
    one_line(&format!("{where_str} {message}"), project_dir)
}

pub fn test_record(title: &str, raw_id: i64, purpose: &str, ok: bool, detail: &str) -> String {
    let res = if ok { "성공" } else { "실패" };
    let det = if detail.is_empty() { "-" } else { detail };
    format!("\n#### {title} (#{raw_id})\n- 목적 : {purpose}\n- 결과 : {res}\n- 상세 : {det}\n")
}

pub fn preparation_of(state: &Map<String, Value>) -> Option<(&'static str, Map<String, Value>)> {
    for key in ["ready", "red"] {
        if let Some(data) = state.get(key).and_then(Value::as_object).filter(|v| !v.is_empty()) {
            return Some((key, data.clone()));
        }
    }
    None
}

fn preparation_data(project_dir: &Path, raw_id: i64) -> Option<(&'static str, Map<String, Value>)> {
    preparation_of(&gate_load(project_dir, raw_id))
}

fn check_item_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*-\s*\[([ xX])\]\s*(.+?)\s*$").unwrap())
}

pub fn evidence(lines: &[String], index: usize) -> Option<String> {
    static RES_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static DET_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let check_re = check_item_re();
    let res_re = RES_RE.get_or_init(|| Regex::new(r"^\s*-?\s*결과\s*:\s*(성공|실패)\s*$").unwrap());
    let det_re = DET_RE.get_or_init(|| Regex::new(r"^\s*-?\s*상세\s*:\s*(.+?)\s*$").unwrap());
    let mut result: Option<String> = None;
    let mut detail: Option<String> = None;
    for line in &lines[(index + 1)..] {
        if check_re.is_match(line) {
            break;
        }
        if let Some(caps) = res_re.captures(line) {
            result = Some(caps[1].to_string());
        } else if let Some(caps) = det_re.captures(line) {
            detail = Some(caps[1].to_string());
        }
    }
    match (result.as_deref(), detail) {
        (Some("성공"), Some(d)) if !d.is_empty() => Some(format!("결과 : 성공 / 상세 : {d}")),
        _ => None,
    }
}

// --- goal helpers -----------------------------------------------------------

fn goal_line_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*(?:-\s*|\d+\.\s*)\[(연결|기능|수정)\]\s*(.+?)\s*$").unwrap()
    })
}

pub fn goal_lines(goal: &str) -> Vec<(String, String)> {
    let re = goal_line_re();
    util::splitlines(goal)
        .into_iter()
        .filter_map(|l| {
            re.captures(&l).map(|c| {
                let kind = c.get(1).unwrap().as_str().to_string();
                (kind, c[2].to_string())
            })
        })
        .collect()
}

pub fn goal_errors(goal: &str) -> Vec<String> {
    if util::strip(goal).is_empty() {
        return vec!["empty goal".to_string()];
    }
    static LINE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let line_re = LINE.get_or_init(|| Regex::new(r"^(\d+\.\s*)?\[(연결|기능|수정)\]\s+\S").unwrap());
    let mut errors: Vec<String> = util::splitlines(goal)
        .iter()
        .map(|l| util::strip(l))
        .filter(|l| !l.is_empty() && !line_re.is_match(l))
        .map(|l| format!("goal line needs `N. [연결|기능|수정] <내용>`: {l}"))
        .collect();
    for (_, line) in goal_lines(goal) {
        if stop_goal_denies_own_hook(&line) {
            errors.push(format!("Stop 목표는 Stop 훅 자체의 미실행을 조건으로 삼을 수 없어요. 실행 여부 대신 훅 검증 기록 또는 판정 결과를 조건으로 쓰세요: {line}"));
        }
    }
    errors
}

/// A Stop decision necessarily runs task-complete. Reject a goal whose
/// precondition says that very execution is absent, before J3 can accept it.
fn stop_goal_denies_own_hook(line: &str) -> bool {
    if !line.contains("Stop") || !line.contains("없는") && !line.contains("않은") {
        return false;
    }
    static DENIED_EXECUTION: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = DENIED_EXECUTION.get_or_init(|| {
        Regex::new(r"(?:task-complete\s*실행|훅\s*실행)([^▷⇒]{0,140}?)(?:없는|않은)").unwrap()
    });
    re.captures_iter(line).any(|capture| {
        let condition = capture.get(1).unwrap().as_str();
        !condition.contains("기록") && !condition.contains("검증")
    })
}

pub fn goal_of(project_dir: &Path, row: &Row) -> R<String> {
    if let Some(goal) = records::changes_goal(project_dir, row.id) {
        return Ok(goal);
    }
    Ok(row.k.clone())
}

pub fn latest_request(project_dir: &Path) -> String {
    let text = records::latest_request_block(project_dir);
    util::tail(util::strip(&text), 6000)
}

// --- jev --------------------------------------------------------------------

fn run_jev_process(bin: &Path, command: &str, row: Option<i64>, file_args: &[(String, PathBuf)]) -> Option<(i32, String, String)> {
    let mut cmd = Command::new(bin);
    cmd.arg(command);
    for (flag, path) in file_args {
        cmd.arg(flag).arg(path);
    }
    if let Some(r) = row {
        cmd.arg("--row").arg(r.to_string());
    }
    let started = std::time::Instant::now();
    let output = cmd.output();
    crate::timing::jev_call(started.elapsed());
    let out = output.ok()?;
    Some((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

pub fn jev(command: &str, row: Option<i64>, texts: &[(&str, &str)]) -> Value {
    let bin = std::env::var_os("JEV_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| util::home().join("ai").join("scripts").join("bin").join("jev"));
    if !bin.is_file() {
        return json!({"ok": false, "boundary": "jev-missing"});
    }
    let tmp = std::env::temp_dir().join(format!("jev-{}-{}", std::process::id(), util::sanitize_key(command)));
    let _ = std::fs::remove_dir_all(&tmp);
    if std::fs::create_dir_all(&tmp).is_err() {
        return json!({"ok": false, "boundary": "jev-timeout"});
    }
    let mut file_args = Vec::new();
    for (name, text) in texts {
        let p = tmp.join(format!("{name}.txt"));
        let _ = std::fs::write(&p, text);
        file_args.push((format!("--{}-file", name.replace('_', "-")), p));
    }

    let parse_out = |(code, stdout, stderr): (i32, String, String)| -> Option<Value> {
        if code != 0 {
            let last_line = util::splitlines(util::strip(&stderr)).into_iter().last().unwrap_or_default();
            if let Ok(v) = serde_json::from_str::<Value>(&last_line) {
                return Some(v);
            }
            return Some(json!({"ok": false, "boundary": "jev", "error": util::tail(&stderr, 200)}));
        }
        serde_json::from_str::<Value>(&stdout).ok()
    };

    let result = run_jev_process(&bin, command, row, &file_args)
        .and_then(parse_out)
        .unwrap_or_else(|| json!({"ok": false, "boundary": "jev-timeout"}));

    let _ = std::fs::remove_dir_all(&tmp);
    result
}

pub fn jev_blocks(result: &Value) -> bool {
    util::truthy(result.get("ok"))
        && result.get("mode").and_then(|v| v.as_str()) == Some("enforce")
        && result.get("verdict").and_then(|v| v.as_str()) == Some("fail")
}

pub fn jev_failed(result: &Value) -> Vec<String> {
    result
        .get("items")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("verdict").and_then(|v| v.as_str()) == Some("fail"))
                .filter_map(|item| {
                    item.get("line")
                        .filter(|v| util::truthy(Some(v)))
                        .or_else(|| item.get("name"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn jev_summary(result: &Value) -> String {
    if !util::truthy(result.get("ok")) {
        let b = result.get("boundary").map(util::val_str).unwrap_or_else(|| "None".to_string());
        return format!("jev: {b} (판정 생략)");
    }
    let failed = jev_failed(result);
    let mode = result.get("mode").and_then(|v| v.as_str()).unwrap_or("");
    let verdict = result.get("verdict").and_then(|v| v.as_str()).unwrap_or("");
    let suffix = if !failed.is_empty() {
        format!(" — 불일치: {}", failed.join("; "))
    } else {
        String::new()
    };
    format!("jev {mode} {verdict}{suffix}")
}

// --- git scope --------------------------------------------------------------

/// Paths left out of the change count: build caches and the records `grist` itself writes
/// (`.ai/changes`, `.ai/gate`), which change on every grist command and say nothing about the work.
fn generated(path: &str) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(^|/)(__pycache__|\.pytest_cache|node_modules|\.mypy_cache)(/|$)|\.pyc$|(^|/)\.ai/(changes|gate)/").unwrap())
        .is_match(path)
}

fn child_project_claimed(project_dir: &Path, path: &str) -> bool {
    let Some((sub, rel)) = path.split_once('/') else { return false };
    let child_dir = project_dir.join(sub);
    if !child_dir.join("AGENTS.md").is_file() { return false; }
    let Ok(entries) = std::fs::read_dir(child_dir.join(".ai/gate")) else { return false };
    let mut allowed: Vec<String> = ALWAYS_EDITABLE.iter().map(|s| s.to_string()).collect();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<i64>().ok()) else { continue };
        let detail = preparation_data(&child_dir, stem).map(|(_, data)| data).unwrap_or_default();
        for key in ["paths", "tests"] {
            if let Some(arr) = detail.get(key).and_then(|v| v.as_array()) {
                allowed.extend(arr.iter().filter_map(|v| v.as_str()).map(String::from));
            }
        }
    }
    inside(rel, &allowed)
}

/// The working tree (tracked and untracked, ignore rules applied) as path, mode and blob IDs.
/// Objects written by `git add` live only in a temporary store, never in the real repository.
pub fn snapshot(project_dir: &Path) -> Option<String> {
    let index = git(project_dir, &["rev-parse", "--git-path", "index"])?;
    let index = project_dir.join(util::strip(&index));
    let repo_objects = git(project_dir, &["rev-parse", "--git-path", "objects"])?;
    let repo_objects = project_dir.join(util::strip(&repo_objects));
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let objects = std::env::temp_dir().join(format!("grist-snapshot-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&objects).ok()?;
    let tmp = objects.join("index");
    let run = |args: &[&str]| {
        Command::new("git").arg("-C").arg(project_dir).args(args).env("GIT_INDEX_FILE", &tmp)
            .env("GIT_OBJECT_DIRECTORY", &objects).env("GIT_ALTERNATE_OBJECT_DIRECTORIES", &repo_objects)
            .stdin(std::process::Stdio::null()).output().ok().filter(|o| o.status.success())
    };
    let result = (|| {
        if index.is_file() { std::fs::copy(&index, &tmp).ok()?; }
        run(&["add", "-A", "."])?;
        let listing = run(&["ls-files", "--stage", "-z", "--", "."])?;
        let mut files = BTreeMap::new();
        for entry in listing.stdout.split(|byte| *byte == 0).filter(|entry| !entry.is_empty()) {
            let line = String::from_utf8_lossy(entry);
            let (meta, path) = line.split_once('\t')?;
            files.insert(path.to_string(), meta.to_string());
        }
        serde_json::to_string(&files).ok()
    })();
    let _ = std::fs::remove_dir_all(&objects);
    result
}

/// Files (relative to the repo at `project_dir`) that differ between the preparation snapshot and now;
/// `external_changed` reuses it for packages outside the project.
/// Compares serialized index entries; temporary objects are gone before this returns.
pub fn changed_since(project_dir: &Path, prepared: &str) -> BTreeSet<String> {
    let before: BTreeMap<String, String> = if prepared.starts_with('{') {
        serde_json::from_str(prepared).unwrap_or_default()
    } else {
        // Snapshots prepared by the previous binary remain readable during an in-flight task.
        let prefix = git(project_dir, &["rev-parse", "--show-prefix"]).unwrap_or_default();
        let Some(output) = Command::new("git").arg("-C").arg(project_dir)
            .args(["ls-tree", "-r", "-z", "--full-tree", prepared])
            .output().ok().filter(|o| o.status.success()) else { return BTreeSet::new() };
        output.stdout.split(|byte| *byte == 0).filter_map(|entry| {
            let line = String::from_utf8_lossy(entry);
            let (meta, path) = line.split_once('\t')?;
            let (mode, rest) = meta.split_once(' ')?;
            let (_, oid) = rest.split_once(' ')?;
            let path = path.strip_prefix(util::strip(&prefix))?;
            Some((path.to_string(), format!("{mode} {oid} 0")))
        }).collect()
    };
    let Some(now) = snapshot(project_dir).and_then(|value| serde_json::from_str::<BTreeMap<String, String>>(&value).ok()) else { return BTreeSet::new() };
    before.keys().chain(now.keys()).filter(|path| before.get(*path) != now.get(*path) && !generated(path))
        .cloned().collect()
}

/// A submitted row's recorded file contents are a later baseline for other rows' outside paths.
/// Older submit records have no snapshot, so their preparation snapshot is the last exact baseline.
fn exclude_other_submissions(project_dir: &Path, raw_id: i64, ids: &[i64], own_allowed: &[String], changed: &mut BTreeSet<String>) {
    let now: BTreeMap<String, String> = snapshot(project_dir)
        .and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    for &other_id in ids {
        if other_id == raw_id { continue; }
        let state = gate_load(project_dir, other_id);
        let Some(submit) = state.get("submit").and_then(Value::as_object) else { continue };
        let submitted_snapshot = submit.get("snapshot").and_then(Value::as_str)
            .or_else(|| state.get("ready").or_else(|| state.get("red"))
                .and_then(|prepared| prepared.get("snapshot")).and_then(Value::as_str));
        // Keep the owned paths for this row in its change set, including changes made before
        // another row was submitted. Only outside paths can be assigned to the other row.
        let saved: BTreeMap<String, String> = submitted_snapshot
            .and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        changed.retain(|path| inside(path, own_allowed) || now.get(path) != saved.get(path));
    }
}

/// Packages outside the project that hold the row's test files, as (git repo root, package path relative to it).
/// The package is the nearest folder with `Cargo.toml` or `package.json` above the test file, inside its repo.
pub fn external_scopes(project_dir: &Path, tests: &[String]) -> Vec<(PathBuf, String)> {
    let mut scopes: Vec<(PathBuf, String)> = Vec::new();
    for test in tests {
        let path = Path::new(test);
        if !path.is_absolute() || path.starts_with(project_dir) { continue; }
        let Some(dir) = path.parent() else { continue };
        let Some(top) = git(dir, &["rev-parse", "--show-toplevel"]).map(|s| PathBuf::from(util::strip(&s))) else { continue };
        let package = dir.ancestors().take_while(|d| d.starts_with(&top))
            .find(|d| d.join("Cargo.toml").is_file() || d.join("package.json").is_file())
            .unwrap_or(dir);
        let scope = package.strip_prefix(&top).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        if !scopes.iter().any(|(r, s)| *r == top && *s == scope) { scopes.push((top, scope)); }
    }
    scopes
}

/// `external` record for ready/red: each outside package with its repo snapshot.
fn external_snapshots(project_dir: &Path, tests: &[String]) -> Value {
    Value::Array(external_scopes(project_dir, tests).into_iter()
        .filter_map(|(repo, scope)| snapshot(&repo).map(|tree| json!({"repo": util::path_str(&repo), "scope": scope, "snapshot": tree})))
        .collect())
}

/// Files changed since preparation inside the recorded outside packages (other packages of the same repo are ignored).
fn external_changed_files(prepared: &Map<String, Value>) -> Vec<String> {
    let Some(items) = prepared.get("external").and_then(Value::as_array) else { return Vec::new() };
    items.iter().flat_map(|item| {
        let (Some(repo), Some(scope), Some(tree)) = (
            item.get("repo").and_then(Value::as_str),
            item.get("scope").and_then(Value::as_str),
            item.get("snapshot").and_then(Value::as_str),
        ) else { return Vec::new() };
        let scope = vec![scope.to_string()];
        changed_since(Path::new(repo), tree).iter()
            .filter(|f| scope[0].is_empty() || inside(f, &scope))
            .map(|f| Path::new(repo).join(f).to_string_lossy().to_string()).collect::<Vec<_>>()
    }).collect()
}

fn is_test_file(path: &str) -> bool {
    let path = path.replace('\\', "/");
    let name = path.rsplit('/').next().unwrap_or("");
    path.split('/').any(|part| matches!(part, "test" | "tests" | "__tests__"))
        || name.starts_with("test_")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.ends_with("_test.rs")
        || name.ends_with("_test.py")
}

/// 폴더 이름 변경으로 미추적이 된 파일은 작업 트리에서 지워진 옛 경로(첫 폴더만 다름)의
/// HEAD 내용에 없던 줄만 돌려준다. 옛 경로가 없으면 None.
fn renamed_added_lines(project_dir: &Path, file: &Path, contents: &str) -> Option<String> {
    let git = |args: &[&str]| Command::new("git").arg("-C").arg(project_dir).args(args).output().ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned());
    let top = PathBuf::from(git(&["rev-parse", "--show-toplevel"])?.trim());
    let rel = file.canonicalize().ok()?.strip_prefix(top.canonicalize().ok()?).ok()?.to_path_buf();
    let tail: PathBuf = rel.components().skip(1).collect();
    if tail.as_os_str().is_empty() { return None; }
    let deleted = git(&["diff", "--name-only", "--diff-filter=D", "HEAD", "--", ":/"])?;
    let old = deleted.lines().find(|line| {
        let old = Path::new(line);
        old.components().skip(1).collect::<PathBuf>() == tail && old.components().next() != rel.components().next()
    })?;
    let previous = git(&["show", &format!("HEAD:{old}")])?;
    let previous: BTreeSet<&str> = previous.lines().collect();
    Some(contents.lines().filter(|line| !previous.contains(line)).collect::<Vec<_>>().join("\n"))
}

fn bridge_sample_boundary<'a>(
    row_label: &str,
    changed_sources: impl IntoIterator<Item = (&'a str, &'a str)>,
    test_contents: impl IntoIterator<Item = &'a str>,
) -> Option<Value> {
    if !row_label.starts_with("[🌍]") { return None; }
    let detected = changed_sources.into_iter().find_map(|(file, contents)| {
        BRIDGE_MARKERS.iter().find(|marker| contents.contains(**marker))
            .map(|marker| (file, *marker))
    });
    let (file, marker) = detected?;
    if test_contents.into_iter().any(|contents| contents.contains("contracts/samples/")) {
        return None;
    }
    Some(json!({
        "boundary": "submit",
        "reason": format!("Bridge consumer/producer 테스트가 {BRIDGE_SAMPLE_PATH} 파일을 읽어야 합니다. 샘플이 없으면 Bridge 프로젝트 행에서 producer 테스트와 함께 먼저 만든다"),
        "marker": marker,
        "file": file
    }))
}

fn bridge_sample_files(project_dir: &Path, row_label: &str, files: &[String], prepared: &Map<String, Value>) -> Option<Value> {
    if !row_label.starts_with("[🌍]") { return None; }
    let changed = prepared.get("snapshot").and_then(Value::as_str).filter(|s| !s.is_empty())
        .map(|snapshot| changed_since(project_dir, snapshot));
    let source_contents: Vec<(String, String)> = files.iter().filter(|path| !is_test_file(path))
        .filter(|path| Path::new(path).is_absolute() || changed.as_ref().is_none_or(|set| set.contains(*path)))
        .filter_map(|path| {
            let file = if Path::new(path).is_absolute() { PathBuf::from(path) } else { project_dir.join(path) };
            let contents = if Path::new(path).is_absolute() {
                std::fs::read_to_string(&file).ok()?
            } else {
                let diff = Command::new("git").arg("-C").arg(project_dir)
                    .args(["diff", "--unified=0", "HEAD", "--", path])
                    .output().ok().filter(|output| output.status.success())?;
                if diff.stdout.is_empty() {
                    let tracked = Command::new("git").arg("-C").arg(project_dir)
                        .args(["ls-files", "--error-unmatch", "--", path])
                        .output().ok().is_some_and(|output| output.status.success());
                    if tracked { String::new() } else {
                        let contents = std::fs::read_to_string(&file).ok()?;
                        renamed_added_lines(project_dir, &file, &contents).unwrap_or(contents)
                    }
                } else {
                    String::from_utf8_lossy(&diff.stdout).lines()
                        .filter_map(|line| line.strip_prefix('+').filter(|_| !line.starts_with("+++")))
                        .collect::<Vec<_>>().join("\n")
                }
            };
            Some((path.clone(), contents))
        }).collect();
    let mut test_paths: BTreeSet<String> = prepared.get("tests").and_then(Value::as_array)
        .into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
    test_paths.extend(files.iter().filter(|path| is_test_file(path)).cloned());
    let test_contents: Vec<String> = test_paths.iter().filter_map(|path| {
        let file = if Path::new(path).is_absolute() { PathBuf::from(path) } else { project_dir.join(path) };
        std::fs::read_to_string(file).ok()
    }).collect();
    bridge_sample_boundary(row_label,
        source_contents.iter().map(|(path, contents)| (path.as_str(), contents.as_str())),
        test_contents.iter().map(String::as_str))
}

/// Compare every table used by the row against its live schema at submit time.
fn grist_schema_boundary(project_dir: &Path, files: &[String], prepared: &Map<String, Value>) -> Option<Value> {
    grist_schema_boundary_with(project_dir, files, prepared, api::columns_for_doc)
}

fn grist_schema_boundary_with<F>(project_dir: &Path, _files: &[String], prepared: &Map<String, Value>, fetch: F) -> Option<Value>
where F: Fn(&str, &str) -> R<Value> {
    let endpoint = Regex::new(r"/tables/([A-Za-z][A-Za-z0-9_]*)/(?:records|columns)").unwrap();
    let mut tables = BTreeSet::new();
    let mut dynamic = false;
    let source_paths = project_source_paths(project_dir);
    let selected = prepared.get("purpose").and_then(Value::as_str).unwrap_or_default().to_string()
        + &prepared.get("tests").and_then(Value::as_array).into_iter().flatten()
            .filter_map(Value::as_str).filter_map(|p| std::fs::read_to_string(project_dir.join(p)).ok())
            .collect::<Vec<_>>().join("\n");
    for path in &source_paths {
        let file = if Path::new(path).is_absolute() { PathBuf::from(path) } else { project_dir.join(path) };
        let Ok(contents) = std::fs::read_to_string(&file) else { continue };
        for capture in endpoint.captures_iter(&contents) {
            if selected.contains(&capture[1]) {
                tables.insert(capture[1].to_string());
            }
        }
        dynamic |= contents.contains("/tables/${") && (contents.contains("/records") || contents.contains("/columns"));
    }
    if tables.is_empty() && !dynamic { return None; }
    let test_paths: Vec<&str> = prepared.get("tests").and_then(Value::as_array)
        .into_iter().flatten().filter_map(Value::as_str).collect();
    let tests = test_paths.iter().filter_map(|p| std::fs::read_to_string(project_dir.join(p)).ok())
        .collect::<Vec<_>>().join("\n");
    if dynamic && !tables.is_empty() {
        let sample_name = Regex::new(r"grist-([A-Za-z][A-Za-z0-9_]*)-columns\.json").unwrap();
        for capture in sample_name.captures_iter(&tests) { tables.insert(capture[1].to_string()); }
    }
    for table in tables {
        let sample = format!("tests/fixtures/grist-{table}-columns.json");
        let path = project_dir.join(&sample);
        let error = |reason: &str| json!({"boundary":"submit", "table":table, "sample":sample, "reason":reason});
        let Ok(raw) = std::fs::read_to_string(&path) else { return Some(error("실제 Grist columns 응답을 계약 샘플로 저장하세요")); };
        let Ok(fixture) = serde_json::from_str::<Value>(&raw) else { return Some(error("Grist 계약 샘플 JSON 형식 오류")); };
        let Some(doc) = fixture.get("docId").and_then(Value::as_str).filter(|s| !s.is_empty()) else { return Some(error("계약 샘플 docId 누락")); };
        if fixture.get("tableId").and_then(Value::as_str) != Some(table.as_str())
            || fixture.get("capturedAt").and_then(Value::as_str).is_none_or(str::is_empty) {
            return Some(error("계약 샘플 tableId 또는 capturedAt 누락"));
        }
        if !tests.contains(&sample) || !tests.contains("fields")
            || !["assert", "expect", "equal"].iter().any(|word| tests.contains(word)) {
            return Some(error("등록된 테스트가 Grist 계약 샘플을 읽어 실제 전송 fields를 단언해야 합니다"));
        }
        let Some(expected) = schema_columns(&fixture) else { return Some(error("계약 샘플 columns의 id/type 누락")); };
        let live = match fetch(doc, &table) {
            Ok(value) => value,
            Err(_) => return Some(error("실제 Grist GET columns 실패")),
        };
        if schema_columns(&live).as_ref() != Some(&expected) { return Some(error("계약 샘플과 실제 Grist 열·타입 불일치")); }
        let Some(fields) = fixture.pointer("/payload/records/0/fields").and_then(Value::as_object) else {
            return Some(error("계약 샘플 payload.records[0].fields 누락"));
        };
        for (key, value) in fields {
            let Some(kind) = expected.get(key) else { return Some(error("보내는 필드 키가 Grist 열에 없음")); };
            let valid = match kind.as_str() {
                "Int" | "Numeric" | "Date" | "DateTime" | "Ref" => value.is_number() || value.is_string(),
                "Bool" => value.is_boolean() || value.is_number(),
                "RefList" | "ChoiceList" => value.is_array(),
                "Text" | "Choice" => value.is_string(),
                _ => true,
            };
            if !valid { return Some(error("보내는 필드 값의 타입이 Grist 열 타입과 다름")); }
        }
    }
    None
}

fn schema_columns(value: &Value) -> Option<BTreeMap<String, String>> {
    let cols = value.get("columns")?.as_array()?;
    let mut map = BTreeMap::new();
    for col in cols {
        let id = col.get("id")?.as_str()?.to_string();
        let kind = col.pointer("/fields/type")?.as_str()?.to_string();
        if map.insert(id, kind).is_some() { return None; }
    }
    if map.is_empty() { None } else { Some(map) }
}

fn project_source_paths(project_dir: &Path) -> Vec<String> {
    // The extensions root owns workflow tooling outside this directory; children own product sources.
    if project_dir == Path::new("/home/tree/extensions") { return Vec::new(); }
    let Ok(output) = Command::new("rg").args(["--files", "--hidden", "-g", "!**/node_modules/**", "-g", "!**/target/**", "-g", "!**/dist/**", "-g", "!**/.git/**", "-g", "!**/.ai/**"]).current_dir(project_dir).output() else { return Vec::new() };
    if !output.status.success() { return Vec::new(); }
    String::from_utf8_lossy(&output.stdout).lines().filter(|path| {
        !is_test_file(path)
            && !path.split_once('/').is_some_and(|(child, _)| project_dir.join(child).join("AGENTS.md").is_file())
            && matches!(Path::new(path).extension().and_then(|s| s.to_str()), Some("js" | "cjs" | "mjs" | "ts" | "tsx" | "rs" | "py" | "sql" | "go" | "java" | "cs" | "php"))
    }).map(str::to_string).collect()
}

fn other_schema_boundary(project_dir: &Path, prepared: &Map<String, Value>) -> Option<Value> {
    let raw_sources = project_source_paths(project_dir).iter().filter_map(|p| std::fs::read_to_string(project_dir.join(p)).ok()).collect::<Vec<_>>().join("\n");
    let sources = raw_sources.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let test_paths: Vec<&str> = prepared.get("tests").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
    let tests = test_paths.iter().filter_map(|p| std::fs::read_to_string(project_dir.join(p)).ok()).collect::<Vec<_>>().join("\n");
    let selected = format!("{}\n{tests}", prepared.get("purpose").and_then(Value::as_str).unwrap_or_default());
    let sql = Regex::new(r#"(?i)\b(?:(?:SELECT\b[^;]{0,200}?\bFROM|DELETE\s+FROM|INSERT\s+(?:OR\s+[A-Za-z]+\s+)?INTO|(?:CREATE|ALTER)\s+TABLE)\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:"([A-Za-z_][A-Za-z0-9_]*)"|`([A-Za-z_][A-Za-z0-9_]*)`|([A-Za-z_][A-Za-z0-9_]*)\b)|UPDATE\s+(?:OR\s+[A-Za-z]+\s+)?(?:"([A-Za-z_][A-Za-z0-9_]*)"|`([A-Za-z_][A-Za-z0-9_]*)`|([A-Za-z_][A-Za-z0-9_]*))\s+SET\b)"#).unwrap();
    let mut tables = BTreeSet::new();
    for capture in sql.captures_iter(&sources) {
        if capture[0].to_ascii_lowercase().contains("import ") { continue; }
        if let Some(table) = (1..=6).find_map(|idx| capture.get(idx)).map(|m| m.as_str()) {
            if selected.contains(table) && !table.eq_ignore_ascii_case("select") { tables.insert(table.to_string()); }
        }
    }
    for table in tables {
        let sample = format!("tests/fixtures/sql-{table}-columns.json");
        let error = |reason: &str| json!({"boundary":"submit","table":table,"sample":sample,"reason":reason});
        let Ok(raw) = std::fs::read_to_string(project_dir.join(&sample)) else { return Some(error("SQL 테이블 계약 샘플 누락")); };
        let Ok(fixture) = serde_json::from_str::<Value>(&raw) else { return Some(error("SQL 계약 샘플 JSON 형식 오류")); };
        if !tests.contains(&sample) { return Some(error("등록된 테스트가 SQL 계약 샘플을 읽어야 합니다")); }
        let Some(command) = fixture.get("schemaCommand").and_then(Value::as_array).filter(|a| !a.is_empty()) else { return Some(error("SQL 실제 스키마 조회 명령 누락")); };
        let Some(parts) = command.iter().map(Value::as_str).collect::<Option<Vec<_>>>() else { return Some(error("SQL 스키마 조회 명령 형식 오류")); };
        let Ok(output) = Command::new(parts[0]).args(&parts[1..]).current_dir(project_dir).output() else { return Some(error("SQL 실제 스키마 조회 실패")); };
        if !output.status.success() { return Some(error("SQL 실제 스키마 조회 실패")); }
        let Ok(live) = serde_json::from_slice::<Value>(&output.stdout) else { return Some(error("SQL 스키마 조회 결과 JSON 형식 오류")); };
        if schema_columns(&fixture) != schema_columns(&live) || schema_columns(&live).is_none() { return Some(error("계약 샘플과 실제 SQL 열·타입 불일치")); }
    }
    let api = Regex::new(r"api-([A-Za-z][A-Za-z0-9_]*)-schema\.json").unwrap();
    let mut names = BTreeSet::new();
    for capture in api.captures_iter(&selected) {
        if sources.contains(&capture[1]) { names.insert(capture[1].to_string()); }
    }
    for name in names {
        let sample = format!("tests/fixtures/api-{name}-schema.json");
        let error = |reason: &str| json!({"boundary":"submit","table":name,"sample":sample,"reason":reason});
        let Ok(raw) = std::fs::read_to_string(project_dir.join(&sample)) else { return Some(error("외부 API 계약 샘플 누락")); };
        let Ok(fixture) = serde_json::from_str::<Value>(&raw) else { return Some(error("외부 API 계약 샘플 JSON 형식 오류")); };
        let Some(url) = fixture.get("schemaUrl").and_then(Value::as_str) else { return Some(error("외부 API 실제 스키마 조회 URL 누락")); };
        if !sources.contains(url) { return Some(error("외부 API 스키마 URL을 프로젝트 소스에서 참조하지 않음")); }
        let Ok(response) = ureq::get(url).call() else { return Some(error("외부 API 실제 스키마 조회 실패")); };
        let Ok(body) = response.into_string() else { return Some(error("외부 API 스키마 응답 읽기 실패")); };
        let Ok(live) = serde_json::from_str::<Value>(&body) else { return Some(error("외부 API 스키마 응답 JSON 형식 오류")); };
        if fixture.get("schema") != Some(&live) { return Some(error("계약 샘플과 실제 외부 API 스키마 불일치")); }
    }
    None
}

pub fn external_changed(prepared: &Map<String, Value>) -> usize {
    external_changed_files(prepared).len()
}

pub fn inside(path: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|p| {
        let p = p.trim_end_matches('/');
        path == p || path.starts_with(&format!("{p}/"))
    })
}

fn gate_err<T>(payload: Value) -> R<T> {
    let mut map = Map::new();
    map.insert("ok".into(), Value::Bool(false));
    if let Value::Object(obj) = payload {
        for (k, v) in obj {
            map.insert(k, v);
        }
    }
    api::gate_fail(Value::Object(map))
}

// --- stage gate commands ----------------------------------------------------

pub fn task_goal(raw_id: i64, project_arg: &str, goal_file: &str, summary: &str) -> R<Value> {
    require_main("task-goal")?;
    let (project_dir, _, _) = records::owned(project_arg, raw_id)?;
    let row = rows(&[raw_id])?.into_iter().next().unwrap();
    if row.h_str() != DRAFT_STAGE {
        return api::fail(
            "goal",
            format!("Table26 #{raw_id} $H={}; $K is written only while $H=초안", util::val_str(&row.h)),
        );
    }
    let item_title = records::item_file(&project_dir, raw_id)
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| records::item_title_in(&t, raw_id))
        .or_else(|| {
            let b = util::val_str(&row.b);
            (!b.is_empty()).then_some(b)
        });
    let (goal, summary) = check_goal_for_title(&project_dir, Some(raw_id), item_title.as_deref(), goal_file, summary)?;
    let mut out = store_goal(&project_dir, raw_id, &goal, &summary)?;
    records::attach_warnings(&mut out, records::goal_conjunction_warnings(&goal));
    Ok(out)
}

/// Pure one-line `$K` summary validation shared by `task-goal` and `task-add`.
pub fn goal_summary(summary: &str) -> Result<String, String> {
    let summary = util::strip(summary);
    if summary.is_empty() || summary.contains('\n') {
        return Err("--summary is the one-line $K goal; the detailed lines go in --goal-file".into());
    }
    Ok(summary.to_string())
}

/// Validates the goal before any Grist write: summary shape, goal format, J3 scope judgment.
/// Returns the canonical goal and the one-line summary.
pub fn check_goal(project_dir: &Path, raw_id: Option<i64>, goal_file: &str, summary: &str) -> R<(String, String)> {
    check_goal_for_title(project_dir, raw_id, None, goal_file, summary)
}

pub fn check_goal_for_title(
    project_dir: &Path,
    raw_id: Option<i64>,
    item_title: Option<&str>,
    goal_file: &str,
    summary: &str,
) -> R<(String, String)> {
    let summary = match goal_summary(summary) {
        Ok(s) => s,
        Err(e) => return api::fail("goal", e),
    };
    let raw_goal = std::fs::read_to_string(goal_file).map_err(api::other)?;
    let goal = records::canonical_goal(&raw_goal);
    let errors = goal_errors(&goal);
    if !errors.is_empty() {
        return gate_err(json!({"boundary": "goal", "raw_id": raw_id, "errors": errors}));
    }
    let request = raw_id.and_then(|id| records::item_file(project_dir, id))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| records::request_for_item_in(&text, item_title))
        .unwrap_or_else(|| latest_request(project_dir));
    let req = records::scope_request_block(&request, item_title);
    let judgment = jev("scope", raw_id, &[("request", &req), ("goal", &goal)]);
    if jev_blocks(&judgment) {
        return gate_err(json!({
            "boundary": "goal",
            "raw_id": raw_id,
            "reason": "각 행 목표는 판정 대상 # 요청 세부 사항 전체를 덮어야 해요. 행을 나누려면 # 요청에 독립된 '- ' 세부 사항이 2개 이상 있어야 하고, 행 제목이 해당 세부 사항과 일치해야 해요. 목표에 요청 밖 내용이 있어도 통과하지 않아요.",
            "jev": judgment.get("items").cloned().unwrap_or(Value::Null)
        }));
    }
    Ok((goal, summary))
}

/// Writes the checked goal to the changes item and `$K`.
pub fn store_goal(project_dir: &Path, raw_id: i64, goal: &str, summary: &str) -> R<Value> {
    let project_dir = project_dir.to_path_buf();
    let goal = goal.to_string();
    let summary = summary.to_string();
    if records::item_file(&project_dir, raw_id).is_none() {
        let fetched = api::get(TASK_TABLE, Some(&[raw_id]), None, Some(&["B".into(), "G".into()]))?;
        let fields = fetched
            .first()
            .and_then(|r| r.get("fields"))
            .and_then(|f| f.as_object())
            .cloned()
            .unwrap_or_default();
        let title = fields
            .get("B")
            .filter(|v| util::truthy(Some(v)))
            .map(util::val_str)
            .unwrap_or_else(|| format!("작업 {raw_id}"));
        let kind = if fields.get("G").and_then(|v| v.as_str()) == Some("오류") {
            "오류 해결"
        } else {
            NEW_FEATURE
        };
        records::register_item(&project_dir, raw_id, &title, kind, "")?;
    }
    records::write_changes_goal(&project_dir, raw_id, &goal)?;
    let mut upd = Map::new();
    upd.insert(GOAL_FIELD.to_string(), Value::String(summary.to_string()));
    api::update(TASK_TABLE, raw_id, &upd)
}

/// What task-red does about the agent origin line in 할당.
#[derive(Debug, PartialEq)]
pub enum OriginAction {
    /// The 할당 section already has the given agent ID.
    Skip,
    /// The agent ID is valid but its line is missing; warn and record nothing.
    Warn(char),
    /// No agent ID was given; record one new line.
    Record,
}

/// Agent character of an 11-character ID (6 project digits, 4 row digits, 1 agent char).
pub fn parse_agent_id(id: &str) -> Result<char, String> {
    let chars: Vec<char> = id.chars().collect();
    if chars.len() != 11 || !chars[..10].iter().all(char::is_ascii_digit) {
        return Err(format!("invalid agent ID {id}: need 6 project digits, 4 row digits and 1 agent character"));
    }
    let c = chars[10];
    if !(c.is_ascii_digit() || c.is_ascii_uppercase()) || c == 'I' || c == 'O' {
        return Err(format!("invalid agent ID {id}: agent character must be 0-9A-HJ-NP-Z"));
    }
    Ok(c)
}

/// Whether the row's `### 할당` already has an `<11자리 ID>` line for `agent`.
pub fn memo_has_agent_char(alloc: &[String], agent: char) -> bool {
    alloc.iter().any(|line| {
        let trimmed = line.trim();
        let body = match trimmed.split_once(". ") {
            Some((n, rest)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => rest.trim(),
            _ => trimmed,
        };
        let id = body.split_once(" · ").map_or(body, |(id, _)| id.trim());
        parse_agent_id(id) == Ok(agent)
    })
}

/// Decides skip / warn / record from the optional agent ID and the row's 할당.
pub fn origin_action(memo: &[String], agent_id: Option<&str>) -> Result<OriginAction, String> {
    match agent_id {
        None => Ok(OriginAction::Record),
        Some(id) => {
            let c = parse_agent_id(id)?;
            Ok(if memo_has_agent_char(memo, c) { OriginAction::Skip } else { OriginAction::Warn(c) })
        }
    }
}

/// Arguments for the record-log `start` call.
pub fn agent_start_args(project_dir: &str, raw_id: i64) -> Vec<String> {
    vec!["start".into(), project_dir.into(), raw_id.to_string(), "--role".into(), format!("t26_{raw_id}")]
}

/// Cargo manifest and release binary of record-log under the given HOME.
pub fn record_log_paths(home: &str) -> (String, String) {
    let base = format!("{home}/ai/skills/record-log/scripts/record-log");
    (format!("{base}/Cargo.toml"), format!("{base}/target/release/record-log"))
}

/// Applies the origin decision through the record-log CLI; failures only warn on stderr.
fn record_agent_origin(project_arg: &str, project_dir: &Path, raw_id: i64, agent_id: Option<&str>) {
    let memo: Vec<String> = records::task_show(project_arg, raw_id, Some("할당"))
        .ok()
        .and_then(|v| v.get("lines").and_then(|l| l.as_array()).cloned())
        .map(|a| a.iter().filter_map(|l| l.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    match origin_action(&memo, agent_id) {
        Err(e) => return eprintln!("warning: {e}; agent origin not recorded for #{raw_id}"),
        Ok(OriginAction::Skip) => return,
        Ok(OriginAction::Warn(c)) => {
            return eprintln!(
                "warning: 할당 of #{raw_id} has no agent {c} ID line; the caller must run record-log start; nothing recorded"
            )
        }
        Ok(OriginAction::Record) => {}
    }
    let args = agent_start_args(&util::path_str(project_dir), raw_id);
    let home = std::env::var("HOME").unwrap_or_default();
    let (manifest, built) = record_log_paths(&home);
    // A stale built binary can lack `start`; always refresh it first (no-op when current).
    match Command::new("cargo").args(["build", "--release", "--quiet", "--manifest-path", &manifest]).output() {
        Ok(b) if b.status.success() => {}
        // Read-only sandboxes (Codex worker) cannot rebuild; the existing binary still records.
        Ok(_) if Path::new(&built).is_file() => {}
        Ok(b) => return eprintln!(
            "warning: record-log build failed for #{raw_id}; agent origin not recorded: {}",
            String::from_utf8_lossy(&b.stderr).trim()
        ),
        Err(e) => return eprintln!("warning: cargo not run for #{raw_id}; agent origin not recorded: {e}"),
    }
    match Command::new(&built).args(&args).output() {
        Ok(out) if out.status.success() && !out.stdout.is_empty() => {
            let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
            eprintln!("agent-id: {id}");
        }
        Ok(out) if out.status.success() => eprintln!("warning: record-log start printed no agent ID for #{raw_id}"),
        Ok(out) => eprintln!(
            "warning: record-log start failed for #{raw_id}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(e) => eprintln!("warning: record-log start not run for #{raw_id}: {e}"),
    }
}

pub fn task_red(
    raw_id: i64,
    project_arg: &str,
    command: &str,
    paths: &[String],
    tests: &[String],
    purpose: &str,
    detail: &str,
    agent_id: Option<&str>,
) -> R<Value> {
    require_row("task-red", raw_id)?;
    let purpose = check_purpose(purpose)?;
    let (project_dir, _, _) = records::owned(project_arg, raw_id)?;
    let row = rows(&[raw_id])?.into_iter().next().unwrap();
    if row.h_str() != DRAFT_STAGE && !WORKING_STAGES.contains(&row.h_str()) {
        return api::fail(
            "red",
            format!(
                "Table26 #{raw_id} $H={}; task-red needs 초안 or one of {}",
                util::val_str(&row.h),
                WORKING_STAGES.join(", ")
            ),
        );
    }
    let goal = goal_of(&project_dir, &row)?;
    if util::strip(&goal).is_empty() {
        return api::fail(
            "red",
            format!("Table26 #{raw_id} has no goal; the main session writes it with task-goal first"),
        );
    }
    record_agent_origin(project_arg, &project_dir, raw_id, agent_id);
    for name in tests {
        if !project_dir.join(name).is_file() {
            return api::fail(
                "red",
                format!("test file {name} does not exist under {}", util::path_str(&project_dir)),
            );
        }
    }
    let run = testrun::run(command, &project_dir);
    let output = run.output.clone();
    if run.code == 0 {
        return gate_err(json!({
            "boundary": "red",
            "raw_id": raw_id,
            "reason": "the test passes; red needs a failing assertion",
            "test": command
        }));
    }
    let missing_skel = Regex::new(
        r"ModuleNotFoundError|ImportError|SyntaxError|command not found|No such file|is not defined|Cannot find module",
    )
    .unwrap();
    if missing_skel.is_match(&output) {
        return gate_err(json!({
            "boundary": "red",
            "raw_id": raw_id,
            "reason": "the test fails before any assertion; write the function skeleton first",
            "output": output
        }));
    }
    let red = json!({
        "purpose": purpose,
        "test": command,
        "tests": tests,
        "paths": paths,
        "snapshot": snapshot(&project_dir),
        "external": external_snapshots(&project_dir, tests)
    });
    let det_one = one_line(detail, &project_dir);
    let rec_detail = if !det_one.is_empty() {
        format!("{det_one} {}", run.suffix())
    } else {
        format!("{} {}", failure_detail(&output, &project_dir), run.suffix())
    };
    let mut ctx = TaskCtx {
        project_dir: Some(project_dir),
        raw_id,
        stage: row.h_str().to_string(),
        prep_kind: "red",
        prepared: red.as_object().cloned().unwrap_or_default(),
        purpose,
        detail: rec_detail,
        result: json!({"id": raw_id, "H": row.h}),
        ..Default::default()
    };
    TaskChain::standard(Step::Ready).run(Step::Ready, &mut ctx)?;
    ctx.result.as_object_mut().unwrap().insert("next".into(), json!(task::STAGE_WORK_CARD));
    Ok(ctx.result)
}

pub fn diagnosis_ready_error(h: &str, j: &str) -> Option<&'static str> {
    if h != ERROR_STAGE { return None; }
    if j.trim().is_empty() || j.contains("[추정]") {
        return Some("H=진단 행의 원인이 비어 있거나 [추정]이면 task-ready(원인 확정)를 쓸 수 없어요; task-red로 실패 재현 테스트를 먼저 등록하세요");
    }
    if j.contains("단위 테스트만") || j.contains("유닛테스트만") || j.contains("유닛 테스트만") {
        return Some("H=진단 행의 원인이 유닛테스트만으로 작성되었어요; 실제 런타임(CDP)에서 에러를 재현한 근거를 기록한 뒤 진행하세요");
    }
    None
}

pub fn task_ready(
    raw_id: i64,
    project_arg: &str,
    command: &str,
    paths: &[String],
    tests: &[String],
    purpose: &str,
    agent_id: Option<&str>,
) -> R<Value> {
    require_row("task-ready", raw_id)?;
    let purpose = check_purpose(purpose)?;
    let (project_dir, _, _) = records::owned(project_arg, raw_id)?;
    let row = rows(&[raw_id])?.into_iter().next().unwrap();
    if row.h_str() != DRAFT_STAGE && !WORKING_STAGES.contains(&row.h_str()) {
        return api::fail(
            "ready",
            format!("Table26 #{raw_id} $H={}; task-ready needs 초안 or one of {}", util::val_str(&row.h), WORKING_STAGES.join(", ")),
        );
    }
    if let Some(reason) = diagnosis_ready_error(row.h_str(), &util::val_str(&row.j)) {
        return api::fail("ready", reason);
    }
    let goal = goal_of(&project_dir, &row)?;
    if util::strip(&goal).is_empty() {
        return api::fail("ready", format!("Table26 #{raw_id} has no goal; the main session writes it with task-goal first"));
    }
    if command.trim().is_empty() || paths.is_empty() || tests.is_empty() || paths.iter().any(|p| p.trim().is_empty() || p == ".") {
        return api::fail("ready", format!("task-ready needs a focused --test command, --tests files, and --paths: {PATHS_FORMAT}"));
    }
    for name in tests {
        if !project_dir.join(name).is_file() {
            return api::fail("ready", format!("test file {name} does not exist under {}", util::path_str(&project_dir)));
        }
    }
    record_agent_origin(project_arg, &project_dir, raw_id, agent_id);
    let ready = json!({
        "purpose": purpose,
        "test": command,
        "tests": tests,
        "paths": paths,
        "snapshot": snapshot(&project_dir),
        "external": external_snapshots(&project_dir, tests)
    });
    let mut ctx = TaskCtx {
        project_dir: Some(project_dir),
        raw_id,
        stage: row.h_str().to_string(),
        prep_kind: "ready",
        prepared: ready.as_object().cloned().unwrap_or_default(),
        purpose,
        detail: "원인 확정: 집중 테스트 등록(제출 시 실행)".into(),
        result: json!({"id": raw_id, "H": row.h}),
        ..Default::default()
    };
    TaskChain::standard(Step::Ready).run(Step::Ready, &mut ctx)?;
    ctx.result.as_object_mut().unwrap().insert("next".into(), json!(task::STAGE_WORK_CARD));
    Ok(ctx.result)
}

pub fn task_submit(raw_id: i64, project_arg: &str) -> R<Value> {
    require_row("task-submit", raw_id)?;
    let (project_dir, _, ids) = records::owned(project_arg, raw_id)?;
    let row = rows(&[raw_id])?.into_iter().next().unwrap();
    if !WORKING_STAGES.contains(&row.h_str()) {
        return api::fail(
            "validation",
            format!(
                "Table26 #{raw_id} $H={}; task-submit needs one of {}",
                util::val_str(&row.h),
                WORKING_STAGES.join(", ")
            ),
        );
    }
    let Some((preparation_kind, prepared)) = preparation_data(&project_dir, raw_id) else {
        return gate_err(json!({
            "boundary": "submit",
            "raw_id": raw_id,
            "reason": "no preparation record; run task-ready for a confirmed cause or task-red when the cause needs reproduction"
        }));
    };
    records::check_goal_steps(&project_dir, raw_id)?;
    let test_cmd = prepared.get("test").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let purpose = prepared
        .get("purpose")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("제출 테스트")
        .to_string();
    let run = testrun::run(&test_cmd, &project_dir);
    if run.code != 0 {
        records::set_impl_status(&project_dir, raw_id, "초안")?;
        return gate_err(json!({
            "boundary": "submit",
            "raw_id": raw_id,
            "exit": run.code,
            "ms": run.ms,
            "output": run.output
        }));
    }
    let own_allowed: Vec<String> = prepared.get("paths").and_then(|v| v.as_array())
        .into_iter().flatten().filter_map(|v| v.as_str()).map(String::from).collect();
    let mut changed: BTreeSet<String> = prepared.get("snapshot").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
        .map(|prepared_tree| changed_since(&project_dir, prepared_tree))
        .unwrap_or_default()
        .into_iter()
        .filter(|f| inside(f, &own_allowed) || !child_project_claimed(&project_dir, f))
        .collect();
    exclude_other_submissions(&project_dir, raw_id, &ids, &own_allowed, &mut changed);
    if prepared.get("snapshot").and_then(|v| v.as_str()).is_some_and(|s| !s.is_empty()) {
        let mut allowed: Vec<String> = ALWAYS_EDITABLE.iter().map(|s| s.to_string()).collect();
        let mut test_files: Vec<String> = Vec::new();
        for &other_id in &ids {
            let detail = preparation_data(&project_dir, other_id).map(|(_, data)| data).unwrap_or_default();
            if let Some(p_arr) = detail.get("paths").and_then(|v| v.as_array()) {
                for p in p_arr.iter().filter_map(|v| v.as_str()) {
                    allowed.push(p.to_string());
                }
            }
            if let Some(arr) = detail.get("tests").and_then(|v| v.as_array()) {
                for k in arr.iter().filter_map(|v| v.as_str()) {
                    test_files.push(k.to_string());
                }
            }
        }
        let mut all_allowed = allowed.clone();
        all_allowed.extend(test_files);
        let outside: Vec<String> = changed.iter().filter(|f| {
            !inside(f, &all_allowed)
                // 릴리스가 만든 루트 dist/와 main.js는 재제출 때 행별 수정 범위를 막지 않는다.
                && !release_artifact(Path::new(f))
                && !matches!(Path::new(f).file_name().and_then(|n| n.to_str()), Some("Cargo.toml" | "Cargo.lock" | "package.json" | "manifest.json" | "blender_manifest.toml" | "runtime-contract.js" | "tauri.conf.json" | "main.js"))
        }).cloned().collect();
        if !outside.is_empty() {
            return gate_err(json!({
                "boundary": "submit",
                "raw_id": raw_id,
                "reason": "files changed outside the editable paths",
                "files": outside
            }));
        }
    }

    let mut files: Vec<String> = changed.iter().cloned().collect();
    files.extend(external_changed_files(&prepared));
    if let Some(boundary) = bridge_sample_files(&project_dir, row.b.as_str().unwrap_or(""), &files, &prepared) {
        return gate_err(boundary);
    }
    if let Some(boundary) = grist_schema_boundary(&project_dir, &files, &prepared) {
        return gate_err(boundary);
    }
    if let Some(boundary) = other_schema_boundary(&project_dir, &prepared) {
        return gate_err(boundary);
    }
    let changed_count = files.len();
    if let Some(boundary) = delegate_boundary(std::env::var("T26_ROLE").ok().as_deref(), raw_id, &files) {
        return gate_err(boundary);
    }
    let mut ctx = TaskCtx {
        project_dir: Some(project_dir),
        raw_id,
        stage: row.h_str().to_string(),
        prep_kind: preparation_kind,
        prepared,
        purpose,
        detail: format!("집중 테스트 통과 {}", run.suffix()),
        test: test_cmd,
        output: run.output,
        changed: changed_count,
        ..Default::default()
    };
    TaskChain::standard(Step::Submit).run(Step::Submit, &mut ctx)?;
    Ok(ctx.result)
}

pub fn submitted_and_hidden(project_dir: &Path, raw_id: i64) -> (Option<Map<String, Value>>, Vec<Map<String, Value>>) {
    let state = gate_load(project_dir, raw_id);
    let Some(submit) = state.get("submit").and_then(|v| v.as_object()).filter(|o| !o.is_empty()) else {
        return (None, Vec::new());
    };
    let sub_test = submit.get("test").and_then(|v| v.as_str());
    let hidden: Vec<Map<String, Value>> = state
        .get("hidden")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_object())
                .filter(|h| h.get("test").and_then(|v| v.as_str()) != sub_test)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    (Some(submit.clone()), hidden)
}

pub fn task_hidden(raw_id: i64, project_arg: &str, purpose: &str, command: &str, detail: &str) -> R<Value> {
    if actor().0 == "verifier" { require_row("task-hidden", raw_id)?; }
    else { require_main("task-hidden")?; }
    let purpose = check_purpose(purpose)?;
    hidden_replay_command(command)?;
    let (project_dir, _, _) = records::owned(project_arg, raw_id)?;
    let row = rows(&[raw_id])?.into_iter().next().unwrap();
    if row.h_str() != SUBMITTED_STAGE {
        return api::fail(
            "validation",
            format!("Table26 #{raw_id} $H={}; task-hidden needs 검증", util::val_str(&row.h)),
        );
    }
    let (submit, mut hidden) = submitted_and_hidden(&project_dir, raw_id);
    let Some(submit) = submit else {
        return gate_err(json!({
            "boundary": "hidden",
            "raw_id": raw_id,
            "reason": "no submit record; run task-submit first"
        }));
    };
    if submit.get("test").and_then(|v| v.as_str()) == Some(command) {
        return gate_err(json!({
            "boundary": "hidden",
            "raw_id": raw_id,
            "reason": "the hidden case must differ from the submitted test"
        }));
    }
    let run = testrun::run(command, &project_dir);
    let (code, output) = (run.code, run.output.clone());
    let mut state = gate_load(&project_dir, raw_id);
    let mut new_h = Map::new();
    new_h.insert("purpose".into(), Value::String(purpose.clone()));
    new_h.insert("test".into(), Value::String(command.to_string()));
    hidden.push(new_h);
    state.insert("submit".into(), Value::Object(submit));
    state.remove("flow_wait");
    state.insert(
        "hidden".into(),
        Value::Array(hidden.into_iter().map(Value::Object).collect()),
    );
    gate_save(&project_dir, raw_id, &state)?;
    let ok = code == 0;
    let det_one = one_line(detail, &project_dir);
    let rec_detail = if !det_one.is_empty() {
        format!("{det_one} {}", run.suffix())
    } else if ok {
        format!("통과 {}", run.suffix())
    } else {
        format!("{} {}", failure_detail(&output, &project_dir), run.suffix())
    };
    records::append_item(&project_dir, raw_id, &test_record("숨긴 사례", raw_id, &purpose, ok, &rec_detail))?;
    if !ok {
        return gate_err(json!({
            "boundary": "hidden",
            "raw_id": raw_id,
            "purpose": purpose,
            "exit": code,
            "ms": run.ms,
            "output": output
        }));
    }
    if actor().0 == "verifier" {
        state.insert("verifier_recorded".into(), json!(true));
        gate_save(&project_dir, raw_id, &state)?;
    }
    Ok(json!({"id": raw_id, "hidden": purpose, "result": "성공"}))
}

fn flow_submitted_ids(rows: &[Row]) -> Vec<i64> {
    rows.iter().filter(|row| row.h_str() == SUBMITTED_STAGE).map(|row| row.id).collect()
}

fn flow_skipped(rows: &[Row]) -> Vec<Value> {
    let allowed_flow = [SUBMITTED_STAGE, WAITING_STAGE, DONE_STAGE];
    rows.iter().filter(|row| !allowed_flow.contains(&row.h_str()))
        .map(|row| json!({"id": row.id, "H": row.h})).collect()
}

fn active_flow_ids(ids: Vec<i64>, current_open: &[i64]) -> Vec<i64> {
    if current_open.is_empty() { ids } else { ids.into_iter().filter(|id| current_open.contains(id)).collect() }
}

pub fn flow_wait(project_arg: &str, only_row: Option<i64>) -> R<Value> {
    require_main("flow-wait")?;
    let (project_dir, _, ids) = records::project(project_arg)?;
    if let Some(row_id) = only_row {
        if !ids.contains(&row_id) {
            return api::fail("flow", format!("#{row_id} does not belong to {}", project_dir.display()));
        }
    }
    let current_open = task::open_ids(&project_dir);
    let active_ids = match only_row {
        Some(row_id) => vec![row_id],
        None => active_flow_ids(ids, &current_open),
    };
    let fetched = rows(&active_ids)?;
    let submitted_ids = flow_submitted_ids(&fetched);
    let skipped = flow_skipped(&fetched);
    if submitted_ids.is_empty() {
        return gate_err(json!({"boundary": "flow", "reason": "no submitted rows", "skipped": skipped}));
    }

    let mut failed = Vec::new();
    let mut missing_hidden = Vec::new();
    for row in &fetched {
        if row.h_str() != SUBMITTED_STAGE {
            continue;
        }
        let (submitted, hidden) = submitted_and_hidden(&project_dir, row.id);
        let Some(last_hidden) = hidden.last() else {
            missing_hidden.push(row.id);
            continue;
        };
        let fallback_sub = {
            let mut m = Map::new();
            m.insert("purpose".into(), Value::String("제출 기록 없음".into()));
            m.insert("test".into(), Value::Null);
            m
        };
        for case in [submitted.as_ref().unwrap_or(&fallback_sub), last_hidden] {
            let cmd_opt = case.get("test").and_then(|v| v.as_str());
            let (code_opt, output, took) = match cmd_opt {
                Some(c) => {
                    let run = testrun::run(c, &project_dir);
                    let took = run.suffix();
                    (Some(run.code), run.output, took)
                }
                None => (None, "no submit record".to_string(), String::new()),
            };
            if code_opt != Some(0) {
                let purpose_str = case.get("purpose").map(util::val_str).unwrap_or_default();
                let note = format!(
                    "{}. 메인 재실행 실패: {} / {} {took}",
                    records::next_number(row.i_str()),
                    purpose_str,
                    failure_detail(&output, &project_dir)
                );
                let mut upd = Map::new();
                upd.insert("H".into(), Value::String(ERROR_STAGE.into()));
                upd.insert("I".into(), Value::String(util::head(&note, 300)));
                api::update(TASK_TABLE, row.id, &upd)?;
                failed.push(json!({
                    "id": row.id,
                    "test": cmd_opt,
                    "exit": code_opt,
                    "output": output,
                    "next": task::STAGE_START_CARD
                }));
                break;
            }
        }
    }
    if !missing_hidden.is_empty() {
        return gate_err(json!({
            "boundary": "flow",
            "reason": "each 검증 row needs a hidden case after task-submit (`- 상태 : 검증`): `task-hidden N --purpose LABEL --test CMD`",
            "rows": missing_hidden,
            "skipped": skipped
        }));
    }
    if !failed.is_empty() {
        return gate_err(json!({
            "boundary": "flow",
            "reason": format!("submitted or hidden tests failed; rows set to {ERROR_STAGE}"),
            "next": task::STAGE_START_CARD,
            "rows": failed,
            "skipped": skipped
        }));
    }

    let mut result = Vec::new();
    let mut missing = Vec::new();
    for id in submitted_ids {
        if let Some(item) = flow_wait_missing(&project_dir, id) {
            missing.push(json!({"id": id, "missing": item}));
        } else {
            result.push(json!({"id": id, "flow_wait": true}));
            let mut state = gate_load(&project_dir, id);
            state.insert("flow_wait".into(), Value::Bool(true));
            gate_save(&project_dir, id, &state)?;
        }
    }
    if !result.is_empty() {
        records::refresh_day_version(&project_dir)?;
    }
    if !missing.is_empty() {
        return gate_err(json!({"boundary": "flow", "passed": result.iter().filter_map(|v| v["id"].as_i64()).collect::<Vec<_>>(), "missing": missing, "skipped": skipped}));
    }
    Ok(json!({"passed": result.iter().filter_map(|v| v["id"].as_i64()).collect::<Vec<_>>(), "skipped": skipped}))
}

pub(crate) fn flow_wait_missing(project_dir: &Path, raw_id: i64) -> Option<&'static str> {
    let has_block = util::glob(&project_dir.join(".ai").join("changes"), "", ".md")
        .into_iter().filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|text| {
            let confirmation = format!("#### 확인 (#{raw_id})");
            task::row_has_verified_status(&text, raw_id) && text.contains(&confirmation)
        });
    if !has_block { return Some("확인 블록"); }
    if flow_summary(project_dir, raw_id).is_none() { return Some("요약"); }
    let confirmation = util::glob(&project_dir.join(".ai").join("changes"), "", ".md")
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|text| task::flow_confirmation_in_text(&text, raw_id).ok().flatten())
        .last();
    let Some(confirmation) = confirmation else { return Some("근거"); };
    if !confirmation.lines().any(|line| line.starts_with("- 근거 ")) { return Some("근거"); }
    if !task::partial_confirmation(&confirmation)
        && task::runtime_evidence_marker(project_dir, raw_id, &confirmation).is_some() {
        return Some("근거");
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn grist_schema_submit_rejects_missing_stale_and_wrong_payload_and_accepts_matching_live_columns() {
        use serde_json::json;
        let project = std::env::temp_dir().join(format!("grist-schema-{}", std::process::id()));
        let _cleanup = Cleanup(project.clone());
        std::fs::create_dir_all(project.join("tests/fixtures")).unwrap();
        std::fs::write(project.join("client.js"), "fetch('/tables/Table8/records');").unwrap();
        std::fs::write(project.join("tests/schema.test.js"), "const schema = require('tests/fixtures/grist-Table8-columns.json'); assert(fields === schema.payload.records[0].fields);").unwrap();
        let files = vec!["client.js".to_string()];
        let prepared = json!({"tests":["tests/schema.test.js"]}).as_object().unwrap().clone();
        let live = json!({"columns":[{"id":"Title","fields":{"type":"Text"}},{"id":"Count","fields":{"type":"Int"}}]});
        let check = |v: serde_json::Value| super::grist_schema_boundary_with(&project, &files, &prepared, |_, _| Ok(v.clone()));
        assert!(check(live.clone()).is_some());
        let sample = project.join("tests/fixtures/grist-Table8-columns.json");
        let fixture = json!({"docId":"doc123","tableId":"Table8","capturedAt":"2026-11-12T10:00:00Z","columns":live["columns"],"payload":{"records":[{"fields":{"Title":"ok","Count":3}}]}});
        std::fs::write(&sample, fixture.to_string()).unwrap();
        assert!(check(json!({"columns":[{"id":"Title","fields":{"type":"Text"}}]})).is_some());
        assert!(check(json!({"columns":[{"id":"Title","fields":{"type":"Text"}},{"id":"Count","fields":{"type":"Numeric"}}]})).is_some());
        let wrong = json!({"docId":"doc123","tableId":"Table8","capturedAt":"2026-11-12T10:00:00Z","columns":live["columns"],"payload":{"records":[{"fields":{"Title":42}}]}});
        std::fs::write(&sample, wrong.to_string()).unwrap();
        assert!(check(live.clone()).is_some());
        std::fs::write(&sample, fixture.to_string()).unwrap();
        let result = check(live);
        assert!(result.is_none(), "{result:?}");
        let unchanged_files = Vec::new();
        let unchanged = |v: serde_json::Value| super::grist_schema_boundary_with(&project, &unchanged_files, &prepared, |_, _| Ok(v.clone()));
        assert!(unchanged(json!({"columns":[{"id":"Title","fields":{"type":"Text"}},{"id":"Count","fields":{"type":"Int"}},{"id":"Added","fields":{"type":"Bool"}}]})).is_some());
        assert!(unchanged(json!({"columns":[{"id":"Title","fields":{"type":"Text"}}]})).is_some());
        assert!(unchanged(json!({"columns":[{"id":"Title","fields":{"type":"Text"}},{"id":"Count","fields":{"type":"Numeric"}}]})).is_some());
        assert!(unchanged(json!({"columns":[{"id":"Title","fields":{"type":"Text"}},{"id":"Count","fields":{"type":"Int"}}]})).is_none());
    }
    #[test]
    fn schema_boundary_ignores_untracked_sources_present_before_preparation() {
        use serde_json::json;
        let project = std::env::temp_dir().join(format!("grist-untracked-schema-{}", std::process::id()));
        let _cleanup = Cleanup(project.clone());
        std::fs::create_dir_all(project.join("tests/fixtures")).unwrap();
        std::process::Command::new("git").arg("-C").arg(&project).args(["init", "-q"]).status().unwrap();
        std::fs::write(project.join("unrelated.js"), "fetch('/tables/Table16/records');").unwrap();
        std::fs::write(project.join("client.js"), "fetch('/tables/Table8/records');").unwrap();
        std::fs::write(project.join("tests/schema.test.js"), "const sample = require('tests/fixtures/grist-Table8-columns.json'); assert(fields === sample.payload.records[0].fields);").unwrap();
        let snapshot = super::snapshot(&project).unwrap();
        std::fs::write(project.join("client.js"), "fetch('/tables/Table8/records'); // changed").unwrap();
        let prepared = json!({"snapshot":snapshot,"purpose":"Table8 저장", "tests":["tests/schema.test.js"]}).as_object().unwrap().clone();
        let files = vec!["unrelated.js".into(), "client.js".into()];
        let live = json!({"columns":[{"id":"Title","fields":{"type":"Text"}}]});
        let check = || super::grist_schema_boundary_with(&project, &files, &prepared, |_, _| Ok(live.clone()));
        assert_eq!(check().unwrap()["table"], "Table8", "changed Table8 still requires a sample");
        std::fs::write(project.join("tests/fixtures/grist-Table8-columns.json"), json!({
            "docId":"doc123","tableId":"Table8","capturedAt":"2026-11-12T10:00:00Z",
            "columns":live["columns"],"payload":{"records":[{"fields":{"Title":"ok"}}]}
        }).to_string()).unwrap();
        assert!(check().is_none(), "unrelated Table16 must not require a sample");
    }
    #[test]
    fn schema_sql_reads_current_database_even_without_changed_table_code() {
        use serde_json::json;
        let project = std::env::temp_dir().join(format!("sql-schema-{}", std::process::id()));
        let _cleanup = Cleanup(project.clone());
        std::fs::create_dir_all(project.join("tests/fixtures")).unwrap();
        std::fs::write(project.join("store.sql"), "SELECT * FROM Items;").unwrap();
        std::fs::write(project.join("build.mjs"), "import fs from \"node:fs\";\nimport { x } from \"./local.js\";\n// select a file from node and copy into dist\n").unwrap();
        let db = project.join("current.sqlite");
        let run = |sql: &str| std::process::Command::new("sqlite3").arg(&db).arg(sql).output().unwrap();
        assert!(run("CREATE TABLE Items (id INTEGER, title TEXT);").status.success());
        let query = "SELECT json_object('columns',json_group_array(json_object('id',name,'fields',json_object('type',type)))) FROM pragma_table_info('Items')";
        let sample = "tests/fixtures/sql-Items-columns.json";
        std::fs::write(project.join("tests/schema.test.js"), format!("const fixture = require('{sample}'); assert(fixture.columns);" )).unwrap();
        let fixture = json!({"schemaCommand":["sqlite3", db.to_str().unwrap(), query],"columns":[{"id":"id","fields":{"type":"INTEGER"}},{"id":"title","fields":{"type":"TEXT"}}]});
        std::fs::write(project.join(sample), fixture.to_string()).unwrap();
        let prepared = json!({"purpose":"Items node local dist 저장", "tests":["tests/schema.test.js"]}).as_object().unwrap().clone();
        assert!(super::other_schema_boundary(&project, &prepared).is_none());
        assert!(run("ALTER TABLE Items ADD COLUMN extra BLOB;").status.success());
        let rejected = super::other_schema_boundary(&project, &prepared).unwrap();
        assert_eq!(rejected["table"], "Items");
        assert_eq!(rejected["reason"], "계약 샘플과 실제 SQL 열·타입 불일치");
    }
    #[test]
    fn schema_web_save_current_page_table8_submit_simulation() {
        use serde_json::json;
        let project = std::path::Path::new("/home/tree/extensions/web_save_current_page");
        if !project.is_dir() { return; }
        let output = std::process::Command::new("/home/tree/ai/scripts/bin/grist").args(["columns", "Table8"]).output().unwrap();
        assert!(output.status.success(), "live Table8 columns query failed");
        let columns: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let live = json!({"columns":columns.as_array().unwrap().iter().map(|col| json!({"id":col["id"],"fields":{"type":col["type"]}})).collect::<Vec<_>>()});
        let prepared = json!({"purpose":"Table8 스키마 저장", "tests":["tests/core.test.cjs"]}).as_object().unwrap().clone();
        let result = super::grist_schema_boundary_with(project, &[], &prepared, |_, _| Ok(live.clone()));
        assert!(result.is_none(), "Table8 submit simulation: {result:?}");
    }
    #[test]
    fn schema_external_api_compares_live_response_with_sample() {
        use serde_json::json;
        let project = std::env::temp_dir().join(format!("api-schema-{}", std::process::id()));
        let _cleanup = Cleanup(project.clone());
        std::fs::create_dir_all(project.join("tests/fixtures")).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/schema", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let _ = std::io::Read::read(&mut stream, &mut [0u8; 1024]);
                let body = r#"{"columns":[{"id":"title","type":"Text"}]}"#;
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
            }
        });
        std::fs::write(project.join("client.js"), format!("const FeedSchemaUrl = '{url}';")).unwrap();
        let sample = "tests/fixtures/api-Feed-schema.json";
        std::fs::write(project.join("tests/schema.test.js"), format!("const fixture = require('{sample}'); assert(fixture.schema);")).unwrap();
        let fixture = json!({"schemaUrl":url,"schema":{"columns":[{"id":"title","type":"Int"}]}});
        std::fs::write(project.join(sample), fixture.to_string()).unwrap();
        let prepared = json!({"purpose":"Feed 스키마", "tests":["tests/schema.test.js"]}).as_object().unwrap().clone();
        assert_eq!(super::other_schema_boundary(&project, &prepared).unwrap()["reason"], "계약 샘플과 실제 외부 API 스키마 불일치");
        let matching = json!({"schemaUrl":url,"schema":{"columns":[{"id":"title","type":"Text"}]}});
        std::fs::write(project.join(sample), matching.to_string()).unwrap();
        assert!(super::other_schema_boundary(&project, &prepared).is_none());
        server.join().unwrap();
    }
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup { fn drop(&mut self) { let _ = std::process::Command::new("chmod").args(["-R", "u+w"]).arg(&self.0).output(); let _ = std::fs::remove_dir_all(&self.0); } }
    #[test]
    fn hidden_replay_rejects_stagehand_ai_actions_but_allows_page_actions() {
        assert!(super::hidden_replay_command("await s.stagehand.act('click tab')").is_err());
        assert!(super::hidden_replay_command("await session.stagehand . extract('read')").is_err());
        assert!(super::hidden_replay_command("await s.page.locator('#tabs a').click()").is_ok());
    }
    #[test]
    fn changed_since_reports_changes_when_git_objects_are_read_only() {
        use std::process::Command;
        let repo = std::env::temp_dir().join(format!("grist-ro-{}", std::process::id()));
        let _cleanup = Cleanup(repo.clone());
        std::fs::create_dir_all(&repo).unwrap();
        Command::new("git").arg("-C").arg(&repo).args(["init", "-q"]).output().unwrap();
        std::fs::write(repo.join("a.txt"), "1").unwrap();
        let prepared = super::snapshot(&repo).unwrap();
        std::fs::write(repo.join("a.txt"), "2").unwrap();
        let chmod = |mode: &str| { Command::new("chmod").args(["-R", mode]).arg(repo.join(".git")).output().unwrap(); };
        chmod("a-w");
        let plain = super::snapshot(&repo);
        let changed: Vec<String> = super::changed_since(&repo, &prepared).into_iter().collect();
        chmod("u+w");
        std::fs::remove_dir_all(&repo).unwrap();
        assert!(plain.is_some());
        assert_eq!(changed, ["a.txt"]);
    }

    #[test]
    fn bridge_sample_rejects_marked_world_row_without_sample_reference() {
        let error = super::bridge_sample_boundary(
            "[🌍] Bridge 연동", [("src/consumer.rs", "fetch('/api/bridge/claim')")],
            ["assert_eq!(status, 200);"],
        ).unwrap();
        assert_eq!(error["marker"], "/api/bridge/");
        assert_eq!(error["file"], "src/consumer.rs");
        let reason = error["reason"].as_str().unwrap();
        assert!(reason.contains("<bridge>/contracts/samples/<계약>/NN-<설명>.json"));
        assert!(reason.contains("샘플이 없으면 Bridge 프로젝트 행에서 producer 테스트와 함께 먼저 만든다"));
    }

    #[test]
    fn bridge_sample_accepts_marked_world_row_with_sample_reference() {
        assert!(super::bridge_sample_boundary(
            "[🌍] Bridge 연동", [("src/consumer.rs", "web_prompt_bridge")],
            ["std::fs::read_to_string(\"web_prompt_bridge/contracts/samples/claim/01-request.json\")"],
        ).is_none());
    }

    #[test]
    fn bridge_sample_skips_other_rows_or_sources_without_bridge_marker() {
        assert!(super::bridge_sample_boundary(
            "[🏗️] 일반", [("src/consumer.rs", "/commands/")], ["no sample"]
        ).is_none());
        assert!(super::bridge_sample_boundary(
            "[🌍] Bridge 연동", [("src/consumer.rs", "ordinary handler")], ["no sample"]
        ).is_none());
    }

    #[test]
    fn bridge_sample_ignores_marker_on_unchanged_lines() {
        use std::process::Command;
        let dir = std::env::temp_dir().join(format!("grist-bridge-diff-{}", std::process::id()));
        let _cleanup = Cleanup(dir.clone());
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&dir).args(args).status().unwrap().success());
        run(&["init", "-q"]);
        std::fs::write(dir.join("consumer.js"), "const old = '127.0.0.1:41600';\nconst value = 1;\n").unwrap();
        run(&["add", "consumer.js"]);
        run(&["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "baseline"]);
        std::fs::write(dir.join("consumer.js"), "const old = '127.0.0.1:41600';\nconst value = 2;\n").unwrap();
        assert!(super::bridge_sample_files(&dir, "[🌍] Grist 저장", &["consumer.js".into()], &serde_json::Map::new()).is_none());
        std::fs::write(dir.join("consumer.js"), "const old = '127.0.0.1:41600';\nconst value = 2;\nfetch('/commands/new');\n").unwrap();
        assert!(super::bridge_sample_files(&dir, "[🌍] Grist 저장", &["consumer.js".into()], &serde_json::Map::new()).is_some());
    }

    #[test]
    fn bridge_sample_ignores_existing_marker_after_folder_rename() {
        use std::process::Command;
        let dir = std::env::temp_dir().join(format!("grist-bridge-rename-{}", std::process::id()));
        let _cleanup = Cleanup(dir.clone());
        std::fs::create_dir_all(dir.join("old_app")).unwrap();
        let run = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&dir).args(args).status().unwrap().success());
        run(&["init", "-q"]);
        std::fs::write(dir.join("old_app/clients.js"), "fetch('/commands/save');\nconst value = 1;\n").unwrap();
        run(&["add", "old_app/clients.js"]);
        run(&["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "baseline"]);
        std::fs::rename(dir.join("old_app"), dir.join("new_app")).unwrap();
        let project = dir.join("new_app");
        std::fs::write(project.join("clients.js"), "fetch('/commands/save');\nconst value = 2;\n").unwrap();
        assert!(super::bridge_sample_files(&project, "[🌍] Grist 저장", &["clients.js".into()], &serde_json::Map::new()).is_none());
        std::fs::write(project.join("fresh.js"), "fetch('/commands/new');\n").unwrap();
        assert!(super::bridge_sample_files(&project, "[🌍] Grist 저장", &["fresh.js".into()], &serde_json::Map::new()).is_some());
        std::fs::write(project.join("clients.js"), "fetch('/commands/save');\nfetch('/queries/added');\n").unwrap();
        assert!(super::bridge_sample_files(&project, "[🌍] Grist 저장", &["clients.js".into()], &serde_json::Map::new()).is_some());
    }

    #[test]
    fn flow_wait_missing_is_scoped_to_each_row_and_names_item() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test/tmp").join(format!("flow-wait-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ai/changes")).unwrap();
        let record = dir.join(".ai/changes/261031.flow.md");
        std::fs::write(&record, "## [구현] a (#10)\n### 구현\n- 상태 : 검증\n#### 확인 (#10)\n- 요약 : [🏗️] 입력 ⇒ 결과\n- [x] 확인\n  결과 : 성공\n  상세 : 실제 값\n- 근거 1 : 실제 값\n## [구현] b (#11)\n### 구현\n- 상태 : 검증\n").unwrap();
        assert_eq!(super::flow_wait_missing(&dir, 10), None);
        assert_eq!(super::flow_wait_missing(&dir, 11), Some("확인 블록"));
        std::fs::write(&record, "## [구현] b (#11)\n### 구현\n- 상태 : 검증\n#### 확인 (#11)\n- [x] 확인\n  결과 : 성공\n  상세 : 실제 값\n- 근거 1 : 실제 값\n").unwrap();
        assert_eq!(super::flow_wait_missing(&dir, 11), Some("요약"));
        std::fs::write(&record, "## [구현] b (#11)\n### 구현\n- 상태 : 검증\n#### 확인 (#11)\n- 요약 : [🏗️] 입력 ⇒ 결과\n- [x] 확인\n  결과 : 성공\n  상세 : 실제 값\n").unwrap();
        assert_eq!(super::flow_wait_missing(&dir, 11), Some("근거"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn partial_confirm_flow_wait_accepts_marked_stop_only() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test/tmp").join(format!("partial-flow-wait-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ai/changes")).unwrap();
        std::fs::write(dir.join("AGENTS.md"), "runtime : 브라우저 확장\n").unwrap();
        let record = dir.join(".ai/changes/261031.flow.md");
        let base = "### 구현\n- 상태 : 검증\n#### 확인 (#10)\n- 요약 : [🏗️] 입력 ⇒ 결과\n- [x] 확인\n  결과 : 성공\n  상세 : 실제 값\n- 근거 1 : 검증 중단\n";
        std::fs::write(&record, base).unwrap();
        assert_eq!(super::flow_wait_missing(&dir, 10), Some("근거"));
        std::fs::write(&record, base.replace("- [x]", "- 부분 확인 : 사용자 지시로 검증 중단\n- [x]")).unwrap();
        assert_eq!(super::flow_wait_missing(&dir, 10), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn version_files_allow_main_but_other_code_delegates_to_worker() {
        let files = vec!["src/main.rs".into()];
        let denied = super::delegate_boundary(None, 375, &files).unwrap();
        assert_eq!(denied["boundary"], "delegate");
        assert_eq!(denied["raw_id"], 375);
        assert_eq!(denied["changed"], 1);
        assert_eq!(denied["files"], serde_json::json!(files));
        assert!(denied["reason"].as_str().unwrap().contains("dispatch-agent"));
        for file in ["README.md", "AGENTS.md", ".ai/docs/a.json", "Cargo.toml", "app/package.json", "app/manifest.json", "app/public/manifest.json", "app/runtime-contract.js", "main.js", "dist/content.js", "/home/tree/ai/scripts/grist/Cargo.toml", "/home/tree/ai/skills/a/SKILL.md", "/home/tree/ai/rules/settings.json"] {
            assert!(super::delegate_boundary(None, 375, &[file.into()]).is_none(), "{file}");
        }
        for file in ["app/package-lock.json", "app/runtime-contract.test.js", "app/scripts/check.cjs", "src/main.js", "app/dist/content.js"] {
            assert!(super::delegate_boundary(None, 375, &[file.into()]).is_some(), "{file}");
        }
        assert!(super::delegate_boundary(None, 375, &["README.md".into(), "src/main.rs".into()]).is_some());
        assert!(super::delegate_boundary(None, 375, &["/home/tree/ai/scripts/a.md".into()]).is_none());
        assert!(super::delegate_boundary(None, 375, &["/home/tree/ai/scripts/dispatch-agent/src/main.rs".into()]).is_none());
        assert!(super::delegate_boundary(Some("worker"), 375, &files).is_none());
    }
    #[test]
    fn worker_row_needs_worker_role_and_numeric_row() {
        assert_eq!(super::worker_row_from(Some("worker"), Some("307")), Some(307));
        assert_eq!(super::worker_row_from(Some("main"), Some("307")), None);
        assert_eq!(super::worker_row_from(Some("worker"), None), None);
        assert_eq!(super::worker_row_from(Some("worker"), Some("x")), None);
    }
    #[test]
    fn worker_task_update_only_own_diagnosis_fields() {
        let allowed = serde_json::json!({"H":"진단", "I":"오류", "D":"[🛣] 점검", "J":"[🛣] 원인", "C":"src/records.rs:77"}).as_object().unwrap().clone();
        assert!(super::worker_update_allowed(Some("worker"), Some(507), 507, &allowed));
        assert!(!super::worker_update_allowed(Some("worker"), Some(507), 508, &allowed));
        for fields in [serde_json::json!({"K":"목표"}), serde_json::json!({"H":"검증"})] {
            assert!(!super::worker_update_allowed(Some("worker"), Some(507), 507, fields.as_object().unwrap()));
        }
        assert!(super::worker_update_allowed(None, None, 507, &allowed));
    }
    #[test]
    fn verifier_row_and_command_boundary() {
        assert_eq!(super::verifier_row_from(Some("verifier"), Some("504")), Some(504));
        assert_eq!(super::verifier_row_from(Some("worker"), Some("504")), None);
        assert_eq!(super::verifier_row_from(Some("verifier"), Some("bad")), None);
        assert!(super::delegate_boundary(Some("verifier"), 504, &["a".into(), "b".into(), "c".into()]).is_none());
        for command in ["task-hidden", "task-step", "task-show", "goal", "task"] {
            assert!(super::row_command_allowed("verifier", Some(504), None, command, 504));
        }
        for command in ["task-ready", "task-submit", "task-goal", "task-add", "flow-wait", "task-complete"] {
            assert!(!super::row_command_allowed("verifier", Some(504), None, command, 504));
        }
        assert!(!super::row_command_allowed("verifier", Some(503), None, "task-hidden", 504));
    }

    #[test]
    fn verifier_rows_allow_listed_rows_and_primary_row() {
        for command in ["task-hidden", "task-step", "task-show", "goal", "task"] {
            for id in [526, 527, 528] {
                assert!(super::row_command_allowed("verifier", Some(528), Some("526,527"), command, id));
            }
        }
    }

    #[test]
    fn verifier_rows_reject_outside_rows_and_unlisted_commands() {
        assert!(!super::row_command_allowed("verifier", Some(528), Some("526,527"), "task-hidden", 529));
        assert!(!super::row_command_allowed("verifier", Some(528), Some("526,527"), "task-submit", 526));
        assert!(!super::row_command_allowed("verifier", Some(528), Some("bad,527"), "task-hidden", 526));
    }

    #[test]
    fn verifier_rows_do_not_expand_worker_access() {
        assert!(!super::row_command_allowed("agent", Some(528), Some("526,527"), "task-step", 526));
        assert!(super::row_command_allowed("agent", Some(528), Some("526,527"), "task-step", 528));
    }

    #[test]
    fn verifier_rows_absent_preserves_single_row_access() {
        assert!(super::row_command_allowed("verifier", Some(528), None, "task-show", 528));
        assert!(!super::row_command_allowed("verifier", Some(528), None, "task-show", 527));
    }

    use super::{record_log_paths, agent_start_args, origin_action, parse_agent_id, OriginAction, Row, flow_submitted_ids, confirmed_fields, confirmation_note, flow_summary_from_text, goal_lines, preparation_of};
    use serde_json::{json, Map, Value};
    use std::path::Path;

    #[test]
    fn flow_wait_selects_only_submitted_row_from_completed_list() {
        let row = |id, stage| Row { id, b: Value::Null, h: json!(stage), d: Value::Null, i: Value::Null, j: Value::Null, k: String::new() };
        let rows = vec![row(234, "완료"), row(269, "확인"), row(276, "검증")];
        assert_eq!(flow_submitted_ids(&rows), vec![276]);
    }

    #[test]
    fn flow_wait_skips_unsubmitted_rows_without_hiding_submitted_ids() {
        let row = |id, stage| Row { id, b: Value::Null, h: json!(stage), d: Value::Null, i: Value::Null, j: Value::Null, k: String::new() };
        let rows = vec![row(548, "초안"), row(551, "검증"), row(552, "작업"), row(553, "검증")];
        assert_eq!(flow_submitted_ids(&rows), vec![551, 553]);
        assert_eq!(super::flow_skipped(&rows), vec![json!({"id":548,"H":"초안"}), json!({"id":552,"H":"작업"})]);
    }

    #[test]
    fn flow_wait_limits_project_history_to_current_open_rows() {
        assert_eq!(super::active_flow_ids(vec![840, 845, 846], &[845]), vec![845]);
        assert_eq!(super::active_flow_ids(vec![840, 845, 846], &[]), vec![840, 845, 846]);
    }

    #[test]
    fn confirmed_write_carries_summary_note_with_stage() {
        let fields = confirmed_fields("확인", Some("[🏗️] 입력 ⇒ 확인 증거"));
        assert_eq!(fields.get("H"), Some(&json!("확인")));
        assert_eq!(fields.get("D"), Some(&json!("[🏗️] 입력 ⇒ 확인 증거")));
        let bare = confirmed_fields("확인", None);
        assert!(!bare.contains_key("D") && bare.get("H") == Some(&json!("확인")));
    }

    #[test]
    fn preparation_accepts_ready_or_red_and_prefers_latest_ready_record() {
        let mut state = Map::new();
        assert!(preparation_of(&state).is_none());
        state.insert("red".into(), json!({"test": "exit 1", "paths": ["src"]}));
        assert_eq!(preparation_of(&state).unwrap().0, "red");
        state.insert("ready".into(), json!({"test": "printf ready", "paths": ["src"]}));
        let (kind, data) = preparation_of(&state).unwrap();
        assert_eq!(kind, "ready");
        assert_eq!(data.get("test"), Some(&Value::String("printf ready".into())));
        let run = crate::testrun::run(data.get("test").unwrap().as_str().unwrap(), Path::new("."));
        assert_eq!((run.code, run.output.as_str()), (0, "ready"));
    }

    #[test]
    fn grist_records_are_not_counted_as_changes() {
        for p in [".ai/changes/261023.a.md", ".ai/gate/7.json", "sub/.ai/changes/x.md", "node_modules/a.js"] {
            assert!(super::generated(p), "{p}");
        }
        for p in [".ai/docs/architecture.md", "src/a.rs", "AGENTS.md", "docs/changes/x.md"] {
            assert!(!super::generated(p), "{p}");
        }
    }

    #[test]
    fn outside_package_changes_count_only_inside_the_test_package() {
        let dir = std::env::temp_dir().join(format!("grist-external-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (project, repo) = (dir.join("project"), dir.join("repo"));
        for p in [&project, &repo.join("a/src"), &repo.join("b/src")] { std::fs::create_dir_all(p).unwrap(); }
        for r in [&project, &repo] { std::process::Command::new("git").arg("-C").arg(r).arg("init").arg("-q").status().unwrap(); }
        std::fs::write(repo.join("a/Cargo.toml"), "[package]").unwrap();
        std::fs::write(repo.join("b/Cargo.toml"), "[package]").unwrap();
        std::fs::write(repo.join("a/src/lib.rs"), "1").unwrap();
        std::fs::write(repo.join("b/src/lib.rs"), "1").unwrap();
        let test = repo.join("a/src/lib.rs").to_string_lossy().to_string();
        let scopes = super::external_scopes(&project, &[test.clone(), "local.rs".into()]);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].1, "a");
        let mut prepared = Map::new();
        prepared.insert("external".into(), super::external_snapshots(&project, &[test]));
        std::fs::write(repo.join("a/src/lib.rs"), "2").unwrap();
        std::fs::write(repo.join("a/src/new.rs"), "3").unwrap();
        std::fs::write(repo.join("b/src/lib.rs"), "2").unwrap();
        println!("external changed = {}", super::external_changed(&prepared));
        assert_eq!(super::external_changed(&prepared), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn snapshot_reports_only_files_changed_after_preparation() {
        let dir = std::env::temp_dir().join(format!("grist-snapshot-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        std::fs::write(dir.join("sub/a.txt"), "a").unwrap();
        std::fs::write(dir.join("sub/keep.txt"), "k").unwrap();
        git(&["add", "-A"]);
        git(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init"]);
        let objects_before = std::process::Command::new("git").arg("-C").arg(&dir).args(["count-objects", "-v"]).output().unwrap().stdout;
        std::fs::write(dir.join("sub/keep.txt"), "dirty before").unwrap();
        let project = dir.join("sub");
        let prepared = super::snapshot(&project).unwrap();
        assert!(prepared.contains("keep.txt"));
        assert!(super::changed_since(&project, &prepared).is_empty());
        std::fs::write(project.join("a.txt"), "changed").unwrap();
        std::fs::write(project.join("new.txt"), "n").unwrap();
        std::fs::create_dir_all(project.join("__pycache__")).unwrap();
        std::fs::write(project.join("__pycache__/x.pyc"), "c").unwrap();
        let changed: Vec<String> = super::changed_since(&project, &prepared).into_iter().collect();
        assert_eq!(changed, vec!["a.txt", "new.txt"]);
        let index = std::fs::read(dir.join(".git/index")).unwrap();
        let _ = super::snapshot(&project);
        assert_eq!(std::fs::read(dir.join(".git/index")).unwrap(), index, "the real index stays untouched");
        let objects_after = std::process::Command::new("git").arg("-C").arg(&dir).args(["count-objects", "-v"]).output().unwrap().stdout;
        assert_eq!(objects_after, objects_before, "snapshot leaves no objects in the repository");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sequential_submissions_keep_only_changes_after_other_rows_submit() {
        use serde_json::json;
        let dir = std::env::temp_dir().join(format!("grist-two-submits-{}", std::process::id()));
        let _cleanup = Cleanup(dir.clone());
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        std::fs::write(dir.join("content.js"), "before").unwrap();
        std::fs::write(dir.join("background.js"), "before").unwrap();
        std::fs::write(dir.join("scripts/check.cjs"), "before").unwrap();
        let row_873_ready = super::snapshot(&dir).unwrap();
        std::fs::write(dir.join("background.js"), "row 874").unwrap();
        std::fs::write(dir.join("scripts/check.cjs"), "row 874").unwrap();
        let row_874_submit = super::snapshot(&dir).unwrap();
        let state = json!({"ready":{"paths":["background.js","scripts"],"snapshot":row_873_ready},"submit":{"snapshot":row_874_submit}});
        super::gate_save(&dir, 874, state.as_object().unwrap()).unwrap();
        std::fs::write(dir.join("content.js"), "row 873").unwrap();
        let mut changed = super::changed_since(&dir, &row_873_ready);
        super::exclude_other_submissions(&dir, 873, &[873, 874], &["content.js".into()], &mut changed);
        assert_eq!(changed, ["content.js".to_string()].into());
        std::fs::write(dir.join("background.js"), "row 873 after 874").unwrap();
        let mut changed = super::changed_since(&dir, &row_873_ready);
        super::exclude_other_submissions(&dir, 873, &[873, 874], &["content.js".into()], &mut changed);
        assert_eq!(changed, ["background.js".to_string(), "content.js".to_string()].into());
        std::fs::write(dir.join("background.js"), "row 874").unwrap();
        let legacy = json!({"ready":{"paths":["content.js","tests"],"snapshot":row_874_submit},"submit":{"changed":2}});
        super::gate_save(&dir, 874, legacy.as_object().unwrap()).unwrap();
        let mut changed = super::changed_since(&dir, &row_873_ready);
        super::exclude_other_submissions(&dir, 873, &[873, 874], &["content.js".into()], &mut changed);
        assert_eq!(changed, ["content.js".to_string()].into());
    }

    #[test]
    fn origin_skips_when_agent_char_line_exists() {
        let memo = vec!["1. 00120000071 · x".to_string()];
        assert_eq!(origin_action(&memo, Some("00120000071")), Ok(OriginAction::Skip));
    }

    #[test]
    fn origin_warns_when_agent_char_line_missing() {
        let memo = vec!["00120000072 · c".to_string()];
        assert_eq!(origin_action(&memo, Some("0012000007A")), Ok(OriginAction::Warn('A')));
        assert_eq!(origin_action(&[], Some("00120000071")), Ok(OriginAction::Warn('1')));
    }

    #[test]
    fn origin_records_without_agent_id() {
        let memo = vec!["00120000071 · c".to_string()];
        assert_eq!(origin_action(&memo, None), Ok(OriginAction::Record));
        assert_eq!(agent_start_args("/p", 7), vec!["start", "/p", "7", "--role", "t26_7"]);
    }

    #[test]
    fn record_log_paths_under_home() {
        let (m, b) = record_log_paths("/h");
        assert!(m.ends_with("/record-log/Cargo.toml") && m.starts_with("/h/ai/skills"));
        assert!(b.ends_with("target/release/record-log"));
    }

    #[test]
    fn agent_id_parsing_and_invalid() {
        assert_eq!(parse_agent_id("0012000007Z"), Ok('Z'));
        for bad in ["", "0012000007", "001200000711", "00120000071a", "0012000007I", "0012000007O", "00120a0007Z", "0012000007-"] {
            assert!(parse_agent_id(bad).is_err(), "{bad}");
        }
        assert!(origin_action(&[], Some("bad")).is_err());
    }

    #[test]
    fn confirmation_note_describes_implemented_path_without_step_number() {
        assert_eq!(
            confirmation_note("[✨] 입력 ⇒ 저장 결과"),
            "[✨] 입력 ⇒ 저장 결과"
        );
    }

    #[test]
    fn confirmation_summary_is_scoped_to_row_and_latest_verification() {
        let text = "## [구현] a (#7)\n### 구현\n- 상태 : 검증\n#### 확인 (#7)\n- 요약 : [✨] 이전 입력 ⇒ 오래된 결과\n#### 확인 (#7)\n- 요약 : [✨] 현재 입력 ⇒ 구현 결과\n## [구현] b (#8)\n### 구현\n- 상태 : 검증\n#### 확인 (#8)\n- 요약 : [⭕️] 다른 입력 ⇒ 다른 결과\n";
        assert_eq!(flow_summary_from_text(text, 7).as_deref(), Some("[✨] 현재 입력 ⇒ 구현 결과"));
        assert_eq!(flow_summary_from_text(text, 8).as_deref(), Some("[⭕️] 다른 입력 ⇒ 다른 결과"));
        assert_eq!(flow_summary_from_text("## [구현] a (#7)\n### 구현\n- 상태 : 초안\n#### 확인 (#7)\n- 요약 : [✨] 현재 입력 ⇒ 구현 결과\n", 7), None);
        assert_eq!(flow_summary_from_text("#### 확인 (#7)\n- 요약 : 입력 ⇒ 결과\n", 7), None);
        assert_eq!(flow_summary_from_text("#### 확인 (#7)\n- 요약 : 사용자 확인 대기\n", 7), None);
        let hidden_after_confirm = "## [구현] a (#7)\n### 구현\n- 상태 : 검증\n#### 확인 (#7)\n- 요약 : [✨] 입력 ⇒ 결과\n- [x] 소비자 확인\n  결과 : 성공\n  상세 : 실제 값\n- 근거 1 : 실제 값\n#### 숨긴 사례 (#7)\n- 목적 : 추가 입력\n- 결과 : 성공\n";
        assert_eq!(flow_summary_from_text(hidden_after_confirm, 7).as_deref(), Some("[✨] 입력 ⇒ 결과"));
        assert!(crate::task::flow_confirmation_in_text(hidden_after_confirm, 7).unwrap().is_some());
    }

    #[test]
    fn goal_lines_parses_only_current_tags() {
        let input = "1. [연결] 서버 연동 ▷ OK\n- [기능] 버튼 추가 ▷ True\n2. [수정] 오류 해결 ▷ 정상";
        assert_eq!(
            goal_lines(input),
            vec![
                ("연결".to_string(), "서버 연동 ▷ OK".to_string()),
                ("기능".to_string(), "버튼 추가 ▷ True".to_string()),
                ("수정".to_string(), "오류 해결 ▷ 정상".to_string()),
            ]
        );
    }

    #[test]
    fn malformed_goal_is_rejected_before_any_api_call() {
        assert!(super::goal_summary("  ").is_err());
        assert!(super::goal_summary("a\nb").is_err());
        assert_eq!(super::goal_summary(" 한 줄 ").unwrap(), "한 줄");
        let dir = std::env::temp_dir().join(format!("grist-check-goal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let empty = dir.join("goal.md");
        std::fs::write(&empty, "\n").unwrap();
        assert!(super::check_goal(&dir, None, empty.to_str().unwrap(), "요약").is_err());
        assert!(super::check_goal(&dir, None, empty.to_str().unwrap(), "").is_err());
        let untagged = dir.join("untagged.md");
        std::fs::write(&untagged, "목표 아님\n").unwrap();
        assert!(super::check_goal(&dir, None, untagged.to_str().unwrap(), "요약").is_err());
        let errors = super::goal_errors("1. [기능] a ▷ b\n목표 아님\n[수정] c");
        println!("goal_errors ▷ {errors:?}");
        assert_eq!(errors.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stop_goal_cannot_require_its_own_hook_to_be_absent() {
        let impossible = [
            "3. [기능] 과정 턴에서 훅 실행(main-edit-gate·task-complete)·cargo test·--role improver 호출이 없는 Stop ▷ 차단",
            "3. [기능] task-complete 실행이 없는 Stop ▷ 차단",
        ];
        for goal in impossible {
            assert!(super::goal_errors(goal).iter().any(|e| e.contains("Stop 훅 자체")), "{goal}");
        }
        for goal in [
            "3. [기능] 훅 검증 기록이 없는 Stop ▷ 차단",
            "3. [기능] task-complete 실행 기록이 없는 Stop ▷ 차단",
            "3. [기능] 과정 턴에서 수정과 훅 검증이 없는 Stop ▷ 차단",
            "3. [기능] task-complete 실행이 있는 Stop ▷ 허용",
        ] {
            assert!(super::goal_errors(goal).is_empty(), "{goal}");
        }
    }

    #[test]
    fn child_project_claimed_ignores_files_prepared_in_child_project() {
        let dir = std::env::temp_dir().join(format!("grist-child-claimed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let child = dir.join("blender_bridge");
        std::fs::create_dir_all(child.join(".ai/gate")).unwrap();
        std::fs::write(child.join("AGENTS.md"), "# 상태\n").unwrap();
        std::fs::write(
            child.join(".ai/gate/846.json"),
            r#"{"ready":{"paths":["ui.py","tests"],"tests":["tests/test_bridge.py"]}}"#,
        ).unwrap();
        assert!(super::child_project_claimed(&dir, "blender_bridge/ui.py"));
        assert!(super::child_project_claimed(&dir, "blender_bridge/AGENTS.md"));
        assert!(super::child_project_claimed(&dir, "blender_bridge/tests/test_bridge.py"));
        assert!(!super::child_project_claimed(&dir, "blender_bridge/unclaimed.py"));
        assert!(!super::child_project_claimed(&dir, "other/ui.py"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unverified_fix_report_and_diagnosis_ready_gate_rejects_unconfirmed_cause() {
        println!("[verify-step] range-check start=2 mid=5 end=8");
        assert!(super::diagnosis_ready_error("진단", "").is_some());
        assert!(super::diagnosis_ready_error("진단", "1. [추정] 정적 코드 추정").is_some());
        assert!(super::diagnosis_ready_error("진단", "1. [🛣] 유닛테스트만 확인").is_some());
        assert!(super::diagnosis_ready_error("진단", "1. [🛣] 관측된 원인").is_none());
        assert!(super::diagnosis_ready_error("초안", "").is_none());
    }
}
