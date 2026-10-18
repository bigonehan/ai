// Feeds hook JSON from tests/fixtures/<case>.json to the architecture-context binary and
// verifies stdout against tests/fixtures/<case>.stdout.
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "architecture-context-test-{}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst),
        name
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Fake reader: records args (one per line) to `called`, prints `raw`, exits with `rawcode`.
fn fake_reader(dir: &Path, raw: &str, code: i32, stderr: &str) -> PathBuf {
    fs::write(dir.join("raw"), raw).unwrap();
    fs::write(dir.join("err"), stderr).unwrap();
    let script = dir.join("fake-reader");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{d}/called'\ncat '{d}/raw'\ncat '{d}/err' >&2\nexit {code}\n",
            d = dir.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    script
}

fn run_hook(input: &str, envs: &[(&str, &Path)], cwd: &Path) -> (String, Option<i32>) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_architecture-context"));
    cmd.current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("READ_ARCHITECTURE_BIN");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code(),
    )
}

fn run_fixture(name: &str) {
    let fx = fixtures();
    let read = |ext: &str| fs::read_to_string(fx.join(format!("{name}.{ext}"))).ok();
    let input = read("json").unwrap();
    let expected_args = read("args").unwrap_or_default();
    let raw = read("raw").unwrap_or_default();
    let raw_code: i32 = read("rawcode").map(|c| c.trim().parse().unwrap()).unwrap_or(0);
    let dir = temp(name);
    let fake = fake_reader(&dir, &raw, raw_code, "");
    let (stdout, code) = run_hook(&input, &[("READ_ARCHITECTURE_BIN", &fake)], &dir);
    let called = fs::read_to_string(dir.join("called")).ok();
    let _ = fs::remove_dir_all(&dir);
    if expected_args.is_empty() {
        assert_eq!(called, None, "reader must not run for {name}");
    } else {
        assert_eq!(called.as_deref(), Some(expected_args.as_str()), "reader args for {name}");
    }
    assert_eq!(stdout, read("stdout").unwrap(), "stdout for {name}");
    let expected_code: i32 = read("code").unwrap().trim().parse().unwrap();
    assert_eq!(code, Some(expected_code), "exit code for {name}");
}

#[test]
fn cwd_inside_project_with_prompt_query() {
    run_fixture("inside_prompt");
}

#[test]
fn cwd_outside_extensions_prints_nothing() {
    run_fixture("outside");
}

#[test]
fn cwd_extensions_root_prints_nothing() {
    run_fixture("root_cwd");
}

#[test]
fn prompt_naming_project_path_wins_over_cwd() {
    run_fixture("outside_named");
}

#[test]
fn prompt_naming_non_project_falls_back_to_cwd() {
    run_fixture("explicit_nonproject");
}

#[test]
fn session_start_has_no_query() {
    run_fixture("session_start");
}

#[test]
fn missing_doc_exit_2_context() {
    run_fixture("missing_doc");
}

#[test]
fn missing_event_name_omits_field() {
    run_fixture("no_event_name");
}

#[test]
fn query_words_deduped_filtered_and_capped_at_8() {
    run_fixture("many_words");
}

const INSIDE: &str = r#"{"cwd":"/home/tree/extensions/plus_test","prompt":"x","hook_event_name":"UserPromptSubmit"}"#;

#[test]
fn reader_failure_reports_stderr() {
    let dir = temp("failure");
    let fake = fake_reader(&dir, "ignored", 1, "  boom happened \n");
    let (stdout, code) = run_hook(INSIDE, &[("READ_ARCHITECTURE_BIN", &fake)], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        stdout,
        "{\"hookSpecificOutput\":{\"hookEventName\":\"UserPromptSubmit\",\"additionalContext\":\"아키텍처 검사 실행 실패: boom happened\"}}\n"
    );
    assert_eq!(code, Some(0));
}

#[test]
fn reader_failure_without_stderr_reports_exit() {
    let dir = temp("failure-exit");
    let fake = fake_reader(&dir, "", 5, "");
    let (stdout, _) = run_hook(INSIDE, &[("READ_ARCHITECTURE_BIN", &fake)], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        stdout,
        "{\"hookSpecificOutput\":{\"hookEventName\":\"UserPromptSubmit\",\"additionalContext\":\"아키텍처 검사 실행 실패: exit 5\"}}\n"
    );
}

#[test]
fn reader_missing_reports_enoent() {
    let dir = temp("enoent");
    let missing = dir.join("no-such-reader");
    let (stdout, code) = run_hook(INSIDE, &[("READ_ARCHITECTURE_BIN", &missing)], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        stdout,
        format!(
            "{{\"hookSpecificOutput\":{{\"hookEventName\":\"UserPromptSubmit\",\"additionalContext\":\"아키텍처 검사 실행 실패: spawnSync {} ENOENT\"}}}}\n",
            missing.display()
        )
    );
    assert_eq!(code, Some(0));
}

#[test]
fn default_reader_is_home_ai_scripts_bin() {
    let dir = temp("default-bin");
    let bin = dir.join("ai/scripts/bin");
    fs::create_dir_all(&bin).unwrap();
    let fake = fake_reader(&dir, "DEFAULT READER\n", 0, "");
    fs::rename(&fake, bin.join("read-architecture")).unwrap();
    let (stdout, _) = run_hook(INSIDE, &[("HOME", &dir)], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        stdout,
        "{\"hookSpecificOutput\":{\"hookEventName\":\"UserPromptSubmit\",\"additionalContext\":\"DEFAULT READER\\n위 구조에서 대상 폴더를 고른 뒤 rg -n으로 위치를 찾고 필요한 범위만 읽으세요.\"}}\n"
    );
}

#[test]
fn invalid_json_fails_without_output() {
    let dir = temp("invalid");
    let (stdout, code) = run_hook("not json", &[], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(stdout, "");
    assert_eq!(code, Some(1));
}
