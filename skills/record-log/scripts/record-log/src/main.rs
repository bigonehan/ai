use std::{env, fs, path::{Path, PathBuf}, process::Command};

const AGENT_CHARS: &str = "0123456789ABCDEFGHJKLMNPQRSTUVWXYZ";

#[derive(Debug, PartialEq)]
struct Parsed { project: u32, row: u32, agent: char }

fn one_line(value: &str, name: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().any(|c| c.is_control() || c == '|') {
        return Err(format!("{name} must be a nonempty single field"));
    }
    Ok(())
}

fn validate_result(result: &str) -> Result<(), String> {
    one_line(result, "result")?;
    if result.chars().count() > 120 { return Err("result exceeds 120 characters".into()); }
    let trimmed = result.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Err("JSON result is not allowed".into());
    }
    let lower = result.to_ascii_lowercase();
    if ["bearer ", "authorization", "api_key", "api-key", "apikey",
        "access_token", "refresh_token", "secret", "sk-", "eyj"]
        .iter().any(|marker| lower.contains(marker)) {
        return Err("result contains a credential marker".into());
    }
    Ok(())
}

fn log_line(id: &str, function: &str, attempt: &str, result: &str) -> Result<String, String> {
    parse_id(id)?;
    one_line(function, "function")?;
    if function.chars().any(char::is_whitespace) {
        return Err("function must not contain whitespace".into());
    }
    if attempt.len() != 2 || !attempt.bytes().all(|b| b.is_ascii_digit()) {
        return Err("attempt must be two digits".into());
    }
    validate_result(result)?;
    Ok(format!("{id} | {function} | {attempt} | {result}"))
}

fn next_agent_char(used: &[char]) -> Result<char, String> {
    AGENT_CHARS.chars().skip(1).find(|c| !used.contains(c))
        .ok_or_else(|| "all agent numbers are assigned".into())
}

fn make_id(project: u32, row: u32, agent: char) -> Result<String, String> {
    if project > 999_999 || row > 9_999 || !AGENT_CHARS.contains(agent) {
        return Err("project, row, or agent is outside the ID range".into());
    }
    Ok(format!("{project:06}{row:04}{agent}"))
}

fn parse_id(id: &str) -> Result<Parsed, String> {
    if id.len() != 11 || !id.is_ascii() || !id.as_bytes()[..10].iter().all(u8::is_ascii_digit) {
        return Err("ID must have six project digits, four row digits, and one agent character".into());
    }
    let agent = id.as_bytes()[10] as char;
    if !AGENT_CHARS.contains(agent) { return Err("invalid agent character".into()); }
    Ok(Parsed {
        project: id[..6].parse().map_err(|_| "invalid project number")?,
        row: id[6..10].parse().map_err(|_| "invalid row number")?,
        agent,
    })
}


fn field(value: &str, name: &str) -> Result<(), String> {
    one_line(value, name)?;
    if value.trim() != value { return Err(format!("{name} must not have outer whitespace")); }
    Ok(())
}

fn memo_line(agent: char, role: &str, conversation: &str, caller: Option<(char, &str)>) -> Result<String, String> {
    if !AGENT_CHARS.contains(agent) { return Err("invalid agent character".into()); }
    field(role, "role")?;
    if role.contains(" · ") { return Err("role must not contain ' · '".into()); }
    field(conversation, "conversation")?;
    if conversation.contains(" ← ") { return Err("conversation must not contain ' ← '".into()); }
    let mut line = format!("에이전트 {agent} : {role} · {conversation}");
    if let Some((c, name)) = caller {
        if !AGENT_CHARS.contains(c) { return Err("invalid caller character".into()); }
        field(name, "caller name")?;
        line.push_str(&format!(" ← {c} {name}"));
    }
    Ok(line)
}

#[derive(Debug, PartialEq)]
struct Entry { agent: char, role: String, conversation: String, caller: Option<(char, String)> }

/// Asia/Seoul date as YYMMDD from Unix seconds.
fn kst_date(unix_secs: u64) -> String {
    let days = ((unix_secs + 9 * 3600) / 86_400) as i64;
    // Civil-from-days (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{:02}{m:02}{d:02}", y % 100)
}

