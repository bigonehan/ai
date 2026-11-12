#[path = "../../process-turn.rs"]
mod process_turn_state;
use process_turn_state::{is_process_correction, process_turn};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

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
                if path == "/home/tree/extensions/.ai/changes" || path == "../.ai/changes" {
                    return true;
                }
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
    if off_project_search(event) { return Some("조회 경로가 현재 프로젝트의 허용 범위 밖입니다. 현재 프로젝트 AGENTS.md의 `runtime :` 두 번째 경로를 읽고, 방금 거부된 명령의 배포 경로를 그 경로로 바꿔 한 번만 다시 실행하세요. 브라우저 프로필·Downloads·다른 프로젝트·루트 .ai/changes는 조회하지 마세요. 경로를 바꿀 필요가 없다면 이 조회를 건너뛰고 run-cdp 흐름 검증을 계속하세요."); }
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
    (s.contains(".ai/dispatch/") && s.contains(".log"))
        || (s.contains("/background/task-") && s.contains(".log"))
        || s.contains("transcript.jsonl")
        || s.contains("transcript_full.jsonl")
        || s.contains(".gemini/antigravity-cli/brain")
        || s.contains(".codex/sessions")
        || s.contains(".claude/projects")
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
    let raw = Path::new(path);
    let file = normalize(&if raw.is_absolute() { raw.to_path_buf() } else { cwd.join(raw) });
    let Some(relative) = workflow_relative(&file) else { return false };
    if relative.components().count() < 2 { return false; }
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

fn role_denial(event: &Value, role: Option<&str>) -> Option<&'static str> {
    if let Some(reason) = pre_worker_denial(event) { return Some(reason); }
    if source_read_denied(event, role) { return Some(SOURCE_READ_REASON); }
    if matches!(role, None | Some("verifier")) && grist_ui_probe(event) { return Some(GRIST_UI_PROBE_REASON); }
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
const VERIFIER_CDP_LIMIT: usize = 4;
const SOURCE_SLICE_REASON: &str = "위임자는 같은 제품 소스 파일을 20줄씩 쪼개 3번 이상 연속 탐색하지 않습니다. 진입점 함수 확인이 끝났으면 내부 구현 탐색은 대행자(dispatch-agent)에게 맡기세요.";
const VERIFIER_CDP_REASON: &str = "검증자는 짧은 CDP 원라이너를 반복 재접속(실행당 4회 초과)하지 않습니다. 한 openStagehand 세션 안에서 page.waitForSelector로 렌더링을 기다리고 클릭·DOM·iframe 확인을 한 번에 마친 뒤 close() 하세요.";

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

/// 검증자(T26_ROLE=verifier)가 같은 검증 세션에서 openStagehand 원라이너를 VERIFIER_CDP_LIMIT회 넘게 반복 실행하면 거부한다.
fn verifier_cdp_reconnect_denied(event: &Value, state_dir: &Path) -> bool {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return false; }
    let Some(cmd) = event.pointer("/tool_input/command").and_then(Value::as_str) else { return false };
    let opens_session = cmd.contains("openStagehand")
        || (cmd.contains("stagehand.mjs") && execution_words(cmd).contains(&"open"));
    if !opens_session { return false; }
    let Some(session) = event.get("session_id").and_then(Value::as_str) else { return false };
    if session.is_empty() || !session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return false; }
    let state = state_dir.join(format!("verifier-cdp-{session}.json"));
    let count: usize = std::fs::read(&state).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(0);
    if count >= VERIFIER_CDP_LIMIT { return true; }
    let _ = std::fs::create_dir_all(state_dir);
    let _ = std::fs::write(&state, serde_json::to_vec(&(count + 1)).unwrap_or_default());
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

fn is_stop_or_kill_demand(prompt: &str) -> bool {
    const RESUME_PATTERNS: [&str; 6] = [
        "계속", "이어서", "재개", "실수로 정지", "실수로 중지", "실수로 중단",
    ];
    if RESUME_PATTERNS.iter().any(|k| prompt.contains(k)) {
        return false;
    }
    const STOP_PATTERNS: [&str; 7] = [
        "중지", "중단", "정지", "바로 쳐닫", "kill로 쳐닫", "쳐닫으라",
        "기다리고 자빠졌",
    ];
    STOP_PATTERNS.iter().any(|k| prompt.contains(k))
}

