use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_DOC_ID: &str = "au1EbKLtZbrW";
const DEFAULT_VAULT_ROOT: &str = "/mnt/c/Users/tende/OneDrive/일기장";
const FORMAT_DIR: &str = "구현/업무/261008.작업관리기/형식";
const DOC_TEMPLATE: &str = "구현/업무/261008.작업관리기/형식/작업관리기.형식-문서.md";
const DEV_TEMPLATE: &str = "구현/업무/261008.작업관리기/형식/작업관리기.형식-개발.md";
const AGENTS_TEMPLATE: &str = "구현/업무/261008.작업관리기/형식/작업관리기.형식-Agents.md.md";
const ARCH_TEMPLATE: &str = "구현/업무/261008.작업관리기/형식/작업관리기.형식-architecture.md";
const PERF_TEMPLATE: &str = "구현/업무/261008.작업관리기/형식/작업관리기.형식-performance.md";
const PROJECT_TYPES: &[&str] = &["업무", "공방", "게임", "보조"];
const MANAGEMENT_RAW_ID: i64 = 1198;
const DEFAULT_GRIST_BASE_URL: &str = "http://localhost:8484";

struct Options {
    project_root: String,
    project_name: String,
    project_type: String,
    runtime: String,
    operation_id: String,
    doc_id: String,
    table7: String,
    table26: String,
    grist_base_url: String,
    grist_api_key: String,
    vault_root: String,
    template_path: String,
    receipt: Option<String>,
    description: String,
}

fn now_ms() -> i64 {
    if let Ok(v) = env::var("INITIALIZE_PROJECT_NOW_MS") {
        if let Ok(n) = v.trim().parse::<i64>() {
            return n;
        }
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = y as i64 - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

fn seoul_ymd(ms: i64) -> (i32, u32, u32) {
    let seoul_secs = ms.div_euclid(1000) + 9 * 3600;
    let days = seoul_secs.div_euclid(86400);
    civil_from_days(days)
}

fn unix_timestamp(ms: i64) -> i64 {
    ms.div_euclid(1000)
}

fn next_year_december3_timestamp(ms: i64) -> i64 {
    let (year, _, _) = seoul_ymd(ms);
    days_from_civil(year + 1, 12, 3) * 86400
}

fn date_prefix(ms: i64) -> String {
    let (year, month, day) = seoul_ymd(ms);
    format!("{:02}{:02}{:02}", year.rem_euclid(100), month, day)
}

fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = String::with_capacity(64);
    for word in h {
        out.push_str(&format!("{:08x}", word));
    }
    out
}

fn path_resolve(base: &str, p: &str) -> String {
    let combined = if p.starts_with('/') {
        p.to_string()
    } else if base.ends_with('/') {
        format!("{base}{p}")
    } else {
        format!("{base}/{p}")
    };
    let mut stack: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            stack.pop();
        } else {
            stack.push(seg);
        }
    }
    if stack.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", stack.join("/"))
    }
}

fn cwd_string() -> String {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("/"))
        .to_string_lossy()
        .to_string()
}

fn load_env_file(file: &Path, env_map: &mut HashMap<String, String>) -> Result<(), String> {
    let content = match fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("env-file: .env 읽기 실패".to_string()),
    };
    let line_re = Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$").unwrap();
    for raw_line in content.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let Some(caps) = line_re.captures(line) else {
            continue;
        };
        let key = caps.get(1).unwrap().as_str();
        if env_map.contains_key(key) {
            continue;
        }
        let mut val = caps.get(2).unwrap().as_str().to_string();
        if val.len() >= 2
            && ((val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\'')))
        {
            val = val[1..val.len() - 1].to_string();
        }
        env_map.insert(key.to_string(), val);
    }
    Ok(())
}

fn load_all_env() -> Result<HashMap<String, String>, String> {
    let mut env_map: HashMap<String, String> = env::vars().collect();
    let configured = env_map.get("GRIST_ENV_FILE").cloned().unwrap_or_default();
    let home = env_map.get("HOME").cloned().unwrap_or_default();
    let access_env = format!("{}/.codex/skills/access-grist/.env", home);
    let skill_env = env::current_exe()
        .ok()
        .and_then(|p| p.parent().and_then(|b| b.parent()).and_then(|s| s.parent()).map(|root| root.join(".env")));

    let mut files: Vec<PathBuf> = Vec::new();
    if !configured.is_empty() {
        files.push(PathBuf::from(configured));
    }
    files.push(PathBuf::from(access_env));
    if let Some(se) = skill_env {
        files.push(se);
    }
    for f in files {
        load_env_file(&f, &mut env_map)?;
    }
    Ok(env_map)
}

