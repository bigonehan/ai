#[path = "../../prompt-decision.rs"]
mod prompt_decision;
use prompt_decision::{decision, decision_at, Label};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

const EXTENSIONS: &str = "/home/tree/extensions";

/// 위임 흐름이 적용되는 루트들. `T26_WORKFLOW_ROOTS`(':' 구분)로 바꿀 수 있다.
fn workflow_roots() -> Vec<PathBuf> {
    match std::env::var("T26_WORKFLOW_ROOTS") {
        Ok(v) if !v.trim().is_empty() => v.split(':').filter(|r| !r.is_empty()).map(PathBuf::from).collect(),
        _ => vec![PathBuf::from(EXTENSIONS), PathBuf::from("/home/tree/project")],
    }
}

fn workflow_relative(path: &Path) -> Option<&Path> {
    workflow_roots().iter().find_map(|root| path.strip_prefix(root).ok())
}

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
    if matches!(name, "Cargo.toml" | "package.json" | "manifest.json" | "blender_manifest.toml" | "runtime-contract.js" | "tauri.conf.json") {
        return None;
    }
    workflow_relative(&file)?;
    if file.starts_with("/home/tree/extensions/manage_decision") { return None; }
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

fn direct_blender(event: &Value) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return false; }
    let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
    segments(&without_heredoc_bodies(command)).iter().any(|segment| {
        let words = execution_words(segment);
        let Some(first) = words.first() else { return false };
        let end = if let Some(q) = first.chars().next().filter(|c| matches!(c, '\'' | '"')) {
            words.iter().position(|w| w.trim_end_matches('\\').ends_with(q)).map_or(1, |i| i + 1)
        } else {
            1
        };
        let exe = words[..end].join(" ");
        let clean = exe.trim_matches(['\'', '"', '\\']).to_ascii_lowercase();
        clean == "blender.exe" || clean.ends_with("/blender.exe") || clean.ends_with("\\blender.exe")
    })
}

fn in_extensions(path: &Path) -> bool {
    workflow_relative(&normalize(path)).is_some()
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

fn off_project_search(event: &Value) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return false; }
    let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
    let cwd = normalize(Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/")));
    if !in_extensions(&cwd) { return false; }
    let subproject = cwd.strip_prefix(EXTENSIONS).ok()
        .and_then(|rel| rel.components().next())
        .and_then(|c| c.as_os_str().to_str())
        .filter(|name| !name.starts_with('.'));
    let bodyless = without_heredoc_bodies(command);
    if bodyless.contains("AppData/Local/Google/Chrome")
        || bodyless.contains("/Users/tende/Downloads")
        || bodyless.contains("/users/tende/Downloads")
    {
        return true;
    }
    segments(&bodyless).iter().any(|segment| {
        let words = execution_words(segment);
        let Some(first) = words.first() else { return false };
        let program = first.rsplit('/').next().unwrap_or(first);
        if !matches!(program, "rg" | "grep" | "ls" | "cat" | "head" | "tail" | "python" | "python3" | "sqlite3") {
            return false;
        }
        words[1..].iter().any(|word| {
            let path = word.trim_matches(['\'', '"']).trim_end_matches('/');
            if matches!(path, "/mnt/d" | "/mnt/e" | "/mnt/f" | "/mnt/g" | "/mnt/h" | "/mnt/h/code" | "/home/tree/extensions") {
                return true;
            }
            if let Some(proj) = subproject {
                if let Some(rest) = path.strip_prefix("/mnt/h/code/") {
                    let target_proj = rest.split('/').next().unwrap_or("");
                    if !target_proj.is_empty() && target_proj != proj {
                        return true;
                    }
                }
            }
            false
        })
    })
}

fn pre_worker_denial(event: &Value) -> Option<&'static str> {
    if direct_nf(event) { return Some("nf 알림은 ask-notify hook이 자동 전송합니다. nf를 직접 실행하지 마세요."); }
    if direct_blender(event) { return Some("blender.exe를 직접 실행하면 세션이 닫히지 않고 멈춥니다. ~/ai/scripts/bin/win-smoke --project <dir> --script <py>로 실행하세요."); }
    if structure_search(event) { return Some("extensions 구조는 read-architecture 출력의 .ai/docs/architecture.md로 고르세요. ls -R·find로 다시 찾지 않습니다."); }
    if off_project_search(event) { return Some("조회 경로가 현재 프로젝트의 허용 범위 밖입니다. 현재 프로젝트 AGENTS.md의 `runtime :` 두 번째 경로를 읽고, 방금 거부된 명령의 배포 경로를 그 경로로 바꿔 한 번만 다시 실행하세요. Windows Chrome 프로필·Windows Downloads는 조회하지 마세요(공용 Chromium 다운로드는 ~/Downloads). 경로를 바꿀 필요가 없다면 이 조회를 건너뛰고 run-cdp 흐름 검증을 계속하세요."); }
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

fn heavy_log_path(s: &str) -> bool {
    let path = s.trim_matches(['\'', '"', '(', ')', ';']);
    (path.contains(".ai/dispatch/") && path.ends_with(".log"))
        || (path.contains("/background/task-") && path.ends_with(".log"))
        || path.ends_with("transcript.jsonl")
        || path.ends_with("transcript_full.jsonl")
        || [".gemini/antigravity-cli/brain", ".codex/sessions", ".claude/projects"]
            .iter().any(|root| path.contains(root)
                && ((path.ends_with(root) || path.ends_with('/')) && !path.contains("/memory/")
                    || path.ends_with(".log") || path.ends_with(".jsonl")))
}

fn main_heavy_work(event: &Value) -> bool {
    let input = event.get("tool_input");
    let text = match event.get("tool_name").and_then(Value::as_str) {
        Some("Read") => input.and_then(|i| i.get("file_path")),
        Some("Bash") => input.and_then(|i| i.get("command")),
        _ => None,
    };
    let Some(text) = text.and_then(Value::as_str) else { return false };
    if event.get("tool_name").and_then(Value::as_str) == Some("Read") {
        let wide_project_log = text.contains(".ai/logs/") && text.ends_with(".log")
            && event.pointer("/tool_input/limit").and_then(Value::as_u64).is_none_or(|n| n > 40);
        return heavy_log_path(text) || wide_project_log;
    }
    segments(&without_heredoc_bodies(text)).iter().any(|segment| {
        let words = execution_words(segment);
        let Some(first) = words.first().map(|w| w.rsplit('/').next().unwrap_or(w)) else { return false };
        let dump_project_log = first == "cat" && words[1..].iter().any(|w| w.contains(".ai/logs/") && w.contains(".log"));
        let read_heavy_log = matches!(first, "tail" | "cat" | "head" | "rg" | "grep" | "less" | "sed" | "awk" | "python" | "python3" | "jq")
            && words[1..].iter().any(|w| heavy_log_path(w));
        read_heavy_log || dump_project_log
    }) || browser_work(event)
}

const SOURCE_READ_REASON: &str = "위임자는 제품 소스를 20줄 넘게 읽지 않습니다: 런타임 실패는 ① runtime-fresh 결과 ② record-log 한 줄 ③ 재현 위임 순서로 보고, 위치는 rg -n으로 찾아 20줄 이하만 읽으세요.";

fn product_source(path: &str, cwd: &Path) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let expanded = match path.strip_prefix("~/") { Some(rest) if !home.is_empty() => Path::new(&home).join(rest), _ => PathBuf::from(path) };
    let file = normalize(&if expanded.is_absolute() { expanded } else { cwd.join(expanded) });
    let Some(relative) = workflow_relative(&file) else { return false };
    if relative.components().count() < 2 { return false; }
    // manage_decision is the hooks' own judgment tool, not a product
    if file.starts_with("/home/tree/extensions/manage_decision") { return false; }
    if relative.components().any(|c| matches!(c.as_os_str().to_str(), Some("test" | "tests" | "dist" | "node_modules" | ".ai"))) { return false; }
    let Some(name) = file.file_name().and_then(|s| s.to_str()) else { return false };
    if name.contains(".test.") { return false; }
    matches!(file.extension().and_then(|s| s.to_str()), Some("ts" | "tsx" | "js" | "cjs" | "mjs" | "rs" | "py" | "lua"))
}

fn sed_line_count(script: &str) -> usize {
    script.split(';').filter_map(|part| {
        let range = part.trim().trim_start_matches(|c: char| !c.is_ascii_digit());
        let (start, end) = range.split_once(',')?;
        let start = start.parse::<usize>().ok()?;
        let end = end.strip_suffix('p')?.parse::<usize>().ok()?;
        Some(end.saturating_sub(start).saturating_add(1))
    }).sum()
}

fn source_read_denied(event: &Value, role: Option<&str>) -> bool {
    if role.is_some() { return false; }
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    match event.get("tool_name").and_then(Value::as_str) {
        Some("Read") => {
            let Some(file) = event.pointer("/tool_input/file_path").and_then(Value::as_str) else { return false };
            let limit = event.pointer("/tool_input/limit").and_then(Value::as_u64);
            product_source(file, cwd) && limit.is_none_or(|n| n > 20)
        }
        Some("Bash") => {
            let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
            let mut cwd = cwd.to_path_buf();
            segments(&without_heredoc_bodies(command)).iter().any(|segment| {
                let words = execution_words(segment);
                if words.first() == Some(&"cd") {
                    if let Some(path) = words.get(1) {
                        let path = path.trim_matches(['\'', '"']);
                        let home = std::env::var("HOME").unwrap_or_default();
                        let expanded = match path.strip_prefix('~') {
                            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("{home}{rest}"),
                            _ => path.to_string(),
                        };
                        let raw = Path::new(&expanded);
                        cwd = normalize(&if raw.is_absolute() { raw.to_path_buf() } else { cwd.join(raw) });
                    }
                    return false;
                }
                let Some(program) = words.first().map(|w| w.rsplit('/').next().unwrap_or(w)) else { return false };
                if !matches!(program, "sed" | "cat" | "head" | "tail") { return false; }
                // A redirect target is written, not read.
                let args: Vec<&str> = words[1..].iter().map(|w| w.trim_matches(['\'', '"']))
                    .take_while(|w| !w.starts_with('>')).collect();
                let has_source = args.iter().any(|arg| !arg.starts_with('-') && product_source(arg, &cwd));
                if !has_source { return false; }
                match program {
                    "cat" => true,
                    "sed" => {
                        let numbered = args.iter().any(|a| *a == "-n" || a.starts_with("-n"));
                        numbered && args.iter().map(|a| sed_line_count(a)).sum::<usize>() > 20
                    }
                    "head" | "tail" => {
                        let mut count = None;
                        for (i, arg) in args.iter().enumerate() {
                            if *arg == "-n" { count = args.get(i + 1).and_then(|n| n.parse::<usize>().ok()); }
                            else if let Some(n) = arg.strip_prefix("--lines=") { count = n.parse::<usize>().ok(); }
                            else if let Some(n) = arg.strip_prefix("-n") { if !n.is_empty() { count = n.parse::<usize>().ok(); } }
                        }
                        count.is_none_or(|n| n > 20)
                    }
                    _ => false,
                }
            })
        }
        _ => false,
    }
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
        let is_node_test = *first == "node" && words[1..].contains(&"--test");
        let browser = ["9222", "stagehand", "Stagehand", "connectOverCDP", "Runtime.evaluate"].iter().any(|k| segment.contains(k));
        let scratch_script = words.get(1).is_some_and(|w| scratch_dir(w) || (in_scratch && !w.starts_with(['/', '~', '-'])));
        *first == "run_cdp"
            || (*first == "node" && !is_node_test && (browser || scratch_script))
            || (*first == "curl" && words[1..].iter().any(|w| w.contains(":9222")))
    })
}

/// 화면 조작용 relay는 유지하고, 화면 밖 CDP 값 조회만 행 목표로 제한한다.
fn hidden_cdp_call(event: &Value) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return false; }
    let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
    segments(&without_heredoc_bodies(command)).iter().any(|segment| {
        let words = execution_words(segment);
        if words.is_empty() { return false; }
        let program = words[0].trim_matches(['\'', '"']).rsplit('/').next().unwrap_or("");
        if matches!(program, "run_cdp" | "run-cdp") { return true; }
        let compact: String = segment.chars().filter(|c| !c.is_whitespace()).collect();
        program == "node" && words.iter().any(|word| *word == "relay")
            && ["\"op\":\"storage\"", "\"op\":\"eval\"", "\"op\":\"network\"",
                "\\\"op\\\":\\\"storage\\\"", "\\\"op\\\":\\\"eval\\\"", "\\\"op\\\":\\\"network\\\""].iter().any(|op| compact.contains(op))
    })
}

fn goal_needs_hidden_cdp(goal: &str) -> bool {
    goal.lines().any(|line| {
        let lower = line.to_lowercase();
        let subject = ["네트워크", "스토리지", "저장소", "network", "storage", "localstorage", "sessionstorage", "indexeddb"]
            .iter().any(|word| lower.contains(word));
        let value = ["값", "응답", "요청", "헤더", "상태", "데이터", "value", "response", "request", "header", "status", "cookie", "key"]
            .iter().any(|word| lower.contains(word));
        subject && value && !["값이 없는 행", "값이 없는데", "값 요구가 없는 행"].iter().any(|phrase| lower.contains(phrase))
    })
}

fn hidden_cdp_denial_with(event: &Value, role: Option<&str>, goal: impl Fn(&str, &str) -> Option<String>) -> Option<String> {
    if !matches!(role, None | Some("verifier")) || !hidden_cdp_call(event) { return None; }
    let row = std::env::var("T26_ROW").ok().filter(|value| value.parse::<u64>().is_ok());
    let project = event.get("cwd").and_then(Value::as_str).unwrap_or("");
    let allowed = row.as_deref().and_then(|id| goal(id, project)).is_some_and(|text| goal_needs_hidden_cdp(&text));
    (!allowed).then(|| "run-cdp 화면 밖 network·storage 값 조회는 현재 행 목표에 그 관측값이 있을 때만 허용됩니다. 화면 확인은 stagehand.mjs relay read·click·type 또는 windows-mcp를 쓰세요.".into())
}

fn hidden_cdp_denial(event: &Value, role: Option<&str>) -> Option<String> {
    hidden_cdp_denial_with(event, role, |row, project| {
        let output = Command::new("/home/tree/ai/scripts/bin/grist")
            .args(["goal", row, "--project", project]).output().ok()?;
        output.status.success().then(|| String::from_utf8(output.stdout).ok()).flatten()
    })
}

const IMPROVER_BROWSER_REASON: &str = "개선자는 규칙·Skill·hook·~/ai/scripts만 고칩니다. CDP·stagehand 실행은 검증자 몫입니다.";
const IMPROVER_PRODUCT_REASON: &str = "개선자는 제품 코드를 고치지 않습니다. 제품 결함은 개선 요구에 적고 위임자가 대행자에게 맡깁니다.";

/// 개선자(T26_ROLE=improver): 브라우저 실행과 제품 코드 수정을 거부하고, 생성본 대신 원본만 고치게 한다.
fn improver_denial(event: &Value) -> Option<String> {
    if browser_work(event) { return Some(IMPROVER_BROWSER_REASON.into()); }
    if let Some(source) = generated_write(event) {
        return Some(format!("생성본은 직접 수정할 수 없습니다. 원본 {source}을 고친 뒤 agent-rules sync를 실행하세요."));
    }
    target(event).is_some().then(|| IMPROVER_PRODUCT_REASON.into())
}

const WORKER_BROWSER_REASON: &str = "CDP·stagehand 실행과 런타임 검증은 검증자(dispatch-agent --role verifier)가 합니다. 대행자는 유닛테스트까지만 하고 task-submit 하세요.";
const MAIN_HEAVY_REASON: &str = "메인은 판단·지시만 합니다. CDP·stagehand 실행은 검증자(--role verifier)에게, 위임 로그 읽기는 구현자에게 맡기고, dispatch-agent의 exit·last 한 줄과 grist task-show --section 구현으로 판정하고, 원인은 --resume으로 구현자에게 한 줄로 받으세요.";
const GRIST_UI_PROBE_REASON: &str = "Grist 스키마·행 조회는 grist 제공 함수(grist tables, grist columns <TABLE>, grist get <TABLE> --id/--filter/--fields)나 CDP window.grist 함수를 바로 쓰세요. _grist_* 내부 메타데이터 조회, grist get 파이프 스크립트(python3/node), localhost:8484/*.js curl 역추적은 거부합니다.";
const MAIN_INTERPRETER_REASON: &str = "작업 도구 스크립트는 Rust만 씁니다. python·node 즉석 실행 대신 Edit·sed -i·rg·grist get --fields를 쓰고, 반복 작업은 ~/ai/scripts Rust 하위 명령으로 추가하세요.";
const VERIFIER_SCRIPT_REASON: &str = "검증자는 CDP 스크립트(test/tmp/*.mjs·openStagehand)를 직접 작성·실행하지 않습니다. node ~/ai/skills/access-stagehand/scripts/stagehand.mjs relay '<json>' 한 줄 요청이나 screen-evidence를 쓰세요.";
const VERIFIER_WINDOWS_SHELL_REASON: &str = "Codex 검증자 셸에서 Windows exe·PowerShell·WebView2 CDP 19339 직접 실행은 120초 무진행을 만듭니다. Windows 배포본은 위임자의 win-smoke 출력으로 확인하고, 그 증거가 없으면 검증 실패로 보고하세요.";

