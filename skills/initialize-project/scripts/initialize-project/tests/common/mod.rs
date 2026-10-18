//! Test harness: fixture cases, a mock Grist server and a file-tree snapshot.
//! Behaviour mirrors the expected outputs recorded from the original
//! initialize-project script.
#![allow(dead_code)]

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub const NOW_MS: &str = "1792292400000";
const TEMPLATE_DIR: &str = "작업/종류/프로젝트/관리/작업관리기";

pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

pub struct MockState {
    rows: Vec<Value>,
    cols: Map<String, Value>,
    fail: Option<Value>,
    pub log: Vec<Value>,
}

pub struct MockGrist {
    pub url: String,
    pub state: Arc<Mutex<MockState>>,
}

impl MockGrist {
    pub fn start(cfg: &Value) -> MockGrist {
        let mut cols = Map::new();
        cols.insert("Table7".into(), cfg["table7_columns"].clone());
        cols.insert("Table26".into(), cfg["table26_columns"].clone());
        let state = Arc::new(Mutex::new(MockState {
            rows: cfg["rows"].as_array().cloned().unwrap_or_default(),
            cols,
            fail: cfg.get("fail").filter(|v| !v.is_null()).cloned(),
            log: Vec::new(),
        }));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let shared = state.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let shared = shared.clone();
                std::thread::spawn(move || serve(stream, &shared));
            }
        });
        MockGrist { url, state }
    }

    pub fn log(&self) -> Vec<Value> {
        self.state.lock().unwrap().log.clone()
    }
}

