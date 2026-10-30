//! Runs one Table26 row's worker (headless `codex exec` or `gemini -p`) in a new tmux pane.
//! The orchestrator reads `.ai/dispatch/<row>.{log,exit,last}`, never the pane text.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::Duration;

const USAGE: &str = "usage: dispatch-agent [--resume] [--agent codex|gemini] [--role agent|verifier] [--project DIR] --row ID [--rows ID,...] --prompt-file F [--add-dir DIR ...]";

struct Args {
    resume: bool,
    agent: String,
    role: String,
    project: PathBuf,
    row: u64,
    rows: Option<Vec<u64>>,
    prompt_file: PathBuf,
    /// Extra writable roots outside the project (codex `--add-dir`), e.g. `~/ai/scripts` for a workflow row.
    add_dirs: Vec<PathBuf>,
}

struct Paths {
    dir: PathBuf,
    prompt: PathBuf,
    script: PathBuf,
    log: PathBuf,
    exit: PathBuf,
    last: PathBuf,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let get = |flag: &str| -> Result<String, String> {
        let at = argv.iter().position(|a| a == flag).ok_or(format!("missing {flag}\n{USAGE}"))?;
        argv.get(at + 1).cloned().ok_or(format!("{flag} needs a value\n{USAGE}"))
    };
    let resume = argv.iter().any(|arg| arg == "--resume");
    let agent = if resume && !argv.iter().any(|arg| arg == "--agent") { "codex".to_string() } else { get("--agent")? };
    if agent != "codex" && agent != "gemini" {
        return Err(format!("--agent must be codex or gemini\n{USAGE}"));
    }
    if resume && agent != "codex" {
        return Err("--resume supports codex only".into());
    }
    let role = if argv.iter().any(|arg| arg == "--role") { get("--role")? } else { "agent".into() };
    if role != "agent" && role != "verifier" { return Err(format!("--role must be agent or verifier\n{USAGE}")); }
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
    let prompt_file = PathBuf::from(get("--prompt-file")?);
    let mut add_dirs = Vec::new();
    for (at, arg) in argv.iter().enumerate() {
        if arg == "--add-dir" {
            let dir = argv.get(at + 1).ok_or(format!("--add-dir needs a value\n{USAGE}"))?;
            add_dirs.push(fs::canonicalize(dir).map_err(|e| format!("--add-dir {dir}: {e}"))?);
        }
    }
    Ok(Args { resume, agent, role, project, row, rows, prompt_file, add_dirs })
}

fn paths(project: &Path, row: u64, role: &str) -> Paths {
    let dir = project.join(".ai").join("dispatch");
    let suffix = if role == "verifier" { ".verify" } else { "" };
    let file = |ext: &str| dir.join(format!("{row}{suffix}.{ext}"));
    Paths { prompt: file("prompt"), script: file("sh"), log: file("log"), exit: file("exit"), last: file("last"), dir }
}

fn role_header(sender: Option<&str>, role: &str, row: u64, rows: Option<&[u64]>) -> String {
    let from = match sender { Some("worker") => "대행자", Some("verifier") => "검증자", _ => "위임자(main)" };
    let (to, skill) = if role == "verifier" { ("검증자", "role-verifier") } else { ("대행자", "role-agent") };
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
    let output = command.output().map_err(|error| {
        let message = error.to_string();
        tmux_log(operation, "not_observed", &message);
        format!("tmux {operation} failed: {message}")
    })?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tmux_log(operation, "rejected", &message);
        return Err(format!("tmux {operation} failed: {message}"));
    }
    tmux_log(operation, "received", "none");
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn tmux_log(operation: &str, ack: &str, error: &str) {
    eprintln!("{}", tmux_log_line(operation, ack, error));
}

fn tmux_log_line(operation: &str, ack: &str, error: &str) -> String {
    let error = error.replace(['\r', '\n', '|'], " ");
    let prefix = format!("00119803531 | tmux.{operation} | 01 | ack={ack} body=none error=");
    let available = 120usize.saturating_sub(prefix.len());
    format!("{prefix}{}", error.chars().take(available).collect::<String>())
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

fn with_window_lock<T>(socket: &str, window: &str, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let path = lock_path(socket, window);
    let file: File = OpenOptions::new().read(true).write(true).create(true).open(&path)
        .map_err(|error| lock_error("acquire", &error))?;
    file.lock().map_err(|error| lock_error("acquire", &error))?;
    let result = action();
    file.unlock().map_err(|error| lock_error("release", &error))?;
    result
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
            "codex exec -C {} -s workspace-write -c sandbox_workspace_write.network_access=true{} -o {} - < {}",
            quote(project),
            add_dirs.iter().map(|d| format!(" --add-dir {}", quote(d))).collect::<String>(),
            quote(&p.last), quote(&p.prompt)
        ),
        _ => format!("gemini --approval-mode auto_edit -p \"$(cat {})\"", quote(&p.prompt)),
    }
}