fn parse_args(argv: &[String], env_map: &HashMap<String, String>, ms: i64) -> Result<Options, String> {
    let mut values: HashMap<String, String> = HashMap::new();
    let mut index = 1;
    while index < argv.len() {
        let value = &argv[index];
        if !value.starts_with("--") {
            return Err(format!("알 수 없는 인자: {value}"));
        }
        let key = &value[2..];
        let next = argv.get(index + 1);
        match next {
            None => return Err(format!("인자 값이 없습니다: --{key}")),
            Some(n) if n.is_empty() || n.starts_with("--") => {
                return Err(format!("인자 값이 없습니다: --{key}"));
            }
            Some(n) => {
                values.insert(key.to_string(), n.clone());
                index += 2;
            }
        }
    }
    let project_root = values.get("project-root").cloned().unwrap_or_default();
    let project_name = values.get("project-name").cloned().unwrap_or_default();
    if project_root.is_empty() || project_name.is_empty() {
        return Err("--project-root와 --project-name이 필요합니다.".to_string());
    }
    let project_type = values
        .get("project-type")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !PROJECT_TYPES.contains(&project_type.as_str()) {
        return Err("--project-type은 업무, 공방, 게임, 보조 중 하나여야 합니다.".to_string());
    }
    let runtime = values
        .get("runtime")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "{진입지점 혹은 실행 방법 한줄 요약}".to_string());
    let cwd = cwd_string();
    let resolved_project_root = path_resolve(&cwd, &project_root);
    let vault_raw = values
        .get("vault-root")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_VAULT_ROOT.to_string());
    let resolved_vault_root = path_resolve(&cwd, &vault_raw);

    let operation_id = values
        .get("operation-id")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("initialize-project-{ms}"));
    let doc_id = values
        .get("grist-doc-id")
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| env_map.get("GRIST_DOC_ID").cloned().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| DEFAULT_DOC_ID.to_string());
    let table7 = values
        .get("table7")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Table7".to_string());
    let table26 = values
        .get("table26")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Table26".to_string());
    let grist_base_url = values
        .get("grist-base-url")
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| env_map.get("GRIST_BASE_URL").cloned().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| DEFAULT_GRIST_BASE_URL.to_string());
    let grist_api_key = values
        .get("grist-api-key")
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| env_map.get("GRIST_API_KEY").cloned().filter(|s| !s.is_empty()))
        .unwrap_or_default();
    let template_path = values
        .get("template-path")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEV_TEMPLATE.to_string());
    let receipt = values.get("receipt").cloned().filter(|s| !s.is_empty());
    let description = values
        .get("description")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| project_name.clone());

    Ok(Options {
        project_root: resolved_project_root,
        project_name,
        project_type,
        runtime,
        operation_id,
        doc_id,
        table7,
        table26,
        grist_base_url,
        grist_api_key,
        vault_root: resolved_vault_root,
        template_path,
        receipt,
        description,
    })
}

fn relative_file(vault_root: &str, relative_path: &str) -> Result<String, String> {
    if relative_path.is_empty()
        || relative_path.contains('\\')
        || relative_path.contains('%')
        || relative_path.starts_with('/')
    {
        return Err("Obsidian 경로는 percent encoding 없는 Vault 상대경로여야 합니다.".to_string());
    }
    let segments: Vec<&str> = relative_path.split('/').collect();
    if segments.iter().any(|s| s.is_empty() || *s == "." || *s == "..") {
        return Err("Obsidian 경로 구간이 올바르지 않습니다.".to_string());
    }
    let root = path_resolve(&cwd_string(), vault_root);
    let file = path_resolve(&root, relative_path);
    if file != root && !file.starts_with(&format!("{root}/")) {
        return Err("Obsidian 경로가 Vault 경계를 벗어났습니다.".to_string());
    }
    Ok(file)
}

fn readable(file: &str) -> Result<String, String> {
    let meta = match fs::metadata(file) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(format!("ENOENT: no such file or directory, access '{file}'"));
        }
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() {
        let base = Path::new(file)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        return Err(format!("파일이 아닙니다: {base}"));
    }
    fs::read_to_string(file).map_err(|e| e.to_string())
}

