//! Management records (`.ai/changes` `# 작업/# 완료/# 진행` and Obsidian `# 📆/# 📍`).

use crate::api::{self, Exit, R, DONE_STAGE, PROJECT_TABLE, TASK_TABLE};
use crate::util;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const RECORD_SECTIONS: [&str; 4] = ["요청", "작업", "완료", "진행"];
pub const NEW_FEATURE: &str = "기능 추가";
pub const CELL_SECTIONS: [(&str, &str); 3] = [("D", "메모"), ("I", "오류"), ("J", "원인")];

/// The single error-symbol set for `$J` cause lines and `$D` check notes written through `task-update`.
pub const ERROR_SYMBOLS: [&str; 10] = ["🅰️", "🗝", "❌", "⭕️", "🔢", "🛣", "👥", "🏷", "🚥", "🔳"];

/// `[<symbol>] <text>`, optionally prefixed by `N. `; the symbol may come with or without U+FE0F.
pub fn valid_symbol_line(line: &str) -> bool {
    let mut rest = line.trim();
    if let Some((num, body)) = rest.split_once(". ") {
        if !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) {
            rest = body.trim_start();
        }
    }
    let Some(inner) = rest.strip_prefix('[') else { return false };
    let Some((symbol, text)) = inner.split_once(']') else { return false };
    let bare = |s: &str| s.replace('\u{FE0F}', "");
    let known = ERROR_SYMBOLS.iter().any(|s| bare(s) == bare(symbol));
    known && text.starts_with(' ') && !text.trim().is_empty()
}

/// `task-update` CLI check: every `$J` line and the single `$D` line must start with an error symbol.
/// Grist `$I`·`$J`·`$D` cells hold only the latest single line; the numbered history lives in changes.
/// Rejects two or more lines and drops a leading `N. ` so changes appends the next number.
pub fn single_line_cells(fields: &mut Map<String, Value>) -> R<()> {
    for (cell, _) in CELL_SECTIONS {
        let Some(value) = fields.get(cell).filter(|v| !v.is_null()) else { continue };
        let text = util::val_str(value);
        let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if lines.len() > 1 {
            return api::fail("validation", format!("${cell} is one latest line in Grist; earlier lines stay in the changes list, so pass only the new line"));
        }
        if let Some(line) = lines.first() {
            let body = parse_numbered_line(line).map(|(_, b)| b.trim()).unwrap_or(line);
            fields.insert(cell.to_string(), Value::String(body.to_string()));
        }
    }
    Ok(())
}

pub fn check_symbol_fields(fields: &Map<String, Value>) -> R<()> {
    let symbols = ERROR_SYMBOLS.join("|");
    let lines = |key: &str| -> Option<Vec<String>> {
        fields.get(key).filter(|v| !v.is_null()).map(|v| {
            util::val_str(v).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
        })
    };
    if let Some(j) = lines("J") {
        if let Some(bad) = j.iter().find(|l| !valid_symbol_line(l)) {
            return api::fail("validation", format!("$J lines must start with [{symbols}] then text: `{bad}`"));
        }
    }
    if let Some(d) = lines("D") {
        if d.len() != 1 || !valid_symbol_line(&d[0]) {
            return api::fail("validation", format!("$D must be one check line starting with [{symbols}] then text"));
        }
    }
    Ok(())
}

/// Goal line body without its `[연결|기능|수정]` tag, for Table26 `$K`.
pub fn goal_k_line(body: &str) -> String {
    let body = body.trim();
    match body.strip_prefix('[').and_then(|r| r.split_once(']')) {
        Some((tag, rest)) if matches!(tag, "연결" | "기능" | "수정") => rest.trim().to_string(),
        _ => body.to_string(),
    }
}

pub fn check_goal_number(n: usize, m: usize) -> Result<(), String> {
    if m == 0 {
        return Err("goal missing; --goal needs `### 목표` lines".into());
    }
    if n == 0 || n > m {
        return Err(format!("--goal {n} out of range 1..={m}"));
    }
    Ok(())
}

/// Goal numbers in 1..=m not yet recorded by `task-step --goal` (gate state `goals_stepped`, else `goal_n`).
pub fn missing_goal_steps(state: &Map<String, Value>, m: usize) -> Vec<usize> {
    let mut stepped: BTreeSet<usize> = state
        .get("goals_stepped")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_u64()).map(|n| n as usize).collect())
        .unwrap_or_default();
    if let Some(n) = state.get("goal_n").and_then(|v| v.as_u64()) {
        stepped.insert(n as usize);
    }
    (1..=m).filter(|n| !stepped.contains(n)).collect()
}

/// `task-submit` check: every goal number needs a `task-step --goal` record.
pub fn check_goal_steps(project_dir: &Path, raw_id: i64) -> R<()> {
    let m = goal_bodies(project_dir, raw_id).map(|b| b.len()).unwrap_or(0);
    let missing = missing_goal_steps(&crate::gate::gate_load(project_dir, raw_id), m);
    if missing.is_empty() {
        return Ok(());
    }
    api::fail(
        "validation",
        format!("#{raw_id} goals without task-step --goal: {missing:?}; record each measurement unit before task-submit"),
    )
}

/// `task-update H=진단` rule: the same `--fields` must carry a non-empty `I` and a `D`.
pub fn diagnosis_fields_ok(fields: &Map<String, Value>) -> bool {
    if fields.get("H").and_then(|v| v.as_str()) != Some(crate::gate::ERROR_STAGE) {
        return true;
    }
    let filled = |key: &str| fields.get(key).filter(|v| !v.is_null()).is_some_and(|v| !util::val_str(v).trim().is_empty());
    filled("I") && filled("D")
}

pub fn check_diagnosis_fields(fields: &Map<String, Value>) -> R<()> {
    if diagnosis_fields_ok(fields) {
        return Ok(());
    }
    api::fail("validation", "H=진단 needs I (오류) and D ([오류 기호] 점검 한 줄) in the same task-update")
}

const NON_CONNECTIVE_GO: [&str; 7] = ["최고", "참고", "재고", "광고", "경고", "보고", "사고"];

fn is_hangul(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c)
}

/// True when the line joins two actions with `~고 `/`~고,` or ` 및 `.
fn joins_two_actions(line: &str) -> bool {
    if line.contains(" 및 ") {
        return true;
    }
    let tokens: Vec<&str> = line.split_whitespace().collect();
    tokens.iter().enumerate().any(|(i, token)| {
        let comma = token.ends_with(',');
        if !comma && i + 1 == tokens.len() {
            return false;
        }
        let word: String = token.trim_end_matches(',').chars().skip_while(|c| !is_hangul(*c)).collect();
        let chars: Vec<char> = word.chars().collect();
        chars.len() >= 2
            && chars.last() == Some(&'고')
            && chars.iter().all(|c| is_hangul(*c))
            && !NON_CONNECTIVE_GO.contains(&word.as_str())
    })
}

/// Goal lines that may bundle two results; `task-add`·`task-goal` print these without blocking.
pub fn goal_conjunction_warnings(goal_text: &str) -> Vec<String> {
    goal_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .enumerate()
        .filter_map(|(i, line)| {
            let (n, body) = parse_numbered_line(line).unwrap_or((i + 1, line));
            joins_two_actions(body).then(|| format!("goal {n}: {body} — 두 결과를 한 줄에 묶었는지 확인(inquire-task)"))
        })
        .collect()
}

/// Adds `"warnings"` to a JSON object output only when there are any.
pub fn attach_warnings(out: &mut Value, warnings: Vec<String>) {
    if warnings.is_empty() {
        return;
    }
    if let Some(obj) = out.as_object_mut() {
        obj.insert("warnings".into(), json!(warnings));
    }
}
pub const VERSION_FILES: [&str; 8] = [
    "package.json",
    "manifest.json",
    "public/manifest.json",
    "plugin.json",
    ".codex-plugin/plugin.json",
    "pyproject.toml",
    "Cargo.toml",
    "blender_manifest.toml",
];

static HOOK_PROJECT: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_hook_project(project: Option<&str>) {
    if let Some(p) = project {
        if !p.is_empty() {
            *HOOK_PROJECT.lock().unwrap() = Some(util::resolve(Path::new(p)));
        }
    }
}

pub fn change_type_code(kind: &str) -> Option<&'static str> {
    match kind {
        "기능 추가" | "개선" => Some("기능"),
        "오류 해결" => Some("오류"),
        _ => None,
    }
}

pub fn extensions_root() -> PathBuf {
    std::env::var_os("GRIST_EXTENSIONS_ROOT")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/tree/extensions"))
}

pub fn vault_root() -> PathBuf {
    std::env::var_os("OBSIDIAN_VAULT")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/mnt/c/Users/tende/OneDrive/일기장"))
}

pub fn has_table7_id(text: &str) -> Option<i64> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?ms)^\s*#\s*상태\b.*?^\s*###\s*grist\s*\n\s*ID\s*:\s*(\d+)\s*$").unwrap()
    });
    re.captures(text).and_then(|c| c[1].parse::<i64>().ok())
}

pub fn has_obsidian_doc(text: &str) -> Option<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?ms)^\s*#\s*상태\b.*?^\s*###\s*obsidian\s*\n\s*문서\s*:\s*([^\n{}]+)\s*$").unwrap()
    });
    re.captures(text)
        .map(|c| util::strip(&c[1]).to_string())
        .filter(|s| !s.is_empty())
}

