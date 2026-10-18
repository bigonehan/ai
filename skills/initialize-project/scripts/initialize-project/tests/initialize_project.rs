//! Equivalence tests: the Rust binary must reproduce the fixtures recorded from
//! the original initialize-project script (stdout, stderr, exit code, Grist
//! requests, written files). Node is never run here.
mod common;

use common::*;
use serde_json::Value;

#[test]
fn new_project() {
    let run = run_case("new_project");
    assert_eq!(run.code, 0);
    assert_matches_fixture("new_project", &run);
}

#[test]
fn new_project_other_name_keeps_existing_files() {
    let run = run_case("new_project_other");
    assert_matches_fixture("new_project_other", &run);
}

#[test]
fn already_initialized() {
    let run = run_case("already_initialized");
    assert!(run.requests.iter().all(|r| r["method"] == "GET"), "no Table7 row is added");
    assert_matches_fixture("already_initialized", &run);
}

#[test]
fn rerun_after_new_project_is_idempotent() {
    let spec = load_case("new_project");
    let root = prepare("rerun", &spec);
    let mock = MockGrist::start(&spec["mock"]);
    let first = run_in(&root, &spec, &mock);
    assert_eq!(first.code, 0, "first run: {}", first.stderr);
    let second = run_in(&root, &spec, &mock);
    assert_eq!(second.code, 0, "second run: {}", second.stderr);
    let posts = second.requests.iter().filter(|r| r["method"] == "POST").count();
    assert_eq!(posts, 1, "second run adds no row");
    let a: Value = serde_json::from_str(&first.stdout).unwrap();
    let b: Value = serde_json::from_str(&second.stdout).unwrap();
    assert_eq!(a["grist"]["table7"], b["grist"]["table7"]);
    assert_eq!(b["files"]["docs"]["architecture.md"], false);
    assert_eq!(first.tree.get("project/AGENTS.md"), second.tree.get("project/AGENTS.md"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_table7_row() {
    let run = run_case("missing_table7");
    assert_eq!(run.code, 2);
    assert_matches_fixture("missing_table7", &run);
}

#[test]
fn template_rendering_edge_cases() {
    assert_matches_fixture("template_edge_ok", &run_case("template_edge_ok"));
}

#[test]
fn template_rendering_calendar_header_with_text_fails_readback() {
    assert_matches_fixture("template_edge", &run_case("template_edge"));
}

#[test]
fn note_without_headers_fails_readback() {
    assert_matches_fixture("note_missing_headers", &run_case("note_missing_headers"));
}

#[test]
fn missing_template() {
    assert_matches_fixture("missing_template", &run_case("missing_template"));
}

#[test]
fn duplicate_name_with_other_obsidian_path() {
    assert_matches_fixture("duplicate_mismatch", &run_case("duplicate_mismatch"));
}

#[test]
fn argument_errors() {
    assert_matches_fixture("bad_args", &run_case("bad_args"));
    assert_matches_fixture("unknown_arg", &run_case("unknown_arg"));
}

#[test]
fn grist_http_error() {
    assert_matches_fixture("http_error", &run_case("http_error"));
}

#[test]
fn grist_bad_json() {
    assert_matches_fixture("bad_json", &run_case("bad_json"));
}

#[test]
fn grist_transport_error() {
    assert_matches_fixture("transport_error", &run_case("transport_error"));
}

#[test]
fn credentials_from_env_files() {
    let run = run_case("env_file");
    assert!(!run.stdout.contains("file-key") && !run.stderr.contains("file-key"), "key never printed");
    assert_matches_fixture("env_file", &run);
}
