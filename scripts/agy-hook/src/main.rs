//! agy(Antigravity) hook adapter: converts agy camelCase hook input/output to the snake_case gates.
mod json;

use json::{dumps, obj_set, parse, s, Value};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

fn bin(name: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/ai/scripts/bin/{name}")
}

fn first_str<'a>(event: &'a Value, key: &str) -> &'a str {
    event.get(key).and_then(Value::as_str).unwrap_or("")
}

fn workspace(event: &Value) -> String {
    let args = event.get("toolCall").and_then(|call| call.get("args"));
    if let Some(cwd) = args.and_then(|args| args.get("Cwd")).and_then(Value::as_str).filter(|cwd| !cwd.is_empty()) {
        return cwd.to_string();
    }
    let paths = match event.get("workspacePaths") { Some(Value::Arr(items)) => items, _ => return String::new() };
    let target = args.and_then(|args| args.get("Path").or_else(|| args.get("AbsolutePath")).or_else(|| args.get("TargetFile")))
        .and_then(Value::as_str).filter(|path| Path::new(path).is_absolute());
    if let Some(target) = target {
        if let Some(best) = paths.iter().filter_map(Value::as_str).filter(|root| Path::new(target).starts_with(root)).max_by_key(|root| root.len()) {
            return best.to_string();
        }
        return Path::new(target).parent().unwrap_or_else(|| Path::new(target)).to_string_lossy().into_owned();
    }
    paths.first().and_then(Value::as_str).unwrap_or("").to_string()
}

/// agy input ⇒ snake_case input for the Claude/Codex gates.
pub fn to_snake(event: &Value, hook_event_name: &str) -> String {
    let mut o = Vec::new();
    obj_set(&mut o, "session_id", s(first_str(event, "conversationId")));
    obj_set(&mut o, "cwd", s(workspace(event)));
    obj_set(&mut o, "transcript_path", s(first_str(event, "transcriptPath")));
    obj_set(&mut o, "hook_event_name", s(hook_event_name));
    if let Some(prompt) = event.get("lastUserInput").and_then(Value::as_str) {
        obj_set(&mut o, "prompt", s(prompt));
    }
    if event.get("executionNum").and_then(Value::as_i64).is_some_and(|n| n >= 2) {
        obj_set(&mut o, "stop_hook_active", Value::Bool(true));
    }
    dumps(&Value::Obj(o))
}

