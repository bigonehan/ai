// Start-of-work structure check for manage-project (crate read-architecture).
// Build: cargo build --release -p read-architecture --manifest-path ~/ai/scripts/Cargo.toml
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{self, Command};

fn resolved(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir().unwrap().join(path)
    };
    let mut result = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn option(args: &[String], name: &str, fallback: &str) -> String {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| fallback.to_owned())
}

fn project_from(start: &Path) -> PathBuf {
    let start = resolved(start);
    let mut path = start.as_path();
    loop {
        if path.join("AGENTS.md").exists() {
            return path.to_path_buf();
        }
        match path.parent() {
            Some(parent) if parent != path => path = parent,
            _ => return start,
        }
    }
}

fn fail(reason: &str, project: &Path) -> ! {
    println!("[architecture] {reason}");
    println!("[architecture] project: {}", project.display());
    println!("[architecture] 작업 전에 .ai/docs/architecture.md를 실제 프로젝트 기준으로 작성한다(# 도메인: 항목 : 설명, # 구조: 루트와 2차 폴더, 파일 이름 없이).");
    process::exit(2);
}

fn empty_template(doc: &str) -> bool {
    let brace_line = doc.lines().any(|line| {
        let line = line.trim_end_matches(char::is_whitespace);
        line.starts_with('{') && line.ends_with('}')
    });
    brace_line || !doc.lines().any(|line| line.starts_with("- "))
}

fn structure_part(doc: &str) -> &str {
    let mut offset = 0;
    for line in doc.split_inclusive('\n') {
        if line
            .trim_end_matches(['\r', '\n'])
            .trim_end_matches([' ', '\t'])
            == "# 구조"
        {
            return &doc[offset + line.len()..];
        }
        offset += line.len();
    }
    ""
}

fn label_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-')
}
fn preceding_char(c: u8) -> bool {
    label_char(c) || c == b'/'
}

fn folder_labels(structure: &str) -> Vec<String> {
    let bytes = structure.as_bytes();
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for i in 0..bytes.len() {
        if i > 0 && preceding_char(bytes[i - 1]) {
            continue;
        }
        let mut start = i;
        if bytes[start] == b'`' {
            start += 1;
        }
        if start >= bytes.len() || !label_char(bytes[start]) {
            continue;
        }
        let mut pos = start;
        while pos < bytes.len() {
            while pos < bytes.len() && label_char(bytes[pos]) {
                pos += 1;
            }
            if pos >= bytes.len() || bytes[pos] != b'/' {
                break;
            }
            pos += 1;
            if pos < bytes.len() && label_char(bytes[pos]) {
                continue;
            }
            let end = pos - 1;
            if pos < bytes.len() && bytes[pos] == b'`' {
                pos += 1;
            }
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos < bytes.len() && matches!(bytes[pos], b':' | b',') {
                let label = &structure[start..end];
                if seen.insert(label.to_owned()) {
                    result.push(label.to_owned());
                }
            }
            break;
        }
    }
    result
}

fn agents_obsidian_doc(agents_text: &str) -> Option<String> {
    for line in agents_text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("문서 :") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn search_targets(project: &Path, named_folders: &[String]) -> Vec<String> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    for folder in named_folders {
        let top = folder.split('/').next().unwrap_or(folder.as_str());
        if !top.is_empty()
            && !top.starts_with('.')
            && project.join(top).exists()
            && !project.join(top).join(".ai/docs/architecture.md").exists()
            && seen.insert(top.to_owned())
        {
            targets.push(top.to_owned());
        }
    }
    for doc in ["AGENTS.md", "README.md"] {
        if project.join(doc).exists() && seen.insert(doc.to_owned()) {
            targets.push(doc.to_owned());
        }
    }
    if targets.is_empty() {
        targets.push(".".to_owned());
    }
    targets
}

/// `~/` paths named in backticks under `# 도메인`: sources outside the project that its work edits.
fn ai_sources(doc: &str, home: &Path) -> Vec<PathBuf> {
    let domain = doc.split("\n# 구조").next().unwrap_or("");
    let mut out = Vec::new();
    for (i, part) in domain.split('`').enumerate() {
        if i % 2 == 1 {
            if let Some(rel) = part.strip_prefix("~/") {
                let path = home.join(rel.trim_end_matches('/'));
                if !rel.is_empty() && path.is_dir() && !out.contains(&path) {
                    out.push(path);
                }
            }
        }
    }
    out
}

