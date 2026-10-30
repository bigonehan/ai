use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process;

const DEFAULT_VAULT_ROOT: &str = "/mnt/c/Users/tende/OneDrive/일기장";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum TargetSection {
    #[default]
    Scene,
    Video,
}

#[derive(Debug, Default, Clone)]
struct Query {
    section: TargetSection,
    scene: Option<u32>,
    cut: Option<u32>,
    code_only: bool,
    json: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ExtractedBlock {
    heading: String,
    block: String,
    prompt: String,
}

fn is_target_h1_section(trimmed: &str, section: TargetSection) -> bool {
    let Some(rest) = trimmed.strip_prefix('#') else {
        return false;
    };
    if rest.starts_with('#') {
        return false;
    }
    let title = rest.trim();
    match section {
        TargetSection::Scene => title == "7. 장면" || title == "7.장면",
        TargetSection::Video => {
            title == "8. 영상" || title == "8.영상" || title == "9. 영상" || title == "9.영상"
        }
    }
}

fn is_doc_h1_after_target(trimmed: &str, section: TargetSection) -> bool {
    let Some(rest) = trimmed.strip_prefix('#') else {
        return false;
    };
    if rest.starts_with('#') {
        return false;
    }
    let title = rest.trim_start();
    let digits_end = title.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
    if digits_end == 0 || !title[digits_end..].starts_with('.') {
        return false;
    }
    !is_target_h1_section(trimmed, section)
}

fn parse_numbered_heading(trimmed: &str, prefix: &str) -> Option<u32> {
    let rest = trimmed.strip_prefix(prefix)?;
    if rest.starts_with('#') || !rest.starts_with(' ') {
        return None;
    }
    let rest = rest.trim_start();
    let digits_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    if digits_end == 0 {
        return None;
    }
    let num: u32 = rest[..digits_end].parse().ok()?;
    let after = &rest[digits_end..];
    if after.is_empty() || after.starts_with(" - ") || after == " -" {
        Some(num)
    } else {
        None
    }
}

fn extract_outer_fenced_code(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let first_open = lines
        .iter()
        .position(|line| line.trim_start().starts_with("```"));
    let last_close = lines.iter().rposition(|line| line.trim() == "```");
    match (first_open, last_close) {
        (Some(open_idx), Some(close_idx)) if close_idx > open_idx => lines[open_idx + 1..close_idx]
            .join("\n")
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

fn extract_from_reader<R: BufRead>(reader: R, query: &Query) -> Result<ExtractedBlock, String> {
    let mut in_target_section = false;
    let mut in_target_scene = query.scene.is_none();
    let mut in_target_cut = query.cut.is_none();
    let mut seen_any_cut_in_target_scene = false;

    let mut heading = String::new();
    let mut full_lines: Vec<String> = Vec::new();
    let mut direct_scene_lines: Vec<String> = Vec::new();
    let mut found_section = false;
    let mut found_scene = query.scene.is_none();
    let mut found_cut = query.cut.is_none();

    for line_res in reader.lines() {
        let line = line_res.map_err(|e| format!("파일 읽기 실패: {e}"))?;
        let trimmed = line.trim();

        if !in_target_section {
            if is_target_h1_section(trimmed, query.section) {
                in_target_section = true;
                found_section = true;
                if query.scene.is_none() {
                    heading = trimmed.to_string();
                    full_lines.push(line);
                }
            }
            continue;
        }

        if is_doc_h1_after_target(trimmed, query.section) {
            break;
        }

        if let Some(scene_num) = parse_numbered_heading(trimmed, "##") {
            if let Some(target_scene) = query.scene {
                if scene_num == target_scene {
                    in_target_scene = true;
                    found_scene = true;
                    seen_any_cut_in_target_scene = false;
                    if query.cut.is_none() {
                        heading = trimmed.to_string();
                        full_lines.push(line);
                    }
                    continue;
                } else if in_target_scene {
                    break;
                } else {
                    continue;
                }
            }
        }

        if !in_target_scene {
            continue;
        }

        if let Some(cut_num) = parse_numbered_heading(trimmed, "###") {
            seen_any_cut_in_target_scene = true;
            if let Some(target_cut) = query.cut {
                if cut_num == target_cut {
                    in_target_cut = true;
                    found_cut = true;
                    heading = trimmed.to_string();
                    full_lines.push(line);
                    continue;
                } else if in_target_cut {
                    break;
                } else {
                    continue;
                }
            }
        }

        if query.scene.is_some() && query.cut.is_none() {
            if !seen_any_cut_in_target_scene {
                direct_scene_lines.push(line.clone());
            }
            full_lines.push(line);
        } else if in_target_cut {
            full_lines.push(line);
        }
    }

    if !found_section {
        let label = match query.section {
            TargetSection::Scene => "7. 장면",
            TargetSection::Video => "8. 영상",
        };
        return Err(format!("{label} 구역을 찾지 못했습니다."));
    }
    if !found_scene {
        return Err(format!("씬 {} 블록을 찾지 못했습니다.", query.scene.unwrap_or_default()));
    }
    if !found_cut {
        return Err(format!(
            "씬 {}의 컷 {} 블록을 찾지 못했습니다.",
            query.scene.unwrap_or_default(),
            query.cut.unwrap_or_default()
        ));
    }

    let block = full_lines.join("\n").trim().to_string();
    let prompt_source = if query.scene.is_some() && query.cut.is_none() {
        direct_scene_lines.join("\n")
    } else {
        block.clone()
    };
    let prompt = extract_outer_fenced_code(&prompt_source);

    Ok(ExtractedBlock {
        heading,
        block,
        prompt,
    })
}

fn json_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn resolve_note_path(file_arg: &str, vault_root: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(file_arg);
    if candidate.is_absolute() {
        return Ok(candidate.to_path_buf());
    }
    let root = Path::new(vault_root);
    Ok(root.join(file_arg))
}

fn main() {
    let mut file_arg: Option<String> = None;
    let mut vault_root = env::var("OBSIDIAN_VAULT_ROOT").unwrap_or_else(|_| DEFAULT_VAULT_ROOT.to_string());
    let mut query = Query::default();

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--file" => file_arg = args.next(),
            "--vault-root" => {
                if let Some(root) = args.next() {
                    vault_root = root;
                }
            }
            "--section" => {
                let raw = args.next().unwrap_or_default();
                query.section = match raw.as_str() {
                    "7" | "7. 장면" | "장면" | "scene" => TargetSection::Scene,
                    "8" | "8. 영상" | "9" | "9. 영상" | "영상" | "video" => TargetSection::Video,
                    other => {
                        eprintln!("유효한 섹션(장면|영상|7|8)이 필요합니다: {other}");
                        process::exit(2);
                    }
                };
            }
            "--video" => query.section = TargetSection::Video,
            "--scene" => {
                let raw = args.next().unwrap_or_default();
                query.scene = Some(raw.parse::<u32>().unwrap_or_else(|_| {
                    eprintln!("유효한 씬 번호가 필요합니다: {raw}");
                    process::exit(2);
                }));
            }
            "--cut" => {
                let raw = args.next().unwrap_or_default();
                query.cut = Some(raw.parse::<u32>().unwrap_or_else(|_| {
                    eprintln!("유효한 컷 번호가 필요합니다: {raw}");
                    process::exit(2);
                }));
            }
            "--code-only" => query.code_only = true,
            "--json" => query.json = true,
            other => {
                eprintln!("알 수 없는 인자: {other}");
                process::exit(2);
            }
        }
    }

    if query.cut.is_some() && query.scene.is_none() {
        eprintln!("--cut 옵션은 --scene과 함께 사용해야 합니다.");
        process::exit(2);
    }

    let Some(file_str) = file_arg else {
        eprintln!("사용법: obsidian-scene-block --file <경로> [--vault-root <루트>] [--section <장면|영상>] [--video] [--scene <번호>] [--cut <번호>] [--code-only] [--json]");
        process::exit(2);
    };

    let path = match resolve_note_path(&file_str, &vault_root) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    };