/// agy PreToolUse input ⇒ ask-notify input (`tool_name`, `tool_input`, `cwd`).
pub fn ask_event(event: &Value) -> String {
    let tool_call = event.get("toolCall");
    let tool_name = tool_call
        .and_then(|tc| tc.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("ask_question");
    let tool_input = tool_call
        .and_then(|tc| tc.get("args"))
        .cloned()
        .unwrap_or(Value::Obj(vec![]));
    let mut o = Vec::new();
    obj_set(&mut o, "tool_name", s(tool_name));
    obj_set(&mut o, "tool_input", tool_input);
    obj_set(&mut o, "cwd", s(workspace(event)));
    dumps(&Value::Obj(o))
}

/// agy `run_command` ⇒ 공통 PreToolUse gate의 Bash 입력.
pub fn command_event(event: &Value) -> String {
    let args = event.get("toolCall").and_then(|call| call.get("args"));
    let command = args.and_then(|args| args.get("CommandLine")).and_then(Value::as_str).unwrap_or("");
    let cwd = args.and_then(|args| args.get("Cwd")).and_then(Value::as_str).unwrap_or("");
    let mut input = Vec::new();
    obj_set(&mut input, "command", s(command));
    let mut output = Vec::new();
    obj_set(&mut output, "tool_name", s("Bash"));
    obj_set(&mut output, "tool_input", Value::Obj(input));
    let fallback_cwd = workspace(event);
    obj_set(&mut output, "cwd", s(if cwd.is_empty() { &fallback_cwd } else { cwd }));
    obj_set(&mut output, "session_id", s(first_str(event, "conversationId")));
    let tp = first_str(event, "transcriptPath");
    if !tp.is_empty() { obj_set(&mut output, "transcript_path", s(tp)); }
    dumps(&Value::Obj(output))
}

/// agy `read_file` ⇒ 공통 PreToolUse gate의 Read 입력.
pub fn read_event(event: &Value) -> String {
    let args = event.get("toolCall").and_then(|call| call.get("args"));
    let path = args.and_then(|args| args.get("Path").or_else(|| args.get("AbsolutePath"))).and_then(Value::as_str).unwrap_or("");
    let mut input = Vec::new();
    obj_set(&mut input, "file_path", s(path));
    if let Some(end) = args.and_then(|a| a.get("EndLine")).and_then(Value::as_i64) {
        let start = args.and_then(|a| a.get("StartLine")).and_then(Value::as_i64).unwrap_or(1);
        if end >= start && start >= 1 {
            obj_set(&mut input, "limit", Value::Num((end - start + 1).to_string()));
        }
    }
    let mut output = Vec::new();
    obj_set(&mut output, "tool_name", s("Read"));
    obj_set(&mut output, "tool_input", Value::Obj(input));
    obj_set(&mut output, "cwd", s(workspace(event)));
    obj_set(&mut output, "session_id", s(first_str(event, "conversationId")));
    let tp = first_str(event, "transcriptPath");
    if !tp.is_empty() { obj_set(&mut output, "transcript_path", s(tp)); }
    dumps(&Value::Obj(output))
}

/// agy `edit_file`·`write_file` ⇒ 공통 PreToolUse gate의 Edit 입력.
pub fn edit_event(event: &Value) -> String {
    let args = event.get("toolCall").and_then(|call| call.get("args"));
    let path = args.and_then(|args| args.get("Path").or_else(|| args.get("TargetFile"))).and_then(Value::as_str).unwrap_or("");
    let mut input = Vec::new();
    obj_set(&mut input, "file_path", s(path));
    let mut output = Vec::new();
    obj_set(&mut output, "tool_name", s("Edit"));
    obj_set(&mut output, "tool_input", Value::Obj(input));
    obj_set(&mut output, "cwd", s(workspace(event)));
    obj_set(&mut output, "session_id", s(first_str(event, "conversationId")));
    let tp = first_str(event, "transcriptPath");
    if !tp.is_empty() { obj_set(&mut output, "transcript_path", s(tp)); }
    dumps(&Value::Obj(output))
}

/// 공통 gate의 Claude/Codex deny 출력 ⇒ agy deny 출력.
pub fn pre_command_output(gate_stdout: &str) -> String {
    if gate_stdout.trim().is_empty() { return pre_tool("allow", ""); }
    let Ok(value) = parse(gate_stdout.trim()) else { return pre_tool("deny", "게이트 출력이 JSON이 아니에요."); };
    let hook = value.get("hookSpecificOutput");
    if hook.and_then(|v| v.get("permissionDecision")).and_then(Value::as_str) == Some("deny") {
        let reason = hook.and_then(|v| v.get("permissionDecisionReason")).and_then(Value::as_str).unwrap_or("");
        pre_tool("deny", reason)
    } else {
        pre_tool("allow", "")
    }
}

fn inject(text: &str) -> String {
    let inner = Value::Obj(vec![("systemMessage".into(), s(text))]);
    let step = Value::Obj(vec![("systemMessage".into(), inner)]);
    dumps(&Value::Obj(vec![("injectSteps".into(), Value::Arr(vec![step]))]))
}

/// architecture-context output ⇒ agy injectSteps.
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
/// 위임 흐름 루트(`T26_WORKFLOW_ROOTS` ':' 구분, 기본 ~/extensions·~/project) 중 하나 아래인지.
pub fn in_workflow(ws: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    match std::env::var("T26_WORKFLOW_ROOTS") {
        Ok(v) if !v.trim().is_empty() => v.split(':').filter(|r| !r.is_empty()).any(|r| in_extensions(ws, r)),
        _ => in_extensions(ws, &format!("{home}/extensions")) || in_extensions(ws, &format!("{home}/project")),
    }
}

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
        Err(_) => return pre_tool("deny", "Grist 게이트 출력이 JSON이 아니에요."),
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

fn idle_window(name: &str, created: u64, now: u64, panes: &str) -> bool {
    if name.starts_with("starting-") { return now.saturating_sub(created) > 600; }
    name.starts_with("dispatch-") && !panes.lines().any(|line| line.split_once('\t').is_some_and(|(tag, dead)| !tag.trim().is_empty() && dead != "1"))
}

fn close_idle_dispatch_windows() {
    let Ok(windows) = Command::new("tmux").args(["list-windows", "-a", "-F", "#{window_id}\t#{window_name}\t#{window_activity}"]).output() else { return };
    if !windows.status.success() { return; }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    for line in String::from_utf8_lossy(&windows.stdout).lines() {
        let mut fields = line.split('\t');
        let (Some(id), Some(name), Some(created)) = (fields.next(), fields.next(), fields.next().and_then(|x| x.parse::<u64>().ok())) else { continue; };
        if id.is_empty() { continue; }
        if name.starts_with("starting-") {
            if idle_window(name, created, now, "") { let _ = Command::new("tmux").args(["kill-window", "-t", id]).output(); }
            continue;
        }
        if !name.starts_with("dispatch-") { continue; }
        let Ok(panes) = Command::new("tmux").args(["list-panes", "-t", id, "-F", "#{@dispatch-agent}\t#{pane_dead}"]).output() else { continue };
        if !panes.status.success() { continue; }
        if idle_window(name, created, now, &String::from_utf8_lossy(&panes.stdout)) {
            let _ = Command::new("tmux").args(["kill-window", "-t", id]).output();
        }
    }
}

fn run_status(name: &str, args: &[&str], stdin: &str) -> Result<(i32, String), String> {
    let mut child = Command::new(bin(name))
        .args(args)
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{name}: {e}"))?;
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(stdin.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| format!("{name}: {e}"))?;
    let code = out.status.code().unwrap_or(-1);
    Ok((code, String::from_utf8_lossy(&out.stdout).into_owned()))
}

fn run(name: &str, args: &[&str], stdin: &str) -> Result<String, String> {
    let (code, out) = run_status(name, args, stdin)?;
    if code != 0 { return Err(format!("{name}: exit {code}")); }
    Ok(out)
}

fn lifecycle_context<F>(event: &Value, hook_event_name: &str, touch: &str, mut invoke: F) -> Result<String, String>
where F: FnMut(&str, &[&str], &str) -> Result<String, String> {
    let snake = to_snake(event, hook_event_name);
    let _ = invoke("task-complete", &[touch], &snake);
    invoke("architecture-context", &[], &snake).map(|output| session_output(&output))
}

fn gemini_common(event: &Value) -> String {
    let name = first_str(event, "tool_name");
    let input = event.get("tool_input").cloned().unwrap_or(Value::Obj(vec![]));
    let mapped = match name {
        "run_shell_command" | "run_command" => "Bash",
        "read_file" | "view_file" => "Read",
        "write_file" | "replace" | "edit_file" => "Edit",
        "ask_question" | "ask_user" => "AskUserQuestion",
        "invoke_subagent" => "Agent",
        other => other,
    };
    let mut args = Vec::new();
    let command = input.get("command").or_else(|| input.get("CommandLine")).and_then(Value::as_str);
    if mapped == "Bash" { obj_set(&mut args, "command", s(command.unwrap_or(""))); }
    else if matches!(mapped, "Read" | "Edit") {
        let path = input.get("file_path").or_else(|| input.get("path")).or_else(|| input.get("Path")).and_then(Value::as_str).unwrap_or("");
        obj_set(&mut args, "file_path", s(path));
    } else {
        let mut out = Vec::new();
        for key in ["cwd", "session_id", "transcript_path"] { obj_set(&mut out, key, s(first_str(event, key))); }
        obj_set(&mut out, "tool_name", s(mapped));
        obj_set(&mut out, "tool_input", input);
        return dumps(&Value::Obj(out));
    }
    let mut out = Vec::new();
    for key in ["cwd", "session_id", "transcript_path"] {
        obj_set(&mut out, key, s(first_str(event, key)));
    }
    obj_set(&mut out, "tool_name", s(mapped));
    obj_set(&mut out, "tool_input", Value::Obj(args));
    dumps(&Value::Obj(out))
}

fn gemini_lifecycle(event: &Value, hook: &str) -> String {
    let mut out = Vec::new();
    for key in ["cwd", "session_id", "transcript_path", "prompt"] {
        obj_set(&mut out, key, s(first_str(event, key)));
    }
    obj_set(&mut out, "hook_event_name", s(hook));
    dumps(&Value::Obj(out))
}

fn manifest_action(target: &str, id: &str, event: &Value) -> Result<String, String> {
    if id == "ensure" { return run("agent-rules", &["ensure"], "").map(|_| "{}".into()); }
    let is_gemini = target == "gemini";
    let lifecycle = |hook: &str| if is_gemini { gemini_lifecycle(event, hook) } else { to_snake(event, hook) };
    match id {
        "gate" => {
            let name = if is_gemini { first_str(event, "tool_name") } else { event.get("toolCall").and_then(|x| x.get("name")).and_then(Value::as_str).unwrap_or("") };
            if name.contains("subagent") { return Ok(pre_tool("deny", "내장 subagent 호출은 dispatch-agent로 맡기세요.")); }
            let payload = if is_gemini { gemini_common(event) } else {
                if name.contains("command") { command_event(event) }
                else if name.contains("read") || name.contains("view") { read_event(event) }
                else { edit_event(event) }
            };
            let gate = pre_command_output(&run("main-edit-gate", &[], &payload)?);
            if gate.contains("\"deny\"") { return Ok(gate); }
            if name.contains("edit") || name.contains("write") || name.contains("replace") {
                let cwd = if is_gemini { first_str(event, "cwd").to_string() } else { workspace(event) };
                if in_workflow(&cwd) { return run("grist", &["gate", "--cwd", &cwd], "").map(|o| pre_edit_output(&o)); }
            }
            Ok(gate)
        }
        "ask" => {
            let payload = if is_gemini { gemini_common(event) } else { ask_event(event) };
            let (code, _) = run_status("ask-notify", &[], &payload)?;
            Ok(if code == 0 { pre_tool("allow", "") } else { pre_tool("deny", &format!("nf 알림 실패(exit {code})")) })
        }
        "architecture" | "prompt-architecture" | "post-architecture" => {
            let hook = if id == "architecture" { "SessionStart" } else if id == "prompt-architecture" { "UserPromptSubmit" } else { "PostToolUse" };
            let output = run("architecture-context", &[], &lifecycle(hook))?;
            Ok(if is_gemini { output } else { session_output(&output) })
        }
        "snapshot" | "touch" => {
            let (arg, hook) = if id == "snapshot" { ("--session-start", "SessionStart") } else { ("--session-touch", "UserPromptSubmit") };
            run("task-complete", &[arg], &lifecycle(hook)).map(|_| "{}".into())
        }
        "grist-sync" => {
            let cwd = if is_gemini { first_str(event, "cwd").to_string() } else { workspace(event) };
            if !cwd.is_empty() { let _ = run("grist", &["sync", "--cwd", &cwd], ""); }
            Ok("{}".into())
        }
        "initialization" | "completion" | "stop-notify" => {
            if !is_gemini {
                close_idle_dispatch_windows();
                let reason = first_str(event, "terminationReason");
                if reason == "USER_CANCELED" || reason == "ERROR" || event.get("fullyIdle") == Some(&Value::Bool(false)) {
                    return Ok("{}".into());
                }
            }
            let name = if id == "initialization" { "initialization-gate" } else if id == "completion" { "task-complete" } else { "ask-notify" };
            let output = run(name, &[], &lifecycle("Stop"))?;
            if let Some(block) = stop_output(&output) {
                if is_gemini {
                    let reason = parse(&block).ok().and_then(|v| v.get("reason").and_then(Value::as_str).map(str::to_owned)).unwrap_or_default();
                    Ok(pre_tool("deny", &reason))
                } else { Ok(block) }
            } else { Ok("{}".into()) }
        }
        _ => Err(format!("unknown manifest action {id}")),
    }
}

fn main() {
    let sub = std::env::args().nth(1).unwrap_or_default();
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let event = parse(&input).unwrap_or(Value::Obj(vec![]));
    let out = match sub.as_str() {
        "manifest" => {
            let target = std::env::args().nth(2).unwrap_or_default();
            let id = std::env::args().nth(3).unwrap_or_default();
            manifest_action(&target, &id, &event)
        }
        "session-start" => lifecycle_context(&event, "SessionStart", "--session-start", run),
        "pre-invocation" => lifecycle_context(&event, "UserPromptSubmit", "--session-touch", run),
        "post-command" => {
            Ok("{}".into())
        }
        "pre-edit" => run("main-edit-gate", &[], &edit_event(&event))
            .map(|output| pre_command_output(&output))
            .and_then(|gate| {
                if gate.contains(r#""decision": "deny""#) {
                    Ok(gate)
                } else {
                    let cwd = workspace(&event);
                    if cwd.is_empty() || !in_workflow(&cwd) {
                        Ok(pre_tool("allow", ""))
                    } else {
                        run("grist", &["gate", "--cwd", &cwd], "").map(|o| pre_edit_output(&o))
                    }
                }
            }),
        "pre-read" => run("main-edit-gate", &[], &read_event(&event))
            .map(|output| pre_command_output(&output)),
        "pre-subagent" => {
            let cwd = workspace(&event);
            if in_workflow(&cwd) {
                Ok(pre_tool("deny", "Antigravity 내장 invoke_subagent는 감시·타임아웃·T26_ROLE 게이트가 적용되지 않아 토큰을 과소모합니다. ~/ai/scripts/bin/dispatch-agent를 사용하세요."))
            } else {
                Ok(pre_tool("allow", ""))
            }
        }
        "pre-ask" => {
            let payload = ask_event(&event);
            match run_status("ask-notify", &[], &payload) {
                Ok((0, _)) => Ok(pre_tool("allow", "")),
                Ok((code, _)) => Ok(pre_tool(
                    "deny",
                    &format!("nf 알림 전송 실패(exit {code}): 질문 전에 nf가 동작하도록 고친 뒤 다시 질문하세요."),
                )),
                Err(e) => Ok(pre_tool("deny", &format!("nf 알림 실행 실패({e})"))),
            }
        }
        "pre-command" => run("main-edit-gate", &[], &command_event(&event))
            .map(|output| pre_command_output(&output)),
        "stop" => {
            close_idle_dispatch_windows();
            let reason = first_str(&event, "terminationReason");
            if reason == "USER_CANCELED" || reason == "ERROR" || event.get("fullyIdle") == Some(&Value::Bool(false)) {
                let cwd = workspace(&event);
                if !cwd.is_empty() { let _ = run("grist", &["sync", "--cwd", &cwd], ""); }
                Ok("{}".into())
            } else {
                (|| -> Result<String, String> {
                    let snake = to_snake(&event, "Stop");
                    let initialization = run("initialization-gate", &[], &snake)?;
                    if let Some(block) = stop_output(&initialization) { return Ok(block); }
                    let completion = run("task-complete", &[], &snake)?;
                    if let Some(block) = stop_output(&completion) { return Ok(block); }
                    let _ = run("ask-notify", &[], &snake);
                    Ok("{}".into())
                })()
            }
        }
        other => Err(format!("unknown subcommand {other}")),
    };
    match out {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("agy-hook: {e}");
            if sub == "manifest" {
                let id = std::env::args().nth(3).unwrap_or_default();
                if matches!(id.as_str(), "gate" | "ask") {
                    println!("{}", pre_tool("deny", "게이트 실행에 실패했어요."));
                } else {
                    std::process::exit(2);
                }
            } else if sub == "pre-edit" || sub == "pre-read" || sub == "pre-subagent" || sub == "pre-ask" || sub == "pre-command" {
                println!("{}", pre_tool("deny", "게이트 실행에 실패했어요."));
            } else if sub == "stop" {
                println!("{}", pre_tool("continue", "종료 게이트 실행에 실패했어요."));
            } else {
                println!("{{}}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gemini_and_agy_write_inputs_reach_same_common_gate_shape() {
        let gemini = super::parse(r#"{"session_id":"s","cwd":"/home/tree/extensions","tool_name":"write_file","tool_input":{"file_path":"/home/tree/.claude/settings.json"}}"#).unwrap();
        let agy = super::parse(r#"{"conversationId":"s","workspacePaths":["/home/tree/extensions"],"toolCall":{"name":"write_file","args":{"Path":"/home/tree/.claude/settings.json"}}}"#).unwrap();
        let g = super::parse(&super::gemini_common(&gemini)).unwrap();
        let a = super::parse(&super::edit_event(&agy)).unwrap();
        assert_eq!(g.get("tool_name").and_then(super::Value::as_str), Some("Edit"));
        assert_eq!(g.get("tool_input"), a.get("tool_input"));
        assert_eq!(g.get("session_id").and_then(super::Value::as_str), Some("s"));
        assert_eq!(a.get("session_id").and_then(super::Value::as_str), Some("s"));
        let lifecycle = super::parse(&super::gemini_lifecycle(&gemini, "Stop")).unwrap();
        assert_eq!(lifecycle.get("hook_event_name").and_then(super::Value::as_str), Some("Stop"));
    }
    #[test]
    fn sweep_closes_dead_tag_and_old_starting_window() {
        assert!(super::idle_window("dispatch-1", 1, 1000, "1\t1\n"));
        assert!(!super::idle_window("dispatch-1", 1, 1000, "1\t0\n"));
        assert!(super::idle_window("starting-1", 100, 701, ""));
        assert!(!super::idle_window("starting-1", 100, 700, ""));
    }
    use super::*;

    #[test]
    fn session_output_wraps_additional_context() {
        let o = session_output(r#"{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"구조 a"}}"#);
        assert_eq!(o, r#"{"injectSteps": [{"systemMessage": {"systemMessage": "구조 a"}}]}"#);
    }

    #[test]
    fn lifecycle_context_injects_even_when_session_record_fails() {
        let event = parse(r#"{"conversationId":"agy","workspacePaths":["/home/tree/extensions"],"lastUserInput":"구조 확인"}"#).unwrap();
        for (hook, touch) in [("SessionStart", "--session-start"), ("UserPromptSubmit", "--session-touch")] {
            let mut called = Vec::new();
            let output = lifecycle_context(&event, hook, touch, |name, args, input| {
                called.push((name.to_string(), args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>()));
                let mapped = parse(input).unwrap();
                assert_eq!(mapped.get("hook_event_name").and_then(Value::as_str), Some(hook));
                if name == "task-complete" { Err("exit 1".into()) }
                else { Ok(format!(r#"{{"hookSpecificOutput":{{"additionalContext":"{hook} 구조"}}}}"#)) }
            }).unwrap();
            assert!(output.contains("injectSteps"));
            assert_eq!(called, vec![("task-complete".into(), vec![touch.into()]), ("architecture-context".into(), vec![])]);
        }
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
        assert!(pre_edit_output("garbage").contains(r#""decision": "deny""#));
    }

    #[test]
    fn stop_block_becomes_continue() {
        assert_eq!(
            stop_output(r#"{"decision":"block","reason":"r"}"#).unwrap(),
            r#"{"decision": "continue", "reason": "r"}"#
        );
        assert!(stop_output("{}").is_none());
    }

    #[test]
    fn ask_event_maps_tool_call() {
        let e = parse(r#"{"toolCall":{"name":"ask_question","args":{"questions":[{"question":"타입 선택"}]}},"workspacePaths":["/home/tree/extensions"]}"#).unwrap();
        assert_eq!(
            ask_event(&e),
            r#"{"tool_name": "ask_question", "tool_input": {"questions": [{"question": "타입 선택"}]}, "cwd": "/home/tree/extensions"}"#
        );
    }

    #[test]
    fn command_event_and_deny_output() {
        let event = parse(r#"{"toolCall":{"name":"run_command","args":{"CommandLine":"nf message","Cwd":"/home/tree/extensions"}},"conversationId":"c1","workspacePaths":["/home/tree/extensions"]}"#).unwrap();
        let converted = parse(&command_event(&event)).unwrap();
        assert_eq!(converted.get("tool_name").and_then(Value::as_str), Some("Bash"));
        assert_eq!(converted.get("tool_input").and_then(|v| v.get("command")).and_then(Value::as_str), Some("nf message"));
        assert_eq!(converted.get("cwd").and_then(Value::as_str), Some("/home/tree/extensions"));
        let denial = r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"nf 차단"}}"#;
        assert_eq!(pre_command_output(denial), r#"{"decision": "deny", "reason": "nf 차단"}"#);
        assert_eq!(pre_command_output(""), r#"{"decision": "allow"}"#);
    }

    #[test]
    fn read_and_edit_events_map_path_and_line_limit() {
        let read = parse(r#"{"toolCall":{"name":"read_file","args":{"Path":"/home/tree/extensions/app/src/a.ts","StartLine":10,"EndLine":25}},"conversationId":"c1","workspacePaths":["/home/tree/extensions/app"]}"#).unwrap();
        let converted = parse(&read_event(&read)).unwrap();
        assert_eq!(converted.get("tool_name").and_then(Value::as_str), Some("Read"));
        assert_eq!(converted.get("tool_input").and_then(|v| v.get("file_path")).and_then(Value::as_str), Some("/home/tree/extensions/app/src/a.ts"));
        assert_eq!(converted.get("tool_input").and_then(|v| v.get("limit")).and_then(Value::as_i64), Some(16));
        let edit = parse(r#"{"toolCall":{"name":"edit_file","args":{"Path":"/home/tree/extensions/app/src/a.ts"}},"conversationId":"c1","workspacePaths":["/home/tree/extensions/app"]}"#).unwrap();
        let converted_edit = parse(&edit_event(&edit)).unwrap();
        assert_eq!(converted_edit.get("tool_name").and_then(Value::as_str), Some("Edit"));
        assert_eq!(converted_edit.get("tool_input").and_then(|v| v.get("file_path")).and_then(Value::as_str), Some("/home/tree/extensions/app/src/a.ts"));
    }

    #[test]
    fn agy_invocation_and_tool_variants_keep_gate_inputs() {
        let event = parse(r#"{"conversationId":"agy-1","workspacePaths":["/home/tree/extensions"],"lastUserInput":"수정해 주세요","executionNum":2}"#).unwrap();
        let mapped = parse(&to_snake(&event, "UserPromptSubmit")).unwrap();
        assert_eq!(mapped.get("prompt").and_then(Value::as_str), Some("수정해 주세요"));
        assert_eq!(mapped.get("stop_hook_active"), Some(&Value::Bool(true)));

        let view = parse(r#"{"conversationId":"agy-1","workspacePaths":["/home/tree/extensions"],"toolCall":{"name":"view_file","args":{"AbsolutePath":"/home/tree/extensions/app/src/a.ts","EndLine":20}}}"#).unwrap();
        let read = parse(&read_event(&view)).unwrap();
        assert_eq!(read.get("tool_input").and_then(|v| v.get("file_path")).and_then(Value::as_str), Some("/home/tree/extensions/app/src/a.ts"));
        assert_eq!(read.get("tool_input").and_then(|v| v.get("limit")).and_then(Value::as_i64), Some(20));

        let edit = parse(r#"{"toolCall":{"name":"replace_file_content","args":{"TargetFile":"/home/tree/extensions/app/src/a.ts"}},"workspacePaths":["/home/tree/extensions"]}"#).unwrap();
        let mapped = parse(&edit_event(&edit)).unwrap();
        assert_eq!(mapped.get("tool_input").and_then(|v| v.get("file_path")).and_then(Value::as_str), Some("/home/tree/extensions/app/src/a.ts"));

        let multi = parse(r#"{"workspacePaths":["/tmp/other","/home/tree/extensions/app"],"toolCall":{"name":"view_file","args":{"AbsolutePath":"/home/tree/extensions/app/src/a.ts"}}}"#).unwrap();
        assert_eq!(workspace(&multi), "/home/tree/extensions/app");
    }
}