fn kst_time(unix_secs: u64) -> String {
    let seconds = (unix_secs + 9 * 3600) % 86_400;
    format!("{:02}:{:02}:{:02}", seconds / 3600, (seconds / 60) % 60, seconds % 60)
}

fn timed_line(now: u64, id: &str, function: &str, attempt: &str, result: &str) -> Result<String, String> {
    Ok(format!("{} | {}", kst_time(now), log_line(id, function, attempt, result)?))
}

fn log_path(dir: &str, date: &str) -> PathBuf { Path::new(dir).join(".ai/logs").join(format!("{date}.log")) }

/// Project number from the top of an AGENTS.md header text.
fn header_project_id(text: &str) -> Option<u32> {
    let mut lines = text.lines();
    while let Some(l) = lines.next() {
        if l.trim() == "### grist" {
            return lines.next().and_then(|l| l.trim().strip_prefix("ID :")).and_then(|v| v.trim().parse().ok());
        }
    }
    None
}

fn timed_fields(line: &str) -> Option<(&str, &str, &str, &str, &str)> {
    let mut fields = line.splitn(5, " | ");
    let (time, id, function, attempt, result) =
        (fields.next()?, fields.next()?, fields.next()?, fields.next()?, fields.next()?);
    if !valid_time(time) || parse_id(id).is_err() || function.is_empty()
        || attempt.len() != 2 || !attempt.bytes().all(|b| b.is_ascii_digit()) { return None; }
    Some((time, id, function, attempt, result))
}

fn valid_time(time: &str) -> bool {
    let b = time.as_bytes();
    b.len() == 8 && b[2] == b':' && b[5] == b':' &&
        [0, 1, 3, 4, 6, 7].iter().all(|&i| b[i].is_ascii_digit()) &&
        &time[..2] < "24" && &time[3..5] < "60" && &time[6..8] < "60"
}

/// Only timestamp-prefixed lines for this ID, capped for display.
fn filter_log(text: &str, id: &str) -> Vec<String> {
    text.lines().filter(|l| timed_fields(l).is_some_and(|(_, found, _, _, _)| found == id))
        .take(20).map(|l| l.chars().take(200).collect()).collect()
}