fn section_ids_in_record(record: &Path, section_name: &str) -> Vec<i64> {
    let Ok(text) = std::fs::read_to_string(record) else {
        return Vec::new();
    };
    let lines = util::splitlines(&text);
    let Some(start) = find(&lines, 1, section_name, 0, None) else {
        return Vec::new();
    };
    let end = span(&lines, start, 1);
    let mut ids = Vec::new();
    for i in numbered(&lines, start, end) {
        if let Some((_, id_part)) = lines[i]
            .trim_end()
            .strip_suffix(')')
            .and_then(|s| s.rsplit_once("(#"))
        {
            if let Ok(id) = id_part.parse::<i64>() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

pub fn changes_task_ids(project_dir: &Path) -> Vec<i64> {
    let changes_dir = project_dir.join(".ai").join("changes");
    for record in util::glob(&changes_dir, "", ".md").into_iter().rev() {
        let mut ids = section_ids_in_record(&record, "작업");
        for id in section_ids_in_record(&record, "완료") {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if !ids.is_empty() {
            return ids;
        }
    }
    Vec::new()
}

pub fn find_task_agents(start: &Path) -> Option<(Vec<i64>, PathBuf)> {
    let ext = util::normalize(&extensions_root());
    let start_norm = util::normalize(start);
    let mut cur = start_norm.clone();
    loop {
        if cur == ext && start_norm != ext {
            if !cur.pop() {
                break;
            }
            continue;
        }
        let agents = cur.join("AGENTS.md");
        if agents.is_file() {
            if let Ok(text) = std::fs::read_to_string(&agents) {
                if has_table7_id(&text).is_some() {
                    let ids = changes_task_ids(&cur);
                    if !ids.is_empty() {
                        return Some((ids, cur));
                    }
                }
            }
        }
        if !cur.pop() {
            break;
        }
    }
    None
}

pub fn project(project_arg: &str) -> R<(PathBuf, String, Vec<i64>)> {
    let project_dir = util::resolve(Path::new(project_arg));
    let agents = project_dir.join("AGENTS.md");
    if !agents.is_file() {
        return api::fail("validation", format!("{} does not exist", util::path_str(&agents)));
    }
    let text = std::fs::read_to_string(&agents).map_err(api::other)?;
    if has_table7_id(&text).is_none() {
        return api::fail(
            "validation",
            format!("{} has no `# 상태` `### grist` `ID :` line", util::path_str(&agents)),
        );
    }
    let ids = changes_task_ids(&project_dir);
    if ids.is_empty() {
        return api::fail(
            "validation",
            format!("{} has no active tasks in .ai/changes (`# 작업`)", util::path_str(&project_dir)),
        );
    }
    Ok((project_dir, text, ids))
}

pub fn owned(project_arg: &str, raw_id: i64) -> R<(PathBuf, String, Vec<i64>)> {
    let project_dir = util::resolve(Path::new(project_arg));
    let agents = project_dir.join("AGENTS.md");
    if !agents.is_file() {
        return api::fail("validation", format!("{} does not exist", util::path_str(&agents)));
    }
    let text = std::fs::read_to_string(&agents).map_err(api::other)?;
    let Some(t7_id) = has_table7_id(&text) else {
        return api::fail(
            "validation",
            format!("{} has no `# 상태` `### grist` `ID :` line", util::path_str(&agents)),
        );
    };
    let mut ids = changes_task_ids(&project_dir);
    if !ids.contains(&raw_id) {
        if item_file(&project_dir, raw_id).is_some() {
            ids.push(raw_id);
        } else {
            let fetched = api::get(TASK_TABLE, Some(&[raw_id]), None, Some(&["A".into()]))?;
            let row_t7 = fetched
                .first()
                .and_then(|r| r.get("fields"))
                .and_then(|f| f.get("A"))
                .and_then(|v| v.as_i64());
            if row_t7 == Some(t7_id) {
                ids.push(raw_id);
            } else {
                return api::fail(
                    "validation",
                    format!(
                        "{} (Table7 #{t7_id}) does not own Table26 #{raw_id}",
                        util::path_str(&agents)
                    ),
                );
            }
        }
    }
    Ok((project_dir, text, ids))
}

pub fn next_number(cell: Option<&str>) -> i64 {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^\s*(\d+)\.").unwrap());
    re.captures(cell.unwrap_or(""))
        .and_then(|c| c[1].parse::<i64>().ok())
        .map(|n| n + 1)
        .unwrap_or(1)
}

pub fn record_title(title: Option<&str>) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"[\s/\\:*?"<>|#]+"#).unwrap());
    let cleaned = re.replace_all(title.unwrap_or(""), "").into_owned();
    let base = if cleaned.is_empty() { "작업".to_string() } else { cleaned };
    util::head(&base, 9)
}

pub fn changes_record(project_dir: &Path, title: Option<&str>) -> R<PathBuf> {
    let changes = project_dir.join(".ai").join("changes");
    let existing = util::glob(&changes, &format!("{}.", util::today()), ".md");
    if let Some(first) = existing.into_iter().next() {
        return Ok(first);
    }
    std::fs::create_dir_all(&changes).map_err(api::other)?;
    let record = changes.join(format!("{}.{}.md", util::today(), record_title(title)));
    let mut init = String::new();
    for name in RECORD_SECTIONS {
        init.push_str(&format!("# {name}\n\n"));
    }
    std::fs::write(&record, init).map_err(api::other)?;
    Ok(record)
}

extern "C" {
    fn flock(fd: std::os::raw::c_int, operation: std::os::raw::c_int) -> std::os::raw::c_int;
}

pub fn with_lock<T, F: FnOnce() -> R<T>>(target: &Path, f: F) -> R<T> {
    let resolved = util::resolve(target);
    let lock_path = std::env::temp_dir().join(format!("grist-record-{}.lock", util::sanitize_key(&util::path_str(&resolved))));
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&lock_path)
        .map_err(api::other)?;
    let fd = file.as_raw_fd();
    unsafe {
        flock(fd, 2); // LOCK_EX
    }
    let res = f();
    unsafe {
        flock(fd, 8); // LOCK_UN
    }
    res
}

/// Append a record block to the end of the row's own `### 구현` section, never the file end.
pub fn append_item(project_dir: &Path, raw_id: i64, text: &str) -> R<()> {
    let block: Vec<String> = util::splitlines(text).into_iter().skip_while(|l| util::strip(l).is_empty()).collect();
    let placed = edit_item(project_dir, raw_id, |lines| {
        let (start, end) = subsection(lines, raw_id, "구현");
        let at = content_end(lines, start, end);
        let mut insert = vec![String::new()];
        insert.extend(block.iter().cloned());
        lines.splice(at..at, insert);
    })?;
    match placed {
        Some(()) => Ok(()),
        None => api::fail("records", format!("Table26 #{raw_id} block not found in .ai/changes")),
    }
}

pub fn sections_in(text: &str, heading: &Regex) -> Vec<Vec<String>> {
    let lines = util::splitlines(text);
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with('#') && heading.is_match(line) {
            let level = line.len() - line.trim_start_matches('#').len();
            let mut body = Vec::new();
            for rest in &lines[index + 1..] {
                if rest.starts_with('#') && (rest.len() - rest.trim_start_matches('#').len()) <= level {
                    break;
                }
                body.push(rest.clone());
            }
            found.push(body);
        }
    }
    found
}

pub fn heading(line: &str) -> Option<(usize, String)> {
    if !line.starts_with('#') {
        return None;
    }
    let level = line.bytes().take_while(|&b| b == b'#').count();
    let rest = &line[level..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let cleaned = util::collapse_space(&util::strip(rest).replace('\u{fe0f}', ""));
    Some((level, cleaned))
}

pub fn span(lines: &[String], start: usize, level: usize) -> usize {
    for (index, line) in lines.iter().enumerate().skip(start + 1) {
        if let Some((lvl, _)) = heading(line) {
            if lvl <= level {
                return index;
            }
        }
    }
    lines.len()
}

pub fn find(lines: &[String], level: usize, text: &str, start: usize, end: Option<usize>) -> Option<usize> {
    let limit = end.unwrap_or(lines.len()).min(lines.len());
    for index in start..limit {
        if let Some((lvl, t)) = heading(&lines[index]) {
            if lvl == level && t == text {
                return Some(index);
            }
        }
    }
    None
}

pub fn content_end(lines: &[String], start: usize, end: usize) -> usize {
    let mut index = end;
    while index > start + 1 && util::strip(&lines[index - 1]).is_empty() {
        index -= 1;
    }
    index
}

fn parse_numbered_line(line: &str) -> Option<(usize, &str)> {
    let trimmed = util::lstrip(line);
    let dot = trimmed.find('.')?;
    if dot == 0 || !trimmed.as_bytes()[..dot].iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let after_dot = &trimmed[dot + 1..];
    if !after_dot.starts_with(char::is_whitespace) {
        return None;
    }
    let num = trimmed[..dot].parse::<usize>().ok()?;
    Some((num, util::strip(after_dot)))
}

pub fn numbered(lines: &[String], start: usize, end: usize) -> Vec<usize> {
    ((start + 1)..end.min(lines.len()))
        .filter(|&i| parse_numbered_line(&lines[i]).is_some())
        .collect()
}

pub fn renumber(lines: &mut Vec<String>, start: usize, end: usize) {
    let idxs = numbered(lines, start, end);
    for (num, idx) in idxs.into_iter().enumerate() {
        let body = parse_numbered_line(&lines[idx]).map(|(_, b)| b.to_string()).unwrap_or_default();
        lines[idx] = format!("{}. {body}", num + 1);
    }
}

pub fn add_numbered(lines: &mut Vec<String>, start: usize, end: usize, text: &str) -> bool {
    let (given_num, body) = if let Some((n, b)) = parse_numbered_line(text) {
        (Some(n), b.to_string())
    } else {
        (None, util::strip(text).to_string())
    };
    let existing = numbered(lines, start, end);
    for &i in &existing {
        if let Some((_, b)) = parse_numbered_line(&lines[i]) {
            if b == body {
                return false;
            }
        }
    }
    let number = given_num.unwrap_or(existing.len() + 1);
    let at = content_end(lines, start, end);
    lines.insert(at, format!("{number}. {body}"));
    true
}

/// Adds missing record sections in `RECORD_SECTIONS` order: before the next existing section, else at the end.
pub fn ensure_sections(lines: &mut Vec<String>) {
    for (pos, name) in RECORD_SECTIONS.iter().enumerate() {
        if find(lines, 1, name, 0, None).is_some() {
            continue;
        }
        let next = RECORD_SECTIONS[pos + 1..].iter().find_map(|later| find(lines, 1, later, 0, None));
        match next {
            Some(at) => {
                lines.insert(at, String::new());
                lines.insert(at, format!("# {name}"));
            }
            None => {
                if lines.last().map(|l| !util::strip(l).is_empty()).unwrap_or(false) {
                    lines.push(String::new());
                }
                lines.push(format!("# {name}"));
                lines.push(String::new());
            }
        }
    }
}

/// End of the level-1 section that starts at `start`.
fn level1_end(lines: &[String], start: usize) -> usize {
    ((start + 1)..lines.len())
        .find(|&i| matches!(heading(&lines[i]), Some((1, _))))
        .unwrap_or(lines.len())
}

fn item_heading_parts(line: &str) -> Option<(String, i64)> {
    let rest = line.strip_prefix("## ")?.trim();
    let (head, tail) = rest.rsplit_once(char::is_whitespace)?;
    let id_str = tail
        .strip_prefix("(#")
        .and_then(|s| s.strip_suffix(')'))?;
    let raw_id = id_str.parse::<i64>().ok()?;
    let title = head
        .trim()
        .trim_start_matches("[구현]")
        .trim()
        .to_string();
    if title.is_empty() {
        return None;
    }
    Some((title, raw_id))
}

pub fn item_file(project_dir: &Path, raw_id: i64) -> Option<PathBuf> {
    for record in util::glob(&project_dir.join(".ai").join("changes"), "", ".md").into_iter().rev() {
        if let Ok(text) = std::fs::read_to_string(&record) {
            for line in text.lines() {
                if let Some((_, id)) = item_heading_parts(line) {
                    if id == raw_id {
                        return Some(record);
                    }
                }
            }
        }
    }
    None
}

pub fn item_span(lines: &[String], raw_id: i64) -> Option<(usize, usize)> {
    for (index, line) in lines.iter().enumerate() {
        if let Some((_, id)) = item_heading_parts(line) {
            if id == raw_id {
                return Some((index, span(lines, index, 2)));
            }
        }
    }
    None
}

pub fn write_atomic(path: &Path, text: &str) -> R<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.tmp"));
    std::fs::write(&temp, text).map_err(api::other)?;
    std::fs::rename(&temp, path).map_err(api::other)?;
    Ok(())
}

pub fn edit_item<T, F: FnOnce(&mut Vec<String>) -> T>(project_dir: &Path, raw_id: i64, edit: F) -> R<Option<T>> {
    with_lock(&project_dir.join(".ai").join("changes"), || {
        let Some(record) = item_file(project_dir, raw_id) else {
            return Ok(None);
        };
        let before = std::fs::read_to_string(&record).map_err(api::other)?;
        let mut lines = util::splitlines(&before);
        let result = edit(&mut lines);
        let text = format!("{}\n", lines.join("\n"));
        if text != before {
            write_atomic(&record, &text)?;
        }
        Ok(Some(result))
    })
}

pub fn format_judge(title: &str, judge: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^\s*\[([^\]]+)\]\s*(.*?)\s*$").unwrap());
    let cleaned_title = util::strip(
        title
            .trim_start()
            .strip_prefix("[구현]")
            .unwrap_or(title),
    );
    let fallback_body = if cleaned_title.is_empty() { "동작 확인" } else { cleaned_title };
    let j = util::strip(judge);
    if j.is_empty() {
        return format!("[상태] {fallback_body}");
    }
    if let Some(caps) = re.captures(j) {
        let tag = match caps[1].trim() {
            "값" => "값",
            "상태" => "상태",
            _ => "상태",
        };
        let rest = util::strip(&caps[2]);
        let body = if rest.is_empty() { fallback_body } else { rest };
        format!("[{tag}] {body}")
    } else {
        format!("[상태] {j}")
    }
}

