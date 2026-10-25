//! Runs one Table26 row's worker (headless `codex exec` or `gemini -p`) in a new tmux pane.
//! The orchestrator reads `.ai/dispatch/<row>.{log,exit,last}`, never the pane text.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

const USAGE: &str = "usage: dispatch-agent [--resume] [--agent codex|gemini] [--project DIR] --row ID --prompt-file F [--add-dir DIR ...]";

struct Args {
    resume: bool,
    agent: String,
    project: PathBuf,
    row: u64,
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
    let row = get("--row")?.parse().map_err(|_| format!("--row must be a number\n{USAGE}"))?;
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
    Ok(Args { resume, agent, project, row, prompt_file, add_dirs })
}

fn paths(project: &Path, row: u64) -> Paths {
    let dir = project.join(".ai").join("dispatch");
    let file = |ext: &str| dir.join(format!("{row}.{ext}"));
    Paths { prompt: file("prompt"), script: file("sh"), log: file("log"), exit: file("exit"), last: file("last"), dir }
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
fn runner_script(agent: &str, project: &Path, add_dirs: &[PathBuf], resume_session: Option<&str>, row: u64, path_env: &str, p: &Paths) -> String {
    let tee = if resume_session.is_some() { "tee -a" } else { "tee" };
    format!(
        "#!/bin/bash\ncd {project} || exit 1\nrm -f {exit}\nexport PATH={path} T26_ROW={row} T26_ROLE=worker\n{cmd} 2>&1 | {tee} {log}\necho \"${{PIPESTATUS[0]}}\" > {exit}.tmp && mv {exit}.tmp {exit}\n",
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

fn run(args: Args) -> Result<i32, String> {
    let tmux_socket = std::env::var("TMUX").ok()
        .and_then(|value| value.split(',').next().filter(|socket| !socket.is_empty()).map(str::to_owned))
        .ok_or("dispatch-agent needs a tmux session (TMUX is not set)")?;
    let caller_pane = std::env::var("TMUX_PANE")
        .ok().filter(|pane| !pane.is_empty())
        .ok_or("dispatch-agent needs a tmux pane (TMUX_PANE is not set)")?;
    let window = tmux_call("display-message", Command::new("tmux")
        .args(["display-message", "-p", "-t", &caller_pane, "#{window_id}"]))?;
    let p = paths(&args.project, args.row);
    fs::create_dir_all(&p.dir).map_err(|e| format!("{}: {e}", p.dir.display()))?;
    let session = if args.resume {
        let log = fs::read_to_string(&p.log).map_err(|e| format!("{}: {e}", p.log.display()))?;
        Some(required_session_id(&log, &p.log)?.to_string())
    } else {
        None
    };
    fs::copy(&args.prompt_file, &p.prompt).map_err(|e| format!("--prompt-file: {e}"))?;
    let stale_paths = [&p.exit, &p.last, &p.log];
    for stale in stale_paths.into_iter().take(if args.resume { 2 } else { 3 }) {
        let _ = fs::remove_file(stale);
    }
    let path_env = std::env::var("PATH").unwrap_or_default();
    fs::write(&p.script, runner_script(&args.agent, &args.project, &args.add_dirs, session.as_deref(), args.row, &path_env, &p)).map_err(|e| e.to_string())?;
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
    println!("{}", result_line(&code, &p, &last));
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

    fn sample() -> Paths {
        paths(Path::new("/w/proj"), 307)
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
        let script = runner_script("codex", Path::new("/w/proj"), &[], None, 307, "/a:/b", &sample());
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
        let script = runner_script("codex", Path::new("/w/proj"), &[], Some("abc-123"), 307, "/a:/b", &sample());
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
}
