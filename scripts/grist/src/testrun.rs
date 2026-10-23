//! Focused test runs. A run keeps a short digest instead of the raw output tail: result lines,
//! passed test names on success, failure lines and a filtered tail on failure, plus the duration.

use crate::util;
use regex::Regex;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Instant;

const NAMES_CAP: usize = 1200;
const TAIL_CAP: usize = 1500;
const FAILURE_LINES: usize = 30;
const LINE_CAP: usize = 300;

pub struct TestRun {
    pub code: i32,
    /// The digest stored in the gate record, printed on failure and sent to J1.
    pub output: String,
    /// The first result line (`34 passed` style) or `-`.
    pub result: String,
    pub ms: u128,
}

impl TestRun {
    /// `· 34 passed · 1.2s` style suffix for a one-line record detail.
    pub fn suffix(&self) -> String {
        if self.result == "-" { format!("· {}", secs(self.ms)) } else { format!("· {} · {}", self.result, secs(self.ms)) }
    }
}

pub fn secs(ms: u128) -> String {
    format!("{}.{}s", ms / 1000, (ms % 1000) / 100)
}

pub fn run(command: &str, project_dir: &Path) -> TestRun {
    let started = Instant::now();
    let out = Command::new("/bin/sh").arg("-c").arg(command).current_dir(project_dir).stdin(Stdio::null()).output();
    let ms = started.elapsed().as_millis();
    match out {
        Ok(out) => {
            let code = out.status.code().unwrap_or(1);
            let (output, result) = digest(code, &String::from_utf8_lossy(&out.stdout), &String::from_utf8_lossy(&out.stderr));
            TestRun { code, output, result, ms }
        }
        Err(e) => TestRun { code: 1, output: e.to_string(), result: "-".into(), ms },
    }
}

fn result_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)test result:|\b\d+ (passed|failed)\b|^\s*tests?:\s+\d|^#\s*(pass|fail|tests)\s+\d|^=+ .*\d+ (passed|failed)").unwrap())
}

fn failure_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"panicked at|assert|Error|error(\[|:)|FAIL|✗|✕|not ok|Traceback|expected|command not found|No such file|Cannot find module|is not defined").unwrap())
}

fn name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^test (\S+) \.\.\. ok$|^\s*(?:✓|✔|ok \d+ -)\s+(.+?)(?:\s+\(\d+(?:\.\d+)?\s*m?s\))?$|^(\S+::\S+) PASSED").unwrap())
}

/// Build output (compile progress, warnings and their source context) that hides the test result.
fn noise(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(Compiling|Finished|Running|Doc-tests|Blocking|Downloaded|Downloading|Updating|Locking|Checking|Fresh)\b|^warning\b|^-->|^\||^\d+\s*\||^= (note|help)|^running \d+ tests?$|^\.\.\.$").unwrap()
    });
    let trimmed = line.trim();
    trimmed.is_empty() || re.is_match(trimmed)
}

fn cap(line: &str) -> String {
    util::head(line.trim_end(), LINE_CAP)
}

/// Digest of one run: `(text, first result line)`. stdout is read before stderr so build noise on
/// stderr never pushes the assertion out of the kept text.
pub fn digest(code: i32, stdout: &str, stderr: &str) -> (String, String) {
    let lines: Vec<&str> = stdout.lines().chain(stderr.lines()).collect();
    let mut results: Vec<String> = Vec::new();
    for line in lines.iter().filter(|l| result_re().is_match(l)) {
        let line = cap(line.trim());
        if !results.contains(&line) { results.push(line); }
    }
    let result = results.first().cloned().unwrap_or_else(|| "-".into());
    let mut text = String::new();
    if !results.is_empty() {
        text.push_str(&format!("결과: {}\n", results.join(" / ")));
    }
    if code == 0 {
        let names: Vec<String> = lines.iter().filter_map(|l| {
            let caps = name_re().captures(l)?;
            caps.iter().skip(1).flatten().next().map(|m| m.as_str().trim().to_string())
        }).collect();
        if !names.is_empty() {
            text.push_str(&format!("통과({}): {}\n", names.len(), util::head(&names.join(", "), NAMES_CAP)));
        }
        if text.is_empty() {
            text = filtered_tail(stdout, stderr, TAIL_CAP / 2);
        }
        return (text.trim_end().to_string(), result);
    }
    let mut failures: Vec<String> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if failures.len() >= FAILURE_LINES { break; }
        let trimmed = line.trim();
        if trimmed.starts_with("warning") || !failure_re().is_match(trimmed) { continue; }
        failures.push(cap(trimmed));
        if trimmed.contains("panicked at") || trimmed.starts_with("error[") || trimmed.starts_with("error:") {
            if let Some(next) = lines.get(index + 1).map(|l| l.trim()).filter(|l| !l.is_empty()) {
                failures.push(cap(next));
            }
        }
    }
    failures.dedup();
    if !failures.is_empty() {
        text.push_str(&format!("실패:\n{}\n", failures.join("\n")));
    }
    text.push_str(&format!("--- 마지막 출력 ---\n{}", filtered_tail(stdout, stderr, TAIL_CAP)));
    (text.trim_end().to_string(), result)
}