struct NoteInfo {
    bytes: usize,
    sha256: String,
    content: String,
}

fn create_markdown(vault_root: &str, relative_path: &str, source: &str) -> Result<NoteInfo, String> {
    let file = relative_file(vault_root, relative_path)?;
    if let Some(parent) = Path::new(&file).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    match OpenOptions::new().write(true).create_new(true).open(&file) {
        Ok(mut f) => {
            f.write_all(source.as_bytes()).map_err(|e| e.to_string())?;
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    let content = readable(&file)?;
    let bytes = content.len();
    let sha256 = sha256_hex(content.as_bytes());
    Ok(NoteInfo { bytes, sha256, content })
}

fn write_text_atomically(file: &str, content: &str, ms: i64) -> Result<String, String> {
    if let Some(parent) = Path::new(file).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temporary = format!("{file}.initialize-project-{}-{ms}", process::id());
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        f.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    }
    fs::rename(&temporary, file).map_err(|e| e.to_string())?;
    readable(file)
}

fn write_if_missing(file: &str, content: &str) -> Result<bool, String> {
    if let Some(parent) = Path::new(file).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    match OpenOptions::new().write(true).create_new(true).open(file) {
        Ok(mut f) => {
            f.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

fn js_replace_first(text: &str, re: &Regex, rep: &str) -> String {
    let Some(m) = re.find(text) else {
        return text.to_string();
    };
    let before = &text[..m.start()];
    let matched = m.as_str();
    let after = &text[m.end()..];
    let mut out = String::with_capacity(text.len() + rep.len());
    out.push_str(before);
    let mut chars = rep.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek().copied() {
                Some('$') => {
                    chars.next();
                    out.push('$');
                }
                Some('&') => {
                    chars.next();
                    out.push_str(matched);
                }
                Some('`') => {
                    chars.next();
                    out.push_str(before);
                }
                Some('\'') => {
                    chars.next();
                    out.push_str(after);
                }
                _ => out.push('$'),
            }
        } else {
            out.push(c);
        }
    }
    out.push_str(after);
    out
}

fn render_project_doc(template: &str, project_type: &str, project_name: &str, year: i32, month: u32, day: u32) -> String {
    let folder_re = Regex::new(r"(?m)^\[\[[^|]*\|📁\]\]\s*$").unwrap();
    let hammer_re = Regex::new(r"(?m)^# 🔨\s*$").unwrap();
    let cal_re = Regex::new(r"(?m)^# 📆\n## YYYY\n### M월\nMMDD : 문서 생성\n?").unwrap();

    let s1 = js_replace_first(template, &folder_re, &format!("[[구현/{project_type}|📁]]"));
    let s2 = js_replace_first(&s1, &hammer_re, &format!("# 🔨\n[[{project_name}-개발.md]]"));
    let cal_block = format!(
        "# 📆\n## {year}\n### {month}월\n{month:02}{day:02} : 문서 생성\n"
    );
    js_replace_first(&s2, &cal_re, &cal_block)
}

fn render_dev_note(template: &str, project_name: &str, description: &str, year: i32, month: u32) -> String {
    let split_re = Regex::new(r"(?m)^# 💡").unwrap();
    let matches: Vec<_> = split_re.find_iter(template).collect();
    let (head, user) = match matches.as_slice() {
        [] => (template, ""),
        [m1] => (&template[..m1.start()], &template[m1.start()..]),
        [m1, m2, ..] => (&template[..m1.start()], &template[m1.start()..m2.start()]),
    };

    let folder_re = Regex::new(r"(?m)^\[\[[^\]|]*\|📁\]\]").unwrap();
    let fence_re = Regex::new(r"(?m)^```[^\n]*\n[\s\S]*?^```\s*$\n?").unwrap();
    let desc_re = Regex::new(r"(?m)^# 📋\s*$").unwrap();
    let cal_re = Regex::new(r"(?m)(^# 📆[^\n]*\n)```[^\n]*\n[\s\S]*?^```\s*$\n?").unwrap();
    let blank_re = Regex::new(r"\n{3,}").unwrap();

    let s1 = js_replace_first(head, &folder_re, &format!("[[{project_name}|📁]]"));
    let s2 = fence_re.replace_all(&s1, "").into_owned();
    let s3 = js_replace_first(&s2, &desc_re, &format!("# 📋\n- {description}"));
    let s4 = blank_re.replace_all(&s3, "\n\n").into_owned();
    let body_trimmed = s4.trim_end();

    let cal_replacement = format!("$1## {year}\n### {month}월\n");
    let calendar = cal_re.replace(user, cal_replacement.as_str()).into_owned();
    format!("{body_trimmed}\n\n{calendar}")
}

fn update_type_doc_content(existing: &str, project_name: &str, year: i32) -> String {
    let link = format!("[[{project_name}]]");
    let base = if existing.trim().is_empty() {
        format!(
            "[[구현|📁]] \n📋\n# 🏆\n\n# 📒\n# 📚\n# 🗄\n# 📆\n## {year}\n# 💭\n# 📜\n\n# 📼\n🗂\n# 🧙\n\n# 📰\n🔖\n"
        )
    } else {
        existing.to_string()
    };
    let mut lines: Vec<String> = base
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    let cab_idx = lines.iter().position(|l| l.trim() == "# 🗄");
    if let Some(idx) = cab_idx {
        let mut end = lines.len();
        for (i, line) in lines.iter().enumerate().skip(idx + 1) {
            if line.trim_start().starts_with('#') {
                end = i;
                break;
            }
        }
        let already = lines[(idx + 1)..end].iter().any(|l| l.trim() == link);
        if !already {
            let mut insert_at = end;
            while insert_at > idx + 1 && lines[insert_at - 1].trim().is_empty() {
                insert_at -= 1;
            }
            lines.insert(insert_at, link);
        }
    } else {
        lines.push("# 🗄".to_string());
        lines.push(link);
    }
    format!("{}\n", lines.join("\n"))
}

fn render_status_section(raw_id: i64, dev_file: &str, runtime: &str) -> String {
    format!(
        "# 상태\n### grist\nID : {raw_id}\n### obsidian\n문서 : {dev_file}\n### 실행\nruntime : {runtime}\n"
    )
}

fn render_agents_md(template: &str, existing_agents: &str, raw_id: i64, dev_file: &str, runtime: &str) -> String {
    let status_block = render_status_section(raw_id, dev_file, runtime);
    let source = if existing_agents.trim().is_empty() {
        template
    } else {
        existing_agents
    };
    let status_re = Regex::new(
        r"(?ms)^\s*#\s*상태\s*\n###\s*grist\s*\nID\s*:[^\n]*\n###\s*obsidian\s*\n문서\s*:[^\n]*\n###\s*실행\s*\nruntime\s*:[^\n]*\n?",
    )
    .unwrap();
    if status_re.is_match(source) {
        return status_re.replace(source, status_block.as_str()).into_owned();
    }
    format!("{status_block}\n{}", source.trim_start())
}

fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn parse_http_url(base_url: &str, doc_id: &str, suffix: &str) -> Result<(String, u16, String), String> {
    let rest = base_url
        .strip_prefix("http://")
        .ok_or_else(|| "grist-transport: fetch failed".to_string())?;
    let (authority, base_path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => {
            let port: u16 = p.parse().map_err(|_| "grist-transport: fetch failed".to_string())?;
            (h.to_string(), port)
        }
        None => (authority.to_string(), 80),
    };
    let (pathname, search) = match suffix.split_once('?') {
        Some((p, q)) => (p, q),
        None => (suffix, ""),
    };
    let trimmed_base = base_path.trim_end_matches('/');
    let mut full_path = format!(
        "{}/api/docs/{}{}",
        trimmed_base,
        encode_uri_component(doc_id),
        pathname
    );
    if !search.is_empty() {
        full_path.push('?');
        full_path.push_str(search);
    }
    Ok((host, port, full_path))
}

fn grist_request(options: &Options, method: &str, suffix: &str, body: Option<&Value>) -> Result<Value, String> {
    let (host, port, full_path) = parse_http_url(&options.grist_base_url, &options.doc_id, suffix)?;
    let serialized = body.map(|b| b.to_string());
    let mut stream = TcpStream::connect((host.as_str(), port))
        .map_err(|_| "grist-transport: fetch failed".to_string())?;

    let mut req = format!("{method} {full_path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\n");
    let key = options.grist_api_key.trim();
    if !key.is_empty() {
        req.push_str(&format!("Authorization: Bearer {key}\r\n"));
    }
    if let Some(ref s) = serialized {
        req.push_str("Content-Type: application/json\r\n");
        req.push_str(&format!("Content-Length: {}\r\n", s.len()));
    }
    req.push_str("Connection: close\r\n\r\n");
    if let Some(ref s) = serialized {
        req.push_str(s);
    }

    stream
        .write_all(req.as_bytes())
        .map_err(|_| "grist-transport: fetch failed".to_string())?;
    let mut resp_bytes = Vec::new();
    stream
        .read_to_end(&mut resp_bytes)
        .map_err(|_| "grist-transport: fetch failed".to_string())?;

    let head_end = resp_bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "grist-transport: fetch failed".to_string())?;
    let head = String::from_utf8_lossy(&resp_bytes[..head_end]);
    let status_line = head.lines().next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "grist-transport: fetch failed".to_string())?;

    let text = String::from_utf8_lossy(&resp_bytes[head_end + 4..]).to_string();
    let value: Value = if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text)
            .map_err(|_| format!("grist-response: JSON 응답을 읽을 수 없습니다 (HTTP {status})."))?
    };

    if !(200..300).contains(&status) {
        let safe_detail: String = value
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(160).collect())
            .unwrap_or_default();
        let detail_suffix = if safe_detail.is_empty() {
            String::new()
        } else {
            format!(" {safe_detail}")
        };
        return Err(format!(
            "grist-http: {method} {suffix} HTTP {status} 응답으로 Grist 요청이 거부됐습니다.{detail_suffix}"
        ));
    }
    Ok(value)
}

