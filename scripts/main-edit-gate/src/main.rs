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

fn pre_worker_denial(event: &Value) -> Option<&'static str> {
    if direct_nf(event) { return Some("nf 알림은 ask-notify hook이 자동 전송합니다. nf를 직접 실행하지 마세요."); }
    if direct_blender(event) { return Some("blender.exe를 직접 실행하면 세션이 닫히지 않고 멈춥니다. ~/ai/scripts/bin/win-smoke --project <dir> --script <py>로 실행하세요."); }
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
        let browser = ["9222", "stagehand", "Stagehand", "connectOverCDP", "Runtime.evaluate"].iter().any(|k| segment.contains(k));
        let scratch_script = words.get(1).is_some_and(|w| scratch_dir(w) || (in_scratch && !w.starts_with(['/', '~', '-'])));
        *first == "run_cdp"
            || (*first == "node" && (browser || scratch_script))
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
const MAIN_HEAVY_REASON: &str = "메인은 판단·지시만 합니다. CDP·stagehand 실행은 검증자(--role verifier)에게, 위임 로그 읽기는 구현자에게 맡기고, dispatch-agent의 exit·last 한 줄과 grist task-show --section 검증으로 판정하고, 원인은 --resume으로 구현자에게 한 줄로 받으세요.";

fn role_denial(event: &Value, role: Option<&str>) -> Option<&'static str> {
    if let Some(reason) = pre_worker_denial(event) { return Some(reason); }
    if source_read_denied(event, role) { return Some(SOURCE_READ_REASON); }
    if role_exempt(role) { return None; }
    if role == Some("worker") { return browser_work(event).then_some(WORKER_BROWSER_REASON); }
    if role == Some("improver") { return None; }
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

/// A `cat` heredoc body is text, not a write target. Interpreter heredocs stay, since their body can write.
fn without_cat_heredoc_bodies(command: &str) -> String {
    let mut kept = Vec::new();
    let mut delimiter: Option<String> = None;
    for line in command.lines() {
        if let Some(end) = &delimiter {
            if line.trim() == end { delimiter = None; }
            continue;
        }
        kept.push(line);
        let opens_cat = line.split(';').last().is_some_and(|part| part.trim_start().starts_with("cat "));
        if let (true, Some((_, tail))) = (opens_cat, line.rsplit_once("<<")) {
            let word = tail.trim_start_matches('-').trim().split_whitespace().next().unwrap_or("");
            let word = word.trim_matches(|c| c == '\'' || c == '"');
            if !word.is_empty() { delimiter = Some(word.to_string()); }
        }
    }
    kept.join("\n")
}

