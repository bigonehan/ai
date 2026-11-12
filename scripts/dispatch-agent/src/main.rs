//! Runs one Table26 row's worker (headless `codex exec` or `gemini -p`) in a new tmux pane.
//! The orchestrator reads `.ai/dispatch/<row>.{log,exit,last}`, never the pane text.

#[path = "../../runtime-kind.rs"]
mod runtime_kind;
use runtime_kind::{runtime_kind, RuntimeKind};
#[path = "../../analysis-report.rs"]
mod analysis_report;
use analysis_report::analysis_report_problem;
use std::fs::{self, File, OpenOptions};
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::net::{TcpStream, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: dispatch-agent [--resume|--attach --decision REASON] [--expect-seconds N] [--agent codex|gemini] [--role agent|verifier|improver] [--analysis] [--project DIR] --row ID [--rows ID,...] [--prompt-file F] [--add-dir DIR ...] [--watch-log PATH ...]\nexit 8: 제출점검 자동 재개 2회 초과";

struct Args {
    submit_resumes: u8,
    resume: bool,
    attach: bool,
    decision: Option<String>,
    agent: String,
    role: String,
    /// `resolve-deadlock` analysis run of a verifier.
    analysis: bool,
    project: PathBuf,
    row: u64,
    rows: Option<Vec<u64>>,
    prompt_file: PathBuf,
    /// Extra writable roots outside the project (codex `--add-dir`), e.g. `~/ai/scripts` for a workflow row.
    add_dirs: Vec<PathBuf>,
    watch_logs: Vec<PathBuf>,
    expect_seconds: Option<u64>,
}

struct Paths {
    dir: PathBuf,
    prompt: PathBuf,
    script: PathBuf,
    log: PathBuf,
    exit: PathBuf,
    last: PathBuf,
    window: PathBuf,
    pane: PathBuf,
    started: PathBuf,
    first: PathBuf,
    resumes: PathBuf,
    /// One line per finished agent run: `<end_ms> <run_ms>` (actual run time, waits between dispatches excluded).
    runs: PathBuf,
    seen: PathBuf,
}

/// A row of the extensions root is workflow work, so its worker always needs the tools repo writable.
fn workflow_add_dirs(project: &Path, home: &Path, add_dirs: &mut Vec<PathBuf>) {
    let tools = home.join("ai");
    if project == home.join("extensions") && tools.is_dir() && !add_dirs.contains(&tools) { add_dirs.push(tools); }
}

/// The verifier records Oat screen issues in `build-ui/ui-oat/references/notes.md` (role-verifier).
fn verifier_add_dirs(role: &str, home: &Path, add_dirs: &mut Vec<PathBuf>) {
    let notes = home.join("ai/skills/build-ui/ui-oat/references");
    if role == "verifier" && notes.is_dir() && !add_dirs.contains(&notes) { add_dirs.push(notes); }
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let get = |flag: &str| -> Result<String, String> {
        let at = argv.iter().position(|a| a == flag).ok_or(format!("missing {flag}\n{USAGE}"))?;
        argv.get(at + 1).cloned().ok_or(format!("{flag} needs a value\n{USAGE}"))
    };
    let resume = argv.iter().any(|arg| arg == "--resume");
    let expect_seconds = if argv.iter().any(|arg| arg == "--expect-seconds") {
        let value = get("--expect-seconds")?.parse::<u64>().map_err(|_| "--expect-seconds must be a positive integer".to_string())?;
        if value == 0 { return Err("--expect-seconds must be a positive integer".into()); }
        Some(value)
    } else { None };
    let attach = argv.iter().any(|arg| arg == "--attach");
    if resume && attach { return Err("--resume and --attach cannot be combined".into()); }
    let decision = if attach {
        let value = argv.iter().position(|arg| arg == "--decision")
            .and_then(|at| argv.get(at + 1)).filter(|value| !value.starts_with("--") && !value.trim().is_empty())
            .ok_or("--attach requires --decision \"<판단 사유>\"")?;
        Some(value.trim().to_owned())
    } else {
        if argv.iter().any(|arg| arg == "--decision") { return Err("--decision requires --attach".into()); }
        None
    };
    let agent = if (resume || attach) && !argv.iter().any(|arg| arg == "--agent") { "codex".to_string() } else { get("--agent")? };
    if agent != "codex" && agent != "gemini" {
        return Err(format!("--agent must be codex or gemini\n{USAGE}"));
    }
    if resume && agent != "codex" {
        return Err("--resume supports codex only".into());
    }
    let role = if argv.iter().any(|arg| arg == "--role") { get("--role")? } else { "agent".into() };
    if !matches!(role.as_str(), "agent" | "verifier" | "improver") { return Err(format!("--role must be agent, verifier or improver\n{USAGE}")); }
    let analysis = argv.iter().any(|arg| arg == "--analysis");
    if analysis && role != "verifier" { return Err("--analysis requires --role verifier".into()); }
    if analysis && resume { return Err("--analysis starts a new session and cannot use --resume".into()); }
    if analysis && argv.iter().any(|arg| arg == "--rows") { return Err("--analysis cannot use --rows".into()); }
    let row = get("--row")?.parse().map_err(|_| format!("--row must be a number\n{USAGE}"))?;
    let rows = if argv.iter().any(|arg| arg == "--rows") {
        if role != "verifier" { return Err("--rows requires --role verifier".into()); }
        let raw = get("--rows")?;
        let ids = raw.split(',').map(|part| part.parse::<u64>()).collect::<Result<Vec<_>, _>>()
            .map_err(|_| format!("--rows must be comma-separated row IDs\n{USAGE}"))?;
        if ids.is_empty() || ids.contains(&0) { return Err(format!("--rows must be comma-separated row IDs\n{USAGE}")); }
        Some(ids)
    } else { None };
    let project_arg = if resume && !argv.iter().any(|arg| arg == "--project") { ".".to_string() } else { get("--project")? };
    let project = fs::canonicalize(project_arg).map_err(|e| format!("--project: {e}"))?;
    let prompt_file = if attach { PathBuf::new() } else { PathBuf::from(get("--prompt-file")?) };
    let mut add_dirs = Vec::new();
    for (at, arg) in argv.iter().enumerate() {
        if arg == "--add-dir" {
            let dir = argv.get(at + 1).ok_or(format!("--add-dir needs a value\n{USAGE}"))?;
            add_dirs.push(fs::canonicalize(dir).map_err(|e| format!("--add-dir {dir}: {e}"))?);
        }
    }
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    workflow_add_dirs(&project, &home, &mut add_dirs);
    verifier_add_dirs(&role, &home, &mut add_dirs);
    let mut watch_logs = Vec::new();
    for (at, arg) in argv.iter().enumerate() {
        if arg == "--watch-log" {
            let path = argv.get(at + 1).filter(|value| !value.starts_with("--"))
                .ok_or(format!("--watch-log needs a path\n{USAGE}"))?;
            watch_logs.push(PathBuf::from(path));
        }
    }
    Ok(Args { submit_resumes: 0, resume, attach, decision, agent, role, analysis, project, row, rows, prompt_file, add_dirs, watch_logs, expect_seconds })
}

fn paths(project: &Path, row: u64, role: &str) -> Paths {
    let dir = project.join(".ai").join("dispatch");
    let suffix = match role { "verifier" => ".verify", "improver" => ".improve", _ => "" };
    let file = |ext: &str| dir.join(format!("{row}{suffix}.{ext}"));
    Paths { prompt: file("prompt"), script: file("sh"), log: file("log"), exit: file("exit"), last: file("last"), window: file("window"), pane: file("pane"), started: file("started"), first: file("first"), resumes: file("resumes"), runs: file("runs"), seen: file("seen"), dir }
}

/// 대행자 위임에 record-log 에이전트 ID가 없으면 발급해 프롬프트에 붙인다(대행자가 ID를 물으며 멈추는 재위임 방지).
fn agent_id_line(project: &Path, row: u64, role: &str, original: &[u8], issue: impl Fn(&Path, u64) -> Option<String>) -> Option<String> {
    if role != "agent" { return None; }
    let has_id_header = String::from_utf8_lossy(original).lines().any(|line| {
        let Some(value) = line.strip_prefix("에이전트 ID: ") else { return false; };
        let Some(id) = value.get(..11) else { return false; };
        id.as_bytes()[..10].iter().all(u8::is_ascii_digit)
            && id.as_bytes()[10].is_ascii_alphanumeric()
            && value[11..].chars().next().is_none_or(|ch| ch.is_whitespace() || ch == '(')
    });
    if has_id_header { return None; }
    let id = issue(project, row)?;
    Some(format!("에이전트 ID: {id} (작업 게이트에 --agent-id {id})\n"))
}

fn issue_agent_id(project: &Path, row: u64) -> Option<String> {
    let conversation = ["CODEX_THREAD_ID", "CLAUDE_CODE_SESSION_ID", "CLAUDE_SESSION_ID"]
        .into_iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "dispatch".into());
    let output = Command::new("/home/tree/ai/skills/record-log/scripts/bin/record-log")
        .args(["start", &project.to_string_lossy(), &row.to_string(), "--role", &format!("codex_{row}"), "--conversation", &conversation])
        .output().ok()?;
    let id = String::from_utf8_lossy(&output.stdout).lines().last()?.trim().to_string();
    (output.status.success() && id.len() == 11 && id.as_bytes()[..10].iter().all(u8::is_ascii_digit) && id.as_bytes()[10].is_ascii_alphanumeric()).then_some(id)
}

fn role_header(sender: Option<&str>, role: &str, row: u64, rows: Option<&[u64]>) -> String {
    let from = match sender { Some("worker") => "대행자", Some("verifier") => "검증자", Some("improver") => "개선자", _ => "위임자(main)" };
    let (to, skill) = match role { "verifier" => ("검증자", "role-verifier"), "improver" => ("개선자", "role-improver"), _ => ("대행자", "role-agent") };
    let row_label = rows.map(|ids| ids.iter().map(u64::to_string).collect::<Vec<_>>().join(","))
        .unwrap_or_else(|| row.to_string());
    format!("[Role] from={from} to={to} row={row_label} skill={skill}\n")
}

fn role_doc_line(project: &Path, role: &str) -> String {
    if role != "verifier" { return String::new(); }
    let path = project.join(".ai/docs/role-verifier.md");
    if path.is_file() { format!("역할 문서: {}\n", path.display()) } else { String::new() }
}

fn runtime_line(agents: &str) -> String {
    let mut status = false;
    let mut execution = false;
    for line in agents.lines() {
        if line.starts_with("# ") { status = line == "# 상태"; execution = false; }
        else if status && line.starts_with("### ") { execution = line == "### 실행"; }
        if status && execution && line.starts_with("runtime :") {
            return format!("{line}\n");
        }
    }
    String::new()
}

fn verifier_runtime_line(project: &Path, role: &str) -> String {
    if role != "verifier" { return String::new(); }
    fs::read_to_string(project.join("AGENTS.md"))
        .map(|agents| runtime_line(&agents)).unwrap_or_default()
}

fn already_verified_local_cli_hook(project: &Path, rows: &[u64]) -> bool {
    if project != Path::new("/home/tree/extensions") || runtime_kind(project) != Ok(RuntimeKind::Cli) {
        return false;
    }
    local_cli_hook_verified_in_gate(project, rows)
}

fn local_cli_hook_verified_in_gate(project: &Path, rows: &[u64]) -> bool {
    rows.iter().any(|row| {
        let path = project.join(".ai/gate").join(format!("{row}.json"));
        fs::read_to_string(path).ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|gate| gate.get("hidden").and_then(|value| value.as_array()).cloned())
            .is_some_and(|cases| cases.iter().any(|case| case.get("test").and_then(|value| value.as_str())
                .is_some_and(|command| command.contains("task-complete --hook") || command.contains("main-edit-gate"))))
    })
}

const VERIFIER_DOCS: &str = "~/ai/skills/role-verifier/references";

/// 검증자가 읽을 모드·런타임 파일 한 줄(`role-verifier` 라우팅 표). 교착 분석은 resolve-deadlock만 따른다.
fn verifier_docs_line(role: &str, analysis: bool, prompt: &[u8], runtime: Result<RuntimeKind, String>) -> String {
    if role != "verifier" || analysis { return String::new(); }
    let mode = if String::from_utf8_lossy(prompt).contains("관측") { "observe" } else { "flow" };
    let mut docs = vec![format!("{VERIFIER_DOCS}/{mode}.md")];
    if let Ok(kind) = runtime {
        let name = match kind { RuntimeKind::Web => "web", RuntimeKind::App => "app", RuntimeKind::Cli => "cli" };
        docs.push(format!("{VERIFIER_DOCS}/runtime-{name}.md"));
    }
    format!("읽을 문서: {}\n", docs.join(", "))
}

#[derive(Clone)]
struct RuntimeProcess {
    pid: u32,
    cwd: PathBuf,
    vite_command: Option<String>,
    started: SystemTime,
}

trait RuntimeProbe {
    fn listeners(&self) -> Result<Vec<(u16, u32)>, String>;
    fn process(&self, pid: u32) -> Result<Option<RuntimeProcess>, String>;
}

struct SystemRuntimeProbe;

fn parse_ss_listeners(output: &str) -> Vec<(u16, u32)> {
    let mut found = Vec::new();
    for line in output.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first() != Some(&"LISTEN") { continue; }
        let Some(port) = fields.get(3).and_then(|address| address.rsplit_once(':'))
            .and_then(|(_, port)| port.parse::<u16>().ok()) else { continue };
        for part in line.split("pid=").skip(1) {
            if let Some(pid) = part.split(|ch: char| !ch.is_ascii_digit()).next()
                .and_then(|digits| digits.parse::<u32>().ok()) {
                found.push((port, pid));
            }
        }
    }
    found
}

impl RuntimeProbe for SystemRuntimeProbe {
    fn listeners(&self) -> Result<Vec<(u16, u32)>, String> {
        let output = Command::new("ss").args(["-ltnpH"]).output()
            .map_err(|error| format!("ss: {error}"))?;
        if !output.status.success() { return Err(format!("ss: exit {}", output.status)); }
        Ok(parse_ss_listeners(&String::from_utf8_lossy(&output.stdout)))
    }

    fn process(&self, pid: u32) -> Result<Option<RuntimeProcess>, String> {
        let proc_dir = PathBuf::from(format!("/proc/{pid}"));
        let cwd = match fs::read_link(proc_dir.join("cwd")) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("pid {pid} cwd: {error}")),
        };
        let stat = fs::read_to_string(proc_dir.join("stat"))
            .map_err(|error| format!("pid {pid} stat: {error}"))?;
        let after_comm = stat.rsplit_once(") ").ok_or(format!("pid {pid} stat malformed"))?.1;
        let ticks = after_comm.split_whitespace().nth(19)
            .and_then(|value| value.parse::<u64>().ok()).ok_or(format!("pid {pid} starttime missing"))?;
        let boot = fs::read_to_string("/proc/stat").map_err(|error| format!("btime: {error}"))?
            .lines().find_map(|line| line.strip_prefix("btime ").and_then(|value| value.parse::<u64>().ok()))
            .ok_or("btime missing")?;
        let clock = Command::new("getconf").arg("CLK_TCK").output()
            .map_err(|error| format!("CLK_TCK: {error}"))?;
        let hz = String::from_utf8_lossy(&clock.stdout).trim().parse::<u64>()
            .map_err(|_| "CLK_TCK invalid")?;
        if hz == 0 { return Err("CLK_TCK zero".into()); }
        let started = UNIX_EPOCH + Duration::from_secs(boot + ticks / hz)
            + Duration::from_nanos((ticks % hz) * 1_000_000_000 / hz);
        let cmdline = fs::read(proc_dir.join("cmdline"))
            .map_err(|error| format!("pid {pid} cmdline: {error}"))?;
        let vite_command = cmdline.split(|byte| *byte == 0).filter_map(|part| std::str::from_utf8(part).ok())
            .find(|arg| arg.contains("node_modules/.bin/vite") || arg.ends_with("/vite/bin/vite.js"))
            .map(str::to_owned);
        Ok(Some(RuntimeProcess { pid, cwd, vite_command, started }))
    }
}

fn runtime_ports(line: &str) -> Vec<u16> {
    let mut ports = Vec::new();
    for prefix in ["localhost:", "127.0.0.1:"] {
        for part in line.split(prefix).skip(1) {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(port) = digits.parse::<u16>() {
                if !ports.contains(&port) { ports.push(port); }
            }
        }
    }
    ports
}

fn newest_runtime_file(root: &Path) -> Result<Option<(PathBuf, SystemTime)>, String> {
    let mut files = Vec::new();
    let server = root.join("server");
    if server.is_dir() {
        let mut dirs = vec![server];
        while let Some(dir) = dirs.pop() {
            for entry in fs::read_dir(&dir).map_err(|error| format!("runtime-fresh read: {error}"))? {
                let entry = entry.map_err(|error| format!("runtime-fresh entry: {error}"))?;
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "__tests__" || name == "test" || name == "tests"
                    || name.contains(".test.") || name.contains(".spec.") { continue; }
                if path.is_dir() { dirs.push(path); } else if path.is_file() { files.push(path); }
            }
        }
    }
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("vite.config.") && entry.path().is_file() { files.push(entry.path()); }
        }
    }
    let package = root.join("package.json");
    if package.is_file() { files.push(package); }
    let mut latest = None;
    for path in files {
        let modified = fs::metadata(&path).and_then(|meta| meta.modified())
            .map_err(|error| format!("runtime-fresh mtime: {error}"))?;
        if latest.as_ref().is_none_or(|(_, time): &(PathBuf, SystemTime)| modified > *time) {
            latest = Some((path, modified));
        }
    }
    Ok(latest)
}

fn runtime_time(time: SystemTime) -> String {
    let seconds = time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs().to_string();
    Command::new("date").args(["-d", &format!("@{seconds}"), "+%y%m%d %H:%M:%S"])
        .output().ok().filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or(seconds)
}

fn runtime_loaded_time(cwd: &Path, port: u16) -> Option<SystemTime> {
    let loaded = fs::read_to_string(cwd.join(format!(".ai/runtime/{port}.loaded"))).ok()?;
    let iso = loaded.trim();
    let bytes = iso.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T'
        || bytes[13] != b':' || bytes[16] != b':' || !bytes[..19].iter().enumerate()
            .all(|(index, byte)| [4, 7, 10, 13, 16].contains(&index) || byte.is_ascii_digit()) {
        return None;
    }
    let suffix = &iso[19..];
    let zone = suffix.strip_suffix('Z').map(|fraction| (fraction, ""))
        .or_else(|| suffix.rsplit_once(['+', '-']).filter(|(_, offset)|
            offset.len() == 5 && offset.as_bytes()[2] == b':' && offset.as_bytes().iter()
                .enumerate().all(|(index, byte)| index == 2 || byte.is_ascii_digit())));
    let (fraction, _) = zone?;
    if !fraction.is_empty() && (!fraction.starts_with('.') || fraction.len() == 1
        || !fraction[1..].bytes().all(|byte| byte.is_ascii_digit())) { return None; }
    let output = Command::new("date").args(["-d", iso, "+%s.%N"]).output().ok()?;
    if !output.status.success() { return None; }
    let printed = std::str::from_utf8(&output.stdout).ok()?.trim();
    let (seconds, nanos) = printed.split_once('.')?;
    Some(UNIX_EPOCH + Duration::new(seconds.parse().ok()?, nanos.parse().ok()?))
}

fn runtime_fresh_with(project: &Path, extensions: &Path, line: &str, probe: &impl RuntimeProbe) -> Result<bool, String> {
    let ports = runtime_ports(line);
    if ports.is_empty() { return Ok(false); }
    let listeners = probe.listeners()?;
    let mut processes = HashMap::new();
    for &(_, pid) in &listeners {
        if let std::collections::hash_map::Entry::Vacant(entry) = processes.entry(pid) {
            if let Some(process) = probe.process(pid)? { entry.insert(process); }
        }
    }
    for &port in &ports {
        for &(_, pid) in listeners.iter().filter(|(number, _)| *number == port) {
            let Some(process) = processes.get(&pid) else { continue };
            if !process.cwd.starts_with(extensions) { continue; }
            let mut latest = None;
            for root in [&process.cwd, project] {
                if let Some((path, modified)) = newest_runtime_file(root)? {
                    if latest.as_ref().is_none_or(|(_, time): &(PathBuf, SystemTime)| modified > *time) {
                        latest = Some((path, modified));
                    }
                }
            }
            let loaded = runtime_loaded_time(&process.cwd, port);
            let reference = loaded.map_or(process.started, |time| time.max(process.started));
            if let Some((path, modified)) = latest.filter(|(_, modified)| *modified > reference) {
                let relative = path.strip_prefix(extensions).unwrap_or(&path);
                let label = if loaded.is_some() { "loaded" } else { "started" };
                return Err(format!("stale runtime: {port} pid {pid} {label} {} < {} modified {} — 런타임 재시작 필요",
                    runtime_time(reference), relative.display(), runtime_time(modified)));
            }
            if let Some(vite) = &process.vite_command {
                let mut orphan: Vec<_> = listeners.iter().filter_map(|(other_port, other_pid)| {
                    let other = processes.get(other_pid)?;
                    if !ports.contains(other_port) && other.cwd == process.cwd && other.vite_command.as_ref() == Some(vite) {
                        Some((*other_port, other.pid))
                    } else { None }
                }).collect();
                orphan.sort_unstable();
                orphan.dedup();
                if !orphan.is_empty() {
                    return Err(format!("orphan runtime: ports {} pids {} — 남은 서버 종료 필요",
                        orphan.iter().map(|(port, _)| port.to_string()).collect::<Vec<_>>().join(","),
                        orphan.iter().map(|(_, pid)| pid.to_string()).collect::<Vec<_>>().join(",")));
                }
            }
        }
    }
    Ok(true)
}

fn section_items(doc: &str, section: &str) -> Vec<(String, String)> {
    let mut in_section = false;
    let mut items = Vec::new();
    for line in doc.lines() {
        if line.starts_with("# ") {
            in_section = line.trim() == section;
            continue;
        }
        if !in_section { continue; }
        if let Some(item) = line.strip_prefix("- ") {
            if let Some((name, command)) = item.split_once(" : ") {
                if !name.trim().is_empty() && !command.trim().is_empty() {
                    items.push((name.trim().to_owned(), command.trim().to_owned()));
                }
            }
        }
    }
    items
}

fn preflight_reason(output: &[u8], status: std::process::ExitStatus) -> String {
    let text = String::from_utf8_lossy(output);
    text.lines().rev().map(str::trim).find(|line| !line.is_empty())
        .map(|line| line.replace(['\r', '\n', '|'], " ").chars().take(80).collect())
        .unwrap_or_else(|| format!("명령 실패 (exit {})", status.code().unwrap_or(1)))
}

fn preflight_log(name: &str, ack: &str, error: &str) {
    let clean = |value: &str| -> String {
        value.replace(['\r', '\n', '|'], " ").split_whitespace()
            .map(|part| if part.starts_with('/') || part.contains("/home/") || part.contains("\\Users\\") { "[경로]" } else { part })
            .collect::<Vec<_>>().join(" ").chars().take(80).collect()
    };
    eprintln!("00119805481 | dispatch-agent.preflight | 01 | ack={ack} body.check={} error={}",
        clean(name), clean(error));
}

fn submit_safe(value: &str, limit: usize) -> String {
    value.replace(['\r', '\n', '|'], " ").split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if part.starts_with('/') || lower.contains("/home/") || lower.contains("\\users\\") { "[경로]" }
            else if ["token=", "authorization=", "api_key=", "apikey=", "secret="].iter().any(|key| lower.contains(key)) { "[비밀값]" }
            else { part }
        }).collect::<Vec<_>>().join(" ").chars().take(limit).collect()
}

#[derive(Debug)]
struct SubmitFailure { name: String, command: String, reason: String, tail: String }

fn submit_check_log(name: &str, ack: &str, error: &str) {
    eprintln!("00119807681 | dispatch-agent.submit-check | 01 | ack={ack} body.check={} error={}",
        submit_safe(name, 80), submit_safe(error, 120));
}

fn submit_check_items_with(
    items: &[(String, String)], project: &Path, mut execute: impl FnMut(&Path, &str) -> io::Result<Output>,
) -> Result<bool, SubmitFailure> {
    for (name, command) in items {
        let result = execute(project, command);
        let failure = match result {
            Ok(output) if output.status.success() => None,
            Ok(output) => {
                let text = if output.stdout.is_empty() { &output.stderr } else { &output.stdout };
                let decoded = String::from_utf8_lossy(text);
                let lines: Vec<_> = decoded.lines().map(str::trim)
                    .filter(|line| !line.is_empty()).map(|line| submit_safe(line, 120)).collect();
                let reason = if [Some(124), Some(137)].contains(&output.status.code()) { "시간 초과".into() }
                    else { lines.last().cloned().unwrap_or_else(|| format!("명령 실패 (exit {})", output.status.code().unwrap_or(1))) };
                Some((reason, lines.iter().rev().take(5).rev().cloned().collect::<Vec<_>>().join(" / ")))
            }
            Err(error) => Some((submit_safe(&format!("실행 불가: {error}"), 120), String::new())),
        };
        if let Some((reason, tail)) = failure {
            submit_check_log(name, "rejected", &reason);
            return Err(SubmitFailure { name: submit_safe(name, 80), command: command.clone(), reason, tail });
        }
        submit_check_log(name, "received", "none");
    }
    Ok(!items.is_empty())
}

fn submit_check_outcome_with(
    role: &str, analysis: bool, code: &str, state: &str, doc: &str, project: &Path,
    execute: impl FnMut(&Path, &str) -> io::Result<Output>,
) -> Result<bool, SubmitFailure> {
    if role != "agent" || analysis || code.trim() != "0" || state != "검증" { return Ok(false); }
    submit_check_items_with(&section_items(doc, "# 제출점검"), project, execute)
}

