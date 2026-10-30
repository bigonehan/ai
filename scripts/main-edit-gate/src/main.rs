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

fn target(event: &Value) -> Option<PathBuf> {
    let name = event.get("tool_name")?.as_str()?;
    if !matches!(name, "Edit" | "Write" | "MultiEdit" | "NotebookEdit") { return None; }
    let input = event.get("tool_input")?;
    let file = input.get(if name == "NotebookEdit" { "notebook_path" } else { "file_path" })?.as_str()?;
    let cwd = Path::new(event.get("cwd")?.as_str()?);
    let file = normalize(&if Path::new(file).is_absolute() { PathBuf::from(file) } else { cwd.join(file) });
    let name = file.file_name()?.to_str()?;
    if matches!(name, "Cargo.toml" | "package.json" | "manifest.json" | "runtime-contract.js") {
        return None;
    }
    if file.starts_with(Path::new(AI).join("scripts")) { return Some(file); }
    if !file.starts_with(EXTENSIONS) { return None; }
    if name == "AGENTS.md" || file.extension().and_then(|ext| ext.to_str()) == Some("md")
        || file.components().any(|c| c.as_os_str() == ".ai") { return None; }
    Some(file)
}

/// 명령 구간의 래퍼와 환경 변수 지정 뒤에 오는 실행 단어부터 돌려준다.
fn execution_words(segment: &str) -> Vec<&str> {
    let words: Vec<&str> = segment.split_whitespace().filter(|w| !w.contains('=')).collect();
    let mut start = 0;
    while let Some(w) = words.get(start) {
        match *w {
            "env" | "nohup" => start += 1,
            "timeout" => start += 2,
            _ => break,
        }
    }
    words[start.min(words.len())..].to_vec()
}

fn direct_nf(event: &Value) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return false; }
    let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
    segments(&without_heredoc_bodies(command)).iter().any(|segment| {
        execution_words(segment).first().is_some_and(|first| *first == "nf" || first.ends_with("/nf"))
    })
}

fn in_extensions(path: &Path) -> bool {
    normalize(path).starts_with(EXTENSIONS)
}

fn structure_search(event: &Value) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return false; }
    let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
    let mut cwd = PathBuf::from(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    segments(&without_heredoc_bodies(command)).iter().any(|segment| {
        let words = execution_words(segment);
        if words.first() == Some(&"cd") {
            if let Some(path) = words.get(1) {
                let path = path.trim_matches(['\'', '"']);
                cwd = normalize(&if Path::new(path).is_absolute() { PathBuf::from(path) } else { cwd.join(path) });
            }
            return false;
        }
        let Some(first) = words.first() else { return false };
        let program = first.rsplit('/').next().unwrap_or(first);
        let recursive = program == "find" || (program == "ls" && words[1..].iter().any(|word| {
            word.starts_with('-') && (word == &"--recursive" || word.trim_start_matches('-').contains('R'))
        }));
        if !recursive { return false; }
        in_extensions(&cwd) || words[1..].iter().any(|word| {
            let path = word.trim_matches(['\'', '"']);
            Path::new(path).is_absolute() && in_extensions(Path::new(path))
        })
    })
}

fn pre_worker_denial(event: &Value) -> Option<&'static str> {
    if direct_nf(event) { return Some("nf 알림은 ask-notify hook이 자동 전송합니다. nf를 직접 실행하지 마세요."); }
    if structure_search(event) { return Some("extensions 구조는 read-architecture 출력의 .ai/docs/architecture.md로 고르세요. ls -R·find로 다시 찾지 않습니다."); }
    None
}

fn role_exempt(role: Option<&str>) -> bool {
    role == Some("verifier")
}