fn generated_write(event: &Value) -> Option<&'static str> {
    let cwd = Path::new(event.get("cwd").and_then(Value::as_str).unwrap_or("/"));
    let tool = event.get("tool_name").and_then(Value::as_str)?;
    if matches!(tool, "Edit" | "Write" | "MultiEdit" | "NotebookEdit") {
        let path = event.pointer(if tool == "NotebookEdit" { "/tool_input/notebook_path" } else { "/tool_input/file_path" }).and_then(Value::as_str)?;
        return generated_path(path, cwd);
    }
    if tool != "Bash" { return None; }
    let command = without_cat_heredoc_bodies(event.pointer("/tool_input/command").and_then(Value::as_str)?);
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
    let cwd = event.get("cwd").cloned().unwrap_or(Value::Null);
    let session = event.get("session_id").cloned().unwrap_or(Value::Null);
    match name {
        "exec_command" | "functions.exec_command" => json!({"tool_name":"Bash","tool_input":{"command":input.get("cmd").or_else(|| input.get("command")).and_then(Value::as_str).unwrap_or("")},"cwd":cwd,"session_id":session}),
        "apply_patch" | "functions.apply_patch" => {
            let patch = input.as_str().or_else(|| input.get("patch").and_then(Value::as_str)).unwrap_or("");
            json!({"tool_name":"Bash","tool_input":{"command":format!("apply_patch {patch}")},"cwd":cwd,"session_id":session})
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

fn is_pure_status_question(prompt: &str) -> bool {
    const STATUS_PATTERNS: [&str; 6] = ["시간이 오래", "오래 걸", "오래 처걸", "왜 진행", "어디까지", "뭐하고 있"];
    const FIX_DEMANDS: [&str; 9] = ["바꾸라고", "쳐바꾸라고", "바꿔", "수정", "고쳐", "고치", "넣어", "삭제", "아무행동을 안"];
    STATUS_PATTERNS.iter().any(|k| prompt.contains(k)) && !FIX_DEMANDS.iter().any(|k| prompt.contains(k))
}

fn status_question_denial(event: &Value, state_dir: &Path) -> Option<String> {
    let tp = event.get("transcript_path").and_then(Value::as_str).unwrap_or("");
    if tp.is_empty() { return None; }
    let text = std::fs::read_to_string(tp).ok()?;
    let (prompt, replied) = last_turn_prompt_and_replied(&text)?;
    if replied || !is_pure_status_question(&prompt) { return None; }
    if event.get("tool_name").and_then(Value::as_str) == Some("Bash") {
        let cmd = event.pointer("/tool_input/command").and_then(Value::as_str).unwrap_or("");
        if ["dispatch-agent", "task-add", "request-record", "cargo build", "npm run build"].iter().any(|k| cmd.contains(k)) {
            return Some("사용자가 진행 지연·현황 이유를 물었어요. 위임·빌드·행 생성으로 응답을 미루지 말고 먼저 이유부터 답하세요.".into());
        }
    }
    let session = event.get("session_id").and_then(Value::as_str).unwrap_or("default");
    let safe: String = session.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    if safe.is_empty() { return None; }
    let _ = std::fs::create_dir_all(state_dir);
    let state_file = state_dir.join(format!("status-q-{safe}.json"));
    let prev: Value = std::fs::read(&state_file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let count = if prev.get("prompt").and_then(Value::as_str) == Some(prompt.as_str()) {
        prev.get("count").and_then(Value::as_u64).unwrap_or(0)
    } else {
        0
    };
    if count >= 2 {
        return Some("진행 지연·이유 질문 턴에서는 파일·상태 조회를 2회까지만 허용해요. 추가 탐색 없이 이미 아는 내용으로 지금 바로 답하세요.".into());
    }
    let _ = std::fs::write(&state_file, serde_json::to_vec(&json!({"prompt": prompt, "count": count + 1})).unwrap_or_default());
    None
}

fn sync_blocking_denial(event: &Value) -> Option<&'static str> {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") { return None; }
    let wait_ms = event.pointer("/tool_input/wait_ms").and_then(Value::as_i64).unwrap_or(0);
    if wait_ms <= 1000 { return None; }
    let cmd = event.pointer("/tool_input/command").and_then(Value::as_str).unwrap_or("");
    if cmd.contains("dispatch-agent") && !cmd.contains("--help") {
        return Some("사용자 메시지 큐 대기 방지: dispatch-agent는 동기 대기(WaitMsBeforeAsync > 1000)로 세션을 점유하지 말고 WaitMsBeforeAsync=500 백그라운드로 즉시 반환하세요.");
    }
    None
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).map_err(|e| e.to_string())?;
    let event: Value = normalize_codex_event(serde_json::from_str(&input).map_err(|e| e.to_string())?);
    if let Some(reason) = sync_blocking_denial(&event) {
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
    if std::env::var("T26_ROLE").ok().as_deref() == Some("improver") {
        if let Some(reason) = improver_denial(&event) {
            println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
        }
        return Ok(());
    }
    if matches!(std::env::var("T26_ROLE").ok().as_deref(), Some("worker" | "verifier")) { return Ok(()); }
    if let Some(source) = generated_write(&event) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":format!("생성본은 직접 수정할 수 없습니다. 원본 {source}을 고친 뒤 agent-rules sync를 실행하세요.")}}));
        return Ok(());
    }
    let state_dir = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state/main-edit-gate");
    if let Some(reason) = status_question_denial(&event, &state_dir) {
        println!("{}", json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}}));
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
        let root = std::env::temp_dir().join(format!("meg-version-{}", std::process::id()));
        let project = Path::new(EXTENSIONS).join(format!("win_key_fixer/test/tmp/zz_meg_test_{}", std::process::id()));
        let _ = root;
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
        let project = Path::new(EXTENSIONS).join(format!("win_key_fixer/test/tmp/zz_meg_scaffold_{}", std::process::id()));
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
    fn source_read_allows_writing_a_source_file_with_cat() {
        let event = json!({"tool_name":"Bash","cwd":"/home/tree/extensions","tool_input":{"command":"cat > x/patch.py <<'PY'\nprint(1)\nPY"}});
        assert!(!source_read_denied(&event, None));
        let read = json!({"tool_name":"Bash","cwd":"/home/tree/extensions","tool_input":{"command":"cat x/patch.py > /dev/null"}});
        assert!(source_read_denied(&read, None));
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
    fn status_question_denies_dispatch_and_third_lookup_until_replied() {
        let dir = std::env::temp_dir().join(format!("meg-statusq-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let tp = dir.join("transcript.jsonl");
        std::fs::write(&tp, "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>\\n뭐가 이렇게 시간이 오래 처걸리고 있지?\\n</USER_REQUEST>\"}\n").unwrap();
        let tp_str = tp.to_string_lossy().into_owned();
        let dispatch = json!({"tool_name":"Bash","tool_input":{"command":"~/ai/scripts/bin/dispatch-agent --agent codex --row 1"},"session_id":"sq1","transcript_path":tp_str});
        assert!(super::status_question_denial(&dispatch, &dir).unwrap().contains("먼저 이유부터"));
        let rg = json!({"tool_name":"Bash","tool_input":{"command":"rg -n foo src"},"session_id":"sq1","transcript_path":tp_str});
        assert!(super::status_question_denial(&rg, &dir).is_none());
        assert!(super::status_question_denial(&rg, &dir).is_none());
        assert!(super::status_question_denial(&rg, &dir).unwrap().contains("2회까지만"));
        std::fs::write(&tp, "{\"type\":\"USER_INPUT\",\"content\":\"<USER_REQUEST>\\n뭐가 이렇게 시간이 오래 처걸리고 있지?\\n</USER_REQUEST>\"}\n{\"type\":\"PLANNER_RESPONSE\",\"content\":\"이유는 다음과 같아요.\"}\n").unwrap();
        assert!(super::status_question_denial(&dispatch, &dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
