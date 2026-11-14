use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::Write;

fn fixture() -> PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let temp = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/tmp");
    fs::create_dir_all(&temp).unwrap();
    for entry in fs::read_dir(&temp).unwrap().filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let old_pid = name.strip_prefix("blackbox-").and_then(|rest| rest.split('-').next()).and_then(|pid| pid.parse::<u32>().ok());
        if old_pid.is_some_and(|pid| !PathBuf::from(format!("/proc/{pid}")).exists()) {
            fs::remove_dir_all(entry.path()).unwrap();
        }
    }
    let root = temp.join(format!("blackbox-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    assert!(Command::new("git").args(["init", "-q"]).arg(&root).status().unwrap().success());
    root
}
fn call(root: &Path, args: &[&str], input: &str) -> String {
    call_in(root.join(".blackbox"), args, input)
}
fn call_in(dir: PathBuf, args: &[&str], input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cli_watch_blackbox"))
        .args(args).env_remove("T26_ROLE").env("CLI_WATCH_BLACKBOX_TEST_NO_DAEMON", "1")
        .env("CLI_WATCH_BLACKBOX_DIR", dir)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn initialize_keeps_existing_ancestor_gitignore_for_nested_blackbox() {
    let root = fixture();
    let dir = root.join("test/tmp/.blackbox");
    let existing = "/test/tmp/\n";
    fs::write(root.join(".gitignore"), existing).unwrap();
    call_in(dir.clone(), &["mark", "--cause", "user_keyboard", "--row", "613"], "");
    assert_eq!(fs::read_to_string(root.join(".gitignore")).unwrap(), existing);
    assert_eq!(fs::read_to_string(dir.join(".gitignore")).unwrap(), "*\n");
    for row in [613, 617, 619] {
        assert!(Command::new("git").arg("-C").arg(&root)
            .args(["check-ignore", "-q"]).arg(dir.join(format!("{row}.jsonl")))
            .status().unwrap().success());
    }
    println!("[verify-step] range-check start=613 mid=617 end=619");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn session_binding_causes_and_git_ignore() {
    let root = fixture();
    let dir = root.join(".blackbox");
    let cwd = root.to_str().unwrap();
    call(&root, &["hook-session-start"], &format!(r#"{{"session_id":"session-303","cwd":"{cwd}"}}"#));
    assert!(dir.join("session-session-303.jsonl").is_file());
    assert_eq!(fs::read_to_string(dir.join(".gitignore")).unwrap(), "*\n");
    assert!(Command::new("git").arg("-C").arg(&root).args(["check-ignore", "-q"]).arg(dir.join("303.jsonl")).status().unwrap().success());
    call(&root, &["bind-row", "--row", "303", "--session", "session-303", "--cwd", cwd], "");
    assert!(dir.join("303.jsonl").is_file());
    assert!(!dir.join("session-session-303.jsonl").exists());
    call(&root, &["hook-stop"], r#"{"session_id":"session-303","terminationReason":"USER_CANCELED"}"#);
    assert!(call(&root, &["query", "303"], "").contains("cause=user_keyboard"));
    call(&root, &["daemon", "--session", "session-303", "--pid", "4294967295"], "");
    assert!(call(&root, &["query", "303"], "").contains("cause=user_keyboard"));
    call(&root, &["complete", "--row", "303"], "");
    assert!(call(&root, &["query", "303"], "").contains("cause=user_keyboard"));
    call(&root, &["mark", "--cause", "other_session", "--row", "305", "--actor", "session-303", "--detail", "tmux kill-pane"], "");
    assert!(call(&root, &["query", "305"], "").contains("cause=other_session"));
    call(&root, &["hook-session-start"], &format!(r#"{{"session_id":"session-308","cwd":"{cwd}"}}"#));
    call(&root, &["bind-row", "--row", "308", "--session", "session-308"], "");
    call(&root, &["hook-stop"], r#"{"session_id":"session-308","terminationReason":"USER_CANCELED"}"#);
    call(&root, &["mark", "--cause", "hook_stop", "--row", "308", "--detail", "dispatch-kill"], "");
    call(&root, &["daemon", "--session", "session-308", "--pid", "4294967295"], "");
    assert!(call(&root, &["query", "308"], "").contains("cause=hook_stop"));
    println!("[verify-step] range-check start=303 mid=305 end=308");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn gc_removes_only_completed_after_one_day() {
    let root = fixture();
    let dir = root.join(".blackbox");
    call(&root, &["mark", "--cause", "user_keyboard", "--row", "403"], "");
    call(&root, &["complete", "--row", "405", "--now", "1700000000"], "");
    call(&root, &["complete", "--row", "408", "--now", "1700000002"], "");
    assert_eq!(call(&root, &["gc", "--now", "1700086400"], "").trim(), "1");
    assert!(dir.join("403.jsonl").exists());
    assert!(!dir.join("405.jsonl").exists());
    assert!(dir.join("408.jsonl").exists());
    call(&root, &["hook-session-start"], &format!(r#"{{"session_id":"question-407","cwd":"{}"}}"#, root.display()));
    call(&root, &["hook-stop"], r#"{"session_id":"question-407"}"#);
    let question = dir.join("session-question-407.jsonl");
    assert!(fs::read_to_string(&question).unwrap().contains("completed_at"));
    call(&root, &["daemon", "--session", "question-407", "--pid", "4294967295"], "");
    assert!(call(&root, &["query", "question-407"], "").contains("cause=normal_exit"));
    let at = fs::read_to_string(&question).unwrap().lines()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok()?.get("completed_at")?.as_u64()).unwrap();
    assert_eq!(call(&root, &["gc", "--now", &(at + 86_399).to_string()], "").trim(), "0");
    assert!(question.exists());
    assert_eq!(call(&root, &["gc", "--now", &(at + 86_401).to_string()], "").trim(), "1");
    assert!(!question.exists());
    println!("[verify-step] range-check start=403 mid=405 end=408");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn worker_lifecycle_does_not_start_or_bind_blackbox() {
    let root = fixture();
    for args in [
        vec!["hook-session-start"],
        vec!["bind-row", "--row", "505", "--session", "worker-505"],
        vec!["hook-stop"],
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cli_watch_blackbox"))
            .args(args).env("T26_ROLE", "worker").env("CLI_WATCH_BLACKBOX_DIR", root.join(".blackbox"))
            .stdin(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(br#"{"session_id":"worker-505"}"#).unwrap();
        assert!(child.wait().unwrap().success());
    }
    assert!(!root.join(".blackbox").exists());
    fs::remove_dir_all(root).unwrap();
}

fn alive(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/status")).ok()
        .is_some_and(|s| !s.lines().any(|l| l.starts_with("State:") && l.contains("Z (zombie)")))
}

#[test]
fn row_daemon_auto_spawns_rebinds_kills_on_exit_and_injects_prompt_context() {
    let root = fixture();
    let dir = root.join(".blackbox");
    let run_live = |args: &[&str], input: &str| -> String {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cli_watch_blackbox"))
            .args(args).env_remove("T26_ROLE").env_remove("CLI_WATCH_BLACKBOX_TEST_NO_DAEMON")
            .env("CLI_WATCH_BLACKBOX_DIR", &dir)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let mut main_proc = Command::new("sleep").arg("30").spawn().unwrap();
    let mpid = main_proc.id().to_string();
    run_live(&["hook-session-start", "--pid", &mpid], r#"{"session_id":"live-901"}"#);
    run_live(&["bind-row", "--row", "901", "--session", "live-901", "--pid", &mpid], "");
    let meta: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join(".sessions/live-901.json")).unwrap()).unwrap();
    let dpid = meta["daemon_pid"].as_u64().unwrap() as u32;
    assert_eq!(meta["daemon_row"].as_str(), Some("901"));
    let cmdline = String::from_utf8_lossy(&fs::read(format!("/proc/{dpid}/cmdline")).unwrap()).into_owned();
    assert!(cmdline.contains("--row\0901"), "expected --row 901 in daemon cmdline: {cmdline:?}");

    run_live(&["hook-stop"], r#"{"session_id":"live-901","terminationReason":"USER_CANCELED"}"#);
    let _ = main_proc.kill();
    let _ = main_proc.wait();
    for _ in 0..40 {
        if !alive(dpid) { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(!alive(dpid), "daemon must auto-exit/kill when main session terminates");
    assert!(fs::read_to_string(dir.join("901.jsonl")).unwrap().contains(r#""event":"session_exit""#));

    let mut second_proc = Command::new("sleep").arg("30").spawn().unwrap();
    let spid = second_proc.id().to_string();
    run_live(&["bind-row", "--row", "903", "--session", "live-903", "--pid", &spid], "");
    let meta2: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join(".sessions/live-903.json")).unwrap()).unwrap();
    let dpid2 = meta2["daemon_pid"].as_u64().unwrap() as u32;
    assert!(alive(dpid2));
    run_live(&["hook-session-end", "--pid", &spid], r#"{"session_id":"live-903"}"#);
    assert!(!alive(dpid2), "hook-session-end must immediately kill daemon");
    let _ = second_proc.kill();
    let _ = second_proc.wait();

    let mut prompt_proc = Command::new("sleep").arg("30").spawn().unwrap();
    let ppid = prompt_proc.id().to_string();
    let injected = run_live(&["hook-prompt", "--pid", &ppid], r#"{"session_id":"ask-901","prompt":"901 왜 중지되었는지 알려줘"}"#);
    assert!(injected.contains("additionalContext") && injected.contains("cause=user_keyboard"), "got: {injected}");
    run_live(&["hook-session-end", "--pid", &ppid], r#"{"session_id":"ask-901"}"#);
    let _ = prompt_proc.kill();
    let _ = prompt_proc.wait();
    println!("[verify-step] range-check start=901 mid=903 end=905");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn gc_never_kills_shell_whose_args_mention_blackbox_daemon() {
    let root = fixture();
    let dir = root.join(".blackbox");
    fs::create_dir_all(dir.join(".sessions")).unwrap();
    let mut shell = Command::new("sh").args(["-c", "sleep 30 # cli_watch_blackbox daemon"]).spawn().unwrap();
    let spid = shell.id();
    fs::write(dir.join(".sessions/fake.json"), format!(r#"{{"session":"fake","pid":4294967295,"daemon_pid":{spid},"at":100}}"#)).unwrap();
    call(&root, &["gc"], "");
    assert!(alive(spid), "shell mentioning cli_watch_blackbox daemon in args must never be killed");
    let _ = shell.kill();
    let _ = shell.wait();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unexpected_kill_ignores_stdout_mtime_and_isolates_concurrent_sessions() {
    let root = fixture();
    let dir = root.join(".blackbox");
    let cwd = root.to_str().unwrap();
    let run_live = |args: &[&str], input: &str| -> String {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cli_watch_blackbox"))
            .args(args).env_remove("T26_ROLE").env_remove("CLI_WATCH_BLACKBOX_TEST_NO_DAEMON").env_remove("TMUX_PANE")
            .env("CLI_WATCH_BLACKBOX_DIR", &dir)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let fake_pty = root.join("fake_pty");
    fs::write(&fake_pty, "init\n").unwrap();
    let mut peer = Command::new("sleep").arg("30").spawn().unwrap();
    let mut victim = Command::new("sh").args(["-c", "sleep 2; true"]).spawn().unwrap();
    let (peer_pid, victim_pid) = (peer.id().to_string(), victim.id().to_string());
    run_live(&["hook-session-start", "--pid", &peer_pid], &format!(r#"{{"session_id":"peer-sess","cwd":"{cwd}"}}"#));
    run_live(&["bind-row", "--row", "703", "--pid", &victim_pid, "--cwd", cwd], "");
    let peer_meta: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join(".sessions/peer-sess.json")).unwrap()).unwrap();
    assert_eq!(peer_meta["pid"].as_u64(), Some(peer.id() as u64), "concurrent session pid must never be hijacked");
    let vmeta_path = dir.join(format!(".sessions/pid-{victim_pid}.json"));
    let mut vmeta: serde_json::Value = serde_json::from_str(&fs::read_to_string(&vmeta_path).unwrap()).unwrap();
    vmeta["tty"] = serde_json::json!(fake_pty.to_str().unwrap());
    vmeta["agent"] = serde_json::json!("claude");
    vmeta["daemon_row"] = serde_json::Value::Null;
    fs::write(&vmeta_path, serde_json::to_vec(&vmeta).unwrap()).unwrap();
    run_live(&["hook-prompt", "--pid", &victim_pid], &format!(r#"{{"cwd":"{cwd}","prompt":"build row 703"}}"#));
    std::thread::sleep(std::time::Duration::from_millis(260));
    fs::OpenOptions::new().append(true).open(&fake_pty).unwrap().write_all(b"ai stdout\n").unwrap();
    let _ = victim.kill();
    let _ = victim.wait();
    std::thread::sleep(std::time::Duration::from_millis(350));
    let report = run_live(&["query", "703"], "");
    assert!(report.contains("cause=unexpected_kill") && report.contains("recovery=cd ") && report.contains("claude --resume") && report.contains("sleep 2"), "got: {report}");
    run_live(&["hook-session-end", "--pid", &peer_pid], r#"{"session_id":"peer-sess"}"#);
    let _ = peer.kill();
    let _ = peer.wait();
    fs::remove_dir_all(root).unwrap();
}
