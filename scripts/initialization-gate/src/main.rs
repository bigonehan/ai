mod json;

use json::{obj_set, s, Value};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const MAX_BLOCKS: i64 = 3;

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn grist_bin() -> PathBuf {
    std::env::var_os("GRIST_BIN")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("ai/scripts/bin/grist"))
}

fn state_dir() -> PathBuf {
    std::env::var_os("INITIALIZATION_GATE_STATE_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".codex/tmp/initialization_gate"))
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

fn parse_cli_cwd() -> Option<String> {
    let mut args = std::env::args().skip(1);
    let mut cwd = None;
    while let Some(arg) = args.next() {
        if arg == "--project" || arg == "--cwd" {
            if let Some(val) = args.next() {
                cwd = Some(val);
            }
        } else if let Some(val) = arg.strip_prefix("--project=").or_else(|| arg.strip_prefix("--cwd=")) {
            cwd = Some(val.to_string());
        }
    }
    cwd
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

fn read_stdin_bounded() -> String {
    if std::io::stdin().is_terminal() || !stdin_ready(20) {
        return String::new();
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut stdin = std::io::stdin().lock();
    loop {
        match stdin.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&chunk[..n]);
                if !stdin_ready(5) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn main() {
    let cli_cwd = parse_cli_cwd();
    let input = if cli_cwd.is_none() {
        read_stdin_bounded()
    } else {
        String::new()
    };
    let event = json::parse(&input).unwrap_or(Value::Obj(vec![]));

    let cwd = cli_cwd
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            event
                .get("cwd")
                .filter(|v| v.truthy())
                .and_then(Value::as_str)
        })
        .unwrap_or(".");

    let out = Command::new(grist_bin())
        .args(["gate", "--cwd", cwd])
        .stdin(Stdio::null())
        .output()
        .expect("failed to execute grist");

    let code = out.status.code().unwrap_or(1);
    if code != 0 {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let msg: String = stderr.trim().chars().take(300).collect();
        let mut obj = Vec::new();
        obj_set(
            &mut obj,
            "systemMessage",
            s(format!("initialization_gate: grist gate 실패(exit {code}) — {msg}")),
        );
        println!("{}", json::dumps(&Value::Obj(obj)));
        return;
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let gate_json = json::parse(&stdout).unwrap_or(Value::Obj(vec![]));
    let project_dir = gate_json
        .get("uninitialized")
        .filter(|v| v.truthy())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let session_raw = event
        .get("session_id")
        .filter(|v| v.truthy())
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let session = sanitize(session_raw);
    let s_dir = state_dir();
    let state_file = s_dir.join(format!("{session}.json"));

    if project_dir.is_empty() {
        let _ = std::fs::remove_file(&state_file);
        println!("{{}}");
        return;
    }

    let state = std::fs::read_to_string(&state_file)
        .ok()
        .and_then(|t| json::parse(&t).ok())
        .unwrap_or(Value::Obj(vec![]));

    let blocks = if state.get("project_dir").and_then(Value::as_str) == Some(project_dir.as_str()) {
        state.get("blocks").and_then(Value::as_i64).unwrap_or(0)
    } else {
        0
    };

    if blocks >= MAX_BLOCKS {
        let _ = std::fs::remove_file(&state_file);
        let mut obj = Vec::new();
        obj_set(
            &mut obj,
            "systemMessage",
            s(format!(
                "initialization_gate: {project_dir} 미초기화 상태로 {MAX_BLOCKS}회 차단 뒤 종료를 허용했어요."
            )),
        );
        println!("{}", json::dumps(&Value::Obj(obj)));
        return;
    }

    let _ = std::fs::create_dir_all(&s_dir);
    let mut new_state = Vec::new();
    obj_set(&mut new_state, "blocks", Value::Num((blocks + 1).to_string()));
    obj_set(&mut new_state, "project_dir", s(&project_dir));
    let _ = std::fs::write(&state_file, json::dumps(&Value::Obj(new_state)));

    let reason = format!(
        "[{}/{MAX_BLOCKS}] 종료 차단: {project_dir}는 `AGENTS.md`에 `# 상태`(`### grist` `ID :`) 헤더가 없는 미초기화 프로젝트인데 변경 파일이 있어요. \
`manage-project` 초기화 절차(`references/initialize.md`)로 Table7 행·Obsidian 문서·`# 상태` 헤더를 만들고, 현재 요청을 `request-record`·`task-add`·`task-goal`로 기록한 뒤 \
`manage-project` 흐름을 이어가세요.",
        blocks + 1
    );
    let mut out_obj = Vec::new();
    obj_set(&mut out_obj, "decision", s("block"));
    obj_set(&mut out_obj, "reason", s(reason));
    println!("{}", json::dumps(&Value::Obj(out_obj)));
}