pub fn register_item(project_dir: &Path, raw_id: i64, title: &str, kind: &str, judge: &str) -> R<PathBuf> {
    with_lock(&project_dir.join(".ai").join("changes"), || {
        if let Some(existing) = item_file(project_dir, raw_id) {
            return Ok(existing);
        }
        let record = changes_record(project_dir, Some(title))?;
        let text = std::fs::read_to_string(&record).map_err(api::other)?;
        let mut lines = util::splitlines(&text);
        ensure_sections(&mut lines);
        let start = find(&lines, 1, "작업", 0, None).unwrap();
        let end = span(&lines, start, 1);
        add_numbered(&mut lines, start, end, &format!("{title} (#{raw_id})"));
        let start = find(&lines, 1, "진행", 0, None).unwrap();
        let end = span(&lines, start, 1);
        let judge_str = format_judge(title, judge);
        let heading_line = format!("## [구현] {title} (#{raw_id})");
        let mut block = vec![
            heading_line,
            format!("- 유형 : {kind}"),
            format!("- 판정 : {judge_str}"),
            "### 목표".to_string(),
            String::new(),
            "### 메모".to_string(),
            String::new(),
            "### 오류".to_string(),
            String::new(),
            "### 원인".to_string(),
            String::new(),
            "### 구현".to_string(),
            String::new(),
        ];
        let at = content_end(&lines, start, end);
        if at > 0 && !util::strip(&lines[at - 1]).is_empty() {
            block.insert(0, String::new());
        }
        lines.splice(at..at, block);
        write_atomic(&record, &format!("{}\n", lines.join("\n")))?;
        Ok(record)
    })
}

pub fn subsection(lines: &mut Vec<String>, raw_id: i64, name: &str) -> (usize, usize) {
    let (start, end) = item_span(lines, raw_id).unwrap();
    let index = match find(lines, 3, name, start, Some(end)) {
        Some(idx) => idx,
        None => {
            let at = content_end(lines, start, end);
            lines.splice(at..at, [format!("### {name}"), String::new()]);
            at
        }
    };
    let sub_end = span(lines, index, 3);
    (index, sub_end)
}