fn parse_memo_line(text: &str) -> Option<Entry> {
    let text = text.trim();
    // Grist numbers memo history lines as `N. body`.
    let body = match text.split_once(". ") {
        Some((n, rest)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => rest,
        _ => text,
    };
    let rest = body.strip_prefix("에이전트 ")?;
    let mut chars = rest.chars();
    let agent = chars.next().filter(|c| AGENT_CHARS.contains(*c))?;
    let (role, conversation) = chars.as_str().strip_prefix(" : ")?.split_once(" · ")?;
    let split = conversation.rsplit_once(" ← ").and_then(|(c, tail)| {
        let (ch, name) = tail.split_once(' ')?;
        let mut cs = ch.chars();
        let first = cs.next().filter(|c| AGENT_CHARS.contains(*c) && cs.next().is_none())?;
        (!name.is_empty()).then(|| (c, (first, name.to_owned())))
    });
    let (conversation, caller) = match split { Some((c, who)) => (c, Some(who)), None => (conversation, None) };
    Some(Entry { agent, role: role.to_owned(), conversation: conversation.to_owned(), caller })
}

fn entries(memo: &[String]) -> Vec<Entry> { memo.iter().filter_map(|l| parse_memo_line(l)).collect() }

fn allocate(memo: &[String], role: &str) -> Result<char, String> {
    let used: Vec<char> = entries(memo).iter().map(|e| e.agent).collect();
    if role != "main" { return next_agent_char(&used); }
    if used.contains(&'0') { return Err("agent 0 (main) is already recorded".into()); }
    Ok('0')
}

fn resolve(memo: &[String], agent: char) -> Result<Entry, String> {
    entries(memo).into_iter().find(|e| e.agent == agent)
        .ok_or_else(|| format!("agent {agent} is not recorded in the row memo"))
}

fn grist() -> PathBuf {
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("ai/scripts/bin/grist")
}

fn run_grist(args: &[&str]) -> Result<String, String> {
    let out = Command::new(grist()).args(args).output().map_err(|e| format!("grist: {e}"))?;
    if !out.status.success() {
        return Err(format!("grist {} failed (exit {:?})", args[0], out.status.code()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn memo_write_args(dir: &str, row: u32, line: &str) -> [String; 6] {
    ["task-memo".into(), row.to_string(), "--project".into(), dir.into(), "--line".into(), line.into()]
}

fn memo_from_show(output: &str) -> Result<Vec<String>, String> {
    let value: serde_json::Value = serde_json::from_str(output).map_err(|e| format!("task-show output: {e}"))?;
    let lines = value.get("lines").and_then(|l| l.as_array()).ok_or("task-show output has no lines")?;
    Ok(lines.iter().filter_map(|l| l.as_str().map(str::to_owned)).collect())
}

fn read_memo(dir: &str, row: u32) -> Result<Vec<String>, String> {
    memo_from_show(&run_grist(&["task-show", &row.to_string(), "--project", dir, "--section", "메모"])?)
}

fn project_id(dir: &str) -> Result<u32, String> {
    let text = fs::read_to_string(Path::new(dir).join("AGENTS.md")).map_err(|e| format!("AGENTS.md: {e}"))?;
    let mut lines = text.lines();
    while let Some(l) = lines.next() {
        if l.trim() == "### grist" {
            let id = lines.next().and_then(|l| l.trim().strip_prefix("ID :")).map(str::trim);
            return id.and_then(|v| v.parse().ok()).ok_or_else(|| "AGENTS.md grist ID is missing".into());
        }
    }
    Err("AGENTS.md has no grist ID".into())
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn start(dir: &str, row: u32, role: &str, conversation: &str, caller: Option<(char, &str)>) -> Result<String, String> {
    let project = project_id(dir)?;
    // Serialize read-allocate-write per row so parallel agents cannot share a character.
    let lock_path = env::temp_dir().join(format!("record-log-{project}-{row}.lock"));
    let lock = fs::OpenOptions::new().create(true).write(true).open(lock_path).map_err(|e| format!("lock: {e}"))?;
    lock.lock().map_err(|e| format!("lock: {e}"))?;
    let agent = allocate(&read_memo(dir, row)?, role)?;
    let line = memo_line(agent, role, conversation, caller)?;
    let id = make_id(project, row, agent)?;
    let args = memo_write_args(dir, row, &line);
    run_grist(&args.each_ref().map(String::as_str))?;
    Ok(id)
}

const ROOTS: &[&str] = &["/home/tree/extensions", "/home/tree/test"];

/// Project directory whose AGENTS.md header carries `ID : <project>`; reads only the first lines.
fn find_project_dir(roots: &[&Path], project: u32) -> Result<String, String> {
    use std::io::{BufRead, BufReader};
    let mut dirs = vec![];
    for root in roots.iter() {
        dirs.push(root.to_path_buf());
        if let Ok(rd) = fs::read_dir(root) { dirs.extend(rd.flatten().map(|e| e.path()).filter(|p| p.is_dir())); }
    }
    dirs.sort();
    for d in dirs {
        let Ok(f) = fs::File::open(d.join("AGENTS.md")) else { continue };
        let head: Vec<String> = BufReader::new(f).lines().take(8).map_while(Result::ok).collect();
        if header_project_id(&head.join("\n")) == Some(project) { return Ok(d.to_string_lossy().into_owned()); }
    }
    Err(format!("no project with grist ID {project}"))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn project_dir(explicit: Option<&str>, roots: &[&Path], project: u32) -> Result<String, String> {
    match explicit {
        Some(dir) => Ok(dir.to_owned()),
        None => find_project_dir(roots, project),
    }
}

fn root_paths() -> Vec<&'static Path> { ROOTS.iter().map(Path::new).collect() }

fn append(id: &str, function: &str, attempt: &str, result: &str, explicit: Option<&str>) -> Result<String, String> {
    use std::io::Write;
    let now = now_secs();
    let line = timed_line(now, id, function, attempt, result)?;
    let dir = project_dir(explicit, &root_paths(), parse_id(id)?.project)?;
    let path = log_path(&dir, &kst_date(now));
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| format!("logs dir: {e}"))?;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| format!("log: {e}"))?;
    writeln!(f, "{line}").map_err(|e| format!("log: {e}"))?;
    Ok(line)
}

#[derive(Clone)]
struct LogEvent {
    time: String,
    stage: String,
    ack: String,
    error: String,
    body: Vec<String>,
}

fn token<'a>(result: &'a str, key: &str) -> Option<&'a str> {
    result.split_whitespace().find_map(|word| word.strip_prefix(key))
}

fn error_value(result: &str) -> Option<&str> {
    result.match_indices("error=").find_map(|(index, _)| {
        if index == 0 || result[..index].chars().last().is_some_and(char::is_whitespace) {
            Some(&result[index + "error=".len()..])
        } else {
            None
        }
    })
}

fn latest_lines(text: &str, fn_filter: Option<&str>, since: Option<&str>) -> Vec<String> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<(String, String), Vec<LogEvent>> = BTreeMap::new();
    for line in text.lines() {
        let Some((time, id, function, _, result)) = timed_fields(line) else { continue };
        if fn_filter.is_some_and(|name| name != function) || since.is_some_and(|start| time < start) { continue; }
        let mut body: Vec<String> = result.split_whitespace().filter(|word| word.starts_with("body=") || word.starts_with("body."))
            .map(str::to_owned).collect();
        body.sort();
        groups.entry((id.into(), function.into())).or_default().push(LogEvent {
            time: time.into(),
            stage: token(result, "stage=").unwrap_or("-").into(),
            ack: token(result, "ack=").unwrap_or("-").into(),
            error: error_value(result).unwrap_or("-").into(),
            body,
        });
    }
    let mut lines = vec![];
    for ((id, function), mut events) in groups {
        events.sort_by(|a, b| a.time.cmp(&b.time));
        let last = events.last().unwrap();
        let last_ok = events.iter().rev().find(|event| event.error == "none").map(|event| event.time.as_str()).unwrap_or("-");
        let fails = events.iter().rev().take_while(|event| event.error != "none").count();
        let failures: Vec<_> = events.iter().filter(|event| event.error != "none").collect();
        let fixed = failures.len() >= 2 && failures[0].ack != "-" && !failures[0].body.is_empty()
            && failures.iter().all(|event| event.ack == failures[0].ack && event.body == failures[0].body);
        let warnings = format!("{}{}",
            if failures.iter().any(|event| event.stage == "-") { " warn=no_stage" } else { "" },
            if fixed { " warn=fixed_ack_body" } else { "" });
        let (mut function, mut stage, mut ack, mut error) =
            (function, last.stage.clone(), last.ack.clone(), last.error.clone());
        let line = loop {
            let line = format!("{} | {id} | {function} | last={} stage={stage} ack={ack} error={error} last_ok={last_ok} fails={fails}{warnings}",
                last.time, if last.error == "none" { "ok" } else { "fail" });
            if line.chars().count() <= 200 { break line; }
            let field = [&mut error, &mut function, &mut stage, &mut ack].into_iter()
                .find(|value| value.chars().count() > 1);
            if let Some(value) = field { value.pop(); } else { break line.chars().take(200).collect(); }
        };
        lines.push((last.time.clone(), line));
    }
    lines.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    lines.into_iter().take(20).map(|(_, line)| line).collect()
}

fn latest(explicit: Option<&str>, fn_filter: Option<&str>, since: Option<&str>, now: u64) -> Result<String, String> {
    let since = since.map(|value| {
        let full = format!("{value}:00");
        if value.len() == 5 && valid_time(&full) { Ok(full) } else { Err("--since must be HH:MM".to_owned()) }
    }).transpose()?;
    let dir = match explicit {
        Some(dir) => dir.to_owned(),
        None => {
            let cwd = env::current_dir().map_err(|e| format!("cwd: {e}"))?;
            let project = project_id(cwd.to_str().ok_or("cwd is not UTF-8")?)?;
            project_dir(None, &root_paths(), project)?
        }
    };
    let path = log_path(&dir, &kst_date(now));
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("log: {e}")),
    };
    Ok(latest_lines(&text, fn_filter, since.as_deref()).join("\n"))
}

