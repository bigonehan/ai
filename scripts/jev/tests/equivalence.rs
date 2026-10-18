use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

static SEQ: AtomicUsize = AtomicUsize::new(0);
const BIN: &str = env!("CARGO_BIN_EXE_jev");

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn temp_case_dir(label: &str) -> PathBuf {
    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("jev-equiv-{}-{}-{}", std::process::id(), id, label));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

fn subst(text: &str, dir: &str, home: &str) -> String {
    text.replace("{DIR}", dir).replace("{HOME}", home)
}

fn normalize(text: &str, dir: &str, home: &str) -> String {
    text.replace(home, "{HOME}").replace(dir, "{DIR}")
}

struct MockServer {
    addr: std::net::SocketAddr,
    url: String,
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<Value>>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    fn start(status: u16, body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://127.0.0.1:{}/v1/systemone", addr.port());
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop_clone = Arc::clone(&stop);
        let req_clone = Arc::clone(&requests);

        let handle = thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                if stop_clone.load(Ordering::SeqCst) {
                    break;
                }
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let mut header_end = None;
                while header_end.is_none() {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&chunk[..n]);
                            header_end = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4);
                        }
                    }
                }
                let Some(hend) = header_end else {
                    continue;
                };
                let header_str = String::from_utf8_lossy(&buf[..hend]).into_owned();
                let mut lines = header_str.split("\r\n");
                let req_line = lines.next().unwrap_or("");
                let mut parts = req_line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let mut content_len = 0usize;
                let mut auth: Option<String> = None;
                let mut ctype: Option<String> = None;
                for line in lines {
                    if let Some((k, v)) = line.split_once(':') {
                        let k_low = k.trim().to_ascii_lowercase();
                        let v_trim = v.trim().to_string();
                        if k_low == "content-length" {
                            content_len = v_trim.parse().unwrap_or(0);
                        } else if k_low == "authorization" {
                            auth = Some(v_trim);
                        } else if k_low == "content-type" {
                            ctype = Some(v_trim);
                        }
                    }
                }
                while buf.len() < hend + content_len {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let req_body = String::from_utf8_lossy(&buf[hend..(hend + content_len).min(buf.len())]).into_owned();
                req_clone.lock().unwrap().push(json!({
                    "method": method,
                    "path": path,
                    "authorization": auth,
                    "content_type": ctype,
                    "body": req_body,
                }));
                let payload = body.as_bytes();
                let resp = format!(
                    "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    status,
                    payload.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.write_all(payload);
                let _ = stream.flush();
            }
        });

        Self {
            addr,
            url,
            stop,
            requests,
            handle: Some(handle),
        }
    }

    fn finish(mut self) -> Vec<Value> {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let guard = self.requests.lock().unwrap();
        guard.clone()
    }
}

