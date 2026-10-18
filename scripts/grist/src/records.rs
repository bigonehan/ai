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

pub const RECORD_SECTIONS: [&str; 3] = ["작업", "완료", "진행"];
pub const NEW_FEATURE: &str = "기능 추가";
pub const CELL_SECTIONS: [(&str, &str); 3] = [("D", "메모"), ("I", "오류"), ("J", "원인")];
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

pub fn parse_ids(text: &str) -> Vec<i64> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?m)^\s*-\s*Table26 task Raw ID:\s*(\d+(?:\s*,\s*\d+)*)").unwrap());
    let Some(caps) = re.captures(text) else {
        return Vec::new();
    };
    caps[1]
        .split(',')
        .filter_map(|s| util::strip(s).parse::<i64>().ok())
        .collect()
}

pub fn has_table7_id(text: &str) -> Option<i64> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?m)^\s*-\s*Table7 Raw ID:\s*(\d+)").unwrap());
    re.captures(text).and_then(|c| c[1].parse::<i64>().ok())
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
                let ids = parse_ids(&text);
                if !ids.is_empty() {
                    return Some((ids, cur));
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
    let ids = parse_ids(&text);
    if ids.is_empty() {
        return api::fail(
            "validation",
            format!("{} has no `- Table26 task Raw ID:` line", util::path_str(&agents)),
        );
    }
    Ok((project_dir, text, ids))
}

