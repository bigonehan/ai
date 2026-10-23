//! agy(Antigravity) hook adapter: converts agy camelCase hook input/output to the snake_case gates.
mod json;

use json::{dumps, obj_set, parse, s, Value};
use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn bin(name: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/ai/scripts/bin/{name}")
}

fn first_str<'a>(event: &'a Value, key: &str) -> &'a str {
    event.get(key).and_then(Value::as_str).unwrap_or("")
}

fn workspace(event: &Value) -> String {
    match event.get("workspacePaths") {
        Some(Value::Arr(items)) => items.first().and_then(Value::as_str).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

/// agy input ⇒ snake_case input for the Claude/Codex gates.
pub fn to_snake(event: &Value, hook_event_name: &str) -> String {
    let mut o = Vec::new();
    obj_set(&mut o, "session_id", s(first_str(event, "conversationId")));
    obj_set(&mut o, "cwd", s(workspace(event)));
    obj_set(&mut o, "transcript_path", s(first_str(event, "transcriptPath")));
    obj_set(&mut o, "hook_event_name", s(hook_event_name));
    dumps(&Value::Obj(o))
}

fn inject(text: &str) -> String {
    let inner = Value::Obj(vec![("systemMessage".into(), s(text))]);
    let step = Value::Obj(vec![("systemMessage".into(), inner)]);
    dumps(&Value::Obj(vec![("injectSteps".into(), Value::Arr(vec![step]))]))
}

/// architecture-context output ⇒ agy SessionStart injectSteps.
pub fn session_output(child_stdout: &str) -> String {
    let v = parse(child_stdout.trim()).unwrap_or(Value::Obj(vec![]));
    let text = v
        .get("hookSpecificOutput")
        .and_then(|h| h.get("additionalContext"))
        .and_then(Value::as_str)
        .or_else(|| v.get("systemMessage").and_then(Value::as_str))
        .unwrap_or("");
    if text.is_empty() { "{}".into() } else { inject(text) }
}

/// True when `ws` is `root` or lies under it (boundary-aware: `/x/extensions2` is outside `/x/extensions`).
pub fn in_extensions(ws: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    ws == root || ws.strip_prefix(root).map_or(false, |r| r.starts_with('/'))
}

fn pre_tool(decision: &str, reason: &str) -> String {
    let mut o = Vec::new();
    obj_set(&mut o, "decision", s(decision));
    if !reason.is_empty() {
        obj_set(&mut o, "reason", s(reason));
    }
    dumps(&Value::Obj(o))
}

/// `grist gate` JSON ⇒ agy PreToolUse decision (uninitialized or no open row ⇒ deny).
pub fn pre_edit_output(gate_stdout: &str) -> String {
    let v = match parse(gate_stdout.trim()) {
        Ok(v) => v,
        Err(_) => return pre_tool("allow", ""),
    };
    if let Some(dir) = v.get("uninitialized").filter(|x| x.truthy()).and_then(Value::as_str) {
        return pre_tool(
            "deny",
            &format!("미초기화 프로젝트({dir})예요. 수정 전에 manage-project 초기화 절차로 Table7·Obsidian·# 상태 헤더를 만들고 request-record·task-add·task-goal로 요청을 기록하세요."),
        );
    }
    let open = match v.get("rows") {
        Some(Value::Arr(rows)) => rows.iter().any(|r| r.get("H").and_then(Value::as_str) != Some("완료")),
        _ => false,
    };
    if open {
        pre_tool("allow", "")
    } else {
        pre_tool(
            "deny",
            "열린 Table26 작업 행이 없어요. 수정 전에 manage-project 절차로 request-record·task-add·task-goal을 실행해 작업 행을 만드세요.",
        )
    }
}

/// Claude/Codex gate output ⇒ agy Stop (block ⇒ continue, otherwise allow stop).
pub fn stop_output(child_stdout: &str) -> Option<String> {
    let v = parse(child_stdout.trim()).ok()?;
    if v.get("decision").and_then(Value::as_str) != Some("block") {
        return None;
    }
    let reason = first_str(&v, "reason");
    let mut o = Vec::new();
    obj_set(&mut o, "decision", s("continue"));
    obj_set(&mut o, "reason", s(reason));
    Some(dumps(&Value::Obj(o)))
}

fn run(name: &str, args: &[&str], stdin: &str) -> Result<String, String> {
    let mut child = Command::new(bin(name))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{name}: {e}"))?;
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(stdin.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| format!("{name}: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn main() {
    let sub = std::env::args().nth(1).unwrap_or_default();
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let event = parse(&input).unwrap_or(Value::Obj(vec![]));
    let out = match sub.as_str() {
        "session-start" => run("architecture-context", &[], &to_snake(&event, "SessionStart"))
            .map(|o| session_output(&o)),
        "pre-edit" => {
            let cwd = workspace(&event);
            let root = format!("{}/extensions", std::env::var("HOME").unwrap_or_default());
            if cwd.is_empty() || !in_extensions(&cwd, &root) {
                Ok(pre_tool("allow", ""))
            } else {
                run("grist", &["gate", "--cwd", &cwd], "").map(|o| pre_edit_output(&o))
            }
        }
        "stop" => {
            let reason = first_str(&event, "terminationReason");
            if reason == "USER_CANCELED" || reason == "ERROR" {
                Ok("{}".into())
            } else {
                let snake = to_snake(&event, "Stop");
                Ok(run("task-complete", &[], &snake).ok().and_then(|o| stop_output(&o)).unwrap_or_else(|| "{}".into()))
            }
        }
        other => Err(format!("unknown subcommand {other}")),
    };
    match out {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("agy-hook: {e}");
            if sub == "pre-edit" {
                println!("{}", pre_tool("allow", ""));
            } else {
                println!("{{}}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_output_wraps_additional_context() {
        let o = session_output(r#"{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"구조 a"}}"#);
        assert_eq!(o, r#"{"injectSteps": [{"systemMessage": {"systemMessage": "구조 a"}}]}"#);
    }

    #[test]
    fn to_snake_maps_fields() {
        let e = parse(r#"{"conversationId":"c1","workspacePaths":["/w"],"transcriptPath":"/t"}"#).unwrap();
        assert_eq!(
            to_snake(&e, "Stop"),
            r#"{"session_id": "c1", "cwd": "/w", "transcript_path": "/t", "hook_event_name": "Stop"}"#
        );
    }

    #[test]
    fn in_extensions_respects_boundary() {
        let r = "/home/tree/extensions";
        assert!(in_extensions("/home/tree/extensions", r));
        assert!(in_extensions("/home/tree/extensions/save_current_page", r));
        assert!(!in_extensions("/home/tree/extensions2", r));
        assert!(!in_extensions("/tmp", r));
    }

    #[test]
    fn pre_edit_decisions() {
        assert!(pre_edit_output(r#"{"uninitialized":"/p","rows":[]}"#).contains(r#""decision": "deny""#));
        assert!(pre_edit_output(r#"{"rows":[]}"#).contains("manage-project"));
        assert_eq!(pre_edit_output(r#"{"rows":[{"id":1,"H":"작업"}]}"#), r#"{"decision": "allow"}"#);
        assert_eq!(pre_edit_output("garbage"), r#"{"decision": "allow"}"#);
    }

    #[test]
    fn stop_block_becomes_continue() {
        assert_eq!(
            stop_output(r#"{"decision":"block","reason":"r"}"#).unwrap(),
            r#"{"decision": "continue", "reason": "r"}"#
        );
        assert!(stop_output("{}").is_none());
    }
}