struct Score {
    hits: u64,
    words: HashSet<String>,
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let project_arg = option(&args, "--project", "");
    let project = if project_arg.is_empty() {
        project_from(&env::current_dir().map_err(|e| e.to_string())?)
    } else {
        resolved(Path::new(&project_arg))
    };
    let limit: usize = option(&args, "--limit", "15")
        .parse()
        .ok()
        .filter(|n| *n > 0)
        .unwrap_or(15);
    let query = option(&args, "--query", "");
    let doc_file = project.join(".ai/docs/architecture.md");
    if !doc_file.exists() {
        fail("missing .ai/docs/architecture.md", &project);
    }
    let doc =
        fs::read_to_string(&doc_file).map_err(|e| format!("architecture.md 읽기 실패: {e}"))?;
    if empty_template(&doc) {
        fail("architecture.md is still the empty template", &project);
    }

    let words: Vec<String> = {
        let mut list = Vec::new();
        let mut seen = HashSet::new();
        for word in query
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|word| word.chars().count() > 1)
        {
            if seen.insert(word.to_owned()) {
                list.push(word.to_owned());
            }
        }
        list
    };

    println!("# {}/.ai/docs/architecture.md", project.display());
    let matched: Vec<&str> = doc
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            line.starts_with('#')
                || (words.is_empty() && trimmed.starts_with("- "))
                || words
                    .iter()
                    .any(|w| line.to_lowercase().contains(&w.to_lowercase()))
        })
        .take(limit * 2)
        .collect();
    println!("{}", matched.join("\n"));

    let perf_file = project.join(".ai/docs/performance.md");
    if !words.is_empty() {
        if let Ok(perf) = fs::read_to_string(&perf_file) {
            let matched_perf: Vec<&str> = perf
                .lines()
                .filter(|line| {
                    !line.trim().is_empty()
                        && !line.contains("해당 사항 없음")
                        && (line.starts_with('#')
                            || words
                                .iter()
                                .any(|w| line.to_lowercase().contains(&w.to_lowercase())))
                })
                .take(limit)
                .collect();
            if matched_perf.iter().any(|l| !l.starts_with('#')) {
                println!("\n# {}/.ai/docs/performance.md", project.display());
                println!("{}", matched_perf.join("\n"));
            }
        }
    }

    let named = folder_labels(structure_part(&doc));
    let missing: Vec<&str> = named
        .iter()
        .map(String::as_str)
        .filter(|name| !project.join(name).exists())
        .collect();
    let named_top: HashSet<&str> = named
        .iter()
        .map(|name| name.split('/').next().unwrap())
        .collect();
    let mut unlisted = Vec::new();
    for entry in fs::read_dir(&project).map_err(|e| format!("project 읽기 실패: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.')
            || matches!(
                name.as_str(),
                "node_modules" | "__pycache__" | ".git" | ".venv" | "venv"
            )
        {
            continue;
        }
        if entry.file_type().map_err(|e| e.to_string())?.is_dir()
            && !named_top.contains(name.as_str())
        {
            unlisted.push(name);
        }
    }
    if !missing.is_empty() || !unlisted.is_empty() {
        println!("\n# 구조 차이 (작업 범위면 architecture.md도 고친다)");
        if !missing.is_empty() {
            println!("- 문서에만 있음: {}", missing.join(", "));
        }
        if !unlisted.is_empty() {
            println!("- 문서에 없음: {}", unlisted.join(", "));
        }
    }

    let agents_file = project.join("AGENTS.md");
    if let Ok(agents_text) = fs::read_to_string(&agents_file) {
        if let Some(obsidian_rel) = agents_obsidian_doc(&agents_text) {
            let vault_root = Path::new("/mnt/c/Users/tende/OneDrive/일기장");
            if vault_root.exists() && !vault_root.join(&obsidian_rel).exists() {
                println!(
                    "\n# Obsidian 문서 경로 경고\n- AGENTS.md 문서 경로가 Vault에 없음: {obsidian_rel}"
                );
            }
        }
    }

    if !words.is_empty() {
        let mut targets = search_targets(&project, &named);
        let home = PathBuf::from(env::var("HOME").unwrap_or_default());
        let home_prefix = format!("{}/", home.display());
        targets.extend(ai_sources(&doc, &home).iter().map(|p| p.display().to_string()));
        let mut score: HashMap<String, Score> = HashMap::new();
        let mut order = Vec::new();
        for word in &words {
            let mut cmd = Command::new("rg");
            cmd.args([
                "-c",
                "-i",
                "-F",
                word,
                "-g",
                "!node_modules",
                "-g",
                "!dist",
                "-g",
                "!build",
                "-g",
                "!target",
                "-g",
                "!.ai",
                "-g",
                "!*.lock",
                "-g",
                "!*-lock.*",
                "-g",
                "!test/tmp",
            ]);
            for target in &targets {
                cmd.arg(target);
            }
            let output = cmd
                .current_dir(&project)
                .output()
                .unwrap_or_else(|e| {
                    println!("[architecture] rg 실행 실패: {e}");
                    process::exit(2)
                });
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Some((file, count)) = line.rsplit_once(':') {
                    let file = file.strip_prefix("./").unwrap_or(file);
                    let file = match file.strip_prefix(home_prefix.as_str()) {
                        Some(rel) if !home_prefix.is_empty() && home_prefix != "/" => format!("~/{rel}"),
                        _ => file.to_owned(),
                    };
                    if !score.contains_key(&file) {
                        order.push(file.clone());
                    }
                    let item = score.entry(file).or_insert_with(|| Score {
                        hits: 0,
                        words: HashSet::new(),
                    });
                    item.hits += count.parse::<u64>().unwrap_or(0);
                    item.words.insert(word.clone());
                }
            }
        }
        order.sort_by(|a, b| {
            let sa = &score[a];
            let sb = &score[b];
            sb.words
                .len()
                .cmp(&sa.words.len())
                .then_with(|| sb.hits.cmp(&sa.hits))
        });
        println!("\n# 작업 후보 파일 (query: {})", words.join(" "));
        if order.is_empty() {
            println!("- 일치 없음: 키워드를 바꾸거나 위 # 구조에서 폴더를 고른다");
        }
        for file in order.iter().take(limit) {
            let item = &score[file];
            println!(
                "- {file} : {}/{}개 키워드, {}줄",
                item.words.len(),
                words.len(),
                item.hits
            );
        }
        println!(
            "다음 단계: 후보 파일에서 `rg -n \"<키워드>\" <파일>`로 위치를 찾고 그 범위만 읽는다."
        );
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("[architecture] {err}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_sources_reads_home_paths_from_domain() {
        let home = env::temp_dir().join(format!("ra-home-{}", process::id()));
        fs::create_dir_all(home.join("ai/skills")).unwrap();
        fs::create_dir_all(home.join("ai/scripts")).unwrap();
        let doc = "# 도메인\n- 외부 원본 : 규칙 `~/ai/rules`, Skill `~/ai/skills`·`~/ai/scripts/`\n# 구조\n- `~/ai/other` : x\n";
        let got: Vec<String> = ai_sources(doc, &home).iter().map(|p| p.strip_prefix(&home).unwrap().display().to_string()).collect();
        println!("ai_sources ▷ {}", got.join(","));
        assert_eq!(got, vec!["ai/skills", "ai/scripts"]);
        fs::create_dir_all(home.join("sub/.ai/docs")).unwrap();
        fs::write(home.join("sub/.ai/docs/architecture.md"), "# 도메인\n").unwrap();
        fs::create_dir_all(home.join("test")).unwrap();
        let targets = search_targets(&home, &["sub".to_owned(), "test".to_owned()]);
        println!("search_targets ▷ {}", targets.join(","));
        assert_eq!(targets, vec!["test"]);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn parses_structure_folders_and_obsidian_doc() {
        let doc = "# 도메인\n- web : 웹\n# 구조\n- src/ : 프런트\n  - src/components/ : 컴포넌트\n- server/ : 서버\n";
        let labels = folder_labels(structure_part(doc));
        assert_eq!(labels, vec!["src", "src/components", "server"]);
        let agents = "# 상태\n### grist\nID : 1201\n### obsidian\n문서 : 구현/공방/260926.콘티 장면 생성기/일지/콘티 장면 생성기-개발.md\n";
        assert_eq!(
            agents_obsidian_doc(agents).as_deref(),
            Some("구현/공방/260926.콘티 장면 생성기/일지/콘티 장면 생성기-개발.md")
        );
    }
}