/// Here-doc 본문은 셸 명령이 아니므로 실행 단어 판정에서 제외한다.
fn without_heredoc_bodies(command: &str) -> String {
    let mut result = String::new();
    let mut delimiter: Option<String> = None;
    for line in command.lines() {
        if let Some(end) = &delimiter {
            if line.trim() == end { delimiter = None; }
            result.push('\n');
            continue;
        }
        result.push_str(line);
        result.push('\n');
        delimiter = line.split_whitespace().find_map(|word| {
            let marker = word.strip_prefix("<<-").or_else(|| word.strip_prefix("<<"))?;
            let marker = marker.trim_matches(['\'', '"']);
            (!marker.is_empty()).then(|| marker.to_string())
        });
    }
    result
}

fn main_heavy_work(event: &Value) -> bool {
    let input = event.get("tool_input");
    let text = match event.get("tool_name").and_then(Value::as_str) {
        Some("Read") => input.and_then(|i| i.get("file_path")),
        Some("Bash") => input.and_then(|i| i.get("command")),
        _ => None,
    };
    let Some(text) = text.and_then(Value::as_str) else { return false };
    if event.get("tool_name").and_then(Value::as_str) == Some("Read") { return text.contains(".ai/dispatch/") && text.ends_with(".log"); }
    segments(text).iter().any(|segment| {
        let words = execution_words(segment);
        let Some(first) = words.first() else { return false };
        matches!(*first, "tail" | "cat" | "head" | "rg" | "less" | "sed" | "awk")
            && words[1..].iter().any(|w| w.contains(".ai/dispatch/") && w.ends_with(".log"))
    }) || browser_work(event)
}

fn browser_work(event: &Value) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return false; }
    let Some(text) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
    // 문자열 속 언급이 아니라 각 명령 구간의 실행 단어만 본다.
    let scratch_dir = |w: &str| w.starts_with("$S") || w.contains("/tmp/") || w.contains("scratchpad");
    // 스크래치패드로 cd한 뒤의 상대경로 스크립트도 스크래치 실행으로 본다.
    let mut in_scratch = event.get("cwd").and_then(Value::as_str).is_some_and(scratch_dir);
    segments(text).iter().any(|segment| {
        let words = execution_words(segment);
        if words.first() == Some(&"cd") {
            in_scratch = words.get(1).is_some_and(|w| scratch_dir(w));
            return false;
        }
        let Some(first) = words.first() else { return false };
        let browser = ["9222", "stagehand", "Stagehand", "connectOverCDP", "Runtime.evaluate"].iter().any(|k| segment.contains(k));
        let scratch_script = words.get(1).is_some_and(|w| scratch_dir(w) || (in_scratch && !w.starts_with(['/', '~', '-'])));
        *first == "run_cdp"
            || (*first == "node" && (browser || scratch_script))
            || (*first == "curl" && words[1..].iter().any(|w| w.contains(":9222")))
    })
}

const WORKER_BROWSER_REASON: &str = "CDP·stagehand 실행과 런타임 검증은 검증자(dispatch-agent --role verifier)가 합니다. 대행자는 유닛테스트까지만 하고 task-submit 하세요.";
const MAIN_HEAVY_REASON: &str = "메인은 판단·지시만 합니다. CDP·stagehand 실행은 검증자(--role verifier)에게, 위임 로그 읽기는 구현자에게 맡기고, dispatch-agent의 exit·last 한 줄과 grist task-show --section 검증으로 판정하고, 원인은 --resume으로 구현자에게 한 줄로 받으세요.";

fn role_denial(event: &Value, role: Option<&str>) -> Option<&'static str> {
    if let Some(reason) = pre_worker_denial(event) { return Some(reason); }
    if role_exempt(role) { return None; }
    if role == Some("worker") { return browser_work(event).then_some(WORKER_BROWSER_REASON); }
    main_heavy_work(event).then_some(MAIN_HEAVY_REASON)
}