fn submit_check_retry_allowed(resumes: u8, agent: &str) -> bool { resumes < 2 && agent == "codex" }

fn record_submit_failure_with(project: &Path, row: u64, failure: &SubmitFailure,
    call: impl FnOnce(&[&str]) -> io::Result<Output>) -> Result<(), String> {
    let fields = serde_json::json!({"I":format!("제출점검 {} 실패: {}", failure.name, failure.reason), "H":"교정"}).to_string();
    let output = call(&["task-update", &row.to_string(), "--project", &project.to_string_lossy(), "--fields", &fields])
        .map_err(|error| format!("submit-check task-update: {error}"))?;
    if output.status.success() { Ok(()) } else { Err(format!("submit-check task-update: {}", grist_failure_error(&output))) }
}

const BROWSER_PROFILE: &str = "/home/tree/.config/codex-run-cdp";
const STAGEHAND_RUNTIME: &str = "/home/tree/.local/share/access-stagehand";
const STAGEHAND_STATE: &str = "/home/tree/.local/state/access-stagehand";
const BROWSER_SCRIPT: &str = "/home/tree/ai/skills/access-stagehand/scripts/stagehand.mjs";

fn browser_log(ack: &str, mode: &str, error: &str) {
    let result = if ack == "received" {
        format!("ack=received body.mode={mode} error=none")
    } else {
        let stage = if error.contains("probe=status") || error.contains("probe=no-body")
            || error.contains("probe=json") || error.contains("probe=no-ws-url") {
            "response_validate"
        } else { "send" };
        format!("ack=rejected body=none stage={stage} error={error}")
    };
    let status = Command::new("/home/tree/ai/skills/record-log/scripts/bin/record-log")
        .args(["append", "00119805931", "dispatch-agent.browser", "01", &result,
            "--project", "/home/tree/extensions"])
        .status();
    if !status.is_ok_and(|code| code.success()) {
        eprintln!("dispatch-agent: browser record-log append failed");
    }
}

fn browser_endpoint_at(address: SocketAddr) -> Result<(), &'static str> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1)).map_err(|_| "connect")?;
    stream.set_write_timeout(Some(Duration::from_secs(1))).map_err(|_| "write")?;
    stream.write_all(b"GET /json/version HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: keep-alive\r\n\r\n")
        .map_err(|_| "write")?;
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut response = Vec::new();
    let mut header_end = None;
    let mut body_len = None;
    loop {
        if let (Some(start), Some(len)) = (header_end, body_len) {
            if response.len() >= start + len {
                let value: serde_json::Value = serde_json::from_slice(&response[start..start + len]).map_err(|_| "json")?;
                return if value.get("webSocketDebuggerUrl").and_then(|url| url.as_str()).is_some_and(|url| !url.is_empty()) {
                    Ok(())
                } else { Err("no-ws-url") };
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { return Err("read-timeout"); }
        stream.set_read_timeout(Some(remaining)).map_err(|_| "read-timeout")?;
        let mut chunk = [0_u8; 2048];
        match stream.read(&mut chunk) {
            Ok(0) => return Err("no-body"),
            Ok(n) => response.extend_from_slice(&chunk[..n]),
            Err(error) if matches!(error.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => return Err("read-timeout"),
            Err(_) => return Err("read"),
        }
        if response.len() > 8192 { return Err("no-body"); }
        if header_end.is_none() {
            if let Some(at) = response.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&response[..at]).map_err(|_| "status")?;
                if !headers.lines().next().is_some_and(|line| line.starts_with("HTTP/1.1 200 ") || line.starts_with("HTTP/1.0 200 ")) {
                    return Err("status");
                }
                let len = headers.lines().find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("Content-Length").then(|| value.trim().parse::<usize>().ok()).flatten()
                }).filter(|len| *len > 0 && *len <= 8192).ok_or("no-body")?;
                header_end = Some(at + 4);
                body_len = Some(len);
            }
        }
    }
}

fn browser_has_legacy_allow_only() -> bool {
    let allow_only = Command::new("ps").args(["-eo", "args="]).output().ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .is_some_and(|text| text.lines().any(|line| {
            line.contains("--remote-debugging-port=9222")
                && !line.contains("--type=")
                && line.contains("--disable-extensions-except=")
        }));
    let worker_failed = fs::read_to_string("/tmp/run_cdp-9222.log")
        .is_ok_and(|log| log.contains("DidStartWorkerFail"));
    allow_only || worker_failed
}

fn browser_endpoint() -> Result<(), &'static str> {
    if browser_has_legacy_allow_only() { return Err("legacy-allow-only"); }
    browser_endpoint_at(SocketAddr::from(([127, 0, 0, 1], 9222)))
}

fn ensure_browser_with(probe: impl Fn() -> Result<(), &'static str>, mut launch: impl FnMut() -> Result<(), String>,
    deadline: Instant, pause: impl Fn(Duration)) -> Result<&'static str, String> {
    let started = Instant::now();
    let mut tries = 1;
    let mut last_probe = match probe() { Ok(()) => return Ok("attach"), Err(reason) => reason };
    launch().map_err(|_| "launch-failed".to_string())?;
    while Instant::now() < deadline {
        tries += 1;
        match probe() { Ok(()) => return Ok("launch"), Err(reason) => last_probe = reason }
        pause(Duration::from_millis(200));
    }
    Err(format!("endpoint-absent probe={last_probe} tries={tries} ms={}", started.elapsed().as_millis()))
}

fn browser_launch_command() -> Command {
    let mut command = Command::new("node");
    command.args([BROWSER_SCRIPT, "browser", "--detach"])
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}

fn ensure_verifier_browser(role: &str) -> Result<String, String> {
    if role != "verifier" { return Ok(String::new()); }
    let deadline = Instant::now() + Duration::from_secs(20);
    let result = ensure_browser_with(browser_endpoint, || {
        let mut child = browser_launch_command().spawn().map_err(|_| "launch-failed")?;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(_)) | Err(_) => return Err("launch-failed".into()),
                Ok(None) => sleep(Duration::from_millis(100)),
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        Err("launch-failed".into())
    }, deadline, sleep);
    match result {
        Ok(mode) => { browser_log("received", mode, "none"); Ok(format!("사전점검: 통과 — browser({mode})\n")) }
        Err(reason) => { browser_log("rejected", "none", &reason); Err(format!("실패 — 사전점검: browser({reason})")) }
    }
}

fn writable_dirs(role: &str, agent: &str, supplied: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = supplied.to_vec();
    if role == "improver" {
        let ai = PathBuf::from(IMPROVER_ROOT);
        if !dirs.contains(&ai) { dirs.push(ai); }
    }
    if role == "verifier" && agent == "codex" {
        for path in [PathBuf::from(BROWSER_PROFILE), PathBuf::from(STAGEHAND_RUNTIME),
            PathBuf::from(STAGEHAND_STATE), std::env::temp_dir().join("access-stagehand-locks")] {
            if !dirs.contains(&path) { dirs.push(path); }
        }
    }
    dirs
}

fn prepare_writable_dirs(role: &str, agent: &str, state: &Path, lock: &Path) -> Result<(), String> {
    if role != "verifier" || agent != "codex" { return Ok(()); }
    for dir in [state, lock] {
        fs::create_dir_all(dir).map_err(|error| format!("verifier writable directory: {error}"))?;
    }
    Ok(())
}

fn verifier_preflight(project: &Path, role: &str) -> Result<String, String> {
    if role != "verifier" { return Ok(String::new()); }
    let runtime = verifier_runtime_line(project, role);
    let roots: Vec<PathBuf> = match std::env::var("T26_WORKFLOW_ROOTS") {
        Ok(v) if !v.trim().is_empty() => v.split(':').filter(|r| !r.is_empty()).map(PathBuf::from).collect(),
        _ => vec![PathBuf::from("/home/tree/extensions"), PathBuf::from("/home/tree/project")],
    };
    let workflow_root = roots.into_iter().find(|root| project.starts_with(root)).unwrap_or_else(|| PathBuf::from("/home/tree/extensions"));
    let runtime_checked = match runtime_fresh_with(project, &workflow_root, &runtime, &SystemRuntimeProbe) {
        Ok(checked) => checked,
        Err(reason) => {
            preflight_log("runtime-fresh", "rejected", &reason);
            return Err(if reason.starts_with("stale runtime:") || reason.starts_with("orphan runtime:") {
                reason
            } else { format!("실패 — 사전점검: runtime-fresh({reason})") });
        }
    };
    if runtime_checked { preflight_log("runtime-fresh", "received", "none"); }
    let doc = match fs::read_to_string(project.join(".ai/docs/role-verifier.md")) {
        Ok(doc) => doc,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(if runtime_checked { "사전점검: 통과 — runtime-fresh\n".into() } else { String::new() }),
        Err(error) => return Err(format!("role-verifier.md: {error}")),
    };
    let items = section_items(&doc, "# 사전점검");
    if items.is_empty() { return Ok(if runtime_checked { "사전점검: 통과 — runtime-fresh\n".into() } else { String::new() }); }
    let mut blocked = Vec::new();
    for (name, command) in &items {
        let script = format!("exec 2>&1; {command}");
        let result = Command::new("timeout").args(["--signal=KILL", "20s", "sh", "-c", &script])
            .current_dir(project).output();
        let reason = match result {
            Ok(output) if output.status.success() => None,
            Ok(output) if output.status.code() == Some(124) || output.status.code() == Some(137) => Some("시간 초과".to_owned()),
            Ok(output) => {
                let text = if output.stdout.is_empty() { &output.stderr } else { &output.stdout };
                let reason = preflight_reason(text, output.status);
                if output.status.code() == Some(127) {
                    Some(format!("실행 불가: {reason}"))
                } else { Some(reason) }
            }
            Err(error) => Some(format!("실행 불가: {error}")),
        };
        match reason {
            Some(reason) => {
                let category = if reason == "시간 초과" { "시간 초과" }
                    else if reason.starts_with("실행 불가:") { "실행 불가" } else { "명령 실패" };
                preflight_log(name, "rejected", &format!("{category}: {reason}"));
                blocked.push(format!("{name}({reason})"));
            }
            None => preflight_log(name, "received", "none"),
        }
    }
    if blocked.is_empty() {
        let mut names = if runtime_checked { vec!["runtime-fresh"] } else { Vec::new() };
        names.extend(items.iter().map(|(name, _)| name.as_str()));
        Ok(format!("사전점검: 통과 — {}\n", names.join(", ")))
    } else {
        Err(format!("실패 — 사전점검: {}", blocked.join("; ")))
    }
}

fn row_state(fields: &serde_json::Value) -> String {
    ["H", "I", "J", "K"].iter().map(|key| {
        let value = fields.get(*key).filter(|value| !value.is_null()).map(|value| {
            value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())
        }).unwrap_or_default();
        format!("{key}={}", value.replace('\n', " / "))
    }).collect::<Vec<_>>().join(" | ")
}

fn row_from_task(row: u64, output: &Output) -> Result<String, String> {
    if !output.status.success() { return Err(format!("grist task {row} failed")); }
    let records: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| format!("grist task {row} returned invalid JSON"))?;
    let fields = records.as_array().and_then(|records| records.iter().find(|record| record["id"].as_u64() == Some(row)))
        .and_then(|record| record.get("fields"))
        .ok_or_else(|| format!("grist task {row} did not return its row"))?;
    Ok(row_state(fields))
}

fn task_context(row: u64, rows: Option<&[u64]>, project: &Path) -> Result<String, String> {
    let ids = rows.unwrap_or(std::slice::from_ref(&row));
    let mut context = String::new();
    for &id in ids {
        let task = grist_output(&["task", &id.to_string(), "--project", &project.to_string_lossy()])
            .map_err(|error| format!("grist task {id}: {error}"))?;
        let state = row_from_task(id, &task)?;
        if rows.is_some() { context.push_str(&format!("row {id}: {state}\n")); }
        else { context.push_str(&format!("row: {state}\n")); }
        let goal = grist_output(&["goal", &id.to_string(), "--project", &project.to_string_lossy()])
            .map_err(|error| format!("grist goal {id}: {error}"))?;
        if !goal.status.success() { return Err(format!("grist goal {id} failed")); }
        context.push_str("목표:\n");
        context.push_str(&String::from_utf8_lossy(&goal.stdout));
        if !context.ends_with('\n') { context.push('\n'); }
    }
    Ok(context)
}

fn schema_file() -> PathBuf {
    std::env::var_os("RULE_SCHEMA_FILE").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("ai/rules/rule_schema.md")
    })
}

fn section<'a>(schema: &'a str, number: usize) -> &'a str {
    let heading = format!("## {number}. ");
    let Some(start) = schema.lines().position(|line| line.starts_with(&heading)) else { return "" };
    let lines: Vec<&str> = schema.lines().collect();
    let end = lines.iter().enumerate().skip(start + 1)
        .find(|(_, line)| line.starts_with("## ")).map(|(index, _)| index).unwrap_or(lines.len());
    let offset = lines[..start].iter().map(|line| line.len() + 1).sum::<usize>();
    let length = lines[start..end].iter().map(|line| line.len() + 1).sum::<usize>();
    &schema[offset..(offset + length).min(schema.len())]
}

fn schema_block(prompt: &str, schema: &str) -> String {
    let mapping = section(schema, 2);
    let definitions = section(schema, 1);
    let mut selected = Vec::new();
    for line in definitions.lines().filter(|line| line.starts_with("- `[")) {
        let Some(kind) = line.split('`').nth(1) else { continue };
        let explicit = prompt.contains(kind);
        let mapped = mapping.lines().filter(|line| line.starts_with("- `") && line.contains('→'))
            .any(|entry| {
                let symbol = entry.split('`').nth(1).unwrap_or("");
                entry.contains(&format!("`{kind}`")) && !symbol.is_empty() &&
                    prompt.replace('\u{fe0f}', "").contains(&symbol.replace('\u{fe0f}', ""))
            });
        if explicit || mapped { selected.push(line); }
    }
    let message = section(schema, 3);
    if selected.is_empty() && message.is_empty() { return String::new() }
    let mut block = String::from("[Schema v1]\n");
    for line in selected { block.push_str(line); block.push('\n'); }
    block.push_str(message);
    if !block.ends_with('\n') { block.push('\n'); }
    block
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn json_quote(value: &str) -> String {
    let mut quoted = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            c if c <= '\u{1f}' => quoted.push_str(&format!("\\u{:04x}", c as u32)),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

fn session_id_from_log(log: &str) -> Option<&str> {
    log.lines()
        .filter_map(|line| line.trim_start().strip_prefix("session id: ").map(str::trim))
        .filter(|id| !id.is_empty())
        .last()
}

fn required_session_id<'a>(log: &'a str, path: &Path) -> Result<&'a str, String> {
    session_id_from_log(log).ok_or_else(|| format!("no session id in {}", path.display()))
}

fn split_args<'a>(caller_pane: &'a str, last_agent_pane: Option<&'a str>) -> Vec<&'a str> {
    let (direction, target) = match last_agent_pane {
        Some(pane) => ("-v", pane),
        None => ("-h", caller_pane),
    };
    vec!["split-window", direction, "-d", "-t", target, "-P", "-F", "#{pane_id}", "-c"]
}

fn last_agent_pane(list: &str) -> Option<&str> {
    list.lines().filter_map(|line| {
        let (pane, marker) = line.split_once('\t')?;
        (!pane.is_empty() && !marker.is_empty()).then_some(pane)
    }).last()
}

fn tmux_call(operation: &str, command: &mut Command) -> Result<String, String> {
    let started = Instant::now();
    let output = command.output().map_err(|error| {
        let message = error.to_string();
        tmux_log(operation, "not_observed", started.elapsed().as_millis(), &message);
        format!("tmux {operation} failed: {message}")
    })?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tmux_log(operation, "rejected", started.elapsed().as_millis(), &message);
        return Err(format!("tmux {operation} failed: {message}"));
    }
    tmux_log(operation, "received", started.elapsed().as_millis(), "none");
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn tmux_log(operation: &str, ack: &str, ms: u128, error: &str) {
    eprintln!("{}", tmux_log_line(operation, ack, ms, error));
}

fn tmux_log_line(operation: &str, ack: &str, ms: u128, error: &str) -> String {
    let error = error.replace(['\r', '\n', '|'], " ");
    let prefix = format!("00119803531 | tmux.{operation} | 01 | ack={ack} body.ms={ms} error=");
    let available = 120usize.saturating_sub(prefix.len());
    format!("{prefix}{}", error.chars().take(available).collect::<String>())
}

struct TmuxTarget {
    socket: String,
    pane: String,
    window: String,
    mode: &'static str,
}

struct DetachedWindowGuard<F: FnOnce(&str)> {
    window: Option<String>,
    close: Option<F>,
}

impl<F: FnOnce(&str)> DetachedWindowGuard<F> {
    fn new(target: &TmuxTarget, close: F) -> Self {
        Self { window: (target.mode == "detached").then(|| target.window.clone()), close: Some(close) }
    }
    fn keep(&mut self) { self.window = None; }
}

impl<F: FnOnce(&str)> Drop for DetachedWindowGuard<F> {
    fn drop(&mut self) {
        if let Some(window) = self.window.take() {
            if let Some(close) = self.close.take() { close(&window); }
        }
    }
}

// `response` is display-message for a caller, or new-window's pane/window pair.
// `sessions` is only supplied when there is no caller tmux environment.
fn select_tmux_target(
    tmux: Option<&str>, pane: Option<&str>, sessions: Option<Result<&str, &str>>,
    response: Result<&str, &str>,
) -> Result<TmuxTarget, String> {
    if let Some(value) = tmux.filter(|value| !value.is_empty()) {
        let socket = value.split(',').next().filter(|value| !value.is_empty())
            .ok_or("dispatch-agent needs a tmux session (TMUX is not set)")?;
        let pane = pane.filter(|value| !value.is_empty())
            .ok_or("dispatch-agent needs a tmux pane (TMUX_PANE is not set)")?;
        let window = response.map_err(str::to_owned)?;
        return Ok(TmuxTarget { socket: socket.to_owned(), pane: pane.to_owned(), window: window.to_owned(), mode: "caller" });
    }
    let sessions = sessions.unwrap_or(Err("list-sessions not observed"));
    sessions.map_err(|error| format!("실패 — tmux 없음: {error}"))?
        .lines().find(|line| !line.is_empty())
        .ok_or("실패 — tmux 없음: 세션 없음")?;
    let pair = response.map_err(|error| format!("실패 — tmux 창 생성 실패: {error}"))?;
    let (pane, window) = pair.split_once('\t')
        .filter(|(pane, window)| !pane.is_empty() && !window.is_empty())
        .ok_or("실패 — tmux 창 생성 실패: pane/window 응답 없음")?;
    Ok(TmuxTarget { socket: "default".into(), pane: pane.into(), window: window.into(), mode: "detached" })
}

fn tmux_target_log(mode: &str, error: &str) {
    let prefix = format!("00119805591 | dispatch-agent.tmux-target | 01 | ack={} body.mode={mode} error=",
        if error == "none" { "received" } else { "rejected" });
    let safe = error.replace(['\r', '\n', '|'], " ");
    eprintln!("{prefix}{}", safe.chars().take(120usize.saturating_sub(prefix.len())).collect::<String>());
}

fn timed_tmux(stages: &mut Vec<(&'static str, u128)>, operation: &'static str, command: &mut Command) -> Result<String, String> {
    let started = Instant::now();
    let result = tmux_call(operation, command);
    stages.push((operation, started.elapsed().as_millis()));
    result
}

fn tmux_target_timed(tmux: Option<&str>, pane: Option<&str>, row: u64, stages: &mut Vec<(&'static str, u128)>) -> Result<TmuxTarget, String> {
    if tmux.is_some_and(|value| !value.is_empty()) {
        let pane = pane.filter(|value| !value.is_empty()).ok_or("dispatch-agent needs a tmux pane (TMUX_PANE is not set)")?;
        let result = timed_tmux(stages, "display-message", Command::new("tmux")
            .args(["display-message", "-p", "-t", pane, "#{window_id}"]));
        let selected = select_tmux_target(tmux, Some(pane), None, result.as_deref().map_err(String::as_str));
        match &selected { Ok(target) => tmux_target_log(target.mode, "none"), Err(error) => tmux_target_log("caller", error) }
        return selected;
    }
    let sessions = timed_tmux(stages, "list-sessions", Command::new("tmux").args(["list-sessions", "-F", "#{session_id}"]));
    let session = match &sessions {
        Ok(value) => value.lines().find(|line| !line.is_empty()),
        Err(_) => None,
    };
    let created = match session {
        Some(session) => {
            let target = format!("{session}:");
            // Not `dispatch-` until its pane is tagged, so no idle sweep can close a window still starting.
            let name = format!("starting-{row}");
            timed_tmux(stages, "new-window", Command::new("tmux")
                .args(["new-window", "-d", "-P", "-F", "#{pane_id}\t#{window_id}", "-t", &target, "-n", &name, "bash"]))
        }
        None => Err("tmux server has no sessions".into()),
    };
    let selected = select_tmux_target(None, None, Some(sessions.as_deref().map_err(String::as_str)), created.as_deref().map_err(String::as_str));
    match &selected { Ok(target) => tmux_target_log(target.mode, "none"), Err(error) => tmux_target_log("detached", error) }
    selected
}

fn write_tmux_failure(p: &Paths, last: &str) -> Result<i32, String> {
    for stale in [&p.exit, &p.last, &p.log] { let _ = fs::remove_file(stale); }
    fs::write(&p.last, last).map_err(|error| format!("{}: {error}", p.last.display()))?;
    fs::write(&p.exit, "2\n").map_err(|error| format!("{}: {error}", p.exit.display()))?;
    println!("{}", result_line("2", p, last));
    Ok(2)
}

fn preflight_then_target<T>(
    preflight: impl FnOnce() -> Result<String, String>,
    target: impl FnOnce() -> Result<T, String>,
) -> Result<(String, T), String> {
    let line = preflight()?;
    Ok((line, target()?))
}

const IMPROVER_ROOT: &str = "/home/tree/ai";

/// 개선자는 흐름이 끝난 행(`$H=확인`) 또는 작업 과정·로그 결함으로 멈춘 행(`$H=진단`·`검증`)의 `## 개선 요구`를 다룬다.
fn improver_preflight(row: u64, fields: &serde_json::Value) -> Result<String, String> {
    match fields.get("H").and_then(serde_json::Value::as_str) {
        Some("확인" | "검증" | "진단") => Ok(String::new()),
        state => Err(format!("실패 — improver: #{row} H={} ($H=진단·검증·확인에서만 시작)", state.unwrap_or(""))),
    }
}

fn agent_row_fields(project: &Path, row: u64) -> Result<serde_json::Value, String> {
    let output = grist_output(&["task", &row.to_string(), "--project", &project.to_string_lossy()])
        .map_err(|error| format!("grist task {row}: {error}"))?;
    if !output.status.success() { return Err(format!("grist task {row} failed")); }
    let records: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| format!("grist task {row} returned invalid JSON"))?;
    records.as_array().and_then(|items| items.iter().find(|item| item["id"].as_u64() == Some(row)))
        .and_then(|item| item.get("fields")).cloned()
        .ok_or_else(|| format!("grist task {row} did not return its row"))
}

fn state_number(path: &Path) -> Result<Option<u64>, String> {
    match fs::read_to_string(path) {
        Ok(value) => value.trim().parse().map(Some).map_err(|_| format!("invalid dispatch state: {}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("dispatch state: {error}")),
    }
}

const ELAPSED_ANALYSIS_MESSAGE: &str = "실패 — 20분 초과: resolve-deadlock대로 검증자 분석(--role verifier --analysis)을 위임하세요";
const CAUSE_MISSING_MESSAGE: &str = "실패 — 20분 초과: 분석 보고서의 원인을 $J·$C에 기록한 뒤 위임하세요";
const FIX_USED_MESSAGE: &str = "실패 — 교착 구현 1회 소진: resolve-deadlock대로 보고서와 함께 사용자에게 보고하세요";
const ANALYSIS_MAX: u64 = 2;

const ANALYSIS_LIMIT: Duration = Duration::from_secs(20 * 60);

/// When this row's cumulative agent run time first passed the 20-minute limit (end of that run).
/// Waits between dispatches are excluded because only finished run durations are summed.
fn run_limit_crossed(p: &Paths) -> Result<Option<SystemTime>, String> {
    let text = match fs::read_to_string(&p.runs) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("dispatch runs: {error}")),
    };
    let mut total = Duration::ZERO;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.split_whitespace().map(str::parse::<u64>);
        let (Some(Ok(end)), Some(Ok(run))) = (parts.next(), parts.next()) else {
            return Err(format!("invalid dispatch state: {}", p.runs.display()));
        };
        total += Duration::from_millis(run);
        if total > ANALYSIS_LIMIT { return Ok(Some(UNIX_EPOCH + Duration::from_millis(end))); }
    }
    Ok(None)
}

/// Finished agent·verifier run time on the row; improver runs fix the process and are not stall time.
fn row_run_total(project: &Path, row: u64) -> Result<Duration, String> {
    let mut total = Duration::ZERO;
    for role in ["agent", "verifier"] {
        let p = paths(project, row, role);
        let text = match fs::read_to_string(&p.runs) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("dispatch runs: {error}")),
        };
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let run = line.split_whitespace().nth(1).and_then(|run| run.parse::<u64>().ok())
                .ok_or_else(|| format!("invalid dispatch state: {}", p.runs.display()))?;
            total += Duration::from_millis(run);
        }
    }
    Ok(total)
}