fn record_rows(value: &Value) -> Result<Vec<Map<String, Value>>, String> {
    let records = value
        .get("records")
        .and_then(|r| r.as_array())
        .ok_or_else(|| "Grist records 응답 형식이 올바르지 않습니다.".to_string())?;
    let mut rows = Vec::new();
    for rec in records {
        let Some(id) = rec.get("id").and_then(|i| i.as_i64()) else {
            continue;
        };
        let Some(fields) = rec.get("fields").and_then(|f| f.as_object()) else {
            continue;
        };
        let mut row = Map::new();
        row.insert("id".to_string(), json!(id));
        for (k, v) in fields {
            row.insert(k.clone(), v.clone());
        }
        rows.push(row);
    }
    Ok(rows)
}

fn column_id(columns: &[Value], label: &str, fallback: &str) -> String {
    for col in columns {
        let id_match = col.get("id").and_then(|v| v.as_str()) == Some(label);
        let label_match = col
            .get("fields")
            .and_then(|f| f.get("label"))
            .and_then(|v| v.as_str())
            == Some(label);
        if id_match || label_match {
            if let Some(id) = col.get("id").and_then(|v| v.as_str()) {
                if !id.is_empty() {
                    return id.to_string();
                }
            }
        }
    }
    fallback.to_string()
}