fn run_fixture_case(case: &Value) {
    let name = case["name"].as_str().unwrap();
    let dir_path = temp_case_dir(name);
    let home_path = dir_path.join("home");
    let dir = dir_path.to_string_lossy().into_owned();
    let home = home_path.to_string_lossy().into_owned();

    if let Some(files) = case["files"].as_object() {
        for (fname, content) in files {
            let text = subst(content.as_str().unwrap(), &dir, &home);
            fs::write(dir_path.join(fname), text).unwrap();
        }
    }

    let mut mock: Option<MockServer> = None;
    let url = match &case["server"] {
        Value::Null => "http://127.0.0.1:9/v1/systemone".to_string(),
        Value::String(s) if s == "refused" => {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            format!("http://127.0.0.1:{}/v1/systemone", port)
        }
        Value::Object(map) => {
            let status = map["status"].as_u64().unwrap() as u16;
            let body = map["body"].as_str().unwrap().to_string();
            let server = MockServer::start(status, body);
            let u = server.url.clone();
            mock = Some(server);
            u
        }
        other => panic!("unexpected server spec: {:?}", other),
    };

    let mut cmd = Command::new(BIN);
    cmd.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd.env("HOME", &home);
    cmd.env("JEV_API_URL", &url);

    if let Some(env_map) = case["env"].as_object() {
        for (k, v) in env_map {
            cmd.env(k, subst(v.as_str().unwrap(), &dir, &home));
        }
    }
    if let Some(fake_str) = case["fake"].as_str() {
        let fake_path = dir_path.join("fake.json");
        fs::write(&fake_path, fake_str).unwrap();
        cmd.env("JEV_FAKE_RESPONSE", &fake_path);
    }

    let args: Vec<String> = case["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| subst(a.as_str().unwrap(), &dir, &home))
        .collect();
    cmd.args(&args);

    let out = cmd.output().unwrap();
    let mut requests = mock.map(|m| m.finish()).unwrap_or_default();
    for req in &mut requests {
        if let Some(b) = req["body"].as_str() {
            req["body"] = Value::String(normalize(b, &dir, &home));
        }
    }

    let log_path = home_path.join(".codex/tmp/jev/judgments.jsonl");
    let at_re = Regex::new(r#""at": "\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\+00:00""#).unwrap();
    let mut logs = Vec::new();
    if log_path.exists() {
        for line in fs::read_to_string(&log_path).unwrap().lines() {
            assert!(at_re.is_match(line), "case {name}: invalid timestamp in log line: {line}");
            let replaced = at_re.replace(line, r#""at": "{AT}""#).into_owned();
            logs.push(Value::String(normalize(&replaced, &dir, &home)));
        }
    }

    let stdout = normalize(&String::from_utf8_lossy(&out.stdout), &dir, &home);
    let stderr = normalize(&String::from_utf8_lossy(&out.stderr), &dir, &home);
    let exit_code = out.status.code().unwrap_or(-1);

    // Key secrecy invariant across all outputs
    let combined = format!("{}{}{}", stdout, stderr, serde_json::to_string(&logs).unwrap());
    assert!(!combined.contains("sk-test-SECRET-123"), "case {name}: leaked secret key");
    assert!(!combined.contains("sk-test-ENVKEY-9"), "case {name}: leaked env key");

    let expected_str = fs::read_to_string(fixtures_dir().join("expected").join(format!("{name}.json"))).unwrap();
    let expected: Value = serde_json::from_str(&expected_str).unwrap();

    let exp_exit = expected["exit"].as_i64().unwrap() as i32;
    let exp_stdout = expected["stdout"].as_str().unwrap();
    let exp_stderr = expected["stderr"].as_str().unwrap();

    let _ = fs::remove_dir_all(&dir_path);

    assert_eq!(exit_code, exp_exit, "case {name}: exit code mismatch (stderr: {stderr})");
    assert_eq!(stdout, exp_stdout, "case {name}: stdout mismatch");
    if exp_exit == 1 {
        assert!(!stderr.is_empty(), "case {name}: expected non-empty stderr on exit 1");
    } else {
        assert_eq!(stderr, exp_stderr, "case {name}: stderr mismatch");
    }
    assert_eq!(Value::Array(requests), expected["requests"], "case {name}: requests mismatch");
    assert_eq!(Value::Array(logs), expected["log"], "case {name}: log mismatch");
}

#[test]
fn all_fixture_cases() {
    let bin_symlink = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bin/jev");
    assert!(bin_symlink.exists(), "~/ai/scripts/bin/jev symlink must exist");
    assert!(
        fs::canonicalize(&bin_symlink).unwrap().ends_with("target/release/jev"),
        "~/ai/scripts/bin/jev must point to target/release/jev"
    );
    let cases_text = fs::read_to_string(fixtures_dir().join("cases.json")).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&cases_text).unwrap();
    assert_eq!(cases.len(), 36);
    for case in &cases {
        run_fixture_case(case);
    }
    println!(
        "jev fixture cases: {} cases (~/ai/scripts/bin/jev scope|match|stop, JEV_MODE=off, 키 없음, 가짜 API 응답) 통과",
        cases.len()
    );
}

#[test]
fn modes_off_shadow_enforce_and_verdict_parsing() {
    let dir_path = temp_case_dir("modes");
    let home_path = dir_path.join("home");
    let req_file = dir_path.join("req.md");
    let goal_file = dir_path.join("goal.md");
    let env_file = dir_path.join("jev.env");
    let fake_file = dir_path.join("fake.json");

    fs::write(&req_file, "모드 및 판정 테스트\n").unwrap();
    fs::write(&goal_file, "- [값] 첫 조건\n- [상태] 둘째 조건\n- [화면] 셋째 조건\n").unwrap();
    fs::write(&env_file, "TYPESAFE_API_KEY=sk-test-MODE-KEY\n").unwrap();
    fs::write(
        &fake_file,
        r#"{"answers":{"g1":{"noul":0.95},"g2":{"noul":0.7},"g3":{"noul":0.2},"coverage":{"noul":0.95}}}"#,
    )
    .unwrap();

    for mode in ["off", "shadow", "enforce"] {
        let out = Command::new(BIN)
            .env_clear()
            .env("HOME", &home_path)
            .env("JEV_ENV_FILE", &env_file)
            .env("JEV_MODE", mode)
            .env("JEV_FAKE_RESPONSE", &fake_file)
            .args([
                "scope",
                "--request-file",
                req_file.to_str().unwrap(),
                "--goal-file",
                goal_file.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["mode"], mode);
        assert_eq!(parsed["verdict"], "fail");
        let items = parsed["items"].as_array().unwrap();
        assert_eq!(items[0]["verdict"], "pass");
        assert_eq!(items[1]["verdict"], "uncertain");
        assert_eq!(items[2]["verdict"], "fail");
    }
    let _ = fs::remove_dir_all(&dir_path);
    println!("jev modes: off·shadow·enforce 및 pass/uncertain/fail 판정 파싱 통과");
}

#[test]
fn fail_open_on_missing_answers_and_unwritable_log() {
    let dir_path = temp_case_dir("fail_open");
    let home_path = dir_path.join("home");
    // Block log directory creation by placing a file at ~/.codex/tmp/jev
    fs::create_dir_all(home_path.join(".codex/tmp")).unwrap();
    fs::write(home_path.join(".codex/tmp/jev"), "not a directory").unwrap();

    let msg_file = dir_path.join("msg.md");
    let fake_file = dir_path.join("fake.json");
    fs::write(&msg_file, "완료 보고 메시지\n").unwrap();
    fs::write(&fake_file, r#"{"answers":{}}"#).unwrap();

    let out = Command::new(BIN)
        .env_clear()
        .env("HOME", &home_path)
        .env("JEV_FAKE_RESPONSE", &fake_file)
        .args(["stop", "--message-file", msg_file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["verdict"], "uncertain");
    let _ = fs::remove_dir_all(&dir_path);
    println!("jev fail-open: 응답 누락 uncertain 처리 및 로그 쓰기 실패 시 fail-open 통과");
}

#[test]
fn key_read_from_default_skill_env_and_never_leaked() {
    let dir_path = temp_case_dir("skill_env");
    let home_path = dir_path.join("home");
    let skill_env_dir = home_path.join(".codex/skills/access-jev");
    fs::create_dir_all(&skill_env_dir).unwrap();
    let secret = "sk-default-skill-env-SECRET-999";
    fs::write(skill_env_dir.join(".env"), format!("TYPESAFE_API_KEY=\"{secret}\"\n")).unwrap();

    let msg_file = dir_path.join("msg.md");
    fs::write(&msg_file, format!("Authorization: Bearer {secret}\nAPI_KEY={secret}\n")).unwrap();

    let server = MockServer::start(
        200,
        r#"{"answers":{"category":{"choice":"완료 보고","confidence":0.95}}}"#.to_string(),
    );
    let out = Command::new(BIN)
        .env_clear()
        .env("HOME", &home_path)
        .env("JEV_API_URL", &server.url)
        .args(["stop", "--message-file", msg_file.to_str().unwrap()])
        .output()
        .unwrap();
    let reqs = server.finish();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0]["authorization"], format!("Bearer {secret}"));
    let sent_body = reqs[0]["body"].as_str().unwrap();
    assert!(!sent_body.contains(secret), "state sent to API must mask secrets");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let log_text = fs::read_to_string(home_path.join(".codex/tmp/jev/judgments.jsonl")).unwrap_or_default();
    assert!(!stdout.contains(secret));
    assert!(!stderr.contains(secret));
    assert!(!log_text.contains(secret));
    let _ = fs::remove_dir_all(&dir_path);
    println!("jev key secrecy: access-jev Skill .env 키 로드 및 출력·로그 노출 없음 통과");
}

#[test]
fn varying_goal_and_output_files_drive_request_and_verdict() {
    let dir_path = temp_case_dir("variant");
    let home_path = dir_path.join("home");
    let env_file = dir_path.join("jev.env");
    fs::write(&env_file, "TYPESAFE_API_KEY=sk-test-VAR\n").unwrap();

    let goal_a = dir_path.join("goal_a.md");
    let out_a = dir_path.join("out_a.md");
    fs::write(&goal_a, "- [값] 알파 출력 일치\n").unwrap();
    fs::write(&out_a, "실제 알파=1\n").unwrap();

    let server_a = MockServer::start(200, r#"{"answers":{"g1":{"noul":0.96}}}"#.to_string());
    let res_a = Command::new(BIN)
        .env_clear()
        .env("HOME", &home_path)
        .env("JEV_ENV_FILE", &env_file)
        .env("JEV_API_URL", &server_a.url)
        .args([
            "match",
            "--goal-file",
            goal_a.to_str().unwrap(),
            "--output-file",
            out_a.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let reqs_a = server_a.finish();
    let json_a: Value = serde_json::from_slice(&res_a.stdout).unwrap();
    assert_eq!(json_a["verdict"], "pass");
    assert!(reqs_a[0]["body"].as_str().unwrap().contains("알파 출력 일치"));
    assert!(reqs_a[0]["body"].as_str().unwrap().contains("실제 알파=1"));

    let goal_b = dir_path.join("goal_b.md");
    let out_b = dir_path.join("out_b.md");
    fs::write(&goal_b, "- [상태] 베타 상태 통과\n").unwrap();
    fs::write(&out_b, "실제 베타=실패\n").unwrap();

    let server_b = MockServer::start(200, r#"{"answers":{"g1":{"noul":0.15}}}"#.to_string());
    let res_b = Command::new(BIN)
        .env_clear()
        .env("HOME", &home_path)
        .env("JEV_ENV_FILE", &env_file)
        .env("JEV_API_URL", &server_b.url)
        .args([
            "match",
            "--goal-file",
            goal_b.to_str().unwrap(),
            "--output-file",
            out_b.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let reqs_b = server_b.finish();
    let json_b: Value = serde_json::from_slice(&res_b.stdout).unwrap();
    assert_eq!(json_b["verdict"], "fail");
    assert!(reqs_b[0]["body"].as_str().unwrap().contains("베타 상태 통과"));
    assert!(reqs_b[0]["body"].as_str().unwrap().contains("실제 베타=실패"));

    let _ = fs::remove_dir_all(&dir_path);
    println!("jev variant: 다른 goal·output 파일 입력 시 요청 생성 및 응답별 판정 결과 변화 통과");
}