fn kill_running_dispatch_agents() {
    if let Ok(panes) = std::process::Command::new("tmux")
        .args(["list-panes", "-a", "-F", "#{pane_id}\t#{@dispatch-agent}"])
        .output()
    {
        for line in String::from_utf8_lossy(&panes.stdout).lines() {
            if let Some((pane_id, tag)) = line.split_once('\t') {
                if !pane_id.trim().is_empty() && !tag.trim().is_empty() {
                    let _ = std::process::Command::new("tmux").args(["kill-pane", "-t", pane_id.trim()]).output();
                }
            }
        }
    }
    if let Ok(windows) = std::process::Command::new("tmux")
        .args(["list-windows", "-a", "-F", "#{window_id}\t#{window_name}"])
        .output()
    {
        for line in String::from_utf8_lossy(&windows.stdout).lines() {
            if let Some((win_id, name)) = line.split_once('\t') {
                if !win_id.trim().is_empty() && (name.starts_with("dispatch-") || name.starts_with("starting-")) {
                    let _ = std::process::Command::new("tmux").args(["kill-window", "-t", win_id.trim()]).output();
                }
            }
        }
    }
    let _ = std::process::Command::new("pkill").args(["-f", "\\.ai/dispatch/.*\\.sh"]).output();
    let _ = std::process::Command::new("pkill").args(["-f", "ai/scripts/bin/dispatch-agent"]).output();
    let _ = std::process::Command::new("pkill").args(["-f", "ai/scripts/bin/win-smoke"]).output();
    let _ = std::process::Command::new("pkill").args(["-f", "/Blender [^/]*/blender\\.exe"]).output();
}

fn is_process_complaint(prompt: &str) -> bool {
    const COMPLAINT_PATTERNS: [&str; 16] = [
        "시간이 오래", "오래 걸", "오래 처걸", "왜 진행", "어디까지", "뭐하고 있",
        "몇년을", "몇년 걸", "쳐걸려", "처걸려", "비교분석하라고", "언제 끝나", "언제 끝", "안 끝나", "다 됐", "끝났",
    ];
    COMPLAINT_PATTERNS.iter().any(|k| prompt.contains(k)) || is_stop_or_kill_demand(prompt)
}

fn fix_without_explanation(prompt: &str) -> bool {
    let no_explanation = ["설명하지", "설명 말", "설명 그만", "이유설명", "설명은 필요", "문제가 뭔지는 나도"]
        .iter().any(|word| prompt.contains(word));
    let fix_first = ["먼저", "우선", "바로"].iter().any(|word| prompt.contains(word))
        && ["고쳐", "고치", "수정", "바꾸"].iter().any(|word| prompt.contains(word));
    no_explanation && fix_first
}

fn is_pure_status_question(prompt: &str) -> bool {
    if is_process_correction(prompt) { return false; }
    const FIX_DEMANDS: [&str; 15] = ["바꾸라고", "쳐바꾸라고", "바꿔", "수정", "고쳐", "고치", "넣어", "삭제", "아무행동을 안", "작업을 쳐하지", "개선", "개선자", "불러", "처불러", "지키라고"];
    const PROCESS_ERRORS: [&str; 10] = ["재위임", "다시 위임", "중복 위임", "반복 출력", "검증 정체", "조기 종료", "잘못 종료", "이미 확인한", "이미 검증한", "과정 오류"];
    if PROCESS_ERRORS.iter().any(|word| prompt.contains(word)) { return false; }
    let why_question = (prompt.contains('왜') || prompt.contains("이유가 뭐")) && prompt.trim_end().ends_with('?');
    is_stop_or_kill_demand(prompt) || ((is_process_complaint(prompt) || why_question) && !FIX_DEMANDS.iter().any(|k| prompt.contains(k)))
}