fn filtered_tail(stdout: &str, stderr: &str, n: usize) -> String {
    let keep = |text: &str| text.lines().filter(|l| !noise(l)).map(cap).collect::<Vec<_>>().join("\n");
    let out = keep(stdout);
    let body = if out.trim().is_empty() { keep(stderr) } else { out };
    util::tail(&body, n).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_keeps_result_and_names_without_build_noise() {
        let stdout = "\nrunning 2 tests\ntest gate::tests::a ... ok\ntest task::tests::b ... ok\n\ntest result: ok. 2 passed; 0 failed; 0 ignored\n";
        let stderr = "   Compiling grist v0.0.16\nwarning: unused import\n  --> src/x.rs:1:5\n   |\n1  | use x;\n    Finished `test` profile\n     Running unittests src/lib.rs\n";
        let (text, result) = digest(0, stdout, stderr);
        assert_eq!(result, "test result: ok. 2 passed; 0 failed; 0 ignored");
        assert!(text.contains("통과(2): gate::tests::a, task::tests::b"));
        assert!(!text.contains("Compiling") && !text.contains("warning") && !text.contains("unused import"));
    }

    #[test]
    fn failure_keeps_assertion_even_when_stderr_warnings_are_long() {
        let stdout = "running 2 tests\ntest a ... ok\ntest b ... FAILED\n\nfailures:\n\n---- b stdout ----\nthread 'b' panicked at src/task.rs:42:9:\nassertion `left == right` failed\n  left: 1\n right: 2\n\ntest result: FAILED. 1 passed; 1 failed\n";
        let stderr = "warning: unused variable\n".repeat(400) + "error: test failed, to rerun pass `--lib`\n";
        let (text, result) = digest(101, stdout, &stderr);
        assert_eq!(result, "test result: FAILED. 1 passed; 1 failed");
        assert!(text.contains("panicked at src/task.rs:42:9:\nassertion `left == right` failed"));
        assert!(text.contains("--- 마지막 출력 ---") && text.contains("right: 2"));
        assert!(!text.contains("unused variable"));
        assert!(text.len() < 2500);
    }

    #[test]
    fn plain_output_without_result_lines_falls_back_to_tail() {
        assert_eq!(digest(0, "ready", ""), ("ready".to_string(), "-".to_string()));
        let (text, _) = digest(127, "", "sh: 1: nope: command not found\n");
        assert!(text.contains("실패:\nsh: 1: nope: command not found"));
    }

    #[test]
    fn node_and_pytest_names_are_collected() {
        let (text, _) = digest(0, "✔ parses input (1.2ms)\nok 2 - writes file\ntests/test_a.py::test_x PASSED\n# pass 2\n", "");
        assert!(text.contains("통과(3): parses input, writes file, tests/test_a.py::test_x"));
        assert!(text.contains("결과: # pass 2"));
    }

    #[test]
    fn run_records_duration_and_suffix() {
        let run = run("printf 'test result: ok. 1 passed'", Path::new("."));
        assert_eq!(run.code, 0);
        assert!(run.suffix().starts_with("· test result: ok. 1 passed · ") && run.suffix().ends_with('s'));
        assert_eq!(secs(12345), "12.3s");
    }
}