fn verifier_windows_shell_denied(event: &Value) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return false; }
    let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
    segments(&without_heredoc_bodies(command)).iter().any(|segment| {
        let words = execution_words(segment);
        let Some(program) = words.first() else { return false };
        let program = program.trim_matches(['\'', '"']).rsplit(['/', '\\']).next().unwrap_or(program).to_ascii_lowercase();
        program.ends_with(".exe")
            || (segment.contains("19339") && matches!(program.as_str(), "curl" | "wget" | "nc" | "ncat" | "socat"))
    })
}

fn is_adhoc_tmp_js(path: &str) -> bool {
    let clean = path.trim_matches(['\'', '"', ';', ')', ',']);
    clean.contains("test/tmp/")
        && [".mjs", ".js", ".cjs", ".ts"].iter().any(|ext| clean.ends_with(ext))
}

fn verifier_adhoc_cdp_denied(event: &Value) -> bool {
    match event.get("tool_name").and_then(Value::as_str) {
        Some("Write" | "Edit") => event
            .pointer("/tool_input/file_path")
            .and_then(Value::as_str)
            .is_some_and(is_adhoc_tmp_js),
        Some("Bash") => event
            .pointer("/tool_input/command")
            .and_then(Value::as_str)
            .is_some_and(|cmd| {
                let head = without_heredoc_bodies(cmd);
                cmd.contains("openStagehand")
                    || head.split_whitespace().any(|w| {
                        let w = w.trim_start_matches(['>', '<']);
                        is_adhoc_tmp_js(w)
                    })
            }),
        _ => false,
    }
}

fn project_for_script(path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(EXTENSIONS).ok()?;
    let mut parts = relative.components();
    let name = parts.next()?.as_os_str().to_str()?;
    if name.starts_with('.') || parts.next().is_none() { return None; }
    let project = Path::new(EXTENSIONS).join(name);
    (project.is_dir() && project.join("AGENTS.md").is_file()).then_some(project)
}

fn python_product(path: &Path) -> bool {
    let Some(project) = project_for_script(path) else { return false };
    project.join("blender_manifest.toml").is_file()
        || (project.join("__init__.py").is_file() && project.join("pyproject.toml").is_file())
}

fn script_path(path: &str, cwd: &Path) -> PathBuf {
    let raw = Path::new(path.trim_matches(['\'', '"']));
    normalize(&if raw.is_absolute() { raw.to_path_buf() } else { cwd.join(raw) })
}

fn main_interpreter_denied(event: &Value) -> bool {
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    match event.get("tool_name").and_then(Value::as_str) {
        Some("Write") | Some("Edit") | Some("MultiEdit") => {
            let Some(path) = event.pointer("/tool_input/file_path").and_then(Value::as_str) else { return false };
            let path = script_path(path, cwd);
            if path.extension().is_some_and(|ext| ext == "py") && python_product(&path) { return false; }
            matches!(path.extension().and_then(|s| s.to_str()), Some("py" | "mjs"))
                && !path.exists() && project_for_script(&path).is_none()
        }
        Some("Bash") => {
            let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
            let bodyless = without_heredoc_bodies(command);
            let mut active_cwd = cwd.to_path_buf();
            for segment in segments(&bodyless) {
                let words = execution_words(&segment);
                let Some(first) = words.first() else { continue };
                if *first == "cd" {
                    if let Some(dir) = words.get(1) { active_cwd = script_path(dir, &active_cwd); }
                    continue;
                }
                let program = first.rsplit('/').next().unwrap_or(first);
                if matches!(program, "python" | "python3") {
                    if segment.contains("<<") || words.iter().skip(1).any(|w| matches!(*w, "-c" | "-" | "-i")) { return true; }
                } else if program == "node" {
                    if segment.contains("<<") || words.iter().skip(1).any(|w| matches!(*w, "-e" | "--eval" | "-p" | "--print") || w.starts_with("--eval=")) { return true; }
                }
                // Redirection targets are creations; heredoc contents were removed above.
                for (i, _) in segment.match_indices('>') {
                    if i > 0 && segment.as_bytes()[i - 1].is_ascii_digit() { continue; }
                    let rest = segment[i + 1..].trim_start_matches(['>', '|', '&']).trim_start();
                    let Some(path) = rest.split_whitespace().next() else { continue };
                    let path = script_path(path, &active_cwd);
                    if matches!(path.extension().and_then(|s| s.to_str()), Some("py" | "mjs"))
                        && !path.exists() && project_for_script(&path).is_none() { return true; }
                }
            }
            false
        }
        _ => false,
    }
}

/// 함수(grist tables/columns/get --filter, CDP window.grist)로 알 수 있는 정보를 _grist_* 메타데이터·번들 curl·파이프 스크립트 bash로 우회 탐색하는 것을 거부한다.
fn grist_ui_probe(event: &Value) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return false; }
    let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false; };
    let bodyless = without_heredoc_bodies(command);
    let segs = segments(&bodyless);
    let has_grist_get = segs.iter().any(|segment| {
        let words = execution_words(segment);
        words.first().is_some_and(|w| w.rsplit('/').next().unwrap_or(w) == "grist")
            && words.get(1) == Some(&"get")
    });
    let pipes_to_script = has_grist_get
        && bodyless.contains('|')
        && segs.iter().any(|segment| {
            let words = execution_words(segment);
            words.first().is_some_and(|w| matches!(w.rsplit('/').next().unwrap_or(w), "python" | "python3" | "node"))
        });
    if pipes_to_script { return true; }
    segs.iter().any(|segment| {
        let words = execution_words(segment);
        let Some(first) = words.first() else { return false; };
        let program = first.rsplit('/').next().unwrap_or(first);
        if program == "grist" && words.get(1) == Some(&"get") {
            return words.get(2).is_some_and(|table| table.starts_with("_grist_"));
        }
        program == "curl"
            && words.iter().skip(1).any(|word| {
                word.contains("localhost:8484")
                    && (word.contains(".js") || word.contains("/o/docs/"))
            })
    })
}

const LOG_BEFORE_CHANGES_REASON: &str = "과거 작업·원인 조사 시 원본 기록(.ai/logs·.ai/dispatch 로그·n8n DB·Grist 동작 이력·docker logs·에이전트 세션 기록)을 뒤지기 전에 먼저 관련 프로젝트 .ai/changes 날짜별 기록(rg -n <키워드> <프로젝트>/.ai/changes/<yymmdd>.*.md 또는 grist task-show)부터 대조하세요.";

/// 261114 #949: 햇반 공모전 조사에서 changes 기록은 보지 않고 n8n DB·Grist 이력·docker 로그·Codex 세션만 뒤졌다.
/// `.ai/logs`뿐 아니라 이 원본 기록 전부를 changes 선행 대조 대상으로 본다.
const RAW_HISTORY_MARKERS: [&str; 9] = [".ai/logs/", ".ai/dispatch/", "n8n/data/database.sqlite", "_gristsys_actionhistory",
    "docker logs", ".codex/sessions", ".claude/projects/", "n8neventlog", "execution_data"];

/// A changes record counts as read only when a tool call actually opened it (Bash command or Read path),
/// not when `.ai/changes` merely appears in git status or prose.
fn transcript_read_changes(transcript: &str) -> bool {
    ["\"command\":\"", "\"file_path\":\"", "\"CommandLine\":\"", "\"Path\":\""].iter().any(|key| {
        transcript.match_indices(key).any(|(start, _)| {
            let rest = &transcript[start + key.len()..];
            let end = rest.find("\",\"").or_else(|| rest.find("\"}")).unwrap_or(rest.len().min(600));
            let value = &rest[..end.min(rest.len())];
            value.contains(".ai/changes/") || value.contains("grist task-show") || value.contains("bin/grist task-show")
        })
    })
}

fn log_before_changes_denial(event: &Value) -> bool {
    let text = match event.get("tool_name").and_then(Value::as_str) {
        Some("Read") => event.pointer("/tool_input/file_path").and_then(Value::as_str),
        Some("Bash") => event.pointer("/tool_input/command").and_then(Value::as_str),
        _ => None,
    };
    let Some(text) = text else { return false };
    let bodyless = without_heredoc_bodies(text);
    let lower = bodyless.to_ascii_lowercase();
    let raw = RAW_HISTORY_MARKERS.iter().any(|marker| lower.contains(&marker.to_ascii_lowercase()))
        && !lower.contains("memory/");
    if !raw || bodyless.contains(".ai/changes") || bodyless.contains("task-show") { return false; }
    let tp = event.get("transcript_path").and_then(Value::as_str).unwrap_or("");
    !std::fs::read_to_string(tp).ok().is_some_and(|t| transcript_read_changes(&t))
}

fn role_denial(event: &Value, role: Option<&str>) -> Option<&'static str> {
    if let Some(reason) = pre_worker_denial(event) { return Some(reason); }
    if source_read_denied(event, role) { return Some(SOURCE_READ_REASON); }
    if matches!(role, None | Some("verifier")) && grist_ui_probe(event) { return Some(GRIST_UI_PROBE_REASON); }
    if role == Some("verifier") && verifier_adhoc_cdp_denied(event) { return Some(VERIFIER_SCRIPT_REASON); }
    if role == Some("verifier") && verifier_windows_shell_denied(event) { return Some(VERIFIER_WINDOWS_SHELL_REASON); }
    if role_exempt(role) { return None; }
    if role == Some("worker") { return browser_work(event).then_some(WORKER_BROWSER_REASON); }
    if role == Some("improver") { return None; }
    if role.is_none() && main_interpreter_denied(event) { return Some(MAIN_INTERPRETER_REASON); }
    if role.is_none() && event.get("tool_name").and_then(Value::as_str) == Some("Bash") {
        if let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) {
            let segments = segments(&without_heredoc_bodies(command));
            if segments.iter().filter(|segment| !segment.trim().is_empty()).count() == 1 && segments.iter().any(|segment| {
                let words = execution_words(segment);
                words.first().is_some_and(|w| w.rsplit('/').next() == Some("node"))
                    && words.get(1).is_some_and(|w| w.ends_with("/access-stagehand/scripts/stagehand.mjs"))
            }) { return None; }
        }
    }
    main_heavy_work(event).then_some(MAIN_HEAVY_REASON)
        .or_else(|| (role.is_none() && log_before_changes_denial(event)).then_some(LOG_BEFORE_CHANGES_REASON))
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


const VERSION_FILES: [&str; 3] = ["package.json", "manifest.json", "Cargo.toml"];

/// Bash 명령에서 버전 파일(package.json·manifest.json·Cargo.toml)의 version을 바꾸는 대상 경로를 찾는다.
fn version_bump_targets(event: &Value) -> Vec<PathBuf> {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return Vec::new(); }
    let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return Vec::new() };
    let mut cwd = PathBuf::from(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    let mut out = Vec::new();
    for segment in segments(&without_heredoc_bodies(command)) {
        let words: Vec<&str> = segment.split_whitespace().map(|w| w.trim_matches(['\'', '"'])).collect();
        if words.first() == Some(&"cd") {
            if let Some(dir) = words.get(1) { cwd = normalize(&cwd.join(dir)); }
            continue;
        }
        let editing = (words.contains(&"sed") && words.iter().any(|w| w.starts_with("-i")))
            || (words.contains(&"npm") && words.contains(&"version"))
            || (words.contains(&"perl") && words.iter().any(|w| w.starts_with("-pi")));
        if !editing { continue; }
        for word in &words {
            if VERSION_FILES.iter().any(|name| word.ends_with(name)) {
                out.push(normalize(&cwd.join(word)));
            }
        }
        if words.contains(&"npm") && words.contains(&"version") { out.push(normalize(&cwd.join("package.json"))); }
    }
    out
}

fn owning_project(file: &Path) -> Option<PathBuf> {
    let mut dir = file.parent()?;
    while workflow_relative(dir).is_some() {
        if dir.join("AGENTS.md").is_file() && dir.join(".ai/changes").is_dir() { return Some(dir.to_path_buf()); }
        dir = dir.parent()?;
    }
    None
}

/// 최신 changes 파일 `# 작업` 절의 열린 행 ID.
fn flow_rows(project: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(project.join(".ai/changes")) else { return Vec::new() };
    let Some(latest) = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "md")).max() else { return Vec::new() };
    let text = std::fs::read_to_string(latest).unwrap_or_default();
    let mut inside = false;
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with("# ") { inside = line.trim() == "# 작업"; continue; }
        if !inside { continue; }
        if let Some(start) = line.rfind("(#") {
            let id: String = line[start + 2..].chars().take_while(char::is_ascii_digit).collect();
            if !id.is_empty() { rows.push(id); }
        }
    }
    rows
}

fn cached_row_closed(project: &Path, id: &str) -> bool {
    std::fs::read_to_string(project.join(format!(".ai/gate/{id}.row")))
        .map(|row| row.starts_with("H=확인") || row.starts_with("H=완료"))
        .unwrap_or(false)
}

/// 캐시가 열린 행이면 `grist task`로 다시 읽어(캐시 갱신) 판정한다. `task-confirm`·`task-complete`는 `$H`를 바꿔도 캐시를 쓰지 않는다(261107 #749 확인 뒤 버전 올리기 오거부).
fn row_closed(project: &Path, id: &str) -> bool {
    if cached_row_closed(project, id) { return true; }
    refresh_row_cache(project, id);
    cached_row_closed(project, id)
}

/// Release versions belong after every active row has reached the submit gate.
fn row_submitted(project: &Path, id: &str) -> bool {
    let path = project.join(format!(".ai/gate/{id}.row"));
    let read = || std::fs::read_to_string(&path).unwrap_or_default();
    let mut state = read();
    if !["H=검증", "H=확인", "H=완료"].iter().any(|stage| state.starts_with(stage)) {
        refresh_row_cache(project, id);
        state = read();
    }
    ["H=검증", "H=확인", "H=완료"].iter().any(|stage| state.starts_with(stage))
}

fn version_in_file(file: &Path) -> Option<String> {
    let content = std::fs::read_to_string(file).ok()?;
    content.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix("\"version\"").or_else(|| line.strip_prefix("version"))?;
        let value = value.trim_start().strip_prefix(':').or_else(|| value.trim_start().strip_prefix('='))?.trim();
        value.strip_prefix('"')?.split('"').next().map(str::to_string)
    })
}

#[cfg(not(test))]
fn refresh_row_cache(project: &Path, id: &str) {
    let Ok(home) = std::env::var("HOME") else { return };
    let _ = std::process::Command::new(format!("{home}/ai/scripts/bin/grist"))
        .args(["task", id, "--project"]).arg(project)
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
fn refresh_row_cache(_project: &Path, _id: &str) {}

/// 같은 흐름(앞선 버전 올리기 때 열려 있던 행이 아직 확인·완료 전)에서 같은 버전 파일을 다시 올리면 거부한다.
fn version_bump_denial(event: &Value) -> Option<String> {
    // 모든 대상 파일을 먼저 검사하고, 전부 통과했을 때만 흐름 상태를 쓴다(앞 파일만 기록되고 뒤 파일에서 거부되면 실행되지 않은 올리기가 남는다, 261107).
    let mut writes = Vec::new();
    for file in version_bump_targets(event) {
        // A scaffold command may create its first version file with sed -i in the
        // same shell. That creation does not consume the flow's release bump.
        if !file.is_file() { continue; }
        let Some(project) = owning_project(&file) else { continue };
        let unsubmitted: Vec<String> = flow_rows(&project).into_iter()
            .filter(|id| !row_submitted(&project, id)).collect();
        if !unsubmitted.is_empty() {
            return Some(format!("{} 버전 변경은 작업 행 #{}의 task-submit 후에 하세요. 먼저 모든 행을 $H=검증으로 제출하세요.", file.display(), unsubmitted.join(",#")));
        }
        let relative = file.strip_prefix(&project).unwrap_or(&file).to_string_lossy().replace('/', "_");
        let state = project.join(format!(".ai/gate/version-flow-{relative}"));
        let version = version_in_file(&file).unwrap_or_default();
        if let Ok(previous) = std::fs::read_to_string(&state) {
            let mut words = previous.split_whitespace();
            let recorded_version = words.next().unwrap_or("");
            let ids: Vec<&str> = if recorded_version.starts_with("version=") { words.collect() } else { previous.split_whitespace().collect() };
            let prior_bump_took_effect = if let Some(before) = recorded_version.strip_prefix("version=") {
                before != version
            } else { version != "0.0.1" };
            let open: Vec<&str> = ids.into_iter().filter(|id| !row_closed(&project, id)).collect();
            if prior_bump_took_effect && !open.is_empty() {
                return Some(format!("같은 요청 흐름에서 {}의 버전을 이미 올렸어요(미확인 행 #{}). 검증 실패 뒤 수정 재배포는 버전을 올리지 말고 같은 버전으로 빌드·배포 복사만 하세요.", file.display(), open.join(",#")));
            }
        }
        let rows = flow_rows(&project);
        if !rows.is_empty() { writes.push((project, state, format!("version={version}\n{}", rows.join(" ")))); }
    }
    for (project, state, rows) in writes {
        let _ = std::fs::create_dir_all(project.join(".ai/gate"));
        let _ = std::fs::write(&state, rows);
    }
    None
}

const IMAGE_LIMIT: usize = 2;
const SOURCE_SLICE_LIMIT: usize = 2;
const SOURCE_SLICE_REASON: &str = "위임자는 같은 제품 소스 파일을 20줄씩 쪼개 3번 이상 연속 탐색하지 않습니다. 진입점 함수 확인이 끝났으면 내부 구현 탐색은 대행자(dispatch-agent)에게 맡기세요.";

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

/// 메인 세션이 같은 제품 소스 파일을 20줄 이하로 쪼개 SOURCE_SLICE_LIMIT회를 넘어 읽으면 거부한다.
fn source_slice_denied(event: &Value, state_dir: &Path) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Read") { return false; }
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    let Some(file) = event.pointer("/tool_input/file_path").and_then(Value::as_str) else { return false };
    if !product_source(file, cwd) { return false; }
    let Some(session) = event.get("session_id").and_then(Value::as_str) else { return false };
    if session.is_empty() || !session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return false; }
    let raw = Path::new(file);
    let norm = normalize(&if raw.is_absolute() { raw.to_path_buf() } else { cwd.join(raw) });
    let offset = event.pointer("/tool_input/offset").and_then(Value::as_u64).unwrap_or(1);
    let limit = event.pointer("/tool_input/limit").and_then(Value::as_u64).unwrap_or(20);
    let key = format!("{}#{offset}:{limit}", norm.display());
    let prefix = format!("{}#", norm.display());
    let state = state_dir.join(format!("source-slices-{session}.json"));
    let mut seen: BTreeSet<String> = std::fs::read(&state).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if seen.contains(&key) { return false; }
    if seen.iter().filter(|entry| entry.starts_with(&prefix)).count() >= SOURCE_SLICE_LIMIT { return true; }
    seen.insert(key);
    let _ = std::fs::create_dir_all(state_dir);
    let _ = std::fs::write(&state, serde_json::to_vec(&seen).unwrap_or_default());
    false
}