fn serve(mut stream: TcpStream, state: &Mutex<MockState>) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let method = first.next().unwrap_or("").to_string();
    let path = first.next().unwrap_or("").to_string();
    let mut length = 0usize;
    let mut auth = Value::Null;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            if k == "content-length" {
                length = v.trim().parse().unwrap_or(0);
            } else if k == "authorization" {
                auth = Value::String(v.trim().to_string());
            }
        }
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    let body = if length > 0 { Value::String(String::from_utf8_lossy(&body[..length]).to_string()) } else { Value::Null };
    let (status, text) = handle(state, &method, &path, auth, body);
    let reason = if status == 200 { "OK" } else { "Error" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn handle(state: &Mutex<MockState>, method: &str, path: &str, auth: Value, body: Value) -> (u16, String) {
    let mut s = state.lock().unwrap();
    s.log.push(json!({"method": method, "path": path, "auth": auth, "body": body.clone()}));
    if let Some(fail) = &s.fail {
        if path.contains(fail["path_contains"].as_str().unwrap()) {
            return (fail["status"].as_u64().unwrap() as u16, fail["body"].as_str().unwrap().to_string());
        }
    }
    let not_found = (404, r#"{"error": "not found"}"#.to_string());
    let bare = path.split('?').next().unwrap();
    let parts: Vec<&str> = bare.split('/').collect();
    if parts.len() != 7 || parts[1] != "api" || parts[2] != "docs" || parts[4] != "tables" {
        return not_found;
    }
    let (table, kind) = (parts[5], parts[6]);
    if kind == "columns" && method == "GET" {
        if let Some(cols) = s.cols.get(table) {
            return (200, json!({"columns": cols}).to_string());
        }
    }
    if kind == "records" && table == "Table7" {
        if method == "GET" {
            return (200, json!({"records": s.rows}).to_string());
        }
        if method == "POST" {
            let payload: Value = serde_json::from_str(body.as_str().unwrap()).unwrap();
            let mut ids = Vec::new();
            for rec in payload["records"].as_array().unwrap() {
                let next = s.rows.iter().filter_map(|r| r["id"].as_i64()).max().unwrap_or(0).max(0) + 1;
                s.rows.push(json!({"id": next, "fields": rec["fields"]}));
                ids.push(json!({"id": next}));
            }
            return (200, json!({"records": ids}).to_string());
        }
    }
    not_found
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn temp_root(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("case-{name}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

pub struct Run {
    pub root: PathBuf,
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
    pub requests: Vec<Value>,
    pub tree: BTreeMap<String, Value>,
}

pub fn load_case(name: &str) -> Value {
    let text = fs::read_to_string(fixtures().join("cases").join(name).join("case.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Sets up a fresh root for the case (vault templates, project, home, extra files).
pub fn prepare(name: &str, spec: &Value) -> PathBuf {
    let root = temp_root(name);
    copy_dir(&fixtures().join("vault"), &root.join("vault"));
    fs::create_dir_all(root.join("project")).unwrap();
    fs::create_dir_all(root.join("home")).unwrap();
    for (rel, content) in spec["files"].as_object().unwrap() {
        let full = root.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content.as_str().unwrap()).unwrap();
    }
    root
}

/// Runs the Rust binary for a case inside `root` against `mock`.
pub fn run_in(root: &Path, spec: &Value, mock: &MockGrist) -> Run {
    let r = root.to_string_lossy().to_string();
    let args: Vec<String> = spec["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().replace("<ROOT>", &r)).collect();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_initialize-project"));
    cmd.env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", root.join("home"))
        .env("INITIALIZE_PROJECT_NOW_MS", NOW_MS)
        .env("GRIST_BASE_URL", &mock.url)
        .current_dir(root.join("project"))
        .args(&args);
    for (k, v) in spec["env"].as_object().unwrap() {
        cmd.env(k, v.as_str().unwrap().replace("<ROOT>", &r));
    }
    let out = cmd.output().unwrap();
    let sub = |s: &[u8]| String::from_utf8_lossy(s).replace(&r, "<ROOT>");
    Run {
        root: root.to_path_buf(),
        stdout: sub(&out.stdout),
        stderr: sub(&out.stderr),
        code: out.status.code().unwrap_or(-1),
        requests: mock.log(),
        tree: snapshot(root, &r),
    }
}

pub fn run_case(name: &str) -> Run {
    let spec = load_case(name);
    let root = prepare(name, &spec);
    let mock = MockGrist::start(&spec["mock"]);
    run_in(&root, &spec, &mock)
}

fn walk(root: &Path, dir: &Path, r: &str, out: &mut BTreeMap<String, Value>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap().to_string_lossy().to_string();
        if entry.file_type().unwrap().is_dir() {
            if fs::read_dir(&path).unwrap().next().is_none() {
                out.insert(format!("{rel}/"), Value::Null);
            } else {
                walk(root, &path, r, out);
            }
        } else {
            if rel.starts_with(&format!("vault/{TEMPLATE_DIR}")) {
                continue;
            }
            let text = String::from_utf8_lossy(&fs::read(&path).unwrap())
                .replace("\r\n", "\n")
                .replace(r, "<ROOT>");
            out.insert(rel, Value::String(text));
        }
    }
}

pub fn snapshot(root: &Path, r: &str) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    walk(root, root, r, &mut out);
    out
}

pub fn expected(name: &str, file: &str) -> String {
    fs::read_to_string(fixtures().join("cases").join(name).join("expected").join(file)).unwrap()
}

/// Compares stdout, stderr, exit code, Grist requests and the written files with
/// the fixture recorded from the original mjs.
pub fn assert_matches_fixture(name: &str, run: &Run) {
    assert_eq!(run.stdout, expected(name, "stdout.txt"), "{name}: stdout");
    assert_eq!(run.stderr, expected(name, "stderr.txt"), "{name}: stderr");
    assert_eq!(format!("{}\n", run.code), expected(name, "exit_code.txt"), "{name}: exit code");
    let requests: Vec<Value> = serde_json::from_str(&expected(name, "requests.json")).unwrap();
    assert_eq!(run.requests, requests, "{name}: Grist requests");
    let tree: BTreeMap<String, Value> = serde_json::from_str(&expected(name, "tree.json")).unwrap();
    let got: Vec<&String> = run.tree.keys().collect();
    let want: Vec<&String> = tree.keys().collect();
    assert_eq!(got, want, "{name}: written file list");
    for (k, v) in &tree {
        assert_eq!(&run.tree[k], v, "{name}: file {k}");
    }
    let _ = fs::remove_dir_all(&run.root);
}
