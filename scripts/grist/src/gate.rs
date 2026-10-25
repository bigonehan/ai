//! Stage gates (`gate`, `task-goal`, `task-red`, `task-ready`, `task-submit`, `task-hidden`, `flow-wait`).

use crate::api::{self, R, DONE_STAGE, GOAL_FIELD, SUBMITTED_STAGE, TASK_TABLE};
use crate::util;
use crate::records::{self, NEW_FEATURE};
use crate::task::{Step, TaskChain, TaskCtx};
use crate::testrun;
use regex::{Regex, RegexBuilder};
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const WAITING_STAGE: &str = "확인";
pub const DRAFT_STAGE: &str = "초안";
pub const ERROR_STAGE: &str = "진단";
pub const WORKING_STAGES: [&str; 3] = ["작업", "교정", "진단"];
pub const ALWAYS_EDITABLE: [&str; 3] = [".ai", "AGENTS.md", "test/tmp"];
pub const OUTPUT_TAIL: usize = 4000;

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

fn flow_summary_from_text(text: &str, raw_id: i64) -> Option<String> {
    let submitted = format!("#### 검증 (#{raw_id})");
    let confirmed = format!("#### 확인 (#{raw_id})");
    let mut in_confirmed = false;
    let mut summary = None;
    for line in text.lines() {
        let line = line.trim();
        if line == submitted {
            in_confirmed = false;
            summary = None;
        } else if line == confirmed {
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
    pub h: Value,
    pub d: Value,
    pub i: Value,
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
            "K": self.k,
        })
    }
    pub fn to_gate_json(&self) -> Value {
        json!({
            "id": self.id,
            "H": self.h,
            "D": self.d,
            "I": self.i,
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

fn delegate_boundary(role: Option<&str>, raw_id: i64, changed: usize, files: &[String]) -> Option<Value> {
    if role == Some("worker") || changed < 3 { return None; }
    Some(json!({
        "boundary": "delegate",
        "raw_id": raw_id,
        "changed": changed,
        "files": files,
        "reason": "main session changed 3 or more files; delegate implementation with dispatch-agent"
    }))
}

pub fn actor() -> (String, Option<i64>) {
    if let Some(row) = worker_row() {
        return ("agent".into(), Some(row));
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
    if role == "helper" || (role == "agent" && row != Some(raw_id)) {
        let suffix = row.map(|r| format!(" t26_{r}")).unwrap_or_default();
        return api::fail(
            "actor",
            format!("`{command} {raw_id}` is run by the main session or agent t26_{raw_id} only (caller: {role}{suffix})"),
        );
    }
    Ok(())
}

pub fn rows(ids: &[i64]) -> R<Vec<Row>> {
    let fields = vec!["H".into(), "D".into(), "I".into(), GOAL_FIELD.into()];
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
                h: f.get("H").cloned().unwrap_or(Value::Null),
                d: f.get("D").cloned().unwrap_or(Value::Null),
                i: f.get("I").cloned().unwrap_or(Value::Null),
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
    let ext = util::resolve(&records::extensions_root());
    let rel = start.strip_prefix(&ext).ok()?;
    let first = rel.components().next()?.as_os_str().to_string_lossy().into_owned();
    if first.is_empty() || first.starts_with('.') || first == "node_modules" || first == "test" {
        return None;
    }
    let project_dir = records::extensions_root().join(&first);
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
            "--purpose is a short one-line label of the wanted result (no command, backticks or --options)",
        );
    }
    Ok(label.to_string())
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
    util::splitlines(goal)
        .iter()
        .map(|l| util::strip(l))
        .filter(|l| !l.is_empty() && !line_re.is_match(l))
        .map(|l| format!("goal line needs `N. [연결|기능|수정] <내용>`: {l}"))
        .collect()
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
    let out = cmd.output().ok()?;
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

/// The working tree (tracked and untracked, ignore rules applied) as one git tree hash. A copy of
/// the real index keeps `git add -A` stat-based, and the real index is never touched.
pub fn snapshot(project_dir: &Path) -> Option<String> {
    let index = git(project_dir, &["rev-parse", "--git-path", "index"])?;
    let index = project_dir.join(util::strip(&index));
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("grist-snapshot-{}-{nanos}.idx", std::process::id()));
    if index.is_file() && std::fs::copy(&index, &tmp).is_err() {
        return None;
    }
    let run = |args: &[&str]| {
        Command::new("git").arg("-C").arg(project_dir).args(args).env("GIT_INDEX_FILE", &tmp)
            .stdin(std::process::Stdio::null()).output().ok().filter(|o| o.status.success())
    };
    let tree = run(&["add", "-A", "."]).and_then(|_| run(&["write-tree"]))
        .map(|o| util::strip(&String::from_utf8_lossy(&o.stdout)).to_string());
    let _ = std::fs::remove_file(&tmp);
    tree.filter(|t| !t.is_empty())
}

/// Files (relative to the repo at `project_dir`) that differ between the preparation snapshot and now;
/// `external_changed` reuses it for packages outside the project.
pub fn changed_since(project_dir: &Path, prepared: &str) -> BTreeSet<String> {
    let Some(now) = snapshot(project_dir) else { return BTreeSet::new() };
    let names = git(project_dir, &["diff", "--name-only", "--relative", prepared, &now]).unwrap_or_default();
    util::splitlines(&names).into_iter().filter(|l| !l.is_empty() && !generated(l)).collect()
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
    let (goal, summary) = check_goal(&project_dir, Some(raw_id), goal_file, summary)?;
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
    let req = latest_request(project_dir);
    let judgment = jev("scope", raw_id, &[("request", &req), ("goal", &goal)]);
    if jev_blocks(&judgment) {
        return gate_err(json!({
            "boundary": "goal",
            "raw_id": raw_id,
            "reason": "goal not grounded in the latest # 요청 or incomplete",
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

/// What task-red does about the agent origin line in 메모.
#[derive(Debug, PartialEq)]
pub enum OriginAction {
    /// The 메모 already has `에이전트 <char> :` for the given agent ID.
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

/// Whether the row's 메모 already has an `에이전트 <agent> : ...` line.
pub fn memo_has_agent_char(memo: &[String], agent: char) -> bool {
    let head = format!("에이전트 {agent} : ");
    memo.iter().any(|line| {
        line.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c.is_whitespace())
            .starts_with(&head)
    })
}

/// Decides skip / warn / record from the optional agent ID and the row's 메모.
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
    let memo: Vec<String> = records::task_show(project_arg, raw_id, Some("메모"))
        .ok()
        .and_then(|v| v.get("lines").and_then(|l| l.as_array()).cloned())
        .map(|a| a.iter().filter_map(|l| l.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    match origin_action(&memo, agent_id) {
        Err(e) => return eprintln!("warning: {e}; agent origin not recorded for #{raw_id}"),
        Ok(OriginAction::Skip) => return,
        Ok(OriginAction::Warn(c)) => {
            return eprintln!(
                "warning: 메모 of #{raw_id} has no `에이전트 {c} :` line; the caller must run record-log start; nothing recorded"
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
    Ok(ctx.result)
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
    let goal = goal_of(&project_dir, &row)?;
    if util::strip(&goal).is_empty() {
        return api::fail("ready", format!("Table26 #{raw_id} has no goal; the main session writes it with task-goal first"));
    }
    if command.trim().is_empty() || paths.is_empty() || tests.is_empty() || paths.iter().any(|p| p.trim().is_empty() || p == ".") {
        return api::fail("ready", "task-ready needs a focused --test command, --tests files, and explicit top-level --paths");
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
        let detail = format!("{} {}", failure_detail(&run.output, &project_dir), run.suffix());
        records::append_item(&project_dir, raw_id, &test_record("검증", raw_id, &purpose, false, &detail))?;
        return gate_err(json!({
            "boundary": "submit",
            "raw_id": raw_id,
            "exit": run.code,
            "ms": run.ms,
            "output": run.output
        }));
    }
    let changed = prepared.get("snapshot").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
        .map(|prepared_tree| changed_since(&project_dir, prepared_tree)).unwrap_or_default();
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
        let outside: Vec<String> = changed.iter().filter(|f| !inside(f, &all_allowed)).cloned().collect();
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
    let changed_count = files.len();
    if let Some(boundary) = delegate_boundary(std::env::var("T26_ROLE").ok().as_deref(), raw_id, changed_count, &files) {
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
    require_main("task-hidden")?;
    let purpose = check_purpose(purpose)?;
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
    records::append_item(&project_dir, raw_id, &test_record("확인", raw_id, &purpose, ok, &rec_detail))?;
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
    Ok(json!({"id": raw_id, "hidden": purpose, "result": "성공"}))
}

fn flow_judged(project_dir: &Path, ids: &[i64]) -> Vec<(String, Option<String>)> {
    let id_pat = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("|");
    let submit_re = Regex::new(&format!(r"^#+\s*검증\s+\(#({id_pat})\)\s*$")).unwrap();
    let flow_re = Regex::new(&format!(r"^#+\s*확인\s+\(#({id_pat})\)\s*$")).unwrap();
    let mut blocks: Vec<Vec<String>> = Vec::new();
    for record in util::glob(&project_dir.join(".ai").join("changes"), "", ".md") {
        let Ok(text) = std::fs::read_to_string(&record) else {
            continue;
        };
        let lines = util::splitlines(&text);
        for (index, line) in lines.iter().enumerate() {
            if !line.starts_with('#') {
                continue;
            }
            if submit_re.is_match(line) {
                blocks.clear();
            } else if flow_re.is_match(line) {
                let mut body = Vec::new();
                for rest in &lines[(index + 1)..] {
                    if rest.starts_with('#') {
                        break;
                    }
                    body.push(rest.clone());
                }
                blocks.push(body);
            }
        }
    }
    let check_re = check_item_re();
    let mut judged: Vec<(String, Option<String>)> = Vec::new();
    for lines in &blocks {
        for (index, line) in lines.iter().enumerate() {
            if let Some(caps) = check_re.captures(line) {
                if &caps[1] == "x" || &caps[1] == "X" {
                    let key = caps[2].to_string();
                    let ev = evidence(lines, index);
                    if let Some(existing) = judged.iter_mut().find(|(k, _)| k == &key) {
                        existing.1 = ev;
                    } else {
                        judged.push((key, ev));
                    }
                }
            }
        }
    }
    judged
}

fn flow_submitted_ids(rows: &[Row]) -> Vec<i64> {
    rows.iter().filter(|row| row.h_str() == SUBMITTED_STAGE).map(|row| row.id).collect()
}

pub fn flow_wait(project_arg: &str) -> R<Value> {
    require_main("flow-wait")?;
    let (project_dir, _, ids) = records::project(project_arg)?;
    let fetched = rows(&ids)?;
    let submitted_ids = flow_submitted_ids(&fetched);
    let allowed_flow = [SUBMITTED_STAGE, WAITING_STAGE, DONE_STAGE];
    let open_rows: Vec<Value> = fetched
        .iter()
        .filter(|r| !allowed_flow.contains(&r.h_str()))
        .map(|r| json!({"id": r.id, "H": r.h}))
        .collect();
    if !open_rows.is_empty() {
        return gate_err(json!({"boundary": "flow", "reason": "rows not submitted", "rows": open_rows}));
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
                    "output": output
                }));
                break;
            }
        }
    }
    if !missing_hidden.is_empty() {
        return gate_err(json!({
            "boundary": "flow",
            "reason": "each 검증 row needs a hidden case after its latest `#### 검증 (#id)`: `task-hidden N --purpose LABEL --test CMD`",
            "rows": missing_hidden
        }));
    }
    if !failed.is_empty() {
        return gate_err(json!({
            "boundary": "flow",
            "reason": format!("submitted or hidden tests failed; rows set to {ERROR_STAGE}"),
            "rows": failed
        }));
    }

    if submitted_ids.is_empty() {
        return Ok(Value::Array(Vec::new()));
    }
    let flow = flow_judged(&project_dir, &submitted_ids);
    if flow.is_empty() || flow.iter().any(|(_, v)| v.is_none()) {
        let missing_ev: Vec<String> = flow
            .iter()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.clone())
            .collect();
        return gate_err(json!({
            "boundary": "flow",
            "reason": "a `#### 확인 (#id)` written after the latest `#### 검증 (#id)` needs `- [x] <목적>` items each followed by `결과 : 성공` and `상세 : <actual value>`; record it with `grist task-confirm` (do not edit .ai/changes) and check it with `grist task-show <id> --section 확인`",
            "missing_evidence": missing_ev
        }));
    }

    let summaries: Vec<(i64, String)> = fetched
        .iter()
        .filter(|row| row.h_str() == SUBMITTED_STAGE)
        .filter_map(|row| flow_summary(&project_dir, row.id).map(|summary| (row.id, summary)))
        .collect();
    if summaries.len() != fetched.iter().filter(|row| row.h_str() == SUBMITTED_STAGE).count() {
        return gate_err(json!({
            "boundary": "flow",
            "reason": "each 검증 row needs `- 요약 : [표시 기호] <입력 또는 이전 단계> ⇒ <구현 결과>` in its latest `#### 확인 (#id)`; record it with `grist task-confirm --summary` (do not edit .ai/changes)"
        }));
    }

    let mut result = Vec::new();
    for row in &fetched {
        if row.h_str() == SUBMITTED_STAGE {
            result.push(json!({"id": row.id, "flow_wait": true}));
            let mut state = gate_load(&project_dir, row.id);
            state.insert("flow_wait".into(), Value::Bool(true));
            gate_save(&project_dir, row.id, &state)?;
        }
    }
    if !result.is_empty() {
        records::refresh_day_version(&project_dir)?;
    }
    Ok(Value::Array(result))
}

#[cfg(test)]
mod tests {
    #[test]
    fn submit_delegates_main_three_files_but_allows_two_and_worker() {
        let files = vec!["a".into(), "b".into(), "c".into()];
        let denied = super::delegate_boundary(None, 375, 3, &files).unwrap();
        assert_eq!(denied["boundary"], "delegate");
        assert_eq!(denied["raw_id"], 375);
        assert_eq!(denied["changed"], 3);
        assert_eq!(denied["files"], serde_json::json!(files));
        assert!(denied["reason"].as_str().unwrap().contains("dispatch-agent"));
        assert!(super::delegate_boundary(None, 375, 2, &files[..2]).is_none());
        assert!(super::delegate_boundary(Some("worker"), 375, 3, &files).is_none());
    }
    #[test]
    fn worker_row_needs_worker_role_and_numeric_row() {
        assert_eq!(super::worker_row_from(Some("worker"), Some("307")), Some(307));
        assert_eq!(super::worker_row_from(Some("main"), Some("307")), None);
        assert_eq!(super::worker_row_from(Some("worker"), None), None);
        assert_eq!(super::worker_row_from(Some("worker"), Some("x")), None);
    }

    use super::{record_log_paths, agent_start_args, origin_action, parse_agent_id, OriginAction, Row, flow_submitted_ids, confirmed_fields, confirmation_note, flow_summary_from_text, goal_lines, preparation_of};
    use serde_json::{json, Map, Value};
    use std::path::Path;

    #[test]
    fn flow_wait_selects_only_submitted_row_from_completed_list() {
        let row = |id, stage| Row { id, h: json!(stage), d: Value::Null, i: Value::Null, k: String::new() };
        let rows = vec![row(234, "완료"), row(269, "확인"), row(276, "검증")];
        assert_eq!(flow_submitted_ids(&rows), vec![276]);
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
        std::fs::write(dir.join("sub/keep.txt"), "dirty before").unwrap();
        let project = dir.join("sub");
        let prepared = super::snapshot(&project).unwrap();
        assert_eq!(prepared.len(), 40);
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
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn origin_skips_when_agent_char_line_exists() {
        let memo = vec!["1. 에이전트 1 : t26_7 · codex://x".to_string()];
        assert_eq!(origin_action(&memo, Some("00120000071")), Ok(OriginAction::Skip));
    }

    #[test]
    fn origin_warns_when_agent_char_line_missing() {
        let memo = vec!["에이전트 2 : reviewer · c".to_string()];
        assert_eq!(origin_action(&memo, Some("0012000007A")), Ok(OriginAction::Warn('A')));
        assert_eq!(origin_action(&[], Some("00120000071")), Ok(OriginAction::Warn('1')));
    }

    #[test]
    fn origin_records_without_agent_id() {
        let memo = vec!["에이전트 1 : t26_7 · c".to_string()];
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
        let text = "#### 검증 (#7)\n#### 확인 (#7)\n- 요약 : [✨] 이전 입력 ⇒ 오래된 결과\n#### 검증 (#7)\n#### 확인 (#8)\n- 요약 : [⭕️] 다른 입력 ⇒ 다른 결과\n#### 확인 (#7)\n- 요약 : [✨] 현재 입력 ⇒ 구현 결과\n";
        assert_eq!(flow_summary_from_text(text, 7).as_deref(), Some("[✨] 현재 입력 ⇒ 구현 결과"));
        assert_eq!(flow_summary_from_text(text, 8).as_deref(), Some("[⭕️] 다른 입력 ⇒ 다른 결과"));
        assert_eq!(flow_summary_from_text("#### 확인 (#7)\n- 요약 : 입력 ⇒ 결과\n", 7), None);
        assert_eq!(flow_summary_from_text("#### 확인 (#7)\n- 요약 : 사용자 확인 대기\n", 7), None);
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
}
