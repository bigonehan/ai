// Supply project structure before an extensions task begins.
// Build: cargo build --release -p architecture-context --manifest-path ~/ai/scripts/Cargo.toml
use regex::Regex;
use serde_json::{Map, Value};
use std::env;
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, IsTerminal, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{self, Command, Stdio};

const ROOT: &str = "/home/tree/extensions";

fn resolved(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let mut result = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn project_from(path: &str) -> Option<String> {
    let absolute = resolved(Path::new(path));
    if absolute == Path::new(ROOT) {
        return Some(ROOT.to_owned());
    }
    let abs_str = absolute.to_string_lossy();
    let prefix = format!("{ROOT}/");
    if !abs_str.starts_with(&prefix) {
        return None;
    }
    let name = abs_str[prefix.len()..].split('/').next().unwrap_or("");
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let project = Path::new(ROOT).join(name);
    if project.join("AGENTS.md").exists() {
        Some(project.to_string_lossy().into_owned())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_root_is_a_project() {
        assert_eq!(project_from(ROOT).as_deref(), Some(ROOT));
        assert_eq!(project_from("/tmp"), None);
    }

    #[test]
    fn skill_args_keep_quoted_query() {
        let event = serde_json::json!({"tool_name":"Skill","tool_input":{"skill":"manage-project","args":"--project /home/tree/extensions --query \"진입 hook\""}});
        assert_eq!(skill_args(&event), Some((Some(ROOT.into()), Some("진입 hook".into()))));
        assert_eq!(skill_args(&serde_json::json!({"tool_name":"Skill","tool_input":{"skill":"other"}})), None);
    }

    #[test]
    fn read_command_and_notification_are_scoped() {
        let read = serde_json::json!({"tool_name":"Bash","tool_input":{"command":"sed -n 1,5p ~/.codex/skills/manage-project/SKILL.md"}});
        let mention = serde_json::json!({"tool_name":"Bash","tool_input":{"command":"printf manage-project/SKILL.md"}});
        assert!(reads_manage_skill(&read));
        assert!(!reads_manage_skill(&mention));
        assert!(notification("<task-notification>done"));
        assert!(notification("[SYSTEM NOTIFICATION: done"));
        assert!(notification("Stop hook feedback: pending"));
        assert!(!notification("구조를 확인해 주세요"));
    }

    #[test]
    fn session_project_emits_only_once() {
        let dir = std::env::temp_dir().join(format!("architecture-context-test-{}", std::process::id()));
        let event = serde_json::json!({"session_id":"test-session"});
        let _ = std::fs::remove_dir_all(&dir);
        assert!(first_in_session_at(&event, ROOT, &dir));
        assert!(!first_in_session_at(&event, ROOT, &dir));
        assert!(first_in_session_at(&event, "/home/tree/extensions/eagle_move", &dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prompt_output_is_empty_for_notice_and_second_project_prompt() {
        let dir = std::env::temp_dir().join(format!("architecture-context-render-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let prompt = serde_json::json!({"session_id":"render-session","hook_event_name":"UserPromptSubmit","prompt":"프로젝트 확인"});
        let notice = serde_json::json!({"session_id":"render-notice","hook_event_name":"UserPromptSubmit","prompt":"<task-notification>done"});
        let first = render_context(&prompt, ROOT, "# 구조", &dir);
        assert!(first.contains("\"additionalContext\":\"# 구조\""));
        assert_eq!(render_context(&prompt, ROOT, "# 구조", &dir), "");
        assert_eq!(render_context(&notice, ROOT, "# 구조", &dir), "");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn session_start_and_resumed_prompt_share_one_injection() {
        let dir = std::env::temp_dir().join(format!("architecture-context-resume-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let start = serde_json::json!({"session_id":"resume-session","hook_event_name":"SessionStart"});
        let prompt = serde_json::json!({"session_id":"resume-session","hook_event_name":"UserPromptSubmit","prompt":"다음 요청"});
        assert!(!render_context(&start, ROOT, "# 구조", &dir).is_empty());
        assert_eq!(render_context(&prompt, ROOT, "# 구조", &dir), "");
        assert_eq!(render_context(&start, ROOT, "# 구조", &dir), "");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn hook_events_emit_once_per_session_and_project() {
        let dir = std::env::temp_dir().join(format!("architecture-context-events-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let schema = "# 구조\nschema=v1\n## 3. 위임 메시지";
        let start = serde_json::json!({"session_id":"events-session","hook_event_name":"SessionStart"});
        let prompt = serde_json::json!({"session_id":"events-session","hook_event_name":"UserPromptSubmit","prompt":"다음 요청"});
        let skill = serde_json::json!({"session_id":"events-session","hook_event_name":"PostToolUse","tool_name":"Skill"});
        let other = "/home/tree/extensions/grist_video_maker";

        assert!(render_context(&start, ROOT, schema, &dir).contains("schema=v1"));
        assert_eq!(render_context(&skill, ROOT, schema, &dir), "");
        assert_eq!(render_context(&prompt, ROOT, schema, &dir), "");
        assert!(!render_context(&prompt, other, schema, &dir).is_empty());
        assert_eq!(render_context(&skill, other, schema, &dir), "");
        assert_eq!(render_context(&skill, ROOT, schema, &dir), "");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn skip_prompt_only_leaves_post_tool_use_active() {
        for name in ["SessionStart", "UserPromptSubmit"] {
            let event = serde_json::json!({"hook_event_name":name});
            assert!(prompt_hook_suppressed(&event, true));
            assert!(!prompt_hook_suppressed(&event, false));
        }
        let event = serde_json::json!({"hook_event_name":"PostToolUse"});
        assert!(!prompt_hook_suppressed(&event, true));
    }

    #[test]
    fn schema_is_injected_only_for_delegator_once_per_session() {
        let schema = "schema=v1\n## 1. 판정 타입\n## 2. 기호\n## 3. 위임 메시지\n## 4. 분해\n";
        for role in [Some("worker"), Some("verifier")] {
            assert!(schema_context(role, Some(schema)).is_empty());
        }
        assert_eq!(schema_context(None, Some(schema)), schema.trim_end());
        let dir = std::env::temp_dir().join(format!("architecture-context-schema-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let event = serde_json::json!({"session_id":"schema-session","hook_event_name":"SessionStart"});
        let context = format!("# 구조\n{}", schema_context(None, Some(schema)));
        let first = render_context(&event, ROOT, &context, &dir);
        assert!(first.contains("schema=v1\\n## 1. 판정 타입"));
        assert_eq!(render_context(&event, ROOT, &context, &dir), "");
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn reader_bin() -> PathBuf {
    if let Some(bin) = env::var_os("READ_ARCHITECTURE_BIN").filter(|v| !v.is_empty()) {
        return PathBuf::from(bin);
    }
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/tree"));
    home.join("ai/scripts/bin/read-architecture")
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

fn read_stdin_bounded() -> Option<String> {
    if io::stdin().is_terminal() || !stdin_ready(20) {
        return None;
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut stdin = io::stdin().lock();
    loop {
        match stdin.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&chunk[..n]);
                if !stdin_ready(5) {
                    break;
                }
            }
            Err(_) => return None,
        }
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn notification(prompt: &str) -> bool {
    ["<task-notification>", "[SYSTEM NOTIFICATION", "Stop hook feedback"]
        .iter().any(|prefix| prompt.trim_start().starts_with(prefix))
}

fn skill_args(event: &Value) -> Option<(Option<String>, Option<String>)> {
    if event.get("tool_name").and_then(Value::as_str) != Some("Skill") { return None; }
    let input = event.get("tool_input")?;
    let skill = input.get("skill").or_else(|| input.get("name"))?.as_str()?;
    if skill != "manage-project" { return None; }
    let args = input.get("args").and_then(Value::as_str).unwrap_or("");
    let tokens = shell_words(args);
    let value = |flag: &str| tokens.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    Some((value("--project"), value("--query")))
}

fn shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut active = false;
    let mut escaped = false;
    for ch in input.chars() {
        if escaped { word.push(ch); escaped = false; active = true; continue; }
        if ch == '\\' && quote != Some('\'') { escaped = true; continue; }
        if Some(ch) == quote { quote = None; active = true; continue; }
        if quote.is_none() && (ch == '\'' || ch == '"') { quote = Some(ch); active = true; continue; }
        if quote.is_none() && ch.is_whitespace() {
            if active { words.push(std::mem::take(&mut word)); active = false; }
        } else { word.push(ch); active = true; }
    }
    if active { words.push(word); }
    words
}

fn reads_manage_skill(event: &Value) -> bool {
    let name = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
    if !["Bash", "bash", "shell", "exec_command", "run_command"].contains(&name) { return false; }
    let command = event.get("tool_input").and_then(|v| v.get("command"))
        .or_else(|| event.get("tool_input").and_then(|v| v.get("cmd")))
        .and_then(Value::as_str).unwrap_or("");
    let tokens = shell_words(command);
    let reader = tokens.iter().any(|s| ["cat", "sed", "head", "tail", "rg", "less", "more"].contains(&s.as_str()));
    reader && tokens.iter().any(|s| s.contains("manage-project/SKILL.md"))
}

fn first_in_session_at(event: &Value, project: &str, dir: &Path) -> bool {
    let Some(session) = event.get("session_id").and_then(Value::as_str).filter(|s| !s.is_empty()) else { return true; };
    if fs::create_dir_all(dir).is_err() { return true; }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (session, project).hash(&mut hasher);
    OpenOptions::new().write(true).create_new(true).open(dir.join(format!("{:016x}", hasher.finish()))).is_ok()
}

fn prompt_hook_suppressed(event: &Value, skip_prompt: bool) -> bool {
    skip_prompt && matches!(event.get("hook_event_name").and_then(Value::as_str),
        Some("SessionStart" | "UserPromptSubmit"))
}

fn schema_context(role: Option<&str>, schema: Option<&str>) -> String {
    if role.is_some_and(|role| !role.is_empty()) { return String::new(); }
    schema.filter(|text| text.lines().next() == Some("schema=v1"))
        .map(|text| text.trim_end().to_owned()).unwrap_or_default()
}

fn schema_file() -> PathBuf {
    env::var_os("RULE_SCHEMA_FILE").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env::var_os("HOME").unwrap_or_default()).join("ai/rules/rule_schema.md")
    })
}

fn render_context(event: &Value, project: &str, context: &str, state_dir: &Path) -> String {
    let hook_event_name = event.get("hook_event_name").and_then(Value::as_str);
    let prompt = event.get("prompt").and_then(Value::as_str).unwrap_or("");
    if hook_event_name == Some("UserPromptSubmit") && notification(prompt) { return String::new(); }
    if !first_in_session_at(event, project, state_dir) { return String::new(); }
    let mut inner = Map::new();
    if let Some(name) = event.get("hook_event_name").filter(|v| !v.is_null()) {
        inner.insert("hookEventName".to_owned(), name.clone());
    }
    inner.insert("additionalContext".to_owned(), Value::String(context.to_owned()));
    let mut outer = Map::new();
    outer.insert("hookSpecificOutput".to_owned(), Value::Object(inner));
    serde_json::to_string(&Value::Object(outer)).unwrap()
}

fn main() {
    let Some(input) = read_stdin_bounded() else {
        process::exit(1);
    };
    let event: Value = match serde_json::from_str(&input) {
        Ok(Value::Object(map)) => Value::Object(map),
        _ => process::exit(1),
    };

    if prompt_hook_suppressed(&event,
        env::var("ARCHITECTURE_CONTEXT_SKIP_PROMPT").as_deref() == Ok("1")) { return; }

    let prompt = event.get("prompt").and_then(Value::as_str).unwrap_or("");
    let hook_event_name = event.get("hook_event_name").and_then(Value::as_str);
    if hook_event_name == Some("UserPromptSubmit") && notification(prompt) { return; }
    let skill = skill_args(&event);
    let command_read = reads_manage_skill(&event);
    if hook_event_name == Some("PostToolUse") && skill.is_none() && !command_read { return; }
    let cwd_raw = event.get("cwd").and_then(Value::as_str).unwrap_or(".");
    let cwd = resolved(Path::new(cwd_raw)).to_string_lossy().into_owned();

    let explicit_re = Regex::new(r#"/home/tree/extensions/[^\s/`'"<>]+"#).unwrap();
    let project = skill.as_ref().and_then(|(p, _)| p.as_deref()).and_then(project_from)
        .or_else(|| explicit_re.find(prompt)
        .and_then(|m| project_from(m.as_str()))
        ).or_else(|| project_from(&cwd));
    let Some(project) = project else {
        process::exit(0);
    };

    let mut args = vec!["--project".to_owned(), project];
    if let Some(query) = skill.and_then(|(_, q)| q).filter(|q| !q.is_empty()) {
        args.push("--query".to_owned());
        args.push(query);
    }

    let script = reader_bin();
    let context = match Command::new(&script).args(&args).stdin(Stdio::null()).output() {
        Ok(result) => {
            let output = String::from_utf8_lossy(&result.stdout).trim().to_owned();
            let error = String::from_utf8_lossy(&result.stderr).trim().to_owned();
            match result.status.code() {
                Some(2) => format!(
                    "{output}\n아키텍처 문서를 작성한 뒤 이 스크립트를 다시 실행하고 파일 탐색을 시작하세요."
                ),
                Some(0) => format!(
                    "{output}\n위 구조에서 대상 폴더를 고른 뒤 rg -n으로 위치를 찾고 필요한 범위만 읽으세요."
                ),
                code => {
                    let detail = if !error.is_empty() {
                        error.chars().take(300).collect::<String>()
                    } else {
                        format!("exit {}", code.unwrap_or(-1))
                    };
                    format!("아키텍처 검사 실행 실패: {detail}")
                }
            }
        }
        Err(err) => {
            let detail = if err.kind() == io::ErrorKind::NotFound {
                format!("spawnSync {} ENOENT", script.display())
            } else {
                err.to_string()
            };
            format!("아키텍처 검사 실행 실패: {detail}")
        }
    };

    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/home/tree"));
    let state_dir = env::var_os("ARCHITECTURE_CONTEXT_STATE_DIR").map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state/architecture-context"));
    let schema = fs::read_to_string(schema_file()).ok();
    let schema = schema_context(env::var("T26_ROLE").ok().as_deref(), schema.as_deref());
    let context = if schema.is_empty() { context } else { format!("{context}\n{schema}") };
    let output = render_context(&event, &args[1], &context, &state_dir);
    if !output.is_empty() { println!("{output}"); }
}
