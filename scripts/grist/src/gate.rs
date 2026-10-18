//! Stage gates (`gate`, `task-goal`, `task-red`, `task-submit`, `task-hidden`, `flow-wait`, `task-complete`).

use crate::api::{self, R, DONE_STAGE, GOAL_FIELD, SUBMITTED_STAGE, TASK_TABLE};
use crate::util;
use crate::records::{self, NEW_FEATURE};
use regex::{Regex, RegexBuilder};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const WAITING_STAGE: &str = "확인";
pub const DRAFT_STAGE: &str = "초안";
pub const ERROR_STAGE: &str = "진단";
pub const WORKING_STAGES: [&str; 3] = ["작업", "교정", "진단"];
pub const ALWAYS_EDITABLE: [&str; 4] = [".ai", "Input.md", "AGENTS.md", "test/tmp"];
pub const OUTPUT_TAIL: usize = 4000;

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

pub fn actor() -> (String, Option<i64>) {
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

pub fn red_data(project_dir: &Path, raw_id: i64) -> Map<String, Value> {
    gate_load(project_dir, raw_id)
        .get("red")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
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
        Regex::new(
            r"^\s*-\s*(?:\[(값|상태|수치|불변|화면|사용자|연결|기능|수정)\]|<(값|상태|수치|불변|화면|사용자|연결|기능|수정)>)\s*(.+?)\s*$",
        )
        .unwrap()
    })
}

pub fn goal_lines(goal: &str) -> Vec<(String, String)> {
    let re = goal_line_re();
    util::splitlines(goal)
        .into_iter()
        .filter_map(|l| {
            re.captures(&l).map(|c| {
                let kind = c.get(1).or_else(|| c.get(2)).unwrap().as_str().to_string();
                (kind, c[3].to_string())
            })
        })
        .collect()
}

pub fn goal_errors(goal: &str) -> Vec<String> {
    if util::strip(goal).is_empty() {
        return vec!["empty goal".to_string()];
    }
    Vec::new()
}

pub fn goal_of(project_dir: &Path, row: &Row) -> R<String> {
    if let Some(goal) = records::changes_goal(project_dir, row.id) {
        return Ok(goal);
    }
    Ok(row.k.clone())
}