/// The first dispatch after a row's cumulative run time passes 20 minutes marks one improvement
/// and is rejected until the improver handles it; later dispatches on the row are not marked again.
fn stall_improvement(project: &Path, row: u64, role: &str, analysis: bool) -> Result<Option<String>, String> {
    if role == "improver" || analysis { return Ok(None); }
    let total = row_run_total(project, row)?;
    if total <= ANALYSIS_LIMIT { return Ok(None); }
    let dir = project.join(".ai/dispatch");
    fs::create_dir_all(&dir).map_err(|error| format!("stall dir: {error}"))?;
    match OpenOptions::new().write(true).create_new(true).open(dir.join(format!("{row}.stall-improve"))) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
        Err(error) => return Err(format!("stall marker: {error}")),
    }
    let minutes = total.as_secs() / 60;
    mark_improvement_required(project, row, &format!("누적 실행 {minutes}m 정체(20분 초과)"))?;
    Ok(Some(format!("과정결함 — {row}: 누적 실행 {minutes}m 정체, grist improve-note 후 dispatch-agent --role improver --row {row}를 호출하세요")))
}

fn row_closed(fields: &serde_json::Value) -> bool {
    matches!(fields["H"].as_str().map(str::trim), Some("완료" | "확인"))
}

fn analysis_report_path(dir: &Path, row: u64, attempt: u64) -> PathBuf {
    dir.join(format!("{row}.analysis.{attempt}.md"))
}

/// Whether a finished agent run ended after `since`.
fn run_ended_after(p: &Paths, since: SystemTime) -> Result<bool, String> {
    let text = fs::read_to_string(&p.runs).map_err(|error| format!("dispatch runs: {error}"))?;
    let since = since.duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_millis() as u64;
    Ok(text.lines().filter_map(|line| line.split_whitespace().next()?.parse::<u64>().ok()).any(|end| end > since))
}

/// The rejection for an agent dispatch on a row past the limit, or None when it may run.
fn deadlock_rejection(project: &Path, row: u64, fields: &serde_json::Value) -> Result<Option<&'static str>, String> {
    if row_closed(fields) { return Ok(None); }
    let p = paths(project, row, "agent");
    let Some(deadline) = run_limit_crossed(&p)? else { return Ok(None); };
    let report = (1..=ANALYSIS_MAX).rev().find_map(|attempt| {
        let path = analysis_report_path(&p.dir, row, attempt);
        let written = fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
        let text = fs::read_to_string(&path).ok()?;
        (written >= deadline && analysis_report_problem(&text).is_none()).then_some(written)
    });
    let Some(written) = report else { return Ok(Some(ELAPSED_ANALYSIS_MESSAGE)); };
    if !fields["J"].as_str().is_some_and(|cause| !cause.trim().is_empty()) { return Ok(Some(CAUSE_MISSING_MESSAGE)); }
    Ok(run_ended_after(&p, written)?.then_some(FIX_USED_MESSAGE))
}

/// Next analysis attempt for the row; the third is rejected.
fn next_analysis_attempt(p: &Paths, row: u64) -> Result<u64, String> {
    let used = state_number(&p.dir.join(format!("{row}.analysis.count")))?.unwrap_or(0);
    if used >= ANALYSIS_MAX {
        return Err(format!("실패 — 분석 한도: {ANALYSIS_MAX}회를 모두 썼어요. resolve-deadlock대로 보고서로 구현을 위임하거나 사용자에게 보고하세요"));
    }
    Ok(used + 1)
}

fn analysis_prompt(dir: &Path, row: u64, attempt: u64) -> String {
    let previous = if attempt > 1 {
        format!(" 1차 보고서={} 1차 로그={}", analysis_report_path(dir, row, 1).display(), dir.join(format!("{row}.analysis.1.log")).display())
    } else { String::new() };
    format!("분석: 회차={attempt}/{ANALYSIS_MAX} 보고서={} 막힌 실행 로그={}{previous}\n",
        analysis_report_path(dir, row, attempt).display(), dir.join(format!("{row}.log")).display())
}

fn multiple_implementations(prompt: &[u8]) -> bool {
    let prompt = String::from_utf8_lossy(prompt);
    let mut numbered = 0;
    for line in prompt.lines() {
        let line = line.trim_start();
        let digits = line.bytes().take_while(u8::is_ascii_digit).count();
        let rest = &line[digits..];
        if digits > 0 && rest.starts_with(['.', ')'])
            && rest[1..].chars().next().is_some_and(char::is_whitespace) {
            numbered += 1;
        }
        if numbered >= 2 { return true; }
    }
    false
}

/// Check concrete entry points before starting a worker. Investigation prompts can name
/// unknown entry points, and resume prompts describe work already in progress.
fn entrypoint_preflight(project: &Path, prompt: &[u8], resume: bool) -> Result<(), String> {
    if resume { return Ok(()); }
    let prompt = String::from_utf8_lossy(prompt);
    if prompt.lines().any(|line| {
        let line = line.trim();
        line.starts_with("조사 위임") || line.starts_with("조사 모드")
    }) { return Ok(()); }
    for line in prompt.lines().map(str::trim) {
        let Some(entry) = line.strip_prefix("진입점:") else { continue };
        let entry = entry.trim();
        if entry.starts_with("rg ") || entry.starts_with('<') { continue; }
        let Some(raw_file) = entry.split_whitespace().next() else { continue };
        let file = raw_file.trim_matches(['`', '"', '\'', ',', ';']);
        if !file.contains('/') && !file.contains('.') { continue; }
        let path = match (file.strip_prefix("~/"), std::env::var_os("HOME")) {
            (Some(rest), Some(home)) => Path::new(&home).join(rest),
            _ => PathBuf::from(file),
        };
        let path = if path.is_absolute() { path } else { project.join(path) };
        if !path.is_file() {
            return Err(format!("실패 — 진입점 파일 없음: {file} (위임 직전 실제 경로를 확인하세요)"));
        }
        let rest = entry[raw_file.len()..].trim_start();
        let function = if let Some(parenthesized) = rest.strip_prefix('(') {
            parenthesized.split(')').next().unwrap_or("")
                .split_whitespace().last().unwrap_or("")
        } else {
            rest.split_whitespace().next().unwrap_or("").trim_matches(['`', ',', ';'])
        };
        if function.is_empty() || (!rest.starts_with('(') && !function.ends_with("()")) { continue; }
        let rust_function = path.extension().is_some_and(|ext| ext == "rs") && function.ends_with("()");
        let pattern = if rust_function {
            format!(r"\bfn\s+{}\s*\(", function.trim_end_matches("()"))
        } else { function.to_string() };
        let mut rg = Command::new("rg");
        rg.arg("-q");
        if !rust_function { rg.arg("-F"); }
        let found = rg.arg(&pattern).arg(&path)
            .status().map_err(|error| format!("진입점 rg 실행: {error}"))?;
        if !found.success() {
            return Err(format!("실패 — 진입점 함수 없음: {file} {function} (위임 직전 실제 함수를 확인하세요)"));
        }
    }
    Ok(())
}

/// 검증자 흐름 프롬프트: mock 사례는 `mock 사례:` 줄에만 두고, 그 뒤 실제 데이터로 시작 동작→최종 소비자까지
/// 진행하는 `실제 흐름:` 줄이 반드시 있어야 한다(261110 #813: 위임 프롬프트가 가짜 requestUrl만 지시해 실제 n8n
/// 흐름 없이 `확인`으로 올라감).
fn verifier_prompt_check(prompt: &[u8]) -> Result<(), String> {
    let text = String::from_utf8_lossy(prompt);
    let mut real = false;
    for line in text.lines().map(str::trim_start).map(|l| l.trim_start_matches(['-', '*', ' '])) {
        if line.starts_with("실제 흐름:") { real = true; continue; }
        if line.starts_with("mock 사례:") { continue; }
        let lower = line.to_lowercase();
        if ["가짜", "mock", "stub", "스텁", "fake"].iter().any(|word| lower.contains(word)) {
            return Err(format!("실패 — 검증자 Mock 지시: mock·가짜는 `mock 사례:` 줄에만 쓰고, 실제 데이터 흐름은 `실제 흐름:` 줄로 지시하세요 ({})",
                line.chars().take(60).collect::<String>()));
        }
    }
    if !real {
        return Err("실패 — 실제 흐름 없음: 검증자 프롬프트에 실제 데이터로 시작 동작→최종 소비자까지 진행하는 `실제 흐름:` 줄이 필요해요".into());
    }
    Ok(())
}

/// A verifier runs the recorded goals. Broad checks of old flows and waiting for a
/// video to finish are separate work unless the goals explicitly ask for them.
fn verifier_scope_check(prompt: &[u8], goals: &str) -> Result<(), String> {
    let prompt = String::from_utf8_lossy(prompt);
    for line in prompt.lines() {
        let old_flow = line.contains("기존")
            && ["저장", "학습", "캡처"].iter().any(|word| line.contains(word))
            && ["확인", "검증", "전수", "점검"].iter().any(|word| line.contains(word));
        if old_flow && !goals.contains("기존") && !goals.contains("회귀") {
            return Err("실패 — 검증 범위 초과: 목표에 없는 기존 저장·학습·캡처 흐름 확인은 별도 행으로 분리하세요".into());
        }
        let playback_wait = ["영상 완료", "영상 끝", "재생 완료", "재생 종료", "끝날 때까지", "재생이 끝날 때까지"]
            .iter().any(|word| line.contains(word))
            && ["대기", "기다", "확인", "검증"].iter().any(|word| line.contains(word));
        if playback_wait && !goals.contains("재생 완료") && !goals.contains("영상 완료") {
            return Err("실패 — 검증 대기 초과: 분량·재생시간은 메타데이터 값으로 확인하고 영상 종료를 기다리지 마세요".into());
        }
    }
    Ok(())
}

fn verifier_goals(project: &Path, row: u64, rows: Option<&[u64]>) -> Result<String, String> {
    let mut goals = String::new();
    for &id in rows.unwrap_or(std::slice::from_ref(&row)) {
        let output = grist_output(&["goal", &id.to_string(), "--project", &project.to_string_lossy()])
            .map_err(|error| format!("grist goal {id}: {error}"))?;
        if !output.status.success() { return Err(format!("grist goal {id} failed")); }
        goals.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    Ok(goals)
}

fn agent_preflight(project: &Path, row: u64, p: &Paths, prompt: &[u8], resume: bool, role: &str,
    fields: &serde_json::Value) -> Result<(), String> {
    if role != "agent" { return Ok(()); }
    if !resume && multiple_implementations(prompt) {
        return Err("실패 — 구현 2개 이상: 행을 나눠 행마다 위임하세요".into());
    }
    let runtime = runtime_kind(project)?;
    // 선행 관측은 오류 행만 요구한다(stage-start: 새 기능·개선 행($G=기능)에는 요구하지 않음).
    if fields["H"].as_str() == Some("진단") && fields["G"].as_str() != Some("기능")
        && matches!(runtime, RuntimeKind::Web | RuntimeKind::App) {
        let created = fields["E"].as_u64().map(|seconds|
            UNIX_EPOCH + Duration::from_secs(seconds) - Duration::from_secs(9 * 3600));
        let boundary = created
            .ok_or_else(|| format!("grist task {row} missing creation time"))?;
        let fresh = fs::metadata(p.dir.join(format!("{row}.verify.log")))
            .and_then(|meta| meta.modified()).is_ok_and(|modified| modified >= boundary);
        if !fresh {
            return Err(format!("실패 — 관측 없음: 행 생성 뒤 {row}.verify.log가 필요해요"));
        }
    }
    if let Some(rejection) = deadlock_rejection(project, row, fields)? {
        return Err(rejection.into());
    }
    if resume && state_number(&p.resumes)?.unwrap_or(0) >= 2 && !prompt.starts_with("[제출점검 자동 재개]".as_bytes()) {
        return Err("실패 — 재위임 한도: 실패 공통 원인을 $J·$C에 쓰고 사용자에게 보고하세요".into());
    }
    Ok(())
}

fn record_agent_start(p: &Paths, resume: bool, now: SystemTime) -> Result<(), String> {
    let millis = now.duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_millis();
    match OpenOptions::new().write(true).create_new(true).open(&p.first) {
        Ok(mut file) => writeln!(file, "{millis}").map_err(|error| format!("first dispatch: {error}"))?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {},
        Err(error) => return Err(format!("first dispatch: {error}")),
    }
    if resume {
        let count = state_number(&p.resumes)?.unwrap_or(0);
        fs::write(&p.resumes, format!("{}\n", count + 1)).map_err(|error| format!("resume count: {error}"))?;
    }
    Ok(())
}

fn write_preflight_rejection(p: &Paths, last: &str) -> Result<i32, String> {
    for stale in [&p.exit, &p.last, &p.log] { let _ = fs::remove_file(stale); }
    fs::write(&p.last, last).map_err(|error| format!("{}: {error}", p.last.display()))?;
    fs::write(&p.exit, "5\n").map_err(|error| format!("{}: {error}", p.exit.display()))?;
    println!("exit=5 last={last}{}", case_suffix("5", last));
    Ok(5)
}

fn lock_path(socket: &str, window: &str) -> PathBuf {
    let hash = socket.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    Path::new("/tmp").join(format!("dispatch-agent-{hash:016x}-{}.lock", window.trim_start_matches('@')))
}

fn lock_error(operation: &str, error: &std::io::Error) -> String {
    let message = error.to_string();
    let prefix = format!("00119803541 | dispatch-agent.lock.{operation} | 01 | ack=rejected body=none error=");
    let available = 120usize.saturating_sub(prefix.len());
    let safe = message.replace(['\r', '\n', '|'], " ");
    eprintln!("{prefix}{}", safe.chars().take(available).collect::<String>());
    format!("window lock {operation} failed: {message}")
}

fn cleanup_old_dispatch(dir: &Path, active_rows: &HashSet<String>, now: SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let row = name.split(['.', '-']).next().unwrap_or("");
        if active_rows.contains(row) { continue; }
        let Ok(meta) = entry.metadata() else { continue; };
        if !name.ends_with(".improve-required") && meta.is_file() && meta.modified().ok().and_then(|time| now.duration_since(time).ok()).is_some_and(|age| age > Duration::from_secs(14 * 24 * 3600)) {
            let _ = fs::remove_file(path);
        }
    }
}

fn active_dispatch_rows() -> HashSet<String> {
    Command::new("tmux").args(["list-panes", "-a", "-F", "#{@dispatch-agent}\t#{pane_dead}"]).output().ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).lines().filter_map(|line| {
            let (row, dead) = line.split_once('\t')?;
            (dead != "1" && !row.is_empty()).then(|| row.to_string())
        }).collect()).unwrap_or_default()
}

fn with_window_lock<T>(socket: &str, window: &str, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let path = lock_path(socket, window);
    loop {
        let file: File = OpenOptions::new().read(true).write(true).create(true).open(&path)
            .map_err(|error| lock_error("acquire", &error))?;
        file.lock().map_err(|error| lock_error("acquire", &error))?;
        let opened = file.metadata().map_err(|error| lock_error("acquire", &error))?;
        let current = fs::metadata(&path).ok();
        if !current.is_some_and(|meta| meta.dev() == opened.dev() && meta.ino() == opened.ino()) {
            file.unlock().map_err(|error| lock_error("release", &error))?;
            continue;
        }
        let result = action();
        fs::remove_file(&path).map_err(|error| lock_error("release", &error))?;
        file.unlock().map_err(|error| lock_error("release", &error))?;
        return result;
    }
}

/// The agent command line; its prompt comes from the saved prompt file.
fn agent_command(agent: &str, project: &Path, add_dirs: &[PathBuf], resume_session: Option<&str>, p: &Paths) -> String {
    if let Some(session) = resume_session {
        let roots = if add_dirs.is_empty() {
            String::new()
        } else {
            let roots = add_dirs.iter().map(|d| json_quote(&d.to_string_lossy())).collect::<Vec<_>>().join(",");
            format!(" -c {}", shell_quote(&format!("sandbox_workspace_write.writable_roots=[{roots}]")))
        };
        return format!(
            "codex exec resume {} -c {} -c sandbox_workspace_write.network_access=true{} -o {} - < {}",
            shell_quote(session), shell_quote("sandbox_mode=\"workspace-write\""), roots,
            quote(&p.last), quote(&p.prompt)
        );
    }
    match agent {
        "codex" => format!(
            "codex exec --skip-git-repo-check -C {} -s workspace-write -c sandbox_workspace_write.network_access=true{} -o {} - < {}",
            quote(project),
            add_dirs.iter().map(|d| format!(" --add-dir {}", quote(d))).collect::<String>(),
            quote(&p.last), quote(&p.prompt)
        ),
        _ => format!("gemini --approval-mode auto_edit -p \"$(cat {})\"", quote(&p.prompt)),
    }
}

/// Bash runner for the pane: the caller's PATH (a tmux pane starts from the server's environment), worker env,
/// full output to the log, exit code written last and atomically.
fn runner_script(agent: &str, role: &str, project: &Path, add_dirs: &[PathBuf], resume_session: Option<&str>, row: u64, rows: Option<&[u64]>, path_env: &str, p: &Paths, analysis_report: Option<&Path>) -> String {
    let tee = if resume_session.is_some() { "tee -a" } else { "tee" };
    let runtime_role = match role { "verifier" => "verifier", "improver" => "improver", _ => "worker" };
    let rows_env = rows.map(|ids| format!(" T26_ROWS={}", ids.iter().map(u64::to_string).collect::<Vec<_>>().join(","))).unwrap_or_default();
    let analysis_env = analysis_report.map(|path| format!(" T26_ANALYSIS_REPORT={}", quote(path))).unwrap_or_default();
    format!(
        "#!/bin/bash\ncd {project} || exit 1\nrm -f {exit}\nexport PATH={path} T26_ROW={row} T26_ROLE={runtime_role}{rows_env}{analysis_env}\nrun_start=$(date +%s%3N)\nrecord_run() {{ [ -n \"$run_recorded\" ] && return; run_recorded=1; run_end=$(date +%s%3N); echo \"$run_end $((run_end - run_start))\" >> {runs}; }}\ntrap 'record_run; echo 129 > {exit}.tmp && mv {exit}.tmp {exit}; exit 129' HUP TERM\n{cmd} 2>&1 | {tee} {log}\nstatus=\"${{PIPESTATUS[0]}}\"\nrecord_run\necho \"$status\" > {exit}.tmp && mv {exit}.tmp {exit}\n",
        project = quote(project),
        path = quote(Path::new(path_env)),
        cmd = agent_command(agent, project, add_dirs, resume_session, p),
        tee = tee,
        log = quote(&p.log),
        exit = quote(&p.exit),
        runs = quote(&p.runs),
    )
}

/// Last non-empty line of the agent's final message (codex `-o`) or of the log (gemini).
fn last_line(text: &str) -> String {
    let line = text.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    line.chars().take(300).collect()
}

fn startup_failure_last(log: &str) -> String {
    let line = last_line(log);
    if line.is_empty() { "실패 — 에이전트 출력 없음".into() } else { format!("실패 — 시작: {line}") }
}

const CASES_DIR: &str = "~/ai/skills/manage-project/references/cases";

/// Case file the delegator reads for this outcome (`case=` suffix); none for an ordinary exit.
fn case_path(code: &str, last: &str) -> Option<String> {
    if last.contains("20분 초과") { return Some("~/ai/skills/resolve-deadlock/SKILL.md".into()); }
    // tmux 없음 leaves through the preflight rejection (exit 5), but it is a startup failure.
    let name = if last.contains("tmux 없음") { "crash" } else { match code.trim() {
        "2" | "7" => "crash",
        "4" => "stall",
        "5" | "8" => "check-fail",
        "6" => "slow",
        "0" if last.trim_end().ends_with('?') => "question",
        _ => return None,
    } };
    Some(format!("{CASES_DIR}/{name}.md"))
}

fn case_suffix(code: &str, last: &str) -> String {
    case_path(code, last).map(|path| format!(" case={path}")).unwrap_or_default()
}

fn result_line(code: &str, p: &Paths, last: &str) -> String {
    format!("exit={} log={} last={}{}", code.trim(), p.log.display(), last, case_suffix(code, last))
}

fn submit_result_line(code: &str, p: &Paths, last: &str, suffix: &str, submit_pass: bool) -> String {
    let pass = if submit_pass { " submit-check=pass" } else { "" };
    format!("{}{suffix}{pass}", result_line(code, p, last))
}

fn checkpoint_log(ack: &str, elapsed: u64, grown: u64, new: usize, repeat: usize, error: &str) {
    eprintln!("00119805691 | dispatch-agent.checkpoint | 01 | ack={ack} body.elapsed={elapsed} body.grown={grown} body.new={new} body.repeat={repeat} error={error}");
}

fn checkpoint_limit(expect_seconds: Option<u64>) -> Duration {
    expect_seconds.or_else(|| std::env::var("DISPATCH_CHECKPOINT_SECONDS").ok().and_then(|value| value.parse::<u64>().ok()))
        .filter(|seconds| *seconds > 0).map(Duration::from_secs).unwrap_or(Duration::from_secs(300))
}

fn role_checkpoint_limit(role: &str, expect_seconds: Option<u64>) -> Duration {
    let limit = checkpoint_limit(expect_seconds);
    if role == "verifier" { limit.min(Duration::from_secs(300)) } else { limit }
}

fn file_size(path: &Path) -> u64 { fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) }

fn write_start_time(p: &Paths, now: SystemTime) -> Result<(), String> {
    let millis = now.duration_since(UNIX_EPOCH).map_err(|error| format!("start clock: {error}"))?.as_millis();
    fs::write(&p.started, millis.to_string()).map_err(|error| format!("start state: {error}"))
}

fn total_elapsed(p: &Paths, now: SystemTime) -> Result<Duration, String> {
    let text = fs::read_to_string(&p.started).map_err(|error| format!("start state: {error}"))?;
    let millis = text.trim().parse::<u64>().map_err(|_| "start state: invalid timestamp".to_string())?;
    Ok(now.duration_since(UNIX_EPOCH + Duration::from_millis(millis)).unwrap_or_default())
}

fn completed_exit(p: &Paths) -> Option<String> { fs::read_to_string(&p.exit).ok() }

fn cached_stage(row: u64) -> String {
    grist_output(&["task", &row.to_string(), "--cached"]).ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|line| line.split("H=").nth(1).and_then(|value| value.split('|').next()).map(str::trim).map(str::to_owned))
        .filter(|stage| !stage.is_empty()).unwrap_or_else(|| "?".into())
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct SeenLines { offset: u64, lines: Vec<String> }

#[derive(Default)]
struct TailSummary { new: usize, repeat: usize, repeated: Option<String>, last: Option<String> }

fn verifier_checkpoint_defect(role: &str, analysis: bool, tail: &TailSummary) -> bool {
    role == "verifier" && !analysis && (tail.new == 0 ||
        (tail.repeat > 0 && tail.repeated.as_deref().is_some_and(|line| line.chars().any(char::is_alphanumeric))))
}

fn normalize_log_line(line: &str) -> String {
    let chars: Vec<char> = line.trim().chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let rest = &chars[i..];
        let time = rest.len() >= 8 && rest[0..2].iter().all(char::is_ascii_digit)
            && rest[2] == ':' && rest[3..5].iter().all(char::is_ascii_digit)
            && rest[5] == ':' && rest[6..8].iter().all(char::is_ascii_digit);
        let iso = rest.len() >= 19 && rest[0..4].iter().all(char::is_ascii_digit)
            && rest[4] == '-' && rest[5..7].iter().all(char::is_ascii_digit)
            && rest[7] == '-' && rest[8..10].iter().all(char::is_ascii_digit)
            && (rest[10] == 'T' || rest[10] == ' ')
            && rest[11..13].iter().all(char::is_ascii_digit) && rest[13] == ':'
            && rest[14..16].iter().all(char::is_ascii_digit) && rest[16] == ':'
            && rest[17..19].iter().all(char::is_ascii_digit);
        if iso {
            let mut end = 19;
            while end < rest.len() && (rest[end].is_ascii_digit() || matches!(rest[end], '.' | 'Z' | '+' | '-' | ':')) { end += 1; }
            out.push('#'); i += end;
        } else if time {
            out.push('#'); i += 8;
        } else if chars[i].is_ascii_hexdigit() {
            let mut end = i;
            while end < chars.len() && chars[end].is_ascii_hexdigit() { end += 1; }
            let span = &chars[i..end];
            if span.len() >= 8 && span.iter().any(|c| c.is_ascii_alphabetic()) {
                out.push('#'); i = end;
            } else if chars[i].is_ascii_digit() {
                let mut digits = i;
                while digits < chars.len() && chars[digits].is_ascii_digit() { digits += 1; }
                let status = digits - i == 3 && i > 0 && chars[i - 1].is_whitespace()
                    && chars[i..digits].iter().collect::<String>().parse::<u16>().is_ok_and(|value| (100..=599).contains(&value));
                if status { out.extend(&chars[i..digits]); } else { out.push('#'); }
                i = digits;
            } else { out.push(chars[i]); i += 1; }
        } else if chars[i].is_ascii_digit() {
            let mut end = i;
            while end < chars.len() && chars[end].is_ascii_digit() { end += 1; }
            out.push('#'); i = end;
        } else { out.push(chars[i]); i += 1; }
    }
    out
}

