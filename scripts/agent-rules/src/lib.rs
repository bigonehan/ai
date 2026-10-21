//! 전역 규칙 단일 원본 `~/ai/rules/Agents.md`를 읽고 고치고, Codex·Claude·Antigravity 활성본으로 동기화한다.
//!
//! 원본의 `<!-- @only codex,claude -->` … `<!-- @end -->` 블록은 나열한 도구의 활성본에만 들어간다.
//! 활성본에는 생성 헤더 한 줄이 붙고, 마지막으로 동기화한 내용을 `.state/<대상>.md`에 남겨
//! 활성본만 직접 바뀐 경우(drift)를 찾는다.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const HEADER: &str = "<!-- 생성본: ~/ai/rules/Agents.md에서 agent-rules가 만든다 -->\n\n";
pub const SIZE_LIMIT: usize = 8 * 1024;

/// 활성본 하나. `tool`은 `@only` 블록에서 쓰는 이름이다.
pub struct Target {
    pub name: &'static str,
    pub tool: &'static str,
    pub rel: &'static str,
}

pub const TARGETS: [Target; 4] = [
    Target { name: "codex", tool: "codex", rel: ".codex/AGENTS.md" },
    Target { name: "claude", tool: "claude", rel: ".claude/CLAUDE.md" },
    Target { name: "antigravity", tool: "antigravity", rel: ".gemini/GEMINI.md" },
    Target { name: "antigravity-cli", tool: "antigravity", rel: ".gemini/antigravity-cli/GEMINI.md" },
];

#[derive(Debug)]
pub struct Error {
    pub code: i32,
    pub msg: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn fail<T>(code: i32, msg: impl Into<String>) -> Result<T> {
    Err(Error { code, msg: msg.into() })
}

fn io<T>(what: &Path, r: std::io::Result<T>) -> Result<T> {
    r.or_else(|e| fail(1, format!("{}: {e}", what.display())))
}

pub struct Paths {
    pub home: PathBuf,
    pub source: PathBuf,
    pub state: PathBuf,
}

impl Paths {
    /// `AGENT_RULES_HOME`(테스트용)이 있으면 그 디렉터리를, 없으면 `HOME`을 기준으로 한다.
    pub fn from_env() -> Self {
        let home = std::env::var_os("AGENT_RULES_HOME")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::new(home)
    }

    pub fn new(home: PathBuf) -> Self {
        let rules = home.join("ai/rules");
        Paths { source: rules.join("Agents.md"), state: rules.join(".state"), home }
    }

    pub fn target(&self, t: &Target) -> PathBuf {
        self.home.join(t.rel)
    }
}

pub fn read_source(p: &Paths) -> Result<String> {
    io(&p.source, fs::read_to_string(&p.source))
}

pub fn write_source(p: &Paths, text: &str) -> Result<()> {
    if let Some(dir) = p.source.parent() {
        io(dir, fs::create_dir_all(dir))?;
    }
    io(&p.source, fs::write(&p.source, text))
}

// ---- 제목·구역 ----

/// (줄 번호 0부터, 수준, 제목). 코드 블록 안의 `#`은 제목이 아니다.
pub fn headings(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut fence = false;
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        let level = line.chars().take_while(|&c| c == '#').count();
        if level > 0 && line[level..].starts_with(' ') {
            out.push((i, level, line[level..].trim().to_string()));
        }
    }
    out
}

/// 제목과 정확히 같은 구역, 없으면 제목을 포함하는 유일한 구역의 줄 범위 [start, end).
fn section_range(text: &str, title: &str) -> Result<(usize, usize)> {
    let hs = headings(text);
    let exact: Vec<_> = hs.iter().filter(|h| h.2 == title).collect();
    let found = if exact.is_empty() { hs.iter().filter(|h| h.2.contains(title)).collect() } else { exact };
    let h = match found.as_slice() {
        [one] => *one,
        [] => return fail(2, format!("제목 `{title}`이 없다")),
        many => {
            let names: Vec<_> = many.iter().map(|h| h.2.as_str()).collect();
            return fail(2, format!("제목 `{title}`이 여러 개다: {}", names.join(", ")));
        }
    };
    let total = text.lines().count();
    let end = hs.iter().find(|o| o.0 > h.0 && o.1 <= h.1).map_or(total, |o| o.0);
    Ok((h.0, end))
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn join(lines: &[&str]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

/// 구역 본문. 다음 제목 앞의 빈 줄은 뺀다.
pub fn section(text: &str, title: &str) -> Result<String> {
    let (start, mut end) = section_range(text, title)?;
    let lines = lines_of(text);
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    Ok(join(&lines[start..end]))
}

/// 구역 끝(뒤따르는 빈 줄 앞)에 한 줄을 넣는다.
pub fn add_line(text: &str, title: &str, line: &str) -> Result<String> {
    let (start, end) = section_range(text, title)?;
    let mut lines = lines_of(text);
    let mut at = end;
    while at > start + 1 && lines[at - 1].trim().is_empty() {
        at -= 1;
    }
    lines.insert(at, line);
    Ok(join(&lines))
}

fn count_unique(text: &str, needle: &str) -> Result<()> {
    if needle.is_empty() {
        return fail(2, "찾을 문자열이 비었다");
    }
    match text.matches(needle).count() {
        1 => Ok(()),
        0 => fail(2, format!("`{needle}`이 원본에 없다")),
        n => fail(2, format!("`{needle}`이 원본에 {n}번 있다. 더 긴 문자열로 하나만 고른다")),
    }
}

/// `old`가 원본에 정확히 한 번 있을 때만 바꾼다.
pub fn replace_unique(text: &str, old: &str, new: &str) -> Result<String> {
    count_unique(text, old)?;
    Ok(text.replacen(old, new, 1))
}

/// `needle`과 같은 줄이 정확히 하나면 그 줄을, 없으면 `needle`을 포함하는 유일한 줄을 지운다.
pub fn remove_line(text: &str, needle: &str) -> Result<String> {
    if needle.is_empty() {
        return fail(2, "지울 문자열이 비었다");
    }
    let lines = lines_of(text);
    let exact: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].trim() == needle.trim()).collect();
    let hits = if exact.is_empty() {
        (0..lines.len()).filter(|&i| lines[i].contains(needle)).collect()
    } else {
        exact
    };
    match hits.as_slice() {
        [i] => {
            let mut rest = lines.clone();
            rest.remove(*i);
            Ok(join(&rest))
        }
        [] => fail(2, format!("`{needle}`을 포함한 줄이 없다")),
        many => fail(2, format!("`{needle}`을 포함한 줄이 {}개다(줄 {})", many.len(),
            many.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(", "))),
    }
}

