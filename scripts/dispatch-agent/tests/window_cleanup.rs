use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Server {
    dir: PathBuf,
}

impl Server {
    fn tmux(&self, args: &[&str]) -> String {
        let output = Command::new("tmux").env_remove("TMUX").env("TMUX_TMPDIR", &self.dir).args(args).output().unwrap();
        assert!(output.status.success(), "tmux {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = Command::new("tmux").env_remove("TMUX").env("TMUX_TMPDIR", &self.dir).arg("kill-server").output();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn detached_run_closes_leftovers_once_and_spares_a_window_still_starting() {
    let dir = std::env::temp_dir().join(format!("dwc{}", std::process::id()));
    let server = Server { dir: dir.clone() };
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(dir.join("AGENTS.md"), "# 상태\n### 실행\nruntime : 없음(로컬 CLI)\n").unwrap();
    fs::create_dir_all(dir.join(".ai/changes")).unwrap();
    fs::write(dir.join(".ai/changes/request.md"), "## row (#77)\n").unwrap();
    server.tmux(&["-f", "/dev/null", "new-session", "-d", "-s", "s", "-n", "base"]);
    server.tmux(&["new-window", "-d", "-n", "starting-9", "bash"]);
    server.tmux(&["new-window", "-d", "-n", "dispatch-8", "bash"]);

    let seen = dir.join("seen.txt");
    script(&bin.join("gemini"), &format!("#!/bin/sh\ntmux list-windows -F '#W' > {}\nprintf 'done\\n'\n", seen.display()));
    script(&bin.join("grist"), "#!/bin/sh\ncase \"$1\" in\n  task-show) printf '{\"id\":%s,\"section\":\"row\",\"lines\":[]}\\n' \"$2\" ;;\n  task) printf '[{\"id\":%s,\"fields\":{\"H\":\"작업\"}}]\\n' \"$2\" ;;\nesac\n");
    let prompt = dir.join("prompt.txt");
    fs::write(&prompt, "row").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_dispatch-agent"))
        .args(["--agent", "gemini", "--project"]).arg(&dir)
        .args(["--row", "77", "--prompt-file"]).arg(&prompt)
        .env_remove("TMUX").env_remove("TMUX_PANE").env("TMUX_TMPDIR", &dir)
        .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()))
        .output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("창 정리 실패"), "{stderr}");
    assert!(!stderr.contains("can't find window"), "{stderr}");

    assert!(fs::read_to_string(&seen).unwrap().lines().any(|name| name == "dispatch-77"));
    let left = server.tmux(&["list-windows", "-F", "#W"]);
    let mut names = left.lines().collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["base", "starting-9"]);
}