fn changes_has_row(dir: &str, row: u32) -> bool {
    let marker = format!("(#{row})");
    fs::read_dir(Path::new(dir).join(".ai/changes")).into_iter().flatten().flatten()
        .any(|e| fs::read_to_string(e.path()).map(|t| t.contains(&marker)).unwrap_or(false))
}

fn find(id: &str, explicit: Option<&str>) -> Result<String, String> {
    let parsed = parse_id(id)?;
    let dir = project_dir(explicit, &root_paths(), parsed.project)?;
    if !changes_has_row(&dir, parsed.row) { return Err(format!("row {} is not in the project changes", parsed.row)); }
    let e = resolve(&read_memo(&dir, parsed.row)?, parsed.agent)?;
    let mut out = vec![match &e.caller {
        Some((c, n)) => format!("에이전트 {} : {} · {} ← {c} {n}", e.agent, e.role, e.conversation),
        None => format!("에이전트 {} : {} · {}", e.agent, e.role, e.conversation),
    }];
    let mut logs: Vec<_> = fs::read_dir(Path::new(&dir).join(".ai/logs")).into_iter().flatten().flatten()
        .map(|x| x.path()).filter(|p| p.extension().is_some_and(|x| x == "log")).collect();
    logs.sort();
    let mut found = vec![];
    for p in logs { if let Ok(t) = fs::read_to_string(&p) { found.extend(filter_log(&t, id)); } }
    found.truncate(20);
    out.extend(found);
    Ok(out.join("\n"))
}