fn claude_agent_denied(event: &Value) -> bool {
    matches!(event.get("tool_name").and_then(Value::as_str), Some("Agent" | "Task"))
}

fn monitor_denied(event: &Value) -> bool {
    event.get("tool_name").and_then(Value::as_str) == Some("Monitor")
}

fn generated_path(path: &str, cwd: &Path) -> Option<&'static str> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/tree".into());
    let expanded = path.strip_prefix('~').map_or_else(|| path.to_string(), |rest| format!("{home}{rest}"));
    let normalized = normalize(&if Path::new(&expanded).is_absolute() { PathBuf::from(expanded) } else { cwd.join(expanded) });
    let rel = normalized.strip_prefix(&home).ok()?.to_str()?;
    match rel {
        ".claude/CLAUDE.md" | ".codex/AGENTS.md" | ".gemini/GEMINI.md" | ".gemini/antigravity-cli/GEMINI.md" => Some("~/ai/rules/Agents.md"),
        ".claude/settings.json" | ".codex/hooks.json" | ".gemini/settings.json" | ".gemini/antigravity-cli/settings.json" => Some("~/ai/codex/hook-manifest.json"),
        _ => None,
    }
}

/// A quoted path in an interpreter program is data unless it is an argument to a file write.
fn interpreter_heredoc_write(command: &str, cwd: &Path) -> Option<&'static str> {
    let mut delimiter: Option<String> = None;
    let mut interpreter = false;
    for line in command.lines() {
        if let Some(end) = &delimiter {
            if line.trim() == end { delimiter = None; continue; }
            if interpreter {
                let code = line.trim_start();
                for marker in ["open(", "Path(", "writeFileSync(", "writeFile("] {
                    let Some((prefix, args)) = code.split_once(marker) else { continue };
                    // A marker inside a quoted assertion or test string is not an executed call.
                    if prefix.matches(['\'', '"']).count() % 2 != 0 { continue; }
                    let args = args.trim_start();
                    let Some(quote) = args.chars().next().filter(|c| *c == '\'' || *c == '"') else { continue };
                    let Some((path, tail)) = args[1..].split_once(quote) else { continue };
                    let writes = match marker {
                        "open(" => tail.trim_start().strip_prefix(',').is_some_and(|mode| {
                            let mode = mode.trim_start().trim_start_matches(['\'', '"']);
                            mode.starts_with(['w', 'a', 'x']) || mode.starts_with("r+")
                        }),
                        "Path(" => tail.starts_with(").write_text(") || tail.starts_with(").write_bytes("),
                        _ => true,
                    };
                    if writes {
                        if let Some(source) = generated_path(path, cwd) { return Some(source); }
                    }
                }
            }
            continue;
        }
        if let Some((head, tail)) = line.rsplit_once("<<") {
            let word = tail.trim_start_matches('-').trim().split_whitespace().next().unwrap_or("");
            let word = word.trim_matches(['\'', '"']);
            if !word.is_empty() {
                delimiter = Some(word.to_string());
                interpreter = head.split_whitespace().any(|word| matches!(word.rsplit('/').next(), Some("python" | "python3" | "node")));
            }
        }
    }
    None
}

fn generated_write(event: &Value) -> Option<&'static str> {
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    let tool = event.get("tool_name").and_then(Value::as_str)?;
    if matches!(tool, "Edit" | "Write" | "MultiEdit" | "NotebookEdit") {
        let path = event.pointer(if tool == "NotebookEdit" { "/tool_input/notebook_path" } else { "/tool_input/file_path" }).and_then(Value::as_str)?;
        return generated_path(path, cwd);
    }
    if tool != "Bash" { return None; }
    let original = event.pointer("/tool_input/command").and_then(Value::as_str)?;
    if let Some(source) = interpreter_heredoc_write(original, cwd) { return Some(source); }
    let command = without_heredoc_bodies(original);
    let command = command.as_str();
    // agent-rules owns generated writes; its source edits and sync are allowed.
    if execution_words(command).iter().any(|w| w.rsplit('/').next() == Some("agent-rules")) { return None; }
    // `>` 리다이렉션은 그 대상만 쓰기다(`2>/dev/null`이 같은 명령의 읽기 경로를 쓰기로 만들지 않게).
    let redirect_targets = command.match_indices('>').filter_map(|(i, _)| {
        let rest = command[i + 1..].trim_start_matches(['>', '|', '&']).trim_start();
        rest.split(|c: char| c.is_whitespace() || matches!(c, ';' | '|' | '&' | ')')).next()
    });
    for target in redirect_targets {
        if let Some(source) = generated_path(target.trim_matches(['\'', '"']), cwd) { return Some(source); }
    }
    let mut write = false;
    for segment in segments(command) {
        let words = execution_words(&segment);
        let Some(program) = words.first().map(|w| w.rsplit('/').next().unwrap_or(w)) else { continue };
        if matches!(program, "tee" | "cp" | "mv" | "install" | "touch" | "rm" | "truncate" | "apply_patch" | "python" | "python3" | "node")
            || (program == "sed" && words.iter().any(|w| w.starts_with("-i"))) { write = true; }
    }
    if !write { return None; }
    for token in command.split(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | ';' | '|' | '&' | '>' | '<' | '=' | '(' | ')')) {
        let token = token.trim_end_matches([',', ':']);
        if let Some(source) = generated_path(token, cwd) { return Some(source); }
    }
    None
}

fn normalize_codex_event(event: Value) -> Value {
    let name = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let input = event.get("tool_input").cloned().unwrap_or(Value::Null);
    let cwd = input.get("workdir").cloned().or_else(|| event.get("cwd").cloned()).unwrap_or(Value::Null);
    let session = event.get("session_id").cloned().unwrap_or(Value::Null);
    let transcript = event.get("transcript_path").cloned().unwrap_or(Value::Null);
    match name {
        "exec_command" | "functions.exec_command" => json!({"tool_name":"Bash","tool_input":{"command":input.get("cmd").or_else(|| input.get("command")).and_then(Value::as_str).unwrap_or("")},"cwd":cwd,"session_id":session,"transcript_path":transcript}),
        "apply_patch" | "functions.apply_patch" => {
            let patch = input.as_str().or_else(|| input.get("patch").and_then(Value::as_str)).unwrap_or("");
            json!({"tool_name":"Bash","tool_input":{"command":format!("apply_patch {patch}")},"cwd":cwd,"session_id":session,"transcript_path":transcript})
        }
        _ => event,
    }
}

fn extract_user_request(raw: &str) -> Option<String> {
    if let Some((_, rest)) = raw.split_once("<USER_REQUEST>") {
        let body = rest.split_once("</USER_REQUEST>").map(|(b, _)| b).unwrap_or(rest).trim();
        return (!body.is_empty()).then(|| body.to_string());
    }
    let trimmed = raw.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('<') || trimmed.starts_with("Stop hook feedback") {
        return None;
    }
    Some(trimmed.trim_end().to_string())
}

fn last_turn_prompt_and_replied(transcript: &str) -> Option<(String, bool)> {
    let mut last_prompt: Option<String> = None;
    let mut replied = false;
    for entry in transcript.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok()) {
        match entry.get("type").and_then(Value::as_str) {
            Some("user") if entry.get("isMeta") != Some(&Value::Bool(true)) => {
                let raw = match entry.pointer("/message/content") {
                    Some(Value::String(t)) => Some(t.as_str()),
                    Some(Value::Array(parts)) => parts.iter()
                        .find(|p| p.get("type").and_then(Value::as_str) == Some("text"))
                        .and_then(|p| p.get("text")).and_then(Value::as_str),
                    _ => None,
                };
                if let Some(p) = raw.and_then(extract_user_request) {
                    last_prompt = Some(p);
                    replied = false;
                }
            }
            Some("USER_INPUT") => {
                if let Some(p) = entry.get("content").and_then(Value::as_str).and_then(extract_user_request) {
                    last_prompt = Some(p);
                    replied = false;
                }
            }
            Some("response_item") if entry.pointer("/payload/role").and_then(Value::as_str) == Some("user") => {
                if let Some(p) = entry.pointer("/payload/content").and_then(Value::as_array)
                    .and_then(|parts| parts.iter().find(|part| part.get("type").and_then(Value::as_str) == Some("input_text")))
                    .and_then(|part| part.get("text")).and_then(Value::as_str).and_then(extract_user_request) {
                    last_prompt = Some(p);
                    replied = false;
                }
            }
            Some("response_item") if last_prompt.is_some() && entry.pointer("/payload/role").and_then(Value::as_str) == Some("assistant") => {
                if entry.pointer("/payload/content").and_then(Value::as_array).is_some_and(|parts| parts.iter().any(|part| {
                    part.get("type").and_then(Value::as_str) == Some("output_text")
                        && part.get("text").and_then(Value::as_str).is_some_and(|text| !text.trim().is_empty())
                })) { replied = true; }
            }
            Some("assistant") if last_prompt.is_some() => {
                if let Some(Value::Array(parts)) = entry.pointer("/message/content") {
                    if parts.iter().any(|p| p.get("type").and_then(Value::as_str) == Some("text")
                        && p.get("text").and_then(Value::as_str).is_some_and(|t| !t.trim().is_empty())) {
                        replied = true;
                    }
                }
            }
            Some("PLANNER_RESPONSE") if last_prompt.is_some() => {
                let has_tools = entry.get("tool_calls").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
                if !has_tools && entry.get("content").and_then(Value::as_str).is_some_and(|t| !t.trim().is_empty()) {
                    replied = true;
                }
            }
            _ => {}
        }
    }
    last_prompt.map(|p| (p, replied))
}

fn intervention_confirmed_at(state: &Path, root: &Path, session: &str, prior: &[String]) -> bool {
    let safe: String = session.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let rows: Vec<String> = std::fs::read_to_string(state.join("session_rows").join(format!("{safe}.json")))
        .ok().and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
        .unwrap_or_default().iter().map(|v| v.to_string().trim_matches('"').to_string()).collect();
    let projects: serde_json::Map<String, Value> = std::fs::read_to_string(state.join("session_projects").join(format!("{safe}.json")))
        .ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let open = flow_rows(root);
    rows.iter().any(|id| !prior.contains(id) && projects.get(id).and_then(Value::as_str) == Some("/home/tree/extensions") && !open.contains(id))
}

fn process_correction_denial(event: &Value) -> Option<String> {
    let transcript = std::fs::read_to_string(event.get("transcript_path")?.as_str()?).ok()?;
    let (prompt, _) = last_turn_prompt_and_replied(&transcript)?;
    let session = event.get("session_id").and_then(Value::as_str).unwrap_or("unknown");
    let turn = decision(session, &prompt);
    if !turn.process || intervention_confirmed_at(&prompt_decision::decision_state_dir(), Path::new("/home/tree/extensions"), session, &turn.prior_rows) { return None; }
    let tool = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/home/tree/extensions"));
    let product_path = |path: &str| {
        // `2>/dev/null`, `>out`, `<in` name the redirect target, not a path relative to cwd
        let path = path.rsplit(['>', '<']).next().unwrap_or(path).trim_start_matches('&');
        let path = path.trim_matches(['\'', '"', ';', ',', '(', ')', '|']);
        if path.is_empty() || path.starts_with('~') || path.starts_with('$') { return false; }
        let resolved = if Path::new(path).is_absolute() { normalize(Path::new(path)) } else { normalize(&cwd.join(path)) };
        // A relative word that names nothing on disk (a sed script `s/=.*/x/`, a URL `localhost:11434/api/tags`)
        // is not a product path; only existing files and folders are.
        if !Path::new(path).is_absolute() && !resolved.exists() { return false; }
        resolved.to_str().and_then(|path| path.strip_prefix("/home/tree/extensions/"))
            .and_then(|rest| rest.split_once('/'))
            .is_some_and(|(project, rel)| !project.starts_with('.') && project != "test"
                // manage_decision and manage_improve are workflow tools, not products
                && project != "manage_decision" && project != "manage_improve"
                && rel != ".ai" && !rel.starts_with(".ai/")
                // a search pattern like `codex/hooks/` is not a product path; only real project folders are
                && Path::new("/home/tree/extensions").join(project).is_dir())
    };
    let product_call = if matches!(tool, "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "Read") {
        event.pointer(if tool == "NotebookEdit" { "/tool_input/notebook_path" } else { "/tool_input/file_path" })
            .and_then(Value::as_str).is_some_and(product_path)
    } else if tool == "Bash" {
        let command = event.pointer("/tool_input/command").and_then(Value::as_str).unwrap_or("");
        let bodyless = without_heredoc_bodies(command);
        let command_head = bodyless.as_str();
        command_head.split_whitespace().filter(|word| word.contains('/')).any(product_path)
            || (command_head.split(['|', ';', '&', '\n']).filter_map(|segment| segment.split_whitespace().next()).any(|word| {
                let word = word.trim_matches(['\'', '"', ';', '(']);
                word == "dispatch-agent" || word.ends_with("/bin/dispatch-agent")
            }) && !command_head.contains("--role improver") && !command_head.contains("--agent codex"))
    } else { false };
    product_call.then(|| "현재 요청은 작업 과정 수정이에요(manage-intervention). 제품 파일 탐색·제품 행 재개를 멈추고 과정 수정·개선자 호출과 훅 검증을 마친 뒤 [⏸] 원래 행으로 복귀하세요.".into())
}

fn ai_write_before_intervention_row(event: &Value) -> Option<String> {
    ai_write_before_intervention_row_at(event, &prompt_decision::decision_state_dir())
}

fn ai_write_before_root_row(event: &Value) -> Option<String> {
    ai_write_before_root_row_at(event, &prompt_decision::decision_state_dir())
}

fn ai_write_before_root_row_at(event: &Value, state: &Path) -> Option<String> {
    if !writes_ai(event) { return None; }
    let session = event.get("session_id").and_then(Value::as_str)?;
    let safe_session: String = session.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let rows: Vec<String> = std::fs::read_to_string(state.join("session_rows").join(format!("{safe_session}.json")))
        .ok().and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
        .unwrap_or_default().iter().map(|v| v.to_string().trim_matches('"').to_string()).collect();
    let projects: serde_json::Map<String, Value> = std::fs::read_to_string(state.join("session_projects").join(format!("{safe_session}.json")))
        .ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if rows.iter().any(|id| projects.get(id).and_then(Value::as_str) == Some("/home/tree/extensions")) { return None; }
    Some("작업관리기 행을 기록하기 전에는 ~/ai를 수정할 수 없어요. /home/tree/extensions에 grist request-record → task-add로 이번 세션의 Table26 행을 만든 뒤 다시 실행하세요.".into())
}