/// Bash runner for the pane: the caller's PATH (a tmux pane starts from the server's environment), worker env,
/// full output to the log, exit code written last and atomically.
fn runner_script(agent: &str, role: &str, project: &Path, add_dirs: &[PathBuf], resume_session: Option<&str>, row: u64, rows: Option<&[u64]>, path_env: &str, p: &Paths) -> String {
    let tee = if resume_session.is_some() { "tee -a" } else { "tee" };
    let runtime_role = if role == "verifier" { "verifier" } else { "worker" };
    let rows_env = rows.map(|ids| format!(" T26_ROWS={}", ids.iter().map(u64::to_string).collect::<Vec<_>>().join(","))).unwrap_or_default();
    format!(
        "#!/bin/bash\ncd {project} || exit 1\nrm -f {exit}\nexport PATH={path} T26_ROW={row} T26_ROLE={runtime_role}{rows_env}\n{cmd} 2>&1 | {tee} {log}\necho \"${{PIPESTATUS[0]}}\" > {exit}.tmp && mv {exit}.tmp {exit}\n",
        project = quote(project),
        path = quote(Path::new(path_env)),
        cmd = agent_command(agent, project, add_dirs, resume_session, p),
        tee = tee,
        log = quote(&p.log),
        exit = quote(&p.exit),
    )
}

/// Last non-empty line of the agent's final message (codex `-o`) or of the log (gemini).
fn last_line(text: &str) -> String {
    let line = text.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    line.chars().take(300).collect()
}

fn result_line(code: &str, p: &Paths, last: &str) -> String {
    format!("exit={} log={} last={}", code.trim(), p.log.display(), last)
}

fn verifier_failure_row(last: &str, rows: &[u64]) -> Option<(u64, String)> {
    let rest = last.strip_prefix("실패 — ")?;
    let (id, detail) = rest.split_once(':')?;
    if detail.trim().is_empty() { return None; }
    let id = id.parse::<u64>().ok()?;
    rows.contains(&id).then(|| (id, last.chars().take(120).collect()))
}

fn verifier_fail_log(row: u64, ack: &str, error: &str) {
    let safe = error.replace(['\r', '\n', '|'], " ");
    eprintln!("00119805311 | dispatch-agent.verifier-fail | 01 | ack={ack} body.row={row} error={}",
        safe.chars().take(100).collect::<String>());
}

