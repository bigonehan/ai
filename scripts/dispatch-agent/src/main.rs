//! Runs one Table26 row's worker (headless `codex exec` or `gemini -p`) in a new tmux pane.
//! The orchestrator reads `.ai/dispatch/<row>.{log,exit,last}`, never the pane text.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

const USAGE: &str = "usage: dispatch-agent --agent codex|gemini --project DIR --row ID --prompt-file F";

struct Args {
    agent: String,
    project: PathBuf,
    row: u64,
    prompt_file: PathBuf,
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
    let agent = get("--agent")?;
    if agent != "codex" && agent != "gemini" {
        return Err(format!("--agent must be codex or gemini\n{USAGE}"));
    }
    let row = get("--row")?.parse().map_err(|_| format!("--row must be a number\n{USAGE}"))?;
    let project = fs::canonicalize(get("--project")?).map_err(|e| format!("--project: {e}"))?;
    let prompt_file = PathBuf::from(get("--prompt-file")?);
    Ok(Args { agent, project, row, prompt_file })
}

fn paths(project: &Path, row: u64) -> Paths {
    let dir = project.join(".ai").join("dispatch");
    let file = |ext: &str| dir.join(format!("{row}.{ext}"));
    Paths { prompt: file("prompt"), script: file("sh"), log: file("log"), exit: file("exit"), last: file("last"), dir }
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// The agent command line; its prompt comes from the saved prompt file.
fn agent_command(agent: &str, project: &Path, p: &Paths) -> String {
    match agent {
        "codex" => format!(
            "codex exec -C {} -s workspace-write -o {} - < {}",
            quote(project), quote(&p.last), quote(&p.prompt)
        ),
        _ => format!("gemini --approval-mode auto_edit -p \"$(cat {})\"", quote(&p.prompt)),
    }
}

/// Bash runner for the pane: the caller's PATH (a tmux pane starts from the server's environment), worker env,
/// full output to the log, exit code written last and atomically.
fn runner_script(agent: &str, project: &Path, row: u64, path_env: &str, p: &Paths) -> String {
    format!(
        "#!/bin/bash\ncd {project} || exit 1\nexport PATH={path} T26_ROW={row} T26_ROLE=worker\n{cmd} 2>&1 | tee {log}\necho \"${{PIPESTATUS[0]}}\" > {exit}.tmp && mv {exit}.tmp {exit}\n",
        project = quote(project),
        path = quote(Path::new(path_env)),
        cmd = agent_command(agent, project, p),
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
    if std::env::var_os("TMUX").is_none() {
        return Err("dispatch-agent needs a tmux session (TMUX is not set)".into());
    }
    let p = paths(&args.project, args.row);
    fs::create_dir_all(&p.dir).map_err(|e| format!("{}: {e}", p.dir.display()))?;
    fs::copy(&args.prompt_file, &p.prompt).map_err(|e| format!("--prompt-file: {e}"))?;
    for stale in [&p.exit, &p.last, &p.log] {
        let _ = fs::remove_file(stale);
    }
    let path_env = std::env::var("PATH").unwrap_or_default();
    fs::write(&p.script, runner_script(&args.agent, &args.project, args.row, &path_env, &p)).map_err(|e| e.to_string())?;
    let pane = Command::new("tmux")
        .args(["split-window", "-d", "-P", "-F", "#{pane_id}", "-c"])
        .arg(&args.project)
        .arg("bash")
        .arg(&p.script)
        .output()
        .map_err(|e| format!("tmux: {e}"))?;
    if !pane.status.success() {
        return Err(format!("tmux split-window failed: {}", String::from_utf8_lossy(&pane.stderr).trim()));
    }
    eprintln!("dispatch-agent: row {} {} in pane {}", args.row, args.agent, String::from_utf8_lossy(&pane.stdout).trim());
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
    fn agent_commands_are_headless_and_scoped_to_the_project() {
        let p = sample();
        assert_eq!(
            agent_command("codex", Path::new("/w/proj"), &p),
            "codex exec -C '/w/proj' -s workspace-write -o '/w/proj/.ai/dispatch/307.last' - < '/w/proj/.ai/dispatch/307.prompt'"
        );
        assert_eq!(
            agent_command("gemini", Path::new("/w/proj"), &p),
            "gemini --approval-mode auto_edit -p \"$(cat '/w/proj/.ai/dispatch/307.prompt')\""
        );
    }

    #[test]
    fn runner_exports_worker_role_and_writes_exit_after_the_log() {
        let script = runner_script("codex", Path::new("/w/proj"), 307, "/a:/b", &sample());
        assert!(script.contains("export PATH='/a:/b' T26_ROW=307 T26_ROLE=worker\n"));
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
}