fn load_grist_schema(options: &Options) -> Result<String, String> {
    let t7 = grist_request(
        options,
        "GET",
        &format!("/tables/{}/columns", encode_uri_component(&options.table7)),
        None,
    )?;
    let t26 = grist_request(
        options,
        "GET",
        &format!("/tables/{}/columns", encode_uri_component(&options.table26)),
        None,
    )?;
    let Some(t7_cols) = t7.get("columns").and_then(|c| c.as_array()) else {
        return Err("grist-schema: Grist column 응답 형식이 올바르지 않습니다.".to_string());
    };
    if t26.get("columns").and_then(|c| c.as_array()).is_none() {
        return Err("grist-schema: Grist column 응답 형식이 올바르지 않습니다.".to_string());
    }
    Ok(column_id(t7_cols, "Obsidian_Id", "Obsidian_Id"))
}

fn fetch_rows(options: &Options, table: &str) -> Result<Vec<Map<String, Value>>, String> {
    let v = grist_request(
        options,
        "GET",
        &format!("/tables/{}/records", encode_uri_component(table)),
        None,
    )?;
    record_rows(&v)
}

fn add_record(options: &Options, table: &str, fields: Map<String, Value>) -> Result<Value, String> {
    let body = json!({
        "records": [
            { "fields": Value::Object(fields) }
        ]
    });
    grist_request(
        options,
        "POST",
        &format!("/tables/{}/records?noparse=true", encode_uri_component(table)),
        Some(&body),
    )
}

fn update_record(options: &Options, table: &str, id: i64, fields: Map<String, Value>) -> Result<Value, String> {
    let body = json!({
        "records": [
            { "id": id, "fields": Value::Object(fields) }
        ]
    });
    grist_request(
        options,
        "PATCH",
        &format!("/tables/{}/records?noparse=true", encode_uri_component(table)),
        Some(&body),
    )
}

