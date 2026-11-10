//! 현재 프로젝트의 Table26 행 중 `확인`·`완료`가 아닌(작업이 남은) 행의
//! 항목(B)·단계(H)·목표·메모(D)·오류(I)·원인(J)을 출력한다.
//!
//! 사용: open-tasks [--project DIR] [--json]

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

const DONE_STAGES: [&str; 2] = ["확인", "완료"];

fn grist_bin() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(&home).join("ai/scripts/bin/grist")
}

/// cwd부터 위로 올라가며 `# 상태` → `### grist` → `ID : N`이 있는 첫 AGENTS.md를 찾는다.
fn find_project(start: &Path) -> Option<(PathBuf, i64)> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if let Ok(text) = std::fs::read_to_string(d.join("AGENTS.md")) {
            if let Some(id) = parse_grist_id(&text) {
                return Some((d.to_path_buf(), id));
            }
        }
        dir = d.parent();
    }
    None
}

fn parse_grist_id(text: &str) -> Option<i64> {
    let mut in_grist = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            in_grist = t.trim_start_matches('#').trim() == "grist";
            continue;
        }
        if in_grist {
            if let Some(rest) = t.strip_prefix("ID") {
                return rest.trim_start().trim_start_matches(':').trim().parse().ok();
            }
        }
    }
    None
}

fn is_open(stage: &str) -> bool {
    !DONE_STAGES.contains(&stage.trim())
}

fn run_grist(args: &[&str]) -> Result<String, String> {
    let out = Command::new(grist_bin())
        .args(args)
        .output()
        .map_err(|e| format!("grist 실행 실패: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().last().unwrap_or("").to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn field(row: &Value, key: &str) -> String {
    match &row["fields"][key] {
        Value::String(s) => s.trim().to_string(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// 목표는 changes 원본(`grist goal`)에서 읽고, 없으면 Grist K 요약으로 대신한다.
fn goal(id: i64, project: &Path, fallback: &str) -> String {
    let id_s = id.to_string();
    let proj = project.to_string_lossy();
    match run_grist(&["goal", &id_s, "--project", &proj]) {
        Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => fallback.to_string(),
    }
}

fn format_text(rows: &[Value]) -> String {
    let mut out = String::new();
    for r in rows {
        out.push_str(&format!("#{} [{}] {}\n", r["id"], r["stage"].as_str().unwrap_or(""), r["item"].as_str().unwrap_or("")));
        for (label, key) in [("목표", "goal"), ("메모", "memo"), ("오류", "error"), ("원인", "cause")] {
            let v = r[key].as_str().unwrap_or("");
            if v.is_empty() {
                continue;
            }
            if v.contains('\n') {
                out.push_str(&format!("  {label}:\n"));
                for l in v.lines() {
                    out.push_str(&format!("    {l}\n"));
                }
            } else {
                out.push_str(&format!("  {label}: {v}\n"));
            }
        }
        out.push('\n');
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("open-tasks [--project DIR] [--json]\n현재 프로젝트 Table26 행 중 $H가 확인·완료가 아닌 행의 항목·단계·목표·메모(D)·오류(I)·원인(J)을 출력한다.\n예: open-tasks --project .");
        return;
    }
    let as_json = args.iter().any(|a| a == "--json");
    let start = args
        .iter()
        .position(|a| a == "--project")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let start = start.canonicalize().unwrap_or(start);

    let Some((project, t7)) = find_project(&start) else {
        eprintln!("open-tasks: `### grist` ID가 있는 AGENTS.md를 찾지 못함: {}", start.display());
        exit(2);
    };
    let filter = json!({ "A": [t7] }).to_string();
    let raw = match run_grist(&["get", "Table26", "--filter", &filter, "--fields", "B,D,H,I,J,K"]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("open-tasks: Table26 조회 실패: {e}");
            exit(1);
        }
    };
    let all: Vec<Value> = serde_json::from_str(&raw).unwrap_or_default();
    let rows: Vec<Value> = all
        .iter()
        .filter(|r| is_open(&field(r, "H")))
        .map(|r| {
            let id = r["id"].as_i64().unwrap_or(0);
            json!({
                "id": id,
                "stage": field(r, "H"),
                "item": field(r, "B"),
                "goal": goal(id, &project, &field(r, "K")),
                "memo": field(r, "D"),
                "error": field(r, "I"),
                "cause": field(r, "J"),
            })
        })
        .collect();

    if as_json {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
    } else if rows.is_empty() {
        println!("작업이 남은 행 없음 (Table7 {t7})");
    } else {
        print!("{}", format_text(&rows));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grist_id_under_state_header() {
        let text = "# 상태\n### grist\nID : 1201\n### obsidian\n문서 : x\n";
        assert_eq!(parse_grist_id(text), Some(1201));
        assert_eq!(parse_grist_id("# 상태\n### obsidian\nID : 3\n"), None);
    }

    #[test]
    fn open_excludes_confirm_and_done_only() {
        for s in ["초안", "진단", "작업", "교정", "검증", ""] {
            assert!(is_open(s), "{s}");
        }
        assert!(!is_open("확인"));
        assert!(!is_open("완료"));
    }

    #[test]
    fn text_format_skips_empty_and_indents_multiline() {
        let rows = vec![json!({"id": 7, "stage": "작업", "item": "항목", "goal": "1. a\n2. b", "memo": "", "error": "e", "cause": ""})];
        assert_eq!(format_text(&rows), "#7 [작업] 항목\n  목표:\n    1. a\n    2. b\n  오류: e\n\n");
    }
}
