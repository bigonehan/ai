use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

const EXTENSIONS: &str = "/home/tree/extensions";
const AI: &str = "/home/tree/ai";

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => { result.pop(); }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn target(event: &Value) -> Option<(PathBuf, PathBuf, String)> {
    let name = event.get("tool_name")?.as_str()?;
    if !matches!(name, "Edit" | "Write" | "MultiEdit" | "NotebookEdit") { return None; }
    let input = event.get("tool_input")?;
    let file = input.get(if name == "NotebookEdit" { "notebook_path" } else { "file_path" })?.as_str()?;
    let cwd = Path::new(event.get("cwd")?.as_str()?);
    let file = normalize(&if Path::new(file).is_absolute() { PathBuf::from(file) } else { cwd.join(file) });
    let session = event.get("session_id")?.as_str()?;
    if session.is_empty() || !session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return None; }
    let project = if let Ok(relative) = file.strip_prefix(EXTENSIONS) {
        let mut parts = relative.components();
        let first = parts.next()?;
        if parts.next().is_some() { Path::new(EXTENSIONS).join(first.as_os_str()) } else { PathBuf::from(EXTENSIONS) }
    } else if file.starts_with(AI) { PathBuf::from(EXTENSIONS) } else { return None };
    let name = file.file_name()?.to_str()?;
    if name == "AGENTS.md" || file.components().any(|c| c.as_os_str() == ".ai") || file.to_string_lossy().contains("/test/tmp/") { return None; }
    Some((project, file, session.to_string()))
}

fn decision(paths: &BTreeSet<String>, next: &str, worker: bool) -> bool {
    worker || paths.contains(next) || paths.len() < 2
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).map_err(|e| e.to_string())?;
    let event: Value = serde_json::from_str(&input).map_err(|e| e.to_string())?;
    if std::env::var("T26_ROLE").ok().as_deref() == Some("worker") { return Ok(()); }
    let Some((project, file, session)) = target(&event) else { return Ok(()) };
    let state = project.join(".ai/gate").join(format!("main-edits-{session}.json"));
    let mut paths: BTreeSet<String> = if state.exists() {
        serde_json::from_slice(&std::fs::read(&state).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?
    } else { BTreeSet::new() };
    let next = file.to_string_lossy().to_string();
    if !decision(&paths, &next, false) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"메인 세션의 세 번째 파일 수정은 dispatch-agent 위임이 필요합니다."}}));
        return Ok(());
    }
    if paths.insert(next) {
        std::fs::create_dir_all(state.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(state, serde_json::to_vec(&paths).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() { eprintln!("main-edit-gate: {error}"); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_edit_denied_and_repeat_allowed() {
        let paths = BTreeSet::from(["/a".into(), "/b".into()]);
        assert!(!decision(&paths, "/c", false));
        assert!(decision(&paths, "/a", false));
        assert!(decision(&paths, "/c", true));
    }

    #[test]
    fn worker_and_outside_scope() {
        let event = |file: &str| json!({"tool_name":"Edit","tool_input":{"file_path":file},"cwd":"/home/tree/extensions","session_id":"abc"});
        assert!(target(&event("/tmp/file.rs")).is_none());
        assert!(target(&event("/home/tree/extensions/app/.ai/x")).is_none());
        assert!(target(&event("/home/tree/extensions/app/AGENTS.md")).is_none());
        assert_eq!(target(&event("/home/tree/ai/scripts/a.rs")).unwrap().0, Path::new(EXTENSIONS));
    }

    #[test]
    fn hidden_375_normalizes_and_assigns_project_scope() {
        let event = |file: &str| json!({
            "tool_name": "Edit",
            "tool_input": { "file_path": file },
            "cwd": "/home/tree/extensions",
            "session_id": "hidden_375"
        });
        let (project, file, session) = target(&event("subproject/src/../src/third.rs")).unwrap();
        assert_eq!(file, Path::new("/home/tree/extensions/subproject/src/third.rs"));
        assert_eq!(project, Path::new("/home/tree/extensions/subproject"));
        assert_eq!(session, "hidden_375");
        assert_eq!(target(&event("root.rs")).unwrap().0, Path::new(EXTENSIONS));
        assert_eq!(target(&event("/home/tree/ai/scripts/main-edit-gate/src/main.rs")).unwrap().0, Path::new(EXTENSIONS));
        assert!(target(&event("/home/tree/extensions/subproject/test/tmp/probe.rs")).is_none());
    }
}