fn read_agents_template(vault_root: &str) -> Result<String, String> {
    let primary = relative_file(vault_root, AGENTS_TEMPLATE)?;
    readable(&primary)
}

fn initialize(options: &Options, ms: i64) -> Result<Value, String> {
    let prefix = date_prefix(ms);
    let (year, month, day) = seoul_ymd(ms);
    let timestamp = unix_timestamp(ms);
    let base_folder = format!("구현/{}/{prefix}.{}", options.project_type, options.project_name);
    let doc_file = format!("{base_folder}/{}.md", options.project_name);
    let dev_file = format!("{base_folder}/일지/{}-개발.md", options.project_name);
    let note_dir_rel = format!("{base_folder}/노트");
    let type_doc_rel = format!("구현/{}/{}.md", options.project_type, options.project_type);

    let dev_template_file = relative_file(&options.vault_root, &options.template_path)?;
    let dev_template = readable(&dev_template_file)?;
    let doc_template_file = relative_file(&options.vault_root, DOC_TEMPLATE)?;
    let doc_template = readable(&doc_template_file)?;
    let agents_template = read_agents_template(&options.vault_root)?;
    let project_root = &options.project_root;

    let obsidian_col = load_grist_schema(options)?;
    let existing = fetch_rows(options, &options.table7)?;
    let has_mgmt = existing
        .iter()
        .any(|r| r.get("id").and_then(|v| v.as_i64()) == Some(MANAGEMENT_RAW_ID));
    if !has_mgmt {
        return Err(format!(
            "grist-management-row: Table7 #{MANAGEMENT_RAW_ID}(작업관리기) 행을 찾지 못했습니다."
        ));
    }
    let duplicate = existing.iter().find(|r| {
        r.get(&obsidian_col).and_then(|v| v.as_str()) == Some(dev_file.as_str())
            || r.get("C").and_then(|v| v.as_str()) == Some(options.project_name.as_str())
    });
    let mut raw_id = duplicate.and_then(|r| r.get("id").and_then(|v| v.as_i64()));

    if let Some(existing_id) = raw_id {
        if duplicate.and_then(|r| r.get(&obsidian_col).and_then(|v| v.as_str())) != Some(dev_file.as_str()) {
            let mut upd = Map::new();
            upd.insert(obsidian_col.clone(), json!(dev_file));
            update_record(options, &options.table7, existing_id, upd)?;
        }
    } else {
        let mut fields = Map::new();
        fields.insert("H".to_string(), json!("코딩"));
        fields.insert("P".to_string(), json!("개인"));
        fields.insert("D".to_string(), json!("진행"));
        fields.insert("M".to_string(), json!("서류"));
        fields.insert("C".to_string(), json!(options.project_name));
        fields.insert("A".to_string(), json!(timestamp));
        fields.insert(obsidian_col.clone(), json!(dev_file));
        fields.insert("I".to_string(), json!("없음"));
        fields.insert("L".to_string(), json!("없음"));
        fields.insert("K".to_string(), json!(""));
        fields.insert("E".to_string(), json!("없음"));
        fields.insert("F".to_string(), json!(next_year_december3_timestamp(ms)));

        add_record(options, &options.table7, fields)?;
        let after = fetch_rows(options, &options.table7)?;
        let created = after.iter().find(|r| {
            r.get(&obsidian_col).and_then(|v| v.as_str()) == Some(dev_file.as_str())
                && r.get("C").and_then(|v| v.as_str()) == Some(options.project_name.as_str())
        });
        let Some(c) = created.and_then(|r| r.get("id").and_then(|v| v.as_i64())) else {
            return Err("Table7 생성 후 동일한 Obsidian_Id·프로젝트명을 가진 행을 readback하지 못했습니다.".to_string());
        };
        raw_id = Some(c);
    }
    let raw_id = raw_id.unwrap();

    let after_rows = fetch_rows(options, &options.table7)?;
    let table7_after = after_rows
        .iter()
        .find(|r| r.get("id").and_then(|v| v.as_i64()) == Some(raw_id));
    if table7_after.and_then(|r| r.get(&obsidian_col).and_then(|v| v.as_str()))
        != Some(dev_file.as_str())
    {
        return Err("Table7 ID 또는 Obsidian_Id readback 값이 일치하지 않습니다.".to_string());
    }

    let rendered_doc = render_project_doc(&doc_template, &options.project_type, &options.project_name, year, month, day);
    let doc_note = create_markdown(&options.vault_root, &doc_file, &rendered_doc)?;

    let rendered_dev = render_dev_note(&dev_template, &options.project_name, &options.description, year, month);
    let dev_note = create_markdown(&options.vault_root, &dev_file, &rendered_dev)?;
    let has_cal = Regex::new(r"(?m)^# 📆\s*$").unwrap().is_match(&dev_note.content);
    let has_pin = Regex::new(r"(?m)^# 📍\s*$").unwrap().is_match(&dev_note.content);
    if !has_cal || !has_pin {
        return Err("obsidian-readback: 개발 문서에 # 📆·# 📍 헤더가 없습니다.".to_string());
    }

    let note_dir_abs = relative_file(&options.vault_root, &note_dir_rel)?;
    fs::create_dir_all(&note_dir_abs).map_err(|e| e.to_string())?;

    let type_doc_abs = relative_file(&options.vault_root, &type_doc_rel)?;
    let existing_type_doc = match readable(&type_doc_abs) {
        Ok(c) => c,
        Err(ref e) if e.starts_with("ENOENT:") => String::new(),
        Err(e) => return Err(e),
    };
    let updated_type_doc = update_type_doc_content(&existing_type_doc, &options.project_name, year);
    write_text_atomically(&type_doc_abs, &updated_type_doc, ms)?;

    let agents_file = format!("{project_root}/AGENTS.md");
    let existing_agents = match readable(&agents_file) {
        Ok(c) => c,
        Err(ref e) if e.starts_with("ENOENT:") => String::new(),
        Err(e) => return Err(e),
    };
    let rendered_agents = render_agents_md(&agents_template, &existing_agents, raw_id, &dev_file, &options.runtime);
    write_text_atomically(&agents_file, &rendered_agents, ms)?;

    let mut docs = Map::new();
    for (name, tpl_rel) in [
        ("architecture.md", ARCH_TEMPLATE),
        ("performance.md", PERF_TEMPLATE),
    ] {
        let file = format!("{project_root}/.ai/docs/{name}");
        let tpl_path = relative_file(&options.vault_root, tpl_rel)?;
        let tpl_content = readable(&tpl_path)?;
        let written = write_if_missing(&file, &tpl_content)?;
        docs.insert(name.to_string(), json!(written));
    }
    fs::create_dir_all(format!("{project_root}/.ai/changes")).map_err(|e| e.to_string())?;

    let mut evidence = Map::new();
    evidence.insert("operation_id".to_string(), json!(options.operation_id));
    evidence.insert("status".to_string(), json!("complete"));
    evidence.insert("project_root".to_string(), json!(project_root));
    evidence.insert("project_type".to_string(), json!(options.project_type));
    evidence.insert("format_dir".to_string(), json!(FORMAT_DIR));
    evidence.insert(
        "obsidian".to_string(),
        json!({
            "document": {
                "path": doc_file,
                "bytes": doc_note.bytes,
                "sha256": doc_note.sha256,
            },
            "project": {
                "path": dev_file,
                "bytes": dev_note.bytes,
                "sha256": dev_note.sha256,
            },
            "notes_dir": note_dir_rel,
            "type_document": type_doc_rel,
        }),
    );
    evidence.insert(
        "grist".to_string(),
        json!({
            "transport": "grist-rest-api",
            "table7": {
                "raw_id": raw_id,
                "obsidian_path": dev_file,
            }
        }),
    );
    evidence.insert("tasks".to_string(), json!([]));
    evidence.insert(
        "files".to_string(),
        json!({
            "agents": agents_file,
            "docs": Value::Object(docs),
        }),
    );
    Ok(Value::Object(evidence))
}