fn ai_write_before_intervention_row_at(event: &Value, state: &Path) -> Option<String> {
    let transcript = std::fs::read_to_string(event.get("transcript_path")?.as_str()?).ok()?;
    let (prompt, _) = last_turn_prompt_and_replied(&transcript)?;
    let session = event.get("session_id").and_then(Value::as_str)?;
    let turn = prompt_decision::decision_at(state, session, &prompt);
    if !turn.process || !writes_ai(event) { return None; }
    let safe_session: String = session.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let rows: Vec<String> = std::fs::read_to_string(state.join("session_rows").join(format!("{safe_session}.json")))
        .ok().and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
        .unwrap_or_default().iter().map(|v| v.to_string().trim_matches('"').to_string()).collect();
    let projects: serde_json::Map<String, Value> = std::fs::read_to_string(state.join("session_projects").join(format!("{safe_session}.json")))
        .ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if rows.iter().any(|id| !turn.prior_rows.contains(id)
        && projects.get(id).and_then(Value::as_str) == Some("/home/tree/extensions")) { return None; }
    Some("개입 수정 전에 이번 세션의 작업관리기 행이 필요해요(manage-intervention §2). ~/ai 쓰기를 멈추고 /home/tree/extensions에 grist request-record → task-add로 개입 행을 만든 뒤 다시 실행하세요.".into())
}

/// 메인은 작업 도구(~/ai/scripts src·Skill scripts·~/.config/fish/functions·hooks)에 새 기능을 직접 구현하지 않는다:
/// 새 파일이나 30줄 이상 추가는 Codex 위임이고, 결함 수정(작은 패치)만 직접 고친다.
const MAIN_TOOL_FEATURE_REASON: &str = "메인은 작업 도구(~/ai/scripts·Skill scripts·~/.config/fish/functions·hooks)의 새 기능 구현(새 파일·30줄 이상 추가)을 직접 쓰지 않습니다. 계획을 ~/plan.md에 쓰고 dispatch-agent --agent codex로 맡기세요. 기존 도구의 결함 수정(10줄 미만 패치)만 직접 고칩니다.";
const MAIN_TOOL_FEATURE_LINES: usize = 30;

fn tool_source_path(raw: &str, cwd: &Path) -> Option<PathBuf> {
    let raw = raw.trim_matches(['\'', '"', ',', ';', '(', ')']);
    if raw.is_empty() || raw.starts_with('$') { return None; }
    let expanded = raw.strip_prefix('~').map(|rest| format!("/home/tree{rest}")).unwrap_or_else(|| raw.to_string());
    let path = Path::new(&expanded);
    let resolved = normalize(&if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) });
    let text = resolved.to_str()?;
    if text.ends_with(".md") { return None; }
    let tool = (text.starts_with("/home/tree/ai/scripts/") && text.contains("/src/"))
        || text.starts_with("/home/tree/ai/scripts/") && text.ends_with(".rs")
        || (text.starts_with("/home/tree/ai/skills/") && text.contains("/scripts/"))
        || text.starts_with("/home/tree/.config/fish/functions/")
        || text.starts_with("/home/tree/ai/codex/hooks");
    tool.then_some(resolved)
}

fn main_tool_feature_denial(event: &Value) -> Option<&'static str> {
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    let tool = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let lines = |s: &str| s.lines().count();
    let file_lines = |path: &Path| std::fs::read_to_string(path).map(|s| lines(&s)).unwrap_or(0);
    let added = match tool {
        "Write" => {
            let path = tool_source_path(event.pointer("/tool_input/file_path")?.as_str()?, cwd)?;
            let content = event.pointer("/tool_input/content").and_then(Value::as_str).unwrap_or("");
            if path.exists() { lines(content).saturating_sub(file_lines(&path)) } else { usize::MAX }
        }
        "Edit" => {
            let path = tool_source_path(event.pointer("/tool_input/file_path")?.as_str()?, cwd)?;
            if !path.exists() { return Some(MAIN_TOOL_FEATURE_REASON); }
            let new = event.pointer("/tool_input/new_string").and_then(Value::as_str).unwrap_or("");
            let old = event.pointer("/tool_input/old_string").and_then(Value::as_str).unwrap_or("");
            lines(new).saturating_sub(lines(old))
        }
        "MultiEdit" => {
            let path = tool_source_path(event.pointer("/tool_input/file_path")?.as_str()?, cwd)?;
            if !path.exists() { return Some(MAIN_TOOL_FEATURE_REASON); }
            event.pointer("/tool_input/edits").and_then(Value::as_array).map(|edits| edits.iter().map(|edit| {
                lines(edit.get("new_string").and_then(Value::as_str).unwrap_or(""))
                    .saturating_sub(lines(edit.get("old_string").and_then(Value::as_str).unwrap_or("")))
            }).sum()).unwrap_or(0)
        }
        "Bash" => {
            let command = event.pointer("/tool_input/command").and_then(Value::as_str).unwrap_or("");
            if command.contains("*** Begin Patch") {
                let mut path = None;
                let mut additions = 0;
                for line in command.lines() {
                    if let Some(raw) = line.strip_prefix("*** Add File: ") {
                        path = tool_source_path(raw, cwd);
                        if path.is_some() { return Some(MAIN_TOOL_FEATURE_REASON); }
                    } else if let Some(raw) = line.strip_prefix("*** Update File: ") {
                        path = tool_source_path(raw, cwd);
                    } else if line.starts_with("*** ") {
                        if additions >= MAIN_TOOL_FEATURE_LINES { return Some(MAIN_TOOL_FEATURE_REASON); }
                        additions = 0;
                    } else if path.is_some() && line.starts_with('+') && !line.starts_with("+++") {
                        additions += 1;
                    }
                }
                if additions >= MAIN_TOOL_FEATURE_LINES { return Some(MAIN_TOOL_FEATURE_REASON); }
            }
            let head = without_heredoc_bodies(command);
            let body_lines = lines(command).saturating_sub(head.lines().filter(|line| !line.is_empty()).count());
            let words: Vec<&str> = head.split_whitespace().collect();
            let mut target = None;
            for (index, word) in words.iter().enumerate() {
                let candidate = match *word {
                    ">" | ">>" => words.get(index + 1).copied(),
                    "tee" => words[index + 1..].iter().find(|w| !w.starts_with('-')).copied(),
                    w if w.starts_with('>') => Some(w.trim_start_matches('>')),
                    _ => None,
                };
                if let Some(path) = candidate.and_then(|c| tool_source_path(c, cwd)) { target = Some(path); break; }
            }
            let path = target?;
            if path.exists() { body_lines } else { usize::MAX }
        }
        _ => return None,
    };
    (added >= MAIN_TOOL_FEATURE_LINES).then_some(MAIN_TOOL_FEATURE_REASON)
}

fn writes_ai(event: &Value) -> bool {
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    let ai_path = |raw: &str, base: &Path| {
        let double_quoted_home = raw.starts_with('"');
        let raw = raw.trim_matches(['\'', '"', ',', ';']);
        let expanded = if double_quoted_home && raw.starts_with("$HOME/") {
            format!("{}{rest}", std::env::var("HOME").unwrap_or_else(|_| "/home/tree".into()), rest = &raw[5..])
        } else {
            raw.strip_prefix('~').map(|rest| format!("/home/tree{rest}")).unwrap_or_else(|| raw.to_string())
        };
        let path = Path::new(&expanded);
        let resolved = normalize(&if path.is_absolute() { path.to_path_buf() } else { base.join(path) });
        resolved.starts_with("/home/tree/ai")
    };
    let tool = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
    if matches!(tool, "Edit" | "Write" | "MultiEdit" | "NotebookEdit") {
        return event.pointer(if tool == "NotebookEdit" { "/tool_input/notebook_path" } else { "/tool_input/file_path" })
            .and_then(Value::as_str).is_some_and(|p| ai_path(p, cwd));
    }
    if tool != "Bash" { return false; }
    let original = event.pointer("/tool_input/command").and_then(Value::as_str).unwrap_or("");
    let command = without_heredoc_bodies(original);
    if command.trim_start().starts_with("apply_patch ")
        && command.split_whitespace().any(|word| ai_path(word, cwd)) { return true; }
    let mut active = cwd.to_path_buf();
    for segment in segments(&command) {
        let words = execution_words(&segment);
        let Some(program) = words.first().map(|w| w.rsplit('/').next().unwrap_or(w)) else { continue };
        if program == "cd" {
            if let Some(path) = words.get(1) {
                let expanded = path.strip_prefix('~').map(|rest| format!("/home/tree{rest}")).unwrap_or_else(|| path.to_string());
                let next = Path::new(&expanded);
                active = normalize(&if next.is_absolute() { next.to_path_buf() } else { active.join(next) });
            }
            continue;
        }
        let mut quote = None;
        let mut escaped = false;
        for (idx, ch) in segment.char_indices() {
            if escaped { escaped = false; continue; }
            if ch == '\\' && quote != Some('\'') { escaped = true; continue; }
            if let Some(delimiter) = quote {
                if ch == delimiter { quote = None; }
                continue;
            }
            if ch == '\'' || ch == '"' { quote = Some(ch); continue; }
            if ch != '>' { continue; }
            let target = segment[idx + 1..].trim_start_matches(['>', '|', '&']).trim_start().split_whitespace().next().unwrap_or("");
            if ai_path(target, &active) { return true; }
        }
        let mutating = matches!(program, "tee" | "cp" | "mv" | "install" | "touch" | "rm" | "truncate" | "apply_patch" | "patch" | "rustfmt")
            || (program == "sed" && words.iter().any(|w| w.starts_with("-i")))
            || (program == "perl" && words.iter().any(|w| w.starts_with("-pi") || w.starts_with("-0pi")))
            || (program == "cargo" && words.iter().any(|w| matches!(*w, "fmt" | "fix")))
            || (program == "git" && words.iter().any(|w| matches!(*w, "apply" | "restore" | "checkout" | "reset" | "clean")));
        if mutating && (active.starts_with("/home/tree/ai") || words.iter().skip(1).any(|w| ai_path(w, &active))) { return true; }
    }
    false
}

fn sibling_grist_creation_denial(event: &Value) -> Option<String> {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return None; }
    let cwd = normalize(Path::new(event.get("cwd")?.as_str()?));
    let current = cwd.strip_prefix(EXTENSIONS).ok()?.components().next()?.as_os_str();
    let command = event.pointer("/tool_input/command")?.as_str()?;
    for segment in segments(&without_heredoc_bodies(command)) {
        let raw_words: Vec<&str> = segment.split_whitespace().collect();
        let Some(exe_at) = raw_words.iter().position(|word| *word == "grist" || word.ends_with("/grist")) else { continue };
        let words = &raw_words[exe_at..];
        if !words.iter().any(|word| matches!(*word, "request-record" | "task-add")) { continue }
        for (idx, word) in words.iter().enumerate() {
            let value = if *word == "--project" { words.get(idx + 1).copied() }
                else { word.strip_prefix("--project=") };
            let Some(value) = value else { continue };
            let value = value.trim_matches(['\'', '"']);
            let project = normalize(&if Path::new(value).is_absolute() { PathBuf::from(value) } else { cwd.join(value) });
            if let Ok(relative) = project.strip_prefix(EXTENSIONS) {
                if relative.components().next().is_some_and(|part| part.as_os_str() != current) {
                    return Some("현재 세션 프로젝트 밖의 형제 extensions 프로젝트에는 grist request-record/task-add로 행을 만들 수 없어요. 제외하거나 차후로 미룬 범위는 기록하지 마세요.".into());
                }
            }
        }
    }
    None
}

fn core_tokens(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|word| word.trim_end_matches(['은', '는', '을', '를', '과', '와', '에', '만', '도', '의']).to_lowercase())
        .filter(|word| word.chars().count() >= 3)
        .filter(|word| !matches!(word.as_str(), "나중에" | "다음에" | "차후에" | "이전에" | "하기" | "먼저" | "하고" | "연동" | "구현" | "기능" | "이번" | "작업"))
        .collect()
}

fn deferred_only_tokens(prompt: &str) -> BTreeSet<String> {
    let future = ["나중에", "다음에", "차후에"].iter()
        .filter_map(|marker| prompt.find(marker).map(|at| (at, marker.len())))
        .min_by_key(|(at, _)| *at);
    let (deferred, current) = if let Some((at, len)) = future {
        let tail = &prompt[at + len..];
        let end = tail.find("먼저").or_else(|| tail.find(['.', '\n', '。'])).unwrap_or(tail.len());
        let clause = &tail[..end];
        if (clause.contains("할때") || clause.contains("할 때")) && !tail.contains("먼저") {
            return BTreeSet::new();
        }
        (clause, &tail[end..])
    } else if let Some(end) = prompt.find("하기 이전에").or_else(|| prompt.find("하기 전에 먼저")) {
        (&prompt[..end], &prompt[end..])
    } else {
        return BTreeSet::new();
    };
    let current = core_tokens(current);
    core_tokens(deferred).difference(&current).cloned().collect()
}

fn deferred_grist_denial(event: &Value) -> Option<String> {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return None; }
    let command = event.pointer("/tool_input/command")?.as_str()?;
    let transcript = std::fs::read_to_string(event.get("transcript_path")?.as_str()?).ok()?;
    let (prompt, _) = last_turn_prompt_and_replied(&transcript)?;
    let deferred = deferred_only_tokens(&prompt);
    if deferred.is_empty() { return None; }
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("."));
    for segment in segments(&without_heredoc_bodies(command)) {
        let words: Vec<&str> = segment.split_whitespace().collect();
        let Some(at) = words.iter().position(|word| *word == "grist" || word.ends_with("/grist")) else { continue };
        let Some(subcommand) = words.get(at + 1) else { continue };
        if !matches!(*subcommand, "request-record" | "task-add" | "task-goal") { continue; }
        let mut payload = String::new();
        let mut idx = at + 2;
        while idx < words.len() {
            let word = words[idx];
            if matches!(word, "--project" | "--goal-file" | "--input-file") {
                if matches!(word, "--goal-file" | "--input-file") {
                    if let Some(path) = words.get(idx + 1) {
                        let path = path.trim_matches(['\'', '"']);
                        let file = if Path::new(path).is_absolute() { PathBuf::from(path) } else { cwd.join(path) };
                        if let Ok(body) = std::fs::read_to_string(file) { payload.push_str(&body); payload.push(' '); }
                    }
                }
                idx += 2;
                continue;
            }
            if let Some(path) = word.strip_prefix("--goal-file=").or_else(|| word.strip_prefix("--input-file=")) {
                let path = path.trim_matches(['\'', '"']);
                let file = if Path::new(path).is_absolute() { PathBuf::from(path) } else { cwd.join(path) };
                if let Ok(body) = std::fs::read_to_string(file) { payload.push_str(&body); payload.push(' '); }
            } else if !word.starts_with("--project=") {
                payload.push_str(word);
                payload.push(' ');
            }
            idx += 1;
        }
        let payload_tokens = core_tokens(&payload);
        if let Some(token) = deferred.intersection(&payload_tokens).next() {
            return Some(format!("사용자가 '나중에/다음에/차후에'로 미룬 대상({token})이 요청·목표에 포함되었어요. 보류 구간을 분리해 이번 작업 대상만 기록하세요."));
        }
    }
    None
}

fn work_in_progress(state: &Path, session: &str, project: &Path) -> bool {
    let safe_session: String = session.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let session_rows: Vec<String> = std::fs::read_to_string(state.join("session_rows").join(format!("{safe_session}.json")))
        .ok().and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok()).unwrap_or_default()
        .iter().map(|v| v.to_string().trim_matches('"').to_string()).collect();
    let projects: serde_json::Map<String, Value> = std::fs::read_to_string(state.join("session_projects").join(format!("{safe_session}.json")))
        .ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let mut paths = vec![project.to_path_buf()];
    for path in projects.values().filter_map(Value::as_str).map(PathBuf::from) {
        if !paths.contains(&path) { paths.push(path); }
    }
    paths.iter().any(|path| {
        let open = flow_rows(path);
        if session_rows.iter().any(|id| open.contains(id)
            && projects.get(id).and_then(Value::as_str).is_none_or(|owner| Path::new(owner) == path)) { return true; }
        std::fs::read_dir(path.join(".ai/dispatch")).ok().is_some_and(|entries| entries.flatten().any(|entry| {
            let path = entry.path();
            path.extension().is_some_and(|ext| ext == "sh") && !path.with_extension("exit").exists()
                && path.with_extension("log").metadata().and_then(|m| m.modified()).ok()
                    .and_then(|modified| std::time::SystemTime::now().duration_since(modified).ok())
                    .is_some_and(|age| age <= std::time::Duration::from_secs(3600))
        }))
    })
}

fn status_question_denial(event: &Value, state_dir: &Path) -> Option<String> {
    let tp = event.get("transcript_path").and_then(Value::as_str).unwrap_or("");
    if tp.is_empty() { return None; }
    let text = std::fs::read_to_string(tp).ok()?;
    let (prompt, replied) = last_turn_prompt_and_replied(&text)?;
    let session = event.get("session_id").and_then(Value::as_str).unwrap_or("unknown");
    let project = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or(EXTENSIONS));
    ( !replied && decision_at(state_dir, session, &prompt).label == Some(Label::Question)
        && work_in_progress(state_dir, session, project))
        .then(|| "작업 과정 중 질문이 들어왔어요. 먼저 질문에 답하세요. 답변 전 도구 호출은 차단됐어요.".into())
}

