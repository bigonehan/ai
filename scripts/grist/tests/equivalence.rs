use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

const SECRET_KEY: &str = "SUPER_SECRET_GRIST_KEY_987654321";

#[derive(Clone, Debug)]
struct FakeState {
    tables: Vec<String>,
    columns: HashMap<String, Vec<Value>>,
    records: HashMap<String, Vec<(i64, Map<String, Value>)>>,
    next_id: i64,
    last_auth: Option<String>,
}

impl FakeState {
    fn new() -> Self {
        let mut columns = HashMap::new();
        columns.insert(
            "Table26".to_string(),
            vec![
                json!({
                    "id": "H",
                    "fields": {
                        "label": "진행",
                        "type": "Choice",
                        "widgetOptions": "{\"choices\":[\"초안\",\"진단\",\"작업\",\"교정\",\"검증\",\"확인\",\"완료\"]}"
                    }
                }),
                json!({
                    "id": "D",
                    "fields": {
                        "label": "메모",
                        "type": "Text",
                        "widgetOptions": "{}"
                    }
                }),
            ],
        );
        columns.insert(
            "Table7".to_string(),
            vec![json!({
                "id": "Obsidian_Id",
                "fields": {
                    "label": "Obsidian_Id",
                    "type": "Text",
                    "widgetOptions": "{}"
                }
            })],
        );

        let mut records = HashMap::new();
        records.insert(
            "Table7".to_string(),
            vec![(
                1,
                 serde_json::from_value(json!({"Obsidian_Id": "작업관리기-초기문서.md"})).unwrap(),
            )],
        );
        records.insert(
            "Table26".to_string(),
            vec![
                (
                    1,
                    serde_json::from_value(json!({
                        "B": "첫 작업",
                        "G": "기능",
                        "H": "초안",
                        "D": "",
                        "I": "",
                        "J": "",
                        "K": ""
                    }))
                    .unwrap(),
                ),
                (
                    2,
                    serde_json::from_value(json!({
                        "B": "둘째 작업",
                        "G": "기능",
                        "H": "확인",
                        "D": "",
                        "I": "",
                        "J": "",
                        "K": "요약"
                    }))
                    .unwrap(),
                ),
            ],
        );

        Self {
            tables: vec!["Table7".to_string(), "Table26".to_string()],
            columns,
            records,
            next_id: 10,
            last_auth: None,
        }
    }
}

struct FakeServer {
    url: String,
    state: Arc<Mutex<FakeState>>,
}

fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let raw = input.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() {
            if let Ok(hex) = std::str::from_utf8(&raw[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    bytes.push(b);
                    i += 3;
                    continue;
                }
            }
        }
        bytes.push(raw[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

impl FakeServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(FakeState::new()));
        let state_bg = Arc::clone(&state);

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let state = Arc::clone(&state_bg);
                thread::spawn(move || {
                    handle_conn(stream, state);
                });
            }
        });

        Self {
            url: format!("http://127.0.0.1:{}", addr.port()),
            state,
        }
    }

    fn get_state(&self) -> FakeState {
        self.state.lock().unwrap().clone()
    }

    fn set_state(&self, new_state: FakeState) {
        *self.state.lock().unwrap() = new_state;
    }
}