fn process_correction_denial(event: &Value) -> Option<String> {
    let transcript = std::fs::read_to_string(event.get("transcript_path")?.as_str()?).ok()?;
    let (prompt, _) = last_turn_prompt_and_replied(&transcript)?;
    let session = event.get("session_id").and_then(Value::as_str).unwrap_or("unknown");
    if !process_turn(session, &prompt).process { return None; }
    let tool = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/home/tree/extensions"));
    let product_path = |path: &str| {
        // `2>/dev/null`, `>out`, `<in` name the redirect target, not a path relative to cwd
        let path = path.rsplit(['>', '<']).next().unwrap_or(path).trim_start_matches('&');
        let path = path.trim_matches(['\'', '"', ';', ',', '(', ')', '|']);
        if path.is_empty() || path.starts_with('~') || path.starts_with('$') { return false; }
        let resolved = if Path::new(path).is_absolute() { normalize(Path::new(path)) } else { normalize(&cwd.join(path)) };
        resolved.to_str().and_then(|path| path.strip_prefix("/home/tree/extensions/"))
            .and_then(|rest| rest.split_once('/'))
            .is_some_and(|(project, _)| !project.starts_with('.') && project != "test"
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
            }) && !command_head.contains("--role improver"))
    } else { false };
    product_call.then(|| "현재 요청은 작업 과정 수정이에요(manage-intervention). 제품 파일 탐색·제품 행 재개를 멈추고 과정 수정·개선자 호출과 훅 검증을 마친 뒤 [⏸] 원래 행으로 복귀하세요.".into())
}

fn ai_write_before_intervention_row(event: &Value) -> Option<String> {
    ai_write_before_intervention_row_at(event, &process_turn_state::process_state_dir())
}