fn run(args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("start") if args.len() >= 3 => {
            let row = args[2].parse::<u32>().map_err(|_| "row must be a number")?;
            let role = flag(args, "--role").ok_or("--role is required")?;
            let conversation = flag(args, "--conversation").unwrap_or_else(|| "unknown".into());
            let caller = args.iter().position(|a| a == "--caller").map(|i| match (args.get(i + 1), args.get(i + 2)) {
                (Some(c), Some(n)) if c.chars().count() == 1 => Ok((c.chars().next().unwrap(), n.as_str())),
                _ => Err("--caller needs <char> <name>"),
            }).transpose()?;
            start(&args[1], row, &role, &conversation, caller)
        }
        Some("parse") if args.len() >= 2 => {
            let parsed = parse_id(&args[1])?;
            let dir = flag(args, "--project").ok_or("--project DIR is required")?;
            if project_id(&dir)? != parsed.project { return Err("ID project does not match --project".into()); }
            let e = resolve(&read_memo(&dir, parsed.row)?, parsed.agent)?;
            Ok(format!("project={} | row={} | agent={} | role={} | conversation={}{}",
                parsed.project, parsed.row, parsed.agent, e.role, e.conversation,
                e.caller.map(|(c, n)| format!(" | caller={c} {n}")).unwrap_or_default()))
        }
        Some("append") if args.len() == 5 || args.len() == 7 => append(&args[1], &args[2], &args[3], &args[4], flag(args, "--project").as_deref()),
        Some("latest") => {
            let mut project = None;
            let mut function = None;
            let mut since = None;
            let mut options = args[1..].chunks_exact(2);
            for pair in &mut options {
                let slot = match pair[0].as_str() {
                    "--project" => &mut project,
                    "--fn" => &mut function,
                    "--since" => &mut since,
                    _ => return Err("usage: latest [--project DIR] [--fn NAME] [--since HH:MM]".into()),
                };
                if slot.is_some() || pair[1].starts_with("--") { return Err("duplicate or missing latest option value".into()); }
                *slot = Some(pair[1].as_str());
            }
            if !options.remainder().is_empty() { return Err("latest option needs a value".into()); }
            latest(project, function, since, now_secs())
        }
        Some("find") if args.len() == 2 || args.len() == 4 => find(&args[1], flag(args, "--project").as_deref()),
        Some("log-line") if args.len() == 5 => log_line(&args[1], &args[2], &args[3], &args[4]),
        _ => Err("usage: record-log start PROJECT_DIR ROW --role NAME [--conversation ID] [--caller CHAR NAME] | parse ID --project DIR | log-line ID FUNCTION ATTEMPT RESULT | append ID FUNCTION ATTEMPT RESULT [--project DIR] | find ID [--project DIR] | latest [--project DIR] [--fn NAME] [--since HH:MM]".into()),
    }
}