/// 따옴표 밖의 `; & | ( 줄바꿈`에서만 명령 구간을 나눈다.
fn segments(text: &str) -> Vec<String> {
    let (mut out, mut cur, mut quote) = (Vec::new(), String::new(), None);
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => { quote = None; cur.push(c); }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => { quote = Some(c); cur.push(c); }
            (None, ';' | '&' | '|' | '\n' | '(') => out.push(std::mem::take(&mut cur)),
            (None, c) => cur.push(c),
        }
    }
    out.push(cur);
    out
}

const IMAGE_LIMIT: usize = 2;

/// 메인 세션의 이미지 Read를 세션당 IMAGE_LIMIT장까지만 허용한다.
fn image_read_denied(event: &Value, state_dir: &Path) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Read") { return false; }
    let Some(file) = event.pointer("/tool_input/file_path").and_then(Value::as_str) else { return false };
    let ext = file.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif") { return false; }
    let Some(session) = event.get("session_id").and_then(Value::as_str) else { return false };
    if session.is_empty() || !session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return false; }
    let state = state_dir.join(format!("images-{session}.json"));
    let mut seen: BTreeSet<String> = std::fs::read(&state).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if seen.contains(file) { return false; }
    if seen.len() >= IMAGE_LIMIT { return true; }
    seen.insert(file.to_string());
    let _ = std::fs::create_dir_all(state_dir);
    let _ = std::fs::write(&state, serde_json::to_vec(&seen).unwrap_or_default());
    false
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).map_err(|e| e.to_string())?;
    let event: Value = serde_json::from_str(&input).map_err(|e| e.to_string())?;
    if let Some(reason) = role_denial(&event, std::env::var("T26_ROLE").ok().as_deref()) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if matches!(std::env::var("T26_ROLE").ok().as_deref(), Some("worker" | "verifier")) { return Ok(()); }
    let state_dir = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state/main-edit-gate");
    if image_read_denied(&event, &state_dir) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":format!("메인 세션의 이미지 읽기는 세션당 {IMAGE_LIMIT}장까지입니다. 화면 판정은 dispatch-agent 구현자에게 실측 조건(값·크기·naturalWidth)으로 맡기고 exit·last만 보세요.")}}));
        return Ok(());
    }
    if target(&event).is_some() {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"코드 구현은 대행자(dispatch-agent)가 합니다. 메인은 규칙·Skill 문서만 직접 고칩니다."}}));
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
    fn first_code_edit_denied_and_documents_allowed() {
        for tool in ["Edit", "Write", "MultiEdit", "NotebookEdit"] {
            let event = |file: &str| json!({
                "tool_name":tool,
                "tool_input": {if tool == "NotebookEdit" { "notebook_path" } else { "file_path" }:file},
                "cwd":EXTENSIONS
            });
            for file in ["app/src/main.rs", "root.json", "app/package-lock.json", "app/runtime-contract.test.js", "/home/tree/ai/scripts/grist/src/main.rs", "app/test/tmp/probe.rs"] {
                assert!(target(&event(file)).is_some(), "{tool}: {file}");
            }
            for file in ["app/.ai/gate/x.json", "app/AGENTS.md", "app/readme.md", "app/Cargo.toml", "app/package.json", "app/manifest.json", "app/public/manifest.json", "app/runtime-contract.js", "/home/tree/ai/scripts/grist/Cargo.toml", "/home/tree/ai/skills/role-agent/SKILL.md", "/home/tree/ai/rules/config.json", "/tmp/file.rs"] {
                assert!(target(&event(file)).is_none(), "{tool}: {file}");
            }
        }
    }

    #[test]
    fn dispatch_log_read_denied() {
        let bash = json!({"tool_name":"Bash","tool_input":{"command":"tail -n 30 .ai/dispatch/471.log"}});
        let read = json!({"tool_name":"Read","tool_input":{"file_path":"/home/tree/extensions/x/.ai/dispatch/471.log"}});
        let last = json!({"tool_name":"Bash","tool_input":{"command":"cat .ai/dispatch/471.last"}});
        assert!(main_heavy_work(&bash));
        assert!(main_heavy_work(&read));
        assert!(!main_heavy_work(&last));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"run_cdp --replace -e dist"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"node ~/ai/skills/access-stagehand/scripts/stagehand.mjs"}})));
        assert!(!main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"npm test"}})));
        assert!(!main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"agent-rules replace --new '메인은 run_cdp·stagehand.mjs·.ai/dispatch/1.log를 하지 않는다'"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"cd x; T=1 run_cdp --url u"}})));
    }

    #[test]
    fn direct_nf_checks_execution_words_only() {
        let bash = |command: &str| json!({"tool_name":"Bash","tool_input":{"command":command}});
        for command in ["nf -m x", "~/.local/bin/nf x", "env A=1 nf x", "echo a && nf x"] {
            assert!(direct_nf(&bash(command)), "{command}");
        }
        for command in ["rg -n nf src", "agent-rules remove --text 'nf ...'", "nfx"] {
            assert!(!direct_nf(&bash(command)), "{command}");
        }
        assert!(!direct_nf(&json!({"tool_name":"Read","tool_input":{"file_path":"/tmp/nf"}})));
        assert!(!direct_nf(&bash("cat <<'EOF'\nnf --message mentioned in documentation\nEOF")));
        assert!(direct_nf(&bash("cat <<EOF\nnf is documentation\nEOF\nnf real")));
    }

    #[test]
    fn extensions_recursive_structure_search_is_denied_before_worker_return() {
        let bash = |cwd: &str, command: &str| json!({"tool_name":"Bash","cwd":cwd,"tool_input":{"command":command}});
        for command in ["find . -name x", "ls -R", "ls -laR", "env A=1 ls -R", "cd /tmp; find /home/tree/extensions -name x"] {
            assert!(structure_search(&bash(EXTENSIONS, command)), "{command}");
            assert_eq!(pre_worker_denial(&bash(EXTENSIONS, command)), Some("extensions 구조는 read-architecture 출력의 .ai/docs/architecture.md로 고르세요. ls -R·find로 다시 찾지 않습니다."));
        }
        for command in ["ls", "rg --files", "cat <<EOF\nfind . -name x\nEOF"] {
            assert!(!structure_search(&bash(EXTENSIONS, command)), "{command}");
        }
        assert!(!structure_search(&bash("/tmp", "find . -name x")));
        assert!(!structure_search(&bash(EXTENSIONS, "cd /tmp; find . -name x")));
        // run() checks pre_worker_denial before its T26_ROLE=worker return.
        assert!(pre_worker_denial(&bash(EXTENSIONS, "find . -name x")).is_some());
    }

    #[test]
    fn verifier_keeps_nf_denial_before_main_exemption() {
        let event = json!({"tool_name":"Bash","tool_input":{"command":"nf -m x"}});
        assert!(pre_worker_denial(&event).is_some());
        let stagehand = json!({"tool_name":"Bash","tool_input":{"command":"node ~/ai/skills/access-stagehand/scripts/stagehand.mjs"}});
        assert!(pre_worker_denial(&stagehand).is_none());
        assert!(main_heavy_work(&stagehand));
        assert!(role_exempt(Some("verifier")));
        assert!(!role_exempt(Some("worker")));
        assert!(!role_exempt(None));
    }

    #[test]
    fn browser_commands_are_denied_for_worker_and_main_but_allowed_for_verifier() {
        let bash = |command: &str| json!({"tool_name":"Bash","tool_input":{"command":command}});
        for command in [
            "run_cdp --replace -e dist",
            "node ~/ai/skills/access-stagehand/scripts/stagehand.mjs",
            "curl http://127.0.0.1:9222/json",
            "node -e 'connectOverCDP()'",
            "node -e 'Runtime.evaluate()'",
        ] {
            let event = bash(command);
            assert!(browser_work(&event), "{command}");
            assert_eq!(role_denial(&event, Some("worker")), Some(WORKER_BROWSER_REASON), "{command}");
            assert_eq!(role_denial(&event, Some("verifier")), None, "{command}");
            assert_eq!(role_denial(&event, None), Some(MAIN_HEAVY_REASON), "{command}");
        }
    }

    #[test]
    fn worker_can_read_dispatch_log_and_run_other_bash_commands() {
        for command in ["cat .ai/dispatch/530.log", "cargo test", "rg -n 'stagehand|:9222' src/main.rs"] {
            let event = json!({"tool_name":"Bash","tool_input":{"command":command}});
            assert!(!browser_work(&event), "{command}");
            assert_eq!(role_denial(&event, Some("worker")), None, "{command}");
        }
    }

    #[test]
    fn scratch_browser_script_denied_and_quoted_pattern_allowed() {
        let bash = |c: &str| json!({"tool_name":"Bash","tool_input":{"command":c}});
        assert!(main_heavy_work(&bash("S=/tmp/x/scratchpad; cd ~; STAGEHAND_STEP_LOG=1 timeout 110s node $S/bt.mjs $S > $S/bt.log")));
        assert!(main_heavy_work(&bash("node -e 'const ws = new WebSocket(\"ws://127.0.0.1:9222\")'")));
        assert!(!main_heavy_work(&bash("rg -n \"stagehand|cdp|run_cdp\" src/main.rs")));
        assert!(!main_heavy_work(&bash("node scripts/build.mjs")));
        assert!(main_heavy_work(&bash("cd $S && node cdp.mjs > out.log")));
        assert!(main_heavy_work(&bash("cd /tmp/claude-1000/x/scratchpad; timeout 60 node sw.mjs")));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","cwd":"/tmp/claude-1000/x/scratchpad","tool_input":{"command":"node cdp.mjs"}})));
        assert!(!main_heavy_work(&bash("cd $S; cd /home/tree/extensions/app && node scripts/build.mjs")));
    }

    #[test]
    fn third_image_read_denied_and_repeat_allowed() {
        let dir = std::env::temp_dir().join(format!("main-edit-gate-test-{}", std::process::id()));
        let read = |f: &str| json!({"tool_name":"Read","tool_input":{"file_path":f},"session_id":"img1"});
        assert!(!image_read_denied(&read("/a.png"), &dir));
        assert!(!image_read_denied(&read("/b.jpg"), &dir));
        assert!(!image_read_denied(&read("/a.png"), &dir));
        assert!(image_read_denied(&read("/c.png"), &dir));
        assert!(!image_read_denied(&read("/c.rs"), &dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn worker_and_outside_scope() {
        let event = |file: &str| json!({"tool_name":"Edit","tool_input":{"file_path":file},"cwd":"/home/tree/extensions","session_id":"abc"});
        assert!(target(&event("/tmp/file.rs")).is_none());
        assert!(target(&event("/home/tree/extensions/app/.ai/x")).is_none());
        assert!(target(&event("/home/tree/extensions/app/AGENTS.md")).is_none());
        assert_eq!(target(&event("/home/tree/ai/scripts/a.rs")).unwrap(), Path::new("/home/tree/ai/scripts/a.rs"));
    }

    #[test]
    fn normalized_paths_keep_code_boundary() {
        let event = |file: &str| json!({
            "tool_name": "Edit",
            "tool_input": { "file_path": file },
            "cwd": "/home/tree/extensions",
            "session_id": "hidden_375"
        });
        let file = target(&event("subproject/src/../src/third.rs")).unwrap();
        assert_eq!(file, Path::new("/home/tree/extensions/subproject/src/third.rs"));
        assert_eq!(target(&event("root.rs")).unwrap(), Path::new("/home/tree/extensions/root.rs"));
        assert_eq!(target(&event("/home/tree/ai/scripts/main-edit-gate/src/main.rs")).unwrap(), Path::new("/home/tree/ai/scripts/main-edit-gate/src/main.rs"));
    }
}
