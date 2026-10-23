//! check-residue: deterministic legacy remnant search (replaces the remove-legacy audit agent).
use std::path::Path;
use std::process::{exit, Command};

const USAGE: &str = "usage: check-residue --paths <p1,p2,...> --pattern <regex> [--pattern <regex> ...] [--exclude <glob> ...] [--max <N>]";
const DEFAULT_EXCLUDES: [&str; 5] = ["target", ".git", "node_modules", "**/.ai/changes/**", "**/.ai/dispatch/**"];
const CONTENT_LIMIT: usize = 160;

#[derive(Debug, PartialEq)]
struct Args {
    paths: Vec<String>,
    patterns: Vec<String>,
    excludes: Vec<String>,
    max: usize,
}

fn expand_home(p: &str, home: &str) -> String {
    if p == "~" {
        home.to_string()
    } else if let Some(rest) = p.strip_prefix("~/") {
        format!("{}/{}", home.trim_end_matches('/'), rest)
    } else {
        p.to_string()
    }
}

fn parse_args(argv: &[String], home: &str) -> Result<Args, String> {
    let mut args = Args { paths: vec![], patterns: vec![], excludes: vec![], max: 20 };
    let mut it = argv.iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("missing value for {flag}"));
        match flag.as_str() {
            "--paths" => {
                for p in value()?.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                    args.paths.push(expand_home(p, home));
                }
            }
            "--pattern" => args.patterns.push(value()?),
            "--exclude" => args.excludes.push(value()?),
            "--max" => {
                let v = value()?;
                args.max = v.parse().map_err(|_| format!("invalid --max: {v}"))?;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if args.paths.is_empty() {
        return Err("missing --paths".into());
    }
    if args.patterns.is_empty() {
        return Err("missing --pattern".into());
    }
    Ok(args)
}

fn tilde(s: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return s.to_string();
    }
    if s == home {
        "~".into()
    } else if let Some(rest) = s.strip_prefix(&format!("{home}/")) {
        format!("~/{rest}")
    } else {
        s.to_string()
    }
}

/// Turns one rg `path:line:content` line into the report line.
fn format_hit(raw: &str, home: &str) -> String {
    let mut parts = raw.splitn(3, ':');
    let (path, line, content) = (parts.next().unwrap_or(""), parts.next(), parts.next());
    match (line, content) {
        (Some(line), Some(content)) => {
            let content: String = content.trim().chars().take(CONTENT_LIMIT).collect();
            format!("  {}:{}: {}", tilde(path, home), line, content)
        }
        _ => format!("  {}", tilde(raw.trim(), home)),
    }
}

fn format_section(pattern: &str, hits: &[String], max: usize, home: &str) -> String {
    let mut out = format!("{pattern} : {}\n", hits.len());
    for h in hits.iter().take(max) {
        out.push_str(&format_hit(h, home));
        out.push('\n');
    }
    if hits.len() > max {
        out.push_str(&format!("  … +{} more\n", hits.len() - max));
    }
    out
}

fn rg_args(pattern: &str, args: &Args) -> Vec<String> {
    let mut v: Vec<String> = ["-n", "--no-heading", "--color", "never", "--hidden", "-e", pattern]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for g in DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).chain(args.excludes.iter().cloned()) {
        v.push("-g".into());
        v.push(format!("!{g}"));
    }
    v.push("--".into());
    v.extend(args.paths.iter().cloned());
    v
}

fn search(pattern: &str, args: &Args) -> Result<Vec<String>, String> {
    let out = Command::new("rg")
        .args(rg_args(pattern, args))
        .output()
        .map_err(|e| format!("failed to run rg: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()),
        Some(1) => Ok(vec![]),
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!("rg error: {}", err.lines().next().unwrap_or("unknown")))
        }
    }
}

/// Returns (report, total).
fn run(args: &Args, home: &str) -> Result<(String, usize), String> {
    if let Some(p) = args.paths.iter().find(|p| !Path::new(p).exists()) {
        return Err(format!("path not found: {}", tilde(p, home)));
    }
    let mut report = String::new();
    let mut total = 0;
    for pattern in &args.patterns {
        let hits = search(pattern, args)?;
        total += hits.len();
        report.push_str(&format_section(pattern, &hits, args.max, home));
    }
    report.push_str(&format!("total : {total}\n"));
    Ok((report, total))
}

fn exit_code(total: usize) -> i32 {
    if total == 0 { 0 } else { 1 }
}