fn main() {
    match run(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(line) => println!("{line}"),
        Err(error) => { eprintln!("record-log: {error}"); std::process::exit(2); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn valid_line() {
        assert_eq!(log_line("0011980237A", "grist.task_add", "01", "ok").unwrap(),
            "0011980237A | grist.task_add | 01 | ok");
    }
    #[test] fn long_json_rejected() {
        let r = format!("{{\"error\":\"{}\"}}", "x".repeat(200));
        assert!(validate_result(&r).is_err());
    }
    #[test] fn short_json_rejected() {
        assert!(validate_result("{\"error\":\"x\"}").is_err());
        assert!(validate_result("[1]").is_err());
    }
    #[test] fn over_120_rejected() {
        assert!(validate_result(&"a".repeat(121)).is_err());
        assert!(validate_result(&"a".repeat(120)).is_ok());
    }
    #[test] fn newline_rejected() { assert!(validate_result("a\nb").is_err()); }
    #[test] fn token_rejected() {
        assert!(validate_result("failed Bearer abc").is_err());
        assert!(validate_result("Authorization: x").is_err());
        assert!(validate_result("api_key=abc123").is_err());
        assert!(validate_result("sk-abcdefghijklmnopqrstuv").is_err());
        assert_eq!(validate_result("http_401"), Ok(()));
    }
    #[test] fn id_round_trip() {
        let id = make_id(1198, 238, 'A').unwrap();
        assert_eq!(id, "0011980238A");
        assert_eq!(parse_id(&id).unwrap(), Parsed { project: 1198, row: 238, agent: 'A' });
    }
    #[test] fn thirty_fourth_agent_errors() {
        let mut used = vec![];
        for _ in 0..33 { used.push(next_agent_char(&used).unwrap()); }
        assert!(next_agent_char(&used).is_err());
    }
    #[test] fn o_and_i_never_allocated() {
        let mut used = vec![];
        for _ in 0..33 { used.push(next_agent_char(&used).unwrap()); }
        assert!(!used.contains(&'O') && !used.contains(&'I'));
        assert_eq!(used[0], '1');
        assert!(!used.contains(&'0'));
    }
    #[test] fn invalid_parts_do_not_make_log_lines() {
        assert!(log_line("0011980237I", "fn", "01", "ok").is_err());
        assert!(log_line("0011980237A", "fn", "1", "ok").is_err());
        assert!(log_line("0011980237A", "fn", "01", "failed | token").is_err());
    }
    fn lines(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }
    #[test] fn memo_line_format() {
        assert_eq!(memo_line('1', "t26_240", "codex://thread-one", None).unwrap(),
            "에이전트 1 : t26_240 · codex://thread-one");
        assert!(memo_line('O', "r", "c", None).is_err());
        assert!(memo_line('1', "a · b", "c", None).is_err());
        assert!(memo_line('1', "r", "c\nd", None).is_err());
    }
    #[test] fn parse_numbered_memo_line() {
        let e = parse_memo_line("2. 에이전트 B : reviewer · codex://x · y").unwrap();
        assert_eq!(e, Entry { agent: 'B', role: "reviewer".into(), conversation: "codex://x · y".into(), caller: None });
        assert_eq!(parse_memo_line("1. 다른 메모"), None);
    }
    #[test] fn allocation_from_memo() {
        assert_eq!(allocate(&[], "t26_1").unwrap(), '1');
        assert_eq!(allocate(&[], "main").unwrap(), '0');
        let main_only = lines(&["1. 에이전트 0 : main · c0"]);
        assert_eq!(allocate(&main_only, "t26_1").unwrap(), '1');
        assert!(allocate(&main_only, "main").is_err());
        let memo = lines(&["1. 에이전트 0 : main · c0", "2. 일반 메모", "3. 에이전트 1 : a · c1"]);
        assert_eq!(allocate(&memo, "x").unwrap(), '2');
    }
    #[test] fn exhaustion_from_memo() {
        let memo: Vec<String> = AGENT_CHARS.chars().enumerate()
            .map(|(i, c)| format!("{}. 에이전트 {c} : r · c", i + 1)).collect();
        assert_eq!(memo.len(), 34);
        assert!(allocate(&memo, "r").is_err());
        assert!(allocate(&memo, "main").is_err());
    }
    #[test] fn resolve_from_memo() {
        let memo = lines(&["1. 에이전트 0 : main · c0", "2. 에이전트 1 : a · c1"]);
        assert_eq!(resolve(&memo, '1').unwrap().conversation, "c1");
        assert!(resolve(&memo, '5').is_err());
    }
    fn tmp_project(base: &Path, name: &str, id: u32) {
        let d = base.join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("AGENTS.md"), format!("# 상태\n### grist\nID : {id}\n")).unwrap();
    }
    #[test] fn project_lookup_scans_all_roots() {
        let base = env::temp_dir().join(format!("rl-roots-{}", std::process::id()));
        let (a, b) = (base.join("a"), base.join("b"));
        tmp_project(&a, "p1", 11);
        tmp_project(&b, "p2", 22);
        let roots: Vec<&Path> = vec![a.as_path(), b.as_path()];
        let found = find_project_dir(&roots, 22).unwrap();
        let missing = find_project_dir(&roots, 33).unwrap_err();
        fs::remove_dir_all(&base).unwrap();
        assert!(found.ends_with("b/p2"), "{found}");
        assert_eq!(missing, "no project with grist ID 33");
    }
    #[test] fn explicit_project_skips_scan() {
        assert_eq!(project_dir(Some("/x/y"), &[], 5).unwrap(), "/x/y");
        assert!(project_dir(None, &[], 5).is_err());
    }
    #[test] fn roots_include_test_dir() {
        assert!(ROOTS.contains(&"/home/tree/test") && ROOTS[0] == "/home/tree/extensions");
    }
    #[test] fn run_rejects_unknown_command() { assert!(run(&["make".into()]).is_err()); }
    #[test] fn memo_from_task_show_json() {
        let out = "{\n \"id\": 1,\n \"section\": \"메모\",\n \"lines\": [\"1. 에이전트 0 : main · c0\"]\n}";
        assert_eq!(memo_from_show(out).unwrap(), vec!["1. 에이전트 0 : main · c0".to_string()]);
        assert!(memo_from_show("not json").is_err());
    }
    #[test] fn memo_line_with_caller() {
        assert_eq!(memo_line('2', "t26_241", "conv-9", Some(('0', "main"))).unwrap(),
            "에이전트 2 : t26_241 · conv-9 ← 0 main");
        assert!(memo_line('2', "r", "c", Some(('O', "main"))).is_err());
        assert!(memo_line('2', "r", "c", Some(('0', "a\nb"))).is_err());
    }
    #[test] fn memo_write_uses_changes_command() {
        let line = "에이전트 1 : codex_338 · 4910ec9c ← 0 main";
        assert_eq!(memo_write_args("/project", 338, line),
            ["task-memo", "338", "--project", "/project", "--line", line]);
    }
    #[test] fn parse_caller_and_legacy() {
        let e = parse_memo_line("3. 에이전트 2 : t26_241 · conv-9 ← 0 main").unwrap();
        assert_eq!(e, Entry { agent: '2', role: "t26_241".into(), conversation: "conv-9".into(), caller: Some(('0', "main".into())) });
        let e = parse_memo_line("1. 에이전트 1 : a · c · d").unwrap();
        assert_eq!((e.conversation.as_str(), e.caller), ("c · d", None));
    }
    #[test] fn kst_date_rolls_over_at_1500_utc() {
        assert_eq!(kst_date(1_792_565_121), "261021"); // 2026-10-21T06:45:21Z
        assert_eq!(kst_date(1_792_594_799), "261021"); // 2026-10-21T14:59:59Z
        assert_eq!(kst_date(1_792_594_800), "261022"); // 2026-10-21T15:00:00Z
    }
    #[test] fn log_path_under_ai_logs() {
        assert_eq!(log_path("/p/x", "261021"), PathBuf::from("/p/x/.ai/logs/261021.log"));
    }
    #[test] fn header_project_id_parsing() {
        assert_eq!(header_project_id("# 상태\n### grist\nID : 1205\n### obsidian\n"), Some(1205));
        assert_eq!(header_project_id("# 정보\nID : 9\n"), None);
        assert_eq!(header_project_id("# 상태\n### grist\nID : x\n"), None);
    }
    #[test] fn filter_log_matches_caps_and_truncates() {
        let id = "0011980241A";
        let mut text = format!("{id} | f | 01 | old\n10:00:00 | 0011980241B | f | 01 | {id}\n");
        for i in 0..30 { text.push_str(&format!("10:00:{i:02} | {id} | f | {i:02} | ok\n")); }
        text.push_str(&format!("10:00:59 | {id} | f | 99 | {}\n", "y".repeat(300)));
        let out = filter_log(&text, id);
        assert_eq!(out.len(), 20);
        assert_eq!(out[0], format!("10:00:00 | {id} | f | 00 | ok"));
        assert!(out.iter().all(|l| l.chars().count() <= 200));
        let long = filter_log(&format!("10:00:59 | {id} | f | 99 | {}\n", "y".repeat(300)), id);
        assert_eq!(long[0].chars().count(), 200);
        assert!(filter_log("nothing\n", id).is_empty());
    }

    #[test] fn append_line_uses_kst_time_from_now() {
        assert_eq!(timed_line(1_792_594_800, "00119803801", "record.append", "01", "ack=received error=none").unwrap(),
            "00:00:00 | 00119803801 | record.append | 01 | ack=received error=none");
        assert_eq!(kst_date(1_792_594_800), "261022");
    }

    #[test] fn latest_groups_judges_and_warns() {
        let text = concat!(
            "09:00:00 | 00119803801 | send | 01 | ack=received error=none\n",
            "09:01:00 | 00119803801 | send | 02 | ack=not_observed body=none error=timeout\n",
            "09:02:00 | 00119803801 | send | 03 | ack=not_observed body=none error=timeout\n",
            "09:03:00 | 00119803801 | other | 01 | stage=send ack=rejected body.status=failed error=bad\n",
            "00119803801 | send | 04 | old\n");
        let lines = latest_lines(text, None, None);
        assert_eq!(lines, vec![
            "09:03:00 | 00119803801 | other | last=fail stage=send ack=rejected error=bad last_ok=- fails=1",
            "09:02:00 | 00119803801 | send | last=fail stage=- ack=not_observed error=timeout last_ok=09:00:00 fails=2 warn=no_stage warn=fixed_ack_body",
        ]);
        assert_eq!(latest_lines(text, Some("send"), Some("09:02:00")), vec![
            "09:02:00 | 00119803801 | send | last=fail stage=- ack=not_observed error=timeout last_ok=- fails=1 warn=no_stage",
        ]);
    }

    #[test] fn latest_keeps_multiword_error_value() {
        let text = concat!(
            "09:00:00 | 00119803801 | send | 01 | ack=received error=none\n",
            "09:01:00 | 00119803801 | send | 02 | stage=send ack=not_observed body=none error=too long\n",
        );
        assert_eq!(latest_lines(text, None, None), vec![
            "09:01:00 | 00119803801 | send | last=fail stage=send ack=not_observed error=too long last_ok=09:00:00 fails=1",
        ]);
    }

    #[test] fn latest_limits_groups_and_line_length() {
        let mut text = String::new();
        for i in 0..25 {
            text.push_str(&format!("10:{i:02}:00 | 00119803801 | fn{i} | 01 | stage=send ack=rejected body=none error={}\n", "x".repeat(120)));
        }
        let lines = latest_lines(&text, None, None);
        assert_eq!(lines.len(), 20);
        assert!(lines[0].starts_with("10:24:00"));
        assert!(lines.iter().all(|line| line.chars().count() <= 200));
    }

    #[test] fn latest_keeps_spaces_in_error_value() {
        let text = "11:00:00 | 00119803801 | send | 01 | ack=received body.status=failed stage=receiver_reject error=too long\n";
        assert_eq!(latest_lines(text, None, None), vec![
            "11:00:00 | 00119803801 | send | last=fail stage=receiver_reject ack=received error=too long last_ok=- fails=1",
        ]);
        assert_eq!(error_value("prefixerror=wrong ack=received error=too long"), Some("too long"));
    }

    #[test] fn latest_trims_error_to_keep_200_character_line() {
        let function = "extraordinarily_long_receiver_function";
        let result = format!("ack=received body.status=failed stage=receiver_reject error=too long {}", "x".repeat(105));
        let text = format!("11:00:00 | 00119803801 | {function} | 01 | {result}\n");
        let line = latest_lines(&text, None, None).remove(0);
        assert_eq!(line.chars().count(), 200);
        assert!(line.contains("error=too long "));
        assert!(line.ends_with("last_ok=- fails=1"));
        assert!(!line.contains(&"x".repeat(105)));
    }
}