    let file = match File::open(&path) {
        Ok(f) => f,
        Err(err) => {
            eprintln!("Obsidian 문서를 열지 못했습니다 ({}): {err}", path.display());
            process::exit(1);
        }
    };

    let reader = BufReader::new(file);
    let extracted = match extract_from_reader(reader, &query) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    };

    if query.code_only && extracted.prompt.is_empty() {
        eprintln!("대상 블록에 프롬프트 코드블록이 없습니다.");
        process::exit(1);
    }

    if query.json {
        println!(
            "{{\"heading\":\"{}\",\"block\":\"{}\",\"prompt\":\"{}\"}}",
            json_escape(&extracted.heading),
            json_escape(&extracted.block),
            json_escape(&extracted.prompt)
        );
    } else if query.code_only {
        println!("{}", extracted.prompt);
    } else {
        println!("{}", extracted.block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const SAMPLE_DOC: &str = "\
# 0. 정보
많은 앞부분 내용
# 6. 대사
```
대사 내용
```
# 7. 장면

## 1 - 출발선 대치하며

```
# 지시문
## 공통 프롬프트
```
장면유닛 / 기본 / 숲길
```
## 개별 프롬프트
### 1-출발선 대치하며 숲길 경주를…
씬 1 ChatGPT 통합 프롬프트
```

### 1 - 출발선 대치하며

```
# 지시문
### 1-출발선 대치하며 숲길 경주를…
씬 1 컷 1 Gemini 프롬프트
```

### 2 - 출발 신호와 함께

```
# 지시문
### 1-출발 신호와 함께 토끼가…
씬 1 컷 2 Gemini 프롬프트
```

## 2 - 낮잠 자는 토끼가

```
# 지시문
씬 2 ChatGPT 통합 프롬프트
```

# 8. 영상

## 1 - 출발선 대치하며

```
와이드샷으로 시작한다. 컷1 내용, 컷2 내용, 컷3 내용
```

## 2 - 낮잠 자는 토끼가

```
풀샷으로 시작한다. 씬2 컷1 내용, 씬2 컷2 내용, 씬2 컷3 내용
```
";

    #[test]
    fn extracts_scene_chatgpt_code_without_reading_whole_doc() {
        let query = Query {
            section: TargetSection::Scene,
            scene: Some(1),
            cut: None,
            code_only: true,
            json: false,
        };
        let res = extract_from_reader(Cursor::new(SAMPLE_DOC), &query).unwrap();
        assert_eq!(res.heading, "## 1 - 출발선 대치하며");
        assert!(res.prompt.contains("# 지시문"));
        assert!(res.prompt.contains("씬 1 ChatGPT 통합 프롬프트"));
        assert!(!res.block.contains("씬 2 ChatGPT"));
        assert!(!res.block.contains("와이드샷으로 시작한다."));
    }

    #[test]
    fn extracts_cut_gemini_code_only() {
        let query = Query {
            section: TargetSection::Scene,
            scene: Some(1),
            cut: Some(2),
            code_only: true,
            json: false,
        };
        let res = extract_from_reader(Cursor::new(SAMPLE_DOC), &query).unwrap();
        assert_eq!(res.heading, "### 2 - 출발 신호와 함께");
        assert!(res.prompt.contains("씬 1 컷 2 Gemini 프롬프트"));
        assert!(!res.block.contains("씬 1 컷 1"));
    }

    #[test]
    fn extracts_video_section_scene_prompt() {
        let query = Query {
            section: TargetSection::Video,
            scene: Some(1),
            cut: None,
            code_only: true,
            json: false,
        };
        let res = extract_from_reader(Cursor::new(SAMPLE_DOC), &query).unwrap();
        assert_eq!(res.heading, "## 1 - 출발선 대치하며");
        assert_eq!(res.prompt, "와이드샷으로 시작한다. 컷1 내용, 컷2 내용, 컷3 내용");
    }
}