fn main() {
    let home = std::env::var("HOME").unwrap_or_default();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv, &home) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("check-residue: {e}");
            eprintln!("{USAGE}");
            exit(2);
        }
    };
    match run(&args, &home) {
        Ok((report, total)) => {
            print!("{report}");
            exit(exit_code(total));
        }
        Err(e) => {
            eprintln!("check-residue: {e}");
            exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sv(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_comma_paths_repeated_flags_and_tilde() {
        let a = parse_args(
            &sv(&["--paths", "~/ai/skills, /tmp/x", "--pattern", "a", "--pattern", "b", "--exclude", "*.md", "--exclude", "x/**", "--max", "5"]),
            "/home/u",
        )
        .unwrap();
        assert_eq!(
            a,
            Args { paths: sv(&["/home/u/ai/skills", "/tmp/x"]), patterns: sv(&["a", "b"]), excludes: sv(&["*.md", "x/**"]), max: 5 }
        );
        assert_eq!(parse_args(&sv(&["--paths", "/x", "--pattern", "a"]), "/h").unwrap().max, 20);
    }

    #[test]
    fn parse_errors() {
        assert_eq!(parse_args(&sv(&["--paths", "/x"]), "/h").unwrap_err(), "missing --pattern");
        assert_eq!(parse_args(&sv(&["--pattern", "a"]), "/h").unwrap_err(), "missing --paths");
        assert!(parse_args(&sv(&["--paths", "/x", "--pattern"]), "/h").unwrap_err().starts_with("missing value"));
        assert!(parse_args(&sv(&["--paths", "/x", "--pattern", "a", "--max", "z"]), "/h").is_err());
        assert!(parse_args(&sv(&["--bogus"]), "/h").is_err());
    }

    #[test]
    fn formats_hits_with_truncation_and_more() {
        let long = "x".repeat(200);
        let hits = sv(&[
            "/home/u/a.md:3:   foo Old.md bar  ",
            &format!("/home/u/b.rs:10:{long}"),
            "/other/c.txt:1:c:d",
        ]);
        let out = format_section("Input\\.md", &hits, 2, "/home/u");
        let expected = format!(
            "Input\\.md : 3\n  ~/a.md:3: foo Old.md bar\n  ~/b.rs:10: {}\n  … +1 more\n",
            "x".repeat(160)
        );
        assert_eq!(out, expected);
        assert_eq!(format_hit("/other/c.txt:1:c:d", "/home/u"), "  /other/c.txt:1: c:d");
        assert_eq!(format_section("p", &[], 20, "/h"), "p : 0\n");
    }

    #[test]
    fn tilde_replacement() {
        assert_eq!(tilde("/home/u/x", "/home/u"), "~/x");
        assert_eq!(tilde("/home/user2/x", "/home/u"), "/home/user2/x");
        assert_eq!(expand_home("~", "/home/u"), "/home/u");
        assert_eq!(exit_code(0), 0);
        assert_eq!(exit_code(3), 1);
    }

    #[test]
    fn real_search_skips_excluded_dirs() {
        let dir = std::env::temp_dir().join(format!("check-residue-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for sub in [".ai/changes", ".ai/docs", "target", "src"] {
            fs::create_dir_all(dir.join(sub)).unwrap();
        }
        fs::write(dir.join("src/a.rs"), "// LEGACY_TOKEN here\nok\n").unwrap();
        fs::write(dir.join(".ai/docs/arch.md"), "LEGACY_TOKEN doc\n").unwrap();
        fs::write(dir.join(".ai/changes/261023.x.md"), "LEGACY_TOKEN log\n").unwrap();
        fs::write(dir.join("target/out.txt"), "LEGACY_TOKEN build\n").unwrap();
        let d = dir.to_string_lossy().to_string();

        let args = parse_args(&sv(&["--paths", &d, "--pattern", "LEGACY_TOKEN", "--pattern", "NEVER_PRESENT_X"]), "/nohome").unwrap();
        let (report, total) = run(&args, "/nohome").unwrap();
        assert_eq!(total, 2, "{report}");
        assert!(!report.contains("changes") && !report.contains("target"), "{report}");
        assert!(report.contains("LEGACY_TOKEN : 2") && report.contains("NEVER_PRESENT_X : 0"), "{report}");
        assert!(report.ends_with("total : 2\n"));
        assert_eq!(exit_code(total), 1);

        let excl = parse_args(&sv(&["--paths", &d, "--pattern", "LEGACY_TOKEN", "--exclude", "*.rs", "--exclude", "**/.ai/docs/**"]), "/nohome").unwrap();
        let (_, total) = run(&excl, "/nohome").unwrap();
        assert_eq!(exit_code(total), 0);

        let missing = parse_args(&sv(&["--paths", &format!("{d}/nope"), "--pattern", "x"]), "/nohome").unwrap();
        assert!(run(&missing, "/nohome").unwrap_err().starts_with("path not found"));

        let bad = parse_args(&sv(&["--paths", &d, "--pattern", "("]), "/nohome").unwrap();
        assert!(run(&bad, "/nohome").unwrap_err().starts_with("rg error"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
