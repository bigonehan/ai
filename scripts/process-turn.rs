// Process-correction turn state shared by main-edit-gate (PreToolUse) and task-complete (Stop).
// One user prompt gets one state file, so both hooks judge the same turn the same way.
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RESUME: [&str; 4] = ["이전작업계속", "원래작업재개", "제품작업재개", "계속작업해"];
const SUBJECT: [&str; 11] = ["작업과정", "과정오류", "절차", "규칙", "skill", "hook", "게이트", "재위임", "개선자", "검증정체", "반복출력"];
const CORRECTION: [&str; 10] = ["수정", "고쳐", "고치", "개선", "막", "왜", "지적", "잘못", "오류", "위반"];

/// A user prompt that points at the work process (not the product) and asks to fix it.
/// Spacing and case are ignored ("작업과정" = "작업 과정", "Hook" = "hook").
pub fn is_process_correction(prompt: &str) -> bool {
    let bare: String = prompt.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    if RESUME.iter().any(|word| bare.contains(word)) { return false; }
    SUBJECT.iter().any(|word| bare.contains(word)) && CORRECTION.iter().any(|word| bare.contains(word))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessTurn {
    pub process: bool,
    /// Unix seconds when the first hook saw this prompt.
    pub started: u64,
    /// Session rows that already existed before this prompt; they are not evidence for it.
    pub prior_rows: Vec<String>,
}

pub fn process_state_dir() -> PathBuf {
    std::env::var_os("TASK_COMPLETE_STATE_DIR").filter(|v| !v.is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".codex/tmp/task_complete"))
}

fn safe(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

fn row_ids(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_digit()).filter(|id| !id.is_empty()).map(str::to_string).collect()
}

/// The turn state for `prompt`; the first hook call of a new prompt records it with the rows known so far.
pub fn process_turn_at(state: &Path, session: &str, prompt: &str) -> ProcessTurn {
    let path = state.join("process_turn").join(safe(session));
    if let Ok(text) = std::fs::read_to_string(&path) {
        let mut lines = text.splitn(4, '\n');
        let (started, process, prior, saved) = (lines.next(), lines.next(), lines.next(), lines.next());
        if saved == Some(prompt) {
            if let (Some(Ok(started)), Some(process), Some(prior)) = (started.map(str::parse), process, prior) {
                return ProcessTurn { process: process == "1", started, prior_rows: row_ids(prior) };
            }
        }
    }
    let rows = std::fs::read_to_string(state.join("session_rows").join(format!("{}.json", safe(session))))
        .map(|text| row_ids(&text)).unwrap_or_default();
    let turn = ProcessTurn {
        process: is_process_correction(prompt),
        started: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        prior_rows: rows,
    };
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    let _ = std::fs::write(&path, format!("{}\n{}\n{}\n{prompt}", turn.started, if turn.process { 1 } else { 0 }, turn.prior_rows.join(",")));
    turn
}

pub fn process_turn(session: &str, prompt: &str) -> ProcessTurn {
    process_turn_at(&process_state_dir(), session, prompt)
}

#[cfg(test)]
mod process_turn_tests {
    #[test]
    fn spacing_variants_and_resume_are_judged_once_per_prompt() {
        assert!(super::is_process_correction("현재 처리하지 못한 작업과정의 문제를 개선해"));
        assert!(super::is_process_correction("Hook 이 왜 안 막아?"));
        assert!(!super::is_process_correction("이전 작업 계속해 주세요, 규칙 수정은 끝났어요"));
        assert!(!super::is_process_correction("저장 버튼 색을 수정해"));
        let dir = std::env::temp_dir().join(format!("process-turn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("session_rows")).unwrap();
        std::fs::write(dir.join("session_rows/s1.json"), "[\"851\", 860]").unwrap();
        let first = super::process_turn_at(&dir, "s1", "작업과정 고쳐");
        assert_eq!((first.process, first.prior_rows.clone()), (true, vec!["851".to_string(), "860".to_string()]));
        // rows added during the turn do not move the turn's snapshot
        std::fs::write(dir.join("session_rows/s1.json"), "[851, 860, 862]").unwrap();
        assert_eq!(super::process_turn_at(&dir, "s1", "작업과정 고쳐"), first);
        // a new prompt starts a new turn with the rows known by then
        let next = super::process_turn_at(&dir, "s1", "이전 작업 계속");
        assert_eq!((next.process, next.prior_rows.len()), (false, 3));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