pub fn owned(project_arg: &str, raw_id: i64) -> R<(PathBuf, String, Vec<i64>)> {
    let (project_dir, text, ids) = project(project_arg)?;
    if !ids.contains(&raw_id) {
        return api::fail(
            "validation",
            format!(
                "{} does not name Table26 task Raw ID {raw_id}",
                util::path_str(&project_dir.join("AGENTS.md"))
            ),
        );
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

pub fn append(project_dir: &Path, text: &str) -> R<()> {
    with_lock(&project_dir.join(".ai").join("changes"), || {
        let record = changes_record(project_dir, None)?;
        let mut f = OpenOptions::new().create(true).append(true).open(&record).map_err(api::other)?;
        std::io::Write::write_all(&mut f, text.as_bytes()).map_err(api::other)?;
        Ok(())
    })
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

pub fn ensure_sections(lines: &mut Vec<String>) {
    for name in RECORD_SECTIONS {
        if find(lines, 1, name, 0, None).is_none() {
            if lines.last().map(|l| !util::strip(l).is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            lines.push(format!("# {name}"));
            lines.push(String::new());
        }
    }
}

fn item_heading_parts(line: &str) -> Option<(String, i64)> {
    let rest = line.strip_prefix("## ")?.trim();
    let (head, tail) = rest.rsplit_once(char::is_whitespace)?;
    let id_str = tail
        .strip_prefix("(#")
        .and_then(|s| s.strip_suffix(')'))
        .or_else(|| tail.strip_prefix('#'))?;
    let raw_id = id_str.parse::<i64>().ok()?;
    let title = head
        .trim()
        .trim_start_matches("[구현]")
        .trim_start_matches("<구현>")
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
        let judge_str = if judge.is_empty() {
            "목표의 [연결]/[기능]/[수정] 줄"
        } else {
            judge
        };
        let heading_line = if title.starts_with('<') || title.starts_with('[') {
            format!("## {title} (#{raw_id})")
        } else {
            format!("## [구현] {title} (#{raw_id})")
        };
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

pub fn changes_goal(project_dir: &Path, raw_id: i64) -> Option<String> {
    let record = item_file(project_dir, raw_id)?;
    let text = std::fs::read_to_string(&record).ok()?;
    let lines = util::splitlines(&text);
    let (start, end) = item_span(&lines, raw_id)?;
    let index = find(&lines, 3, "목표", start, Some(end))?;
    let sub_end = span(&lines, index, 3);
    let out: Vec<String> = ((index + 1)..sub_end)
        .filter_map(|i| parse_numbered_line(&lines[i]).map(|(_, body)| format!("- {body}")))
        .collect();
    if out.is_empty() {
        None
    } else {
        Some(out.join("\n"))
    }
}

pub fn write_changes_goal(project_dir: &Path, raw_id: i64, goal: &str) -> R<()> {
    let goal_lines: Vec<String> = util::splitlines(goal)
        .into_iter()
        .filter(|l| !util::strip(l).is_empty())
        .map(|l| {
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
    let t7_id = std::fs::read_to_string(&agents).ok().and_then(|t| has_table7_id(&t));
    let Some(t7_id) = t7_id else {
        return api::fail(
            "obsidian",
            format!("{} has no `- Table7 Raw ID:` line", util::path_str(&agents)),
        );
    };
    let rows = api::get(PROJECT_TABLE, Some(&[t7_id]), None, Some(&["Obsidian_Id".into()]))?;
    let note = rows
        .first()
        .and_then(|r| r.get("fields"))
        .and_then(|f| f.get("Obsidian_Id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
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
                if parse_ids(&text).contains(&raw_id) {
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
        let agents = dir.join("AGENTS.md");
        if agents.is_file() {
            if let Ok(text) = std::fs::read_to_string(&agents) {
                if parse_ids(&text).contains(&raw_id) {
                    return Some(dir.clone());
                }
            }
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
        let info = item_info(project_dir, raw_id);
        if info.is_some() && complete_item(project_dir, raw_id)? {
            done.push("완료 이동".to_string());
        }
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
        let agents = dir.join("AGENTS.md");
        let ids = if agents.is_file() {
            std::fs::read_to_string(&agents)
                .map(|t| parse_ids(&t))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
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

pub fn input_text(project_dir: &Path) -> String {
    std::fs::read_to_string(project_dir.join("Input.md")).unwrap_or_default()
}

pub fn request_record(project_arg: &str, input_file: &str, title: &str, summary: &str) -> R<Value> {
    crate::gate::require_main("request-record")?;
    let project_dir = util::resolve(Path::new(project_arg));
    let text = std::fs::read_to_string(input_file).map_err(api::other)?;
    let req_re = Regex::new(r"(?m)^#\s*요구 사항\s*$").unwrap();
    let det_re = Regex::new(r"(?m)^#\s*세부 사항\s*$").unwrap();
    if !req_re.is_match(&text) || !det_re.is_match(&text) {
        return api::fail(
            "validation",
            "the new Input.md needs `# 요구 사항` and `# 세부 사항` headings",
        );
    }
    let input_path = project_dir.join("Input.md");
    let _ = std::fs::remove_file(&input_path);
    std::fs::write(&input_path, format!("{}\n", text.trim_end_matches('\n'))).map_err(api::other)?;
    let record_name = with_lock(&project_dir.join(".ai").join("changes"), || {
        let record = changes_record(&project_dir, Some(title))?;
        let before = std::fs::read_to_string(&record).map_err(api::other)?;
        let mut lines = util::splitlines(&before);
        let orig = lines.clone();
        ensure_sections(&mut lines);
        if lines != orig {
            write_atomic(&record, &format!("{}\n", lines.join("\n")))?;
        }
        Ok(record.file_name().unwrap().to_string_lossy().into_owned())
    })?;
    let obsidian = add_request(&project_dir, util::strip(title), util::strip(summary))?;
    Ok(json!({
        "input": util::path_str(&input_path),
        "record": record_name,
        "obsidian": obsidian
    }))
}

pub fn task_add(mut fields: Map<String, Value>, project_arg: Option<&str>, kind: Option<&str>, judge: &str) -> R<Value> {
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
    let mut result = api::add(TASK_TABLE, &fields)?;
    if let Some(p) = project_arg {
        let project_dir = util::resolve(Path::new(p));
        let raw_id = result.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let title = fields
            .get("B")
            .filter(|v| util::truthy(Some(v)))
            .map(util::val_str)
            .unwrap_or_else(|| format!("작업 {raw_id}"));
        let record = register_item(&project_dir, raw_id, &title, kind.unwrap_or(""), judge)?;
        if let Some(obj) = result.as_object_mut() {
            obj.insert(
                "record".to_string(),
                Value::String(record.file_name().unwrap().to_string_lossy().into_owned()),
            );
        }
        apply_row(&project_dir, raw_id, &fields)?;
    }
    Ok(result)
}