fn summarize_tail(p: &Paths) -> Result<TailSummary, String> {
    let mut seen: SeenLines = fs::read(&p.seen).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    let mut file = File::open(&p.log).map_err(|error| format!("log tail: {error}"))?;
    let size = file.metadata().map_err(|error| format!("log tail metadata: {error}"))?.len();
    if size < seen.offset { seen = SeenLines::default(); }
    let start = seen.offset.max(size.saturating_sub(64 * 1024));
    file.seek(SeekFrom::Start(start)).map_err(|error| format!("log tail seek: {error}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|error| format!("log tail read: {error}"))?;
    if start > seen.offset {
        if let Some(at) = bytes.iter().position(|byte| *byte == b'\n') { bytes.drain(..=at); } else { bytes.clear(); }
    }
    let previous: HashSet<String> = seen.lines.iter().cloned().collect();
    let mut current = HashSet::new();
    let mut counts = HashMap::<String, usize>::new();
    let mut summary = TailSummary::default();
    for raw in String::from_utf8_lossy(&bytes).lines() {
        if raw.trim_start().starts_with("hook: ") { continue; }
        let line = normalize_log_line(raw);
        if line.is_empty() { continue; }
        *counts.entry(line.clone()).or_default() += 1;
        if current.insert(line.clone()) {
            if !previous.contains(&line) { summary.new += 1; }
            summary.last = Some(line.clone());
            seen.lines.retain(|prior| prior != &line);
            seen.lines.push(line);
        }
    }
    if let Some((line, count)) = counts.into_iter().filter(|(_, count)| *count >= 3).max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))) {
        summary.repeated = Some(line); summary.repeat = count;
    }
    if seen.lines.len() > 2000 { seen.lines.drain(..seen.lines.len() - 2000); }
    seen.offset = size;
    let temp = p.seen.with_extension("seen.tmp");
    fs::write(&temp, serde_json::to_vec(&seen).map_err(|error| format!("seen encode: {error}"))?)
        .map_err(|error| format!("seen write: {error}"))?;
    fs::rename(&temp, &p.seen).map_err(|error| format!("seen rename: {error}"))?;
    Ok(summary)
}

fn checkpoint_line(row: u64, elapsed: Duration, grown: u64, tail: &TailSummary, stage: &str, watch_line: Option<&str>, expect_seconds: Option<u64>) -> String {
    let watch = watch_line.unwrap_or("없음").replace(['\r', '\n'], " ");
    let excess = expect_seconds.filter(|seconds| elapsed >= Duration::from_secs(*seconds))
        .map(|seconds| format!(", 기대 {seconds}s 초과")).unwrap_or_default();
    let repeated = tail.repeated.as_deref().map(|line| format!("반복 {} ×{}", line.chars().take(60).collect::<String>(), tail.repeat)).unwrap_or_else(|| "반복 없음".into());
    let last = tail.last.as_deref().unwrap_or("없음").chars().take(80).collect::<String>();
    let stalled = if tail.new == 0 || tail.repeat > 0 { ", 내용 정체" } else { "" };
    format!("진행 — {row}: 경과 {}m, 로그 +{grown}B, 새 줄 {}, {repeated}, 마지막 {last}, H={stage}, 감시 {}{excess}{stalled}",
        elapsed.as_secs() / 60, tail.new, watch.chars().take(80).collect::<String>())
}

fn record_decision(project: &Path, row: u64, decision: &str) -> Result<(), String> {
    let result = record_decision_with(project, row, decision, grist_output);
    checkpoint_log(if result.is_ok() { "received" } else { "rejected" }, 0, 0, 0, 0,
        if result.is_ok() { "none" } else { "decision_update_failed" });
    result
}

fn record_decision_with(
    project: &Path, row: u64, decision: &str,
    call: impl FnOnce(&[&str]) -> io::Result<Output>,
) -> Result<(), String> {
    let safe = decision.replace(['\r', '\n'], " ");
    let fields = serde_json::json!({"D": format!("[🕧] 대기 판단: {safe}")}).to_string();
    let output = call(&["task-update", &row.to_string(), "--project", &project.to_string_lossy(), "--fields", &fields])
        .map_err(|error| format!("decision task-update: {error}"))?;
    if !output.status.success() { return Err("decision task-update failed".into()); }
    Ok(())
}

fn is_process_log_defect_only(detail: &str) -> bool {
    (detail.contains("확인됐으나") || detail.contains("통과했으나")) && detail.contains("로그")
}

fn verifier_process_defect_row(last: &str, rows: &[u64]) -> Option<(u64, String)> {
    if let Some(rest) = last.strip_prefix("과정결함 — ") {
        let (id, detail) = rest.split_once(':')?;
        if detail.trim().is_empty() { return None; }
        let id = id.parse::<u64>().ok()?;
        return rows.contains(&id).then(|| (id, last.chars().take(120).collect()));
    }
    let rest = last.strip_prefix("실패 — ")?;
    let (id, detail) = rest.split_once(':')?;
    if !is_process_log_defect_only(detail) { return None; }
    let id = id.parse::<u64>().ok()?;
    rows.contains(&id).then(|| (id, last.chars().take(120).collect()))
}

fn verifier_failure_row(last: &str, rows: &[u64]) -> Option<(u64, String)> {
    if verifier_process_defect_row(last, rows).is_some() { return None; }
    let rest = last.strip_prefix("실패 — ")?;
    let (id, detail) = rest.split_once(':')?;
    if detail.trim().is_empty() { return None; }
    let id = id.parse::<u64>().ok()?;
    rows.contains(&id).then(|| (id, last.chars().take(120).collect()))
}

fn analysis_process_timeout(role: &str, analysis: bool, last: &str) -> bool {
    role == "verifier" && analysis && last.contains("20분 초과")
}

fn verifier_fail_log(row: u64, ack: &str, error: &str) {
    let safe = error.replace(['\r', '\n', '|'], " ");
    eprintln!("00119805311 | dispatch-agent.verifier-fail | 01 | ack={ack} body.row={row} error={}",
        safe.chars().take(100).collect::<String>());
}

fn grist_failure_error(output: &Output) -> String {
    let response = if output.stderr.is_empty() { &output.stdout } else { &output.stderr };
    serde_json::from_slice::<serde_json::Value>(response).ok()
        .and_then(|value| value.get("error").and_then(serde_json::Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| String::from_utf8_lossy(response).trim().to_owned())
}

fn record_verifier_failure_with(project: &Path, row: u64, line: &str, call: impl FnOnce(&[&str]) -> io::Result<Output>) -> bool {
    let fields = serde_json::json!({"H":"진단", "I":line, "D":format!("[🛣] 검증 실패 — {row} 원인 점검")}).to_string();
    let output = call(&["task-update", &row.to_string(), "--project", &project.to_string_lossy(), "--fields", &fields]);
    match output {
        Ok(output) if output.status.success() => { verifier_fail_log(row, "received", "none"); true }
        Ok(output) => {
            verifier_fail_log(row, "rejected", &grist_failure_error(&output));
            false
        }
        Err(error) => { verifier_fail_log(row, "not_observed", &error.to_string()); false }
    }
}

fn record_verifier_failure(project: &Path, row: u64, line: &str) -> bool {
    record_verifier_failure_with(project, row, line, grist_output)
}

fn row_owner_result(row: u64, result: io::Result<Output>) -> Result<(), String> {
    let output = result.map_err(|error| format!("grist task-show failed: {error}"))?;
    let response = if output.status.success() { &output.stdout } else { &output.stderr };
    let value: serde_json::Value = serde_json::from_slice(response)
        .map_err(|_| "grist task-show returned invalid JSON".to_string())?;
    if output.status.success()
        && value.get("ok").and_then(serde_json::Value::as_bool) != Some(false)
        && value.get("id").and_then(serde_json::Value::as_u64) == Some(row)
        && value.get("section").and_then(serde_json::Value::as_str) == Some("row")
    {
        return Ok(());
    }
    let error = value.get("error").and_then(serde_json::Value::as_str)
        .unwrap_or("grist task-show did not confirm row ownership");
    Err(error.replace(['\r', '\n'], " "))
}

fn grist_output(args: &[&str]) -> io::Result<Output> {
    if let Some(bin) = std::env::var_os("GRIST_BIN") {
        return Command::new(bin).args(args).output();
    }
    let output = Command::new("grist").args(args).output();
    if !matches!(&output, Err(error) if error.kind() == io::ErrorKind::NotFound) { return output; }
    let home = std::env::var_os("HOME").ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME missing"))?;
    Command::new(PathBuf::from(home).join("ai/scripts/bin/grist")).args(args).output()
}

fn improvement_marker(project: &Path, row: u64) -> PathBuf {
    project.join(".ai/dispatch").join(format!("{row}.improve-required"))
}

fn mark_improvement_required(project: &Path, row: u64, detail: &str) -> Result<bool, String> {
    let dir = project.join(".ai/dispatch");
    fs::create_dir_all(&dir).map_err(|error| format!("improvement dir: {error}"))?;
    let marker = improvement_marker(project, row);
    let mut file = match OpenOptions::new().write(true).create_new(true).open(&marker) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(format!("improvement marker: {error}")),
    };
    let safe = detail.replace(['\r', '\n'], " ").chars().take(160).collect::<String>();
    if let Err(error) = file.write_all(safe.as_bytes()) {
        let _ = fs::remove_file(&marker);
        return Err(format!("improvement marker: {error}"));
    }
    Ok(true)
}

fn pending_improvement(project: &Path) -> Result<Option<u64>, String> {
    let dir = project.join(".ai/dispatch");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("improvement signals: {error}")),
    };
    let mut rows = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("improvement signals: {error}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(row) = name.strip_suffix(".improve-required").and_then(|id| id.parse::<u64>().ok()) {
            rows.push(row);
        }
    }
    Ok(rows.into_iter().min())
}

fn require_improvement_handled(project: &Path, role: &str, row: u64) -> Result<(), String> {
    if let Some(pending) = pending_improvement(project)? {
        if role != "improver" || row != pending {
            return Err(format!("과정결함 미처리 — {pending}: grist improve-note 후 dispatch-agent --role improver --row {pending}를 호출하세요"));
        }
    }
    Ok(())
}

fn clear_improvement_required(project: &Path, row: u64) -> Result<(), String> {
    match fs::remove_file(improvement_marker(project, row)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("improvement signal clear: {error}")),
    }
}

fn check_row_owner(project: &Path, row: u64) -> Result<(), String> {
    let output = grist_output(&["task-show", &row.to_string(), "--project", &project.to_string_lossy(), "--section", "row"]);
    row_owner_result(row, output)
}

#[derive(Default)]
struct LogCursor {
    offset: u64,
    pending: Vec<u8>,
}

struct StallLimits {
    repeated_rejections: usize,
    silence: Duration,
}

impl Default for StallLimits {
    fn default() -> Self {
        Self { repeated_rejections: 2, silence: Duration::from_secs(300) }
    }
}

struct LogWatch {
    directory: PathBuf,
    files: HashMap<PathBuf, LogCursor>,
    dispatch_log: Option<(PathBuf, u64)>,
    last_line: Option<String>,
    last_rejected: Option<String>,
    rejected_count: usize,
    last_progress: Duration,
    limits: StallLimits,
}

fn project_logs(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "log") && path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn dispatch_log_watch_declared(project: &Path) -> bool {
    fs::read_to_string(project.join(".ai/docs/role-verifier.md"))
        .ok()
        .is_some_and(|doc| doc.lines().any(|line| line.trim() == "watch-source: dispatch-log"))
}

fn rejected_key(line: &str) -> Option<&str> {
    if !line.contains("ack=rejected") { return None; }
    let (time, rest) = line.split_once(" | ")?;
    let bytes = time.as_bytes();
    if bytes.len() == 8 && bytes[2] == b':' && bytes[5] == b':'
        && bytes.iter().enumerate().all(|(at, byte)| at == 2 || at == 5 || byte.is_ascii_digit()) {
        Some(rest)
    } else {
        None
    }
}

impl LogWatch {
    fn new(project: &Path, explicit: &[PathBuf], role: &str, limits: StallLimits) -> Result<Self, String> {
        if role == "verifier" && explicit.iter().any(|path| path.starts_with(project.join("test/tmp"))) {
            return Err("verifier --watch-log must name a runtime log, not test/tmp".into());
        }
        let directory = project.join(".ai/logs");
        let mut files = HashMap::new();
        for path in project_logs(&directory).map_err(|error| format!("watch logs: {error}"))?
            .into_iter().chain(explicit.iter().cloned()) {
            let offset = match fs::metadata(&path) {
                Ok(meta) => meta.len(),
                Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
                Err(error) => return Err(format!("watch log {}: {error}", path.display())),
            };
            files.insert(path, LogCursor { offset, pending: Vec::new() });
        }
        if role == "verifier" && files.is_empty() && !dispatch_log_watch_declared(project) {
            return Err("verifier needs --watch-log <record-log path> or .ai/docs/role-verifier.md watch-source: dispatch-log".into());
        }
        Ok(Self { directory, files, dispatch_log: None, last_line: None, last_rejected: None,
            rejected_count: 0, last_progress: Duration::ZERO, limits })
    }

    fn track_dispatch_log(&mut self, path: &Path) {
        self.dispatch_log = Some((path.to_path_buf(), file_size(path)));
    }

    fn observe(&mut self, line: String, elapsed: Duration) -> Option<String> {
        self.last_progress = elapsed;
        self.last_line = Some(line.clone());
        if let Some(key) = rejected_key(&line) {
            if self.last_rejected.as_deref() == Some(key) { self.rejected_count += 1; }
            else { self.last_rejected = Some(key.to_owned()); self.rejected_count = 1; }
            if self.rejected_count >= self.limits.repeated_rejections {
                return Some(format!("실패 — stall: {line}"));
            }
        } else {
            self.last_rejected = None;
            self.rejected_count = 0;
        }
        None
    }

    fn poll(&mut self, elapsed: Duration) -> Result<Option<String>, String> {
        if let Some((path, previous)) = &mut self.dispatch_log {
            let current = file_size(path);
            if current > *previous { self.last_progress = elapsed; }
            *previous = current;
        }
        for path in project_logs(&self.directory).map_err(|error| format!("watch logs: {error}"))? {
            self.files.entry(path).or_default();
        }
        let mut paths: Vec<_> = self.files.keys().cloned().collect();
        paths.sort();
        for path in paths {
            let mut file = match File::open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("watch log {}: {error}", path.display())),
            };
            let cursor = self.files.get_mut(&path).expect("registered watch log");
            let len = file.metadata().map_err(|error| format!("watch log {}: {error}", path.display()))?.len();
            if len < cursor.offset { cursor.offset = 0; cursor.pending.clear(); }
            if len == cursor.offset { continue; }
            file.seek(SeekFrom::Start(cursor.offset)).map_err(|error| format!("watch log {}: {error}", path.display()))?;
            let mut appended = Vec::new();
            file.read_to_end(&mut appended).map_err(|error| format!("watch log {}: {error}", path.display()))?;
            cursor.offset += appended.len() as u64;
            cursor.pending.extend(appended);
            let complete = cursor.pending.iter().rposition(|byte| *byte == b'\n');
            let lines = complete.map(|end| cursor.pending.drain(..=end).collect::<Vec<_>>()).unwrap_or_default();
            for raw in lines.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
                let line = String::from_utf8_lossy(raw).trim_end_matches('\r').to_string();
                if let Some(stall) = self.observe(line, elapsed) { return Ok(Some(stall)); }
            }
        }
        if (!self.files.is_empty() || self.dispatch_log.is_some()) && elapsed >= self.limits.silence
            && elapsed.saturating_sub(self.last_progress) >= self.limits.silence {
            return Ok(Some(format!("실패 — stall: 로그 무진행 {}s, 마지막 줄: {}",
                self.limits.silence.as_secs(), self.last_line.as_deref().unwrap_or("none"))));
        }
        Ok(None)
    }
}

/// `tmux kill-pane` alone leaves the agent running, so the pane's process group gets TERM first;
/// the runner's trap then records the run time.
fn stop_pane(pane: &str) {
    let pid = Command::new("tmux").args(["display-message", "-p", "-t", pane, "#{pane_pid}"]).output().ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse::<u32>().ok());
    if let Some(pid) = pid {
        let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).status();
        sleep(Duration::from_millis(200));
    }
    let _ = tmux_call("kill-pane", Command::new("tmux").args(["kill-pane", "-t", pane]));
}

fn write_stall_result(p: &Paths, pane: &str, last: &str) -> Result<(), String> {
    stop_pane(pane);
    fs::write(&p.last, last).map_err(|error| format!("{}: {error}", p.last.display()))?;
    let temp = p.exit.with_extension("exit.tmp");
    fs::write(&temp, "4\n").map_err(|error| format!("{}: {error}", temp.display()))?;
    fs::rename(&temp, &p.exit).map_err(|error| format!("{}: {error}", p.exit.display()))
}

enum WaitResult { Finished(String, Option<String>), Checkpoint(Duration, u64, Option<String>), StartupTimeout }

fn wait_for_exit(p: &Paths, pane: &str, watch: &mut LogWatch, limit: Duration, startup: Option<(Instant, Duration)>) -> Result<WaitResult, String> {
    watch.track_dispatch_log(&p.log);
    let started = Instant::now();
    let initial_size = file_size(&p.log);
    let mut first_log_seen = startup.is_none();
    loop {
        if let Some((pane_created, deadline)) = startup {
            if !first_log_seen && file_size(&p.log) > 0 {
                first_log_seen = true;
                eprintln!("00119805661 | dispatch-agent.startup | 01 | ack=received body.first_log_ms={} error=none", pane_created.elapsed().as_millis());
            }
            if !first_log_seen && pane_created.elapsed() >= deadline {
                return Ok(WaitResult::StartupTimeout);
            }
        }
        if let Ok(code) = fs::read_to_string(&p.exit) { return Ok(WaitResult::Finished(code, None)); }
        let elapsed = started.elapsed();
        if let Some(last) = watch.poll(elapsed)? {
            write_stall_result(p, pane, &last)?;
            return Ok(WaitResult::Finished("4\n".into(), Some(last)));
        }
        if elapsed >= limit {
            // Completion wins a race at the checkpoint boundary.
            if let Ok(code) = fs::read_to_string(&p.exit) { return Ok(WaitResult::Finished(code, None)); }
            return Ok(WaitResult::Checkpoint(elapsed, file_size(&p.log).saturating_sub(initial_size), watch.last_line.clone()));
        }
        let remaining = startup.filter(|_| !first_log_seen)
            .map(|(created, deadline)| deadline.saturating_sub(created.elapsed()))
            .unwrap_or(Duration::from_secs(2));
        sleep(Duration::from_secs(2).min(limit.saturating_sub(elapsed)).min(remaining));
    }
}

fn idle_dispatch_windows(windows: &str, now: u64, panes_for: impl Fn(&str) -> Result<String, String>) -> Vec<String> {
    windows.lines().filter_map(|line| {
        let mut fields = line.split('\t');
        let (id, name, created) = (fields.next()?, fields.next()?, fields.next()?.parse::<u64>().ok()?);
        if id.is_empty() { return None; }
        if name.starts_with("starting-") { return (now.saturating_sub(created) > 600).then(|| id.to_owned()); }
        if !name.starts_with("dispatch-") { return None; }
        let panes = panes_for(id).ok()?;
        let active = panes.lines().any(|line| line.split_once('\t').is_some_and(|(tag, dead)| !tag.trim().is_empty() && dead != "1"));
        (!active).then(|| id.to_owned())
    }).collect()
}

fn close_idle_dispatch_windows() {
    let Ok(windows) = Command::new("tmux").args(["list-windows", "-a", "-F", "#{window_id}\t#{window_name}\t#{window_activity}"]).output() else { return };
    if !windows.status.success() { return; }
    let text = String::from_utf8_lossy(&windows.stdout);
    for window in idle_dispatch_windows(&text, SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(), |id| {
        let out = Command::new("tmux").args(["list-panes", "-t", id, "-F", "#{@dispatch-agent}\t#{pane_dead}"]).output().map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }) {
        let _ = tmux_call("kill-window", Command::new("tmux").args(["kill-window", "-t", &window]));
    }
}

/// The idle sweep or the saved-window cleanup may already have closed it, which is not a failure.
fn close_window(window: &str) {
    let open = Command::new("tmux").args(["list-windows", "-a", "-F", "#{window_id}"]).output()
        .map(|out| String::from_utf8_lossy(&out.stdout).lines().any(|id| id == window))
        .unwrap_or(true);
    if !open { return; }
    if let Err(error) = tmux_call("kill-window", Command::new("tmux").args(["kill-window", "-t", window])) {
        tmux_target_log("detached", &format!("창 정리 실패: {error}"));
    }
}

fn close_saved_window(p: &Paths) {
    if let Ok(window) = fs::read_to_string(&p.window) {
        let window = window.trim();
        if !window.is_empty() { close_window(window); }
    }
    let _ = fs::remove_file(&p.window);
    let _ = fs::remove_file(&p.pane);
    close_idle_dispatch_windows();
}

fn startup_last(stages: &[(&str, u128)], deadline: Duration) -> String {
    let parts = stages.iter().map(|(name, ms)| format!("{name} {ms}ms")).collect::<Vec<_>>().join(", ");
    format!("실패 — 기동 지연: {parts}, 첫 로그 없음 {}s", deadline.as_secs())
}

fn startup_result(p: &Paths, pane: &str, stages: &[(&str, u128)], deadline: Duration,
    kill: impl FnOnce(&str)) -> Result<i32, String> {
    kill(pane);
    close_saved_window(p);
    let last = startup_last(stages, deadline);
    fs::write(&p.last, &last).map_err(|error| format!("startup last: {error}"))?;
    fs::write(&p.exit, "7\n").map_err(|error| format!("startup exit: {error}"))?;
    let slowest = stages.iter().max_by_key(|(_, ms)| ms).copied().unwrap_or(("unknown", 0));
    eprintln!("00119805661 | dispatch-agent.startup | 01 | ack=rejected body.slowest={} body.ms={} error=no_first_log", slowest.0, slowest.1);
    println!("{}", result_line("7", p, &last));
    Ok(7)
}

fn startup_deadline() -> Duration {
    std::env::var("DISPATCH_STARTUP_SECONDS").ok().and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0).map(Duration::from_secs).unwrap_or(Duration::from_secs(60))
}

fn current_analysis_problem(p: &Paths, row: u64) -> Option<String> {
    let attempt = state_number(&p.dir.join(format!("{row}.analysis.count"))).ok().flatten()?;
    let path = analysis_report_path(&p.dir, row, attempt);
    match fs::read_to_string(&path) {
        Ok(text) => analysis_report_problem(&text),
        Err(_) => Some(format!("{} 없음", path.display())),
    }
}

fn report_result(args: &Args, p: &Paths, code: String, stalled: Option<String>, submit_pass: bool) -> i32 {
    let source = if args.agent == "codex" { &p.last } else { &p.log };
    let mut last = stalled.clone().unwrap_or_else(|| last_line(&fs::read_to_string(source).unwrap_or_default()));
    // 최종 메시지 없이 실패하면(예: codex 시작 거부) 로그 끝줄을 붙여 위임자가 로그를 열지 않고 원인을 본다.
    if last.is_empty() && code.trim() != "0" {
        last = startup_failure_last(&fs::read_to_string(&p.log).unwrap_or_default());
    }
    if args.analysis && stalled.is_none() {
        if let Some(problem) = current_analysis_problem(p, args.row) { last = format!("실패 — 분석 보고서: {problem} ({last})"); }
    }
    let recorded = if analysis_process_timeout(&args.role, args.analysis, &last) {
        match mark_improvement_required(&args.project, args.row, &last) {
            Ok(_) => Some(format!(" process_defect=improve:{}", args.row)),
            Err(error) => {
                eprintln!("dispatch-agent improvement signal: {error}");
                Some(format!(" process_defect=signal-error:{}", args.row))
            }
        }
    } else if args.role == "verifier" && !args.analysis {
        let parsed_rows: Vec<u64> = args.rows.clone().unwrap_or_else(|| {
            fs::read_to_string(&p.script).ok()
                .and_then(|sh| sh.split("T26_ROWS=").nth(1)
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(|csv| csv.split(',').filter_map(|s| s.parse::<u64>().ok()).collect()))
                .unwrap_or_else(|| vec![args.row])
        });
        for &id in &parsed_rows {
            if id != args.row && p.log.exists() {
                let _ = fs::copy(&p.log, p.dir.join(format!("{id}.verify.log")));
            }
        }
        if let Some((row, line)) = verifier_process_defect_row(&last, &parsed_rows) {
            match mark_improvement_required(&args.project, row, &line) {
                Ok(_) => Some(format!(" process_defect=improve:{row}")),
                Err(error) => {
                    eprintln!("dispatch-agent improvement signal: {error}");
                    Some(format!(" process_defect=signal-error:{row}"))
                }
            }
        } else {
            verifier_failure_row(&last, &parsed_rows).and_then(|(row, line)|
                record_verifier_failure(&args.project, row, &line).then(|| format!(" recorded=I:{row}")))
        }
    } else { None };
    let suffix = recorded.unwrap_or_default();
    println!("{}", submit_result_line(&code, p, &last, &suffix, submit_pass));
    code.trim().parse().unwrap_or(1)
}

fn finish_result(args: &Args, p: &Paths, code: String, stalled: Option<String>) -> Result<i32, String> {
    if args.role == "improver" && stalled.is_none() && code.trim() == "0" {
        clear_improvement_required(&args.project, args.row)?;
    }
    if args.role != "agent" || args.analysis || stalled.is_some() || code.trim() != "0" {
        return Ok(report_result(args, p, code, stalled, false));
    }
    let doc = match fs::read_to_string(args.project.join(".ai/docs/role-delegator.md")) {
        Ok(doc) => doc,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("role-delegator.md: {error}")),
    };
    if section_items(&doc, "# 제출점검").is_empty() {
        return Ok(report_result(args, p, code, stalled, false));
    }
    let fields = agent_row_fields(&args.project, args.row)?;
    let state = fields["H"].as_str().unwrap_or("");
    if state != "검증" { return Ok(report_result(args, p, code, stalled, false)); }
    let outcome = submit_check_outcome_with(&args.role, args.analysis, &code, state, &doc, &args.project,
        |project, command| Command::new("timeout").args(["--signal=KILL", "120s", "bash", "-c", &format!("exec 2>&1; {command}")])
            .current_dir(project).output());
    match outcome {
        Ok(passed) => Ok(report_result(args, p, code, stalled, passed)),
        Err(failure) => {
            record_submit_failure_with(&args.project, args.row, &failure, grist_output)?;
            if !submit_check_retry_allowed(args.submit_resumes, &args.agent) {
                let last = format!("실패 — 제출점검: {}({}) 재시도 2회 초과", failure.name, failure.reason);
                fs::write(&p.last, &last).map_err(|error| format!("submit-check last: {error}"))?;
                fs::write(&p.exit, "8\n").map_err(|error| format!("submit-check exit: {error}"))?;
                return Ok(report_result(args, p, "8\n".into(), None, false));
            }
            let prompt = p.dir.join(format!("{}.submit-check.prompt", args.row));
            fs::write(&prompt, format!("[제출점검 자동 재개]\n제출점검 {} 실패: {} → {}. 원인을 고치고 유닛테스트 뒤 다시 task-submit 하라.\n",
                failure.name, submit_safe(&failure.command, 300), failure.tail))
                .map_err(|error| format!("submit-check prompt: {error}"))?;
            let resumed = Args { submit_resumes: args.submit_resumes + 1, resume: true, attach: false,
                decision: None, agent: args.agent.clone(), role: args.role.clone(), analysis: false,
                project: args.project.clone(), row: args.row, rows: args.rows.clone(), prompt_file: prompt,
                add_dirs: args.add_dirs.clone(), watch_logs: args.watch_logs.clone(), expect_seconds: args.expect_seconds };
            run(resumed)
        }
    }
}

