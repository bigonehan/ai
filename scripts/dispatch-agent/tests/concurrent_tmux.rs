use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const SERVER: &str = "dispatch354";

struct Fixture {
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new("tmux").args(["-L", SERVER, "kill-server"]).output();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn tmux(args: &[&str]) -> String {
    let output = Command::new("tmux").args(["-L", SERVER]).args(args).output().unwrap();
    assert!(output.status.success(), "tmux failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim_end_matches('\n').to_owned()
}

fn dispatch(project: &Path, prompt: &Path, row: u32, socket: &str, caller: &str, bin: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dispatch-agent"));
    command.args(["--agent", "gemini", "--project"])
        .arg(project)
        .args(["--row", &row.to_string(), "--prompt-file"])
        .arg(prompt)
        .env("TMUX", format!("{socket},1,0"))
        .env("TMUX_PANE", caller)
        .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    command
}

fn successful(output: Output) {
    assert!(output.status.success(), "dispatch failed: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn concurrent_dispatches_split_right_then_below_first_agent() {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test/tmp").join(format!("dispatch354-{stamp}"));
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let _fixture = Fixture { dir: dir.clone() };
    let gemini = bin.join("gemini");
    fs::write(&gemini, "#!/bin/sh\nprintf 'done\\n'\n").unwrap();
    fs::set_permissions(&gemini, fs::Permissions::from_mode(0o700)).unwrap();
    let first_prompt = dir.join("first.txt");
    let second_prompt = dir.join("second.txt");
    fs::write(&first_prompt, "first").unwrap();
    fs::write(&second_prompt, "second").unwrap();

    tmux(&["-f", "/dev/null", "new-session", "-d", "-x", "120", "-y", "40", "-s", SERVER, "sleep 60"]);
    tmux(&["new-session", "-d", "-s", "keepalive", "cat"]);
    tmux(&["set-option", "-g", "remain-on-exit", "on"]);
    let (socket, caller) = tmux(&["display-message", "-p", "-t", SERVER, "#{socket_path}\t#{pane_id}"])
        .split_once('\t').map(|(a, b)| (a.to_owned(), b.to_owned())).unwrap();
    let first = dispatch(&dir, &first_prompt, 35401, &socket, &caller, &bin).spawn().unwrap();
    let second = dispatch(&dir, &second_prompt, 35402, &socket, &caller, &bin).spawn().unwrap();
    successful(first.wait_with_output().unwrap());
    successful(second.wait_with_output().unwrap());

    let panes = tmux(&["list-panes", "-t", SERVER, "-F", "#{pane_id}\t#{pane_left}\t#{pane_top}\t#{@dispatch-agent}"]);
    let entries: Vec<_> = panes.lines().map(|line| {
        let fields: Vec<_> = line.split('\t').collect();
        (fields[0], fields[1].parse::<u32>().unwrap(), fields[2].parse::<u32>().unwrap(), fields[3])
    }).collect();
    assert_eq!(entries.len(), 3, "{panes}");
    let caller_left = entries.iter().find(|pane| pane.0 == caller).unwrap().1;
    let mut agents: Vec<_> = entries.iter().filter(|pane| pane.3 == "35401" || pane.3 == "35402").collect();
    assert_eq!(agents.len(), 2, "{panes}");
    agents.sort_by_key(|pane| pane.2);
    assert!(agents[0].1 > caller_left, "first pane must split right: {panes}");
    assert_eq!(agents[1].1, agents[0].1, "second pane must stay below the first: {panes}");
    assert!(agents[1].2 > agents[0].2, "second pane must split below the first: {panes}");
}
