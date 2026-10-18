//! Equivalence tests for validate-layout-contract.
//! Expected stdout/exit codes were recorded from the original validator into
//! tests/fixtures/cases.json; Python is never run here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Case {
    name: String,
    args: Vec<String>,
    contract: Option<Vec<u8>>,
    stdout: String,
    exit_code: i32,
}

// Minimal readers for the fixture file (keeps the crate dependency-free).
fn b64(text: &str) -> Vec<u8> {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        if c == b'=' {
            break;
        }
        let v = table.iter().position(|&t| t == c).expect("base64") as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

#[derive(Debug)]
enum J {
    Null,
    Num(i64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
    Bool,
}

struct P<'a> {
    s: &'a [char],
    i: usize,
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_whitespace() {
            self.i += 1;
        }
    }
    fn value(&mut self) -> J {
        self.ws();
        match self.s[self.i] {
            '{' => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    if self.s[self.i] == '}' {
                        self.i += 1;
                        break;
                    }
                    if self.s[self.i] == ',' {
                        self.i += 1;
                        continue;
                    }
                    let J::Str(k) = self.value() else { panic!() };
                    self.ws();
                    assert_eq!(self.s[self.i], ':');
                    self.i += 1;
                    let v = self.value();
                    items.push((k, v));
                }
                J::Obj(items)
            }
            '[' => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    if self.s[self.i] == ']' {
                        self.i += 1;
                        break;
                    }
                    if self.s[self.i] == ',' {
                        self.i += 1;
                        continue;
                    }
                    items.push(self.value());
                }
                J::Arr(items)
            }
            '"' => {
                self.i += 1;
                let mut out = String::new();
                loop {
                    let c = self.s[self.i];
                    self.i += 1;
                    match c {
                        '"' => break,
                        '\\' => {
                            let e = self.s[self.i];
                            self.i += 1;
                            out.push(match e {
                                'n' => '\n',
                                't' => '\t',
                                'r' => '\r',
                                'b' => '\u{8}',
                                'f' => '\u{c}',
                                'u' => {
                                    let h: String = self.s[self.i..self.i + 4].iter().collect();
                                    self.i += 4;
                                    char::from_u32(u32::from_str_radix(&h, 16).unwrap()).unwrap()
                                }
                                other => other,
                            });
                        }
                        other => out.push(other),
                    }
                }
                J::Str(out)
            }
            'n' => {
                self.i += 4;
                J::Null
            }
            't' => {
                self.i += 4;
                J::Bool
            }
            'f' => {
                self.i += 5;
                J::Bool
            }
            _ => {
                let start = self.i;
                while self.i < self.s.len() && (self.s[self.i] == '-' || self.s[self.i].is_ascii_digit()) {
                    self.i += 1;
                }
                let t: String = self.s[start..self.i].iter().collect();
                J::Num(t.parse().unwrap())
            }
        }
    }
}

fn get<'a>(obj: &'a J, key: &str) -> &'a J {
    let J::Obj(items) = obj else { panic!("object") };
    &items.iter().find(|(k, _)| k == key).expect(key).1
}

fn load_cases() -> Vec<Case> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cases.json");
    let text: Vec<char> = fs::read_to_string(path).expect("fixtures").chars().collect();
    let J::Arr(items) = (P { s: &text, i: 0 }).value() else { panic!() };
    items
        .iter()
        .map(|item| {
            let s = |k: &str| match get(item, k) {
                J::Str(v) => v.clone(),
                other => panic!("{k}: {other:?}"),
            };
            let J::Arr(args) = get(item, "args") else { panic!() };
            let J::Num(code) = get(item, "exit_code") else { panic!() };
            Case {
                name: s("name"),
                args: args.iter().map(|a| match a { J::Str(v) => v.clone(), _ => panic!() }).collect(),
                contract: match get(item, "contract_b64") {
                    J::Str(v) => Some(b64(v)),
                    _ => None,
                },
                stdout: s("stdout"),
                exit_code: *code as i32,
            }
        })
        .collect()
}

