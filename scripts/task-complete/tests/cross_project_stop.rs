use std::{fs, io::Write, path::PathBuf, process::{Command, Stdio}};

#[test]
fn stop_hook_keeps_other_project_pending_after_process_row_is_confirmed() {
    let root = PathBuf::from("/home/tree/extensions");
    let fixture = root.join("test/tmp").join(format!("stop-cross-{}", std::process::id()));
    let _ = fs::remove_dir_all(&fixture);
    let product = root.join("web_save_current_page");
    fs::create_dir_all(fixture.join("session_rows")).unwrap();
    fs::create_dir_all(fixture.join("session_projects")).unwrap();
    fs::write(fixture.join("session_rows/stop-cross.json"), "[858,853]").unwrap();
    fs::write(fixture.join("session_projects/stop-cross.json"), format!(r#"{{"858":"{}","853":"{}"}}"#, root.display(), product.display())).unwrap();
    let transcript = fixture.join("transcript.jsonl");
    fs::write(&transcript, "{\"type\":\"user\",\"message\":{\"content\":\"과정 수정 후 원래 작업 계속\"}}\n").unwrap();
    let mock = fixture.join("grist");
    fs::write(&mock, format!("#!/bin/sh\nif [ \"$1\" = sync ]; then printf '{{}}'; exit 0; fi\nif [ \"$1\" = gate ]; then\n  if [ \"$3\" = '{}' ]; then\n    printf '{{\"rows\":[{{\"id\":858,\"H\":\"확인\"}}],\"project_dir\":\"{}\",\"uninitialized\":[]}}'\n  elif [ \"$4\" = --row ]; then\n    printf '{{\"rows\":[{{\"id\":853,\"H\":\"검증\"}}],\"project_dir\":\"{}\",\"uninitialized\":[]}}'\n  else\n    printf '{{\"rows\":[],\"project_dir\":\"{}\",\"uninitialized\":[]}}'\n  fi\n  exit 0\nfi\nexit 2\n", root.display(), root.display(), product.display(), product.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&mock, fs::Permissions::from_mode(0o755)).unwrap();
    let event = format!(r#"{{"cwd":"{}","session_id":"stop-cross","transcript_path":"{}","last_assistant_message":"완료했어요"}}"#, root.display(), transcript.display());
    let decision = fixture.join("manage_decision");
    fs::write(&decision, "#!/bin/sh\ncat >/dev/null\nexit 2\n").unwrap(); // 재개 지시는 판정하지 않음: CLI 실패 = 판정 건너뜀
    fs::set_permissions(&decision, fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_task-complete"))
        .arg("--hook").env_clear().env("HOME", "/home/tree").env("PATH", "/usr/bin:/bin")
        .env("GRIST_BIN", &mock).env("MANAGE_DECISION_BIN", &decision).env("TASK_COMPLETE_STATE_DIR", &fixture)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"decision\": \"block\"") && stdout.contains("#853=검증"), "{stdout}");
    let confirmed_product = product;
    fs::write(&mock, format!("#!/bin/sh\nif [ \"$1\" = sync ]; then printf '{{}}'; exit 0; fi\nif [ \"$1\" = gate ]; then\n  if [ \"$3\" = '{}' ]; then\n    printf '{{\"rows\":[{{\"id\":858,\"H\":\"확인\"}}],\"project_dir\":\"{}\",\"uninitialized\":[]}}'\n  else\n    printf '{{\"rows\":[{{\"id\":853,\"H\":\"확인\"}}],\"project_dir\":\"{}\",\"uninitialized\":[]}}'\n  fi\n  exit 0\nfi\nexit 2\n", root.display(), root.display(), confirmed_product.display())).unwrap();
    let jev = fixture.join("jev");
    fs::write(&jev, "#!/bin/sh\nprintf called > \"$TASK_COMPLETE_STATE_DIR/jev-called\"\nprintf '{\"ok\":true,\"mode\":\"enforce\",\"command\":\"stop\",\"verdict\":\"pass\",\"category\":\"완료 보고\",\"items\":[{\"name\":\"category\",\"verdict\":\"pass\"}]}'\n").unwrap();
    fs::set_permissions(&jev, fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_task-complete"))
        .arg("--hook").env_clear().env("HOME", "/home/tree").env("PATH", "/usr/bin:/bin")
        .env("GRIST_BIN", &mock).env("JEV_BIN", &jev).env("MANAGE_DECISION_BIN", &decision).env("TASK_COMPLETE_STATE_DIR", &fixture)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("\"decision\": \"block\""), "{stdout}");
    assert_eq!(fs::read_to_string(fixture.join("jev-called")).unwrap(), "called");
    fs::remove_dir_all(fixture).unwrap();
}
