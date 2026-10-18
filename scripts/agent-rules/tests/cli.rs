//! agent-rules CLI를 임시 HOME(AGENT_RULES_HOME)에서 실제로 실행해 원본·활성본을 읽어 확인한다.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const SOURCE: &str = "# A\n- a\n## B\n- b\n# C\n- c\n<!-- @only codex -->\n- codex 전용\n<!-- @end -->\n";
const TARGETS: [&str; 4] = [
    ".codex/AGENTS.md",
    ".claude/CLAUDE.md",
    ".gemini/GEMINI.md",
    ".gemini/antigravity-cli/GEMINI.md",
];

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn home() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("agent-rules-test-{}-{}", std::process::id(), n));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("ai/rules")).unwrap();
    fs::write(dir.join("ai/rules/Agents.md"), SOURCE).unwrap();
    dir
}

fn run(home: &Path, args: &[&str]) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_agent-rules"))
        .args(args)
        .env("AGENT_RULES_HOME", home)
        .output()
        .unwrap();
    println!(
        "$ agent-rules {} → exit {:?}\n{}{}",
        args.join(" "),
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn source(home: &Path) -> String {
    fs::read_to_string(home.join("ai/rules/Agents.md")).unwrap()
}

fn target(home: &Path, rel: &str) -> String {
    fs::read_to_string(home.join(rel)).unwrap_or_default()
}

fn body(text: &str) -> String {
    // 생성 헤더(첫 줄 주석과 빈 줄)를 뺀 본문
    text.splitn(2, "-->\n\n").nth(1).unwrap_or("").to_string()
}

#[test]
fn read_prints_whole_source_and_one_section() {
    let h = home();
    let out = run(&h, &["read"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), SOURCE);
    let out = run(&h, &["read", "--section", "B"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "## B\n- b\n");
}

#[test]
fn sync_writes_all_targets_and_only_blocks() {
    let h = home();
    let out = run(&h, &["sync"]);
    assert!(out.status.success());
    for rel in TARGETS {
        let text = target(&h, rel);
        assert!(text.starts_with("<!--"), "{rel} has generated header");
        assert!(body(&text).contains("## B\n- b\n"), "{rel} has source body");
        assert!(!body(&text).contains("@only"), "{rel} has no markers");
        let codex_only = body(&text).contains("- codex 전용");
        assert_eq!(codex_only, rel == ".codex/AGENTS.md", "{rel} codex-only block");
    }
    let status = run(&h, &["status"]);
    let status = String::from_utf8_lossy(&status.stdout).to_string();
    assert_eq!(status.matches("동기화됨").count(), 4, "status: {status}");
}

#[test]
fn add_replace_remove_write_update_source_and_targets() {
    let h = home();
    assert!(run(&h, &["add", "--section", "B", "--text", "- b2"]).status.success());
    assert!(source(&h).contains("## B\n- b\n- b2\n# C"));
    assert!(run(&h, &["replace", "--old", "- a\n", "--new", "- a2\n"]).status.success());
    assert!(source(&h).starts_with("# A\n- a2\n"));
    assert!(run(&h, &["remove", "--text", "- c"]).status.success());
    assert!(!source(&h).contains("- c\n"));
    for rel in TARGETS {
        let b = body(&target(&h, rel));
        assert!(b.contains("- a2\n## B\n- b\n- b2\n# C\n"), "{rel}: {b}");
        assert!(!b.contains("- c\n"), "{rel}");
    }
    let file = h.join("new.md");
    fs::write(&file, "# 새 원본\n- x\n").unwrap();
    assert!(run(&h, &["write", "--file", file.to_str().unwrap()]).status.success());
    assert_eq!(source(&h), "# 새 원본\n- x\n");
    for rel in TARGETS {
        assert_eq!(body(&target(&h, rel)), "# 새 원본\n- x\n", "{rel}");
    }
}

#[test]
fn replace_and_remove_refuse_missing_or_ambiguous_text() {
    let h = home();
    fs::write(h.join("ai/rules/Agents.md"), "# A\n- 같음\n- 같음\n").unwrap();
    let before = source(&h);
    assert!(!run(&h, &["replace", "--old", "없는문자열", "--new", "x"]).status.success());
    assert!(!run(&h, &["replace", "--old", "- 같음", "--new", "x"]).status.success());
    assert!(!run(&h, &["remove", "--text", "같음"]).status.success());
    assert_eq!(source(&h), before);
}

#[test]
fn sync_keeps_and_backs_up_drifted_target_until_force() {
    let h = home();
    assert!(run(&h, &["sync"]).status.success());
    let claude = h.join(".claude/CLAUDE.md");
    fs::create_dir_all(claude.parent().unwrap()).unwrap();
    fs::write(&claude, "직접 고친 활성본\n").unwrap();
    let out = run(&h, &["sync"]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(target(&h, ".claude/CLAUDE.md"), "직접 고친 활성본\n");
    let backups = fs::read_dir(h.join("ai/rules/.state/backup")).map(|d| d.count()).unwrap_or(0);
    assert_eq!(backups, 1);
    let out = run(&h, &["sync", "--force"]);
    assert!(out.status.success());
    assert_eq!(body(&target(&h, ".claude/CLAUDE.md")), SOURCE.split("<!-- @only").next().unwrap());
}

#[test]
fn changing_one_line_changes_only_that_line_in_targets() {
    let h = home();
    assert!(run(&h, &["sync"]).status.success());
    let before: Vec<String> = TARGETS.iter().map(|r| target(&h, r)).collect();
    assert!(run(&h, &["replace", "--old", "- b\n", "--new", "- 바뀐 b\n"]).status.success());
    for (rel, old) in TARGETS.iter().zip(before) {
        let new = target(&h, rel);
        let diff: Vec<_> = old.lines().zip(new.lines()).filter(|(a, b)| a != b).collect();
        assert_eq!(diff, vec![("- b", "- 바뀐 b")], "{rel}");
    }
}