// ---- 렌더·동기화 ----

/// `tool` 활성본 내용: 생성 헤더 + `@only` 블록을 거른 원본.
pub fn render(text: &str, tool: &str) -> String {
    let mut out = String::from(HEADER);
    let mut keep = true;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("<!-- @only ").and_then(|r| r.strip_suffix("-->")) {
            keep = rest.split(',').any(|s| s.trim() == tool);
            continue;
        }
        if t == "<!-- @end -->" {
            keep = true;
            continue;
        }
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[derive(Debug, PartialEq)]
pub enum State {
    Synced,
    Stale,
    Missing,
    Drift,
}

impl State {
    pub fn label(&self) -> &'static str {
        match self {
            State::Synced => "동기화됨",
            State::Stale => "갱신 필요",
            State::Missing => "없음",
            State::Drift => "활성본 직접 수정됨",
        }
    }
}

fn recorded(p: &Paths, t: &Target) -> Option<String> {
    fs::read_to_string(p.state.join(format!("{}.md", t.name))).ok()
}

/// 활성본 상태와 렌더 결과.
pub fn inspect(p: &Paths, source: &str, t: &Target) -> (State, String) {
    let want = render(source, t.tool);
    let state = match fs::read_to_string(p.target(t)) {
        Err(_) => State::Missing,
        Ok(have) => {
            if have == want {
                State::Synced
            } else if recorded(p, t).is_some_and(|r| r == have) {
                State::Stale
            } else {
                State::Drift
            }
        }
    };
    (state, want)
}

pub struct Report {
    pub name: &'static str,
    pub path: PathBuf,
    pub state: State,
    pub written: bool,
    pub bytes: usize,
    pub backup: Option<PathBuf>,
}

/// 원본을 렌더해 활성본에 쓴다. drift 대상은 백업만 하고 `force`일 때만 덮어쓴다.
pub fn sync(p: &Paths, only: Option<&str>, force: bool) -> Result<Vec<Report>> {
    let source = read_source(p)?;
    let selected: Vec<&Target> = TARGETS.iter().filter(|t| only.is_none_or(|o| o == t.name || o == t.tool)).collect();
    if selected.is_empty() {
        let names: Vec<_> = TARGETS.iter().map(|t| t.name).collect();
        return fail(2, format!("대상 `{}`이 없다: {}", only.unwrap_or(""), names.join(", ")));
    }
    let mut reports = Vec::new();
    for t in selected {
        let path = p.target(t);
        let (state, want) = inspect(p, &source, t);
        let mut backup = None;
        if state == State::Drift {
            backup = Some(back_up(p, t, &path)?);
        }
        let write = state != State::Synced && (state != State::Drift || force);
        if write {
            if let Some(dir) = path.parent() {
                io(dir, fs::create_dir_all(dir))?;
            }
            io(&path, fs::write(&path, &want))?;
        }
        let current = fs::read_to_string(&path).unwrap_or_default();
        if current == want {
            io(&p.state, fs::create_dir_all(&p.state))?;
            let file = p.state.join(format!("{}.md", t.name));
            io(&file, fs::write(&file, &want))?;
        }
        reports.push(Report { name: t.name, path, state, written: write, bytes: want.len(), backup });
    }
    Ok(reports)
}

fn back_up(p: &Paths, t: &Target, path: &Path) -> Result<PathBuf> {
    let dir = p.state.join("backup");
    io(&dir, fs::create_dir_all(&dir))?;
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let file = dir.join(format!("{}-{secs}.md", t.name));
    io(&file, fs::copy(path, &file))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_ignores_headings_in_code_fences() {
        let text = "# A\n```\n# 코드\n```\n- a\n# B\n";
        assert_eq!(section(text, "A").unwrap(), "# A\n```\n# 코드\n```\n- a\n");
    }

    #[test]
    fn add_line_goes_before_trailing_blank_lines() {
        let text = "## A\n- a\n\n## B\n";
        assert_eq!(add_line(text, "A", "- a2").unwrap(), "## A\n- a\n- a2\n\n## B\n");
    }
}