pub fn latest_request(project_dir: &Path) -> String {
    let text = records::input_text(project_dir);
    static REQ_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let req_re = REQ_RE.get_or_init(|| Regex::new(r"(?m)^#\s*요구 사항\s*$").unwrap());
    if req_re.is_match(&text) {
        return util::tail(util::strip(&text), 6000);
    }
    if util::strip(&text).is_empty() {
        return String::new();
    }
    let mut sections = Vec::new();
    let mut cur = Vec::new();
    for line in util::splitlines(&text) {
        if line.starts_with('#') && !cur.is_empty() {
            sections.push(std::mem::take(&mut cur).join("\n"));
        }
        cur.push(line);
    }
    if !cur.is_empty() {
        sections.push(cur.join("\n"));
    }
    for sec in sections.iter().rev() {
        let s = util::strip(sec);
        if !s.is_empty() && s.matches('\n').count() >= 1 {
            return util::tail(s, 6000);
        }
    }
    util::tail(&text, 6000)
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

pub fn jev_brief(result: &Value) -> String {
    if !util::truthy(result.get("ok")) {
        return "Jev 판정 생략".to_string();
    }
    let failed = jev_failed(result).len();
    let mode = result.get("mode").and_then(|v| v.as_str()).unwrap_or("");
    let verdict = result.get("verdict").and_then(|v| v.as_str()).unwrap_or("");
    let suffix = if failed > 0 {
        format!(", 목표 {failed}줄 불일치")
    } else {
        String::new()
    };
    format!("Jev {mode} {verdict}{suffix}")
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

// --- git scope & literals ---------------------------------------------------

pub fn dirty(project_dir: &Path, base: &str) -> BTreeSet<String> {
    let tracked = git(project_dir, &["diff", "--name-only", "--relative", base]).unwrap_or_default();
    let untracked = git(project_dir, &["ls-files", "--others", "--exclude-standard"]).unwrap_or_default();
    let gen_re = Regex::new(r"(^|/)(__pycache__|\.pytest_cache|node_modules|\.mypy_cache)(/|$)|\.pyc$").unwrap();
    util::splitlines(&format!("{tracked}{untracked}"))
        .into_iter()
        .filter(|l| !l.is_empty() && !gen_re.is_match(l))
        .collect()
}

pub fn changed_since(project_dir: &Path, base: &str, baseline: &BTreeSet<String>) -> BTreeSet<String> {
    dirty(project_dir, base)
        .into_iter()
        .filter(|f| !baseline.contains(f))
        .collect()
}

pub fn inside(path: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|p| {
        let p = p.trim_end_matches('/');
        path == p || path.starts_with(&format!("{p}/"))
    })
}

pub fn run_test(command: &str, project_dir: &Path) -> (i32, String) {
    match Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .current_dir(project_dir)
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(out) => {
            let code = out.status.code().unwrap_or(1);
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            (code, util::tail(&combined, OUTPUT_TAIL))
        }
        Err(e) => (1, e.to_string()),
    }
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
    let summary = util::strip(summary);
    if summary.is_empty() || summary.contains('\n') {
        return api::fail(
            "goal",
            "--summary is the one-line $K goal; the detailed lines go in --goal-file",
        );
    }
    let raw_goal = std::fs::read_to_string(goal_file).map_err(api::other)?;
    let goal = records::canonical_goal(&raw_goal);
    let errors = goal_errors(&goal);
    if !errors.is_empty() {
        return gate_err(json!({"boundary": "goal", "raw_id": raw_id, "errors": errors}));
    }
    let req = latest_request(&project_dir);
    let judgment = jev("scope", Some(raw_id), &[("request", &req), ("goal", &goal)]);
    if jev_blocks(&judgment) {
        return gate_err(json!({
            "boundary": "goal",
            "raw_id": raw_id,
            "reason": "goal not grounded in Input.md or incomplete",
            "jev": judgment.get("items").cloned().unwrap_or(Value::Null)
        }));
    }
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

pub fn task_red(
    raw_id: i64,
    project_arg: &str,
    command: &str,
    paths: &[String],
    tests: &[String],
    purpose: &str,
    detail: &str,
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
    for name in tests {
        if !project_dir.join(name).is_file() {
            return api::fail(
                "red",
                format!("test file {name} does not exist under {}", util::path_str(&project_dir)),
            );
        }
    }
    let (code, output) = run_test(command, &project_dir);
    if code == 0 {
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
            "output_tail": output
        }));
    }
    let base = git(&project_dir, &["rev-parse", "HEAD"])
        .map(|s| util::strip(&s).to_string())
        .filter(|s| !s.is_empty());
    let dirty_list: Vec<String> = base
        .as_deref()
        .map(|b| dirty(&project_dir, b).into_iter().collect())
        .unwrap_or_default();
    let red = json!({
        "purpose": purpose,
        "test": command,
        "tests": tests,
        "paths": paths,
        "base": base,
        "dirty": dirty_list
    });
    let mut state = Map::new();
    state.insert("red".into(), red);
    gate_save(&project_dir, raw_id, &state)?;
    let det_one = one_line(detail, &project_dir);
    let rec_detail = if !det_one.is_empty() {
        det_one
    } else {
        failure_detail(&output, &project_dir)
    };
    records::append(&project_dir, &test_record("초안", raw_id, &purpose, false, &rec_detail))?;
    if row.h_str() == DRAFT_STAGE || row.h_str() == "진단" {
        let mut upd = Map::new();
        upd.insert("H".into(), Value::String("작업".into()));
        api::update(TASK_TABLE, raw_id, &upd)
    } else {
        Ok(row.to_json())
    }
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
    let red = red_data(&project_dir, raw_id);
    if red.is_empty() {
        return gate_err(json!({
            "boundary": "submit",
            "raw_id": raw_id,
            "reason": "no red record; run task-red with a failing test first"
        }));
    }
    let goal = goal_of(&project_dir, &row)?;
    let test_cmd = red.get("test").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let (code, output) = run_test(&test_cmd, &project_dir);
    if code != 0 {
        return gate_err(json!({
            "boundary": "submit",
            "raw_id": raw_id,
            "test": test_cmd,
            "exit": code,
            "output_tail": output
        }));
    }
    if let Some(base) = red.get("base").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        let mut allowed: Vec<String> = ALWAYS_EDITABLE.iter().map(|s| s.to_string()).collect();
        let mut test_files: Vec<String> = Vec::new();
        for &other_id in &ids {
            let detail = red_data(&project_dir, other_id);
            if let Some(p_arr) = detail.get("paths").and_then(|v| v.as_array()) {
                for p in p_arr.iter().filter_map(|v| v.as_str()) {
                    allowed.push(p.to_string());
                }
            }
            match detail.get("tests") {
                Some(Value::Array(arr)) => {
                    for k in arr.iter().filter_map(|v| v.as_str()) {
                        test_files.push(k.to_string());
                    }
                }
                Some(Value::Object(t_obj)) => {
                    for k in t_obj.keys() {
                        test_files.push(k.clone());
                    }
                }
                _ => {}
            }
        }
        let mut all_allowed = allowed.clone();
        all_allowed.extend(test_files);
        let baseline: BTreeSet<String> = match red.get("dirty") {
            Some(Value::Array(arr)) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            Some(Value::Object(obj)) => obj.keys().cloned().collect(),
            _ => BTreeSet::new(),
        };
        let changed = changed_since(&project_dir, base, &baseline);
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

    let judgment = jev("match", Some(raw_id), &[("goal", &goal), ("output", &output)]);
    if jev_blocks(&judgment) {
        let failed_joined = jev_failed(&judgment).join("; ");
        let note = format!("{}. Jev 불일치: {}", records::next_number(row.i_str()), util::head(&failed_joined, 200));
        let mut upd = Map::new();
        upd.insert("H".into(), Value::String(ERROR_STAGE.into()));
        upd.insert("I".into(), Value::String(note));
        api::update(TASK_TABLE, raw_id, &upd)?;
        return gate_err(json!({
            "boundary": "submit",
            "raw_id": raw_id,
            "reason": format!("actual output does not meet the goal; row set to {ERROR_STAGE}"),
            "jev": judgment.get("items").cloned().unwrap_or(Value::Null)
        }));
    }
    let purpose = red
        .get("purpose")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("제출 테스트")
        .to_string();
    let mut state = gate_load(&project_dir, raw_id);
    state.insert("red".into(), Value::Object(red));
    state.insert("submit".into(), json!({"purpose": purpose, "test": test_cmd}));
    state.insert("hidden".into(), json!([]));
    gate_save(&project_dir, raw_id, &state)?;
    records::append(
        &project_dir,
        &test_record("검증", raw_id, &purpose, true, &jev_brief(&judgment)),
    )?;
    let mut upd = Map::new();
    upd.insert("H".into(), Value::String(SUBMITTED_STAGE.into()));
    api::update(TASK_TABLE, raw_id, &upd)
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
    let (code, output) = run_test(command, &project_dir);
    let mut state = gate_load(&project_dir, raw_id);
    let mut new_h = Map::new();
    new_h.insert("purpose".into(), Value::String(purpose.clone()));
    new_h.insert("test".into(), Value::String(command.to_string()));
    hidden.push(new_h);
    state.insert("submit".into(), Value::Object(submit));
    state.insert(
        "hidden".into(),
        Value::Array(hidden.into_iter().map(Value::Object).collect()),
    );
    gate_save(&project_dir, raw_id, &state)?;
    let ok = code == 0;
    let det_one = one_line(detail, &project_dir);
    let rec_detail = if ok && detail.is_empty() {
        "-".to_string()
    } else if !det_one.is_empty() {
        det_one
    } else {
        failure_detail(&output, &project_dir)
    };
    records::append(&project_dir, &test_record("확인", raw_id, &purpose, ok, &rec_detail))?;
    if !ok {
        return gate_err(json!({
            "boundary": "hidden",
            "raw_id": raw_id,
            "purpose": purpose,
            "exit": code,
            "output_tail": output
        }));
    }
    Ok(json!({"id": raw_id, "hidden": purpose, "result": "성공"}))
}