fn run_all(binary: &Path, label: &str) {
    assert!(binary.is_file(), "{label}: binary missing at {}", binary.display());
    let cases = load_cases();
    assert!(cases.len() >= 300, "fixture count {}", cases.len());
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("equivalence-{label}"));
    let mut failures = Vec::new();
    for case in &cases {
        let dir = root.join(&case.name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        if let Some(bytes) = &case.contract {
            fs::write(dir.join("contract.json"), bytes).unwrap();
        }
        let out = Command::new(binary).args(&case.args).current_dir(&dir).output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let code = out.status.code().unwrap_or(-1);
        if stdout != case.stdout || code != case.exit_code {
            failures.push(format!(
                "{}: exit {} (want {})\n--- got\n{}--- want\n{}",
                case.name, code, case.exit_code, stdout, case.stdout
            ));
        }
    }
    let _ = fs::remove_dir_all(&root);
    println!("{label}: {}/{} fixture cases match the Python original", cases.len() - failures.len(), cases.len());
    let shown: Vec<_> = failures.iter().take(5).cloned().collect();
    assert!(failures.is_empty(), "{label}: {} mismatches\n{}", failures.len(), shown.join("\n"));
}

#[test]
fn cargo_binary_matches_python_fixtures() {
    run_all(Path::new(env!("CARGO_BIN_EXE_validate-layout-contract")), "cargo-bin");
}

#[test]
fn installed_skill_binary_matches_python_fixtures() {
    let bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bin/validate-layout-contract");
    run_all(&bin, "skill-bin");
}

fn case<'a>(cases: &'a [Case], name: &str) -> &'a Case {
    cases.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("case {name}"))
}

/// Category coverage: the fixture set itself must hold the pass/violation/edge cases the goal requires.
#[test]
fn fixtures_cover_pass_violation_and_edges() {
    let cases = load_cases();
    for (name, code, needle) in [
        ("pass_base", 0, "\"ok\": true"),
        ("pass_edge_exact_tolerance", 0, "\"errors\": []"),
        ("fail_edge_just_over", 1, "differs by 1.001px"),
        ("fail_edge_tiny_over", 1, "differs by 1.000px"),
        ("fail_missing_everything", 1, "name must be non-empty text"),
        ("fail_kind_mismatch", 1, "measures thumbnail, not requested column"),
        ("badjson_05", 2, "Illegal trailing comma"),
        ("cli_missing_file", 2, "[Errno 2]"),
        ("cli_self_test", 0, "out-of-tolerance size rejected"),
    ] {
        let c = case(&cases, name);
        assert_eq!(c.exit_code, code, "{name}");
        assert!(c.stdout.contains(needle), "{name}: {}", c.stdout);
    }
}

/// Behaviour check independent of fixtures: varying the contract values changes the verdict.
#[test]
fn verdict_follows_contract_values() {
    let bin = Path::new(env!("CARGO_BIN_EXE_validate-layout-contract"));
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("variation");
    fs::create_dir_all(&dir).unwrap();
    let make = |target: &str, runtime: &str, tolerance: &str| {
        format!(
            r#"{{"version":1,"targets":[{{"name":"c","requested_kind":"cell","measured_kind":"cell","selector":"td","axis":"height","layout_owner":"tr","conflicting_constraints":[],"before_px":{{"height":10}},"target_px":{{"height":{target}}},"runtime_px":{{"height":{runtime}}},"tolerance_px":{tolerance},"measurement_method":"computedStyle","runtime_context":"ctx"}}]}}"#
        )
    };
    for (target, runtime, tolerance, code, needle) in [
        ("40", "42.5", "2.5", 0, "\"errors\": []"),
        ("40", "42.5", "2.4", 1, "targets[0].height differs by 2.500px (target 40px, measured 42.5px, tolerance 2.4px)"),
        ("40.25", "40", "0", 1, "differs by 0.250px (target 40.25px, measured 40px, tolerance 0px)"),
        ("7", "7", "0", 0, "\"ok\": true"),
        ("7", "-7", "20", 1, "runtime_px.height must be a non-negative finite number"),
    ] {
        let file = dir.join("c.json");
        fs::write(&file, make(target, runtime, tolerance)).unwrap();
        let out = Command::new(bin).arg(&file).output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(code), "{target}/{runtime}/{tolerance}: {stdout}");
        assert!(stdout.contains(needle), "{target}/{runtime}/{tolerance}: {stdout}");
    }
}