fn n8n_write_command(command: &str) -> bool {
    segments(&without_heredoc_bodies(command)).iter().any(|segment| {
        let words = execution_words(segment);
        let lower = segment.to_ascii_lowercase();
        let program = words.first().map(|word| word.rsplit('/').next().unwrap_or(word)).unwrap_or("");
        let n8n_cli = program == "n8n" || (program == "docker" && words.get(1) == Some(&"exec")
            && words.iter().skip(3).any(|word| *word == "n8n" || word.ends_with("/n8n")));
        if lower.contains("import:workflow") || lower.contains("update:workflow") {
            return n8n_cli;
        }
        if matches!(program, "curl" | "wget") {
            let method = ["post", "put", "patch", "delete"].iter().any(|method| {
                lower.contains(&format!("-x {method}")) || lower.contains(&format!("-x{method}"))
                    || lower.contains(&format!("--request {method}")) || lower.contains(&format!("--request={method}"))
                    || lower.contains(&format!("--method {method}")) || lower.contains(&format!("--method={method}"))
            }) || lower.contains("--post-data") || lower.contains("--post-file") || lower.contains("--data") || lower.contains("-d ");
            // Webhook calls (/webhook/…) run a workflow; only workflow definition endpoints change it.
            return method && (lower.contains("/rest/workflows") || lower.contains("/api/v1/workflows"));
        }
        program == "sqlite3" && lower.contains("database.sqlite")
            && ["insert", "update", "delete", "replace"].iter().any(|verb| {
                lower.contains(&format!(" {verb} ")) || lower.contains(&format!("\"{verb} "))
                    || lower.contains(&format!("'{verb} "))
            })
    })
}

/// 261114 #948: 에이전트가 Grist 데이터 행(공모전 등)을 API로 지울 때도 열린 행과 당일 백업을 요구한다.
/// 작업관리 표(Table26) 정리는 grist 명령의 자체 기록 흐름이라 대상에서 뺀다.
fn grist_data_delete_command(command: &str) -> bool {
    segments(&without_heredoc_bodies(command)).iter().any(|segment| {
        let words = execution_words(segment);
        let lower = segment.to_ascii_lowercase();
        let program = words.first().map(|word| word.rsplit('/').next().unwrap_or(word)).unwrap_or("");
        if program == "grist" && words.get(1) == Some(&"delete") {
            return words.get(2).is_some_and(|table| *table != "Table26");
        }
        if matches!(program, "curl" | "wget") && lower.contains("/api/docs/") && !lower.contains("/tables/table26/") {
            let delete = lower.contains("-x delete") || lower.contains("--request delete") || lower.contains("--request=delete");
            let post_delete = lower.contains("/records/delete");
            let apply_remove = lower.contains("/apply") && lower.contains("removerecord");
            return delete || post_delete || apply_remove;
        }
        false
    })
}

#[cfg(test)]
mod grist_delete_tests {
    #[test]
    fn grist_data_delete_commands_are_detected_but_task_rows_and_reads_are_not() {
        assert!(super::grist_data_delete_command("~/ai/scripts/bin/grist delete Table7 --id 1405"));
        assert!(super::grist_data_delete_command("curl -s -X POST http://localhost:8484/api/docs/au1/tables/Table7/records/delete -d '[1405]'"));
        assert!(super::grist_data_delete_command("curl -s -X POST http://localhost:8484/api/docs/au1/apply -d '[[\"BulkRemoveRecord\",\"Table7\",[1405]]]'"));
        assert!(!super::grist_data_delete_command("~/ai/scripts/bin/grist delete Table26 --id 690 --project ."));
        assert!(!super::grist_data_delete_command("~/ai/scripts/bin/grist get Table7 --fields C"));
        assert!(!super::grist_data_delete_command("curl -s http://localhost:8484/api/docs/au1/tables/Table7/records"));
    }
}

fn n8n_kst_date(unix_seconds: i64) -> String {
    let days = (unix_seconds + 9 * 3600).div_euclid(86400);
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 { year += 1; }
    format!("{:02}{month:02}{day:02}", year.rem_euclid(100))
}

fn n8n_denial_at(event: &Value, state: &Path, backup_dir: &Path, unix_seconds: i64) -> Option<String> {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return None; }
    let command = event.pointer("/tool_input/command").and_then(Value::as_str)?;
    let grist_delete = grist_data_delete_command(command);
    if !n8n_write_command(command) && !grist_delete { return None; }
    let session = event.get("session_id").and_then(Value::as_str).unwrap_or("");
    let safe: String = session.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') { c } else { '_' }).collect();
    let rows: Vec<Value> = std::fs::read_to_string(state.join("session_rows").join(format!("{safe}.json")))
        .ok().and_then(|body| serde_json::from_str(&body).ok()).unwrap_or_default();
    let projects: serde_json::Map<String, Value> = std::fs::read_to_string(state.join("session_projects").join(format!("{safe}.json")))
        .ok().and_then(|body| serde_json::from_str(&body).ok()).unwrap_or_default();
    let active = rows.iter().rev().find_map(|value| {
        let id = value.as_str().map(str::to_owned).or_else(|| value.as_i64().map(|n| n.to_string()))?;
        let project = PathBuf::from(projects.get(&id)?.as_str()?);
        let cached = project.join(format!(".ai/gate/{id}.row"));
        refresh_row_cache(&project, &id);
        let row = std::fs::read_to_string(cached).unwrap_or_default();
        ["H=작업", "H=교정", "H=진단", "H=검증"].iter().any(|stage| row.starts_with(stage)).then_some((id, project))
    });
    let Some((id, project)) = active else {
        if grist_delete {
            return Some("Grist 데이터 행 삭제는 이 세션에 $H=작업·교정·진단·검증인 열린 행이 있어야 해요. 먼저 grist task-ready로 행을 여세요.".into());
        }
        return Some("n8n 워크플로 쓰기는 이 세션에 $H=작업·교정·진단·검증(배포)인 열린 행이 있어야 해요. 먼저 grist task-ready로 행을 여세요.".into());
    };
    let today = n8n_kst_date(unix_seconds);
    if grist_delete {
        let grist_backup = std::fs::read_dir(backup_dir).ok().into_iter().flatten().filter_map(Result::ok).any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            entry.path().is_file() && name.starts_with(&format!("{today}-grist-")) && name.ends_with(".json")
        });
        if !grist_backup {
            return Some(format!("Grist 행 삭제 전에 오늘(KST) 대상 테이블 백업이 필요해요. ~/ai/scripts/bin/grist get <Table> > {}/{today}-grist-<Table>.json", backup_dir.display()));
        }
        return None;
    }
    let backup = std::fs::read_dir(backup_dir).ok().into_iter().flatten().filter_map(Result::ok).any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        entry.path().is_file() && name.starts_with(&format!("{today}-")) && name.ends_with(".json")
    });
    if !backup {
        return Some(format!("n8n 쓰기 전에 오늘(KST) 워크플로 export 백업이 필요해요. n8n export:workflow --id=<workflowId> --output={}/{today}-<workflowId>.json", backup_dir.display()));
    }
    let marker = project.join(format!(".ai/gate/{id}.n8n"));
    if let Err(error) = std::fs::write(&marker, format!("{today}\n")) {
        return Some(format!("n8n 검증 표식 {} 생성 실패: {error}", marker.display()));
    }
    None
}

fn stop_command(command: &str) -> bool {
    segments(&without_heredoc_bodies(command)).iter().any(|segment| {
        let words = execution_words(segment);
        let Some(first) = words.first() else { return false };
        let exe = first.rsplit('/').next().unwrap_or(first);
        if ["kill", "pkill", "killall"].contains(&exe) { return true; }
        if exe != "tmux" { return false; }
        words.iter().any(|word| ["kill-pane", "kill-window", "kill-session"].contains(word))
            || (words.iter().any(|word| *word == "send-keys")
                && words.iter().any(|word| *word == "C-c" || *word == "SIGINT"))
    })
}

fn mark_external_stop(event: &Value) {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return; }
    let Some(command) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return };
    if !stop_command(command) { return; }
    let home = std::env::var_os("HOME").unwrap_or_default();
    let binary = PathBuf::from(home).join("ai/scripts/bin/cli_watch_blackbox");
    if !binary.is_file() { return; }
    let actor = event.get("session_id").and_then(Value::as_str).unwrap_or("");
    let _ = Command::new(binary).args(["mark", "--cause", "other_session", "--actor", actor, "--detail", command])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).map_err(|e| e.to_string())?;
    let event: Value = normalize_codex_event(serde_json::from_str(&input).map_err(|e| e.to_string())?);
    let state = prompt_decision::decision_state_dir();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?.as_secs() as i64;
    if let Some(reason) = n8n_denial_at(&event, &state, Path::new("/mnt/d/n8n/backup"), now) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if let Some(reason) = hidden_cdp_denial(&event, std::env::var("T26_ROLE").ok().as_deref()) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if claude_agent_denied(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"토큰 절감: 메인은 Claude 서브에이전트를 부르지 않습니다. 조사·구현은 dispatch-agent --agent codex로 맡기세요."}}));
        return Ok(());
    }
    if monitor_denied(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"토큰 절감: 진행 감시는 Monitor 스트리밍 대신 완료 조건까지 기다리는 단일 명령(dispatch-agent 대기·until 루프)으로 최종 결과만 읽으세요."}}));
        return Ok(());
    }
    if let Some(reason) = role_denial(&event, std::env::var("T26_ROLE").ok().as_deref()) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    let state_dir = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state/main-edit-gate");
    if std::env::var("T26_ROLE").ok().as_deref() == Some("improver") {
        if let Some(reason) = improver_denial(&event) {
            println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        }
        return Ok(());
    }
    if std::env::var("T26_ROLE").ok().as_deref() == Some("verifier") { return Ok(()); }
    if std::env::var("T26_ROLE").ok().as_deref() == Some("worker") { return Ok(()); }
    if let Some(reason) = main_tool_feature_denial(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if let Some(reason) = sibling_grist_creation_denial(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if let Some(reason) = deferred_grist_denial(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if let Some(source) = generated_write(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":format!("생성본은 직접 수정할 수 없습니다. 원본 {source}을 고친 뒤 agent-rules sync를 실행하세요.")}}));
        return Ok(());
    }
    if let Some(reason) = status_question_denial(&event, &prompt_decision::decision_state_dir()) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if let Some(reason) = ai_write_before_intervention_row(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if let Some(reason) = ai_write_before_root_row(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if let Some(reason) = process_correction_denial(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if source_slice_denied(&event, &state_dir) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":SOURCE_SLICE_REASON}}));
        return Ok(());
    }
    if image_read_denied(&event, &state_dir) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":format!("메인 세션의 이미지 읽기는 세션당 {IMAGE_LIMIT}장까지입니다. 화면 판정은 dispatch-agent 구현자에게 실측 조건(값·크기·naturalWidth)으로 맡기고 exit·last만 보세요.")}}));
        return Ok(());
    }
    if let Some(reason) = version_bump_denial(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if target(&event).is_some() {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"코드 구현은 대행자(dispatch-agent)가 합니다. 메인은 규칙·Skill 문서만 직접 고칩니다."}}));
    }
    mark_external_stop(&event);
    Ok(())
}

fn main() {
    if let Err(error) = run() { eprintln!("main-edit-gate: {error}"); }
}