fn record_verifier_failure(project: &Path, row: u64, line: &str) -> bool {
    let fields = serde_json::json!({"H":"진단", "I":line}).to_string();
    let output = grist_output(&["task-update", &row.to_string(), "--project", &project.to_string_lossy(), "--fields", &fields]);
    match output {
        Ok(output) if output.status.success() => { verifier_fail_log(row, "received", "none"); true }
        Ok(_) => { verifier_fail_log(row, "rejected", "grist task-update failed"); false }
        Err(error) => { verifier_fail_log(row, "not_observed", &error.to_string()); false }
    }
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

fn check_row_owner(project: &Path, row: u64) -> Result<(), String> {
    let output = grist_output(&["task-show", &row.to_string(), "--project", &project.to_string_lossy(), "--section", "row"]);
    row_owner_result(row, output)
}

fn run(args: Args) -> Result<i32, String> {
    check_row_owner(&args.project, args.row)?;
    if let Some(rows) = &args.rows {
        for &row in rows { check_row_owner(&args.project, row)?; }
    }
    let tmux_socket = std::env::var("TMUX").ok()
        .and_then(|value| value.split(',').next().filter(|socket| !socket.is_empty()).map(str::to_owned))
        .ok_or("dispatch-agent needs a tmux session (TMUX is not set)")?;
    let caller_pane = std::env::var("TMUX_PANE")
        .ok().filter(|pane| !pane.is_empty())
        .ok_or("dispatch-agent needs a tmux pane (TMUX_PANE is not set)")?;
    let window = tmux_call("display-message", Command::new("tmux")
        .args(["display-message", "-p", "-t", &caller_pane, "#{window_id}"]))?;
    let p = paths(&args.project, args.row, &args.role);
    fs::create_dir_all(&p.dir).map_err(|e| format!("{}: {e}", p.dir.display()))?;
    let session = if args.resume {
        let log = fs::read_to_string(&p.log).map_err(|e| format!("{}: {e}", p.log.display()))?;
        Some(required_session_id(&log, &p.log)?.to_string())
    } else {
        None
    };
    let original = fs::read(&args.prompt_file).map_err(|e| format!("--prompt-file: {e}"))?;
    let mut prompt = role_header(std::env::var("T26_ROLE").ok().as_deref(), &args.role, args.row, args.rows.as_deref()).into_bytes();
    prompt.extend(role_doc_line(&args.project, &args.role).as_bytes());
    prompt.extend(verifier_runtime_line(&args.project, &args.role).as_bytes());
    prompt.extend(task_context(args.row, args.rows.as_deref(), &args.project)?.as_bytes());
    if let Ok(schema) = fs::read_to_string(schema_file()) {
        prompt.extend(schema_block(&String::from_utf8_lossy(&original), &schema).as_bytes());
    }
    prompt.extend(original);
    fs::write(&p.prompt, prompt).map_err(|e| format!("{}: {e}", p.prompt.display()))?;
    let stale_paths = [&p.exit, &p.last, &p.log];
    for stale in stale_paths.into_iter().take(if args.resume { 2 } else { 3 }) {
        let _ = fs::remove_file(stale);
    }
    let path_env = std::env::var("PATH").unwrap_or_default();
    fs::write(&p.script, runner_script(&args.agent, &args.role, &args.project, &args.add_dirs, session.as_deref(), args.row, args.rows.as_deref(), &path_env, &p)).map_err(|e| e.to_string())?;
    let pane = with_window_lock(&tmux_socket, &window, || {
        let panes = tmux_call("list-panes", Command::new("tmux")
            .args(["list-panes", "-t", &window, "-F", "#{pane_id}\t#{@dispatch-agent}"]))?;
        let pane = tmux_call("split-window", Command::new("tmux")
            .args(split_args(&caller_pane, last_agent_pane(&panes)))
            .arg(&args.project)
            .arg("bash")
            .arg(&p.script))?;
        tmux_call("set-option", Command::new("tmux")
            .args(["set-option", "-p", "-t", &pane, "@dispatch-agent", &args.row.to_string()]))?;
        Ok(pane)
    })?;
    eprintln!("dispatch-agent: row {} {} in pane {}", args.row, args.agent, pane);
    let code = loop {
        if let Ok(code) = fs::read_to_string(&p.exit) {
            break code;
        }
        sleep(Duration::from_secs(2));
    };
    let source = if args.agent == "codex" { &p.last } else { &p.log };
    let last = last_line(&fs::read_to_string(source).unwrap_or_default());
    let recorded = if args.role == "verifier" {
        let ids = args.rows.as_deref().unwrap_or(std::slice::from_ref(&args.row));
        verifier_failure_row(&last, ids).and_then(|(row, line)|
            record_verifier_failure(&args.project, row, &line).then_some(row))
    } else { None };
    let suffix = recorded.map(|row| format!(" recorded=I:{row}")).unwrap_or_default();
    println!("{}{suffix}", result_line(&code, &p, &last));
    Ok(code.trim().parse().unwrap_or(1))
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match parse_args(&argv).and_then(run) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("dispatch-agent: {message}");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ROLE_DOC_TEST_ID: AtomicUsize = AtomicUsize::new(0);

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
        let line = tmux_log_line("split-window", "rejected", "can't find pane: %12\nretry");
        assert_eq!(line, "00119803531 | tmux.split-window | 01 | ack=rejected body=none error=can't find pane: %12 retry");
    }

    #[test]
    fn agent_commands_are_headless_and_scoped_to_the_project() {
        let p = sample();
        assert_eq!(
            agent_command("codex", Path::new("/w/proj"), &[], None, &p),
            "codex exec -C '/w/proj' -s workspace-write -c sandbox_workspace_write.network_access=true -o '/w/proj/.ai/dispatch/307.last' - < '/w/proj/.ai/dispatch/307.prompt'"
        );
        assert_eq!(
            agent_command("gemini", Path::new("/w/proj"), &[], None, &p),
            "gemini --approval-mode auto_edit -p \"$(cat '/w/proj/.ai/dispatch/307.prompt')\""
        );
    }

    #[test]
    fn runner_exports_worker_role_and_writes_exit_after_the_log() {
        let script = runner_script("codex", "agent", Path::new("/w/proj"), &[], None, 307, None, "/a:/b", &sample());
        assert!(script.contains("export PATH='/a:/b' T26_ROW=307 T26_ROLE=worker\n"));
        assert!(script.contains("rm -f '/w/proj/.ai/dispatch/307.exit'\n"));
        assert!(script.contains("| tee '/w/proj/.ai/dispatch/307.log'\n"));
        assert!(script.ends_with("&& mv '/w/proj/.ai/dispatch/307.exit'.tmp '/w/proj/.ai/dispatch/307.exit'\n"));
    }

    #[test]
    fn result_is_one_line_with_the_last_message() {
        assert_eq!(last_line("done\n\n  final answer \n\n"), "final answer");
        assert_eq!(result_line("0\n", &sample(), "ok"), "exit=0 log=/w/proj/.ai/dispatch/307.log last=ok");
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
        let script = runner_script("codex", "agent", Path::new("/w/proj"), &[], Some("abc-123"), 307, None, "/a:/b", &sample());
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
        assert!(runner_script("codex", "verifier", Path::new("/w/proj"), &[], None, 504, None, "/bin", &p).contains("T26_ROLE=verifier"));
        assert!(runner_script("codex", "verifier", Path::new("/w/proj"), &[], Some("abc"), 504, None, "/bin", &p).contains("tee -a '/w/proj/.ai/dispatch/504.verify.log'"));
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
        let script = runner_script("codex", "verifier", Path::new("/w/proj"), &[], None, 528, parsed.rows.as_deref(), "/bin", &sample());
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
}
