use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DAY: u64 = 86_400;
const RECENT_MARK: u64 = 15;

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }
fn main_session() -> bool { std::env::var("T26_ROLE").is_ok_and(|role| role.is_empty()) || std::env::var("T26_ROLE").is_err() }
fn home() -> PathBuf { PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/home/tree".into())) }
fn root() -> PathBuf {
    std::env::var_os("CLI_WATCH_BLACKBOX_DIR").map(PathBuf::from).unwrap_or_else(|| home().join("ai/.blackbox"))
}
fn safe(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') { c } else { '_' }).collect()
}
fn arg(args: &[String], key: &str) -> Option<String> {
    args.windows(2).find(|pair| pair[0] == key).map(|pair| pair[1].clone())
}
fn initialize(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir.join(".sessions"))?;
    let ignore = dir.join(".gitignore");
    if fs::read_to_string(&ignore).ok().as_deref() != Some("*\n") { fs::write(ignore, "*\n")?; }
    for ancestor in dir.ancestors().skip(1) {
        if !ancestor.join(".git").exists() { continue; }
        let already_ignored = Command::new("git").arg("-C").arg(ancestor).args(["check-ignore", "-q"]).arg(dir).status().is_ok_and(|s| s.success());
        if !already_ignored {
            let parent_ignore = ancestor.join(".gitignore");
            let relative = dir.strip_prefix(ancestor).unwrap();
            let pattern = format!("/{}/", relative.to_string_lossy());
            let existing = fs::read_to_string(&parent_ignore).unwrap_or_default();
            if !existing.lines().any(|line| line.trim() == pattern || line.trim() == ".blackbox/") {
                let mut file = OpenOptions::new().create(true).append(true).open(parent_ignore)?;
                if !existing.is_empty() && !existing.ends_with('\n') { writeln!(file)?; }
                writeln!(file, "{pattern}")?;
            }
        }
        let probe = dir.join("blackbox-check.jsonl");
        let ignored = Command::new("git").arg("-C").arg(ancestor).args(["check-ignore", "-q"]).arg(&probe).status()?;
        if !ignored.success() { return Err(io::Error::other("blackbox git check-ignore failed")); }
        break;
    }
    Ok(())
}
fn session_path(dir: &Path, id: &str) -> PathBuf { dir.join(".sessions").join(format!("{}.json", safe(id))) }
fn log_path(dir: &Path, id: &str) -> PathBuf { dir.join(format!("session-{}.jsonl", safe(id))) }
fn row_path(dir: &Path, row: &str) -> PathBuf { dir.join(format!("{}.jsonl", safe(row))) }
fn read_json(path: &Path) -> Value {
    fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Value::Null)
}
fn write_json(path: &Path, value: &Value) -> io::Result<()> {
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&tmp, serde_json::to_vec(value)?)?;
    fs::rename(tmp, path)
}
fn append(path: &Path, record: Value) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(format!("{record}\n").as_bytes())
}
fn logs_for(dir: &Path, meta: &Value, session: &str) -> Vec<PathBuf> {
    if let Some(active) = meta.get("rows").and_then(Value::as_array)
        .and_then(|rows| rows.iter().rev().filter_map(Value::as_str).map(|r| row_path(dir, r)).find(|p| completed_at(p).is_none())) {
        vec![active]
    } else { vec![log_path(dir, session)] }
}
fn event_for(dir: &Path, session: &str, record: Value) -> io::Result<()> {
    let meta = read_json(&session_path(dir, session));
    for path in logs_for(dir, &meta, session) { append(&path, record.clone())?; }
    Ok(())
}
fn proc_name(pid: u32) -> String {
    fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim().to_string()
}
fn parent(pid: u32) -> Option<u32> {
    fs::read_to_string(format!("/proc/{pid}/status")).ok()?.lines()
        .find_map(|line| line.strip_prefix("PPid:")?.trim().parse().ok())
}
fn ai_agent_kind(pid: u32) -> Option<&'static str> {
    match proc_name(pid).as_str() {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "gemini" => Some("gemini"),
        "agy" | "antigravity" => Some("agy"),
        "node" => {
            let cmd = String::from_utf8_lossy(&fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default()).to_ascii_lowercase();
            if cmd.contains("claude") { Some("claude") } else if cmd.contains("codex") { Some("codex") } else if cmd.contains("gemini") { Some("gemini") } else { None }
        }
        _ => None,
    }
}
fn agent_name(pid: u32) -> String { ai_agent_kind(pid).map(str::to_owned).unwrap_or_else(|| proc_name(pid)) }
fn ai_pid() -> u32 {
    let fallback = parent(std::process::id()).unwrap_or(std::process::id());
    let (mut pid, mut matched) = (fallback, None);
    for _ in 0..16 {
        if ai_agent_kind(pid).is_some() { matched = Some(pid); }
        let Some(next) = parent(pid).filter(|next| *next > 1 && *next != pid) else { break };
        pid = next;
    }
    matched.unwrap_or(fallback)
}
fn tty(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/fd/0")).ok().filter(|path| path.to_string_lossy().starts_with("/dev/"))
}
fn pid_alive(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/status")).ok().is_some_and(|status| !status.lines().any(|line| line.starts_with("State:") && line.contains("Z (zombie)")))
}
fn signal_pending(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/status")).ok().is_some_and(|status| {
        status.lines().filter(|line| line.starts_with("SigPnd:") || line.starts_with("ShdPnd:")).any(|line| {
            let mask = line.split_whitespace().nth(1).and_then(|value| u64::from_str_radix(value, 16).ok()).unwrap_or(0);
            [1_u32, 2, 15].iter().any(|signal| mask & (1_u64 << (signal - 1)) != 0)
        })
    })
}
fn pty_atime_ns(path: &Path) -> (u64, u64) {
    fs::metadata(path).map(|m| (m.atime().max(0) as u64, m.atime_nsec().max(0) as u64)).unwrap_or((0, 0))
}
fn oom_count() -> u64 {
    fs::read_to_string("/proc/vmstat").ok()
        .and_then(|s| s.lines().find_map(|l| l.strip_prefix("oom_kill ")?.trim().parse().ok()))
        .unwrap_or(0)
}
fn session_id(event: &Value) -> Option<String> {
    ["session_id", "sessionId", "conversationId"].iter()
        .find_map(|key| event.get(key).and_then(Value::as_str).filter(|value| !value.is_empty()).map(str::to_owned))
}
fn session_by_pid(dir: &Path, target_pid: u32) -> Option<String> {
    if target_pid <= 1 { return None; }
    let mut sessions: Vec<(u64, String)> = fs::read_dir(dir.join(".sessions")).ok()?
        .filter_map(Result::ok).filter_map(|entry| {
            let meta = read_json(&entry.path());
            let id = meta.get("session")?.as_str()?.to_string();
            let pid = meta.get("pid")?.as_u64()? as u32;
            (pid == target_pid && pid_alive(pid)).then(|| (meta.get("at").and_then(Value::as_u64).unwrap_or(0), id))
        }).collect();
    sessions.sort();
    sessions.pop().map(|(_, id)| id)
}
fn same_or_unowned(dir: &Path, id: &str, target_pid: u32) -> bool {
    let meta = read_json(&session_path(dir, id));
    let pid = meta.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
    pid == 0 || pid == target_pid || !pid_alive(pid)
}
fn pane_session(dir: &Path, target_pid: u32) -> Option<String> {
    let pane = std::env::var("TMUX_PANE").ok().filter(|p| !p.is_empty())?;
    let mut sessions: Vec<(u64, String)> = fs::read_dir(dir.join(".sessions")).ok()?
        .filter_map(Result::ok).filter_map(|entry| {
            let meta = read_json(&entry.path());
            let id = meta.get("session")?.as_str()?.to_string();
            let pid = meta.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
            (meta.get("tmux_pane").and_then(Value::as_str) == Some(pane.as_str()) && (pid == target_pid || !pid_alive(pid)))
                .then(|| (meta.get("at").and_then(Value::as_u64).unwrap_or(0), id))
        }).collect();
    sessions.sort();
    sessions.pop().map(|(_, id)| id)
}
fn claude_project_session(cwd: &str) -> Option<String> {
    let canonical = fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd));
    let path = home().join(".codex/tmp/task_complete/claude_project_session").join(format!("{}.json", safe(&canonical.to_string_lossy())));
    let pointer = read_json(&path);
    (pointer.get("cwd").and_then(Value::as_str) == Some(canonical.to_string_lossy().as_ref()))
        .then(|| pointer.get("session_id").and_then(Value::as_str).map(str::to_owned)).flatten()
}
fn resolve_session(dir: &Path, args: &[String], event: Option<&Value>) -> Option<String> {
    let explicit_pid = arg(args, "--pid").and_then(|v| v.parse().ok());
    let target_pid = explicit_pid.unwrap_or_else(ai_pid);
    arg(args, "--session")
        .or_else(|| event.and_then(session_id))
        .or_else(|| session_by_pid(dir, target_pid))
        .or_else(|| pane_session(dir, target_pid))
        .or_else(|| explicit_pid.is_none().then(|| std::env::var("CODEX_THREAD_ID").ok()).flatten())
        .or_else(|| explicit_pid.is_none().then(|| std::env::var("CLAUDE_CODE_SESSION_ID").ok()).flatten())
        .or_else(|| {
            arg(args, "--cwd").or_else(|| event.and_then(|e| e.get("cwd").and_then(Value::as_str).map(str::to_owned)))
                .and_then(|cwd| claude_project_session(&cwd))
                .filter(|id| same_or_unowned(dir, id, target_pid))
        })
}
fn is_blackbox_daemon(dpid: u32) -> bool {
    if dpid <= 1 || !pid_alive(dpid) || !proc_name(dpid).starts_with("cli_watch_black") { return false; }
    let cmd = fs::read(format!("/proc/{dpid}/cmdline")).unwrap_or_default();
    let parts: Vec<String> = cmd.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned()).collect();
    parts.first().is_some_and(|exe| exe.ends_with("cli_watch_blackbox")) && parts.get(1).map(String::as_str) == Some("daemon")
}
fn kill_daemon_pid(dpid: u32) {
    if !is_blackbox_daemon(dpid) { return; }
    let _ = Command::new("/bin/kill").args(["-TERM", &dpid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    for _ in 0..15 {
        if !pid_alive(dpid) { return; }
        std::thread::sleep(Duration::from_millis(10));
    }
    if is_blackbox_daemon(dpid) {
        let _ = Command::new("/bin/kill").args(["-KILL", &dpid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}
fn reap_stale_daemons(dir: &Path, keep_session: &str, current_pid: u32) {
    if let Ok(entries) = fs::read_dir(dir.join(".sessions")) {
        for entry in entries.filter_map(Result::ok) {
            let mut meta = read_json(&entry.path());
            let Some(sess) = meta.get("session").and_then(Value::as_str).map(str::to_owned) else { continue };
            let pid = meta.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
            let dpid = meta.get("daemon_pid").and_then(Value::as_u64).unwrap_or(0) as u32;
            if dpid > 0 && (!pid_alive(pid) || (pid == current_pid && sess != keep_session)) {
                kill_daemon_pid(dpid);
                meta["daemon_pid"] = json!(0);
                let _ = write_json(&entry.path(), &meta);
            }
        }
    }
    if std::env::var_os("CLI_WATCH_BLACKBOX_DIR").is_some() { return; }
    let Ok(procs) = fs::read_dir("/proc") else { return };
    for proc_entry in procs.filter_map(Result::ok) {
        let Ok(dpid) = proc_entry.file_name().to_string_lossy().parse::<u32>() else { continue };
        if dpid == std::process::id() || !is_blackbox_daemon(dpid) { continue; }
        let raw = fs::read(format!("/proc/{dpid}/cmdline")).unwrap_or_default();
        let parts: Vec<String> = raw.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned()).collect();
        let Some(sess) = arg(&parts, "--session") else { kill_daemon_pid(dpid); continue };
        let meta = read_json(&session_path(dir, &sess));
        let registered = meta.get("daemon_pid").and_then(Value::as_u64).unwrap_or(0) as u32;
        let owner_pid = meta.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
        if registered != dpid || !pid_alive(owner_pid) {
            kill_daemon_pid(dpid);
        }
    }
}
fn ensure_daemon(dir: &Path, session: &str, meta: &mut Value) -> io::Result<()> {
    let pid = meta.get("pid").and_then(Value::as_u64).unwrap_or_else(|| ai_pid() as u64) as u32;
    let row = meta.get("rows").and_then(Value::as_array).and_then(|rows| rows.last()).and_then(Value::as_str).map(str::to_owned);
    reap_stale_daemons(dir, session, pid);
    let dpid = meta.get("daemon_pid").and_then(Value::as_u64).unwrap_or(0) as u32;
    let drow = meta.get("daemon_row").and_then(Value::as_str);
    if is_blackbox_daemon(dpid) && drow == row.as_deref() {
        return write_json(&session_path(dir, session), meta);
    }
    if dpid > 0 { kill_daemon_pid(dpid); }
    meta["daemon_pid"] = json!(0);
    meta["daemon_row"] = match &row { Some(r) => json!(r), None => Value::Null };
    if std::env::var_os("CLI_WATCH_BLACKBOX_TEST_NO_DAEMON").is_none() && pid_alive(pid) {
        let executable = std::env::current_exe()?;
        let mut cmd = Command::new(executable);
        cmd.args(["daemon", "--session", session, "--pid", &pid.to_string()]);
        if let Some(r) = &row { cmd.args(["--row", r]); }
        let child = cmd.process_group(0).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        meta["daemon_pid"] = json!(child.id());
    }
    write_json(&session_path(dir, session), meta)
}
fn last_records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path).unwrap_or_default().lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}
fn completed_at(path: &Path) -> Option<u64> {
    for record in last_records(path).iter().rev() {
        if let Some(at) = record.get("completed_at").and_then(Value::as_u64) { return Some(at); }
        if matches!(record.get("event").and_then(Value::as_str), Some("session_start" | "row_bound")) { return None; }
    }
    None
}
fn completed(dir: &Path, path: &Path, kind: &str, at: u64) -> io::Result<()> {
    initialize(dir)?;
    if completed_at(path).is_none() {
        append(path, json!({"at":at,"event":"completed","completed_at":at,"kind":kind}))?;
    }
    Ok(())
}
fn ensure_session_meta(dir: &Path, session: &str, pid: u32, cwd: &str) -> Value {
    let path = session_path(dir, session);
    let mut meta = read_json(&path);
    let agent = agent_name(pid);
    let cur_tty = tty(pid).map(|p| p.to_string_lossy().to_string());
    let cur_pane = std::env::var("TMUX_PANE").ok().filter(|p| !p.is_empty());
    if meta.is_null() {
        meta = json!({"session":session,"pid":pid,"agent":agent,"daemon_pid":0,"daemon_row":null,"cwd":cwd,"tty":cur_tty,"tmux_pane":cur_pane,"rows":[],"at":now()});
    } else {
        meta["pid"] = json!(pid);
        if ai_agent_kind(pid).is_some() || meta.get("agent").and_then(Value::as_str).unwrap_or("").is_empty() {
            meta["agent"] = json!(agent);
        }
        if !cwd.is_empty() && meta.get("cwd").and_then(Value::as_str).unwrap_or("").is_empty() { meta["cwd"] = json!(cwd); }
        if let Some(t) = cur_tty { meta["tty"] = json!(t); }
        if let Some(p) = cur_pane.filter(|_| meta.get("tmux_pane").and_then(Value::as_str).unwrap_or("").is_empty()) { meta["tmux_pane"] = json!(p); }
    }
    meta
}
fn hook_start(dir: &Path, args: &[String], event: &Value) -> io::Result<()> {
    if !main_session() { return Ok(()); }
    initialize(dir)?;
    let _ = gc(dir, now());
    let pid = arg(args, "--pid").and_then(|v| v.parse().ok()).unwrap_or_else(ai_pid);
    let session = session_id(event).or_else(|| arg(args, "--session")).unwrap_or_else(|| format!("pid-{pid}"));
    let cwd = event.get("cwd").and_then(Value::as_str).unwrap_or("");
    let mut meta = ensure_session_meta(dir, &session, pid, cwd);
    write_json(&session_path(dir, &session), &meta)?;
    event_for(dir, &session, json!({"at":now(),"event":"session_start","pid":pid,"session":session}))?;
    ensure_daemon(dir, &session, &mut meta)
}
fn bind_row(dir: &Path, args: &[String]) -> io::Result<()> {
    if !main_session() { return Ok(()); }
    initialize(dir)?;
    let row = arg(args, "--row").ok_or_else(|| io::Error::other("missing --row"))?;
    if !row.bytes().all(|byte| byte.is_ascii_digit()) { return Err(io::Error::other("invalid row")); }
    let pid = arg(args, "--pid").and_then(|v| v.parse().ok()).unwrap_or_else(ai_pid);
    let cwd = arg(args, "--cwd").unwrap_or_default();
    let session = resolve_session(dir, args, None).unwrap_or_else(|| format!("pid-{pid}"));
    let target = row_path(dir, &row);
    let source = log_path(dir, &session);
    if source.exists() {
        let mut output = OpenOptions::new().create(true).append(true).open(&target)?;
        output.write_all(&fs::read(&source)?)?;
        fs::remove_file(source)?;
    }
    let mut meta = ensure_session_meta(dir, &session, pid, &cwd);
    if !meta["rows"].is_array() { meta["rows"] = json!([]); }
    let rows = meta["rows"].as_array_mut().unwrap();
    rows.retain(|value| value.as_str() != Some(&row));
    rows.push(json!(row));
    write_json(&session_path(dir, &session), &meta)?;
    append(&target, json!({"at":now(),"event":"row_bound","row":row,"session":session}))?;
    ensure_daemon(dir, &session, &mut meta)
}
fn mark(dir: &Path, args: &[String]) -> io::Result<()> {
    initialize(dir)?;
    let cause = arg(args, "--cause").ok_or_else(|| io::Error::other("missing --cause"))?;
    if !["user_keyboard", "other_session", "hook_stop", "watchdog_timeout"].contains(&cause.as_str()) { return Err(io::Error::other("invalid cause")); }
    let actor = arg(args, "--actor").unwrap_or_default();
    let detail = arg(args, "--detail").unwrap_or_default();
    let record = json!({"at":now(),"event":"mark","cause":cause,"actor":actor,"detail":detail.chars().take(400).collect::<String>()});
    if let Some(row) = arg(args, "--row") {
        append(&row_path(dir, &row), record)
    } else if let Some(session) = arg(args, "--session") {
        event_for(dir, &session, record)
    } else {
        let target_pids: Vec<u32> = detail.split_whitespace().filter_map(|part| part.parse::<u32>().ok()).filter(|pid| *pid > 1).collect();
        let target_panes: Vec<&str> = detail.split_whitespace().filter(|part| part.starts_with('%') && part[1..].bytes().all(|byte| byte.is_ascii_digit())).collect();
        for entry in fs::read_dir(dir.join(".sessions"))? {
            let entry = entry?;
            let meta = read_json(&entry.path());
            let Some(session) = meta.get("session").and_then(Value::as_str) else { continue };
            let pid = meta.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
            if !pid_alive(pid) || (cause == "other_session" && session == actor) { continue; }
            let pane = meta.get("tmux_pane").and_then(Value::as_str).unwrap_or("");
            if (!target_pids.is_empty() || !target_panes.is_empty())
                && !target_pids.contains(&pid) && !target_panes.contains(&pane) { continue; }
            event_for(dir, session, record.clone())?;
        }
        Ok(())
    }
}
fn hook_stop(dir: &Path, event: &Value) -> io::Result<()> {
    if !main_session() { return Ok(()); }
    initialize(dir)?;
    let Some(session) = resolve_session(dir, &[], Some(event)) else { return Ok(()) };
    let canceled = event.get("terminationReason").and_then(Value::as_str) == Some("USER_CANCELED")
        || event.get("interrupted").and_then(Value::as_bool) == Some(true)
        || event.get("cancelled").and_then(Value::as_bool) == Some(true);
    if canceled {
        event_for(dir, &session, json!({"at":now(),"event":"interrupted","cause":"user_keyboard"}))?;
    } else {
        event_for(dir, &session, json!({"at":now(),"event":"turn_stop"}))?;
        let meta = read_json(&session_path(dir, &session));
        if meta.get("rows").and_then(Value::as_array).is_none_or(Vec::is_empty) {
            completed(dir, &log_path(dir, &session), "question_session", now())?;
        }
    }
    Ok(())
}
fn active_child_cmd(pid: u32) -> String {
    let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else { return String::new() };
    let mut best = (0_u32, String::new());
    for entry in tasks.filter_map(Result::ok) {
        let text = fs::read_to_string(entry.path().join("children")).unwrap_or_default();
        for cpid in text.split_whitespace().filter_map(|s| s.parse::<u32>().ok()) {
            if is_blackbox_daemon(cpid) { continue; }
            let raw = fs::read(format!("/proc/{cpid}/cmdline")).unwrap_or_default();
            let cmd = raw.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned()).collect::<Vec<_>>().join(" ");
            if !cmd.is_empty() && !cmd.contains("-mcp") && cpid > best.0 { best = (cpid, cmd.chars().take(200).collect()); }
        }
    }
    best.1
}
fn resume_cmd(agent: &str, session: &str, cwd: &str) -> String {
    let prefix = if cwd.is_empty() { String::new() } else { format!("cd {cwd} && ") };
    match agent {
        "claude" => format!("{prefix}claude --resume {session}"),
        "codex" => format!("{prefix}codex resume {session}"),
        "gemini" => format!("{prefix}gemini --resume {session}"),
        "agy" => format!("{prefix}agy (conversation: {session})"),
        _ => format!("{prefix}# session={session}"),
    }
}
struct ExitInfo<'a> { pid: u32, signal: bool, last_input: u64, oom: bool, graceful: bool, child_cmd: &'a str }
fn record_session_exit(dir: &Path, session: &str, meta: &Value, info: ExitInfo<'_>) -> io::Result<()> {
    let agent = meta.get("agent").and_then(Value::as_str).unwrap_or("");
    let cwd = meta.get("cwd").and_then(Value::as_str).unwrap_or("");
    let pane = meta.get("tmux_pane").and_then(Value::as_str).unwrap_or("");
    let resume = resume_cmd(agent, session, cwd);
    for path in logs_for(dir, meta, session) {
        let records = last_records(&path);
        if records.last().and_then(|r| r.get("event")).and_then(Value::as_str) == Some("session_exit") { continue; }
        let external = records.iter().rev().find(|record| {
            record.get("event").and_then(Value::as_str) == Some("mark")
                && record.get("at").and_then(Value::as_u64).is_some_and(|time| now().saturating_sub(time) <= RECENT_MARK)
        });
        let recent_turn = records.iter().rev().find_map(|r| r.get("event")?.as_str().filter(|e| matches!(*e, "interrupted" | "turn_stop" | "turn_start" | "row_bound" | "session_start")));
        let cause = if info.oom { "oom_killed" }
            else if let Some(ext) = external.and_then(|r| r.get("cause").and_then(Value::as_str)) { ext }
            else if recent_turn == Some("interrupted") || info.signal || (info.last_input > 0 && now().saturating_sub(info.last_input) <= 5) { "user_keyboard" }
            else if info.graceful || recent_turn == Some("turn_stop") { "normal_exit" }
            else { "unexpected_kill" };
        if cause == "normal_exit" && meta.get("rows").and_then(Value::as_array).is_none_or(Vec::is_empty) {
            completed(dir, &path, "question_session", now())?;
        }
        append(&path, json!({"at":now(),"event":"session_exit","cause":cause,"pid":info.pid,"agent":agent,"cwd":cwd,"tmux_pane":pane,"interrupted_cmd":info.child_cmd,"resume_cmd":resume,"actor":external.and_then(|v|v.get("actor")).cloned(),"detail":external.and_then(|v|v.get("detail")).cloned()}))?;
    }
    Ok(())
}
fn hook_session_end(dir: &Path, args: &[String], event: &Value) -> io::Result<()> {
    if !main_session() { return Ok(()); }
    initialize(dir)?;
    let Some(session) = resolve_session(dir, args, Some(event)) else { return Ok(()) };
    let path = session_path(dir, &session);
    let mut meta = read_json(&path);
    if meta.is_null() { return Ok(()); }
    let dpid = meta.get("daemon_pid").and_then(Value::as_u64).unwrap_or(0) as u32;
    let pid = meta.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
    if dpid > 0 { kill_daemon_pid(dpid); }
    meta["daemon_pid"] = json!(0);
    write_json(&path, &meta)?;
    record_session_exit(dir, &session, &meta, ExitInfo { pid, signal: false, last_input: 0, oom: false, graceful: true, child_cmd: "" })
}
fn prompt_text<'a>(event: &'a Value) -> &'a str {
    ["prompt", "user_prompt", "lastUserInput", "query"].iter()
        .find_map(|key| event.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()))
        .unwrap_or("")
}
fn asks_stop_cause(prompt: &str) -> bool {
    if prompt.contains("blackbox") { return true; }
    let stop_word = ["중지", "정지", "멈췄", "멈춘", "끊겼", "종료됐", "꺼졌", "닫혔", "쳐닫", "죽었", "튕겼", "interrupted", "취소"]
        .iter().any(|word| prompt.contains(word));
    let why_word = ["왜", "원인", "이유", "누가", "어떻게", "무슨 입력", "어떤 입력"]
        .iter().any(|word| prompt.contains(word));
    stop_word && why_word
}
fn latest_log_key(dir: &Path) -> Option<String> {
    let mut files: Vec<(SystemTime, String)> = fs::read_dir(dir).ok()?
        .filter_map(Result::ok).filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") { return None; }
            let stem = path.file_stem()?.to_string_lossy().to_string();
            let key = stem.strip_prefix("session-").map(str::to_owned).unwrap_or(stem);
            Some((entry.metadata().ok()?.modified().ok()?, key))
        }).collect();
    files.sort_by_key(|(time, _)| *time);
    files.pop().map(|(_, key)| key)
}
fn hook_prompt(dir: &Path, args: &[String], event: &Value) -> io::Result<()> {
    if !main_session() { println!("{{}}"); return Ok(()); }
    initialize(dir)?;
    let pid = arg(args, "--pid").and_then(|v| v.parse().ok()).unwrap_or_else(ai_pid);
    let cwd = event.get("cwd").and_then(Value::as_str).unwrap_or("");
    let session = resolve_session(dir, args, Some(event)).unwrap_or_else(|| format!("pid-{pid}"));
    let mut meta = ensure_session_meta(dir, &session, pid, cwd);
    ensure_daemon(dir, &session, &mut meta)?;
    let prompt = prompt_text(event);
    let in_turn = logs_for(dir, &meta, &session).first()
        .and_then(|p| last_records(p).last().and_then(|r| r.get("event").and_then(Value::as_str).map(str::to_owned)))
        .as_deref() == Some("turn_start");
    if in_turn { println!("{{}}"); return Ok(()); }
    if !asks_stop_cause(prompt) {
        if !prompt.is_empty() { event_for(dir, &session, json!({"at":now(),"event":"turn_start"}))?; }
        println!("{{}}");
        return Ok(());
    }
    let explicit_row = prompt.split(|c: char| !c.is_ascii_digit()).filter(|part| !part.is_empty() && part.len() <= 8)
        .find(|num| row_path(dir, num).is_file()).map(str::to_owned);
    let fallback_row = meta.get("rows").and_then(Value::as_array).and_then(|r| r.last()).and_then(Value::as_str)
        .filter(|num| row_path(dir, num).is_file()).map(str::to_owned);
    let Some(key) = explicit_row.or(fallback_row).or_else(|| latest_log_key(dir)) else { println!("{{}}"); return Ok(()) };
    let Ok(report) = query_text(dir, &key) else { println!("{{}}"); return Ok(()) };
    let _ = event_for(dir, &session, json!({"at":now(),"event":"turn_start"}));
    let out = json!({"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":format!("[cli_watch_blackbox 자동 조회 ({key})]\n{report}")}});
    println!("{out}");
    Ok(())
}
fn complete(dir: &Path, args: &[String]) -> io::Result<()> {
    if let Some(row) = arg(args, "--row") {
        completed(dir, &row_path(dir, &row), "row", arg(args, "--now").and_then(|v| v.parse().ok()).unwrap_or_else(now))
    } else if let Some(session) = resolve_session(dir, args, None) {
        completed(dir, &log_path(dir, &session), "question_session", arg(args, "--now").and_then(|v| v.parse().ok()).unwrap_or_else(now))
    } else { Err(io::Error::other("missing row or session")) }
}
fn gc(dir: &Path, at: u64) -> io::Result<u64> {
    initialize(dir)?;
    reap_stale_daemons(dir, "", 0);
    let mut removed = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().extension().is_none_or(|ext| ext != "jsonl") { continue; }
        if completed_at(&entry.path()).is_some_and(|completed| at.saturating_sub(completed) >= DAY) {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    for entry in fs::read_dir(dir.join(".sessions"))? {
        let entry = entry?;
        let meta = read_json(&entry.path());
        let pid = meta.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
        let created = meta.get("at").and_then(Value::as_u64).unwrap_or(at);
        if !pid_alive(pid) && at.saturating_sub(created) >= DAY { fs::remove_file(entry.path())?; }
    }
    Ok(removed)
}
fn daemon(dir: &Path, args: &[String]) -> io::Result<()> {
    let session = arg(args, "--session").ok_or_else(|| io::Error::other("missing --session"))?;
    let pid = arg(args, "--pid").and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| io::Error::other("missing --pid"))?;
    let poll = arg(args, "--poll-ms").and_then(|s| s.parse::<u64>().ok()).unwrap_or(200).max(10);
    let self_pid = std::process::id();
    let meta = read_json(&session_path(dir, &session));
    let tty = meta.get("tty").and_then(Value::as_str).map(PathBuf::from);
    let base_atime = tty.as_deref().map(pty_atime_ns).unwrap_or((0, 0));
    let base_oom = oom_count();
    let (mut signal, mut last_input, mut polls, mut last_child) = (false, 0_u64, 0_u64, String::new());
    while pid_alive(pid) {
        if polls % 5 == 0 {
            let live_meta = read_json(&session_path(dir, &session));
            let active_dpid = live_meta.get("daemon_pid").and_then(Value::as_u64).unwrap_or(0) as u32;
            if active_dpid != 0 && active_dpid != self_pid { return Ok(()); }
        }
        signal |= signal_pending(pid);
        if let Some(path) = &tty {
            let cur = pty_atime_ns(path);
            if cur > base_atime { last_input = last_input.max(cur.0); }
        }
        let child = active_child_cmd(pid);
        if !child.is_empty() { last_child = child; }
        polls += 1;
        std::thread::sleep(Duration::from_millis(poll));
    }
    let path = session_path(dir, &session);
    let mut meta = read_json(&path);
    record_session_exit(dir, &session, &meta, ExitInfo { pid, signal, last_input, oom: oom_count() > base_oom, graceful: false, child_cmd: &last_child })?;
    if meta.get("daemon_pid").and_then(Value::as_u64) == Some(self_pid as u64) {
        meta["daemon_pid"] = json!(0);
        let _ = write_json(&path, &meta);
    }
    Ok(())
}
fn query_text(dir: &Path, key: &str) -> io::Result<String> {
    let path = if key.bytes().all(|byte| byte.is_ascii_digit()) { row_path(dir, key) } else { log_path(dir, key) };
    let records = last_records(&path);
    if records.is_empty() { return Err(io::Error::new(io::ErrorKind::NotFound, "log not found")); }
    let reason = records.iter().rev().find(|record| {
        matches!(record.get("event").and_then(Value::as_str), Some("session_exit" | "interrupted" | "mark"))
    }).or_else(|| records.iter().rev().find(|record| record.get("event").and_then(Value::as_str) == Some("completed"))).unwrap_or(records.last().unwrap());
    let cause = reason.get("cause").and_then(Value::as_str).unwrap_or(
        if reason.get("event").and_then(Value::as_str) == Some("completed") { "normal_complete" } else { "unknown" }
    );
    let mut lines = vec![format!("cause={cause} at={} actor={} detail={}", reason.get("at").and_then(Value::as_u64).unwrap_or(0), reason.get("actor").and_then(Value::as_str).unwrap_or(""), reason.get("detail").and_then(Value::as_str).unwrap_or(""))];
    if let Some(resume) = reason.get("resume_cmd").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        lines.push(format!("recovery={resume} pane={} interrupted_cmd={}", reason.get("tmux_pane").and_then(Value::as_str).unwrap_or(""), reason.get("interrupted_cmd").and_then(Value::as_str).unwrap_or("")));
    }
    for record in records.iter().rev().take(5).rev() { lines.push(record.to_string()); }
    Ok(lines.join("\n"))
}
fn query(dir: &Path, key: &str) -> io::Result<()> {
    println!("{}", query_text(dir, key)?);
    Ok(())
}
fn run(args: &[String], input: &str) -> io::Result<()> {
    let dir = root();
    match args.first().map(String::as_str).unwrap_or("") {
        "hook-session-start" => hook_start(&dir, args, &serde_json::from_str(input).unwrap_or(Value::Null)),
        "hook-session-end" => hook_session_end(&dir, args, &serde_json::from_str(input).unwrap_or(Value::Null)),
        "hook-prompt" => hook_prompt(&dir, args, &serde_json::from_str(input).unwrap_or(Value::Null)),
        "hook-stop" => hook_stop(&dir, &serde_json::from_str(input).unwrap_or(Value::Null)),
        "bind-row" => bind_row(&dir, args),
        "mark" => mark(&dir, args),
        "complete" => complete(&dir, args),
        "gc" => { println!("{}", gc(&dir, arg(args, "--now").and_then(|v| v.parse().ok()).unwrap_or_else(now))?); Ok(()) }
        "query" => query(&dir, args.get(1).ok_or_else(|| io::Error::other("missing row or session"))?),
        "daemon" => daemon(&dir, args),
        _ => Err(io::Error::other("usage: cli_watch_blackbox hook-session-start|hook-session-end|hook-prompt|hook-stop|bind-row|mark|complete|gc|query|daemon")),
    }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut input = String::new();
    if matches!(args.first().map(String::as_str), Some("hook-session-start" | "hook-session-end" | "hook-prompt" | "hook-stop")) {
        let _ = std::io::stdin().read_to_string(&mut input);
    }
    if let Err(error) = run(&args, &input) { eprintln!("cli_watch_blackbox: {error}"); std::process::exit(1); }
}