#[cfg(test)]
mod tests {
    #[test]
    fn question_blocks_tools_only_during_active_work() {
        let dir = std::path::PathBuf::from("/home/tree/extensions/test/tmp").join(format!("meg-question-{}", std::process::id()));
        let state = dir.join("state");
        let project = dir.join("project");
        std::fs::create_dir_all(state.join("session_rows")).unwrap();
        std::fs::create_dir_all(project.join(".ai/changes")).unwrap();
        std::fs::write(state.join("session_rows/s.json"), "[23,45,67]").unwrap();
        std::fs::write(project.join(".ai/changes/261114.task.md"), "# 작업\n- [⌨️] 진행 (#45)\n").unwrap();
        let transcript = dir.join("transcript.jsonl");
        let prompt = "진행 상황을 알려줘";
        let write_prompt = |reply: bool| std::fs::write(&transcript,
            format!("{}{}", serde_json::json!({"type":"USER_INPUT","content":prompt}),
                if reply { "\n{\"type\":\"PLANNER_RESPONSE\",\"content\":\"진행 중이에요.\"}\n" } else { "\n" })).unwrap();
        write_prompt(false);
        let event = serde_json::json!({"tool_name":"Bash","tool_input":{"command":"rg -n foo src"},"session_id":"s","cwd":project,"transcript_path":transcript});
        for (index, label) in [Some(super::Label::Question), Some(super::Label::Improvement), Some(super::Label::Feature), Some(super::Label::Resolution), None].into_iter().enumerate() {
            let prompt = format!("case-{index}");
            std::fs::write(&transcript, format!("{}\n", serde_json::json!({"type":"USER_INPUT","content":prompt}))).unwrap();
            super::prompt_decision::decision_with(&state, "s", &prompt, |_| label);
            assert_eq!(super::status_question_denial(&event, &state).is_some(), label == Some(super::Label::Question));
        }
        super::prompt_decision::decision_with(&state, "s", prompt, |_| Some(super::Label::Question));
        write_prompt(false);
        assert!(super::status_question_denial(&event, &state).unwrap().contains("먼저 질문에 답하세요"));
        write_prompt(true);
        assert!(super::status_question_denial(&event, &state).is_none());
        std::fs::write(project.join(".ai/changes/261114.task.md"), "# 작업\n").unwrap();
        write_prompt(false);
        assert!(super::status_question_denial(&event, &state).is_none());
        eprintln!("[verify-step] range-check start=23 mid=45 end=67");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn ai_write_redirects_ignore_quoted_operators_but_check_quoted_targets() {
        let event = |command: &str| serde_json::json!({
            "tool_name": "Bash",
            "cwd": "/home/tree/ai/scripts",
            "session_id": "meg-write-redirect-regression",
            "tool_input": {"command": command},
        });
        let empty_state = std::env::temp_dir().join("meg-write-redirect-regression-no-rows");
        for command in [
            "ls -la --time-style=long-iso /home/tree/.codex/sessions/2026/11/13 | awk 'NR>3{print $7}' | cut -c1-2",
            "rg -n \"fn own_changes\" -A 70 task-complete/src/main.rs | rg -n \"free|> |cargo\" | head -40",
            "rg -n 'a>b' main-edit-gate/src/main.rs",
        ] {
            assert!(!super::writes_ai(&event(command)), "allowed: {command}");
            assert!(super::ai_write_before_root_row_at(&event(command), &empty_state).is_none(), "allowed: {command}");
        }
        for command in [
            "echo x > ~/ai/x",
            "printf a > \"$HOME/ai/x\"",
            "sed -i 's/a/b/' ~/ai/rules/Agents.md",
        ] {
            assert!(super::writes_ai(&event(command)), "denied: {command}");
            let reason = super::ai_write_before_root_row_at(&event(command), &empty_state).expect("PreToolUse deny reason");
            assert!(reason.contains("~/ai를 수정할 수 없어요"), "denied: {command}");
        }
        println!("[verify-step] range-check start=3 mid=7 end=12");
    }

    #[test]
    fn blackbox_query_is_allowed_and_external_stop_commands_are_recognized() {
        for command in ["kill -INT 303", "pkill -f codex", "killall codex", "tmux kill-pane -t %305", "tmux kill-window -t 308", "tmux kill-session -t work", "tmux send-keys -t %311 C-c"] {
            assert!(super::stop_command(command), "{command}");
        }
        for command in ["echo kill -INT 303", "tmux list-panes", "rg -n 'kill' src"] {
            assert!(!super::stop_command(command), "{command}");
        }
        println!("[verify-step] range-check start=303 mid=308 end=311");
    }
    #[test]
    fn ai_write_requires_session_root_row_before_first_change() {
        let state = std::env::temp_dir().join(format!("meg-root-row-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&state);
        std::fs::create_dir_all(state.join("session_rows")).unwrap();
        std::fs::create_dir_all(state.join("session_projects")).unwrap();
        let event = |tool: &str, path: &str| serde_json::json!({"tool_name":tool,"cwd":"/home/tree/extensions","session_id":"s2","tool_input": if tool == "Bash" { serde_json::json!({"command":path}) } else { serde_json::json!({"file_path":path}) }});
        let write = event("Edit", "/home/tree/ai/scripts/main-edit-gate/src/main.rs");
        assert!(super::ai_write_before_root_row_at(&write, &state).unwrap().contains("request-record → task-add"));
        assert!(super::ai_write_before_root_row_at(&event("Bash", "rg -n foo ~/ai/scripts/main-edit-gate/src/main.rs"), &state).is_none());
        std::fs::write(state.join("session_rows/s2.json"), "[853]").unwrap();
        std::fs::write(state.join("session_projects/s2.json"), "{\"853\":\"/home/tree/extensions/grist_video_maker\"}").unwrap();
        assert!(super::ai_write_before_root_row_at(&write, &state).is_some());
        std::fs::write(state.join("session_rows/s2.json"), "[853,881]").unwrap();
        std::fs::write(state.join("session_projects/s2.json"), "{\"853\":\"/home/tree/extensions/grist_video_maker\",\"881\":\"/home/tree/extensions\"}").unwrap();
        assert!(super::ai_write_before_root_row_at(&write, &state).is_none());
        let _ = std::fs::remove_dir_all(state);
    }

    #[test]
    fn intervention_ai_write_requires_new_root_row() {
        let state = std::env::temp_dir().join(format!("meg-intervention-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&state);
        std::fs::create_dir_all(state.join("session_rows")).unwrap();
        std::fs::create_dir_all(state.join("session_projects")).unwrap();
        let transcript = state.join("transcript.jsonl");
        std::fs::write(&transcript, "{\"type\":\"USER_INPUT\",\"content\":\"작업 과정 오류를 수정해\"}\n").unwrap();
        std::fs::write(state.join("session_rows/s1.json"), "[860]").unwrap();
        std::fs::write(state.join("session_projects/s1.json"), "{\"860\":\"/home/tree/extensions\"}").unwrap();
        super::prompt_decision::decision_with(&state, "s1", "작업 과정 오류를 수정해", |_| Some(super::Label::Improvement));
        let event = |tool: &str, command: &str| serde_json::json!({"tool_name":tool,"cwd":"/home/tree/extensions","session_id":"s1","transcript_path":transcript,"tool_input": if tool == "Bash" { serde_json::json!({"command":command}) } else { serde_json::json!({"file_path":command}) }});
        assert!(super::ai_write_before_intervention_row_at(&event("Bash", "rg -n foo ~/ai/scripts/main-edit-gate/src/main.rs"), &state).is_none());
        assert!(super::ai_write_before_intervention_row_at(&event("Edit", "/home/tree/ai/scripts/main-edit-gate/src/main.rs"), &state).unwrap().contains("request-record → task-add"));
        assert!(super::ai_write_before_intervention_row_at(&event("Bash", "sed -i 's/a/b/' ~/ai/scripts/main-edit-gate/src/main.rs"), &state).is_some());
        assert!(super::ai_write_before_intervention_row_at(&event("Bash", "cd ~/ai/scripts; sed -i 's/a/b/' main-edit-gate/src/main.rs"), &state).is_some());
        assert!(super::ai_write_before_intervention_row_at(&event("Bash", "cd ~/ai/scripts; cargo fmt"), &state).is_some());
        assert!(super::ai_write_before_intervention_row_at(&event("Bash", "apply_patch *** Begin Patch\n*** Update File: /home/tree/ai/scripts/main-edit-gate/src/main.rs"), &state).is_some());
        std::fs::write(state.join("session_rows/s1.json"), "[860, 872]").unwrap();
        std::fs::write(state.join("session_projects/s1.json"), "{\"860\":\"/home/tree/extensions\",\"872\":\"/home/tree/extensions/sibling\"}").unwrap();
        assert!(super::ai_write_before_intervention_row_at(&event("Edit", "/home/tree/ai/scripts/main-edit-gate/src/main.rs"), &state).is_some());
        std::fs::write(state.join("session_projects/s1.json"), "{\"860\":\"/home/tree/extensions\",\"872\":\"/home/tree/extensions\"}").unwrap();
        assert!(super::ai_write_before_intervention_row_at(&event("Edit", "/home/tree/ai/scripts/main-edit-gate/src/main.rs"), &state).is_none());
        std::fs::remove_dir_all(state).unwrap();
    }


    #[test]
    fn process_correction_allows_fix_and_blocks_product_reentry() {
        let dir = std::env::temp_dir().join(format!("meg-process-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("TASK_COMPLETE_STATE_DIR", &dir);
        let transcript = dir.join("transcript.jsonl");
        std::fs::write(&transcript, "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>과정 오류를 왜 수정하지 않았어?</USER_REQUEST>\"}\n").unwrap();
        super::prompt_decision::decision_with(&dir, "unknown", "과정 오류를 왜 수정하지 않았어?", |_| Some(super::Label::Improvement));
        let event = |command: &str| serde_json::json!({"tool_name":"Bash","tool_input":{"command":command},"transcript_path":transcript});
        assert!(super::process_correction_denial(&event("rg -n foo /home/tree/ai/scripts/task-complete/src/main.rs")).is_none());
        assert!(super::process_correction_denial(&event("rg -n foo /home/tree/ai/scripts/dispatch-agent/src/main.rs")).is_none());
        assert!(super::process_correction_denial(&event("dispatch-agent --role improver --row 860")).is_none());
        assert!(super::process_correction_denial(&event("/home/tree/ai/scripts/bin/task-complete --hook <<'JSON'\n{\"cwd\":\"/home/tree/extensions\",\"message\":\"제품 작업을 재개하지 않았어요\"}\nJSON")).is_none());
        assert!(super::process_correction_denial(&event("python3 - <<'PY'\ntext = '/home/tree/extensions/grist_mindmap/src/main.js'\nPY")).is_none());
        assert!(super::process_correction_denial(&event("python3 - <<'PY'\nprint('ok')\nPY\nrg -n foo /home/tree/extensions/grist_mindmap/src/main.js")).unwrap().contains("manage-intervention"));
        let ai_diff = super::normalize_codex_event(serde_json::json!({"tool_name":"exec_command","tool_input":{"cmd":"git diff -- scripts/dispatch-agent/src/main.rs","workdir":"/home/tree/ai"},"cwd":"/home/tree/extensions","transcript_path":transcript}));
        assert!(super::process_correction_denial(&ai_diff).is_none());
        assert!(super::process_correction_denial(&event("ls -la ~/x/hooks 2>/dev/null | head -3")).is_none());
        assert!(super::process_correction_denial(&event("rg -n x ~/y.json 2>/dev/null >/tmp/out")).is_none());
        assert!(super::process_correction_denial(&event("cargo build --release -p dispatch-agent 2>&1 | tail -n1")).is_none());
        assert!(super::process_correction_denial(&event("rg -n \"codex/hooks/|join\" ~/ai/scripts/agent-rules/src")).is_none());
        assert!(super::process_correction_denial(&event("rg -n foo /home/tree/extensions/obsidian_daily_manager/.ai/changes")).is_none());
        assert!(super::process_correction_denial(&event("rg -n foo /home/tree/extensions/grist_mindmap/src/main.js")).unwrap().contains("manage-intervention"));
        println!("[verify-step] range-check start=2 mid=5 end=9");
        assert!(super::process_correction_denial(&event("rg -n foo grist_video_maker/AGENTS.md")).is_some());
        // relative words that name nothing on disk are not product paths (sed script, URL, tool path from a product cwd)
        assert!(super::process_correction_denial(&event("rg -no '^[A-Z_]*MODEL' ~/ai/x.env | sed 's/=.*/=x/'")).is_none());
        assert!(super::process_correction_denial(&event("curl -s localhost:11434/api/tags | head -c 600")).is_none());
        assert!(super::process_correction_denial(&event("rg -n fn scripts/dispatch-agent/src/main.rs")).is_none());
        assert!(super::process_correction_denial(&event("dispatch-agent --role verifier --row 853")).is_some());
        // the spaced-out wording is the same turn kind; task-complete reads the state this call wrote
        std::fs::write(&transcript, "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>~/Report.md를 보고 작업과정의 문제를 개선해</USER_REQUEST>\"}\n").unwrap();
        super::prompt_decision::decision_with(&dir, "unknown", "~/Report.md를 보고 작업과정의 문제를 개선해", |_| Some(super::Label::Improvement));
        assert!(super::process_correction_denial(&event("rg -n foo grist_video_maker/AGENTS.md")).is_some());
        assert!(std::fs::read_to_string(dir.join("process_turn/unknown")).unwrap().lines().nth(1) == Some("개선"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn grist_creation_stays_in_session_project() {
        let event = |command: &str| serde_json::json!({"tool_name":"Bash","cwd":"/home/tree/extensions/blender_scene_creator","tool_input":{"command":command}});
        assert!(super::sibling_grist_creation_denial(&event("~/ai/scripts/bin/grist request-record --project /home/tree/extensions/grist_video_maker --text x")).is_some());
        assert!(super::sibling_grist_creation_denial(&event("env X=1 ~/ai/scripts/bin/grist task-add --project=../grist_video_maker --type x")).is_some());
        assert!(super::sibling_grist_creation_denial(&event("cd /tmp; ~/ai/scripts/bin/grist task-add --project /home/tree/extensions/grist_video_maker")).is_some());
        assert!(super::sibling_grist_creation_denial(&event("~/ai/scripts/bin/grist task-add --project /home/tree/extensions/blender_scene_creator")).is_none());
        assert!(super::sibling_grist_creation_denial(&event("~/ai/scripts/bin/grist task-show 830 --project /home/tree/extensions/grist_video_maker")).is_none());
    }

    #[test]
    fn deferred_targets_are_rejected_from_grist_records_and_goals() {
        let dir = std::env::temp_dir().join(format!("meg-deferred-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join("transcript.jsonl");
        let goals = dir.join("goals.txt");
        let check = |prompt: &str, command: &str| {
            std::fs::write(&transcript, format!("{}\n", json!({"type":"USER_INPUT","content":format!("<USER_REQUEST>{prompt}</USER_REQUEST>")}))).unwrap();
            let event = json!({"tool_name":"Bash","cwd":dir,"transcript_path":transcript,"tool_input":{"command":command}});
            super::deferred_grist_denial(&event)
        };
        let first = "차후에 grist_video_creator와 연동하기 이전에 먼저 eagle과 통합되는 기능을 구현하려고 해";
        assert!(check(first, "grist request-record --project . --summary 'grist_video_creator 연동'").unwrap().contains("grist_video_creator"));
        assert!(check(first, "grist request-record --project . --summary 'eagle 통합'").is_none());
        std::fs::write(&goals, "1. [연결] grist_video_creator 연동 ▷ 성공").unwrap();
        assert!(check(first, &format!("grist task-add --project . --goal-file {} --summary 'eagle 통합'", goals.display())).is_some());
        std::fs::write(&goals, "1. [기능] eagle 통합 ▷ 성공").unwrap();
        assert!(check(first, &format!("grist task-goal 830 --project . --goal-file {} --summary 'eagle 통합'", goals.display())).is_none());
        let second = "나중에 대시보드 연동은 하고 먼저 로컬 저장만 구현해";
        assert!(check(second, "grist task-goal 830 --project . --summary '대시보드 저장'").unwrap().contains("대시보드"));
        assert!(check(second, "grist task-goal 830 --project . --summary '로컬 저장'").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn improver_edits_ai_sources_reads_dispatch_logs_and_never_product_code_or_browser() {
        let edit = |file: &str| json!({"tool_name":"Edit","cwd":"/home/tree/ai","tool_input":{"file_path":file}});
        let bash = |command: &str| json!({"tool_name":"Bash","cwd":"/home/tree/ai","tool_input":{"command":command}});
        assert_eq!(improver_denial(&edit("/home/tree/ai/skills/role-agent/SKILL.md")), None);
        assert_eq!(improver_denial(&edit("/home/tree/ai/scripts/grist/src/main.rs")), None);
        assert_eq!(improver_denial(&edit("/home/tree/extensions/app/src/main.ts")).as_deref(), Some(IMPROVER_PRODUCT_REASON));
        assert!(improver_denial(&edit(&format!("/home/tree/.{}/{}.md", "claude", "CLAUDE"))).unwrap().contains("생성본"));
        assert_eq!(improver_denial(&bash("node ~/ai/skills/access-stagehand/scripts/stagehand.mjs")).as_deref(), Some(IMPROVER_BROWSER_REASON));
        assert_eq!(role_denial(&bash("tail -n 20 /home/tree/extensions/.ai/dispatch/12.log"), Some("improver")), None);
    }

    #[test]
    fn workflow_roots_cover_extensions_and_project() {
        let write = |file: &str| json!({"tool_name":"Write","cwd":"/","tool_input":{"file_path":file}});
        assert!(target(&write("/home/tree/project/color_map/src/main.ts")).is_some());
        assert!(target(&write("/home/tree/extensions/app/src/a.ts")).is_some());
        assert!(target(&write("/home/tree/other/src/a.ts")).is_none());
        assert!(target(&write("/home/tree/projects/x/src/a.ts")).is_none());
    }

    use super::*;

    #[test]
    fn second_version_bump_in_open_flow_is_denied() {
        let project = Path::new(EXTENSIONS).join(format!("win_key_fixer/test/tmp/zz_meg_test_{}", std::process::id()));
        std::fs::create_dir_all(project.join(".ai/changes")).unwrap();
        std::fs::create_dir_all(project.join(".ai/gate")).unwrap();
        std::fs::write(project.join("AGENTS.md"), "").unwrap();
        std::fs::write(project.join(".ai/changes/261107.a.md"), "# 작업\n1. [🌍] x (#9001)\n\n# 완료\n").unwrap();
        std::fs::write(project.join("package.json"), "{\n  \"version\": \"0.3.6\"\n}").unwrap();
        std::fs::write(project.join("Cargo.toml"), "[package]\nversion = \"0.3.7\"\n").unwrap();
        std::fs::write(project.join(".ai/gate/9001.row"), "H=검증 | I=").unwrap();
        let bash = |command: &str| json!({"tool_name":"Bash","cwd":project.to_string_lossy(),"tool_input":{"command":command}});
        let bump = "sed -i '4s/\"0.3.6\"/\"0.3.7\"/' package.json";
        assert!(version_bump_denial(&bash(bump)).is_none(), "first bump allowed");
        std::fs::write(project.join("package.json"), "{\n  \"version\": \"0.3.7\"\n}").unwrap();
        assert!(version_bump_denial(&bash(bump)).is_some(), "second bump in open flow denied");
        assert!(version_bump_denial(&bash("sed -i '4s/0.0.38/0.0.39/' sub/manifest.json")).is_none(), "other version file has its own flow");
        assert!(version_bump_denial(&bash("sed -n 4p package.json")).is_none(), "read is not a bump");
        std::fs::write(project.join(".ai/gate/9001.row"), "H=확인 | I=").unwrap();
        assert!(version_bump_denial(&bash(bump)).is_none(), "bump allowed after flow rows confirmed");
        std::fs::write(project.join(".ai/changes/261107.a.md"), "# 작업\n1. [🌍] x (#9001)\n2. [🌍] y (#9002)\n").unwrap();
        std::fs::write(project.join(".ai/gate/9002.row"), "H=검증 | I=").unwrap();
        std::fs::write(project.join(".ai/gate/version-flow-Cargo.toml"), "9003").unwrap();
        let both = "sed -i '4s/0.3.7/0.3.8/' package.json; sed -i '3s/0.3.7/0.3.8/' Cargo.toml";
        assert!(version_bump_denial(&bash(both)).is_some(), "denied on the second file");
        assert_eq!(std::fs::read_to_string(project.join(".ai/gate/version-flow-package.json")).unwrap(), "version=0.3.7\n9001", "denied command leaves earlier file state untouched");
        std::fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn scaffolded_first_version_does_not_consume_release_bump() {
        let project = Path::new(EXTENSIONS).join(format!("win_key_fixer/test/tmp/zz_meg_scaffold_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        std::fs::create_dir_all(project.join(".ai/changes")).unwrap();
        std::fs::create_dir_all(project.join(".ai/gate")).unwrap();
        std::fs::write(project.join("AGENTS.md"), "").unwrap();
        std::fs::write(project.join(".ai/changes/261110.a.md"), "# 작업\n1. [🌍] x (#9001)\n").unwrap();
        let event = |command: &str| json!({"tool_name":"Bash","cwd":project.to_string_lossy(),"tool_input":{"command":command}});
        let command = "sed -i '3s/0.0.1/0.0.2/' Cargo.toml";
        assert!(version_bump_denial(&event(command)).is_none());
        assert!(!project.join(".ai/gate/version-flow-Cargo.toml").exists());
        std::fs::write(project.join("Cargo.toml"), "[package]\nversion = \"0.0.1\"\n").unwrap();
        std::fs::write(project.join(".ai/gate/9001.row"), "H=검증 | I=").unwrap();
        assert!(version_bump_denial(&event(command)).is_none());
        std::fs::write(project.join("Cargo.toml"), "[package]\nversion = \"0.0.2\"\n").unwrap();
        assert!(version_bump_denial(&event(command)).is_some());
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn version_bump_waits_for_every_row_submit() {
        let project = Path::new(EXTENSIONS).join(format!("win_key_fixer/test/tmp/zz_meg_stage_{}", std::process::id()));
        std::fs::create_dir_all(project.join(".ai/changes")).unwrap();
        std::fs::create_dir_all(project.join(".ai/gate")).unwrap();
        std::fs::write(project.join("AGENTS.md"), "").unwrap();
        std::fs::write(project.join(".ai/changes/261113.a.md"), "# 작업\n1. x (#9001)\n2. y (#9002)\n").unwrap();
        std::fs::write(project.join(".ai/gate/9001.row"), "H=검증 | I=").unwrap();
        std::fs::write(project.join(".ai/gate/9002.row"), "H=작업 | I=").unwrap();
        std::fs::write(project.join("Cargo.toml"), "[package]\nversion = \"0.0.5\"\n").unwrap();
        let event = json!({"tool_name":"Bash","cwd":project.to_string_lossy(),"tool_input":{"command":"sed -i 's/0.0.5/0.0.6/' Cargo.toml"}});
        assert!(version_bump_denial(&event).unwrap().contains("#9002"));
        assert!(!project.join(".ai/gate/version-flow-Cargo.toml").exists());
        std::fs::write(project.join(".ai/gate/9002.row"), "H=검증 | I=").unwrap();
        assert!(version_bump_denial(&event).is_none());
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn heredoc_text_naming_a_generated_file_is_not_a_write() {
        let bash = |command: &str| json!({"tool_name":"Bash","cwd":"/tmp","tool_input":{"command":command}});
        let home = std::env::var("HOME").unwrap();
        let target = format!("{home}/.claude/settings.json");
        assert!(generated_write(&bash(&format!("cat > /tmp/prompt <<'EOF'\ncheck {target}\nEOF\ndispatch-agent --row 1"))).is_none());
        assert!(generated_write(&bash(&format!("cat > {target} <<'EOF'\n{{}}\nEOF"))).is_some());
        assert!(generated_write(&bash(&format!("rg -n ask {target} 2>/dev/null | head -3"))).is_none(), "stderr redirect on a read is not a write");
        assert!(generated_write(&bash(&format!("echo x >> {target}"))).is_some());
        assert!(generated_write(&bash(&format!("python3 - <<'EOF'\nopen('{target}','w')\nEOF"))).is_some());
        assert!(generated_write(&bash(&format!("python3 - <<'PY'\ntext = '{target}'\nassert '{target}' in text\nPY"))).is_none());
        assert!(generated_write(&bash(&format!("python3 - <<'PY'\nassert \"open('{target}', 'w')\"\nPY"))).is_none());
        assert!(generated_write(&bash(&format!("python3 - <<'PY'\nfrom pathlib import Path\nPath('{target}').write_text('x')\nPY"))).is_some());
        assert!(generated_write(&bash(&format!("node <<'JS'\nfs.writeFileSync('{target}', 'x')\nJS"))).is_some());
    }

    #[test]
    fn generated_files_deny_edit_write_and_bash_but_allow_sync() {
        for (file, source) in [
            ("/home/tree/.claude/CLAUDE.md", "~/ai/rules/Agents.md"),
            ("/home/tree/.claude/settings.json", "~/ai/codex/hook-manifest.json"),
            ("/home/tree/.codex/AGENTS.md", "~/ai/rules/Agents.md"),
            ("/home/tree/.codex/hooks.json", "~/ai/codex/hook-manifest.json"),
            ("/home/tree/.gemini/GEMINI.md", "~/ai/rules/Agents.md"),
            ("/home/tree/.gemini/antigravity-cli/settings.json", "~/ai/codex/hook-manifest.json"),
        ] {
            for tool in ["Edit", "Write"] {
                let event = json!({"tool_name":tool,"cwd":"/home/tree/extensions","tool_input":{"file_path":file}});
                assert_eq!(generated_write(&event), Some(source));
            }
            let event = json!({"tool_name":"Bash","cwd":"/home/tree/extensions","tool_input":{"command":format!("tee {file} < /tmp/x")}});
            assert_eq!(generated_write(&event), Some(source));
        }
        assert_eq!(generated_write(&json!({"tool_name":"Bash","cwd":"/home/tree/extensions","tool_input":{"command":"agent-rules sync --force"}})), None);
        let codex = normalize_codex_event(json!({"tool_name":"exec_command","cwd":"/home/tree/extensions","tool_input":{"cmd":"tee ~/.codex/hooks.json"}}));
        assert_eq!(generated_write(&codex), Some("~/ai/codex/hook-manifest.json"));
    }

    #[test]
    fn claude_agents_are_denied_and_other_tools_pass() {
        assert!(claude_agent_denied(&json!({"tool_name":"Agent"})));
        assert!(claude_agent_denied(&json!({"tool_name":"Task"})));
        assert!(!claude_agent_denied(&json!({"tool_name":"Bash"})));
        assert!(monitor_denied(&json!({"tool_name":"Monitor"})));
        assert!(!monitor_denied(&json!({"tool_name":"Bash"})));
    }

    #[test]
    fn source_read_denies_wide_main_reads() {
        let file = "/home/tree/extensions/app/src/resolveOpen.ts";
        let read = |limit: Option<u64>| json!({"tool_name":"Read","cwd":"/home/tree/extensions/app","tool_input":{"file_path":file,"limit":limit}});
        assert_eq!(role_denial(&read(None), None), Some(SOURCE_READ_REASON));
        assert_eq!(role_denial(&read(Some(40)), None), Some(SOURCE_READ_REASON));
        assert_eq!(role_denial(&read(Some(20)), None), None);
        let bash = |command: &str| json!({"tool_name":"Bash","cwd":"/home/tree/extensions/app","tool_input":{"command":command}});
        for command in [
            "sed -n '55,100p' src/resolveOpen.ts",
            "cat src/resolveOpen.ts",
            "head src/resolveOpen.ts",
            "tail src/resolveOpen.ts",
            "head -n 30 src/resolveOpen.ts",
            "tail -n 30 src/resolveOpen.ts",
            "rg -n x src/resolveOpen.ts | cat src/resolveOpen.ts",
            "wc src/resolveOpen.ts && sed -n '1,12p;30,40p' src/resolveOpen.ts",
        ] {
            assert_eq!(role_denial(&bash(command), None), Some(SOURCE_READ_REASON), "{command}");
        }
    }

    #[test]
    fn source_read_allows_workflow_tool_reads_after_tilde_cd() {
        let event = json!({"tool_name":"Bash","cwd":"/home/tree/extensions/app","tool_input":{"command":"cd ~/ai/scripts/main-edit-gate/src; sed -n '1,200p' main.rs"}});
        assert!(!source_read_denied(&event, None));
    }

    #[test]
    fn source_read_does_not_mistake_cat_redirect_for_source_read() {
        let event = json!({"tool_name":"Bash","cwd":"/home/tree/extensions","tool_input":{"command":"cat > x/patch.py <<'PY'\nprint(1)\nPY"}});
        assert!(!source_read_denied(&event, None));
        let read = json!({"tool_name":"Bash","cwd":"/home/tree/extensions","tool_input":{"command":"cat x/patch.py > /dev/null"}});
        assert!(source_read_denied(&read, None));
    }

    #[test]
    fn interpreter_denies_inline_code_and_external_script_creation_with_exceptions() {
        let bash = |cwd: &str, command: &str| json!({"tool_name":"Bash","cwd":cwd,"tool_input":{"command":command}});
        let write = |path: &str| json!({"tool_name":"Write","cwd":EXTENSIONS,"tool_input":{"file_path":path,"content":"print(1)"}});
        for command in [
            "python3 - <<'PY'\nprint(1)\nPY",
            "python3 -c 'print(1)'",
            "python -c 'print(1)'",
            "node -e 'console.log(1)'",
            "node <<'JS'\nconsole.log(1)\nJS",
            "node --input-type=module -e 'console.log(1)'",
            "cat > x/patch.py <<'PY'\nprint(1)\nPY",
            "cat > x/patch.mjs <<'JS'\nconsole.log(1)\nJS",
        ] {
            assert_eq!(role_denial(&bash(EXTENSIONS, command), None), Some(MAIN_INTERPRETER_REASON), "{command}");
            assert_ne!(role_denial(&bash(EXTENSIONS, command), Some("worker")), Some(MAIN_INTERPRETER_REASON), "{command}");
        }
        assert_eq!(role_denial(&write("/home/tree/ai/scripts/patch.py"), None), Some(MAIN_INTERPRETER_REASON));
        assert_eq!(role_denial(&write("/tmp/patch.mjs"), None), Some(MAIN_INTERPRETER_REASON));
        assert_eq!(role_denial(&write("/home/tree/extensions/blender_scene_creator/test/tmp/blender_smoke.py"), None), None);
        assert_eq!(role_denial(&write("/home/tree/extensions/comfyui_dh_nodes/test/example.py"), None), None);
        for command in [
            "~/ai/scripts/bin/win-smoke --project /home/tree/extensions/blender_scene_creator --script test/tmp/blender_smoke.py",
            "python -m pytest tests/test_scene.py",
            "python3 -m unittest tests.test_scene",
            "python /home/tree/extensions/blender_scene_creator/tests/test_scene.py",
            "node ~/ai/skills/access-stagehand/scripts/stagehand.mjs",
        ] {
            assert_eq!(role_denial(&bash("/home/tree/extensions/blender_scene_creator", command), None), None, "{command}");
        }
        assert_eq!(role_denial(&bash(EXTENSIONS, "rg -n 'python3 -c' main.rs"), None), None);
    }

    #[test]
    fn source_read_allows_narrow_and_excluded_reads() {
        let bash = |command: &str| json!({"tool_name":"Bash","cwd":"/home/tree/extensions/app","tool_input":{"command":command}});
        for command in [
            "rg -n x src/resolveOpen.ts",
            "grep -n x src/resolveOpen.ts",
            "wc src/resolveOpen.ts",
            "ls src/resolveOpen.ts",
            "git diff --stat",
            "sed -n '4606,4618p' src/resolveOpen.ts",
            "sed -n '99p' src/resolveOpen.ts",
            "head -n 20 src/resolveOpen.ts",
            "tail -n 20 src/resolveOpen.ts",
            "cat README.md",
            "cat test/resolveOpen.ts",
            "cat tests/resolveOpen.ts",
            "cat dist/resolveOpen.js",
            "cat node_modules/resolveOpen.js",
            "cat .ai/resolveOpen.js",
            "cat src/resolveOpen.test.ts",
            "cat /tmp/resolveOpen.ts",
        ] {
            assert_eq!(role_denial(&bash(command), None), None, "{command}");
        }
        let read = |file: &str| json!({"tool_name":"Read","cwd":"/home/tree/extensions/app","tool_input":{"file_path":file}});
        assert_eq!(role_denial(&read("README.md"), None), None);
        assert_eq!(role_denial(&read("src/resolveOpen.test.ts"), None), None);
        assert_eq!(role_denial(&read("/tmp/resolveOpen.ts"), None), None);
        assert_eq!(role_denial(&read("src/resolveOpen.ts"), Some("worker")), None);
        assert_eq!(role_denial(&read("src/resolveOpen.ts"), Some("verifier")), None);
    }

    #[test]
    fn version_files_allowed_and_other_code_edits_denied() {
        for tool in ["Edit", "Write", "MultiEdit", "NotebookEdit"] {
            let event = |file: &str| json!({
                "tool_name":tool,
                "tool_input": {if tool == "NotebookEdit" { "notebook_path" } else { "file_path" }:file},
                "cwd":EXTENSIONS
            });
            for file in ["app/src/main.rs", "root.json", "app/package-lock.json", "app/runtime-contract.test.js", "app/scripts/check.cjs", "app/test/tmp/probe.rs"] {
                assert!(target(&event(file)).is_some(), "{tool}: {file}");
            }
            for file in ["app/.ai/gate/x.json", "app/AGENTS.md", "app/readme.md", "app/Cargo.toml", "app/package.json", "app/manifest.json", "app/public/manifest.json", "app/runtime-contract.js", "/home/tree/ai/scripts/grist/Cargo.toml", "/home/tree/ai/scripts/grist/src/main.rs", "/home/tree/ai/skills/role-agent/SKILL.md", "/home/tree/ai/rules/config.json", "/tmp/file.rs"] {
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
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"python3 -c \"open('.ai/dispatch/624.log')\""}})));
        assert!(main_heavy_work(&json!({"tool_name":"Read","tool_input":{"file_path":"/home/tree/.gemini/antigravity-cli/brain/x/background/task-45.log"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Read","tool_input":{"file_path":"/home/tree/.gemini/antigravity-cli/brain/x/.system_generated/logs/transcript.jsonl"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Read","tool_input":{"file_path":"/home/tree/.codex/sessions/2026/11/01/rollout-1.jsonl"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Read","tool_input":{"file_path":"/home/tree/extensions/x/.ai/logs/app.log"}})));
        assert!(!main_heavy_work(&json!({"tool_name":"Read","tool_input":{"file_path":"/home/tree/extensions/x/.ai/logs/app.log","limit":20}})));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"cat .ai/logs/app.log"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"rg -n --hidden USER_INPUT /home/tree/.gemini/antigravity-cli/brain"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"rg -n x /home/tree/.codex/sessions/2026/11/"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"rg -n x /home/tree/.claude/projects/"}})));
        assert!(!main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"tail -n 20 .ai/logs/app.log"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"run_cdp --replace -e dist"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"node ~/ai/skills/access-stagehand/scripts/stagehand.mjs"}})));
        assert!(!main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"npm test"}})));
        assert!(!main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"agent-rules replace --new '메인은 run_cdp·stagehand.mjs·.ai/dispatch/1.log를 하지 않는다'"}})));
        assert!(main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"cd x; T=1 run_cdp --url u"}})));
        assert!(!main_heavy_work(&json!({"tool_name":"Read","tool_input":{"file_path":"/home/tree/.claude/projects/x/memory/MEMORY.md"}})));
        assert!(!main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"docker ps --format '{{.Names}}' | cat"}})));
        assert!(!main_heavy_work(&json!({"tool_name":"Bash","tool_input":{"command":"rg -n 'dispatch/929.log' /home/tree/.claude/projects/x/memory/MEMORY.md"}})));
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
    fn grist_ui_probe_blocks_metadata_and_bundle_but_allows_data_readback() {
        let bash = |command: &str| json!({"tool_name":"Bash","cwd":EXTENSIONS,"tool_input":{"command":command}});
        for command in [
            "~/ai/scripts/bin/grist get _grist_Views_section --fields id,options",
            "~/ai/scripts/bin/grist get _grist_Pages --fields id",
            "~/ai/scripts/bin/grist get _grist_Tables_column --fields id",
            "env X=1 /home/tree/ai/scripts/bin/grist get _grist_Filters --fields colRef",
            "~/ai/scripts/bin/grist get Table7 --fields C | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))'",
            "curl -sf http://localhost:8484/static/app.bundle.js",
            "curl -sf http://localhost:8484/grist-plugin-api.js",
            "curl -sf http://localhost:8484/o/docs/doc/au1EbKLtZbrW/p/47",
        ] {
            assert_eq!(role_denial(&bash(command), Some("verifier")), Some(GRIST_UI_PROBE_REASON), "{command}");
            assert_eq!(role_denial(&bash(command), None), Some(GRIST_UI_PROBE_REASON), "{command}");
        }
        for command in [
            "~/ai/scripts/bin/grist get Table34 --filter '{\"C\":[1255]}' --fields A,B,C",
            "~/ai/scripts/bin/grist get Table34 --filter '{\"C\":1255}' --fields A,B,C",
            "~/ai/scripts/bin/grist get Table7 --id 1255 --fields C",
            "~/ai/scripts/bin/grist columns Table7",
            "~/ai/scripts/bin/grist tables",
            "rg -n '_grist_Filters|bundle.js' .ai/docs/role-verifier.md",
            "curl -sf http://localhost:5175/video/",
            "node --test /home/tree/ai/skills/access-stagehand/scripts/stagehand.test.mjs",
        ] {
            assert_eq!(role_denial(&bash(command), Some("verifier")), None, "{command}");
            assert_eq!(role_denial(&bash(command), None), None, "{command}");
        }
    }

    #[test]
    fn tilde_tool_paths_are_not_product_source_from_project_cwd() {
        let cwd = Path::new("/home/tree/extensions/grist_video_maker");
        assert!(!product_source("~/ai/skills/access-stagehand/scripts/stagehand.mjs", cwd));
        assert!(product_source("src/grist.ts", cwd));
    }

    #[test]
    fn source_slice_limit_enforced() {
        let dir = std::env::temp_dir().join(format!("meg-slice-cdp-{}", std::process::id()));
        let read = |file: &str, offset: u64| json!({"tool_name":"Read","cwd":EXTENSIONS,"session_id":"s1","tool_input":{"file_path":file,"offset":offset,"limit":20}});
        assert!(!source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/grist.ts", 1), &dir));
        assert!(!source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/grist.ts", 21), &dir));
        assert!(!source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/grist.ts", 1), &dir));
        assert!(source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/grist.ts", 41), &dir));
        assert!(!source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/App.tsx", 1), &dir));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn direct_blender_denies_raw_execution_and_allows_win_smoke_and_search() {
        let bash = |command: &str| json!({"tool_name":"Bash","tool_input":{"command":command}});
        for command in [
            "\"/mnt/c/Program Files/Blender Foundation/Blender 5.2/blender.exe\" -b --factory-startup",
            "timeout 8s \"/mnt/c/Program Files/Blender Foundation/Blender 5.2/blender.exe\" -b",
            "blender.exe --version",
        ] {
            assert!(direct_blender(&bash(command)), "{command}");
            assert!(pre_worker_denial(&bash(command)).unwrap().contains("win-smoke"));
        }
        for command in [
            "~/ai/scripts/bin/win-smoke --project /home/tree/extensions/blender_bridge --script tests/smoke.py",
            "rg -n blender.exe ~/ai",
            "stat -c '%y' blender.exe",
        ] {
            assert!(!direct_blender(&bash(command)), "{command}");
        }
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
            let main_reason = if command.contains("/access-stagehand/scripts/stagehand.mjs") {
                None
            } else if command.starts_with("node -e") {
                Some(MAIN_INTERPRETER_REASON)
            } else {
                Some(MAIN_HEAVY_REASON)
            };
            assert_eq!(role_denial(&event, None), main_reason, "{command}");
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
        assert!(target(&event("/home/tree/ai/scripts/a.rs")).is_none());
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
        assert!(target(&event("/home/tree/ai/scripts/main-edit-gate/src/main.rs")).is_none());
    }






    #[test]
    fn off_project_search_denies_drives_browser_profiles_and_sibling_runtime() {
        let ev = |cmd: &str| json!({
            "tool_name": "Bash",
            "tool_input": { "command": cmd },
            "cwd": "/home/tree/extensions/web_prompt_runner"
        });
        assert!(super::off_project_search(&ev("ls -lt /mnt/d /mnt/e /mnt/h")));
        assert!(super::off_project_search(&ev("rg -n foo /mnt/h/code")));
        assert!(super::off_project_search(&ev("rg -n foo /mnt/h/code/side_prompt_gallery/download-contract.js")));
        assert!(!super::off_project_search(&ev("rg -n foo /home/tree/extensions/.ai/changes/")));
        assert!(super::off_project_search(&ev("ls -lt /mnt/c/Users/tende/Downloads")));
        assert!(super::off_project_search(&ev("rg -o path '/mnt/c/Users/tende/AppData/Local/Google/Chrome/User Data/Default/Preferences'")));
        assert!(!super::off_project_search(&ev("rg -n foo /home/tree/extensions/web_prompt_runner/.ai/changes/")));
        assert!(!super::off_project_search(&ev("rg -n foo /mnt/h/code/web_prompt_runner/download-contract.js")));
        let rejected = ev("rg -n 'matches|content_scripts|version|action' /mnt/h/code/save_current_page/manifest.json | head -n 20");
        let reason = super::pre_worker_denial(&rejected).unwrap();
        assert!(reason.contains("runtime :` 두 번째 경로"));
        assert!(reason.contains("공용 Chromium 다운로드는 ~/Downloads"));
        assert!(reason.contains("run-cdp 흐름 검증을 계속"));
        assert!(super::pre_worker_denial(&ev("rg -n 'matches|content_scripts|version|action' /mnt/h/code/web_prompt_runner/manifest.json | head -n 20")).is_none());
        assert_eq!(super::role_denial(&ev("rg -n err /home/tree/extensions/.ai/logs/api.log"), None), Some(super::LOG_BEFORE_CHANGES_REASON));
        // 261114 #949: n8n DB·Grist 이력·docker logs·세션 기록도 changes 선행 대조 대상이다.
        for raw in ["sqlite3 /mnt/d/n8n/data/database.sqlite 'select 1'", "docker logs grist --since 2026-11-10", "rg -l 1405 /home/tree/.codex/sessions/2026/11/10", "sqlite3 doc.grist 'select body from _gristsys_ActionHistory'"] {
            assert_eq!(super::role_denial(&ev(raw), None), Some(super::LOG_BEFORE_CHANGES_REASON), "{raw}");
        }
        assert_eq!(super::role_denial(&ev("rg -n 햇반 /home/tree/extensions/obsidian_daily_manager/.ai/changes/261110.a.md"), None), None);
        let tp = std::env::temp_dir().join(format!("meg-changes-first-{}.jsonl", std::process::id()));
        // git status·설명 속 글자만으로는 changes를 읽은 것으로 보지 않는다.
        std::fs::write(&tp, "M ../.ai/changes/261110.공모마감일.md\n").unwrap();
        let prose = json!({"tool_name":"Bash","tool_input":{"command":"rg -n err /home/tree/extensions/.ai/logs/api.log"},"transcript_path":tp.to_str().unwrap()});
        assert_eq!(super::role_denial(&prose, None), Some(super::LOG_BEFORE_CHANGES_REASON));
        std::fs::write(&tp, r#"{"type":"tool_use","input":{"command":"rg -n 햇반 /home/tree/extensions/.ai/changes/261110.공모마감일.md","description":"x"}}"#).unwrap();
        let with_tp = json!({"tool_name":"Bash","tool_input":{"command":"rg -n err /home/tree/extensions/.ai/logs/api.log"},"transcript_path":tp.to_str().unwrap()});
        assert_eq!(super::role_denial(&with_tp, None), None);
        let _ = std::fs::remove_file(&tp);
        println!("[verify-step] range-check start=2 mid=5 end=9");
    }

    #[test]
    fn main_cannot_write_new_tool_feature_but_may_patch_small_defect() {
        let big = (0..40).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        let ev = |tool: &str, input: Value| json!({"tool_name": tool, "cwd": "/home/tree/extensions", "tool_input": input});
        assert!(super::main_tool_feature_denial(&ev("Write", json!({"file_path": "/home/tree/.config/fish/functions/zz_new_tool.fish", "content": "function x\nend"}))).is_some());
        assert!(super::main_tool_feature_denial(&ev("Edit", json!({"file_path": "/home/tree/.config/fish/functions/zz_new_tool.fish", "old_string": "", "new_string": "function x\nend"}))).is_some());
        assert!(super::main_tool_feature_denial(&ev("Bash", json!({"command": format!("cat > ~/ai/scripts/main-edit-gate/src/main.rs <<'EOF'\n{big}\nEOF")}))).is_some());
        let patch = format!("apply_patch *** Begin Patch\n*** Update File: /home/tree/ai/scripts/main-edit-gate/src/main.rs\n@@\n{}\n*** End Patch", big.lines().map(|line| format!("+{line}")).collect::<Vec<_>>().join("\n"));
        assert!(super::main_tool_feature_denial(&ev("Bash", json!({"command": patch}))).is_some());
        assert!(super::main_tool_feature_denial(&ev("Edit", json!({"file_path": "/home/tree/ai/scripts/main-edit-gate/src/main.rs", "old_string": "a", "new_string": "a\nb\nc"}))).is_none());
        assert!(super::main_tool_feature_denial(&ev("Write", json!({"file_path": "/home/tree/ai/skills/run-cdp/SKILL.md", "content": big}))).is_none());
        assert!(super::main_tool_feature_denial(&ev("Bash", json!({"command": "sed -i 's/a/b/' ~/ai/scripts/main-edit-gate/src/main.rs"}))).is_none());
        assert!(super::main_tool_feature_denial(&ev("Bash", json!({"command": "rg -n foo ~/ai/scripts/main-edit-gate/src/main.rs > /dev/null"}))).is_none());
    }


    #[test]
    fn verifier_blocks_adhoc_tmp_cdp_scripts_and_open_stagehand_while_allowing_relay() {
        let bash = |cmd: &str| json!({"tool_name":"Bash","tool_input":{"command":cmd}});
        let write = |p: &str| json!({"tool_name":"Write","tool_input":{"file_path":p}});
        assert_eq!(role_denial(&write("test/tmp/check.mjs"), Some("verifier")), Some(VERIFIER_SCRIPT_REASON));
        assert_eq!(role_denial(&write("/home/tree/extensions/p/test/tmp/probe.js"), Some("verifier")), Some(VERIFIER_SCRIPT_REASON));
        assert_eq!(role_denial(&bash("node test/tmp/check.mjs"), Some("verifier")), Some(VERIFIER_SCRIPT_REASON));
        assert_eq!(role_denial(&bash("cat <<'EOF' > test/tmp/probe.mjs\nconsole.log(1)\nEOF"), Some("verifier")), Some(VERIFIER_SCRIPT_REASON));
        assert_eq!(role_denial(&bash("node -e \"import('/home/tree/ai/skills/access-stagehand/scripts/stagehand.mjs').then(m => m.openStagehand())\""), Some("verifier")), Some(VERIFIER_SCRIPT_REASON));
        assert_eq!(role_denial(&bash("node ~/ai/skills/access-stagehand/scripts/stagehand.mjs relay '{\"op\":\"read\",\"selector\":\"#k\"}'"), Some("verifier")), None);
        assert_eq!(role_denial(&bash("node ~/ai/skills/access-stagehand/scripts/stagehand.mjs serve --detach"), Some("verifier")), None);
        assert_eq!(role_denial(&write("test/tmp/evidence.png"), Some("verifier")), None);
    }

    #[test]
    fn verifier_cannot_launch_windows_process_or_poll_webview_from_shell() {
        let bash = |cwd: &str, cmd: &str| json!({"tool_name":"Bash","cwd":cwd,"tool_input":{"command":cmd}});
        let project = "/home/tree/extensions/win_key_fixer";
        for cmd in [
            "/mnt/h/code/win_key_fixer/win_key_fixer.exe &",
            "powershell.exe -NoProfile -Command Start-Process win_key_fixer.exe",
            "curl http://127.0.0.1:19339/json",
        ] {
            assert_eq!(role_denial(&bash(project, cmd), Some("verifier")), Some(VERIFIER_WINDOWS_SHELL_REASON), "{cmd}");
        }
        for cmd in [
            "rg -n '19339' .ai/docs/role-verifier.md",
            "stat /mnt/h/code/win_key_fixer/win_key_fixer.exe",
            "cargo test --manifest-path src-tauri/Cargo.toml",
        ] {
            assert_eq!(role_denial(&bash(project, cmd), Some("verifier")), None, "{cmd}");
        }
        assert_eq!(role_denial(&bash("/home/tree/extensions/other", "powershell.exe -Command Get-Date"), Some("verifier")), Some(VERIFIER_WINDOWS_SHELL_REASON));
    }

    #[test]
    fn hidden_cdp_requires_an_observable_network_or_storage_goal() {
        let event = |cmd: &str| json!({"tool_name":"Bash","cwd":"/home/tree/extensions/web_save_current_page","tool_input":{"command":cmd}});
        let storage = event("node ~/ai/skills/access-stagehand/scripts/stagehand.mjs relay '{\"op\":\"storage\",\"keys\":[\"apiKey\"]}'");
        let screen = event("node ~/ai/skills/access-stagehand/scripts/stagehand.mjs relay '{\"op\":\"read\",\"selector\":\"#saved\"}'");
        assert!(super::hidden_cdp_call(&storage));
        assert!(super::hidden_cdp_call(&event("node $R relay '{\"op\": \"storage\", \"keys\": [\"apiKey\"]}'")));
        assert!(super::hidden_cdp_call(&event("run_cdp --replace")));
        assert!(!super::hidden_cdp_call(&screen));
        assert!(!super::hidden_cdp_call(&event("rg -n run-cdp SKILL.md")));
        assert!(super::goal_needs_hidden_cdp("1. [수정] 설정 저장 클릭 ⇒ chrome.storage.local obsidianApiKey ▷ abc"));
        assert!(super::goal_needs_hidden_cdp("3. [기능] 저장 클릭 ⇒ 확장 저장소 obsidianApiKey·gristApiKey ▷ 입력한 값"));
        assert!(!super::goal_needs_hidden_cdp("1. [기능] 저장 클릭 ▷ 화면에 저장됨 표시"));
        assert!(!super::goal_needs_hidden_cdp("2. [수정] 목표에 네트워크·스토리지 값이 없는 행의 run-cdp 호출 ▷ 거부"));
    }


    #[test]
    fn intervention_confirmed_allows_product_resume_in_same_turn() {
        println!("[verify-step] range-check start=2 mid=4 end=7");
        let dir = std::env::temp_dir().join(format!("meg-int-resume-{}", std::process::id()));
        let root = dir.join("ext");
        std::fs::create_dir_all(dir.join("session_rows")).unwrap();
        std::fs::create_dir_all(dir.join("session_projects")).unwrap();
        std::fs::create_dir_all(root.join(".ai/changes")).unwrap();
        std::fs::write(dir.join("session_rows/s1.json"), "[\"925\",\"928\"]").unwrap();
        std::fs::write(dir.join("session_projects/s1.json"), "{\"925\":\"/home/tree/extensions\",\"928\":\"/home/tree/extensions\"}").unwrap();
        std::fs::write(root.join(".ai/changes/261114.a.md"), "# 작업\n1. x (#928)\n# 완료\n1. y (#925)\n").unwrap();
        assert!(!super::intervention_confirmed_at(&dir, &root, "s1", &["925".to_string()]));
        std::fs::write(root.join(".ai/changes/261114.a.md"), "# 작업\n# 완료\n1. y (#925)\n2. x (#928)\n").unwrap();
        assert!(super::intervention_confirmed_at(&dir, &root, "s1", &["925".to_string()]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn n8n_write_commands_cover_cli_rest_and_sql_without_blocking_reads() {
        for command in [
            "n8n import:workflow --input=/tmp/flow.json",
            "docker exec n8n n8n update:workflow --id=7 --active=true",
            "curl -X PUT http://localhost:5760/rest/workflows/7",
            "curl --request=PATCH https://example.test/api/v1/workflows/7",
            "wget --method DELETE http://localhost:5760/api/v1/workflows/7",
            "curl --data '{}' http://localhost:5760/rest/workflows",
            "sqlite3 /mnt/d/n8n/data/database.sqlite 'UPDATE workflow_entity SET name=\"x\" WHERE id=7;'",
            "sqlite3 /mnt/d/n8n/data/database.sqlite 'INSERT INTO workflow_entity VALUES (7);'",
            "sqlite3 /mnt/d/n8n/data/database.sqlite 'DELETE FROM workflow_entity WHERE id=7;'",
            "sqlite3 /mnt/d/n8n/data/database.sqlite 'REPLACE INTO workflow_entity VALUES (7);'",
        ] { assert!(super::n8n_write_command(command), "{command}"); }
        for command in [
            "n8n export:workflow --id=7 --output=/tmp/flow.json",
            "curl -X GET http://localhost:5760/rest/workflows/7",
            "sqlite3 /mnt/d/n8n/data/database.sqlite 'SELECT * FROM workflow_entity;'",
            "rg -n 'n8n import:workflow' src/main.rs",
        ] { assert!(!super::n8n_write_command(command), "{command}"); }
        println!("[verify-step] range-check start=3 mid=7 end=12");
    }

    #[test]
    fn n8n_requires_active_session_row_and_kst_dated_export_then_marks_project() {
        use serde_json::json;
        let dir = std::path::PathBuf::from("/home/tree/extensions/test/tmp")
            .join(format!("n8n-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let state = dir.join("state");
        let project = dir.join("project");
        let backups = dir.join("backup");
        std::fs::create_dir_all(state.join("session_rows")).unwrap();
        std::fs::create_dir_all(state.join("session_projects")).unwrap();
        std::fs::create_dir_all(project.join(".ai/gate")).unwrap();
        std::fs::create_dir_all(&backups).unwrap();
        let event = json!({"tool_name":"Bash","session_id":"n8n-test","tool_input":{"command":"n8n import:workflow --input=/tmp/flow.json"}});
        let now = 1794582000;
        let denied = super::n8n_denial_at(&event, &state, &backups, now).unwrap();
        assert!(denied.contains("열린 행"));
        std::fs::write(state.join("session_rows/n8n-test.json"), "[3,7,12]").unwrap();
        std::fs::write(state.join("session_projects/n8n-test.json"), json!({"3":project,"7":project,"12":project}).to_string()).unwrap();
        for (id, stage) in [(3,"H=확인"),(7,"H=검증"),(12,"H=교정")] {
            std::fs::write(project.join(format!(".ai/gate/{id}.row")), stage).unwrap();
        }
        let denied = super::n8n_denial_at(&event, &state, &backups, now).unwrap();
        assert!(denied.contains("n8n export:workflow --id=<workflowId>"));
        assert!(!project.join(".ai/gate/12.n8n").exists());
        std::fs::write(backups.join("261113-12.json"), "{}").unwrap();
        assert!(super::n8n_denial_at(&event, &state, &backups, now).is_some());
        std::fs::write(backups.join("261114-12.json"), "{}").unwrap();
        assert!(super::n8n_denial_at(&event, &state, &backups, now).is_none());
        assert_eq!(std::fs::read_to_string(project.join(".ai/gate/12.n8n")).unwrap(), "261114\n");
        // 검증(배포) 단계 행도 메인의 n8n 배포를 허용한다.
        std::fs::write(project.join(".ai/gate/12.row"), "H=확인").unwrap();
        assert!(super::n8n_denial_at(&event, &state, &backups, now).is_none());
        assert!(project.join(".ai/gate/7.n8n").exists());
        std::fs::write(project.join(".ai/gate/7.row"), "H=확인").unwrap();
        assert!(super::n8n_denial_at(&event, &state, &backups, now).unwrap().contains("열린 행"));
        std::fs::remove_dir_all(dir).unwrap();
        println!("[verify-step] range-check start=3 mid=7 end=12");
    }

    #[test]
    fn n8n_webhook_post_is_not_a_workflow_write() {
        assert!(!super::n8n_write_command("curl -s -X POST http://127.0.0.1:5760/webhook/find-new-contest -d '{}'"));
        assert!(super::n8n_write_command("curl -s -X PUT http://127.0.0.1:5760/rest/workflows/abc -d @x.json"));
        assert!(super::n8n_write_command("curl -s -X POST http://localhost:5760/api/v1/workflows -d @x.json"));
    }

    #[test]
    fn n8n_kst_backup_date_handles_utc_day_and_calendar_boundaries() {
        for (seconds, expected) in [
            (1794582000, "261114"), // KST 00:00, UTC previous day
            (1794614340, "261114"), // KST 08:59
            (1794614400, "261114"), // KST 09:00
            (1798727400, "261231"), // KST year end
            (1709217000, "240229"), // leap day
        ] { assert_eq!(super::n8n_kst_date(seconds), expected); }
    }
}
