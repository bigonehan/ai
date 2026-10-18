//! Runs the Rust hook binary against the fake grist/jev/nf CLIs and compares each run with the
//! output the original Python hook produced for the same input (tests/fixtures/expected.json).
#[path = "../src/json.rs"]
mod json;

use json::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_initialization-gate");
const STATE_ENV: &str = "INITIALIZATION_GATE_STATE_DIR";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load(name: &str) -> Value {
    json::parse(&std::fs::read_to_string(fixtures().join(name)).unwrap()).unwrap()
}

fn find<'a>(doc: &'a Value, name: &str) -> &'a Value {
    match doc.get("cases") {
        Some(Value::Arr(cases)) => cases
            .iter()
            .find(|c| c.get("name").and_then(Value::as_str) == Some(name))
            .unwrap_or_else(|| panic!("case {name} missing")),
        _ => panic!("no cases"),
    }
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path).map(|t| t.lines().map(String::from).collect()).unwrap_or_default()
}

fn str_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Arr(items)) => items.iter().map(|i| i.as_str().unwrap().to_string()).collect(),
        _ => vec![],
    }
}

fn run_case(name: &str) {
    let spec = find(&load("cases.json"), name).clone();
    let expected = find(&load("expected.json"), name).clone();
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{}-{}", BIN.rsplit('/').next().unwrap(), name));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("home")).unwrap();
    let mut transcripts = BTreeMap::new();
    if let Some(Value::Obj(items)) = spec.get("transcripts") {
        for (k, v) in items {
            let p = tmp.join(format!("{k}.jsonl"));
            std::fs::write(&p, format!("{}\n{{\"type\":\"event\"}}\n", v.as_str().unwrap())).unwrap();
            transcripts.insert(k.clone(), p.to_string_lossy().to_string());
        }
    }
    let (Some(Value::Arr(runs)), Some(Value::Arr(want))) = (spec.get("runs"), expected.get("runs")) else { panic!() };
    assert_eq!(runs.len(), want.len());
    for (i, (run, want)) in runs.iter().zip(want).enumerate() {
        let logs: BTreeMap<&str, PathBuf> =
            ["grist", "jev", "nf"].iter().map(|k| (*k, tmp.join(format!("{k}.log")))).collect();
        for p in logs.values() {
            let _ = std::fs::remove_file(p);
        }
        let Some(Value::Obj(event)) = run.get("event") else { panic!() };
        let event: Vec<(String, Value)> = event
            .iter()
            .map(|(k, v)| match v.as_str() {
                Some(s) if s.starts_with("{T:") => (k.clone(), json::s(transcripts[&s[3..s.len() - 1]].clone())),
                _ => (k.clone(), v.clone()),
            })
            .collect();
        let mut cmd = Command::new(BIN);
        for (k, _) in std::env::vars() {
            if k.starts_with("FAKE_") {
                cmd.env_remove(k);
            }
        }
        cmd.env("HOME", tmp.join("home"))
            .env("JEV_MODE", "off")
            .env(STATE_ENV, tmp.join("state"))
            .env("GRIST_BIN", fixtures().join("fake-grist.sh"))
            .env("NF_BIN", fixtures().join("fake-nf.sh"))
            .env("JEV_BIN", if run.get("jev").is_some() { fixtures().join("fake-jev.sh") } else { tmp.join("no-jev") })
            .env("FAKE_GRIST_LOG", &logs["grist"])
            .env("FAKE_JEV_LOG", &logs["jev"])
            .env("FAKE_NF_LOG", &logs["nf"])
            .env("FAKE_GATE_JSON", run.get("gate").map(json::dumps).unwrap_or_default())
            .env("FAKE_JEV_JSON", json::dumps(run.get("jev").unwrap_or(&Value::Obj(vec![]))));
        if let Some(Value::Obj(extra)) = run.get("env") {
            for (k, v) in extra {
                cmd.env(k, v.as_str().unwrap());
            }
        }
        let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(json::dumps(&Value::Obj(event)).as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let ctx = format!("case {name} run {i}");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        println!("{ctx}: {}", stdout.trim_end());
        assert_eq!(out.status.code().map(|c| c.to_string()), want.get("code").map(Value::py_str), "{ctx} exit");
        assert_eq!(stdout, want.get("stdout").and_then(Value::as_str).unwrap(), "{ctx} stdout");
        assert_eq!(String::from_utf8_lossy(&out.stderr), "", "{ctx} stderr");
        let mut states = BTreeMap::new();
        if let Ok(dir) = std::fs::read_dir(tmp.join("state")) {
            for e in dir.flatten() {
                states.insert(e.file_name().to_string_lossy().to_string(), std::fs::read_to_string(e.path()).unwrap());
            }
        }
        let want_states: BTreeMap<String, String> = match want.get("states") {
            Some(Value::Obj(items)) => items.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string())).collect(),
            _ => BTreeMap::new(),
        };
        assert_eq!(states, want_states, "{ctx} state files");
        assert_eq!(lines(&logs["grist"]), str_list(want.get("grist")), "{ctx} grist calls");
        assert_eq!(lines(&logs["jev"]), str_list(want.get("jev")), "{ctx} jev calls");
        let want_nf = str_list(want.get("nf"));
        let start = Instant::now();
        while !want_nf.is_empty() && lines(&logs["nf"]).len() < want_nf.len() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(lines(&logs["nf"]), want_nf, "{ctx} nf notification");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn every_fixture_case_has_a_test() {
    let Some(Value::Arr(cases)) = load("cases.json").get("cases").cloned() else { panic!() };
    let names: Vec<String> = cases.iter().map(|c| c.get("name").unwrap().py_str()).collect();
    let mut listed: Vec<String> = CASES.iter().map(|s| s.to_string()).collect();
    listed.sort();
    let mut names_sorted = names.clone();
    names_sorted.sort();
    assert_eq!(listed, names_sorted);
}

macro_rules! cases {
    ($($name:ident),* $(,)?) => {
        const CASES: &[&str] = &[$(stringify!($name)),*];
        $(#[test] fn $name() { run_case(stringify!($name)); })*
    };
}

cases!(
    claude_stop_blocks_uninitialized_project,
    clean_or_initialized_project_passes,
    block_limit_releases_and_restarts,
    pass_clears_state,
    other_project_restarts_count,
    gate_failure_reports_system_message,
    session_name_is_sanitized,
    missing_cwd_and_session_use_defaults,
);