fn ai_write_before_intervention_row_at(event: &Value, state: &Path) -> Option<String> {
    let transcript = std::fs::read_to_string(event.get("transcript_path")?.as_str()?).ok()?;
    let (prompt, _) = last_turn_prompt_and_replied(&transcript)?;
    let session = event.get("session_id").and_then(Value::as_str)?;
    let turn = process_turn_state::process_turn_at(state, session, &prompt);
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

fn writes_ai(event: &Value) -> bool {
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    let ai_path = |raw: &str, base: &Path| {
        let raw = raw.trim_matches(['\'', '"', ',', ';']);
        let expanded = raw.strip_prefix('~').map(|rest| format!("/home/tree{rest}")).unwrap_or_else(|| raw.to_string());
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
        for (idx, _) in segment.match_indices('>') {
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
        (&tail[..end], &tail[end..])
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

fn is_first_prompt(transcript: &str) -> bool {
    transcript.lines().filter(|line| last_turn_prompt_and_replied(line).is_some()).count() <= 1
}

fn is_read_only_call(tool: &str, event: &Value) -> bool {
    match tool {
        "Read" | "Grep" | "Glob" => true,
        "Bash" => {
            let cmd = event.pointer("/tool_input/command").and_then(Value::as_str).unwrap_or("");
            ![">", "dispatch-agent", "task-add", "request-record", "task-confirm", "task-update", "task-complete", "flow-wait", "cargo", "win-smoke", "rm ", "mv ", "cp ", "sed -i", "perl -pi", "perl -0pi", "git commit", "git push"]
                .iter().any(|k| cmd.replace("2>/dev/null", "").contains(k))
        }
        _ => false,
    }
}

fn status_question_denial(event: &Value, _state_dir: &Path) -> Option<String> {
    let tp = event.get("transcript_path").and_then(Value::as_str).unwrap_or("");
    if tp.is_empty() { return None; }
    let text = std::fs::read_to_string(tp).ok()?;
    let (prompt, replied) = last_turn_prompt_and_replied(&text)?;
    let tool = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
    if is_stop_or_kill_demand(&prompt) {
        kill_running_dispatch_agents();
    }
    if tool == "Bash" {
        let cmd = event.pointer("/tool_input/command").and_then(Value::as_str).unwrap_or("");
        if is_process_complaint(&prompt) && cmd.contains("dispatch-agent") && cmd.contains("--attach") {
            return Some("사용자가 지연·불필요한 과정을 지적했어요. 기존 위임에 dispatch-agent --attach(계속 대기)를 걸지 말고 바로 답하세요.".into());
        }
    }
    if !replied && is_stop_or_kill_demand(&prompt) {
        return Some("실행 중이던 백그라운드 에이전트·프로세스는 훅이 즉시 강제 정지시켰어요. 정지 명령이나 다른 도구를 호출하지 말고 지금 바로 텍스트로만 대답하세요.".into());
    }
    if fix_without_explanation(&prompt) { return None; }
    if !replied && is_process_complaint(&prompt) && tool == "Bash"
        && event.pointer("/tool_input/command").and_then(Value::as_str).is_some_and(|cmd| cmd.contains("win-smoke")) {
        return Some("사용자의 과정 지적에 먼저 답하세요. 답변 전 win-smoke 재실행은 차단됐어요.".into());
    }
    if replied || !is_pure_status_question(&prompt) { return None; }
    // a fresh session has no delayed work yet, so its first question may read code to answer
    if is_first_prompt(&text) && is_read_only_call(tool, event) { return None; }
    if matches!(tool, "Edit" | "Read") {
        return Some("사용자가 진행 지연·현황 이유를 물었어요. 파일 읽기·수정으로 응답을 미루지 말고 먼저 현황·이유부터 바로 답하세요.".into());
    }
    if tool == "Bash" {
        let cmd = event.pointer("/tool_input/command").and_then(Value::as_str).unwrap_or("").trim();
        let allowed_status_read = (cmd.contains("grist gate") || cmd.contains("grist task ") || cmd.contains("grist task-show"))
            && !["dispatch-agent", "task-add", "request-record", "task-confirm", "flow-wait", "task-hidden", "task-complete", "task-update", "cargo", "rg ", "tmux", "stat ", "ls "].iter().any(|k| cmd.contains(k));
        if !allowed_status_read {
            return Some("사용자가 진행 지연·현황 이유를 물었어요. 위임·마감·빌드·탐색으로 응답을 미루지 말고(단일 grist 상태 조회 외 도구 호출 금지) 먼저 이유부터 답하세요.".into());
        }
    }
    None
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).map_err(|e| e.to_string())?;
    let event: Value = normalize_codex_event(serde_json::from_str(&input).map_err(|e| e.to_string())?);
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
    if std::env::var("T26_ROLE").ok().as_deref() == Some("verifier") {
        if verifier_cdp_reconnect_denied(&event, &state_dir) {
            println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":VERIFIER_CDP_REASON}}));
        }
        return Ok(());
    }
    if std::env::var("T26_ROLE").ok().as_deref() == Some("worker") { return Ok(()); }
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
    if let Some(reason) = status_question_denial(&event, &state_dir) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        return Ok(());
    }
    if let Some(reason) = ai_write_before_intervention_row(&event) {
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
    Ok(())
}

fn main() {
    if let Err(error) = run() { eprintln!("main-edit-gate: {error}"); }
}

#[cfg(test)]
mod tests {
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
    fn complaints_do_not_kill_or_block_fix_work() {
        for prompt in ["왜 개선자를 안 부르고 냅두지?", "토큰낭비는 쳐하면서 왜 규칙은 안 따르냐", "불필요한 개선 작업 왜 쳐하고 있나?", "실수로 정지시켰으니까 계속 작업해"] {
            assert!(!super::is_stop_or_kill_demand(prompt));
            assert!(!super::is_pure_status_question(prompt));
        }
        for prompt in ["중지", "중단", "정지", "바로 쳐닫", "kill로 쳐닫", "쳐닫으라", "기다리고 자빠졌"] {
            assert!(super::is_stop_or_kill_demand(prompt));
        }
    }

    #[test]
    fn process_correction_allows_fix_and_blocks_product_reentry() {
        let dir = std::env::temp_dir().join(format!("meg-process-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("TASK_COMPLETE_STATE_DIR", &dir);
        let transcript = dir.join("transcript.jsonl");
        std::fs::write(&transcript, "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>과정 오류를 왜 수정하지 않았어?</USER_REQUEST>\"}\n").unwrap();
        let event = |command: &str| serde_json::json!({"tool_name":"Bash","tool_input":{"command":command},"transcript_path":transcript});
        assert!(!super::is_pure_status_question("과정 오류를 왜 수정하지 않았어?"));
        assert!(super::process_correction_denial(&event("rg -n foo /home/tree/ai/scripts/task-complete/src/main.rs")).is_none());
        assert!(super::process_correction_denial(&event("rg -n foo /home/tree/ai/scripts/dispatch-agent/src/main.rs")).is_none());
        assert!(super::process_correction_denial(&event("dispatch-agent --role improver --row 860")).is_none());
        assert!(super::process_correction_denial(&event("/home/tree/ai/scripts/bin/task-complete --hook <<'JSON'\n{\"cwd\":\"/home/tree/extensions\",\"message\":\"제품 작업을 재개하지 않았어요\"}\nJSON")).is_none());
        assert!(super::process_correction_denial(&event("python3 - <<'PY'\ntext = '/home/tree/extensions/web_save_current_page/src/main.ts'\nPY")).is_none());
        assert!(super::process_correction_denial(&event("python3 - <<'PY'\nprint('ok')\nPY\nrg -n foo /home/tree/extensions/web_save_current_page/src/main.ts")).unwrap().contains("manage-intervention"));
        let ai_diff = super::normalize_codex_event(serde_json::json!({"tool_name":"exec_command","tool_input":{"cmd":"git diff -- scripts/dispatch-agent/src/main.rs","workdir":"/home/tree/ai"},"cwd":"/home/tree/extensions","transcript_path":transcript}));
        assert!(super::process_correction_denial(&ai_diff).is_none());
        assert!(super::process_correction_denial(&event("ls -la ~/x/hooks 2>/dev/null | head -3")).is_none());
        assert!(super::process_correction_denial(&event("rg -n x ~/y.json 2>/dev/null >/tmp/out")).is_none());
        assert!(super::process_correction_denial(&event("cargo build --release -p dispatch-agent 2>&1 | tail -n1")).is_none());
        assert!(super::process_correction_denial(&event("rg -n \"codex/hooks/|join\" ~/ai/scripts/agent-rules/src")).is_none());
        assert!(super::process_correction_denial(&event("rg -n foo /home/tree/extensions/web_save_current_page/src/main.ts")).unwrap().contains("manage-intervention"));
        assert!(super::process_correction_denial(&event("rg -n foo web_save_current_page/src/main.ts")).is_some());
        assert!(super::process_correction_denial(&event("dispatch-agent --role verifier --row 853")).is_some());
        // the spaced-out wording is the same turn kind; task-complete reads the state this call wrote
        std::fs::write(&transcript, "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>~/Report.md를 보고 작업과정의 문제를 개선해</USER_REQUEST>\"}\n").unwrap();
        assert!(super::process_correction_denial(&event("rg -n foo web_save_current_page/src/main.ts")).is_some());
        assert!(std::fs::read_to_string(dir.join("process_turn/unknown")).unwrap().lines().nth(1) == Some("1"));
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
        let project = Path::new(EXTENSIONS).join(format!("blender_scene_creator/test/tmp/zz_meg_test_{}", std::process::id()));
        std::fs::create_dir_all(project.join(".ai/changes")).unwrap();
        std::fs::create_dir_all(project.join(".ai/gate")).unwrap();
        std::fs::write(project.join("AGENTS.md"), "").unwrap();
        std::fs::write(project.join(".ai/changes/261107.a.md"), "# 작업\n1. [🌍] x (#9001)\n\n# 완료\n").unwrap();
        std::fs::write(project.join("package.json"), "{\n  \"version\": \"0.3.6\"\n}").unwrap();
        std::fs::write(project.join("Cargo.toml"), "[package]\nversion = \"0.3.7\"\n").unwrap();
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
        std::fs::write(project.join(".ai/gate/version-flow-Cargo.toml"), "9003").unwrap();
        let both = "sed -i '4s/0.3.7/0.3.8/' package.json; sed -i '3s/0.3.7/0.3.8/' Cargo.toml";
        assert!(version_bump_denial(&bash(both)).is_some(), "denied on the second file");
        assert_eq!(std::fs::read_to_string(project.join(".ai/gate/version-flow-package.json")).unwrap(), "version=0.3.7\n9001", "denied command leaves earlier file state untouched");
        std::fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn scaffolded_first_version_does_not_consume_release_bump() {
        let project = Path::new(EXTENSIONS).join(format!("blender_scene_creator/test/tmp/zz_meg_scaffold_{}", std::process::id()));
        std::fs::create_dir_all(project.join(".ai/changes")).unwrap();
        std::fs::write(project.join("AGENTS.md"), "").unwrap();
        std::fs::write(project.join(".ai/changes/261110.a.md"), "# 작업\n1. [🌍] x (#9001)\n").unwrap();
        let event = |command: &str| json!({"tool_name":"Bash","cwd":project.to_string_lossy(),"tool_input":{"command":command}});
        let command = "sed -i '3s/0.0.1/0.0.2/' Cargo.toml";
        assert!(version_bump_denial(&event(command)).is_none());
        assert!(!project.join(".ai/gate/version-flow-Cargo.toml").exists());
        std::fs::write(project.join("Cargo.toml"), "[package]\nversion = \"0.0.1\"\n").unwrap();
        assert!(version_bump_denial(&event(command)).is_none());
        std::fs::write(project.join("Cargo.toml"), "[package]\nversion = \"0.0.2\"\n").unwrap();
        assert!(version_bump_denial(&event(command)).is_some());
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
    fn source_slice_and_verifier_cdp_reconnect_limits_enforced() {
        let dir = std::env::temp_dir().join(format!("meg-slice-cdp-{}", std::process::id()));
        let read = |file: &str, offset: u64| json!({"tool_name":"Read","cwd":EXTENSIONS,"session_id":"s1","tool_input":{"file_path":file,"offset":offset,"limit":20}});
        assert!(!source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/grist.ts", 1), &dir));
        assert!(!source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/grist.ts", 21), &dir));
        assert!(!source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/grist.ts", 1), &dir));
        assert!(source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/grist.ts", 41), &dir));
        assert!(!source_slice_denied(&read("/home/tree/extensions/grist_video_maker/src/App.tsx", 1), &dir));

        let cdp = json!({"tool_name":"Bash","session_id":"v1","tool_input":{"command":"node -e 'import { openStagehand } from \"./stagehand.mjs\"; await openStagehand();'"}});
        for _ in 0..VERIFIER_CDP_LIMIT {
            assert!(!verifier_cdp_reconnect_denied(&cdp, &dir));
        }
        assert!(verifier_cdp_reconnect_denied(&cdp, &dir));
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
    fn status_question_blocks_search_until_replied_and_allows_single_status_read() {
        let dir = std::env::temp_dir().join(format!("meg-statusq-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let tp = dir.join("transcript.jsonl");
        let earlier = "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>\\n작업해\\n</USER_REQUEST>\"}\n{\"type\":\"PLANNER_RESPONSE\",\"content\":\"진행 중이에요.\"}\n";
        std::fs::write(&tp, earlier.to_string() + "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>\\n뭐가 이렇게 시간이 오래 처걸리고 있지?\\n</USER_REQUEST>\"}\n").unwrap();
        let tp_str = tp.to_string_lossy().into_owned();
        let dispatch = json!({"tool_name":"Bash","tool_input":{"command":"~/ai/scripts/bin/dispatch-agent --agent codex --row 1"},"session_id":"sq1","transcript_path":tp_str});
        assert!(super::status_question_denial(&dispatch, &dir).unwrap().contains("먼저 이유부터"));
        let rg = json!({"tool_name":"Bash","tool_input":{"command":"rg -n foo src"},"session_id":"sq1","transcript_path":tp_str});
        assert!(super::status_question_denial(&rg, &dir).is_some());
        let status = json!({"tool_name":"Bash","tool_input":{"command":"~/ai/scripts/bin/grist task 843 --project /home/tree/extensions/blender_camera_control"},"session_id":"sq1","transcript_path":tp_str});
        assert!(super::status_question_denial(&status, &dir).is_none());
        std::fs::write(&tp, "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>\\n뭐가 이렇게 시간이 오래 처걸리고 있지?\\n</USER_REQUEST>\"}\n{\"type\":\"PLANNER_RESPONSE\",\"content\":\"이유는 다음과 같아요.\"}\n").unwrap();
        assert!(super::status_question_denial(&dispatch, &dir).is_none());
        assert!(super::status_question_denial(&rg, &dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_question_of_session_allows_read_only_tools() {
        let dir = std::env::temp_dir().join(format!("meg-first-q-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tp = dir.join("transcript.jsonl");
        std::fs::write(&tp, "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>\\n설정메뉴에 grist table을 설정하는 메뉴는 왜있지?\\n</USER_REQUEST>\"}\n").unwrap();
        let tp_str = tp.to_string_lossy().into_owned();
        let call = |tool: &str, input: serde_json::Value| json!({"tool_name":tool,"tool_input":input,"transcript_path":tp_str});
        assert!(super::status_question_denial(&call("Bash", json!({"command":"rg -n -i table . 2>/dev/null | head"})), &dir).is_none());
        assert!(super::status_question_denial(&call("Read", json!({"file_path":"/home/tree/extensions/web_save_current_page/options.js"})), &dir).is_none());
        assert!(super::status_question_denial(&call("Bash", json!({"command":"dispatch-agent --agent codex --row 1"})), &dir).is_some());
        assert!(super::status_question_denial(&call("Bash", json!({"command":"echo x > a.txt"})), &dir).is_some());
        assert!(super::status_question_denial(&call("Edit", json!({"file_path":"a.js"})), &dir).is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_fix_first_without_explanation_allows_tools() {
        let dir = std::env::temp_dir().join(format!("meg-fix-first-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join("transcript.jsonl");
        let prompt = "왜 아직 안 고쳤어? 이유설명은 그만하고 먼저 고쳐";
        std::fs::write(&transcript, format!("{{\"type\":\"USER_INPUT\",\"content\":\"{prompt}\"}}\n")).unwrap();
        let event = serde_json::json!({"tool_name":"Bash","tool_input":{"command":"rg -n foo src"},"transcript_path":transcript});
        assert!(super::fix_without_explanation(prompt));
        assert!(super::status_question_denial(&event, &dir).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn question_pointing_out_process_error_allows_fix_tools() {
        let dir = std::env::temp_dir().join(format!("meg-process-question-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join("transcript.jsonl");
        let prompt = "이미 검증한 로컬 CLI 훅을 왜 재위임했어?";
        std::fs::write(&transcript, format!("{{\"type\":\"USER_INPUT\",\"content\":\"{prompt}\"}}\n")).unwrap();
        let event = serde_json::json!({"tool_name":"Bash","tool_input":{"command":"rg -n foo src"},"transcript_path":transcript});
        assert!(!super::is_pure_status_question(prompt));
        assert!(super::status_question_denial(&event, &dir).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn process_complaint_blocks_win_smoke_until_replied() {
        let dir = std::env::temp_dir().join(format!("meg-win-smoke-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let tp = dir.join("transcript.jsonl");
        let command = json!({"tool_name":"Bash","tool_input":{"command":"~/ai/scripts/bin/win-smoke --project p --script s.py"},"transcript_path":tp});
        for prompt in ["몇년을 기다려야 해?", "몇년 걸리는 거야?", "왜 쳐걸려?", "왜 처걸려?", "비교분석하라고 한 적 없어. 왜 실행해?"] {
            std::fs::write(&tp, format!("{{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>\\n{prompt}\\n</USER_REQUEST>\"}}\n")).unwrap();
            assert!(super::is_process_complaint(prompt));
            assert!(super::status_question_denial(&command, &dir).unwrap().contains("win-smoke"));
        }
        std::fs::write(&tp, "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>\\n왜 쳐걸려?\\n</USER_REQUEST>\"}\n{\"type\":\"PLANNER_RESPONSE\",\"content\":\"원인을 설명했어요.\"}\n").unwrap();
        assert!(super::status_question_denial(&command, &dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
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
        assert!(super::off_project_search(&ev("rg -n foo /home/tree/extensions/.ai/changes/")));
        assert!(super::off_project_search(&ev("ls -lt /mnt/c/Users/tende/Downloads")));
        assert!(super::off_project_search(&ev("rg -o path '/mnt/c/Users/tende/AppData/Local/Google/Chrome/User Data/Default/Preferences'")));
        assert!(!super::off_project_search(&ev("rg -n foo /home/tree/extensions/web_prompt_runner/.ai/changes/")));
        assert!(!super::off_project_search(&ev("rg -n foo /mnt/h/code/web_prompt_runner/download-contract.js")));
        let rejected = ev("rg -n 'matches|content_scripts|version|action' /mnt/h/code/save_current_page/manifest.json | head -n 20");
        let reason = super::pre_worker_denial(&rejected).unwrap();
        assert!(reason.contains("runtime :` 두 번째 경로"));
        assert!(reason.contains("run-cdp 흐름 검증을 계속"));
        assert!(super::pre_worker_denial(&ev("rg -n 'matches|content_scripts|version|action' /mnt/h/code/web_prompt_runner/manifest.json | head -n 20")).is_none());
    }
}