pub fn record_cell(project_dir: &Path, raw_id: i64, name: &str, value: &str) -> R<bool> {
    let lines_in: Vec<String> = util::splitlines(value)
        .into_iter()
        .map(|l| util::strip(&l).to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if lines_in.is_empty() {
        return Ok(false);
    }
    let res = edit_item(project_dir, raw_id, |lines| {
        let mut changed = false;
        for line in &lines_in {
            let (start, end) = subsection(lines, raw_id, name);
            if add_numbered(lines, start, end, line) {
                changed = true;
            }
        }
        changed
    })?;
    Ok(res.unwrap_or(false))
}

/// Append one agent identity to the changes memo without writing Table26 `$D`.
pub fn record_agent_memo(project_dir: &Path, raw_id: i64, line: &str) -> R<bool> {
    if !line.starts_with("에이전트 ") || line.trim() != line || line.contains(['\n', '\r']) {
        return api::fail("validation", "task-memo needs one agent line starting with `에이전트 `");
    }
    if item_file(project_dir, raw_id).is_none() {
        return api::fail("records", format!("Table26 #{raw_id} block not found in .ai/changes"));
    }
    record_cell(project_dir, raw_id, "메모", line)
}

pub fn task_memo(project_arg: &str, raw_id: i64, line: &str) -> R<Value> {
    let (project_dir, _, _) = owned(project_arg, raw_id)?;
    let added = record_agent_memo(&project_dir, raw_id, line)?;
    Ok(json!({ "id": raw_id, "added": added }))
}

pub struct ItemInfo {
    pub title: String,
    pub kind: String,
}

pub fn item_info(project_dir: &Path, raw_id: i64) -> Option<ItemInfo> {
    let record = item_file(project_dir, raw_id)?;
    let text = std::fs::read_to_string(&record).ok()?;
    let lines = util::splitlines(&text);
    let (start, end) = item_span(&lines, raw_id)?;
    let kind = lines[start..end]
        .iter()
        .find_map(|l| {
            let s = l.trim_start().strip_prefix('-')?.trim_start();
            let s = s.strip_prefix("유형")?.trim_start();
            let s = s.strip_prefix(':')?.trim();
            if s.is_empty() { None } else { Some(s.to_string()) }
        })
        .unwrap_or_default();
    let (title, _) = item_heading_parts(&lines[start])?;
    Some(ItemInfo { title, kind })
}

pub fn complete_item(project_dir: &Path, raw_id: i64) -> R<bool> {
    let suffix = format!("(#{raw_id})");
    let res = edit_item(project_dir, raw_id, |lines| {
        ensure_sections(lines);
        let start = find(lines, 1, "작업", 0, None).unwrap();
        let end = span(lines, start, 1);
        let found: Vec<usize> = numbered(lines, start, end)
            .into_iter()
            .filter(|&i| lines[i].trim_end().ends_with(&suffix))
            .collect();
        if found.is_empty() {
            return false;
        }
        let text = parse_numbered_line(&lines[found[0]])
            .map(|(_, body)| body.to_string())
            .unwrap_or_default();
        lines.remove(found[0]);
        let new_end = span(lines, start, 1);
        renumber(lines, start, new_end);
        let done = find(lines, 1, "완료", 0, None).unwrap();
        let done_end = span(lines, done, 1);
        add_numbered(lines, done, done_end, &text);
        let done_end2 = span(lines, done, 1);
        renumber(lines, done, done_end2);
        true
    })?;
    Ok(res.unwrap_or(false))
}

fn confirmed_completion_line(lines: &[String], raw_id: i64) -> Option<String> {
    let heading = format!("#### 확인 (#{raw_id})");
    let start = lines.iter().rposition(|line| line.trim() == heading)?;
    let evidence: Vec<&str> = lines[start + 1..].iter()
        .map(|line| line.trim())
        .take_while(|line| !line.starts_with('#'))
        .collect();
    let summary = evidence.iter().find_map(|line| line.strip_prefix("- 요약 :").map(str::trim))?;
    let marked = summary.strip_prefix('[')?;
    let (symbol, body) = marked.split_once(']')?;
    let body = body.trim();
    if symbol.trim().is_empty() || body.is_empty() || !body.contains('⇒') {
        return None;
    }
    let checked = evidence.iter().any(|line| line.starts_with("- [x] "));
    let success = evidence.iter().any(|line| *line == "결과 : 성공");
    let detail = evidence.iter().any(|line| line.strip_prefix("상세 :").map(str::trim).is_some_and(|v| !v.is_empty() && v != "-"));
    if checked && success && detail {
        Some(format!("1. [{}] {body}", symbol.trim()))
    } else {
        None
    }
}

pub fn ensure_completion_note(project_dir: &Path, raw_id: i64) -> R<bool> {
    Ok(edit_item(project_dir, raw_id, |lines| {
        let Some((item_start, item_end)) = item_span(lines, raw_id) else { return false; };
        let Some(impl_start) = find(lines, 3, "구현", item_start, Some(item_end)) else { return false; };
        let impl_end = span(lines, impl_start, 3).min(item_end);
        let heading = format!("#### 완료 (#{raw_id})");
        if lines[impl_start + 1..impl_end].iter().any(|line| line.trim() == heading) { return false; }
        let actions = completion_action_lines(&lines[impl_start + 1..impl_end]);
        let mut note = vec![heading];
        if actions.is_empty() {
            let Some(line) = confirmed_completion_line(&lines[impl_start + 1..impl_end], raw_id) else { return false; };
            note.push(line);
        } else {
            note.extend(actions);
        }
        lines.splice(impl_end..impl_end, note);
        true
    })?.unwrap_or(false))
}

fn completion_action_lines(lines: &[String]) -> Vec<String> {
    let actions: Vec<String> = lines.iter().filter_map(|line| {
        let trimmed = line.trim();
        let (text, numbered) = if let Some(text) = trimmed.strip_prefix("- [▶]") {
            (text.trim(), true)
        } else {
            (trimmed.strip_prefix("- [○]")?.trim(), false)
        };
        let text = if numbered {
            text.split_once(". ")
                .filter(|(number, _)| number.parse::<usize>().is_ok())
                .map_or(text, |(_, body)| body.trim())
        } else { text };
        if text.is_empty() { None } else { Some(text.to_string()) }
    }).collect();
    actions.iter().rev().take(5).rev().enumerate()
        .map(|(index, action)| format!("{}. [🏗️] {}", index + 1, action))
        .collect()
}

pub fn has_completion_note(project_dir: &Path, raw_id: i64) -> bool {
    let Some(record) = item_file(project_dir, raw_id) else { return false; };
    let Ok(text) = std::fs::read_to_string(record) else { return false; };
    let lines = util::splitlines(&text);
    let Some((start, end)) = item_span(&lines, raw_id) else { return false; };
    let heading = format!("#### 완료 (#{raw_id})");
    lines[start..end].iter().any(|line| line.trim() == heading)
}

/// The one `### 목표` parser: goal bodies (without numbering) of an item, in order.
fn goal_bodies(project_dir: &Path, raw_id: i64) -> Option<Vec<String>> {
    let record = item_file(project_dir, raw_id)?;
    let text = std::fs::read_to_string(&record).ok()?;
    let lines = util::splitlines(&text);
    let (start, end) = item_span(&lines, raw_id)?;
    let index = find(&lines, 3, "목표", start, Some(end))?;
    let sub_end = span(&lines, index, 3);
    let out: Vec<String> = ((index + 1)..sub_end)
        .filter_map(|i| parse_numbered_line(&lines[i]).map(|(_, body)| body.to_string()))
        .collect();
    if out.is_empty() { None } else { Some(out) }
}

pub fn changes_goal(project_dir: &Path, raw_id: i64) -> Option<String> {
    let bodies = goal_bodies(project_dir, raw_id)?;
    Some(bodies.iter().map(|b| format!("- {b}")).collect::<Vec<_>>().join("\n"))
}

/// `1. [기능] ...` lines of the item's goal, joined by newline (what `grist goal` prints).
pub fn goal_text(project_arg: &str, raw_id: i64) -> R<String> {
    let (project_dir, _, _) = owned(project_arg, raw_id)?;
    match goal_bodies(&project_dir, raw_id) {
        Some(b) => Ok(b.iter().enumerate().map(|(n, t)| format!("{}. {t}", n + 1)).collect::<Vec<_>>().join("\n")),
        None => api::fail("goal", format!("goal missing (#{raw_id})")),
    }
}

pub fn write_changes_goal(project_dir: &Path, raw_id: i64, goal: &str) -> R<()> {
    let goal_lines: Vec<String> = util::splitlines(goal)
        .into_iter()
        .filter(|l| !util::strip(l).is_empty())
        .map(|l| {
            if let Some((_, body)) = parse_numbered_line(&l) {
                return body.to_string();
            }
            let trimmed = l.trim_start();
            let stripped = trimmed.strip_prefix('-').map(|s| s.trim_start()).unwrap_or(trimmed);
            util::strip(stripped).to_string()
        })
        .collect();
    edit_item(project_dir, raw_id, |lines| {
        let (start, end) = subsection(lines, raw_id, "목표");
        let mut replacement: Vec<String> = goal_lines
            .iter()
            .enumerate()
            .map(|(n, text)| format!("{}. {text}", n + 1))
            .collect();
        replacement.push(String::new());
        lines.splice((start + 1)..end, replacement);
    })?;
    Ok(())
}

pub fn canonical_goal(goal: &str) -> String {
    util::splitlines(goal)
        .into_iter()
        .map(|l| util::strip(&l).to_string())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn obsidian_file(project_dir: &Path) -> R<PathBuf> {
    if let Ok(override_path) = std::env::var("GRIST_OBSIDIAN_FILE") {
        if !override_path.is_empty() {
            return Ok(PathBuf::from(override_path));
        }
    }
    let agents = project_dir.join("AGENTS.md");
    let agents_text = std::fs::read_to_string(&agents).unwrap_or_default();
    let Some(t7_id) = has_table7_id(&agents_text) else {
        return api::fail(
            "obsidian",
            format!("{} has no `# 상태` `### grist` `ID :` line", util::path_str(&agents)),
        );
    };
    let rows = api::get(PROJECT_TABLE, Some(&[t7_id]), None, Some(&["Obsidian_Id".into()]))?;
    let note = rows
        .first()
        .and_then(|r| r.get("fields"))
        .and_then(|f| f.get("Obsidian_Id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| has_obsidian_doc(&agents_text));
    let Some(note) = note else {
        return api::fail("obsidian", format!("Table7 #{t7_id} has no Obsidian_Id"));
    };
    Ok(vault_root().join(note))
}

fn mtime_ns(path: &Path) -> Option<u128> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_nanos())
}

pub fn edit_note<F: FnMut(&mut Vec<String>) -> R<()>>(note: &Path, mut edit: F) -> R<bool> {
    with_lock(note, || {
        let name = note.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        for _ in 0..3 {
            let Some(stamp) = mtime_ns(note) else {
                return api::fail("obsidian", format!("{name} is not readable"));
            };
            let Ok(before) = std::fs::read_to_string(note) else {
                return api::fail("obsidian", format!("{name} is not readable"));
            };
            let mut lines = util::splitlines(&before);
            edit(&mut lines)?;
            let text = format!("{}\n", lines.join("\n"));
            if text.trim_end_matches('\n') == before.trim_end_matches('\n') {
                return Ok(false);
            }
            if mtime_ns(note) != Some(stamp) {
                continue;
            }
            write_atomic(note, &text)?;
            return Ok(true);
        }
        api::fail("obsidian", format!("{name} kept changing while it was edited"))
    })
}

pub fn note_section(lines: &[String], name: &str, note: &Path) -> R<(usize, usize)> {
    let note_name = note.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let Some(index) = find(lines, 1, name, 0, None) else {
        return api::fail(
            "obsidian",
            format!("{note_name} has no `# {name}` heading; restructure it to the 초기문서 format"),
        );
    };
    let end = span(lines, index, 1);
    let marker = ((index + 1)..end)
        .find(|&i| util::strip(&lines[i]).starts_with('🔖'))
        .unwrap_or(end);
    Ok((index, marker))
}

pub fn add_feature(project_dir: &Path, title: &str) -> R<bool> {
    let note = obsidian_file(project_dir)?;
    edit_note(&note, |lines| {
        let (start, end) = note_section(lines, "📍", &note)?;
        let target = format!("- {title}");
        let exists = lines[(start + 1)..end].iter().any(|l| util::strip(l) == target);
        if !exists {
            let at = content_end(lines, start, end);
            lines.insert(at, target);
        }
        Ok(())
    })
}

pub fn project_version(project_dir: &Path) -> Option<String> {
    let ver_re = Regex::new(r#"(?m)^version\s*=\s*"([^"]+)""#).unwrap();
    for name in VERSION_FILES {
        let file = project_dir.join(name);
        if !file.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let value = if name.ends_with(".json") {
            serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.get("version").and_then(|s| s.as_str()).map(|s| s.to_string()))
        } else {
            ver_re.captures(&text).map(|c| c[1].to_string())
        };
        if let Some(v) = value {
            let s = util::strip(&v);
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn day_line_str(day: &str, version: Option<&str>, summary: &str) -> String {
    match version {
        Some(v) if !v.is_empty() => format!("{day} : [{v}] {summary}"),
        _ => format!("{day} : {summary}"),
    }
}

fn day_line_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\d{4}) : (?:\[([^\]]*)\] )?(.*)$").unwrap())
}

pub fn calendar_day(lines: &mut Vec<String>, note: &Path, y: i64, m: u32) -> R<(usize, usize)> {
    let bound = |lines: &[String], index: usize, level: usize| -> R<usize> {
        let (_, marker) = note_section(lines, "📆", note)?;
        Ok(span(lines, index, level).min(marker))
    };
    let (start, _) = note_section(lines, "📆", note)?;

    let ensure_child = |lines: &mut Vec<String>, parent: usize, level: usize, text: &str| -> R<usize> {
        let b = bound(lines, parent, level - 1)?;
        for at in (parent + 1)..b {
            if heading(&lines[at]) == Some((level, text.to_string())) {
                return Ok(at);
            }
        }
        lines.insert(parent + 1, format!("{} {text}", "#".repeat(level)));
        Ok(parent + 1)
    };

    let year = ensure_child(lines, start, 2, &format!("{y:04}"))?;
    let month = ensure_child(lines, year, 3, &format!("{m}월"))?;
    let end = bound(lines, month, 3)?;
    let body: Vec<String> = lines[(month + 1)..end].to_vec();
    let re = day_line_re();
    let mut days: Vec<String> = body
        .iter()
        .filter(|l| re.is_match(util::strip(l)))
        .map(|l| util::strip(l).to_string())
        .collect();
    days.sort_by(|a, b| {
        let ak: String = a.chars().take(4).collect();
        let bk: String = b.chars().take(4).collect();
        bk.cmp(&ak)
    });
    let days_len = days.len();
    let others: Vec<String> = body
        .into_iter()
        .filter(|l| !re.is_match(util::strip(l)))
        .collect();
    days.extend(others);
    lines.splice((month + 1)..end, days);
    Ok((month, month + 1 + days_len))
}

pub fn add_request(project_dir: &Path, _title: &str, summary: &str) -> R<bool> {
    let note = obsidian_file(project_dir)?;
    let (y, m, d) = util::seoul_date();
    let day = format!("{m:02}{d:02}");
    let version = project_version(project_dir);
    edit_note(&note, |lines| {
        let (month, end) = calendar_day(lines, &note, y, m)?;
        let prefix = format!("{day} : ");
        let found = ((month + 1)..end).find(|&at| lines[at].starts_with(&prefix));
        if let Some(at) = found {
            lines[at] = day_line_str(&day, version.as_deref(), summary);
        } else {
            lines.insert(month + 1, day_line_str(&day, version.as_deref(), summary));
            calendar_day(lines, &note, y, m)?;
        }
        Ok(())
    })
}

pub fn refresh_day_version(project_dir: &Path) -> R<bool> {
    let Some(version) = project_version(project_dir) else {
        return Ok(false);
    };
    let note = obsidian_file(project_dir)?;
    let (y, m, d) = util::seoul_date();
    let day = format!("{m:02}{d:02}");
    let re = day_line_re();
    edit_note(&note, |lines| {
        let (month, end) = calendar_day(lines, &note, y, m)?;
        for at in (month + 1)..end {
            let current = lines[at].clone();
            if let Some(caps) = re.captures(&current) {
                if &caps[1] == day {
                    lines[at] = day_line_str(&day, Some(&version), &caps[3]);
                }
            }
        }
        Ok(())
    })
}

pub fn hook_project(raw_id: i64) -> Option<PathBuf> {
    if let Some(p) = HOOK_PROJECT.lock().unwrap().clone() {
        return Some(p);
    }
    let start = util::resolve(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    let mut cur = start;
    loop {
        let agents = cur.join("AGENTS.md");
        if agents.is_file() {
            if let Ok(text) = std::fs::read_to_string(&agents) {
                if has_table7_id(&text).is_some() && item_file(&cur, raw_id).is_some() {
                    return Some(cur);
                }
            }
        }
        if !cur.pop() {
            break;
        }
    }
    let ext = extensions_root();
    let mut candidates = Vec::new();
    if ext.is_dir() {
        candidates.push(ext.clone());
        if let Ok(entries) = std::fs::read_dir(&ext) {
            let mut subdirs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            subdirs.sort();
            candidates.extend(subdirs);
        }
    }
    for dir in &candidates {
        if dir.join(".ai").join("changes").is_dir() && item_file(dir, raw_id).is_some() {
            return Some(dir.clone());
        }
    }
    None
}

pub fn apply_row(project_dir: &Path, raw_id: i64, fields: &Map<String, Value>) -> R<Vec<String>> {
    let mut done = Vec::new();
    let mut cell_inputs: Vec<(&'static str, Vec<String>)> = Vec::new();
    for (cell, name) in CELL_SECTIONS {
        if util::truthy(fields.get(cell)) {
            let val = fields.get(cell).map(util::val_str).unwrap_or_default();
            let lines_in: Vec<String> = util::splitlines(&val)
                .into_iter()
                .map(|l| util::strip(&l).to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if !lines_in.is_empty() {
                cell_inputs.push((name, lines_in));
            }
        }
    }
    if !cell_inputs.is_empty() {
        if let Some(applied_names) = edit_item(project_dir, raw_id, |lines| {
            let mut changed_names = Vec::new();
            for (name, lines_in) in &cell_inputs {
                let mut changed = false;
                for line in lines_in {
                    let (start, end) = subsection(lines, raw_id, name);
                    if add_numbered(lines, start, end, line) {
                        changed = true;
                    }
                }
                if changed {
                    changed_names.push((*name).to_string());
                }
            }
            changed_names
        })? {
            done.extend(applied_names);
        }
    }
    if fields.get("H").and_then(|v| v.as_str()) == Some(DONE_STAGE) {
        // The user's Grist `완료` outranks the changes list: move a still-open item to `# 완료`.
        if complete_item(project_dir, raw_id)? {
            done.push("# 완료 이동".to_string());
        }
        let info = item_info(project_dir, raw_id);
        if let Some(info) = info {
            if info.kind == NEW_FEATURE && add_feature(project_dir, &info.title)? {
                done.push("📍 추가".to_string());
            }
        }
    }
    Ok(done)
}

pub fn on_task_write(raw_id: i64, fields: &Map<String, Value>) {
    if std::env::var_os("GRIST_NO_HOOK").map(|v| !v.is_empty()).unwrap_or(false) {
        return;
    }
    let has_relevant = CELL_SECTIONS
        .iter()
        .map(|(c, _)| *c)
        .chain(std::iter::once("H"))
        .any(|k| util::truthy(fields.get(k)));
    if !has_relevant {
        return;
    }
    let res: R<()> = (|| {
        if let Some(project_dir) = hook_project(raw_id) {
            apply_row(&project_dir, raw_id, fields)?;
        }
        Ok(())
    })();
    match res {
        Ok(()) => {}
        Err(Exit::Fail(_, _)) | Err(Exit::Gate(_)) => {
            eprintln!("{}", util::compact(&json!({"warning": "record hook", "raw_id": raw_id})));
        }
        Err(Exit::Other(error)) => {
            eprintln!(
                "{}",
                util::compact(&json!({"warning": "record hook", "raw_id": raw_id, "error": util::head(&error, 200)}))
            );
        }
    }
}

pub fn sync(project_arg: Option<&str>, cwd: &str) -> R<Value> {
    let (ids, project_dir) = if let Some(p) = project_arg {
        let dir = util::resolve(Path::new(p));
        let ids = changes_task_ids(&dir);
        (ids, dir)
    } else {
        let Some((ids, dir)) = find_task_agents(&util::resolve(Path::new(cwd))) else {
            return Ok(json!({"project_dir": null, "synced": []}));
        };
        (ids, dir)
    };
    let mut pending: BTreeSet<i64> = ids.into_iter().collect();
    for record in util::glob(&project_dir.join(".ai").join("changes"), "", ".md") {
        if let Ok(text) = std::fs::read_to_string(&record) {
            let lines = util::splitlines(&text);
            if let Some(start) = find(&lines, 1, "작업", 0, None) {
                let end = span(&lines, start, 1);
                for i in numbered(&lines, start, end) {
                    if let Some((_, id_part)) = lines[i].trim_end().strip_suffix(')').and_then(|s| s.rsplit_once("(#")) {
                        if let Ok(id) = id_part.parse::<i64>() {
                            pending.insert(id);
                        }
                    }
                }
            }
        }
    }
    let mut synced = Vec::new();
    if !pending.is_empty() {
        let sorted_ids: Vec<i64> = pending.into_iter().collect();
        let fields = vec!["H".into(), "D".into(), "I".into(), "J".into()];
        for row in api::get(TASK_TABLE, Some(&sorted_ids), None, Some(&fields))? {
            if let (Some(row_id), Some(row_fields)) = (
                row.get("id").and_then(|v| v.as_i64()),
                row.get("fields").and_then(|v| v.as_object()),
            ) {
                let done = apply_row(&project_dir, row_id, row_fields)?;
                if !done.is_empty() {
                    synced.push(json!({"id": row_id, "applied": done}));
                }
            }
        }
    }
    Ok(json!({"project_dir": util::path_str(&project_dir), "synced": synced}))
}

/// `# 요구 사항`·`# 세부 사항` input ⇒ `## <요구 사항>` heading and `- <세부 사항>` lines for the `# 요청` section.
pub fn request_block(text: &str) -> Option<Vec<String>> {
    let mut part = "";
    let (mut asks, mut details) = (Vec::new(), Vec::new());
    for line in text.lines() {
        let trimmed = util::strip(line);
        match trimmed.strip_prefix('#').map(util::strip) {
            Some("요구 사항") => { part = "ask"; continue; }
            Some("세부 사항") => { part = "detail"; continue; }
            Some(_) => { part = ""; continue; }
            None => {}
        }
        let Some(item) = trimmed.strip_prefix("- ").map(util::strip).filter(|t| !t.is_empty()) else { continue };
        match part {
            "ask" => asks.push(item.to_string()),
            "detail" => details.push(format!("- {item}")),
            _ => {}
        }
    }
    if asks.is_empty() || details.is_empty() {
        return None;
    }
    let mut block = vec![format!("## {}", asks.join(" · "))];
    block.extend(details);
    Some(block)
}

fn append_request(lines: &mut Vec<String>, block: &[String]) {
    let start = find(lines, 1, "요청", 0, None).expect("ensure_sections adds # 요청");
    let at = content_end(lines, start, level1_end(lines, start));
    let mut insert = Vec::new();
    if at > start + 1 {
        insert.push(String::new());
    }
    insert.extend(block.iter().cloned());
    for (offset, line) in insert.into_iter().enumerate() {
        lines.insert(at + offset, line);
    }
}

/// The last `## ` block of the `# 요청` section in `text`.
pub fn last_request_in(text: &str) -> Option<String> {
    let lines = util::splitlines(text);
    let start = find(&lines, 1, "요청", 0, None)?;
    let end = level1_end(&lines, start);
    let block_start = ((start + 1)..end).rev().find(|&i| matches!(heading(&lines[i]), Some((2, _))))?;
    let block_end = content_end(&lines, block_start, end);
    Some(lines[block_start..block_end].join("\n"))
}

/// The newest request across the project's changes records (J3 input).
pub fn latest_request_block(project_dir: &Path) -> String {
    util::glob(&project_dir.join(".ai").join("changes"), "", ".md")
        .into_iter()
        .rev()
        .find_map(|record| std::fs::read_to_string(record).ok().and_then(|text| last_request_in(&text)))
        .unwrap_or_default()
}

pub fn request_record(project_arg: &str, input_file: &str, title: &str, summary: &str) -> R<Value> {
    crate::gate::require_main("request-record")?;
    let project_dir = util::resolve(Path::new(project_arg));
    let text = std::fs::read_to_string(input_file).map_err(api::other)?;
    let req_re = Regex::new(r"(?m)^#\s*요구 사항\s*$").unwrap();
    let det_re = Regex::new(r"(?m)^#\s*세부 사항\s*$").unwrap();
    let block = match request_block(&text) {
        Some(block) if req_re.is_match(&text) && det_re.is_match(&text) => block,
        _ => {
            return api::fail(
                "validation",
                "the request needs `# 요구 사항` and `# 세부 사항` headings, each with `- ` lines",
            )
        }
    };
    let record_name = with_lock(&project_dir.join(".ai").join("changes"), || {
        let record = changes_record(&project_dir, Some(title))?;
        let before = std::fs::read_to_string(&record).map_err(api::other)?;
        let mut lines = util::splitlines(&before);
        ensure_sections(&mut lines);
        append_request(&mut lines, &block);
        write_atomic(&record, &format!("{}\n", lines.join("\n")))?;
        Ok(record.file_name().unwrap().to_string_lossy().into_owned())
    })?;
    let obsidian = add_request(&project_dir, util::strip(title), util::strip(summary))?;
    Ok(json!({
        "request": block[0].trim_start_matches("## "),
        "record": record_name,
        "obsidian": obsidian
    }))
}

fn task_add_fields(mut fields: Map<String, Value>) -> Map<String, Value> {
    fields.entry("E".to_string()).or_insert_with(|| Value::from(util::today_epoch()));
    fields
}

fn session_rows_path(thread: &str) -> PathBuf {
    state_dir().join("session_rows").join(format!("{}.json", util::sanitize_key(thread)))
}

fn state_dir() -> PathBuf {
    std::env::var_os("TASK_COMPLETE_STATE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".codex/tmp/task_complete"))
}

fn claude_session_id_at(state: &Path, project: &Path) -> Option<String> {
    let project = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let path = state.join("claude_project_session").join(format!("{}.json", util::sanitize_key(&project.to_string_lossy())));
    let pointer: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    (pointer.get("cwd")?.as_str()? == project.to_string_lossy() && !pointer.get("session_id")?.as_str()?.is_empty())
        .then(|| pointer.get("session_id").unwrap().as_str().unwrap().to_string())
}

fn register_session_row_at(path: &Path, raw_id: i64) -> R<()> {
    std::fs::create_dir_all(path.parent().unwrap()).map_err(api::other)?;
    with_lock(&path, || {
        let mut rows: Vec<i64> = std::fs::read_to_string(&path).ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        if !rows.contains(&raw_id) { rows.push(raw_id); }
        write_atomic(&path, &serde_json::to_string(&rows).map_err(api::other)?)
    })
}

pub fn register_session_row(raw_id: i64) -> R<()> {
    let Ok(thread) = std::env::var("CODEX_THREAD_ID") else { return Ok(()); };
    if thread.is_empty() { return Ok(()); }
    register_session_row_at(&session_rows_path(&thread), raw_id)
}

fn register_claude_session_row_at(state: &Path, project: &Path, raw_id: i64, codex_thread: Option<&str>) -> R<()> {
    if codex_thread.is_some_and(|id| !id.is_empty()) { return Ok(()); }
    if let Some(session_id) = claude_session_id_at(state, project) {
        register_session_row_at(&state.join("session_rows").join(format!("{}.json", util::sanitize_key(&session_id))), raw_id)?;
    }
    Ok(())
}

fn register_claude_session_row(project: &Path, raw_id: i64) -> R<()> {
    let codex_thread = std::env::var("CODEX_THREAD_ID").ok();
    register_claude_session_row_at(&state_dir(), project, raw_id, codex_thread.as_deref())
}

pub fn task_add(mut fields: Map<String, Value>, project_arg: Option<&str>, kind: Option<&str>, judge: &str) -> R<Value> {
    crate::gate::require_main("task-add")?;
    if project_arg.is_some() && kind.is_none() {
        return api::fail("arguments", "task-add --project needs --type 기능 추가|개선|오류 해결");
    }
    if let Some(k) = kind {
        let Some(code) = change_type_code(k) else {
            return api::fail("arguments", "task-add --project needs --type 기능 추가|개선|오류 해결");
        };
        if !fields.contains_key("G") {
            fields.insert("G".to_string(), Value::String(code.to_string()));
        }
    }
    let mut ctx = crate::task::TaskCtx {
        project_dir: project_arg.map(|p| util::resolve(Path::new(p))),
        fields: task_add_fields(fields),
        kind: kind.unwrap_or("").to_string(),
        judge: judge.to_string(),
        ..Default::default()
    };
    let step = crate::task::Step::Add;
    crate::task::TaskChain::standard(step).run(step, &mut ctx)?;
    if let Some(project) = ctx.project_dir.as_deref() {
        register_claude_session_row(project, ctx.raw_id)?;
    }
    Ok(ctx.result)
}

pub fn task_show(project_arg: &str, raw_id: i64, section_opt: Option<&str>) -> R<Value> {
    let (project_dir, _, _) = owned(project_arg, raw_id)?;
    let Some(record) = item_file(&project_dir, raw_id) else {
        return api::fail("records", format!("Table26 #{raw_id} block not found in .ai/changes"));
    };
    let text = std::fs::read_to_string(&record).map_err(api::other)?;
    let lines = util::splitlines(&text);
    let Some((start, end)) = item_span(&lines, raw_id) else {
        return api::fail("records", format!("Table26 #{raw_id} span not found in .ai/changes"));
    };
    let extract_sub = |name: &str| -> Vec<String> {
        let Some(sub_start) = find(&lines, 3, name, start, Some(end)) else {
            return Vec::new();
        };
        let sub_end = span(&lines, sub_start, 3);
        let mut out = Vec::new();
        for line in &lines[(sub_start + 1)..sub_end] {
            if line.starts_with("#### ") {
                break;
            }
            let trimmed = util::strip(line);
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        out
    };
    let goal_lines = || -> Vec<String> {
        goal_bodies(&project_dir, raw_id)
            .map(|b| b.iter().enumerate().map(|(n, t)| format!("{}. {t}", n + 1)).collect())
            .unwrap_or_default()
    };
    if let Some(sec) = section_opt.filter(|s| !s.trim().is_empty()) {
        let lines = match sec.trim() {
            "목표" => goal_lines(),
            stage @ ("초안" | "검증" | "확인" | "완료") => latest_stage_block(&lines[start..end], stage, raw_id),
            name => extract_sub(name),
        };
        return Ok(json!({
            "id": raw_id,
            "section": sec,
            "lines": lines
        }));
    }
    Ok(json!({
        "id": raw_id,
        "goal": goal_lines(),
        "steps": extract_sub("구현")
    }))
}

/// Lines of the latest `#### <stage> (#id)` record inside a row's block, up to the next heading.
fn latest_stage_block(block: &[String], stage: &str, raw_id: i64) -> Vec<String> {
    let heading_line = format!("#### {stage} (#{raw_id})");
    let Some(at) = block.iter().rposition(|line| util::strip(line) == heading_line) else {
        return Vec::new();
    };
    block[at + 1..]
        .iter()
        .take_while(|line| !line.starts_with('#'))
        .map(|line| line.trim_end().to_string())
        .filter(|line| !util::strip(line).is_empty())
        .collect()
}

/// Replaces the matching step and goal, keeping goal steps ahead of confirmation steps.
fn upsert_step(lines: &mut Vec<String>, raw_id: i64, step: &str, clean: &str, goal: Option<usize>) -> String {
    let prefix = format!("- [{step}]");
    let key = goal.map_or_else(|| prefix.clone(), |number| format!("{prefix} {number}."));
    let new_line = goal.map_or_else(|| format!("{prefix} {clean}"), |number| format!("{prefix} {number}. {clean}"));
    let (sub_start, sub_end) = subsection(lines, raw_id, "구현");
    let mut h4_at = sub_end;
    for idx in (sub_start + 1)..sub_end {
        if lines[idx].starts_with("#### ") {
            h4_at = idx;
            break;
        }
    }
    for idx in (sub_start + 1)..h4_at {
        if lines[idx].trim_start().starts_with(&format!("{key} ")) {
            lines[idx] = new_line.clone();
            return new_line;
        }
    }
    let mut insert_at = sub_start + 1;
    for idx in (sub_start + 1)..h4_at {
        let line = lines[idx].trim_start();
        if (goal.is_none() && line.starts_with("- ["))
            || (goal.is_some() && (line.starts_with("- [▷]") || line.starts_with("- [▶]"))) {
            insert_at = idx + 1;
        }
    }
    lines.insert(insert_at, new_line.clone());
    new_line
}

pub fn task_step(project_arg: &str, raw_id: i64, step: &str, text: &str, goal: usize) -> R<Value> {
    if matches!(step, "○" | "●") {
        return api::fail("validation", "○ and ● are recorded only by `task-confirm`; task-step takes ▷ or ▶");
    }
    if !matches!(step, "▷" | "▶") {
        return api::fail("validation", "step must be one of ▷, ▶");
    }
    let clean = util::strip(text).replace('\n', " ");
    if clean.is_empty() {
        return api::fail("validation", "step text must not be empty");
    }
    let (project_dir, _, _) = owned(project_arg, raw_id)?;
    let bodies = goal_bodies(&project_dir, raw_id).unwrap_or_default();
    if let Err(e) = check_goal_number(goal, bodies.len()) {
        return api::fail("validation", format!("#{raw_id} {e}"));
    }
    let updated = edit_item(&project_dir, raw_id, |lines| upsert_step(lines, raw_id, step, &clean, Some(goal)))?;
    let Some(new_line) = updated else {
        return api::fail("records", format!("Table26 #{raw_id} not found in .ai/changes"));
    };
    let mut state = crate::gate::gate_load(&project_dir, raw_id);
    state.insert("goal_n".into(), json!(goal));
    let mut stepped: BTreeSet<u64> = state
        .get("goals_stepped")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_u64()).collect())
        .unwrap_or_default();
    stepped.insert(goal as u64);
    state.insert("goals_stepped".into(), json!(stepped.into_iter().collect::<Vec<_>>()));
    crate::gate::gate_save(&project_dir, raw_id, &state)?;
    let k = goal_k_line(&bodies[goal - 1]);
    let mut fields = Map::new();
    fields.insert(api::GOAL_FIELD.into(), Value::String(k.clone()));
    api::update(TASK_TABLE, raw_id, &fields)?;
    Ok(json!({"id": raw_id, "step": step, "line": new_line, "K": k}))
}

/// Per-goal evidence: non-empty lines `N. <근거>` numbered 1..=expected in order.
pub fn parse_evidence(text: &str, expected: usize) -> Result<Vec<String>, String> {
    let need = || format!("evidence needs {expected} lines `N. <근거>` (N = 1..{expected}), one per goal line");
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some((number, body)) = line.split_once(". ") else { return Err(need()) };
        let body = body.trim();
        if number.parse::<usize>().ok() != Some(out.len() + 1) || body.is_empty() { return Err(need()); }
        out.push(body.replace('\n', " "));
    }
    if expected == 0 || out.len() != expected { return Err(need()); }
    Ok(out)
}

/// Records `- [○] <check>`, `- [●] 사용자 검토 대기` and the `#### 확인` block in one pass; idempotent for the step lines.
/// The evidence is validated against the goal line count before anything is written.
fn confirm_in_project(project_dir: &Path, raw_id: i64, summary: &str, check: &str, detail: &str, evidence: &str) -> R<usize> {
    let Some(goal) = changes_goal(project_dir, raw_id) else {
        return api::fail("validation", format!("goal missing (#{raw_id}); evidence needs one line per goal"));
    };
    let evidence = match parse_evidence(evidence, crate::task::match_goal_count(&goal)) {
        Ok(lines) => lines,
        Err(e) => return api::fail("validation", format!("#{raw_id} {e}")),
    };
    let steps = edit_item(project_dir, raw_id, |lines| {
        upsert_step(lines, raw_id, "○", check, None);
        upsert_step(lines, raw_id, "●", "사용자 검토 대기", None);
    })?;
    if steps.is_none() {
        return api::fail("records", format!("Table26 #{raw_id} not found in .ai/changes"));
    }
    let block = confirm_block(raw_id, summary, check, detail, &evidence);
    append_item(project_dir, raw_id, &block)?;
    Ok(evidence.len())
}

fn confirm_block(raw_id: i64, summary: &str, check: &str, detail: &str, evidence: &[String]) -> String {
    let lines: String = evidence.iter().enumerate().map(|(n, e)| format!("- 근거 {} : {e}\n", n + 1)).collect();
    format!("\n#### 확인 (#{raw_id})\n- 요약 : {summary}\n- [x] {check}\n  결과 : 성공\n  상세 : {detail}\n{lines}")
}

pub fn task_confirm(project_arg: &str, raw_id: i64, summary: &str, check: &str, detail: &str, evidence: &str) -> R<Value> {
    let summary_clean = util::strip(summary).replace('\n', " ");
    let check_clean = util::strip(check).replace('\n', " ");
    let detail_clean = util::strip(detail).replace('\n', " ");
    if summary_clean.is_empty() || check_clean.is_empty() || detail_clean.is_empty() {
        return api::fail("validation", "summary, check, and detail must not be empty");
    }
    let (project_dir, _, _) = owned(project_arg, raw_id)?;
    let count = confirm_in_project(&project_dir, raw_id, &summary_clean, &check_clean, &detail_clean, evidence)?;
    Ok(json!({"id": raw_id, "summary": summary_clean, "check": check_clean, "evidence": count}))
}

#[cfg(test)]
mod tests {
    use super::{format_judge, register_item, register_session_row_at, task_add_fields};

    #[test]
    fn goal_steps_keep_each_number_and_precede_confirmation_steps() {
        let mut lines: Vec<String> = "## [구현] 기록 (#327)\n### 구현\n- [○] 확인\n- [●] 사용자 검토 대기\n#### 확인 (#327)"
            .lines().map(String::from).collect();
        super::upsert_step(&mut lines, 327, "▶", "첫 내역", Some(1));
        super::upsert_step(&mut lines, 327, "▶", "둘째 내역", Some(2));
        assert_eq!(&lines[2..6], ["- [▶] 1. 첫 내역", "- [▶] 2. 둘째 내역", "- [○] 확인", "- [●] 사용자 검토 대기"]);

        super::upsert_step(&mut lines, 327, "▶", "첫 내역 수정", Some(1));
        assert_eq!(&lines[2..6], ["- [▶] 1. 첫 내역 수정", "- [▶] 2. 둘째 내역", "- [○] 확인", "- [●] 사용자 검토 대기"]);
        super::upsert_step(&mut lines, 327, "▷", "초안", Some(2));
        assert_eq!(&lines[2..7], ["- [▶] 1. 첫 내역 수정", "- [▶] 2. 둘째 내역", "- [▷] 2. 초안", "- [○] 확인", "- [●] 사용자 검토 대기"]);
        super::upsert_step(&mut lines, 327, "○", "다시 확인", None);
        assert_eq!(lines[5], "- [○] 다시 확인");
    }

    #[test]
    fn completion_actions_omit_goal_number_prefix() {
        let lines = ["- [▶] 1. 첫 내역", "- [▶] 2. 둘째 내역", "- [○] 확인"]
            .map(String::from);
        assert_eq!(super::completion_action_lines(&lines), [
            "1. [🏗️] 첫 내역", "2. [🏗️] 둘째 내역", "3. [🏗️] 확인"
        ]);
    }

    #[test]
    fn missing_goal_steps_lists_unrecorded_numbers() {
        let state = |v: serde_json::Value| v.as_object().cloned().unwrap();
        assert_eq!(super::missing_goal_steps(&state(serde_json::json!({"goals_stepped": [1, 2, 3]})), 3), Vec::<usize>::new());
        assert_eq!(super::missing_goal_steps(&state(serde_json::json!({"goal_n": 1, "goals_stepped": [1]})), 3), vec![2, 3]);
        assert_eq!(super::missing_goal_steps(&state(serde_json::json!({})), 2), vec![1, 2]);
    }

    #[test]
    fn diagnosis_update_needs_i_and_d() {
        let f = |v: serde_json::Value| v.as_object().cloned().unwrap();
        assert!(super::diagnosis_fields_ok(&f(serde_json::json!({"H": "진단", "I": "오류", "D": "🔍 점검"}))));
        assert!(!super::diagnosis_fields_ok(&f(serde_json::json!({"H": "진단", "D": "🔍 점검"}))));
        assert!(!super::diagnosis_fields_ok(&f(serde_json::json!({"H": "진단", "I": "오류"}))));
        assert!(!super::diagnosis_fields_ok(&f(serde_json::json!({"H": "진단", "I": " ", "D": "🔍 점검"}))));
        assert!(super::diagnosis_fields_ok(&f(serde_json::json!({"H": "작업"}))));
    }

    #[test]
    fn conjunction_warnings_flag_two_action_lines() {
        let w = super::goal_conjunction_warnings("1. [수정] 값을 바꾸고 저장한다\n2. [기능] 목록 및 상세를 보인다\n3. [수정] 남기고, 닫는다");
        assert_eq!(w.len(), 3);
        assert_eq!(w[0], "goal 1: [수정] 값을 바꾸고 저장한다 — 두 결과를 한 줄에 묶었는지 확인(inquire-task)");
        assert!(w[1].starts_with("goal 2: ") && w[2].starts_with("goal 3: "));
        assert!(super::goal_conjunction_warnings("1. [수정] 경고 문구를 보인다\n2. [기능] 참고 링크를 출력한다").is_empty());
    }

    #[test]
    fn stage_section_returns_only_the_latest_record_block() {
        let block: Vec<String> = "## [구현] [✒️] a (#7)\n#### 확인 (#7)\n- 요약 : 옛\n#### 검증 (#7)\n- 결과 : 성공\n#### 확인 (#7)\n- 요약 : 새\n- [x] 검사\n  결과 : 성공\n\n## [구현] [✒️] b (#8)\n".lines().map(String::from).collect();
        assert_eq!(super::latest_stage_block(&block, "확인", 7), vec!["- 요약 : 새", "- [x] 검사", "  결과 : 성공"]);
        assert_eq!(super::latest_stage_block(&block, "검증", 7), vec!["- 결과 : 성공"]);
        assert!(super::latest_stage_block(&block, "초안", 7).is_empty());
    }

    #[test]
    fn request_input_becomes_a_block_at_the_end_of_the_request_section() {
        let block = super::request_block("# 요구 사항\n- 첫 요청\n\n# 세부 사항\n- [✒️] 문서\n- [📐] 함수\n").unwrap();
        assert_eq!(block, vec!["## 첫 요청", "- [✒️] 문서", "- [📐] 함수"]);
        assert!(super::request_block("# 요구 사항\n- a\n# 세부 사항\n").is_none());

        let mut lines: Vec<String> = "# 작업\n\n# 완료\n\n# 진행\n".lines().map(String::from).collect();
        super::ensure_sections(&mut lines);
        assert_eq!(lines[0], "# 요청");
        super::append_request(&mut lines, &block);
        super::append_request(&mut lines, &["## 둘째".to_string(), "- [🚿] 필터".to_string()]);
        let text = lines.join("\n");
        assert!(text.starts_with("# 요청\n## 첫 요청\n- [✒️] 문서\n- [📐] 함수\n\n## 둘째\n- [🚿] 필터\n\n# 작업"));
        assert_eq!(super::last_request_in(&text).unwrap(), "## 둘째\n- [🚿] 필터");
        assert!(super::last_request_in("# 작업\n").is_none());
    }
    use serde_json::{Map, Value};

    #[test]
    fn task_add_sets_today_in_date_column() {
        let fields = task_add_fields(Map::new());
        println!("task-add E={:?}, expected={}", fields.get("E"), crate::util::today_epoch());
        assert_eq!(fields.get("E"), Some(&Value::from(crate::util::today_epoch())));
    }

    #[test]
    fn task_add_keeps_explicit_date_column() {
        let mut fields = Map::new();
        fields.insert("E".to_string(), Value::from(1_760_000_000));
        let fields = task_add_fields(fields);
        assert_eq!(fields.get("E"), Some(&Value::from(1_760_000_000)));
    }

    #[test]
    fn session_row_registration_preserves_created_ids_without_duplicates() {
        let dir = std::env::temp_dir().join(format!("grist-session-row-{}", std::process::id()));
        let path = dir.join("rows.json");
        register_session_row_at(&path, 277).unwrap();
        register_session_row_at(&path, 277).unwrap();
        register_session_row_at(&path, 278).unwrap();
        let rows: Vec<i64> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(rows, vec![277, 278]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn claude_project_pointer_resolves_session_for_task_add() {
        let dir = std::env::temp_dir().join(format!("grist-claude-session-{}", std::process::id()));
        let project = dir.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let pointer = dir.join("claude_project_session").join(format!("{}.json", crate::util::sanitize_key(&project.to_string_lossy())));
        std::fs::create_dir_all(pointer.parent().unwrap()).unwrap();
        std::fs::write(&pointer, serde_json::json!({"cwd": project, "session_id": "claude-123"}).to_string()).unwrap();
        assert_eq!(super::claude_session_id_at(&dir, &project).as_deref(), Some("claude-123"));
        let row_path = dir.join("session_rows/claude-123.json");
        super::register_claude_session_row_at(&dir, &project, 410, None).unwrap();
        let rows: Vec<i64> = serde_json::from_str(&std::fs::read_to_string(row_path).unwrap()).unwrap();
        assert_eq!(rows, vec![410]);
        super::register_claude_session_row_at(&dir, &project, 411, Some("codex-1")).unwrap();
        let rows: Vec<i64> = serde_json::from_str(&std::fs::read_to_string(dir.join("session_rows/claude-123.json")).unwrap()).unwrap();
        assert_eq!(rows, vec![410]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn format_judge_cases() {
        assert_eq!(format_judge("랜더링 성능 개선", ""), "[상태] 랜더링 성능 개선");
        assert_eq!(
            format_judge("기타 작업", "[상태] 랜더링 성능 개선"),
            "[상태] 랜더링 성능 개선"
        );
        assert_eq!(
            format_judge("기타 작업", "[값] 반환 개수 일치"),
            "[값] 반환 개수 일치"
        );
        assert_eq!(
            format_judge("기타 작업", "태그 없는 판정 요약"),
            "[상태] 태그 없는 판정 요약"
        );
    }

    #[test]
    fn register_item_writes_judge_format() {
        let tmp = std::env::temp_dir().join(format!("grist-unit-reg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let rec = register_item(&tmp, 42, "랜더링 성능 개선", "개선", "[상태] 랜더링 성능 개선").unwrap();
        let text = std::fs::read_to_string(&rec).unwrap();
        assert!(text.contains("## [구현] 랜더링 성능 개선 (#42)\n- 유형 : 개선\n- 판정 : [상태] 랜더링 성능 개선\n### 목표\n"));

        let rec = register_item(&tmp, 43, "[✒️] 문서 형식 전환", "개선", "[상태] 형식 전환").unwrap();
        let text = std::fs::read_to_string(&rec).unwrap();
        assert!(text.contains("## [구현] [✒️] 문서 형식 전환 (#43)\n- 유형 : 개선\n- 판정 : [상태] 형식 전환\n### 목표\n"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn append_item_writes_into_own_row_block_between_rows() {
        let project = std::env::temp_dir().join(format!("grist-append-item-{}", std::process::id()));
        let changes = project.join(".ai/changes");
        std::fs::create_dir_all(&changes).unwrap();
        let record = changes.join("260101.작업.md");
        std::fs::write(&record, "# 작업\n1. [✒️] a (#1)\n2. [📐] b (#2)\n# 완료\n# 진행\n## [구현] [✒️] a (#1)\n### 구현\n\n## [구현] [📐] b (#2)\n### 구현\n#### 초안 (#2)\n").unwrap();
        super::append_item(&project, 1, "\n#### 확인 (#1)\n- 요약 : x ⇒ y\n").unwrap();
        let text = std::fs::read_to_string(&record).unwrap();
        let first = text.find("#### 확인 (#1)").unwrap();
        assert!(first < text.find("## [구현] [📐] b (#2)").unwrap());
        assert!(text.ends_with("#### 초안 (#2)\n"));
        assert!(super::append_item(&project, 9, "x").is_err());
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn user_done_sync_moves_active_item_to_completed_list() {
        let project = std::env::temp_dir().join(format!("grist-user-done-{}", std::process::id()));
        let changes = project.join(".ai/changes");
        std::fs::create_dir_all(&changes).unwrap();
        let record = changes.join("260101.작업.md");
        std::fs::write(&record, "# 작업\n1. [✒️] 문서 절차 (#7)\n# 완료\n# 진행\n## [구현] [✒️] 문서 절차 (#7)\n- 유형 : 개선\n### 구현\n").unwrap();
        let fields = serde_json::json!({"H": "완료"}).as_object().unwrap().clone();
        assert_eq!(super::apply_row(&project, 7, &fields).unwrap(), vec!["# 완료 이동"]);
        assert!(super::apply_row(&project, 7, &fields).unwrap().is_empty());
        let text = std::fs::read_to_string(&record).unwrap();
        assert!(text.contains("# 작업\n# 완료\n1. [✒️] 문서 절차 (#7)"));
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn changes_task_ids_uses_previous_completed_list_when_today_is_empty() {
        let project = std::env::temp_dir().join(format!("grist-rollover-{}", std::process::id()));
        let changes = project.join(".ai/changes");
        std::fs::create_dir_all(&changes).unwrap();
        std::fs::write(changes.join("261021.작업.md"), "# 작업\n# 완료\n1. [✒️] 검증 대기 (#276)\n# 진행\n").unwrap();
        std::fs::write(changes.join("261022.작업.md"), "# 작업\n# 완료\n# 진행\n").unwrap();
        assert_eq!(super::changes_task_ids(&project), vec![276]);
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn changes_task_ids_merges_active_and_completed() {
        let project = std::env::temp_dir().join(format!("grist-mixed-{}", std::process::id()));
        let changes = project.join(".ai/changes");
        std::fs::create_dir_all(&changes).unwrap();
        let record = changes.join("260101.작업.md");
        std::fs::write(&record, "# 작업\n1. [✒️] 진행 중 (#7)\n# 완료\n1. [✒️] 완료됨 (#8)\n2. [✒️] 중복 (#7)\n# 진행\n").unwrap();
        assert_eq!(super::changes_task_ids(&project), vec![7, 8]);
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn completion_note_uses_successful_confirmation_without_implementation_step() {
        let project = std::env::temp_dir().join(format!("grist-confirm-note-{}", std::process::id()));
        let changes = project.join(".ai/changes");
        std::fs::create_dir_all(&changes).unwrap();
        let record = changes.join("260101.작업.md");
        let text = "# 작업\n1. [✒️] 문서 절차 (#275)\n# 완료\n# 진행\n## [구현] [✒️] 문서 절차 (#275)\n### 구현\n#### 확인 (#275)\n- 요약 : [⭕️] 재탐색 ⇒ 기존 행에서 완료\n- [x] 링크와 충돌 검사\n  결과 : 성공\n  상세 : 충돌 0건\n";
        std::fs::write(&record, text).unwrap();
        assert!(super::ensure_completion_note(&project, 275).unwrap());
        assert!(!super::ensure_completion_note(&project, 275).unwrap());
        let updated = std::fs::read_to_string(&record).unwrap();
        assert!(updated.contains("#### 완료 (#275)\n1. [⭕️] 재탐색 ⇒ 기존 행에서 완료"));
        std::fs::write(&record, text.replace("결과 : 성공", "결과 : 실패")).unwrap();
        assert!(!super::ensure_completion_note(&project, 275).unwrap());
        assert!(!std::fs::read_to_string(&record).unwrap().contains("#### 완료 (#275)"));
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn completion_note_and_list_move_are_idempotent() {
        let project = std::env::temp_dir().join(format!("grist-complete-{}", std::process::id()));
        let changes = project.join(".ai/changes");
        std::fs::create_dir_all(&changes).unwrap();
        let record = changes.join("260101.작업.md");
        std::fs::write(&record, "# 작업\n1. [✒️] 작업 (#7)\n# 완료\n# 진행\n## [구현] [✒️] 작업 (#7)\n### 구현\n#### 검증 (#7)\n- [▶] Rust 실행 파일 연결\n#### 확인 (#7)\n- [○] 종료 입력 검증\n").unwrap();
        assert_eq!(super::ensure_completion_note(&project, 7).unwrap(), true);
        assert_eq!(super::ensure_completion_note(&project, 7).unwrap(), false);
        assert_eq!(super::complete_item(&project, 7).unwrap(), true);
        assert_eq!(super::complete_item(&project, 7).unwrap(), false);
        let text = std::fs::read_to_string(&record).unwrap();
        assert_eq!(text.matches("#### 완료 (#7)").count(), 1);
        assert!(text.contains("1. [🏗️] Rust 실행 파일 연결"));
        assert!(text.contains("2. [🏗️] 종료 입력 검증"));
        assert!(text.contains("# 완료\n1. [✒️] 작업 (#7)"));
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn judge_docs_and_manage_project_split_cases() {
        let agents = std::fs::read_to_string("/home/tree/extensions/AGENTS.md").unwrap();
        assert!(agents.contains("`manage-project` Skill 3절이 단일 원본"));
        assert!(!agents.contains("판정 : [값|상태]"));

        let skill_dir = crate::util::home().join("ai/skills/manage-project");
        let skill_md = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(skill_md.contains("판정 : [값|상태]"));
        assert!(skill_md.contains("references/start.md"));
        assert!(skill_md.contains("references/work.md"));
        assert!(skill_md.contains("references/verify.md"));
        assert!(skill_md.lines().count() <= 115);

        for rel in ["references/start.md", "references/work.md", "references/verify.md", "references/plan.md", "references/initialize.md"] {
            assert!(skill_dir.join(rel).is_file(), "missing {rel}");
        }
        let work_md = std::fs::read_to_string(skill_dir.join("references/work.md")).unwrap();
        assert!(work_md.contains("task-red"));
        assert!(work_md.contains("task-ready"));
        assert!(skill_dir.parent().unwrap().join("Complete_task/SKILL.md").is_file());
        assert!(skill_md.contains("red test용 실패 초안 구현"));
    }

    #[test]
    fn status_header_and_template_paths_use_current_format() {
        let new_agents = "# 상태\n### grist\nID : 1198\n### obsidian\n문서 : 구현/업무/261008.작업관리기/일지/작업관리기-개발.md\n### 실행\nruntime : 로컬 CLI\n";
        assert_eq!(super::has_table7_id(new_agents), Some(1198));
        assert_eq!(
            super::has_obsidian_doc(new_agents).as_deref(),
            Some("구현/업무/261008.작업관리기/일지/작업관리기-개발.md")
        );
        let root_agents = std::fs::read_to_string("/home/tree/extensions/AGENTS.md").unwrap();
        assert!(root_agents.contains("# 상태"));
        assert!(root_agents.contains("### grist"));
        assert!(root_agents.contains("ID : 1198"));
        assert!(root_agents.contains("### obsidian"));
        assert!(root_agents.contains("문서 : 구현/업무/261008.작업관리기/일지/작업관리기-개발.md"));
        assert!(root_agents.contains("구현/업무/261008.작업관리기/형식/"));
        let init_skill = std::fs::read_to_string(
            crate::util::home().join("ai/skills/manage-project/references/initialize.md"),
        )
        .unwrap();
        assert!(init_skill.contains("--project-type"));
        assert!(init_skill.contains("업무"));
        assert!(init_skill.contains("공방"));
        assert!(init_skill.contains("게임"));
        assert!(init_skill.contains("보조"));
        assert!(init_skill.contains("일지/"));
        assert!(init_skill.contains("노트/"));
        assert!(init_skill.contains("구현/업무/261008.작업관리기/형식/작업관리기.형식-"));
        println!("status_header_and_template_paths_use_current_format OK: # 상태 ID=1198, 문서=구현/업무/261008.작업관리기/일지/작업관리기-개발.md, 형식=구현/업무/261008.작업관리기/형식/");
    }

    #[test]
    fn task_show_extracts_only_requested_section_without_full_block() {
        let global = std::fs::read_to_string(crate::util::home().join("ai/rules/Agents.md")).unwrap();
        assert!(global.contains("grist task-show --section"));
        assert!(!global.contains("약 300줄이 넘으면"));
        let start_md = std::fs::read_to_string(crate::util::home().join("ai/skills/manage-project/references/start.md")).unwrap();
        assert!(!start_md.contains("단일 소규모 행은 메인 세션이 직접"));
        assert!(!start_md.contains("끝까지 읽고"));
        println!("task_show_extracts_only_requested_section_without_full_block ▷ 필요한 줄·필드만 발췌해 읽고 출력하며 구형 경로·전체 덤프·불필요한 해시 검증 0건");
    }

    const GOAL_ITEM: &str = "# 작업\n1. [✒️] 작업 (#7)\n# 완료\n# 진행\n## [구현] [✒️] 작업 (#7)\n- 유형 : 기능 추가\n### 목표\n1. [기능] 첫째\n2. [수정] 둘째\n\n### 메모\n- 메모 줄\n### 구현\n- [▶] 구현 줄\n#### 검증 (#7)\n- 검증\n";

    fn temp_project(name: &str, text: &str) -> std::path::PathBuf {
        let project = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        std::fs::create_dir_all(project.join(".ai/changes")).unwrap();
        std::fs::write(project.join(".ai/changes/260101.작업.md"), text).unwrap();
        project
    }

    #[test]
    fn goal_prints_only_goal_lines() {
        let project = temp_project("grist-goal-lines", GOAL_ITEM);
        let bodies = super::goal_bodies(&project, 7).unwrap();
        let text = bodies.iter().enumerate().map(|(n, t)| format!("{}. {t}", n + 1)).collect::<Vec<_>>().join("\n");
        assert_eq!(text, "1. [기능] 첫째\n2. [수정] 둘째");
        assert_eq!(super::changes_goal(&project, 7).unwrap(), "- [기능] 첫째\n- [수정] 둘째");
        assert!(super::goal_bodies(&project, 8).is_none());
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn task_memo_appends_only_agent_line_to_memo() {
        let project = temp_project("grist-task-memo", GOAL_ITEM);
        let line = "에이전트 1 : codex_338 · 4910ec9c ← 0 main";
        assert!(super::record_agent_memo(&project, 7, line).unwrap());
        assert!(!super::record_agent_memo(&project, 7, line).unwrap());
        assert!(super::record_agent_memo(&project, 7, "다른 메모").is_err());
        assert!(super::record_agent_memo(&project, 7, "에이전트 2 : a\nb").is_err());
        let file = super::item_file(&project, 7).unwrap();
        let content = std::fs::read_to_string(file).unwrap();
        let mut lines = crate::util::splitlines(&content);
        let (start, end) = super::subsection(&mut lines, 7, "메모");
        assert_eq!(lines[(start + 1)..end].iter().filter(|l| !l.is_empty()).cloned().collect::<Vec<_>>(),
            vec!["- 메모 줄", "1. 에이전트 1 : codex_338 · 4910ec9c ← 0 main"]);
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn goal_survives_complete_item() {
        let project = temp_project("grist-goal-complete", GOAL_ITEM);
        let before = super::changes_goal(&project, 7).unwrap();
        assert!(super::complete_item(&project, 7).unwrap());
        assert_eq!(super::changes_goal(&project, 7).unwrap(), before);
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn confirm_records_circle_steps_once_and_flow_evidence_passes() {
        let project = temp_project("grist-confirm", GOAL_ITEM);
        for _ in 0..2 {
            super::confirm_in_project(&project, 7, "입력 ⇒ 결과", "충돌 없음 확인", "충돌 검사 완료", "1. 첫째 값\n2. 둘째 값\n").unwrap();
        }
        let text = std::fs::read_to_string(project.join(".ai/changes/260101.작업.md")).unwrap();
        assert_eq!(text.matches("- [○] 충돌 없음 확인").count(), 1);
        assert_eq!(text.matches("- [●] 사용자 검토 대기").count(), 1);
        assert!(text.contains("  상세 : 충돌 검사 완료\n- 근거 1 : 첫째 값\n- 근거 2 : 둘째 값\n"));
        let evidence = crate::task::flow_confirmation_in_text(&text, 7).unwrap();
        assert!(evidence.is_some());
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn confirm_rejects_evidence_count_mismatch_before_writing() {
        let project = temp_project("grist-confirm-mismatch", GOAL_ITEM);
        for bad in ["1. 하나만", "1. a\n3. b", "1. a\n2. ", "", "1. a\n2. b\n3. c"] {
            let err = super::confirm_in_project(&project, 7, "입력 ⇒ 결과", "검사", "값", bad).unwrap_err();
            assert!(format!("{err:?}").contains("evidence needs 2 lines"), "{bad}");
        }
        let text = std::fs::read_to_string(project.join(".ai/changes/260101.작업.md")).unwrap();
        assert!(!text.contains("#### 확인 (#7)") && !text.contains("- [○]"));
        assert_eq!(super::parse_evidence("\n1. a\n\n2. b\n", 2).unwrap(), vec!["a", "b"]);
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn goal_k_line_and_number_range() {
        assert_eq!(super::goal_k_line("[수정] task-step --goal N ⇒ 줄이 남는다"), "task-step --goal N ⇒ 줄이 남는다");
        assert_eq!(super::goal_k_line("[기능] a"), "a");
        assert_eq!(super::goal_k_line("[값] a"), "[값] a");
        assert!(super::check_goal_number(1, 3).is_ok() && super::check_goal_number(3, 3).is_ok());
        assert!(super::check_goal_number(0, 3).is_err() && super::check_goal_number(4, 3).is_err());
        assert!(super::check_goal_number(1, 0).is_err());
    }

    #[test]
    fn grist_cells_keep_one_unnumbered_line() {
        let mut two = Map::new();
        two.insert("J".into(), Value::String("1. [🛣] a\n2. [🏷] b".into()));
        assert!(super::single_line_cells(&mut two).is_err());
        let mut one = Map::new();
        one.insert("I".into(), Value::String("2. 수정 뒤에도 빈 값".into()));
        one.insert("J".into(), Value::String("[🏷] 옛 빌드".into()));
        super::single_line_cells(&mut one).unwrap();
        assert_eq!(one.get("I"), Some(&Value::String("수정 뒤에도 빈 값".into())));
        assert_eq!(one.get("J"), Some(&Value::String("[🏷] 옛 빌드".into())));
    }

    #[test]
    fn symbol_lines_and_d_single_line() {
        for ok in ["[⭕️] a", "[⭕] a", "1. [🛣] b", "[🗝\u{FE0F}] c", "2. [🅰] d", "[🔳] e"] {
            assert!(super::valid_symbol_line(ok), "{ok}");
        }
        for bad in ["[유실] a", "1. [유실] a", "[❌]", "[❌] ", "[❌]x", "❌ a", "b"] {
            assert!(!super::valid_symbol_line(bad), "{bad}");
        }
        let f = |k: &str, v: &str| serde_json::json!({ k: v }).as_object().unwrap().clone();
        assert!(super::check_symbol_fields(&f("J", "1. [⭕️] a\n2. [🛣] b")).is_ok());
        assert!(super::check_symbol_fields(&f("J", "1. [⭕️] a\n2. b")).is_err());
        assert!(super::check_symbol_fields(&f("D", "[🚥] 점검")).is_ok());
        assert!(super::check_symbol_fields(&f("D", "[🚥] a\n[🚥] b")).is_err());
        assert!(super::check_symbol_fields(&f("D", "메모")).is_err());
        assert!(super::check_symbol_fields(&f("I", "자유")).is_ok());
    }

    #[test]
    fn task_step_rejects_circle_steps() {
        for step in ["○", "●"] {
            let err = super::task_step("/nonexistent", 7, step, "x", 1).unwrap_err();
            assert!(format!("{err:?}").contains("task-confirm"));
        }
    }
}
