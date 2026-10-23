// Supply project structure before an extensions task begins in Codex.
// Build: cargo build --release -p architecture-context --manifest-path ~/ai/scripts/Cargo.toml
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::env;
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

fn is_ignored_word(word: &str) -> bool {
    matches!(
        word,
        "home"
            | "tree"
            | "extensions"
            | "project"
            | "manage_project"
            | "manage-project"
            | "AGENTS.md"
    )
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

fn main() {
    let Some(input) = read_stdin_bounded() else {
        process::exit(1);
    };
    let event: Value = match serde_json::from_str(&input) {
        Ok(Value::Object(map)) => Value::Object(map),
        _ => process::exit(1),
    };

    let prompt = event.get("prompt").and_then(Value::as_str).unwrap_or("");
    let cwd_raw = event.get("cwd").and_then(Value::as_str).unwrap_or(".");
    let cwd = resolved(Path::new(cwd_raw)).to_string_lossy().into_owned();

    let explicit_re = Regex::new(r#"/home/tree/extensions/[^\s/`'"<>]+"#).unwrap();
    let project = explicit_re
        .find(prompt)
        .and_then(|m| project_from(m.as_str()))
        .or_else(|| project_from(&cwd));
    let Some(project) = project else {
        process::exit(0);
    };

    let word_re = Regex::new(r"[\p{L}\p{N}_.-]{2,}").unwrap();
    let mut words = Vec::new();
    let mut seen = HashSet::new();
    for m in word_re.find_iter(prompt) {
        let word = m.as_str();
        if !is_ignored_word(word) && seen.insert(word.to_owned()) {
            words.push(word.to_owned());
            if words.len() >= 8 {
                break;
            }
        }
    }

    let hook_event_name = event.get("hook_event_name").and_then(Value::as_str);
    let mut args = vec!["--project".to_owned(), project];
    if hook_event_name == Some("UserPromptSubmit") && !words.is_empty() {
        args.push("--query".to_owned());
        args.push(words.join(" "));
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

    let mut inner = Map::new();
    if let Some(name) = event.get("hook_event_name").filter(|v| !v.is_null()) {
        inner.insert("hookEventName".to_owned(), name.clone());
    }
    inner.insert("additionalContext".to_owned(), Value::String(context));
    let mut outer = Map::new();
    outer.insert("hookSpecificOutput".to_owned(), Value::Object(inner));
    println!("{}", serde_json::to_string(&Value::Object(outer)).unwrap());
}