fn fail_and_exit(message: &str, argv: &[String], env_map: &HashMap<String, String>, ms: i64) -> ! {
    let boundary = match message.find(':') {
        Some(idx) => &message[..idx],
        None => "initialize-project",
    };
    let mut err_obj = Map::new();
    err_obj.insert("status".to_string(), json!("runtime-unverified"));
    err_obj.insert("first_failing_boundary".to_string(), json!(boundary));
    err_obj.insert("error".to_string(), json!(message));
    eprintln!("{}", Value::Object(err_obj.clone()));

    if let Ok(options) = parse_args(argv, env_map, ms) {
        let receipt_path = options.receipt.unwrap_or_else(|| {
            format!("{}/test/tmp/{}.json", cwd_string(), options.operation_id)
        });
        err_obj.insert("operation_id".to_string(), json!(options.operation_id));
        if let Ok(pretty) = serde_json::to_string_pretty(&Value::Object(err_obj)) {
            let _ = write_text_atomically(&receipt_path, &format!("{pretty}\n"), ms);
        }
    }
    process::exit(2);
}

fn main() {
    let ms = now_ms();
    let argv: Vec<String> = env::args().collect();
    let env_map = match load_all_env() {
        Ok(m) => m,
        Err(e) => fail_and_exit(&e, &argv, &HashMap::new(), ms),
    };
    let options = match parse_args(&argv, &env_map, ms) {
        Ok(o) => o,
        Err(e) => fail_and_exit(&e, &argv, &env_map, ms),
    };
    match initialize(&options, ms) {
        Ok(result) => {
            let receipt_path = options.receipt.clone().unwrap_or_else(|| {
                format!("{}/test/tmp/initialization-receipt.json", options.project_root)
            });
            let pretty = serde_json::to_string_pretty(&result).unwrap();
            if let Err(e) = write_text_atomically(&receipt_path, &format!("{pretty}\n"), ms) {
                fail_and_exit(&e, &argv, &env_map, ms);
            }
            println!("{result}");
        }
        Err(e) => fail_and_exit(&e, &argv, &env_map, ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_format_files_exist_in_default_vault() {
        for relative in [DOC_TEMPLATE, DEV_TEMPLATE, AGENTS_TEMPLATE, ARCH_TEMPLATE, PERF_TEMPLATE] {
            let file = relative_file(DEFAULT_VAULT_ROOT, relative).unwrap();
            assert!(Path::new(&file).is_file(), "missing format file: {relative}");
        }
    }

    #[test]
    fn project_type_validation_and_obsidian_rendering() {
        let env_map = HashMap::new();
        let bad = vec![
            "initialize-project".to_string(),
            "--project-root".to_string(),
            "/tmp/x".to_string(),
            "--project-name".to_string(),
            "demo".to_string(),
        ];
        assert!(parse_args(&bad, &env_map, 0).is_err());

        let doc_tpl = "[[구현/[프로젝트타입]|📁]]\n# 📋\n# 🏆\n# 📕\n# 🔨\n# 💾\n# 📚\n# 📆\n## YYYY\n### M월\nMMDD : 문서 생성\n# 💭\n🔖\n";
        let doc_out = render_project_doc(doc_tpl, "공방", "샘플", 2026, 10, 20);
        assert!(doc_out.contains("[[구현/공방|📁]]"));
        assert!(doc_out.contains("# 🔨\n[[샘플-개발.md]]"));
        assert!(doc_out.contains("# 📆\n## 2026\n### 10월\n1020 : 문서 생성"));

        let dev_tpl = "[[<프로젝트 이름>|📁]]\n# 📋\n```\n설명\n```\n# 📍\n```\n기능\n```\n# 🏗\n# 💡\n# 📆\n```\n형식\n```\n🔖";
        let dev_out = render_dev_note(dev_tpl, "샘플", "샘플 프로젝트 설명", 2026, 10);
        assert!(dev_out.contains("[[샘플|📁]]"));
        assert!(dev_out.contains("# 📋\n- 샘플 프로젝트 설명"));
        assert!(dev_out.contains("# 📆\n## 2026\n### 10월"));

        let type_out = update_type_doc_content("[[구현|📁]]\n# 🗄\n[[작업관리기]]\n# 📆\n", "샘플", 2026);
        assert!(type_out.contains("# 🗄\n[[작업관리기]]\n[[샘플]]\n# 📆"));

        let agents_tpl = "# 상태 \n### grist \nID : {grist에서 기록한 table raw ID}\n### obsidian \n문서 : {obsidian 안에 만들어진 개발 문서 Path}\n### 실행 \nruntime : {진입지점 혹은 실행 방법 한줄 요약}\n# 정보 \n# tip\n";
        let agents_out = render_agents_md(
            agents_tpl,
            "",
            1299,
            "구현/공방/261020.샘플/일지/샘플-개발.md",
            "npm test",
        );
        assert!(agents_out.contains("# 상태\n### grist\nID : 1299\n### obsidian\n문서 : 구현/공방/261020.샘플/일지/샘플-개발.md\n### 실행\nruntime : npm test"));
        println!("initialize-project render test OK: {agents_out}");
    }
}