fn handle_conn(mut stream: std::net::TcpStream, state: Arc<Mutex<FakeState>>) {
    let mut buf = Vec::new();
    let mut temp = [0u8; 4096];
    let header_end = loop {
        match stream.read(&mut temp) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                buf.extend_from_slice(&temp[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos + 4;
                }
            }
        }
    };
    let header_str = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header_str.split("\r\n");
    let req_line = lines.next().unwrap_or("");
    let parts: Vec<&str> = req_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let target = parts[1];
    let mut content_len = 0usize;
    let mut auth = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                content_len = v.trim().parse().unwrap_or(0);
            } else if k.eq_ignore_ascii_case("authorization") {
                auth = Some(v.trim().to_string());
            }
        }
    }
    while buf.len() < header_end + content_len {
        match stream.read(&mut temp) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&temp[..n]),
        }
    }
    let body_bytes = &buf[header_end..buf.len().min(header_end + content_len)];
    let body_json: Value = if body_bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(body_bytes).unwrap_or(json!({}))
    };

    let mut st = state.lock().unwrap();
    st.last_auth = auth;

    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let prefix = "/api/docs/au1EbKLtZbrW";
    let subpath = path.strip_prefix(prefix).unwrap_or(path);

    let (status, resp_val) = if method == "GET" && subpath == "/tables" {
        let tables: Vec<Value> = st.tables.iter().map(|t| json!({"id": t})).collect();
        (200, json!({"tables": tables}))
    } else if let Some(rest) = subpath.strip_prefix("/tables/") {
        let segs: Vec<&str> = rest.split('/').collect();
        let table = segs[0];
        if segs.len() == 2 && segs[1] == "columns" && method == "GET" {
            let cols = st.columns.get(table).cloned().unwrap_or_default();
            (200, json!({"columns": cols}))
        } else if segs.len() == 2 && segs[1] == "columns" && method == "PATCH" {
            if let Some(updates) = body_json.get("columns").and_then(|v| v.as_array()) {
                let cols = st.columns.entry(table.to_string()).or_default();
                for u in updates {
                    let cid = u.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if let Some(ufields) = u.get("fields").and_then(|v| v.as_object()) {
                        for col in cols.iter_mut() {
                            if col.get("id").and_then(|v| v.as_str()) == Some(cid) {
                                if let Some(cfields) = col.get_mut("fields").and_then(|v| v.as_object_mut()) {
                                    for (k, v) in ufields {
                                        cfields.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            (200, json!({}))
        } else if segs.len() == 2 && segs[1] == "records" && method == "GET" {
            let mut filter_map: Map<String, Value> = Map::new();
            if let Some(encoded) = query.strip_prefix("filter=") {
                let decoded = percent_decode(encoded);
                if let Ok(Value::Object(m)) = serde_json::from_str(&decoded) {
                    filter_map = m;
                }
            }
            let rows = st.records.get(table).cloned().unwrap_or_default();
            let mut out = Vec::new();
            for (id, fields) in rows {
                let mut ok = true;
                for (fk, fv) in &filter_map {
                    if let Some(arr) = fv.as_array() {
                        if fk == "id" {
                            if !arr.iter().any(|v| v.as_i64() == Some(id)) {
                                ok = false;
                            }
                        } else {
                            let val = fields.get(fk).cloned().unwrap_or(Value::Null);
                            if !arr.contains(&val) {
                                ok = false;
                            }
                        }
                    }
                }
                if ok {
                    out.push(json!({"id": id, "fields": fields}));
                }
            }
            (200, json!({"records": out}))
        } else if segs.len() == 2 && segs[1] == "records" && method == "POST" {
            let mut created = Vec::new();
            if let Some(recs) = body_json.get("records").and_then(|v| v.as_array()) {
                for r in recs {
                    let id = st.next_id;
                    st.next_id += 1;
                    let fields = r
                        .get("fields")
                        .and_then(|v| v.as_object())
                        .cloned()
                        .unwrap_or_default();
                    st.records.entry(table.to_string()).or_default().push((id, fields));
                    created.push(json!({"id": id}));
                }
            }
            (200, json!({"records": created}))
        } else if segs.len() == 2 && segs[1] == "records" && method == "PATCH" {
            if let Some(recs) = body_json.get("records").and_then(|v| v.as_array()) {
                let table_rows = st.records.entry(table.to_string()).or_default();
                for r in recs {
                    let id = r.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
                    if let Some(ufields) = r.get("fields").and_then(|v| v.as_object()) {
                        for (rid, rfields) in table_rows.iter_mut() {
                            if *rid == id {
                                for (k, v) in ufields {
                                    rfields.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
            }
            (200, json!({}))
        } else if segs.len() == 3 && segs[1] == "data" && segs[2] == "delete" && method == "POST" {
            if let Some(ids) = body_json.as_array() {
                let del_ids: Vec<i64> = ids.iter().filter_map(|v| v.as_i64()).collect();
                if let Some(table_rows) = st.records.get_mut(table) {
                    table_rows.retain(|(rid, _)| !del_ids.contains(rid));
                }
            }
            (200, json!({}))
        } else {
            (404, json!({"error": "not found"}))
        }
    } else {
        (404, json!({"error": "not found"}))
    };

    let body_str = serde_json::to_string(&resp_val).unwrap();
    let status_text = if status == 200 { "OK" } else { "Not Found" };
    let resp = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body_str}",
        body_str.len()
    );
    let _ = stream.write_all(resp.as_bytes());
}

fn grist_rs() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_grist"))
}

fn run_bin(bin: &Path, args: &[&str], env: &[(String, String)], cwd: &Path) -> (i32, String, String) {
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to execute binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn copy_dir_all(src: &Path, dst: &Path) {
    let _ = std::fs::remove_dir_all(dst);
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap().flatten() {
        let ty = entry.file_type().unwrap();
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path);
        } else {
            std::fs::copy(entry.path(), dest_path).unwrap();
        }
    }
}

fn collect_files(dir: &Path, base: &Path, out: &mut Map<String, Value>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut list: Vec<_> = entries.flatten().collect();
    list.sort_by_key(|e| e.file_name());
    for entry in list {
        let path = entry.path();
        if path.file_name().and_then(|s| s.to_str()) == Some(".git") {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, base, out);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            let rel = path.strip_prefix(base).unwrap().to_string_lossy().into_owned();
            out.insert(rel, Value::String(text));
        }
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(status.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&status.stderr));
}

#[test]
fn api_and_management_commands() {
    let server = FakeServer::start();
    let tmp = std::env::temp_dir().join(format!("grist-eq-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let env_file = tmp.join(".env");
    std::fs::write(
        &env_file,
        format!("GRIST_API_KEY={SECRET_KEY}\nGRIST_BASE_URL={}\nGRIST_DOC_ID=au1EbKLtZbrW\n", server.url),
    )
    .unwrap();

    let work = tmp.join("work");
    let backup = tmp.join("backup");
    std::fs::create_dir_all(&work).unwrap();
    let proj = work.join("proj");
    std::fs::create_dir_all(&proj).unwrap();

    let note = work.join("note.md");
    std::fs::write(&note, "# 📍\n\n# 📆\n\n🔖\n").unwrap();
    std::fs::write(
        proj.join("AGENTS.md"),
        "## 작업 상태\n- Table7 Raw ID: 1\n- Table26 task Raw ID: 1, 2\n",
    )
    .unwrap();
    std::fs::write(proj.join("package.json"), "{\"version\": \"0.1.2\"}\n").unwrap();
    std::fs::write(
        proj.join("Input.md"),
        "# 요구 사항\n첫 요구\n\n# 세부 사항\n- 세부 1\n",
    )
    .unwrap();
    let new_input = work.join("new_input.md");
    std::fs::write(&new_input, "# 요구 사항\n새 요구 내용\n\n# 세부 사항\n- 항목 1\n").unwrap();
    let goal_file = work.join("goal1.md");
    std::fs::write(
        &goal_file,
        "- [기능] flag 파일을 읽으면 ▷ ok 출력\n- [기능] 기록 형식이 ▷ 세 줄\n",
    )
    .unwrap();
    std::fs::write(
        proj.join("t.sh"),
        "flag=$(cat flag 2>/dev/null | tr -d '\\r\\n')\n[ \"$flag\" = \"ok\" ] || { echo \"t.sh:1: ASSERT FAIL: flag='$flag'\"; exit 1; }\necho 'flag ok'\n",
    )
    .unwrap();
    std::fs::write(
        proj.join("h.sh"),
        "val=$(cat flag 2>/dev/null | tr -d '\\r\\n')\n[ -n \"$val\" ] || { echo \"h.sh:1: ASSERT FAIL: val='$val'\"; exit 1; }\necho 'not empty'\n",
    )
    .unwrap();
    git(&proj, &["init", "-q"]);
    git(&proj, &["config", "user.email", "t@example.com"]);
    git(&proj, &["config", "user.name", "t"]);
    git(&proj, &["add", "-A"]);
    git(&proj, &["commit", "-qm", "init"]);

    let empty_jev_env = tmp.join("empty_jev.env");
    std::fs::write(&empty_jev_env, "").unwrap();
    let base_env = vec![
        ("GRIST_ENV_FILE".to_string(), env_file.to_string_lossy().into_owned()),
        ("GRIST_OBSIDIAN_FILE".to_string(), note.to_string_lossy().into_owned()),
        ("GRIST_EXTENSIONS_ROOT".to_string(), work.to_string_lossy().into_owned()),
        ("JEV_ENV_FILE".to_string(), empty_jev_env.to_string_lossy().into_owned()),
        ("JEV_MODE".to_string(), "off".to_string()),
        ("CODEX_THREAD_ID".to_string(), String::new()),
    ];

    let tmp_s = tmp.to_string_lossy().into_owned();
    let proj_s = proj.to_string_lossy().into_owned();
    let new_input_s = new_input.to_string_lossy().into_owned();
    let goal_file_s = goal_file.to_string_lossy().into_owned();

    let norm_str = |s: &str| -> String {
        let mut out = s.replace(&tmp_s, "{TMP}");
        while let Some(idx) = out.find("\"base\": \"") {
            let start = idx + 9;
            if out.len() >= start + 40 && out[start..start + 40].chars().all(|c| c.is_ascii_hexdigit()) {
                out.replace_range(start..start + 40, "{SHA}");
            } else {
                break;
            }
        }
        out
    };
    let norm_files = |f: &Map<String, Value>| -> Map<String, Value> {
        f.iter()
            .map(|(k, v)| {
                let nv = v.as_str().map(|s| Value::String(norm_str(s))).unwrap_or_else(|| v.clone());
                (k.clone(), nv)
            })
            .collect()
    };

    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    let fixture_path = fixture_dir.join("expected.json");
    let update_expected = std::env::var("UPDATE_EXPECTED").ok().as_deref() == Some("1");
    let mut recorded: Vec<Value> = if !update_expected && fixture_path.is_file() {
        serde_json::from_str(&std::fs::read_to_string(&fixture_path).unwrap()).unwrap()
    } else {
        Vec::new()
    };
    let mut step_idx = 0usize;

    let mut assert_equiv = |args: &[&str]| {
        copy_dir_all(&work, &backup);
        let state_before = server.get_state();

        let update_expected = std::env::var("UPDATE_EXPECTED").ok().as_deref() == Some("1");
        let (code_exp, out_exp, err_exp, files_exp, records_exp, columns_exp) = if update_expected {
            let (c, o, e) = run_bin(&grist_rs(), args, &base_env, &proj);
            let o_norm = norm_str(&o);
            let e_norm = norm_str(&e);
            let st = server.get_state();
            let mut f = Map::new();
            collect_files(&work, &work, &mut f);
            let f_norm = norm_files(&f);
            let rec_json = serde_json::to_value(&st.records).unwrap();
            let col_json = serde_json::to_value(&st.columns).unwrap();
            recorded.push(json!({
                "code": c,
                "out": o_norm,
                "err": e_norm,
                "files": f_norm,
                "records": rec_json,
                "columns": col_json,
            }));
            std::fs::write(&fixture_path, serde_json::to_string(&recorded).unwrap()).unwrap();
            copy_dir_all(&backup, &work);
            server.set_state(state_before);
            (c, o_norm, e_norm, f_norm, rec_json, col_json)
        } else {
            let snap = &recorded[step_idx];
            step_idx += 1;
            (
                snap["code"].as_i64().unwrap() as i32,
                snap["out"].as_str().unwrap().to_string(),
                snap["err"].as_str().unwrap().to_string(),
                snap["files"].as_object().unwrap().clone(),
                snap["records"].clone(),
                snap["columns"].clone(),
            )
        };

        let (code_rs, out_rs, err_rs) = run_bin(&grist_rs(), args, &base_env, &proj);
        let out_rs_norm = norm_str(&out_rs);
        let err_rs_norm = norm_str(&err_rs);
        let state_rs = server.get_state();
        let mut files_rs = Map::new();
        collect_files(&work, &work, &mut files_rs);
        let files_rs_norm = norm_files(&files_rs);

        assert_eq!(code_rs, code_exp, "exit code mismatch for {:?}\nexp stderr: {}\nrs stderr: {}", args, err_exp, err_rs_norm);
        assert_eq!(out_rs_norm, out_exp, "stdout mismatch for {:?}", args);
        assert_eq!(err_rs_norm, err_exp, "stderr mismatch for {:?}", args);
        assert_eq!(files_rs_norm, files_exp, "files mismatch after {:?}", args);
        assert_eq!(serde_json::to_value(&state_rs.records).unwrap(), records_exp, "server records mismatch after {:?}", args);
        assert_eq!(serde_json::to_value(&state_rs.columns).unwrap(), columns_exp, "server columns mismatch after {:?}", args);
        assert!(!out_rs.contains(SECRET_KEY), "API key leaked on stdout for {:?}", args);
        assert!(!err_rs.contains(SECRET_KEY), "API key leaked on stderr for {:?}", args);
    };

    // 1. Basic CRUD and shortcuts
    assert_equiv(&["tables"]);
    assert_equiv(&["columns", "Table26"]);
    assert_equiv(&["get", "Table26"]);
    assert_equiv(&["get", "Table26", "--id", "1", "--fields", "H,D"]);
    assert_equiv(&["get", "Table26", "--filter", "{\"H\": [\"초안\"]}", "--fields", "B,H"]);
    assert_equiv(&["project", "1"]);
    assert_equiv(&["task", "1"]);
    assert_equiv(&["add", "Table7", "--fields", "{\"Obsidian_Id\": \"extra.md\"}"]);
    assert_equiv(&["update", "Table7", "--id", "10", "--fields", "{\"Obsidian_Id\": \"extra2.md\"}"]);
    assert_equiv(&["delete", "Table7", "--id", "10"]);
    assert_equiv(&["column-update", "Table26", "D", "--fields", "{\"label\": \"메모2\"}"]);
    assert_equiv(&["task-update", "1", "--fields", "{\"H\": \"완료\"}"]); // validation fail (exit 2)
    assert_equiv(&["task-update", "1", "--fields", "{\"K\": \"직접쓰기\"}"]); // validation fail (exit 2)

    // 2. Management records: request-record & task-add
    assert_equiv(&[
        "request-record",
        "--project",
        &proj_s,
        "--input-file",
        &new_input_s,
        "--title",
        "전역규칙원본동기화",
        "--summary",
        "전역 규칙과 Rust 스크립트 정리",
    ]);
    assert_equiv(&[
        "task-add",
        "--fields",
        "{\"B\": \"추가 작업\", \"H\": \"초안\"}",
        "--project",
        &proj_s,
        "--type",
        "기능 추가",
    ]);

    // 3. Stage gate lifecycle: task-goal -> task-red -> task-submit -> task-hidden -> flow-wait -> task-complete
    assert_equiv(&[
        "task-goal",
        "1",
        "--project",
        &proj_s,
        "--goal-file",
        &goal_file_s,
        "--summary",
        "플래그 읽기와 세 줄 기록",
    ]);
    git(&proj, &["add", "-A"]);
    git(&proj, &["commit", "-qm", "after goal"]);

    // Reject command-like purpose
    assert_equiv(&[
        "task-red",
        "1",
        "--project",
        &proj_s,
        "--purpose",
        "`sh t.sh`",
        "--test",
        "sh t.sh",
        "--tests",
        "t.sh",
        "--paths",
        "flag",
    ]);

    assert_equiv(&[
        "task-red",
        "1",
        "--project",
        &proj_s,
        "--purpose",
        "플래그 읽기",
        "--test",
        "sh t.sh",
        "--tests",
        "t.sh",
        "--paths",
        "flag",
    ]);

    // Check 3-line record in changes and .ai/gate/1.json
    let gate1: Value = serde_json::from_str(&std::fs::read_to_string(proj.join(".ai/gate/1.json")).unwrap()).unwrap();
    assert_eq!(gate1["red"]["purpose"], "플래그 읽기");
    assert_eq!(gate1["red"]["test"], "sh t.sh");

    // Make test pass and run task-submit
    std::fs::write(proj.join("flag"), "ok\n").unwrap();
    assert_equiv(&["task-submit", "1", "--project", &proj_s]);

    // Hidden case (failing first, then passing)
    std::fs::write(proj.join("flag"), "\n").unwrap();
    assert_equiv(&[
        "task-hidden",
        "1",
        "--project",
        &proj_s,
        "--purpose",
        "빈 플래그 거부",
        "--test",
        "sh h.sh",
    ]);
    std::fs::write(proj.join("flag"), "ok\n").unwrap();
    assert_equiv(&[
        "task-hidden",
        "1",
        "--project",
        &proj_s,
        "--purpose",
        "빈 플래그 거부",
        "--test",
        "sh h.sh",
    ]);

    // flow-wait: first with failing flow judgment, then succeeding
    let changes_dir = proj.join(".ai/changes");
    let rec_file = std::fs::read_dir(&changes_dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .unwrap();
    let append_rec = |extra: &str| {
        let mut cur = std::fs::read_to_string(&rec_file).unwrap();
        cur.push_str(extra);
        std::fs::write(&rec_file, cur).unwrap();
    };
    append_rec("\n#### 확인 (#1)\n- [x] 생성본 재생성\n  결과 : 실패\n  상세 : 미비\n");
    assert_equiv(&["flow-wait", "--project", &proj_s, "--waiting", "사용자 확인"]);

    append_rec("\n#### 확인 (#1)\n- [x] 생성본 재생성\n  결과 : 성공\n  상세 : 동기화됨\n");
    std::fs::write(proj.join("package.json"), "{\"version\": \"0.1.3\"}\n").unwrap();
    assert_equiv(&["flow-wait", "--project", &proj_s, "--waiting", "사용자 확인"]);

    // task-complete: first without #### 완료 (#1), then with #### 완료 (#1)
    assert_equiv(&["task-complete", "1", "--project", &proj_s]);

    append_rec(
        "\n#### 완료 (#1)\n1. [⚙️] flag 파일 읽기와 세 줄 기록 완료\n",
    );
    assert_equiv(&["task-complete", "1", "--project", &proj_s]);

    // Verify changes record contains only `- 목적 :`, `- 결과 :`, `- 상세 :` under 초안/검증/확인 and never `sh t.sh`
    let rec_text = std::fs::read_to_string(&rec_file).unwrap();
    assert!(!rec_text.contains("sh t.sh"), "changes record must not contain test command:\n{}", rec_text);
    assert!(rec_text.contains("- 목적 : 플래그 읽기"));
    assert!(rec_text.contains("- 목적 : 빈 플래그 거부"));

    // 4. gate & sync equivalence
    assert_equiv(&["gate", "--cwd", &proj_s]);
    assert_equiv(&["gate", "--cwd", &proj_s, "--row", "1"]);
    assert_equiv(&["sync", "--project", &proj_s]);

    // 5. Variation: another Raw ID (77) and another project directory (`proj_var`)
    {
        let mut st = server.get_state();
        st.records.get_mut("Table26").unwrap().push((
            77,
            serde_json::from_value(json!({
                "B": "변형 작업",
                "G": "기능",
                "H": "초안",
                "D": "",
                "I": "",
                "J": "",
                "K": ""
            }))
            .unwrap(),
        ));
        server.set_state(st);
    }
    let proj_var = work.join("proj_var");
    std::fs::create_dir_all(&proj_var).unwrap();
    std::fs::write(
        proj_var.join("AGENTS.md"),
        "## 작업 상태\n- Table7 Raw ID: 1\n- Table26 task Raw ID: 77\n",
    )
    .unwrap();
    std::fs::write(proj_var.join("Input.md"), "# 요구 사항\n변형 요구\n\n# 세부 사항\n- 변형 세부\n").unwrap();
    let proj_var_s = proj_var.to_string_lossy().into_owned();
    assert_equiv(&[
        "task-goal",
        "77",
        "--project",
        &proj_var_s,
        "--goal-file",
        &goal_file_s,
        "--summary",
        "변형 행 요약",
    ]);
    assert_equiv(&["gate", "--cwd", &proj_var_s]);

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn gate_uninitialized_and_hidden_cases() {
    let tmp = std::env::temp_dir().join(format!("grist-uninit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    git(&tmp, &["init", "-q"]);
    git(&tmp, &["config", "user.email", "t@example.com"]);
    git(&tmp, &["config", "user.name", "t"]);

    for (name, header) in [
        ("alpha", ""),
        ("beta", "## 작업 상태\n- Table7 Raw ID: 1\n"),
        ("gamma", ""),
    ] {
        let dir = tmp.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("AGENTS.md"), format!("{header}# rules\n")).unwrap();
        std::fs::write(dir.join("main.js"), "1\n").unwrap();
    }
    git(&tmp, &["add", "-A"]);
    git(&tmp, &["commit", "-qm", "init"]);

    std::fs::write(tmp.join("alpha/main.js"), "2\n").unwrap();
    std::fs::write(tmp.join("beta/main.js"), "2\n").unwrap();
    std::fs::create_dir_all(tmp.join("delta/src/deep")).unwrap();
    std::fs::write(tmp.join("delta/src/deep/new.js"), "1\n").unwrap();
    std::fs::create_dir_all(tmp.join("test/tmp")).unwrap();
    std::fs::write(tmp.join("test/tmp/x.txt"), "x\n").unwrap();

    let env = vec![
        ("GRIST_EXTENSIONS_ROOT".to_string(), tmp.to_string_lossy().into_owned()),
        ("GRIST_NO_HOOK".to_string(), "1".to_string()),
    ];

    for (rel, want_uninit) in [
        ("alpha", Some("alpha")),
        ("beta", None),
        ("gamma", None),
        ("delta/src/deep", Some("delta")),
        ("", None),
        ("test/tmp", None),
    ] {
        let target = if rel.is_empty() { tmp.clone() } else { tmp.join(rel) };
        let target_s = target.to_string_lossy().into_owned();
        let (c_rs, o_rs, _e_rs) = run_bin(&grist_rs(), &["gate", "--cwd", &target_s], &env, &tmp);
        assert_eq!(c_rs, 0);
        let parsed: Value = serde_json::from_str(&o_rs).unwrap();
        let got_uninit: Option<String> = parsed
            .get("uninitialized")
            .and_then(|v| v.as_str())
            .map(|p| Path::new(p).file_name().unwrap().to_string_lossy().into_owned());
        assert_eq!(got_uninit.as_deref(), want_uninit, "uninitialized mismatch on {rel}");
    }

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn records_rerun_failure_and_secret_key_protection() {
    let server = FakeServer::start();
    let tmp = std::env::temp_dir().join(format!("grist-records-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let env_file = tmp.join(".env");
    std::fs::write(
        &env_file,
        format!("GRIST_API_KEY={SECRET_KEY}\nGRIST_BASE_URL={}\nGRIST_DOC_ID=au1EbKLtZbrW\n", server.url),
    )
    .unwrap();
    let note = tmp.join("note.md");
    std::fs::write(&note, "# 📍\n\n# 📆\n\n🔖\n").unwrap();

    let proj = tmp.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("AGENTS.md"),
        "## 작업 상태\n- Table7 Raw ID: 1\n- Table26 task Raw ID: 1, 2\n",
    )
    .unwrap();
    std::fs::write(
        proj.join("t.sh"),
        "flag=$(cat flag 2>/dev/null | tr -d '\\r\\n')\n[ \"$flag\" = \"ok\" ] || { echo \"t.sh:1: ASSERT FAIL: flag='$flag'\"; exit 1; }\necho 'flag ok'\n",
    )
    .unwrap();
    std::fs::write(
        proj.join("h.sh"),
        "val=$(cat flag 2>/dev/null | tr -d '\\r\\n')\n[ -n \"$val\" ] || { echo \"h.sh:1: ASSERT FAIL: val='$val'\"; exit 1; }\necho 'not empty'\n",
    )
    .unwrap();
    let goal_file = tmp.join("goal.md");
    std::fs::write(
        &goal_file,
        "- [기능] flag 파일을 읽으면 ▷ ok\n- [기능] 기록이 ▷ 세 줄\n",
    )
    .unwrap();
    git(&proj, &["init", "-q"]);
    git(&proj, &["config", "user.email", "t@example.com"]);
    git(&proj, &["config", "user.name", "t"]);
    git(&proj, &["add", "-A"]);
    git(&proj, &["commit", "-qm", "init"]);

    let empty_jev_env = tmp.join("empty_jev.env");
    std::fs::write(&empty_jev_env, "").unwrap();
    let env = vec![
        ("GRIST_ENV_FILE".to_string(), env_file.to_string_lossy().into_owned()),
        ("GRIST_OBSIDIAN_FILE".to_string(), note.to_string_lossy().into_owned()),
        ("GRIST_EXTENSIONS_ROOT".to_string(), tmp.to_string_lossy().into_owned()),
        ("JEV_ENV_FILE".to_string(), empty_jev_env.to_string_lossy().into_owned()),
        ("JEV_MODE".to_string(), "off".to_string()),
        ("CODEX_THREAD_ID".to_string(), String::new()),
    ];
    let proj_s = proj.to_string_lossy().into_owned();
    let goal_s = goal_file.to_string_lossy().into_owned();

    // Goal + Red
    let (c, _, e) = run_bin(&grist_rs(), &["task-goal", "1", "--project", &proj_s, "--goal-file", &goal_s, "--summary", "요약"], &env, &proj);
    assert_eq!(c, 0, "task-goal failed: {e}");
    git(&proj, &["add", "-A"]);
    git(&proj, &["commit", "-qm", "goal"]);

    let (c, _, e) = run_bin(
        &grist_rs(),
        &["task-red", "1", "--project", &proj_s, "--purpose", "플래그 읽기", "--test", "sh t.sh", "--tests", "t.sh", "--paths", "flag"],
        &env,
        &proj,
    );
    assert_eq!(c, 0, "task-red failed: {e}");

    // Submit + Hidden + Flow rerun failure writes purpose (not command) to $I
    std::fs::write(proj.join("flag"), "ok\n").unwrap();
    assert_eq!(run_bin(&grist_rs(), &["task-submit", "1", "--project", &proj_s], &env, &proj).0, 0);
    assert_eq!(
        run_bin(
            &grist_rs(),
            &["task-hidden", "1", "--project", &proj_s, "--purpose", "빈 플래그 거부", "--test", "sh h.sh"],
            &env,
            &proj
        )
        .0,
        0
    );

    let changes_dir = proj.join(".ai/changes");
    let rec_file = std::fs::read_dir(&changes_dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .unwrap();
    let mut cur = std::fs::read_to_string(&rec_file).unwrap();
    cur.push_str("\n#### 확인 (#1)\n- [x] 생성본 재생성\n  결과 : 성공\n  상세 : 네 생성본 동기화됨\n");
    std::fs::write(&rec_file, cur).unwrap();

    // Break hidden test and run flow-wait
    std::fs::write(proj.join("h.sh"), "echo 'h.sh:1: ASSERT FAIL: branch=empty'; exit 1\n").unwrap();
    let (c, _, _) = run_bin(&grist_rs(), &["flow-wait", "--project", &proj_s, "--waiting", "사용자 확인"], &env, &proj);
    assert_eq!(c, 2);

    let st = server.get_state();
    let row1 = &st.records["Table26"].iter().find(|(id, _)| *id == 1).unwrap().1;
    let i_cell = row1.get("I").and_then(|v| v.as_str()).unwrap_or("");
    assert!(i_cell.starts_with("1. 메인 재실행 실패: 빈 플래그 거부 / "), "unexpected $I: {i_cell}");
    assert!(i_cell.contains("branch=empty"), "unexpected $I: {i_cell}");
    assert!(!i_cell.contains("sh h.sh"), "$I must not contain command: {i_cell}");
    assert_eq!(row1.get("H").and_then(|v| v.as_str()), Some("진단"));

    // Connection failure must not leak API key
    let bad_env = vec![
        ("GRIST_ENV_FILE".to_string(), env_file.to_string_lossy().into_owned()),
        ("GRIST_BASE_URL".to_string(), "http://127.0.0.1:9".to_string()),
    ];
    let (c_rs, _o_rs, e_rs) = run_bin(&grist_rs(), &["tables"], &bad_env, &proj);
    assert_eq!(c_rs, 2);
    assert!(e_rs.contains("\"boundary\": \"connection\""), "stderr: {e_rs}");
    assert!(!e_rs.contains(SECRET_KEY));

    let _ = std::fs::remove_dir_all(&tmp);
}