const ATTACH_MAX_ELAPSED: Duration = Duration::from_secs(10 * 60);

fn attach_timeout_message(total: Duration, summary: &str, analysis: bool) -> Option<String> {
    if analysis {
        return (total >= ANALYSIS_LIMIT).then(|| format!(
            "실패 — 20분 초과: 경과 {}m, 분석 세션을 종료했습니다. resolve-deadlock 다음 단계로 넘어가세요 ({summary})",
            total.as_secs() / 60
        ));
    }
    (total >= ATTACH_MAX_ELAPSED).then(|| format!(
        "실패 — 10분 초과: 경과 {}m, 세션을 종료했습니다. 로그를 직접 읽지 말고 상황을 점검해 조치하세요 ({summary})",
        total.as_secs() / 60
    ))
}

fn process_stall_result(role: &str, analysis: bool, row: u64, stalled: String) -> String {
    if role == "verifier" && !analysis { format!("과정결함 — {row}: {stalled}") } else { stalled }
}

fn finish_completed_attach(args: &Args, p: &Paths) -> Option<Result<i32, String>> {
    let code = completed_exit(p)?;
    close_saved_window(p);
    let _ = fs::remove_file(&p.started);
    Some(finish_result(args, p, code, None))
}

/// An analysis run may start without watch logs: a failure with no logs is what it exists to explain.
fn watch_role(args: &Args) -> &str { if args.analysis { "analysis" } else { &args.role } }

fn run(mut args: Args) -> Result<i32, String> {
    let mut startup_stages = Vec::new();
    let stage_start = Instant::now();
    require_improvement_handled(&args.project, &args.role, args.row)?;
    if let Some(rejection) = stall_improvement(&args.project, args.row, &args.role, args.analysis)? {
        return Err(rejection);
    }
    check_row_owner(&args.project, args.row)?;
    if let Some(rows) = &args.rows {
        for &row in rows { check_row_owner(&args.project, row)?; }
    }
    startup_stages.push(("row-owner", stage_start.elapsed().as_millis()));
    let p = paths(&args.project, args.row, &args.role);
    fs::create_dir_all(&p.dir).map_err(|e| format!("{}: {e}", p.dir.display()))?;
    cleanup_old_dispatch(&p.dir, &active_dispatch_rows(), SystemTime::now());
    let analysis_active = p.dir.join(format!("{}.analysis.active", args.row));
    if args.attach {
        args.analysis |= args.role == "verifier" && analysis_active.exists();
        record_decision(&args.project, args.row, args.decision.as_deref().expect("parsed decision"))?;
        if let Some(code) = finish_completed_attach(&args, &p) { return code; }
        let pane = match fs::read_to_string(&p.pane) {
            Ok(pane) => pane,
            Err(error) => {
                if let Some(code) = finish_completed_attach(&args, &p) { return code; }
                return Err(format!("attach pane: {error}"));
            }
        };
        let mut watch = LogWatch::new(&args.project, &args.watch_logs, watch_role(&args), StallLimits::default())?;
        let result = wait_for_exit(&p, pane.trim(), &mut watch, role_checkpoint_limit(watch_role(&args), args.expect_seconds), None)?;
        return match result {
            WaitResult::Checkpoint(_elapsed, grown, last) => {
                let total = total_elapsed(&p, SystemTime::now())?;
                let tail = summarize_tail(&p)?;
                let summary = checkpoint_line(args.row, total, grown, &tail, &cached_stage(args.row), last.as_deref(), args.expect_seconds);
                if verifier_checkpoint_defect(&args.role, args.analysis, &tail) {
                    let defect = format!("과정결함 — {}: 검증 정체 {}", args.row, summary);
                    write_stall_result(&p, pane.trim(), &defect)?;
                    close_saved_window(&p);
                    let _ = fs::remove_file(&p.started);
                    return Ok(report_result(&args, &p, "4\n".into(), Some(defect), false));
                }
                if let Some(stalled) = attach_timeout_message(total, &summary, args.analysis) {
                    let stalled = process_stall_result(&args.role, args.analysis, args.row, stalled);
                    write_stall_result(&p, pane.trim(), &stalled)?;
                    close_saved_window(&p);
                    let _ = fs::remove_file(&p.started);
                    checkpoint_log("rejected", total.as_secs(), grown, tail.new, tail.repeat, "attach_10m_exceeded");
                    return Ok(report_result(&args, &p, "4\n".into(), Some(stalled), false));
                }
                checkpoint_log("received", total.as_secs(), grown, tail.new, tail.repeat, "none");
                println!("{}", result_line("6", &p, &summary));
                Ok(6)
            }
            WaitResult::Finished(code, stalled) => {
                close_saved_window(&p);
                let _ = fs::remove_file(&p.started);
                finish_result(&args, &p, code, stalled)
            }
            WaitResult::StartupTimeout => unreachable!("attach has no startup deadline"),
        };
    }
    let mut watch = LogWatch::new(&args.project, &args.watch_logs, watch_role(&args), StallLimits::default())?;
    let session = if args.resume {
        let log = fs::read_to_string(&p.log).map_err(|e| format!("{}: {e}", p.log.display()))?;
        Some(required_session_id(&log, &p.log)?.to_string())
    } else {
        None
    };
    let analysis_attempt = if args.analysis {
        match next_analysis_attempt(&p, args.row) {
            Ok(attempt) => Some(attempt),
            Err(last) => return write_preflight_rejection(&p, &last),
        }
    } else { None };
    let original = fs::read(&args.prompt_file).map_err(|e| format!("--prompt-file: {e}"))?;
    if !p.exit.exists() {
        if let Ok(pane) = fs::read_to_string(&p.pane) {
            let pane = pane.trim();
            if !pane.is_empty() { stop_pane(pane); }
        }
    }
    close_saved_window(&p);
    let tmux = std::env::var("TMUX").ok();
    let tmux_pane = std::env::var("TMUX_PANE").ok();
    let preflight_ms = std::cell::Cell::new(0);
    let (preflight, target) = match preflight_then_target(
        || {
            let started = Instant::now();
            let result = if args.role == "agent" {
                agent_row_fields(&args.project, args.row).and_then(|fields|
                    agent_preflight(&args.project, args.row, &p, &original, args.resume, &args.role, &fields))
                    .and_then(|()| entrypoint_preflight(&args.project, &original, args.resume))
                    .map(|()| String::new())
            } else if args.role == "improver" {
                agent_row_fields(&args.project, args.row).and_then(|fields| improver_preflight(args.row, &fields))
            } else {
                (if args.role == "verifier" && !args.analysis && !args.resume
                    && already_verified_local_cli_hook(&args.project, args.rows.as_deref().unwrap_or(&[args.row])) {
                    Err("실패 — 로컬 CLI 훅 경로가 task-hidden에서 이미 검증됨: 기존 명령·출력으로 마감하세요".to_string())
                } else if args.role == "verifier" && !args.analysis && !args.resume {
                    verifier_prompt_check(&original).and_then(|()| {
                        verifier_goals(&args.project, args.row, args.rows.as_deref())
                            .and_then(|goals| verifier_scope_check(&original, &goals))
                    })
                } else { Ok(()) })
                    .and_then(|()| ensure_verifier_browser(&args.role))
                    .and_then(|browser| verifier_preflight(&args.project, &args.role)
                        .map(|checks| format!("{browser}{checks}")))
            };
            preflight_ms.set(started.elapsed().as_millis());
            result
        },
        || tmux_target_timed(tmux.as_deref(), tmux_pane.as_deref(), args.row, &mut startup_stages),
    ) {
        Ok(result) => result,
        Err(last) if last.starts_with("실패 — ") || last.starts_with("stale runtime:") || last.starts_with("orphan runtime:") => {
            return write_preflight_rejection(&p, &last);
        }
        Err(last) if last.starts_with("실패 — tmux") => return write_tmux_failure(&p, &last),
        Err(error) => return Err(error),
    };
    startup_stages.insert(1, ("preflight", preflight_ms.get()));
    let mut window_guard = DetachedWindowGuard::new(&target, close_window);
    let header = role_header(std::env::var("T26_ROLE").ok().as_deref(), &args.role, args.row, args.rows.as_deref());
    let mut prompt = match analysis_attempt {
        Some(attempt) => format!("{}{}", header.replace("skill=role-verifier", "skill=resolve-deadlock"), analysis_prompt(&p.dir, args.row, attempt)),
        None => header,
    }.into_bytes();
    prompt.extend(role_doc_line(&args.project, &args.role).as_bytes());
    prompt.extend(verifier_runtime_line(&args.project, &args.role).as_bytes());
    prompt.extend(verifier_docs_line(&args.role, analysis_attempt.is_some(), &original, runtime_kind(&args.project)).as_bytes());
    prompt.extend(preflight.as_bytes());
    prompt.extend(task_context(args.row, args.rows.as_deref(), &args.project)?.as_bytes());
    if let Some(line) = agent_id_line(&args.project, args.row, &args.role, &original, |project, row| issue_agent_id(project, row)) {
        prompt.extend(line.as_bytes());
    }
    if let Ok(schema) = fs::read_to_string(schema_file()) {
        prompt.extend(schema_block(&String::from_utf8_lossy(&original), &schema).as_bytes());
    }
    prompt.extend(original);
    fs::write(&p.prompt, prompt).map_err(|e| format!("{}: {e}", p.prompt.display()))?;
    if analysis_attempt == Some(2) {
        let _ = fs::copy(&p.log, p.dir.join(format!("{}.analysis.1.log", args.row)));
    }
    if let Some(attempt) = analysis_attempt {
        fs::write(p.dir.join(format!("{}.analysis.count", args.row)), format!("{attempt}\n")).map_err(|error| format!("analysis count: {error}"))?;
        fs::write(&analysis_active, "").map_err(|error| format!("analysis state: {error}"))?;
    } else if args.role == "verifier" {
        let _ = fs::remove_file(&analysis_active);
    }
    let stale_paths = [&p.exit, &p.last, &p.log];
    for stale in stale_paths.into_iter().take(if args.resume { 2 } else { 3 }) {
        let _ = fs::remove_file(stale);
    }
    for stale in [&p.window, &p.pane, &p.started] { let _ = fs::remove_file(stale); }
    if !args.resume { let _ = fs::remove_file(&p.seen); }
    let path_env = std::env::var("PATH").unwrap_or_default();
    prepare_writable_dirs(&args.role, &args.agent, Path::new(STAGEHAND_STATE),
        &std::env::temp_dir().join("access-stagehand-locks"))?;
    let add_dirs = writable_dirs(&args.role, &args.agent, &args.add_dirs);
    fs::write(&p.script, runner_script(&args.agent, &args.role, &args.project, &add_dirs, session.as_deref(), args.row, args.rows.as_deref(), &path_env, &p, analysis_attempt.map(|attempt| analysis_report_path(&p.dir, args.row, attempt)).as_deref())).map_err(|e| e.to_string())?;
    let split_start = Instant::now();
    let mut pane_created = None;
    let pane = with_window_lock(&target.socket, &target.window, || {
        let panes = timed_tmux(&mut startup_stages, "list-panes", Command::new("tmux")
            .args(["list-panes", "-t", &target.window, "-F", "#{pane_id}\t#{@dispatch-agent}"]))?;
        let pane = timed_tmux(&mut startup_stages, "split-window", Command::new("tmux")
            .args(split_args(&target.pane, last_agent_pane(&panes)))
            .arg(&args.project)
            .arg("bash")
            .arg(&p.script))?;
        pane_created = Some(Instant::now());
        timed_tmux(&mut startup_stages, "set-option", Command::new("tmux")
            .args(["set-option", "-p", "-t", &pane, "@dispatch-agent", &args.row.to_string()]))?;
        if target.mode == "detached" {
            timed_tmux(&mut startup_stages, "rename-window", Command::new("tmux")
                .args(["rename-window", "-t", &target.window, &format!("dispatch-{}", args.row)]))?;
        }
        Ok(pane)
    })?;
    startup_stages.push(("split-and-option", split_start.elapsed().as_millis()));
    let pane_created = pane_created.expect("split-window returned a pane");
    eprintln!("dispatch-agent: row {} {} in pane {}", args.row, args.agent, pane);
    fs::write(&p.pane, &pane).map_err(|error| format!("pane state: {error}"))?;
    if target.mode == "detached" {
        fs::write(&p.window, &target.window).map_err(|error| format!("window state: {error}"))?;
    }
    if args.role == "agent" { record_agent_start(&p, args.resume, SystemTime::now())?; }
    write_start_time(&p, SystemTime::now())?;
    let deadline = startup_deadline();
    match wait_for_exit(&p, &pane, &mut watch, role_checkpoint_limit(watch_role(&args), args.expect_seconds), Some((pane_created, deadline)))? {
        WaitResult::Checkpoint(_elapsed, grown, last) => {
            let total = total_elapsed(&p, SystemTime::now())?;
            let tail = summarize_tail(&p)?;
            let summary = checkpoint_line(args.row, total, grown, &tail, &cached_stage(args.row), last.as_deref(), args.expect_seconds);
            if verifier_checkpoint_defect(&args.role, args.analysis, &tail) {
                let defect = format!("과정결함 — {}: 검증 정체 {}", args.row, summary);
                write_stall_result(&p, &pane, &defect)?;
                let _ = fs::remove_file(&p.started);
                return Ok(report_result(&args, &p, "4\n".into(), Some(defect), false));
            }
            window_guard.keep();
            checkpoint_log("received", total.as_secs(), grown, tail.new, tail.repeat, "none");
            println!("{}", result_line("6", &p, &summary));
            Ok(6)
        }
        WaitResult::Finished(code, stalled) => {
            drop(window_guard);
            close_saved_window(&p);
            let _ = fs::remove_file(&p.started);
            finish_result(&args, &p, code, stalled)
        }
        WaitResult::StartupTimeout => {
            window_guard.keep();
            startup_result(&p, &pane, &startup_stages, deadline, stop_pane)
        }
    }
}

