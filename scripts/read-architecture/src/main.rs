// Start-of-work structure check for work-project (crate read-architecture).
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

    println!("# {}/.ai/docs/architecture.md", project.display());
    println!("{}", doc.trim_end());

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

    if !query.trim().is_empty() {
        let mut words = Vec::new();
        let mut seen = HashSet::new();
        for word in query
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|word| word.chars().count() > 1)
        {
            if seen.insert(word.to_owned()) {
                words.push(word.to_owned());
            }
        }
        let mut score: HashMap<String, Score> = HashMap::new();
        let mut order = Vec::new();
        for word in &words {
            let output = Command::new("rg")
                .args([
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
                    "!.ai",
                    "-g",
                    "!*.lock",
                    "-g",
                    "!*-lock.*",
                    "-g",
                    "!test/tmp",
                    ".",
                ])
                .current_dir(&project)
                .output()
                .unwrap_or_else(|e| {
                    println!("[architecture] rg 실행 실패: {e}");
                    process::exit(2)
                });
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Some((file, count)) = line.rsplit_once(':') {
                    let file = file.strip_prefix("./").unwrap_or(file).to_owned();
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