fn flow_judged(project_dir: &Path, ids: &[i64]) -> Vec<(String, Option<String>)> {
    let id_pat = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("|");
    let submit_re = Regex::new(&format!(r"^#+\s*검증\s+\(#({id_pat})\)\s*$")).unwrap();
    let flow_re = Regex::new(r"^#+\s*(?:흐름 판정|확인)(?:\s+\(#\d+\))?\s*$").unwrap();
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

pub fn flow_wait(project_arg: &str, waiting: &str) -> R<Value> {
    require_main("flow-wait")?;
    let (project_dir, _, ids) = records::project(project_arg)?;
    let fetched = rows(&ids)?;
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
            let (code_opt, output) = match cmd_opt {
                Some(c) => {
                    let (code, out) = run_test(c, &project_dir);
                    (Some(code), out)
                }
                None => (None, "no submit record".to_string()),
            };
            if code_opt != Some(0) {
                let purpose_str = case.get("purpose").map(util::val_str).unwrap_or_default();
                let note = format!(
                    "{}. 메인 재실행 실패: {} / {}",
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
                    "output_tail": output
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

    let flow = flow_judged(&project_dir, &ids);
    if flow.is_empty() || flow.iter().any(|(_, v)| v.is_none()) {
        let missing_ev: Vec<String> = flow
            .iter()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.clone())
            .collect();
        return gate_err(json!({
            "boundary": "flow",
            "reason": "a `#### 확인 (#id)` written after the latest `#### 검증 (#id)` needs `- [x] <목적>` items each followed by `결과 : 성공` and `상세 : <actual value>`",
            "missing_evidence": missing_ev
        }));
    }

    let mut goals: BTreeMap<i64, String> = BTreeMap::new();
    for row in &fetched {
        goals.insert(row.id, goal_of(&project_dir, row)?);
    }
    let mut screen_lines = Vec::new();
    for row in &fetched {
        for (kind, text) in goal_lines(&goals[&row.id]) {
            if kind == "화면" {
                screen_lines.push(format!("- [화면] {text}"));
            }
        }
    }
    let screen_goal = screen_lines.join("\n");
    if !screen_goal.is_empty() {
        let out_str = flow
            .iter()
            .map(|(k, v)| format!("{k}\n{}", v.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("\n");
        let judgment = jev("match", None, &[("goal", &screen_goal), ("output", &out_str)]);
        if jev_blocks(&judgment) {
            return gate_err(json!({
                "boundary": "flow",
                "reason": "`#### 확인 (#id)` does not show the [화면] goals",
                "jev": judgment.get("items").cloned().unwrap_or(Value::Null)
            }));
        }
    }

    let mut result = Vec::new();
    for row in &fetched {
        if row.h_str() == SUBMITTED_STAGE {
            let users: Vec<String> = goal_lines(&goals[&row.id])
                .into_iter()
                .filter(|(k, _)| k == "사용자")
                .map(|(_, t)| t)
                .collect();
            let wait_str = if !users.is_empty() {
                users.join("; ")
            } else {
                waiting.to_string()
            };
            let note = format!(
                "{}. 흐름 검증 통과, 사용자 확인 대기: {wait_str}",
                records::next_number(row.d_str())
            );
            let mut upd = Map::new();
            upd.insert("H".into(), Value::String(WAITING_STAGE.into()));
            upd.insert("D".into(), Value::String(util::head(&note, 300)));
            result.push(api::update(TASK_TABLE, row.id, &upd)?);
        }
    }
    if !result.is_empty() {
        records::refresh_day_version(&project_dir)?;
    }
    Ok(Value::Array(result))
}

fn final_completed_list(project_dir: &Path, raw_id: i64) -> Vec<String> {
    let heading = Regex::new(&format!(r"^#+\s*(?:최종 완료|완료)\s+\(#{raw_id}\)\s*$")).unwrap();
    let item_re = Regex::new(r"^\s*(?:-\s*|\d+\.\s*)(?:\[([^\]]+)\]|<([^>]+)>)\s*(.+?)\s*$").unwrap();
    let mut items = Vec::new();
    for record in util::glob(&project_dir.join(".ai").join("changes"), &format!("{}.", util::today()), ".md") {
        let Ok(text) = std::fs::read_to_string(&record) else {
            continue;
        };
        for lines in records::sections_in(&text, &heading) {
            for line in lines {
                if let Some(caps) = item_re.captures(&line) {
                    let tag = caps.get(1).or_else(|| caps.get(2)).unwrap().as_str().trim();
                    if tag != " " && tag != "x" && tag != "X" {
                        items.push(line);
                    }
                }
            }
        }
    }
    items
}

pub fn task_complete(raw_id: i64, project_arg: &str) -> R<Value> {
    require_main("task-complete")?;
    let (project_dir, _, _) = records::owned(project_arg, raw_id)?;
    let row = rows(&[raw_id])?.into_iter().next().unwrap();
    let goal = goal_of(&project_dir, &row)?;
    if util::strip(&goal).is_empty() {
        return api::fail(
            "validation",
            format!("Table26 #{raw_id} has no goal"),
        );
    }
    let done_items = final_completed_list(&project_dir, raw_id);
    if done_items.is_empty() {
        return gate_err(json!({
            "boundary": "completion",
            "raw_id": raw_id,
            "record": format!(".ai/changes/{}.*.md #### 완료 (#{raw_id})", util::today())
        }));
    }
    let done_out = done_items.join("\n");
    let judgment = jev("match", Some(raw_id), &[("goal", &goal), ("output", &done_out)]);
    if jev_blocks(&judgment) {
        return gate_err(json!({
            "boundary": "completion",
            "raw_id": raw_id,
            "reason": "jev judged remaining work before completion",
            "jev": judgment.get("items").cloned().unwrap_or(Value::Null)
        }));
    }
    let mut upd = Map::new();
    upd.insert("H".into(), Value::String(DONE_STAGE.into()));
    upd.insert("F".into(), Value::Number(util::today_epoch().into()));
    api::update(TASK_TABLE, raw_id, &upd)
}