fn main_result_code(result: Result<i32, String>) -> i32 {
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("dispatch-agent: {message}");
            2
        }
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let parsed = parse_args(&argv);
    if parsed.as_ref().err().is_some_and(|error| error.starts_with("--attach requires --decision")) {
        checkpoint_log("rejected", 0, 0, 0, 0, "missing_decision");
    }
    let code = main_result_code(parsed.and_then(run));
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    #[test]
    fn verifier_gets_oat_notes_dir_and_agent_does_not() {
        let home = std::env::temp_dir().join(format!("dispatch-oat-{}", std::process::id()));
        let notes = home.join("ai/skills/build-ui/ui-oat/references");
        std::fs::create_dir_all(&notes).unwrap();
        let mut dirs = Vec::new();
        super::verifier_add_dirs("agent", &home, &mut dirs);
        assert!(dirs.is_empty());
        super::verifier_add_dirs("verifier", &home, &mut dirs);
        super::verifier_add_dirs("verifier", &home, &mut dirs);
        assert_eq!(dirs, vec![notes]);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn old_dispatch_records_are_removed_except_active_row() {
        use std::time::{Duration, SystemTime};
        let dir = std::env::temp_dir().join(format!("dispatch-cleanup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = SystemTime::now() - Duration::from_secs(15 * 24 * 3600);
        for name in ["1.log", "2.log", "3.log"] {
            let file = std::fs::File::create(dir.join(name)).unwrap();
            file.set_times(std::fs::FileTimes::new().set_modified(old)).unwrap();
        }
        super::cleanup_old_dispatch(&dir, &["2".to_string()].into_iter().collect(), SystemTime::now());
        assert!(!dir.join("1.log").exists());
        assert!(dir.join("2.log").exists());
        assert!(!dir.join("3.log").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    use super::*;

    #[test]
    fn root_rows_get_the_tools_repo_writable_without_a_flag() {
        let home = std::env::temp_dir().join(format!("dah{}", std::process::id()));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
        let _cleanup = Cleanup(home.clone());
        fs::create_dir_all(home.join("ai")).unwrap();
        let (mut root, mut product) = (Vec::new(), Vec::new());
        workflow_add_dirs(&home.join("extensions"), &home, &mut root);
        workflow_add_dirs(&home.join("extensions/app"), &home, &mut product);
        workflow_add_dirs(&home.join("extensions"), &home, &mut root);
        fs::remove_dir_all(&home).unwrap();
        assert_eq!(root, [home.join("ai")]);
        assert!(product.is_empty());
    }
    use std::os::unix::process::ExitStatusExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ROLE_DOC_TEST_ID: AtomicUsize = AtomicUsize::new(0);

    fn agent_guard_fixture() -> (PathBuf, Paths, serde_json::Value, SystemTime) {
        let id = ROLE_DOC_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let project = std::env::current_dir().unwrap()
            .join(format!("test/tmp/agent_guard_{}_{}", std::process::id(), id));
        let p = paths(&project, 591, "agent");
        fs::create_dir_all(&p.dir).unwrap();
        fs::write(project.join("AGENTS.md"), "# 상태\n### 실행\nruntime : http://localhost:5175\n").unwrap();
        (project, p, serde_json::json!({"H":"진단", "E":42_400}), UNIX_EPOCH + Duration::from_secs(11_000))
    }

    fn write_runs(p: &Paths, runs: &[(SystemTime, u64)]) {
        let text: String = runs.iter().map(|(end, secs)| format!("{} {}\n",
            end.duration_since(UNIX_EPOCH).unwrap().as_millis(), secs * 1000)).collect();
        fs::write(&p.runs, text).unwrap();
    }

    fn write_verifier_log(p: &Paths, modified: SystemTime) {
        let log = p.dir.join("591.verify.log");
        fs::write(&log, "verifier ran\n").unwrap();
        File::options().write(true).open(log).unwrap().set_modified(modified).unwrap();
    }

    #[test]
    fn verifier_prompt_requires_real_flow_after_mock() {
        let fake_only = "- 사용자 입력 1: 명령 실행\n- 주의: Webhook은 requestUrl 가짜로 측정해 실제 n8n을 돌리지 않는다\n실제 흐름: 명령 실행";
        assert!(verifier_prompt_check(fake_only.as_bytes()).unwrap_err().contains("Mock 지시"));
        assert!(verifier_prompt_check("- 사용자 입력 1: 명령 실행".as_bytes()).unwrap_err().contains("실제 흐름 없음"));
        let ok = "mock 사례: 가짜 requestUrl로 호출 횟수\n실제 흐름: 배포본 명령 실행 → 실제 n8n POST → log.txt status=200";
        assert!(verifier_prompt_check(ok.as_bytes()).is_ok());
    }

    #[test]
    fn verifier_prompt_rejects_extra_old_flows_and_playback_wait() {
        let goals = "현재 페이지 저장 ⇒ Table16 값 생성\nYouTube 분량 메타데이터 저장";
        assert!(verifier_scope_check("기존 Table8 페이지 저장·학습·캡처 시작 동작도 확인한다".as_bytes(), goals)
            .unwrap_err().contains("검증 범위 초과"));
        assert!(verifier_scope_check("영상 재생 완료까지 대기해 분량을 확인한다".as_bytes(), goals)
            .unwrap_err().contains("검증 대기 초과"));
        assert!(verifier_scope_check("YouTube 메타데이터의 분량을 확인한다".as_bytes(), goals).is_ok());
        assert!(verifier_scope_check("기존 저장 흐름을 확인한다".as_bytes(), "기존 저장 흐름 유지").is_ok());
    }

    #[test]
    fn agent_preflight_rejects_multiple_implementations_before_target() {
        let (project, p, mut fields, _) = agent_guard_fixture();
        fields["H"] = serde_json::json!("작업");
        let message = "실패 — 구현 2개 이상: 행을 나눠 행마다 위임하세요";
        for prompt in [
            "1. 첫 구현\n2) 둘째 구현\n",
            "  1. 첫 구현\n\t2) 둘째 구현\n",
        ] {
            let mut target_called = false;
            let result = preflight_then_target(
                || agent_preflight(&project, 591, &p, prompt.as_bytes(), false, "agent", &fields)
                    .map(|()| String::new()),
                || { target_called = true; Ok(()) },
            );
            assert_eq!(result.unwrap_err(), message, "{prompt}");
            assert!(!target_called, "{prompt}");
        }
        for prompt in [
            "1. 한 구현\n진입점: src/one.rs first\n",
            "진입점: src/one.rs first\n진입점: src/one.rs second\n",
            "진입점: src/one.rs first\n진입점: src/two.rs second\n",
            "진입점: src/one.rs first, src/two.rs second\n",
        ] {
            assert!(agent_preflight(&project, 591, &p, prompt.as_bytes(), false, "agent", &fields).is_ok());
        }
        let multiple = b"1. first\n2. second\n";
        assert!(agent_preflight(&project, 591, &p, multiple, true, "agent", &fields).is_ok());
        assert!(agent_preflight(&project, 591, &p, multiple, false, "verifier", &fields).is_ok());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn entrypoint_preflight_rejects_stale_file_and_function() {
        let project = std::env::temp_dir().join(format!("dispatch-entrypoint-{}", std::process::id()));
        let src = project.join("src-tauri/src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("main.rs"), "fn main() { app::run(); }\n").unwrap();
        fs::write(src.join("lib.rs"), "pub fn run() { /* setup */ }\n").unwrap();
        let stale = "진입점: src-tauri/src/missing.rs run()\n".as_bytes();
        assert!(entrypoint_preflight(&project, stale, false).unwrap_err().contains("진입점 파일 없음"));
        let misplaced = "진입점: src-tauri/src/main.rs setup()\n".as_bytes();
        assert!(entrypoint_preflight(&project, misplaced, false).unwrap_err().contains("진입점 함수 없음"));
        let called_elsewhere = "진입점: src-tauri/src/main.rs run()\n".as_bytes();
        assert!(entrypoint_preflight(&project, called_elsewhere, false).unwrap_err().contains("진입점 함수 없음"));
        let original_error = "진입점: src-tauri/src/main.rs (Tauri setup), lib.rs에 메뉴 추가\n".as_bytes();
        assert!(entrypoint_preflight(&project, original_error, false).unwrap_err().contains("진입점 함수 없음"));
        let corrected = "진입점: src-tauri/src/lib.rs run() (Tauri Builder setup)\n".as_bytes();
        assert!(entrypoint_preflight(&project, corrected, false).is_ok());
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let rel = format!(".cache/dispatch-tilde-{}", std::process::id());
        fs::create_dir_all(home.join(&rel)).unwrap();
        fs::write(home.join(&rel).join("gate.rs"), "fn role_denial() {}\n").unwrap();
        let tilde = format!("진입점: ~/{rel}/gate.rs role_denial(383)\n");
        let tilde_result = entrypoint_preflight(&project, tilde.as_bytes(), false);
        fs::remove_dir_all(home.join(&rel)).unwrap();
        assert!(tilde_result.is_ok(), "{tilde_result:?}");
        assert!(entrypoint_preflight(&project, stale, true).is_ok());
        assert!(entrypoint_preflight(&project, "조사 위임\n진입점: src-tauri/src/missing.rs run()\n".as_bytes(), false).is_ok());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn agent_guard_rejects_missing_and_stale_observation() {
        let (project, p, fields, _) = agent_guard_fixture();
        let missing = agent_preflight(&project, 591, &p, b"", false, "agent", &fields).unwrap_err();
        assert_eq!(missing, "실패 — 관측 없음: 행 생성 뒤 591.verify.log가 필요해요");
        assert_eq!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).unwrap_err(), missing);
        record_agent_start(&p, false, UNIX_EPOCH + Duration::from_secs(10_500)).unwrap();
        assert_eq!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).unwrap_err(), missing);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn diagnostic_gate_skips_feature_rows() {
        let (project, p, mut fields, _) = agent_guard_fixture();
        fields["G"] = serde_json::json!("기능");
        assert!(agent_preflight(&project, 591, &p, b"plain text", false, "agent", &fields).is_ok());
        fields["G"] = serde_json::json!("오류");
        assert!(agent_preflight(&project, 591, &p, b"plain text", false, "agent", &fields).unwrap_err().contains("관측 없음"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn diagnostic_gate_uses_runtime_and_stage_not_prompt_words() {
        let (project, p, mut fields, _) = agent_guard_fixture();
        assert!(agent_preflight(&project, 591, &p, b"plain text", false, "agent", &fields).unwrap_err().contains("관측 없음"));
        fields["H"] = serde_json::json!("작업");
        assert!(agent_preflight(&project, 591, &p, b"openStagehand DOM", false, "agent", &fields).is_ok());
        fields["H"] = serde_json::json!("진단");
        fs::write(project.join("AGENTS.md"), "# 상태\n### 실행\nruntime : Blender 5.2 addon\n").unwrap();
        assert!(agent_preflight(&project, 591, &p, b"plain text", false, "agent", &fields).unwrap_err().contains("관측 없음"));
        fs::write(project.join("AGENTS.md"), "# 상태\n### 실행\nruntime : 없음(로컬 CLI)\n").unwrap();
        assert!(agent_preflight(&project, 591, &p, b"openStagehand DOM", false, "agent", &fields).is_ok());
        fs::write(project.join("AGENTS.md"), "# 상태\n### 실행\nruntime : 알 수 없음\n").unwrap();
        assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).unwrap_err().contains("판정할 수 없어요"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn agent_guard_allows_observation_after_kst_midnight_without_first_dispatch() {
        let (project, p, mut fields, _) = agent_guard_fixture();
        fields["E"] = serde_json::json!(1_793_491_200_u64);
        let kst_midnight = UNIX_EPOCH + Duration::from_secs(1_793_491_200 - 9 * 3600);
        write_verifier_log(&p, kst_midnight);
        assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).is_ok());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn observe_requires_fresh_verifier_execution_log() {
        let (project, p, fields, _) = agent_guard_fixture();
        let created = UNIX_EPOCH + Duration::from_secs(42_400 - 9 * 3600);
        let rejected = agent_preflight(&project, 591, &p, b"", false, "agent", &fields).unwrap_err();
        assert!(rejected.starts_with("실패 — 관측 없음"));
        write_verifier_log(&p, created - Duration::from_millis(1));
        assert_eq!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).unwrap_err(), rejected);
        write_verifier_log(&p, created);
        assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).is_ok());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn agent_guard_rejects_observation_before_kst_midnight_without_first_dispatch() {
        let (project, p, mut fields, _) = agent_guard_fixture();
        fields["E"] = serde_json::json!(1_793_491_200_u64);
        assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields)
            .unwrap_err().contains("관측 없음"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn agent_guard_rejects_elapsed_first_dispatch_and_third_resume() {
        let (project, p, fields, now) = agent_guard_fixture();
        write_runs(&p, &[(now - Duration::from_secs(1), 1_201)]);
        write_verifier_log(&p, UNIX_EPOCH + Duration::from_secs(10_000));
        assert_eq!(agent_preflight(&project, 591, &p, b"", true, "agent", &fields).unwrap_err(),
            ELAPSED_ANALYSIS_MESSAGE);
        fs::remove_file(&p.runs).unwrap();
        record_agent_start(&p, false, now - Duration::from_secs(60)).unwrap();
        write_verifier_log(&p, now);
        record_agent_start(&p, true, now).unwrap();
        record_agent_start(&p, true, now).unwrap();
        assert_eq!(state_number(&p.resumes).unwrap(), Some(2));
        assert_eq!(agent_preflight(&project, 591, &p, b"", true, "agent", &fields).unwrap_err(),
            "실패 — 재위임 한도: 실패 공통 원인을 $J·$C에 쓰고 사용자에게 보고하세요");
        fs::remove_dir_all(project).unwrap();
    }

    const VALID_REPORT: &str = "# 재현\n1회\n# 수치\n300ms\n# 배제한 가설\n없음\n# 원인 후보\nA\n# 확신도\n낮음\n# 해결책\nB\n# 확인 방법\nC\n";

    fn write_report(p: &Paths, attempt: u64, text: &str, modified: SystemTime) {
        let report = analysis_report_path(&p.dir, 591, attempt);
        fs::write(&report, text).unwrap();
        File::options().write(true).open(report).unwrap().set_modified(modified).unwrap();
    }

    #[test]
    fn deadlock_gate_allows_one_fix_after_a_valid_report_and_cause() {
        let (project, p, mut fields, _) = agent_guard_fixture();
        let now = SystemTime::now();
        fields["H"] = serde_json::json!("작업");
        let deadline = now - Duration::from_secs(60);
        write_runs(&p, &[(deadline, 1_201)]);
        write_report(&p, 2, VALID_REPORT, deadline + Duration::from_secs(1));
        fields["J"] = serde_json::json!("[추정] 원인");
        assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).is_ok());
        write_runs(&p, &[(deadline, 1_201), (now, 60)]);
        assert_eq!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).unwrap_err(), FIX_USED_MESSAGE);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn deadlock_gate_rejects_missing_stale_or_incomplete_reports_and_empty_cause() {
        let (project, p, mut fields, _) = agent_guard_fixture();
        fields["H"] = serde_json::json!("작업");
        let deadline = SystemTime::now() - Duration::from_secs(60);
        write_runs(&p, &[(deadline, 1_201)]);
        fields["J"] = serde_json::json!("[원인] 분석 확정");
        let reject = |fields: &serde_json::Value| agent_preflight(&project, 591, &p, b"", false, "agent", fields).unwrap_err();
        // a verifier log and an old $J no longer open the gate
        write_verifier_log(&p, deadline + Duration::from_secs(1));
        assert_eq!(reject(&fields), ELAPSED_ANALYSIS_MESSAGE);
        write_report(&p, 1, VALID_REPORT, deadline - Duration::from_secs(1));
        assert_eq!(reject(&fields), ELAPSED_ANALYSIS_MESSAGE);
        write_report(&p, 1, &VALID_REPORT.replace("# 해결책\nB\n", ""), deadline + Duration::from_secs(1));
        assert_eq!(reject(&fields), ELAPSED_ANALYSIS_MESSAGE);
        write_report(&p, 1, VALID_REPORT, deadline + Duration::from_secs(1));
        fields["J"] = serde_json::json!("  ");
        assert_eq!(reject(&fields), CAUSE_MISSING_MESSAGE);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn analysis_report_rejects_each_broken_section() {
        assert_eq!(analysis_report_problem(VALID_REPORT), None);
        assert!(analysis_report_problem(&VALID_REPORT.replace("# 재현\n1회\n", "")).unwrap().contains("재현"));
        assert!(analysis_report_problem(&VALID_REPORT.replace("A\n", "")).unwrap().contains("원인 후보"));
        assert!(analysis_report_problem(&VALID_REPORT.replace("300ms", "느림")).unwrap().contains("수치"));
        assert!(analysis_report_problem(&VALID_REPORT.replace("낮음", "모름")).unwrap().contains("확신도"));
    }

    #[test]
    fn analysis_attempts_stop_at_two_and_prompt_names_the_first_report() {
        let (project, p, _, _) = agent_guard_fixture();
        assert_eq!(next_analysis_attempt(&p, 591).unwrap(), 1);
        fs::write(p.dir.join("591.analysis.count"), "1\n").unwrap();
        assert_eq!(next_analysis_attempt(&p, 591).unwrap(), 2);
        assert!(analysis_prompt(&p.dir, 591, 2).contains("회차=2/2"));
        assert!(analysis_prompt(&p.dir, 591, 2).contains("591.analysis.1.md"));
        assert!(!analysis_prompt(&p.dir, 591, 1).contains("1차 보고서"));
        fs::write(p.dir.join("591.analysis.count"), "2\n").unwrap();
        assert!(next_analysis_attempt(&p, 591).unwrap_err().starts_with("실패 — 분석 한도"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn analysis_flag_starts_without_watch_logs_only_for_a_new_verifier() {
        let project = std::env::temp_dir().join(format!("dispatch-analysis-{}", std::process::id()));
        fs::create_dir_all(&project).unwrap();
        assert!(LogWatch::new(&project, &[], "verifier", StallLimits::default()).is_err());
        assert!(LogWatch::new(&project, &[], "analysis", StallLimits::default()).is_ok());
        let parse = |extra: &[&str]| {
            let mut argv: Vec<String> = ["--agent", "codex", "--project", "/", "--row", "5", "--prompt-file", "f"].map(String::from).to_vec();
            argv.extend(extra.iter().map(|arg| arg.to_string()));
            parse_args(&argv)
        };
        assert!(parse(&["--analysis", "--role", "verifier"]).unwrap().analysis);
        assert!(parse(&["--analysis"]).is_err());
        assert!(parse(&["--analysis", "--role", "verifier", "--resume"]).is_err());
        assert!(parse(&["--analysis", "--role", "verifier", "--rows", "1,2"]).is_err());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn analysis_attach_limit_is_twenty_minutes() {
        assert!(attach_timeout_message(Duration::from_secs(19 * 60), "진행", true).is_none());
        assert!(attach_timeout_message(Duration::from_secs(20 * 60), "진행", true).unwrap().contains("resolve-deadlock"));
    }

    #[test]
    fn elapsed_limit_sums_run_time_and_ignores_waits_between_dispatches() {
        let (project, p, mut fields, now) = agent_guard_fixture();
        fields["H"] = serde_json::json!("작업");
        // first dispatch hours ago, two 10-minute runs: wall clock is far past 20 minutes, run time is not.
        record_agent_start(&p, false, now - Duration::from_secs(10_000)).unwrap();
        write_runs(&p, &[(now - Duration::from_secs(9_000), 600), (now - Duration::from_secs(10), 600)]);
        assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).is_ok());
        // a third run crosses the cumulative limit at its end; analysis before that point does not count.
        write_runs(&p, &[(now - Duration::from_secs(9_000), 600), (now - Duration::from_secs(10), 600), (now - Duration::from_secs(5), 1)]);
        fields["J"] = serde_json::json!("[원인] 분석 확정");
        write_report(&p, 1, VALID_REPORT, now - Duration::from_secs(6));
        assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).unwrap_err().contains("20분 초과"));
        write_report(&p, 1, VALID_REPORT, now - Duration::from_secs(5));
        assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).is_ok());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn cumulative_stall_marks_one_improvement_across_roles() {
        let (project, p, _, now) = agent_guard_fixture();
        // 12 minutes of agent + 6 minutes of verifier: under the limit, nothing is marked
        write_runs(&p, &[(now - Duration::from_secs(60), 720)]);
        let verify = paths(&project, 591, "verifier");
        fs::write(&verify.runs, format!("{} 360000\n", now.duration_since(UNIX_EPOCH).unwrap().as_millis())).unwrap();
        assert_eq!(super::stall_improvement(&project, 591, "agent", false), Ok(None));
        assert_eq!(super::require_improvement_handled(&project, "agent", 591), Ok(()));
        // another 3 minutes of verifier crosses 20 minutes: one marker, the dispatch is sent to the improver
        fs::write(&verify.runs, format!("1 360000\n{} 180000\n", now.duration_since(UNIX_EPOCH).unwrap().as_millis())).unwrap();
        assert_eq!(super::stall_improvement(&project, 591, "improver", false), Ok(None));
        assert_eq!(super::stall_improvement(&project, 591, "verifier", true), Ok(None));
        assert!(super::stall_improvement(&project, 591, "verifier", false).unwrap().unwrap().contains("--role improver --row 591"));
        assert!(super::require_improvement_handled(&project, "agent", 591).unwrap_err().contains("--row 591"));
        assert_eq!(super::require_improvement_handled(&project, "improver", 591), Ok(()));
        // after the improver clears it, the same row is not marked again
        super::clear_improvement_required(&project, 591).unwrap();
        assert_eq!(super::stall_improvement(&project, 591, "agent", false), Ok(None));
        assert_eq!(super::require_improvement_handled(&project, "agent", 591), Ok(()));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn elapsed_limit_skips_closed_rows() {
        let (project, p, mut fields, now) = agent_guard_fixture();
        fields["H"] = serde_json::json!("작업");
        write_runs(&p, &[(now - Duration::from_secs(1), 3_000)]);
        assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).unwrap_err().contains("20분 초과"));
        for closed in ["완료", "확인"] {
            fields["H"] = serde_json::json!(closed);
            assert!(agent_preflight(&project, 591, &p, b"", false, "agent", &fields).is_ok());
        }
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn agent_guard_allows_fresh_observation_two_resumes_and_verifier() {
        let (project, p, fields, now) = agent_guard_fixture();
        record_agent_start(&p, false, now - Duration::from_secs(60)).unwrap();
        assert_eq!(state_number(&p.first).unwrap(), Some(10_940_000));
        write_verifier_log(&p, now);
        assert!(agent_preflight(&project, 591, &p, b"", true, "agent", &fields).is_ok());
        record_agent_start(&p, true, now).unwrap();
        record_agent_start(&p, true, now).unwrap();
        assert_eq!(state_number(&p.first).unwrap(), Some(10_940_000));
        assert!(agent_preflight(&project, 591, &p, b"", false, "verifier", &fields).is_ok());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn agent_rejection_writes_exit_five_without_selecting_pane() {
        let (project, p, fields, _) = agent_guard_fixture();
        let selected = std::cell::Cell::new(false);
        let failure = preflight_then_target(
            || agent_preflight(&project, 591, &p, b"run_cdp", false, "agent", &fields).map(|()| String::new()),
            || { selected.set(true); Ok("pane") },
        ).unwrap_err();
        assert_eq!(write_preflight_rejection(&p, &failure), Ok(5));
        assert!(!selected.get());
        assert!(!p.pane.exists());
        assert_eq!(fs::read_to_string(&p.exit).unwrap(), "5\n");
        assert_eq!(fs::read_to_string(&p.last).unwrap(), failure);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn checkpoint_tail_reports_repeated_404_and_stagnation() {
        let project = watch_test_project();
        let p = paths(&project, 569, "agent");
        fs::create_dir_all(&p.dir).unwrap();
        let mut log = String::new();
        for second in 0..10 { log.push_str(&format!("GET /api 404 at 14:01:{second:02}\n")); }
        fs::write(&p.log, &log).unwrap();
        let first = summarize_tail(&p).unwrap();
        assert_eq!(first.new, 1);
        let second_log = (10..20).map(|second| format!("GET /api 404 at 14:01:{second:02}\n")).collect::<String>();
        fs::write(&p.log, format!("{log}{second_log}")).unwrap();
        let tail = summarize_tail(&p).unwrap();
        assert_eq!(tail.new, 0);
        assert_eq!(tail.repeat, 10);
        let line = checkpoint_line(569, Duration::from_secs(300), second_log.len() as u64, &tail, "교정", None, None);
        assert!(line.contains("새 줄 0, 반복 GET /api 404 at # ×10"));
        assert!(line.ends_with("내용 정체"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn checkpoint_tail_reports_three_new_lines_and_attach_seen_state() {
        let project = watch_test_project();
        let p = paths(&project, 569, "verifier");
        fs::create_dir_all(&p.dir).unwrap();
        fs::write(&p.log, "step alpha\nstep beta\nstep gamma\n").unwrap();
        let first = summarize_tail(&p).unwrap();
        assert_eq!(first.new, 3);
        assert_eq!(first.last.as_deref(), Some("step gamma"));
        let line = checkpoint_line(569, Duration::from_secs(300), 30, &first, "교정", None, None);
        assert!(line.contains("새 줄 3, 반복 없음, 마지막 step gamma"));
        assert!(!line.contains("내용 정체"));
        assert!(p.seen.ends_with("569.verify.seen"));
        fs::write(&p.log, "step alpha\nstep beta\nstep gamma\nstep beta\n").unwrap();
        let attached = summarize_tail(&p).unwrap();
        assert_eq!(attached.new, 0);
        assert_eq!(attached.last.as_deref(), Some("step beta"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn checkpoint_tail_ignores_hook_metadata_during_real_progress() {
        let project = watch_test_project();
        let p = paths(&project, 569, "verifier");
        fs::create_dir_all(&p.dir).unwrap();
        fs::write(&p.log, format!("{}[verify-step] settings-action opened\n[verify-step] readback consumer=Grist value=Table8\n", "hook: PostToolUse\n".repeat(12))).unwrap();
        let tail = summarize_tail(&p).unwrap();
        assert_eq!(tail.new, 2);
        assert_eq!(tail.repeat, 0);
        assert!(!super::verifier_checkpoint_defect("verifier", false, &tail));
        assert!(!checkpoint_line(569, Duration::from_secs(240), 100, &tail, "검증", None, None).contains("내용 정체"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn completed_attach_reads_exit_without_pane_and_reports_existing_result() {
        let project = watch_test_project();
        let p = paths(&project, 563, "agent");
        fs::create_dir_all(&p.dir).unwrap();
        fs::write(&p.exit, "0\n").unwrap();
        fs::write(&p.last, "제출\n").unwrap();
        write_start_time(&p, UNIX_EPOCH + Duration::from_secs(1_000)).unwrap();
        assert!(!p.pane.exists());
        let code = completed_exit(&p).expect("completed run");
        assert_eq!(result_line(&code, &p, &last_line(&fs::read_to_string(&p.last).unwrap())),
            format!("exit=0 log={} last=제출", p.log.display()));
        let args = parse_args(&["--attach".into(), "--decision".into(), "계속 대기".into(),
            "--project".into(), project.to_string_lossy().into_owned(), "--row".into(), "563".into()]).unwrap();
        assert_eq!(finish_completed_attach(&args, &p).unwrap().unwrap(), 0);
        assert!(!p.started.exists());
        assert_eq!(completed_exit(&p).as_deref(), Some("0\n"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn attach_checkpoint_uses_first_wait_start_and_role_scoped_file() {
        let project = watch_test_project();
        let p = paths(&project, 563, "verifier");
        assert!(p.started.ends_with("563.verify.started"));
        fs::create_dir_all(&p.dir).unwrap();
        let first_wait = UNIX_EPOCH + Duration::from_secs(1_000);
        write_start_time(&p, first_wait).unwrap();
        let after_attach = first_wait + Duration::from_secs(6 * 60 + 25);
        let elapsed = total_elapsed(&p, after_attach).unwrap();
        assert_eq!(checkpoint_line(563, elapsed, 17, &TailSummary::default(), "교정", None, None),
            "진행 — 563: 경과 6m, 로그 +17B, 새 줄 0, 반복 없음, 마지막 없음, H=교정, 감시 없음, 내용 정체");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn checkpoint_preserves_pane_and_reports_progress_without_exit_file() {
        let project = watch_test_project();
        let p = paths(&project, 563, "agent");
        fs::create_dir_all(&p.dir).unwrap();
        fs::write(&p.log, "working\n").unwrap();
        let mut watch = LogWatch::new(&project, &[], "agent", StallLimits::default()).unwrap();
        let result = wait_for_exit(&p, "%17", &mut watch, Duration::from_millis(1), None).unwrap();
        match result {
            WaitResult::Checkpoint(elapsed, grown, line) => {
                assert!(elapsed >= Duration::from_millis(1));
                assert_eq!(grown, 0);
                assert_eq!(checkpoint_line(563, Duration::from_secs(300), grown, &TailSummary::default(), "교정", line.as_deref(), None),
                    "진행 — 563: 경과 5m, 로그 +0B, 새 줄 0, 반복 없음, 마지막 없음, H=교정, 감시 없음, 내용 정체");
            }
            _ => panic!("expected checkpoint"),
        }
        assert!(!p.exit.exists());
        fs::write(&p.exit, "0\n").unwrap();
        assert!(matches!(wait_for_exit(&p, "%17", &mut watch, Duration::from_millis(1), None).unwrap(),
            WaitResult::Finished(code, None) if code == "0\n"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn expected_seconds_sets_checkpoint_and_summary_for_new_and_attach() {
        let project = watch_test_project();
        let argv = ["--attach", "--decision", "계속", "--project", project.to_str().unwrap(),
            "--row", "566", "--expect-seconds", "1"].map(str::to_string);
        let args = parse_args(&argv).unwrap();
        assert_eq!(checkpoint_limit(args.expect_seconds), Duration::from_secs(1));
        let p = paths(&project, 566, "agent");
        fs::create_dir_all(&p.dir).unwrap();
        let mut watch = LogWatch::new(&project, &[], "agent", StallLimits::default()).unwrap();
        let started = Instant::now();
        let result = wait_for_exit(&p, "%17", &mut watch, Duration::from_millis(15), None).unwrap();
        assert!(started.elapsed() >= Duration::from_millis(15));
        assert!(matches!(result, WaitResult::Checkpoint(..)));
        let first = UNIX_EPOCH + Duration::from_secs(1_000);
        write_start_time(&p, first).unwrap();
        let total = total_elapsed(&p, first + Duration::from_secs(61)).unwrap();
        assert!(checkpoint_line(566, total, 0, &TailSummary::default(), "교정", None, args.expect_seconds).ends_with("기대 1s 초과, 내용 정체"));
        assert!(!checkpoint_line(566, Duration::from_millis(900), 0, &TailSummary::default(), "교정", None, args.expect_seconds).contains("기대"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn verifier_checkpoint_never_waits_past_five_minutes() {
        assert_eq!(super::role_checkpoint_limit("verifier", Some(540)), std::time::Duration::from_secs(300));
        assert_eq!(super::role_checkpoint_limit("verifier", Some(90)), std::time::Duration::from_secs(90));
        assert_eq!(super::role_checkpoint_limit("agent", Some(540)), std::time::Duration::from_secs(540));
        assert_eq!(super::role_checkpoint_limit("analysis", Some(540)), std::time::Duration::from_secs(540));
    }

    #[test]
    fn verifier_checkpoint_defect_requires_real_stagnation() {
        let mut tail = super::TailSummary { new: 20, repeat: 8, repeated: Some("}".into()), last: None };
        assert!(!super::verifier_checkpoint_defect("verifier", false, &tail));
        tail.repeated = Some("Error: retry failed".into());
        assert!(super::verifier_checkpoint_defect("verifier", false, &tail));
        assert!(!super::verifier_checkpoint_defect("agent", false, &tail));
        assert!(!super::verifier_checkpoint_defect("verifier", true, &tail));
        tail.new = 0;
        tail.repeated = None;
        assert!(super::verifier_checkpoint_defect("verifier", false, &tail));
    }

    #[test]
    fn process_defect_returns_one_pending_signal_without_launching() {
        let dir = std::env::temp_dir().join(format!("dispatch-improvement-signal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(super::mark_improvement_required(&dir, 853, "검증 정체"), Ok(true));
        assert_eq!(super::mark_improvement_required(&dir, 853, "검증 정체"), Ok(false));
        assert_eq!(std::fs::read_to_string(super::improvement_marker(&dir, 853)).unwrap(), "검증 정체");
        assert!(!dir.join(".ai/dispatch/853.auto-improve.prompt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_improvement_blocks_delegation_until_improver_finishes() {
        let dir = std::env::temp_dir().join(format!("dispatch-improvement-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(super::require_improvement_handled(&dir, "agent", 854), Ok(()));
        super::mark_improvement_required(&dir, 853, "검증 정체").unwrap();
        assert!(super::require_improvement_handled(&dir, "agent", 854).unwrap_err().contains("--row 853"));
        assert!(super::require_improvement_handled(&dir, "improver", 854).is_err());
        assert_eq!(super::require_improvement_handled(&dir, "improver", 853), Ok(()));
        let argv = ["--agent", "codex", "--role", "improver", "--project", dir.to_str().unwrap(), "--row", "853", "--prompt-file", "/dev/null"]
            .map(str::to_string);
        let args = super::parse_args(&argv).unwrap();
        let p = super::paths(&dir, 853, "improver");
        std::fs::write(&p.last, "improve-done recorded").unwrap();
        assert_eq!(super::finish_result(&args, &p, "0\n".into(), None), Ok(0));
        assert_eq!(super::require_improvement_handled(&dir, "agent", 854), Ok(()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_deadline_reports_stages_and_cleans_pane() {
        let project = watch_test_project();
        let p = paths(&project, 566, "agent");
        fs::create_dir_all(&p.dir).unwrap();
        let mut watch = LogWatch::new(&project, &[], "agent", StallLimits::default()).unwrap();
        let deadline = Duration::from_millis(10);
        let result = wait_for_exit(&p, "%17", &mut watch, Duration::from_secs(1), Some((Instant::now(), deadline))).unwrap();
        assert!(matches!(result, WaitResult::StartupTimeout));
        let mut killed = String::new();
        let stages = [("row-owner", 2), ("split-window", 7), ("set-option", 1)];
        assert_eq!(startup_result(&p, "%17", &stages, Duration::from_secs(60), |pane| killed = pane.to_string()).unwrap(), 7);
        assert_eq!(killed, "%17");
        assert_eq!(fs::read_to_string(&p.exit).unwrap(), "7\n");
        assert_eq!(fs::read_to_string(&p.last).unwrap(),
            "실패 — 기동 지연: row-owner 2ms, split-window 7ms, set-option 1ms, 첫 로그 없음 60s");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn first_log_allows_startup_to_reach_checkpoint() {
        let project = watch_test_project();
        let p = paths(&project, 566, "agent");
        fs::create_dir_all(&p.dir).unwrap();
        fs::write(&p.log, "started\n").unwrap();
        let mut watch = LogWatch::new(&project, &[], "agent", StallLimits::default()).unwrap();
        let result = wait_for_exit(&p, "%17", &mut watch, Duration::from_millis(1),
            Some((Instant::now() - Duration::from_secs(1), Duration::from_millis(10)))).unwrap();
        assert!(matches!(result, WaitResult::Checkpoint(..)));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn attach_requires_reason_and_accepts_same_run_without_prompt_or_agent() {
        let missing: Vec<String> = ["--attach", "--project", "/", "--row", "563"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_args(&missing).err().unwrap(), "--attach requires --decision \"<판단 사유>\"");
        let mut empty = missing.clone();
        empty.extend(["--decision".into(), " ".into()]);
        assert_eq!(parse_args(&empty).err().unwrap(), "--attach requires --decision \"<판단 사유>\"");
        empty.pop(); empty.push("계속 기다림".into());
        let args = parse_args(&empty).unwrap();
        assert!(args.attach);
        assert_eq!(args.decision.as_deref(), Some("계속 기다림"));
        assert!(args.prompt_file.as_os_str().is_empty());
    }

    #[test]
    fn attach_decision_records_d_and_checkpoint_keeps_detached_window() {
        record_decision_with(Path::new("/w/proj"), 563, "계속\n대기", |args| {
            assert_eq!(&args[..5], ["task-update", "563", "--project", "/w/proj", "--fields"]);
            assert_eq!(serde_json::from_str::<serde_json::Value>(args[5]).unwrap()["D"], "[🕧] 대기 판단: 계속 대기");
            Ok(Output { status: std::process::ExitStatus::from_raw(0), stdout: vec![], stderr: vec![] })
        }).unwrap();
        let closed = std::cell::RefCell::new(Vec::new());
        let detached = TmuxTarget { socket: "default".into(), pane: "%17".into(), window: "@8".into(), mode: "detached" };
        {
            let mut guard = DetachedWindowGuard::new(&detached, |id| closed.borrow_mut().push(id.to_owned()));
            guard.keep();
        }
        assert!(closed.borrow().is_empty());
        { let _guard = DetachedWindowGuard::new(&detached, |id| closed.borrow_mut().push(id.to_owned())); }
        assert_eq!(&*closed.borrow(), &["@8"]);
    }

    struct FakeRuntimeProbe {
        listeners: Vec<(u16, u32)>,
        processes: HashMap<u32, RuntimeProcess>,
    }

    impl RuntimeProbe for FakeRuntimeProbe {
        fn listeners(&self) -> Result<Vec<(u16, u32)>, String> { Ok(self.listeners.clone()) }
        fn process(&self, pid: u32) -> Result<Option<RuntimeProcess>, String> { Ok(self.processes.get(&pid).cloned()) }
    }

    fn runtime_fresh_fixture() -> (PathBuf, PathBuf, FakeRuntimeProbe) {
        let id = ROLE_DOC_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("test/tmp/runtime_fresh_{}_{}", std::process::id(), id));
        let project = root.join("project");
        let cwd = root.join("vite_host");
        fs::create_dir_all(project.join("server")).unwrap();
        fs::create_dir_all(&cwd).unwrap();
        fs::write(project.join("server/api.ts"), "export const ok = true;").unwrap();
        let process = RuntimeProcess {
            pid: 101, cwd, vite_command: Some("node_modules/.bin/vite".into()),
            started: SystemTime::now() + Duration::from_secs(60),
        };
        let probe = FakeRuntimeProbe {
            listeners: vec![(5175, 101)],
            processes: HashMap::from([(101, process)]),
        };
        (root, project, probe)
    }

    #[test]
    fn runtime_fresh_parses_actual_ss_columns_and_multiple_pids() {
        let output = "LISTEN 0 511 0.0.0.0:5175 0.0.0.0:* users:((\"node\",pid=101,fd=18),(\"node\",pid=102,fd=19))\nLISTEN 0 1 [::1]:8484 [::]:*\n";
        assert_eq!(parse_ss_listeners(output), vec![(5175, 101), (5175, 102)]);
    }

    #[test]
    fn runtime_fresh_rejects_source_modified_after_listener_started() {
        let (root, project, mut probe) = runtime_fresh_fixture();
        probe.processes.get_mut(&101).unwrap().started = UNIX_EPOCH + Duration::from_secs(1);
        let result = runtime_fresh_with(&project, &root, "runtime : http://localhost:5175/video", &probe).unwrap_err();
        assert!(result.starts_with("stale runtime: 5175 pid 101 started "));
        assert!(result.contains("project/server/api.ts modified"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_fresh_accepts_source_modified_before_loaded() {
        let (root, project, mut probe) = runtime_fresh_fixture();
        let cwd = probe.processes[&101].cwd.clone();
        probe.processes.get_mut(&101).unwrap().started = UNIX_EPOCH + Duration::from_secs(1);
        fs::create_dir_all(cwd.join(".ai/runtime")).unwrap();
        fs::write(cwd.join(".ai/runtime/5175.loaded"), "2099-01-01T00:00:00.000Z\n").unwrap();
        assert_eq!(runtime_fresh_with(&project, &root, "runtime : http://localhost:5175/video", &probe), Ok(true));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_fresh_uses_process_start_when_loaded_is_older() {
        let (root, project, probe) = runtime_fresh_fixture();
        let cwd = probe.processes[&101].cwd.clone();
        fs::create_dir_all(cwd.join(".ai/runtime")).unwrap();
        fs::write(cwd.join(".ai/runtime/5175.loaded"), "2000-01-01T00:00:00Z\n").unwrap();
        assert_eq!(runtime_fresh_with(&project, &root, "runtime : http://localhost:5175/video", &probe), Ok(true));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_fresh_rejects_source_modified_after_loaded() {
        let (root, project, mut probe) = runtime_fresh_fixture();
        let cwd = probe.processes[&101].cwd.clone();
        probe.processes.get_mut(&101).unwrap().started = UNIX_EPOCH + Duration::from_secs(1);
        fs::create_dir_all(cwd.join(".ai/runtime")).unwrap();
        fs::write(cwd.join(".ai/runtime/5175.loaded"), "2000-01-01T00:00:00.000Z\n").unwrap();
        let result = runtime_fresh_with(&project, &root, "runtime : http://localhost:5175/video", &probe).unwrap_err();
        assert!(result.starts_with("stale runtime: 5175 pid 101 loaded "));
        assert!(result.contains("project/server/api.ts modified"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_fresh_ignores_invalid_loaded_time() {
        let (root, project, mut probe) = runtime_fresh_fixture();
        let cwd = probe.processes[&101].cwd.clone();
        probe.processes.get_mut(&101).unwrap().started = UNIX_EPOCH + Duration::from_secs(1);
        fs::create_dir_all(cwd.join(".ai/runtime")).unwrap();
        fs::write(cwd.join(".ai/runtime/5175.loaded"), "yesterday\n").unwrap();
        let result = runtime_fresh_with(&project, &root, "runtime : http://localhost:5175/video", &probe).unwrap_err();
        assert!(result.starts_with("stale runtime: 5175 pid 101 started "));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_fresh_rejects_other_vite_port_with_same_cwd_and_command() {
        let (root, project, mut probe) = runtime_fresh_fixture();
        let mut other = probe.processes[&101].clone();
        other.pid = 102;
        probe.processes.insert(102, other);
        probe.listeners.push((5176, 102));
        assert_eq!(runtime_fresh_with(&project, &root, "runtime : http://127.0.0.1:5175/video", &probe).unwrap_err(),
            "orphan runtime: ports 5176 pids 102 — 남은 서버 종료 필요");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_fresh_accepts_current_listener() {
        let (root, project, probe) = runtime_fresh_fixture();
        assert_eq!(runtime_fresh_with(&project, &root, "runtime : http://localhost:5175/video", &probe), Ok(true));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_fresh_skips_without_localhost_port() {
        let (root, project, probe) = runtime_fresh_fixture();
        assert_eq!(runtime_fresh_with(&project, &root, "runtime : local CLI", &probe), Ok(false));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_fresh_skips_listener_outside_extensions() {
        let (root, project, mut probe) = runtime_fresh_fixture();
        probe.processes.get_mut(&101).unwrap().cwd = PathBuf::from("/tmp/external-service");
        probe.processes.get_mut(&101).unwrap().started = UNIX_EPOCH + Duration::from_secs(1);
        assert_eq!(runtime_fresh_with(&project, &root, "runtime : http://localhost:5175/video", &probe), Ok(true));
        fs::remove_dir_all(root).unwrap();
    }

    fn row_owner_output(code: i32, stdout: &str) -> Output {
        Output { status: std::process::ExitStatus::from_raw(code << 8), stdout: stdout.as_bytes().to_vec(), stderr: Vec::new() }
    }

    #[test]
    fn row_owner_accepts_matching_success_json() {
        assert!(row_owner_result(527, Ok(row_owner_output(0, r#"{"id":527,"section":"row","lines":[]}"#))).is_ok());
        assert!(row_owner_result(527, Ok(row_owner_output(0, r#"{"id":526,"section":"row","lines":[]}"#))).is_err());
    }

    #[test]
    fn row_owner_returns_grist_error_on_ok_false_json() {
        let mut output = row_owner_output(2, "");
        output.stderr = br#"{"ok":false,"boundary":"validation","error":"eagle_move does not own Table26 #526"}"#.to_vec();
        assert_eq!(row_owner_result(526, Ok(output)).unwrap_err(), "eagle_move does not own Table26 #526");
    }

    #[test]
    fn row_owner_rejects_command_execution_failure() {
        let error = io::Error::new(io::ErrorKind::NotFound, "grist missing");
        assert_eq!(row_owner_result(527, Err(error)).unwrap_err(), "grist task-show failed: grist missing");
    }

    #[test]
    fn task_row_uses_live_fields_and_rejects_other_ids() {
        let output = row_owner_output(0, r#"[{"id":531,"fields":{"H":"진단","I":"실패","J":"원인\n다음","K":"목표"}}]"#);
        assert_eq!(row_from_task(531, &output).unwrap(), "H=진단 | I=실패 | J=원인 / 다음 | K=목표");
        assert!(row_from_task(532, &output).is_err());
    }

    #[test]
    fn runtime_line_requires_status_execution_section() {
        let agents = "# 상태\n### 정보\nruntime : wrong\n### 실행\nruntime : local CLI\n# 규칙\n### 실행\nruntime : other";
        assert_eq!(runtime_line(agents), "runtime : local CLI\n");
        assert_eq!(runtime_line("# 규칙\n### 실행\nruntime : other"), "");
    }

    #[test]
    fn verifier_failure_only_records_a_named_row_and_truncates_i() {
        let rows = [526, 527];
        assert_eq!(verifier_failure_row("실패 — 527: consumer error", &rows), Some((527, "실패 — 527: consumer error".into())));
        assert!(verifier_failure_row("실패 — 528: consumer error", &rows).is_none());
        assert!(verifier_failure_row("실패 — 527: ", &rows).is_none());
        assert!(verifier_failure_row("완료 — 527: ok", &rows).is_none());
        let long = format!("실패 — 526: {}", "가".repeat(150));
        assert_eq!(verifier_failure_row(&long, &rows).unwrap().1.chars().count(), 120);
    }

    #[test]
    fn verifier_failure_updates_d_i_h_and_keeps_grist_error() {
        assert!(record_verifier_failure_with(Path::new("/w/proj"), 590, "실패 — 590: consumer error", |args| {
            assert_eq!(&args[..5], ["task-update", "590", "--project", "/w/proj", "--fields"]);
            let fields: serde_json::Value = serde_json::from_str(args[5]).unwrap();
            assert_eq!(fields["H"], "진단");
            assert_eq!(fields["I"], "실패 — 590: consumer error");
            assert_eq!(fields["D"], "[🛣] 검증 실패 — 590 원인 점검");
            Ok(Output { status: std::process::ExitStatus::from_raw(0), stdout: vec![], stderr: vec![] })
        }));
        let rejected = Output { status: std::process::ExitStatus::from_raw(1), stdout: vec![],
            stderr: r#"{"ok":false,"error":"H=진단 needs I and D"}"#.as_bytes().to_vec() };
        assert_eq!(grist_failure_error(&rejected), "H=진단 needs I and D");
    }

    fn role_doc_test_project(with_doc: bool) -> PathBuf {
        let id = ROLE_DOC_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("test/tmp/role_doc_{}_{}", std::process::id(), id));
        fs::create_dir_all(&project).unwrap();
        if with_doc {
            let docs = project.join(".ai/docs");
            fs::create_dir_all(&docs).unwrap();
            fs::write(docs.join("role-verifier.md"), "# 역할\n").unwrap();
        }
        project
    }

    #[test]
    fn agent_prompt_gets_issued_id_unless_present_or_verifier() {
        let project = Path::new("/tmp");
        let issue = |_: &Path, _: u64| Some("00120107151".to_string());
        assert_eq!(agent_id_line(project, 7, "agent", "원문".as_bytes(), issue).unwrap(), "에이전트 ID: 00120107151 (작업 게이트에 --agent-id 00120107151)\n");
        assert_eq!(agent_id_line(project, 7, "agent", "프롬프트의 11자리 에이전트 ID를 사용".as_bytes(), issue).unwrap(), "에이전트 ID: 00120107151 (작업 게이트에 --agent-id 00120107151)\n");
        assert_eq!(agent_id_line(project, 7, "agent", "에이전트 ID: 1".as_bytes(), issue).unwrap(), "에이전트 ID: 00120107151 (작업 게이트에 --agent-id 00120107151)\n");
        assert_eq!(agent_id_line(project, 7, "agent", "설명\n  에이전트 ID: 00120107151".as_bytes(), issue).unwrap(), "에이전트 ID: 00120107151 (작업 게이트에 --agent-id 00120107151)\n");
        assert_eq!(agent_id_line(project, 7, "agent", "에이전트 ID: 00120107151x".as_bytes(), issue).unwrap(), "에이전트 ID: 00120107151 (작업 게이트에 --agent-id 00120107151)\n");
        assert!(agent_id_line(project, 7, "agent", "설명\n에이전트 ID: 00120107151 (기존 발급)".as_bytes(), issue).is_none());
        assert!(agent_id_line(project, 7, "verifier", "원문".as_bytes(), issue).is_none());
    }

    #[test]
    fn role_doc_verifier_with_file_follows_role_header() {
        let project = role_doc_test_project(true);
        let prompt = format!("{}{}원문", role_header(None, "verifier", 526, None), role_doc_line(&project, "verifier"));
        assert_eq!(prompt, format!("[Role] from=위임자(main) to=검증자 row=526 skill=role-verifier\n역할 문서: {}\n원문", project.join(".ai/docs/role-verifier.md").display()));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn role_doc_verifier_without_file_is_empty() {
        let project = role_doc_test_project(false);
        assert_eq!(role_doc_line(&project, "verifier"), "");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn role_doc_agent_with_file_is_empty() {
        let project = role_doc_test_project(true);
        assert_eq!(role_doc_line(&project, "agent"), "");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn preflight_collects_every_failure_and_last_output_line() {
        let project = role_doc_test_project(true);
        fs::write(project.join(".ai/docs/role-verifier.md"),
            "# 역할\n# 사전점검\n- 적재 버전 : true\n- 앱 단일 프로세스 : false\n- MCP 연결 : echo 이전; echo 연결 안 됨; exit 1\n- 미저장 문서 : true\n# 다음\n- 제외 : false\n").unwrap();
        let failure = verifier_preflight(&project, "verifier").unwrap_err();
        assert_eq!(failure, "실패 — 사전점검: 앱 단일 프로세스(명령 실패 (exit 1)); MCP 연결(연결 안 됨)");
        assert!(verifier_failure_row(&failure, &[548]).is_none());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn preflight_passes_all_names_and_skips_missing_section_or_agent() {
        let project = role_doc_test_project(true);
        let doc = project.join(".ai/docs/role-verifier.md");
        fs::write(&doc, "# 역할\n- 무시 : false\n").unwrap();
        assert_eq!(verifier_preflight(&project, "verifier").unwrap(), "");
        fs::write(&doc, "# 사전점검\n- 적재 버전 : true\n- MCP 연결 : true\n").unwrap();
        assert_eq!(verifier_preflight(&project, "verifier").unwrap(), "사전점검: 통과 — 적재 버전, MCP 연결\n");
        fs::write(&doc, "# 사전점검\n- 검사 : false\n").unwrap();
        assert_eq!(verifier_preflight(&project, "agent").unwrap(), "");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn preflight_reports_unavailable_command_and_limits_reason() {
        let project = role_doc_test_project(true);
        fs::write(project.join(".ai/docs/role-verifier.md"),
            "# 사전점검\n- 실행 도구 : no_such_preflight_command_548\n").unwrap();
        let failure = verifier_preflight(&project, "verifier").unwrap_err();
        assert!(failure.starts_with("실패 — 사전점검: 실행 도구(실행 불가: "), "{failure}");
        assert!(failure.contains("no_such_preflight_command_548"));
        assert_eq!(preflight_reason(format!("{}\n", "가".repeat(100)).as_bytes(),
            std::process::ExitStatus::from_raw(1 << 8)).chars().count(), 80);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn submit_check_parser_shared_with_preflight_and_missing_section_unchanged() {
        let doc = "# 사전점검\n- 시작 : echo pre\n# 제출점검\n- 빌드 : cargo test\n- 빈 명령 : \n# 다음\n- 제외 : false\n";
        assert_eq!(section_items(doc, "# 사전점검"), vec![("시작".into(), "echo pre".into())]);
        assert_eq!(section_items(doc, "# 제출점검"), vec![("빌드".into(), "cargo test".into())]);
        let mut called = false;
        let result = submit_check_outcome_with("agent", false, "0", "검증", "# 역할\n", Path::new("/w/proj"), |_, _| {
            called = true; unreachable!()
        });
        assert_eq!(result.unwrap(), false);
        assert!(!called);
        assert_eq!(result_line("0", &sample(), "제출"), "exit=0 log=/w/proj/.ai/dispatch/307.log last=제출");
    }

    #[test]
    fn submit_check_passes_in_order_only_after_agent_submission() {
        let doc = "# 제출점검\n- 첫째 : one\n- 둘째 : two\n";
        let mut commands = Vec::new();
        let passed = submit_check_outcome_with("agent", false, "0", "검증", doc, Path::new("/w/proj"), |project, command| {
            assert_eq!(project, Path::new("/w/proj"));
            commands.push(command.to_owned());
            Ok(Output { status: std::process::ExitStatus::from_raw(0), stdout: Vec::new(), stderr: Vec::new() })
        }).unwrap();
        assert!(passed);
        assert_eq!(commands, ["one", "two"]);
        assert_eq!(submit_result_line("0", &sample(), "제출", "", passed),
            "exit=0 log=/w/proj/.ai/dispatch/307.log last=제출 submit-check=pass");
        for (role, analysis, code, state) in [("verifier", false, "0", "검증"), ("agent", true, "0", "검증"),
            ("agent", false, "1", "검증"), ("agent", false, "0", "교정")] {
            assert!(!submit_check_outcome_with(role, analysis, code, state, doc, Path::new("/w/proj"), |_, _| unreachable!()).unwrap());
        }
    }

    #[test]
    fn submit_check_first_failure_records_stage_and_limits_reason() {
        let doc = "# 제출점검\n- 실패 항목 : first\n- 실행 금지 : second\n";
        let mut calls = 0;
        let failure = match submit_check_outcome_with("agent", false, "0", "검증", doc, Path::new("/w/proj"), |_, _| {
            calls += 1;
            Ok(Output { status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: format!("앞줄\n{}\n", "가".repeat(150)).into_bytes(), stderr: Vec::new() })
        }) { Err(failure) => failure, Ok(_) => panic!("failure expected") };
        assert_eq!(calls, 1);
        assert_eq!(failure.name, "실패 항목");
        assert_eq!(failure.reason.chars().count(), 120);
        assert!(failure.tail.contains("앞줄"));
        record_submit_failure_with(Path::new("/w/proj"), 768, &failure, |args| {
            assert_eq!(&args[..5], ["task-update", "768", "--project", "/w/proj", "--fields"]);
            let fields: serde_json::Value = serde_json::from_str(args[5]).unwrap();
            assert_eq!(fields["H"], "교정");
            assert!(fields["I"].as_str().unwrap().starts_with("제출점검 실패 항목 실패: "));
            Ok(Output { status: std::process::ExitStatus::from_raw(0), stdout: Vec::new(), stderr: Vec::new() })
        }).unwrap();
    }

    #[test]
    fn submit_check_two_resumes_then_exit_eight() {
        assert!(submit_check_retry_allowed(0, "codex"));
        assert!(submit_check_retry_allowed(1, "codex"));
        assert!(!submit_check_retry_allowed(2, "codex"));
        assert!(!submit_check_retry_allowed(0, "gemini"));
        assert!(USAGE.contains("exit 8"));
    }

    fn sample() -> Paths {
        paths(Path::new("/w/proj"), 307, "agent")
    }

    #[test]
    fn records_live_under_ai_dispatch_by_row() {
        let p = sample();
        assert_eq!(p.log, PathBuf::from("/w/proj/.ai/dispatch/307.log"));
        assert_eq!(p.exit, PathBuf::from("/w/proj/.ai/dispatch/307.exit"));
        assert_eq!(p.last, PathBuf::from("/w/proj/.ai/dispatch/307.last"));
    }

    #[test]
    fn extra_writable_roots_become_codex_add_dir_flags() {
        let dirs = [PathBuf::from("/home/u/ai/scripts"), PathBuf::from("/home/u/ai/skills")];
        let cmd = agent_command("codex", Path::new("/w/proj"), &dirs, None, &sample());
        assert!(cmd.contains("network_access=true --add-dir '/home/u/ai/scripts' --add-dir '/home/u/ai/skills' -o "));
        assert!(!agent_command("gemini", Path::new("/w/proj"), &dirs, None, &sample()).contains("--add-dir"));
    }

    #[test]
    fn verifier_browser_attaches_or_launches_and_reports_distinct_failures() {
        use std::cell::Cell;
        let command = browser_launch_command();
        assert_eq!(command.get_program(), "node");
        assert_eq!(command.get_args().collect::<Vec<_>>(),
            [BROWSER_SCRIPT, "browser", "--detach"].map(std::ffi::OsStr::new).to_vec());
        let calls = Cell::new(0);
        let mode = ensure_browser_with(|| Ok(()), || { calls.set(calls.get() + 1); Ok(()) },
            Instant::now() + Duration::from_secs(1), |_| {}).unwrap();
        assert_eq!(mode, "attach");
        assert_eq!(calls.get(), 0);

        let available = Cell::new(false);
        let mode = ensure_browser_with(|| if available.get() { Ok(()) } else { Err("connect") },
            || { calls.set(calls.get() + 1); available.set(true); Ok(()) },
            Instant::now() + Duration::from_secs(1), |_| {}).unwrap();
        assert_eq!(mode, "launch");
        assert_eq!(calls.get(), 1);

        assert_eq!(ensure_browser_with(|| Err("connect"), || Err("spawn failed".into()),
            Instant::now() + Duration::from_secs(1), |_| {}).unwrap_err(), "launch-failed");
        let failure = ensure_browser_with(|| Err("read-timeout"), || Ok(()),
            Instant::now() + Duration::from_millis(1), sleep).unwrap_err();
        assert!(failure.starts_with("endpoint-absent probe=read-timeout tries=2 ms="), "{failure}");
    }

    fn mock_browser_endpoint(reply: Option<String>, hold: Duration) -> (Result<(), &'static str>, Duration) {
        use std::net::TcpListener;
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            if let Some(reply) = reply { socket.write_all(reply.as_bytes()).unwrap(); }
            sleep(hold);
        });
        let started = Instant::now();
        let result = browser_endpoint_at(address);
        let elapsed = started.elapsed();
        server.join().unwrap();
        (result, elapsed)
    }

    fn mock_browser_response(status: &str, body: &str) -> String {
        format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}", body.len())
    }

    #[test]
    fn browser_endpoint_reads_content_length_without_waiting_for_close() {
        let response = mock_browser_response("200 OK", "{\"webSocketDebuggerUrl\":\"ws://localhost/devtools\"}");
        let (result, elapsed) = mock_browser_endpoint(Some(response), Duration::from_millis(1100));
        assert_eq!(result, Ok(()));
        assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");
    }

    #[test]
    fn browser_endpoint_names_probe_failure_boundaries() {
        use std::net::TcpListener;
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        assert_eq!(browser_endpoint_at(address), Err("connect"));
        assert_eq!(mock_browser_endpoint(None, Duration::from_millis(1100)).0, Err("read-timeout"));
        assert_eq!(mock_browser_endpoint(Some(mock_browser_response("404 Not Found", "{}")), Duration::ZERO).0, Err("status"));
        assert_eq!(mock_browser_endpoint(Some(mock_browser_response("200 OK", "{}")), Duration::ZERO).0, Err("no-ws-url"));
        assert_eq!(mock_browser_endpoint(Some(mock_browser_response("200 OK", "not json")), Duration::ZERO).0, Err("json"));
        assert_eq!(mock_browser_endpoint(Some("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".into()), Duration::ZERO).0, Err("no-body"));
    }

    #[test]
    fn verifier_codex_receives_profile_and_runtime_writable_roots_only() {
        let supplied = [PathBuf::from("/home/u/ai/scripts")];
        let dirs = writable_dirs("verifier", "codex", &supplied);
        let command = agent_command("codex", Path::new("/w/proj"), &dirs, None, &sample());
        assert!(command.contains(" --add-dir '/home/tree/.config/codex-run-cdp'"));
        assert!(command.contains(" --add-dir '/home/tree/.local/share/access-stagehand'"));
        assert!(command.contains(" --add-dir '/home/tree/.local/state/access-stagehand'"));
        assert!(dirs.contains(&std::env::temp_dir().join("access-stagehand-locks")));
        let resumed = agent_command("codex", Path::new("/w/proj"), &dirs, Some("session"), &sample());
        assert!(resumed.contains("/home/tree/.config/codex-run-cdp"));
        assert!(resumed.contains("/home/tree/.local/share/access-stagehand"));
        assert!(resumed.contains("/home/tree/.local/state/access-stagehand"));
        assert_eq!(writable_dirs("agent", "codex", &supplied), supplied);
        assert_eq!(writable_dirs("verifier", "gemini", &supplied), supplied);
        assert_eq!(ensure_verifier_browser("agent").unwrap(), "");
    }

    #[test]
    fn verifier_creates_missing_state_and_shared_lock_directories_before_start() {
        let root = role_doc_test_project(false);
        let state = root.join("state/access-stagehand");
        let lock = root.join("tmp/access-stagehand-locks");
        assert!(!state.exists());
        assert!(!lock.exists());
        prepare_writable_dirs("verifier", "codex", &state, &lock).unwrap();
        assert!(state.is_dir());
        assert!(lock.is_dir());
        fs::remove_dir_all(&root).unwrap();

        prepare_writable_dirs("agent", "codex", &state, &lock).unwrap();
        assert!(!state.exists());
        assert!(!lock.exists());
    }

    #[test]
    fn first_worker_pane_opens_beside_the_caller() {
        assert_eq!(split_args("%12", None), ["split-window", "-h", "-d", "-t", "%12", "-P", "-F", "#{pane_id}", "-c"]);
    }

    #[test]
    fn next_worker_pane_opens_below_the_last_marked_pane() {
        let panes = "%12\t\n%13\t353\n%14\t\n%15\t354\n";
        assert_eq!(last_agent_pane(panes), Some("%15"));
        assert_eq!(split_args("%12", last_agent_pane(panes)), ["split-window", "-v", "-d", "-t", "%15", "-P", "-F", "#{pane_id}", "-c"]);
    }

    #[test]
    fn tmux_failure_log_keeps_the_error_on_one_line() {
        let line = tmux_log_line("split-window", "rejected", 12, "can't find pane: %12\nretry");
        assert_eq!(line, "00119803531 | tmux.split-window | 01 | ack=rejected body.ms=12 error=can't find pane: %12 retry");
    }

    #[test]
    fn tmux_target_selects_caller_or_detached_window_and_reports_absent_server() {
        let caller = select_tmux_target(Some("/tmp/tmux-1000/default,1,2"), Some("%3"), None, Ok("@4")).unwrap();
        assert_eq!((caller.mode, caller.pane.as_str(), caller.window.as_str()), ("caller", "%3", "@4"));
        let detached = select_tmux_target(None, None, Some(Ok("$1\n$2")), Ok("%9\t@8")).unwrap();
        assert_eq!((detached.mode, detached.socket.as_str(), detached.pane.as_str(), detached.window.as_str()),
            ("detached", "default", "%9", "@8"));
        let absent = select_tmux_target(None, None, Some(Err("no server running")), Err("not called")).err().unwrap();
        assert_eq!(absent, "실패 — tmux 없음: no server running");
        assert_ne!(main_result_code(Err(absent)), 0);
    }

    #[test]
    fn tmux_target_distinguishes_window_creation_failure() {
        assert_eq!(select_tmux_target(None, None, Some(Ok("$1")), Err("can't create window")).err().unwrap(),
            "실패 — tmux 창 생성 실패: can't create window");
    }

    #[test]
    fn absent_tmux_server_writes_last_and_nonzero_exit() {
        let project = role_doc_test_project(false);
        let p = paths(&project, 559, "agent");
        fs::create_dir_all(&p.dir).unwrap();
        let failure = select_tmux_target(None, None, Some(Err("no server running")), Err("not called")).err().unwrap();
        assert_eq!(write_tmux_failure(&p, &failure).unwrap(), 2);
        assert_eq!(fs::read_to_string(&p.last).unwrap(), "실패 — tmux 없음: no server running");
        assert_eq!(fs::read_to_string(&p.exit).unwrap(), "2\n");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn failed_preflight_never_selects_tmux_target() {
        let calls = std::cell::RefCell::new(Vec::new());
        let result = preflight_then_target(
            || { calls.borrow_mut().push("preflight"); Err("실패 — 사전점검: 앱 준비 실패".into()) },
            || { calls.borrow_mut().push("tmux-target"); Ok(()) },
        );
        assert_eq!(result.err().unwrap(), "실패 — 사전점검: 앱 준비 실패");
        assert_eq!(*calls.borrow(), ["preflight"]);
    }

    #[test]
    fn detached_window_is_closed_on_normal_stall_and_error_paths_only() {
        let target = |mode: &'static str, window: &str| TmuxTarget {
            socket: "default".into(), pane: "%9".into(), window: window.into(), mode,
        };
        let closed = std::cell::RefCell::new(Vec::new());
        for (mode, window) in [("detached", "@normal"), ("detached", "@stall"),
            ("detached", "@error"), ("caller", "@caller")] {
            let path = || -> Result<i32, ()> {
                let _guard = DetachedWindowGuard::new(&target(mode, window), |id| closed.borrow_mut().push(id.to_owned()));
                if window == "@error" { return Err(()); }
                if window == "@stall" { return Ok(4); }
                Ok(0)
            };
            let _ = path();
        }
        assert_eq!(*closed.borrow(), ["@normal", "@stall", "@error"]);
    }

    #[test]
    fn empty_last_reports_log_tail_on_startup_failure() {
        assert_eq!(startup_failure_last("Not inside a trusted directory and --skip-git-repo-check was not specified.\n"),
            "실패 — 시작: Not inside a trusted directory and --skip-git-repo-check was not specified.");
        assert_eq!(startup_failure_last(""), "실패 — 에이전트 출력 없음");
    }

    #[test]
    fn agent_commands_are_headless_and_scoped_to_the_project() {
        let p = sample();
        assert_eq!(
            agent_command("codex", Path::new("/w/proj"), &[], None, &p),
            "codex exec --skip-git-repo-check -C '/w/proj' -s workspace-write -c sandbox_workspace_write.network_access=true -o '/w/proj/.ai/dispatch/307.last' - < '/w/proj/.ai/dispatch/307.prompt'"
        );
        assert_eq!(
            agent_command("gemini", Path::new("/w/proj"), &[], None, &p),
            "gemini --approval-mode auto_edit -p \"$(cat '/w/proj/.ai/dispatch/307.prompt')\""
        );
    }

    #[test]
    fn runner_exports_worker_role_and_writes_exit_after_the_log() {
        let script = runner_script("codex", "agent", Path::new("/w/proj"), &[], None, 307, None, "/a:/b", &sample(), None);
        assert!(script.contains("export PATH='/a:/b' T26_ROW=307 T26_ROLE=worker\n"));
        assert!(script.contains("rm -f '/w/proj/.ai/dispatch/307.exit'\n"));
        assert!(script.contains("| tee '/w/proj/.ai/dispatch/307.log'\n"));
        assert!(script.ends_with("&& mv '/w/proj/.ai/dispatch/307.exit'.tmp '/w/proj/.ai/dispatch/307.exit'\n"));
        assert!(script.contains("run_end=$(date +%s%3N); echo \"$run_end $((run_end - run_start))\" >> '/w/proj/.ai/dispatch/307.runs'; }\ntrap 'record_run; echo 129 > '/w/proj/.ai/dispatch/307.exit'.tmp && mv '/w/proj/.ai/dispatch/307.exit'.tmp '/w/proj/.ai/dispatch/307.exit'; exit 129' HUP TERM\n"));
        assert!(script.contains("status=\"${PIPESTATUS[0]}\"\nrecord_run\n"));
    }

    #[test]
    fn result_is_one_line_with_the_last_message() {
        assert_eq!(last_line("done\n\n  final answer \n\n"), "final answer");
        assert_eq!(result_line("0\n", &sample(), "ok"), "exit=0 log=/w/proj/.ai/dispatch/307.log last=ok");
    }

    #[test]
    fn verifier_prompt_names_one_mode_and_one_runtime_doc() {
        let docs = "~/ai/skills/role-verifier/references";
        assert_eq!(verifier_docs_line("verifier", false, "603 저장 버튼".as_bytes(), Ok(RuntimeKind::Web)),
            format!("읽을 문서: {docs}/flow.md, {docs}/runtime-web.md\n"));
        assert_eq!(verifier_docs_line("verifier", false, "관측: 603 저장 실패".as_bytes(), Ok(RuntimeKind::App)),
            format!("읽을 문서: {docs}/observe.md, {docs}/runtime-app.md\n"));
        assert_eq!(verifier_docs_line("verifier", false, b"x", Err("unknown".into())), format!("읽을 문서: {docs}/flow.md\n"));
        assert_eq!(verifier_docs_line("verifier", true, b"x", Ok(RuntimeKind::Cli)), "");
        assert_eq!(verifier_docs_line("agent", false, b"x", Ok(RuntimeKind::Cli)), "");
    }

    #[test]
    fn improver_role_has_its_own_files_header_dirs_and_preflight() {
        assert!(paths(Path::new("/w/proj"), 7, "improver").log.ends_with("7.improve.log"));
        assert_eq!(role_header(None, "improver", 7, None), "[Role] from=위임자(main) to=개선자 row=7 skill=role-improver\n");
        assert!(writable_dirs("improver", "codex", &[]).contains(&PathBuf::from("/home/tree/ai")));
        assert!(!writable_dirs("agent", "codex", &[]).contains(&PathBuf::from("/home/tree/ai")));
        assert_eq!(improver_preflight(7, &serde_json::json!({"H": "확인"})), Ok(String::new()));
        assert_eq!(improver_preflight(7, &serde_json::json!({"H": "검증"})), Ok(String::new()));
        assert_eq!(improver_preflight(7, &serde_json::json!({"H": "진단"})), Ok(String::new()));
        assert!(improver_preflight(7, &serde_json::json!({"H": "초안"})).unwrap_err().starts_with("실패 — improver: #7 H=초안"));
        assert_eq!(
            verifier_process_defect_row("실패 — 844: Ctrl+5 ⇒ Grist 상태와 다이어리 기록은 확인됐으나 updateGrist 로그가 없음", &[844]),
            Some((844, "실패 — 844: Ctrl+5 ⇒ Grist 상태와 다이어리 기록은 확인됐으나 updateGrist 로그가 없음".into()))
        );
        assert!(verifier_failure_row("실패 — 844: Ctrl+5 ⇒ Grist 상태와 다이어리 기록은 확인됐으나 updateGrist 로그가 없음", &[844]).is_none());
        let argv: Vec<String> = ["--agent", "codex", "--project", "/", "--row", "7", "--prompt-file", "f", "--role", "improver"].map(String::from).to_vec();
        assert_eq!(parse_args(&argv).unwrap().role, "improver");
        let project = std::env::temp_dir().join(format!("dispatch-improver-{}", std::process::id()));
        fs::create_dir_all(&project).unwrap();
        assert!(LogWatch::new(&project, &[], "improver", StallLimits::default()).is_ok());
    }

    #[test]
    fn recorded_cli_hook_check_prevents_duplicate_verifier() {
        let project = std::env::temp_dir().join(format!("dispatch-cli-hook-{}", std::process::id()));
        std::fs::create_dir_all(project.join(".ai/gate")).unwrap();
        std::fs::write(project.join(".ai/gate/860.json"), r#"{"hidden":[{"test":"env -i HOME=/home/tree PATH=/usr/bin:/bin task-complete --hook"}]}"#).unwrap();
        assert!(super::local_cli_hook_verified_in_gate(&project, &[859, 860]));
        assert!(!super::local_cli_hook_verified_in_gate(&project, &[859]));
        std::fs::write(project.join(".ai/gate/860.json"), r#"{"hidden":[{"test":"cargo test -p task-complete"}]}"#).unwrap();
        assert!(!super::local_cli_hook_verified_in_gate(&project, &[860]));
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn result_routes_non_ordinary_outcomes_to_a_case_file() {
        let cases = "~/ai/skills/manage-project/references/cases";
        assert_eq!(result_line("6", &sample(), "진행 — 307: 경과 5m"),
            format!("exit=6 log=/w/proj/.ai/dispatch/307.log last=진행 — 307: 경과 5m case={cases}/slow.md"));
        for (code, name) in [("2", "crash"), ("7", "crash"), ("4", "stall"), ("5", "check-fail"), ("8", "check-fail")] {
            assert_eq!(case_path(code, "실패 — x"), Some(format!("{cases}/{name}.md")));
        }
        assert_eq!(case_path("0", "이 경로로 바꿀까요?"), Some(format!("{cases}/question.md")));
        assert_eq!(case_path("1", "실패 — 20분 초과: 경과 21m"), Some("~/ai/skills/resolve-deadlock/SKILL.md".into()));
        assert_eq!(case_path("5", "실패 — tmux 없음: tmux list-sessions failed"), Some(format!("{cases}/crash.md")));
        assert_eq!(case_path("0", "제출"), None);
        assert_eq!(case_path("1", "실패 — 테스트"), None);
    }

    #[test]
    fn arguments_reject_unknown_agents() {
        let argv: Vec<String> = ["--agent", "x", "--project", "/", "--row", "1", "--prompt-file", "f"].iter().map(|s| s.to_string()).collect();
        assert!(parse_args(&argv).err().unwrap().starts_with("--agent must be"));
    }

    #[test]
    fn session_id_uses_last_nonempty_match() {
        assert_eq!(session_id_from_log("session id: old\nother\n  session id: new\n"), Some("new"));
        assert_eq!(session_id_from_log("session id: \nother\n"), None);
    }

    #[test]
    fn resume_command_restores_sandbox_network_roots_and_prompt() {
        let dirs = [PathBuf::from("/home/u/ai/scripts"), PathBuf::from("/home/u/has space")];
        let cmd = agent_command("codex", Path::new("/w/proj"), &dirs, Some("abc-123"), &sample());
        assert_eq!(cmd, "codex exec resume 'abc-123' -c 'sandbox_mode=\"workspace-write\"' -c sandbox_workspace_write.network_access=true -c 'sandbox_workspace_write.writable_roots=[\"/home/u/ai/scripts\",\"/home/u/has space\"]' -o '/w/proj/.ai/dispatch/307.last' - < '/w/proj/.ai/dispatch/307.prompt'");
        assert!(!cmd.contains(" -C "));
        assert!(!cmd.contains(" --add-dir "));
    }

    #[test]
    fn resume_runner_appends_log_and_removes_old_exit() {
        let script = runner_script("codex", "agent", Path::new("/w/proj"), &[], Some("abc-123"), 307, None, "/a:/b", &sample(), None);
        assert!(script.contains("rm -f '/w/proj/.ai/dispatch/307.exit'\n"));
        assert!(script.contains("T26_ROW=307 T26_ROLE=worker\n"));
        assert!(script.contains("2>&1 | tee -a '/w/proj/.ai/dispatch/307.log'\n"));
    }

    #[test]
    fn resume_rejects_gemini_and_missing_session_id() {
        let argv: Vec<String> = ["--resume", "--agent", "gemini", "--row", "1", "--prompt-file", "f"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_args(&argv).err().unwrap(), "--resume supports codex only");
        assert_eq!(required_session_id("no session here\n", &sample().log).err().unwrap(),
            "no session id in /w/proj/.ai/dispatch/307.log");
    }

    #[test]
    fn role_parsing_paths_environment_and_headers() {
        let base: Vec<String> = ["--agent", "codex", "--project", "/", "--row", "504", "--prompt-file", "f"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_args(&base).unwrap().role, "agent");
        let mut verifier = base.clone();
        verifier.extend(["--role".into(), "verifier".into()]);
        let parsed = parse_args(&verifier).unwrap();
        assert_eq!(parsed.role, "verifier");
        verifier.pop(); verifier.push("invalid".into());
        assert!(parse_args(&verifier).err().unwrap().starts_with("--role must"));
        let p = paths(Path::new("/w/proj"), 504, "verifier");
        assert_eq!(p.log, PathBuf::from("/w/proj/.ai/dispatch/504.verify.log"));
        assert_eq!(p.prompt, PathBuf::from("/w/proj/.ai/dispatch/504.verify.prompt"));
        assert!(runner_script("codex", "verifier", Path::new("/w/proj"), &[], None, 504, None, "/bin", &p, None).contains("T26_ROLE=verifier"));
        assert!(runner_script("codex", "verifier", Path::new("/w/proj"), &[], Some("abc"), 504, None, "/bin", &p, None).contains("tee -a '/w/proj/.ai/dispatch/504.verify.log'"));
        for (sender, from) in [(None, "위임자(main)"), (Some("worker"), "대행자"), (Some("verifier"), "검증자")] {
            assert_eq!(role_header(sender, "agent", 504, None), format!("[Role] from={from} to=대행자 row=504 skill=role-agent\n"));
            assert_eq!(role_header(sender, "verifier", 504, None), format!("[Role] from={from} to=검증자 row=504 skill=role-verifier\n"));
        }
        assert_eq!(role_header(None, "verifier", 504, None), "[Role] from=위임자(main) to=검증자 row=504 skill=role-verifier\n");
    }

    #[test]
    fn verifier_rows_parse_export_and_header() {
        let argv: Vec<String> = ["--agent", "codex", "--project", "/", "--row", "528", "--rows", "526,527", "--role", "verifier", "--prompt-file", "f"]
            .iter().map(|s| s.to_string()).collect();
        let parsed = parse_args(&argv).unwrap();
        assert_eq!(parsed.rows.as_deref(), Some(&[526, 527][..]));
        let script = runner_script("codex", "verifier", Path::new("/w/proj"), &[], None, 528, parsed.rows.as_deref(), "/bin", &sample(), None);
        assert!(script.contains("T26_ROW=528 T26_ROLE=verifier T26_ROWS=526,527\n"));
        assert_eq!(role_header(None, "verifier", 528, parsed.rows.as_deref()), "[Role] from=위임자(main) to=검증자 row=526,527 skill=role-verifier\n");
    }

    #[test]
    fn verifier_rows_reject_worker_and_malformed_list() {
        let base: Vec<String> = ["--agent", "codex", "--project", "/", "--row", "528", "--rows", "526,527", "--prompt-file", "f"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_args(&base).err().unwrap(), "--rows requires --role verifier");
        let mut verifier = base.clone();
        verifier.extend(["--role".into(), "verifier".into()]);
        verifier[7] = "526,bad".into();
        assert!(parse_args(&verifier).err().unwrap().starts_with("--rows must be"));
        verifier[7] = "526,".into();
        assert!(parse_args(&verifier).err().unwrap().starts_with("--rows must be"));
    }

    fn watch_test_project() -> PathBuf {
        let id = ROLE_DOC_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("test/tmp/log_watch_{}_{}", std::process::id(), id));
        fs::create_dir_all(project.join(".ai/logs")).unwrap();
        project
    }

    fn test_limits() -> StallLimits {
        StallLimits { repeated_rejections: 2, silence: Duration::from_secs(5) }
    }

    #[test]
    fn repeated_rejected_lines_ignore_time_but_require_adjacency() {
        let project = watch_test_project();
        let log = project.join(".ai/logs/one.log");
        fs::write(&log, "old line\n").unwrap();
        let mut watch = LogWatch::new(&project, &[], "agent", test_limits()).unwrap();
        assert!(watch.poll(Duration::ZERO).unwrap().is_none());
        let first = "10:00:00 | 00119805461 | job.claim | 01 | ack=rejected body.status=invalid_body";
        let different = "10:00:01 | 00119805461 | job.claim | 02 | ack=rejected body.status=invalid_body";
        fs::write(&log, format!("old line\n{first}\n{different}\n")).unwrap();
        assert!(watch.poll(Duration::from_secs(1)).unwrap().is_none());
        fs::write(&log, format!("old line\n{first}\n{different}\n10:00:02 | 00119805461 | job.claim | 02 | ack=rejected body.status=invalid_body\n")).unwrap();
        assert_eq!(watch.poll(Duration::from_secs(2)).unwrap(),
            Some("실패 — stall: 10:00:02 | 00119805461 | job.claim | 02 | ack=rejected body.status=invalid_body".into()));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn silence_stalls_after_limit_and_new_lines_reset_clock() {
        let project = watch_test_project();
        let log = project.join(".ai/logs/one.log");
        fs::write(&log, "old line\n").unwrap();
        let mut watch = LogWatch::new(&project, &[], "agent", test_limits()).unwrap();
        assert!(watch.poll(Duration::from_secs(4)).unwrap().is_none());
        fs::write(&log, "old line\nnew line\n").unwrap();
        assert!(watch.poll(Duration::from_secs(5)).unwrap().is_none());
        assert!(watch.poll(Duration::from_secs(9)).unwrap().is_none());
        assert_eq!(watch.poll(Duration::from_secs(10)).unwrap(), Some("실패 — stall: 로그 무진행 5s, 마지막 줄: new line".into()));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn dispatch_log_growth_resets_silence_even_without_watch_lines() {
        let project = watch_test_project();
        let watched = project.join(".ai/logs/one.log");
        let dispatch = project.join(".ai/dispatch/590.log");
        fs::create_dir_all(dispatch.parent().unwrap()).unwrap();
        fs::write(&watched, "old line\n").unwrap();
        fs::write(&dispatch, "start").unwrap();
        let mut watch = LogWatch::new(&project, &[], "agent", test_limits()).unwrap();
        watch.track_dispatch_log(&dispatch);
        assert!(watch.poll(Duration::from_secs(4)).unwrap().is_none());
        fs::write(&dispatch, "start and still writing").unwrap();
        assert!(watch.poll(Duration::from_secs(5)).unwrap().is_none());
        assert!(watch.poll(Duration::from_secs(9)).unwrap().is_none());
        assert_eq!(watch.poll(Duration::from_secs(10)).unwrap(),
            Some("실패 — stall: 로그 무진행 5s, 마지막 줄: none".into()));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn verifier_requires_watch_source_and_late_explicit_file_is_read() {
        let project = watch_test_project();
        assert_eq!(LogWatch::new(&project, &[], "verifier", test_limits()).err().unwrap(),
            "verifier needs --watch-log <record-log path> or .ai/docs/role-verifier.md watch-source: dispatch-log");
        let doc = project.join(".ai/docs/role-verifier.md");
        fs::create_dir_all(doc.parent().unwrap()).unwrap();
        fs::write(&doc, "watch-source: unrelated\n").unwrap();
        assert!(LogWatch::new(&project, &[], "verifier", test_limits()).is_err());
        fs::write(&doc, "watch-source: dispatch-log\n").unwrap();
        let fake = project.join("test/tmp/runtime-diagnostic.log");
        assert_eq!(LogWatch::new(&project, std::slice::from_ref(&fake), "verifier", test_limits()).err().unwrap(),
            "verifier --watch-log must name a runtime log, not test/tmp");
        let mut dispatch_watch = LogWatch::new(&project, &[], "verifier", test_limits()).unwrap();
        let dispatch = project.join(".ai/dispatch/569.verify.log");
        fs::create_dir_all(dispatch.parent().unwrap()).unwrap();
        fs::write(&dispatch, "[verify-step] settings-action opened\n").unwrap();
        dispatch_watch.track_dispatch_log(&dispatch);
        assert!(dispatch_watch.poll(Duration::from_secs(4)).unwrap().is_none());
        fs::write(&dispatch, "[verify-step] settings-action opened\n[verify-step] readback consumer=Grist value=Table8\n").unwrap();
        assert!(dispatch_watch.poll(Duration::from_secs(5)).unwrap().is_none());
        assert!(dispatch_watch.poll(Duration::from_secs(9)).unwrap().is_none());
        assert!(dispatch_watch.poll(Duration::from_secs(10)).unwrap().is_some());
        let log = project.join("external.log");
        let mut watch = LogWatch::new(&project, std::slice::from_ref(&log), "verifier", test_limits()).unwrap();
        fs::write(&log, "new line\n").unwrap();
        assert!(watch.poll(Duration::from_secs(4)).unwrap().is_none());
        assert_eq!(watch.last_line.as_deref(), Some("new line"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn worker_without_logs_has_no_silence_watch() {
        let project = watch_test_project();
        let mut watch = LogWatch::new(&project, &[], "agent", test_limits()).unwrap();
        assert!(watch.poll(Duration::from_secs(30)).unwrap().is_none());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn schema_selects_explicit_and_mapped_types_and_keeps_message_block() {
        let schema = "schema=v1\n## 1. 판정 타입\n- `[타입]` 변형 검사\n- `[순서]` 순서 검사\n- `[형식]` 문자열 검사\n\n## 2. 기호 → 판정 타입\n- `🕧` → `[순서]`\n- `✒️` → `[형식]`\n\n## 3. 위임 메시지\n```\n판정: [<타입>] 조건\n```\n## 4. 분해\n끝\n";
        let block = schema_block("판정: [타입] A\n오류 [🕧] B", schema);
        assert!(block.starts_with("[Schema v1]\n- `[타입]` 변형 검사\n- `[순서]` 순서 검사\n"));
        assert!(!block.contains("- `[형식]` 문자열 검사"));
        assert!(block.contains("## 3. 위임 메시지\n```\n판정: [<타입>] 조건\n```"));
        assert!(!block.contains("## 4."));
        assert_eq!(schema_block("다른 판정", schema), "[Schema v1]\n## 3. 위임 메시지\n```\n판정: [<타입>] 조건\n```\n");
        assert!(schema_block("없는 파일", "").is_empty());
    }

    #[test]
    fn idle_dispatch_windows_selects_only_dispatch_windows_without_active_pane() {
        let windows = "@0\tagy\t100\n@39\tdispatch-624\t100\n@41\tdispatch-624\t100\n@48\tdispatch-625\t100\n@49\tstarting-1\t100\n@50\tstarting-2\t900\n@51\tdispatch-9\t100\n";
        let idle = idle_dispatch_windows(windows, 1000, |id| match id {
            "@39" => Ok("\t0\n".into()),
            "@41" => Ok("624\t0\n".into()),
            "@48" => Ok("   \t0\n".into()),
            "@51" => Ok("9\t1\n".into()),
            _ => Ok(String::new()),
        });
        assert_eq!(idle, vec!["@39".to_string(), "@48".to_string(), "@49".to_string(), "@51".to_string()]);
    }

    #[test]
    fn attach_timeout_message_fires_at_ten_minutes() {
        assert!(attach_timeout_message(Duration::from_secs(9 * 60 + 59), "진행", false).is_none());
        let msg = attach_timeout_message(Duration::from_secs(10 * 60), "진행 — 624", false).unwrap();
        assert!(msg.starts_with("실패 — 10분 초과: 경과 10m, 세션을 종료했습니다."));
        assert!(msg.contains("로그를 직접 읽지 말고"));
        let defect = super::process_stall_result("verifier", false, 860, msg);
        assert_eq!(super::verifier_process_defect_row(&defect, &[860]).map(|(row, _)| row), Some(860));
        assert_eq!(super::process_stall_result("verifier", true, 860, "분석".into()), "분석");
        assert!(super::analysis_process_timeout("verifier", true, "실패 — 20분 초과: 분석 정체"));
        assert!(!super::analysis_process_timeout("verifier", false, "실패 — 20분 초과"));
    }
}
