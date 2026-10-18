// Runs the read-architecture binary on temp copies of tests/fixtures/projects and compares
// stdout and exit code with tests/fixtures/expected, which were recorded from the original
// single-file binary (~/ai/skills/work-project/scripts/read_architecture) on the same inputs.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Case {
    fixture: String,
    cwd: String,
    args: Vec<String>,
}

fn case(name: &str) -> Case {
    let table = fs::read_to_string(fixtures().join("cases.tsv")).unwrap();
    for line in table.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields[0] == name {
            return Case {
                fixture: fields[1].to_owned(),
                cwd: fields[2].to_owned(),
                args: fields[3..].iter().map(|s| s.to_string()).collect(),
            };
        }
    }
    panic!("case {name} not in cases.tsv");
}

fn run_case(name: &str) {
    let case = case(name);
    let base = std::env::temp_dir().join(format!(
        "read-architecture-test-{}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst),
        name
    ));
    let _ = fs::remove_dir_all(&base);
    let project = base.join("proj");
    copy_tree(&fixtures().join("projects").join(&case.fixture), &project);
    if case.fixture == "present" {
        // test/tmp is git-ignored, so the excluded folder is created here.
        fs::create_dir_all(project.join("test/tmp")).unwrap();
        fs::write(project.join("test/tmp/skip.txt"), "alpha\nalpha\n").unwrap();
    }
    let p = project.to_string_lossy().into_owned();
    let args: Vec<String> = case.args.iter().map(|a| a.replace("{P}", &p)).collect();
    let cwd = if case.cwd == "-" {
        project.clone()
    } else {
        project.join(&case.cwd)
    };
    let output = Command::new(env!("CARGO_BIN_EXE_read-architecture"))
        .args(&args)
        .current_dir(&cwd)
        .output()
        .unwrap();
    let canonical = fs::canonicalize(&project)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let stdout = String::from_utf8_lossy(&output.stdout)
        .replace(&p, "<PROJECT>")
        .replace(&canonical, "<PROJECT>");
    let expected = fs::read_to_string(fixtures().join(format!("expected/{name}.stdout"))).unwrap();
    let expected_code: i32 = fs::read_to_string(fixtures().join(format!("expected/{name}.code")))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let _ = fs::remove_dir_all(&base);
    assert_eq!(stdout, expected, "stdout of case {name}");
    assert_eq!(output.status.code(), Some(expected_code), "exit code of case {name}");
}

#[test]
fn doc_present_prints_doc() {
    run_case("present_noquery");
}

#[test]
fn doc_present_from_cwd_without_project() {
    run_case("present_cwd");
}

#[test]
fn doc_present_project_with_dotdot() {
    run_case("present_dotdot");
}

#[test]
fn doc_missing_exits_2() {
    run_case("missing");
}

#[test]
fn project_dir_missing_exits_2() {
    run_case("missing_dir");
}

#[test]
fn empty_template_brace_exits_2() {
    run_case("template_brace");
}

#[test]
fn empty_template_without_list_exits_2() {
    run_case("template_nolist");
}

#[test]
fn structure_difference_reported() {
    run_case("diff");
}

#[test]
fn query_candidates_ranked() {
    run_case("present_query");
}

#[test]
fn query_candidates_limited() {
    run_case("present_limit");
}

#[test]
fn query_bad_limit_falls_back() {
    run_case("present_badlimit");
}

#[test]
fn query_without_match() {
    run_case("present_nomatch");
}
