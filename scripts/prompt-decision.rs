// A single decision per user prompt, shared by PreToolUse and Stop hooks.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label { Improvement, Feature, Resolution, Question, Continue, Stop }

impl Label {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "개선" => Some(Self::Improvement),
            "기능" => Some(Self::Feature),
            "해결" => Some(Self::Resolution),
            "질문" => Some(Self::Question),
            "계속" => Some(Self::Continue),
            "중지" => Some(Self::Stop),
            _ => None,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Improvement => "개선", Self::Feature => "기능",
            Self::Resolution => "해결", Self::Question => "질문", Self::Continue => "계속", Self::Stop => "중지",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptDecision {
    pub label: Option<Label>,
    pub process: bool,
    pub started: u64,
    pub prior_rows: Vec<String>,
}

pub fn decision_state_dir() -> PathBuf {
    std::env::var_os("TASK_COMPLETE_STATE_DIR").filter(|v| !v.is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".codex/tmp/task_complete"))
}

fn safe(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

fn row_ids(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_digit()).filter(|id| !id.is_empty()).map(str::to_string).collect()
}

fn decision_bin() -> PathBuf {
    std::env::var_os("MANAGE_DECISION_BIN").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/tree/extensions/manage_decision/target/release/manage_decision"))
}

fn override_label(prompt: &str) -> Option<Label> {
    if matches!(prompt.trim(), "그렇게 해줘" | "그렇게 해" | "그대로 해줘" | "그대로 진행해줘" | "진행해줘" | "진행해") {
        return Some(Label::Continue);
    }
    let log = std::env::var_os("MANAGE_DECISION_HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/tree/extensions/manage_decision")).join("logs/decisions.jsonl");
    let prefix: String = prompt.chars().take(4000).collect();
    let (mut id, mut truths) = (None, std::collections::HashMap::new());
    for line in std::fs::read_to_string(log).ok()?.lines() {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(row_id) = row.get("id").and_then(serde_json::Value::as_str) else { continue };
        if row.get("prompt").and_then(serde_json::Value::as_str).is_some_and(|p| p == prompt || p == prefix) { id = Some(row_id.to_string()); }
        if let Some(t) = row.get("truth").and_then(serde_json::Value::as_str) { truths.insert(row_id.to_string(), t.to_string()); }
    }
    id.and_then(|id| truths.get(&id).and_then(|t| Label::parse(t)))
}

fn decide(prompt: &str) -> Option<Label> {
    decide_with_bin(prompt, &decision_bin(), Duration::from_secs(8))
}

fn decide_with_bin(prompt: &str, bin: &Path, timeout: Duration) -> Option<Label> {
    let mut child = Command::new(bin).arg("decide")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut stdin = child.stdin.take()?;
    let input = prompt.as_bytes().to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => { let _ = child.kill(); let _ = child.wait(); return None; }
        }
    }
    if writer.join().ok()?.is_err() { return None; }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() { return None; }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.get("label").and_then(serde_json::Value::as_str).and_then(Label::parse)
}

pub fn decision_at(state: &Path, session: &str, prompt: &str) -> PromptDecision {
    decision_with(state, session, prompt, decide)
}

pub fn decision_with(state: &Path, session: &str, prompt: &str, classify: impl FnOnce(&str) -> Option<Label>) -> PromptDecision {
    let path = state.join("process_turn").join(safe(session));
    let forced = override_label(prompt);
    if let Ok(text) = std::fs::read_to_string(&path) {
        let mut lines = text.splitn(4, '\n');
        let (started, label, prior, saved) = (lines.next(), lines.next(), lines.next(), lines.next());
        if saved == Some(prompt) {
            if let (Some(Ok(started)), Some(label), Some(prior)) = (started.map(str::parse), label, prior) {
                let label = forced.or_else(|| Label::parse(label));
                return PromptDecision { label, process: label == Some(Label::Improvement), started, prior_rows: row_ids(prior) };
            }
        }
    }
    let rows = std::fs::read_to_string(state.join("session_rows").join(format!("{}.json", safe(session))))
        .map(|text| row_ids(&text)).unwrap_or_default();
    let label = forced.or_else(|| classify(prompt));
    let turn = PromptDecision {
        label, process: label == Some(Label::Improvement),
        started: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        prior_rows: rows,
    };
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    let _ = std::fs::write(&path, format!("{}\n{}\n{}\n{prompt}", turn.started, label.map(Label::as_str).unwrap_or(""), turn.prior_rows.join(",")));
    turn
}

pub fn decision(session: &str, prompt: &str) -> PromptDecision {
    decision_at(&decision_state_dir(), session, prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn labels_and_fail_open_are_cached_once() {
        let dir = PathBuf::from("/home/tree/extensions/test/tmp").join(format!("decision-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("session_rows")).unwrap();
        std::fs::write(dir.join("session_rows/s.json"), "[23,45,67]").unwrap();
        let calls = Cell::new(0);
        for (index, label, expected) in [Some(Label::Improvement), Some(Label::Feature), Some(Label::Resolution), Some(Label::Question), Some(Label::Continue), Some(Label::Stop), None].into_iter().enumerate().map(|(i,l)| (i,l,l == Some(Label::Improvement))) {
            let prompt = format!("prompt-{index}");
            let classify = |_: &str| { calls.set(calls.get()+1); label };
            let first = decision_with(&dir, "s", &prompt, classify);
            assert_eq!(first.process, expected);
            assert_eq!(first.label, label);
            assert_eq!(first.prior_rows, ["23", "45", "67"]);
            assert_eq!(decision_with(&dir, "s", &prompt, |_| panic!("cached label")), first);
        }
        assert_eq!(calls.get(), 7);
        assert_eq!(decision_with(&dir, "s", "그렇게 해줘", |_| Some(Label::Question)).label, Some(Label::Continue));
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        std::fs::write(dir.join("logs/decisions.jsonl"), "{\"id\":\"d1\",\"prompt\":\"prompt-0\",\"label\":\"개선\"}\n{\"id\":\"d1\",\"truth\":\"질문\"}\n").unwrap();
        std::env::set_var("MANAGE_DECISION_HOME", &dir);
        let corrected = decision_with(&dir, "s", "prompt-0", |_| panic!("cached"));
        std::env::remove_var("MANAGE_DECISION_HOME");
        assert_eq!((corrected.label, corrected.process), (Some(Label::Question), false));
        eprintln!("[verify-step] range-check start=23 mid=45 end=67");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cli_json_exit_and_timeout_follow_fail_open() {
        use std::os::unix::fs::PermissionsExt;
        let dir = PathBuf::from("/home/tree/extensions/test/tmp").join(format!("decision-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake.sh");
        let calls = dir.join("calls");
        let write_script = |body: &str| {
            std::fs::write(&script, body).unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        write_script("#!/bin/sh\ncat >/dev/null\nprintf '{\"label\":\"계속\",\"provider\":\"keyword\"}'\n");
        assert_eq!(decide_with_bin("계속해", &script, Duration::from_secs(1)), Some(Label::Continue));
        assert!(!decision_with(&dir, "continue", "계속해", |_| Some(Label::Continue)).process);
        write_script("#!/bin/sh\ncat >/dev/null\nprintf '{\"label\":\"중지\",\"provider\":\"keyword\"}'\n");
        assert_eq!(decide_with_bin("중지해", &script, Duration::from_secs(1)), Some(Label::Stop));
        assert!(!decision_with(&dir, "stop", "중지해", |_| Some(Label::Stop)).process);
        for (label, expected) in [("개선", Label::Improvement), ("기능", Label::Feature), ("해결", Label::Resolution), ("질문", Label::Question)] {
            write_script(&format!("#!/bin/sh\nprintf x >> {}\ncat >/dev/null\nprintf '{{\"label\":\"{label}\"}}\\n'\n", calls.display()));
            assert_eq!(decide_with_bin("프롬프트", &script, Duration::from_secs(1)), Some(expected));
        }
        assert_eq!(std::fs::read_to_string(&calls).unwrap(), "xxxx");
        std::env::set_var("MANAGE_DECISION_BIN", &script);
        assert_eq!(decision_bin(), script);
        assert_eq!(decision_at(&dir, "env-session", "환경변수 경로").label, Some(Label::Question));
        std::env::remove_var("MANAGE_DECISION_BIN");
        let cached = decision_with(&dir, "session", "한 프롬프트", |p| decide_with_bin(p, &script, Duration::from_secs(1)));
        assert_eq!(cached.label, Some(Label::Question));
        assert_eq!(decision_with(&dir, "session", "한 프롬프트", |_| panic!("cached")), cached);
        assert_eq!(std::fs::read_to_string(&calls).unwrap(), "xxxxxx");
        write_script("#!/bin/sh\ncat >/dev/null\nexit 2\n");
        assert_eq!(decide_with_bin("프롬프트", &script, Duration::from_secs(1)), None);
        let failed = decision_with(&dir, "failed-session", "CLI 오류", |p| decide_with_bin(p, &script, Duration::from_secs(1)));
        assert_eq!(failed.label, None);
        assert_eq!(decision_with(&dir, "failed-session", "CLI 오류", |_| panic!("failure cached")), failed);
        write_script("#!/bin/sh\ncat >/dev/null\nsleep 2\n");
        assert_eq!(decide_with_bin("프롬프트", &script, Duration::from_millis(50)), None);
        write_script("#!/bin/sh\ncat >/dev/null\nprintf '{bad json}'\n");
        assert_eq!(decide_with_bin("프롬프트", &script, Duration::from_secs(1)), None);
        write_script("#!/bin/sh\ncat >/dev/null\nprintf '{\"label\":\"unknown\"}'\n");
        assert_eq!(decide_with_bin("프롬프트", &script, Duration::from_secs(1)), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
