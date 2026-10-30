use grist::api::{self, Exit, R, PROJECT_TABLE, TASK_TABLE};
use grist::{gate, records, task, util};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
mod help;

struct ParsedArgs {
    command: String,
    pos: Vec<String>,
    flags: HashMap<String, Vec<String>>,
}

fn help_hint(command: Option<&str>) -> String {
    match command.filter(|name| help::COMMANDS.iter().any(|c| c.name == *name)) {
        Some(name) => format!(" (grist {name} --help)"),
        None => " (grist --help)".into(),
    }
}

fn usage_error<T>(msg: &str) -> R<T> {
    let command = std::env::args().nth(1).unwrap_or_default();
    eprintln!("error: {msg}{}", help_hint(Some(&command)));
    std::process::exit(2);
}

fn parse_cli(argv: &[String]) -> R<ParsedArgs> {
    if argv.is_empty() || argv.iter().any(|a| a == "-h" || a == "--help") {
        if argv.iter().any(|a| a == "-h" || a == "--help") {
            match help::render(argv.first().filter(|s| !s.starts_with('-')).map(String::as_str)) {
                Ok(output) => { println!("{output}"); std::process::exit(0); }
                Err(message) => return usage_error(&message),
            }
        }
        return usage_error("the following arguments are required: command");
    }
    let command = argv[0].clone();
    let mut pos = Vec::new();
    let mut flags: HashMap<String, Vec<String>> = HashMap::new();
    let mut i = 1;
    while i < argv.len() {
        let arg = &argv[i];
        if let Some(rest) = arg.strip_prefix("--") {
            if command == "task" && rest == "cached" {
                flags.entry(rest.to_string()).or_default();
                i += 1;
                continue;
            }
            if let Some((k, v)) = rest.split_once('=') {
                flags.entry(k.to_string()).or_default().push(v.to_string());
                i += 1;
            } else {
                if i + 1 >= argv.len() {
                    return usage_error(&format!("argument --{rest}: expected one argument"));
                }
                flags.entry(rest.to_string()).or_default().push(argv[i + 1].clone());
                i += 2;
            }
        } else {
            pos.push(arg.clone());
            i += 1;
        }
    }
    Ok(ParsedArgs { command, pos, flags })
}

impl ParsedArgs {
    fn pos_str(&self, idx: usize, name: &str) -> R<&str> {
        self.pos
            .get(idx)
            .map(|s| s.as_str())
            .ok_or_else(|| {
                eprintln!("error: the following arguments are required: {name}");
                std::process::exit(2);
            })
    }

    fn pos_i64(&self, idx: usize, name: &str) -> R<i64> {
        let s = self.pos_str(idx, name)?;
        s.parse::<i64>().map_err(|_| {
            eprintln!("error: argument {name}: invalid int value: '{s}'");
            std::process::exit(2);
        })
    }

    fn flag_opt(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(|v| v.last()).map(|s| s.as_str())
    }

    fn flag_req(&self, name: &str) -> R<&str> {
        self.flag_opt(name).ok_or_else(|| {
            eprintln!("error: the following arguments are required: --{name}");
            std::process::exit(2);
        })
    }

    fn flag_i64_req(&self, name: &str) -> R<i64> {
        let s = self.flag_req(name)?;
        s.parse::<i64>().map_err(|_| {
            eprintln!("error: argument --{name}: invalid int value: '{s}'");
            std::process::exit(2);
        })
    }

    fn flag_i64_opt(&self, name: &str) -> R<Option<i64>> {
        let Some(s) = self.flag_opt(name) else {
            return Ok(None);
        };
        let n = s.parse::<i64>().map_err(|_| {
            eprintln!("error: argument --{name}: invalid int value: '{s}'");
            std::process::exit(2);
        })?;
        Ok(Some(n))
    }

    fn flag_i64_list(&self, name: &str) -> R<Option<Vec<i64>>> {
        let Some(list) = self.flags.get(name) else {
            return Ok(None);
        };
        let mut out = Vec::new();
        for s in list {
            let n = s.parse::<i64>().map_err(|_| {
                eprintln!("error: argument --{name}: invalid int value: '{s}'");
                std::process::exit(2);
            })?;
            out.push(n);
        }
        Ok(Some(out))
    }
}

fn parse_fields_obj(raw: &str, flag_name: &str) -> R<Map<String, Value>> {
    let val = api::parse_json(raw, flag_name)?;
    val.as_object()
        .cloned()
        .ok_or_else(|| Exit::Other(format!("{flag_name} must be a JSON object")))
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|v| util::strip(v).to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

fn cwd_str() -> String {
    std::env::current_dir()
        .map(|p| util::path_str(&p))
        .unwrap_or_else(|_| ".".to_string())
}

fn run(argv: &[String]) -> R<Value> {
    let args = parse_cli(argv)?;
    records::set_hook_project(args.flag_opt("project"));

    match args.command.as_str() {
        "tables" => api::tables(),
        "columns" => {
            let table = args.pos_str(0, "table")?;
            api::columns(table)
        }
        "get" => {
            let table = args.pos_str(0, "table")?;
            let ids = args.flag_i64_list("id")?;
            let filter = match args.flag_opt("filter") {
                Some(f) => Some(parse_fields_obj(f, "--filter")?),
                None => None,
            };
            let fields: Option<Vec<String>> = args
                .flag_opt("fields")
                .map(|s| s.split(',').map(|x| x.to_string()).collect());
            let recs = api::get(table, ids.as_deref(), filter.as_ref(), fields.as_deref())?;
            Ok(Value::Array(recs))
        }
        "add" => {
            let table = args.pos_str(0, "table")?;
            let fields = parse_fields_obj(args.flag_req("fields")?, "--fields")?;
            if table == TASK_TABLE {
                api::validate_task_fields(&fields)?;
            }
            api::add(table, &fields)
        }
        "update" => {
            let table = args.pos_str(0, "table")?;
            let row_id = args.flag_i64_req("id")?;
            let fields = parse_fields_obj(args.flag_req("fields")?, "--fields")?;
            if table == TASK_TABLE {
                api::validate_task_fields(&fields)?;
            }
            api::update(table, row_id, &fields)
        }
        "delete" => {
            let table = args.pos_str(0, "table")?;
            let row_id = args.flag_i64_req("id")?;
            api::delete(table, row_id)
        }
        "column-update" => {
            let table = args.pos_str(0, "table")?;
            let column = args.pos_str(1, "column")?;
            let fields = parse_fields_obj(args.flag_req("fields")?, "--fields")?;
            api::column_update(table, column, &fields)
        }
        "project" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let recs = api::get(PROJECT_TABLE, Some(&[row_id]), None, None)?;
            Ok(Value::Array(recs))
        }
        "task" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = cache_project(args.flag_opt("project"));
            if args.flags.contains_key("cached") {
                if let Some(line) = cached_row_output(&project, row_id)? {
                    println!("{line}");
                    std::process::exit(0);
                }
            }
            let recs = api::get(TASK_TABLE, Some(&[row_id]), None, None)?;
            if let Some(fields) = recs.first().and_then(|r| r.get("fields")) {
                let line = row_state(fields);
                write_row_cache(&project, row_id, &line)?;
                if args.flags.contains_key("cached") {
                    println!("{line}");
                    std::process::exit(0);
                }
            }
            Ok(Value::Array(recs))
        }
        "task-update" => {
            let row_id = args.pos_i64(0, "row_id")?;
            if gate::actor().0 == "verifier" { gate::require_row("task-update", row_id)?; }
            let mut fields = parse_fields_obj(args.flag_req("fields")?, "--fields")?;
            api::validate_task_fields(&fields)?;
            if !gate::worker_update_allowed(
                std::env::var("T26_ROLE").ok().as_deref(), gate::worker_row(), row_id, &fields
            ) {
                return api::fail("actor", "worker task-update may write only D, I, J and H=진단/교정 on its own row");
            }
            records::single_line_cells(&mut fields)?;
            records::check_symbol_fields(&fields)?;
            records::check_diagnosis_fields(&fields)?;
            let title_project = if fields.contains_key("B") {
                let default_cwd = cwd_str();
                let project = args.flag_opt("project").unwrap_or(&default_cwd);
                Some(records::owned(project, row_id)?.0)
            } else { None };
            if let Some(title) = fields.get("B").and_then(Value::as_str) {
                fields.insert("B".to_string(), Value::String(records::normalize_detail_title(title)));
            }
            let record = api::update(TASK_TABLE, row_id, &fields)?;
            if let (Some(project), Some(title)) = (title_project.as_deref(), fields.get("B").and_then(Value::as_str)) {
                records::update_item_title(project, row_id, title)?;
            }
            let mut result = with_row_state(record, args.flag_opt("project"))?;
            if fields.get("H").and_then(Value::as_str) == Some("진단") {
                result.as_object_mut().unwrap().insert("next".into(), Value::String(task::STAGE_START_CARD.into()));
            }
            Ok(result)
        }
        "task-add" => {
            let fields = parse_fields_obj(args.flag_req("fields")?, "--fields")?;
            api::validate_task_fields(&fields)?;
            let project = args.flag_opt("project");
            let kind = args.flag_opt("type");
            if let Some(k) = kind {
                if records::change_type_code(k).is_none() {
                    return usage_error(&format!("invalid choice: '{k}'"));
                }
            }
            let judge = args.flag_opt("judge").unwrap_or("");
            let goal_args = match task_add_goal_args(project, args.flag_opt("goal-file"), args.flag_opt("summary")) {
                Ok(g) => g,
                Err(msg) => return usage_error(&msg),
            };
            let checked = match (project, goal_args) {
                (Some(p), Some((file, summary))) => {
                    Some(gate::check_goal(&util::resolve(std::path::Path::new(p)), None, file, summary)?)
                }
                _ => None,
            };
            let warnings = checked.as_ref().map(|(goal, _)| records::goal_conjunction_warnings(goal)).unwrap_or_default();
            let mut added = records::task_add(fields, project, kind, judge)?;
            if let (Some(p), Some((goal, summary)), Some(id)) = (project, checked, added.get("id").and_then(|v| v.as_i64())) {
                gate::store_goal(&util::resolve(std::path::Path::new(p)), id, &goal, &summary)?;
            }
            if let Some(id) = added.get("id").and_then(|v| v.as_i64()) {
                let keys: Vec<String> = ROW_STATE_KEYS.iter().map(|k| k.to_string()).collect();
                if let Some(rec) = api::get(TASK_TABLE, Some(&[id]), None, Some(&keys))?.into_iter().next() {
                    if let (Some(obj), Some(f)) = (added.as_object_mut(), rec.get("fields")) {
                        let line = row_state(f);
                        write_row_cache(&cache_project(project), id, &line)?;
                        obj.insert("row".into(), Value::String(line));
                    }
                }
            }
            records::attach_warnings(&mut added, warnings);
            added.as_object_mut().unwrap().insert("next".into(), Value::String(task::STAGE_START_CARD.into()));
            Ok(added)
        }
        "gate" => {
            let default_cwd = cwd_str();
            let cwd = args.flag_opt("cwd").unwrap_or(&default_cwd);
            let row = args.flag_i64_opt("row")?;
            gate::gate(cwd, row)
        }
        "lists" => {
            let project = args.flag_req("project")?;
            let lists = grist::task::lists_text(&util::resolve(std::path::Path::new(project)));
            if lists.is_empty() {
                return api::fail("records", "no `# 작업`/`# 완료` list in .ai/changes");
            }
            println!("{lists}");
            std::process::exit(0);
        }
        "request" => {
            let project = args.flag_req("project")?;
            let block = records::latest_request_block(&util::resolve(std::path::Path::new(project)));
            if block.is_empty() {
                return api::fail("records", "no `# 요청` block in .ai/changes; record the request with request-record");
            }
            println!("{block}");
            std::process::exit(0);
        }
        "goal" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            println!("{}", records::goal_text(project, row_id)?);
            std::process::exit(0);
        }
        "task-goal" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let goal_file = args.flag_req("goal-file")?;
            let summary = args.flag_req("summary")?;
            with_row_state(gate::task_goal(row_id, project, goal_file, summary)?, Some(project))
        }
        "task-red" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let test = args.flag_req("test")?;
            let tests = split_csv(args.flag_req("tests")?);
            let paths = split_csv(args.flag_opt("paths").unwrap_or(""));
            let purpose = args.flag_req("purpose")?;
            let detail = args.flag_opt("detail").unwrap_or("");
            let agent_id = args.flag_opt("agent-id");
            gate::task_red(row_id, project, test, &paths, &tests, purpose, detail, agent_id)
        }
        "task-ready" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let test = args.flag_req("test")?;
            let tests = split_csv(args.flag_req("tests")?);
            let paths = split_csv(args.flag_req("paths")?);
            let purpose = args.flag_req("purpose")?;
            let agent_id = args.flag_opt("agent-id");
            gate::task_ready(row_id, project, test, &paths, &tests, purpose, agent_id)
        }
        "task-hidden" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let purpose = args.flag_req("purpose")?;
            let test = args.flag_req("test")?;
            let detail = args.flag_opt("detail").unwrap_or("");
            gate::task_hidden(row_id, project, purpose, test, detail)
        }
        "task-submit" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            gate::task_submit(row_id, project)
        }
        "flow-wait" => {
            let project = args.flag_req("project")?;
            gate::flow_wait(project)
        }
        "task-show" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let section = args.flag_opt("section");
            records::task_show(project, row_id, section)
        }
        "task-memo" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let line = args.flag_req("line")?;
            records::task_memo(project, row_id, line)
        }
        "task-step" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let step = args.flag_req("step")?;
            let text = args.flag_req("text")?;
            let goal_arg = args.flag_req("goal")?;
            let Ok(goal) = goal_arg.parse::<usize>() else {
                return api::fail("validation", format!("--goal must be a goal line number, got `{goal_arg}`"));
            };
            records::task_step(project, row_id, step, text, goal)
        }
        "task-confirm" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let summary = args.flag_req("summary")?;
            let check = args.flag_req("check")?;
            let detail = args.flag_req("detail")?;
            let can = args.flag_opt("can");
            let evidence_file = args.flag_req("evidence")?;
            let evidence = match std::fs::read_to_string(evidence_file) {
                Ok(text) => text,
                Err(e) => return api::fail("validation", format!("--evidence {evidence_file}: {e}")),
            };
            records::task_confirm(project, row_id, summary, check, detail, &evidence, can)
        }
        "request-record" => {
            let project = args.flag_req("project")?;
            let input_file = args.flag_req("input-file")?;
            let title = args.flag_req("title")?;
            let summary = args.flag_req("summary")?;
            records::request_record(project, input_file, title, summary)
        }
        "sync" => {
            let default_cwd = cwd_str();
            let project = args.flag_opt("project");
            let cwd = args.flag_opt("cwd").unwrap_or(&default_cwd);
            records::sync(project, cwd)
        }
        other => usage_error(&format!("invalid choice: '{other}'")),
    }
}

/// `task-add --project` needs both `--goal-file` and `--summary`; without `--project` neither is used.
fn task_add_goal_args<'a>(project: Option<&str>, goal_file: Option<&'a str>, summary: Option<&'a str>) -> Result<Option<(&'a str, &'a str)>, String> {
    if project.is_none() {
        return Ok(None);
    }
    match (goal_file, summary) {
        (Some(f), Some(s)) => Ok(Some((f, s))),
        _ => Err("task-add --project needs --goal-file F and --summary S (goal is recorded with the row)".into()),
    }
}

/// Columns a caller must know before the next step; the write commands print them so no `grist task` re-read is needed.
const ROW_STATE_KEYS: [&str; 4] = ["H", "I", "J", "K"];

/// `$H`·`$I`·`$J`·`$K` of a Table26 row's fields as one line.
fn row_state(fields: &Value) -> String {
    ROW_STATE_KEYS
        .iter()
        .map(|k| {
            let v = fields.get(*k).filter(|v| !v.is_null()).map(util::val_str).unwrap_or_default();
            format!("{k}={}", v.replace('\n', " / "))
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Adds the `row` state line to a returned Table26 record.
fn cache_project(project: Option<&str>) -> PathBuf {
    util::resolve(Path::new(project.unwrap_or(".")))
}

fn row_cache_path(project: &Path, row_id: i64) -> PathBuf {
    project.join(".ai/gate").join(format!("{row_id}.row"))
}

fn write_row_cache(project: &Path, row_id: i64, line: &str) -> R<()> {
    let path = row_cache_path(project, row_id);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(api::other)?;
    std::fs::write(path, format!("{line}\n")).map_err(api::other)
}

fn cached_row_output(project: &Path, row_id: i64) -> R<Option<String>> {
    match std::fs::read_to_string(row_cache_path(project, row_id)) {
        Ok(line) => Ok(Some(line.trim_end_matches('\n').to_string())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(api::other(err)),
    }
}

fn with_row_state(mut record: Value, project: Option<&str>) -> R<Value> {
    let line = record.get("fields").map(row_state);
    if let (Some(line), Some(obj)) = (line, record.as_object_mut()) {
        if let Some(id) = obj.get("id").and_then(Value::as_i64) {
            write_row_cache(&cache_project(project), id, &line)?;
        }
        obj.insert("row".into(), Value::String(line));
    }
    Ok(record)
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let hint = help_hint(argv.first().map(String::as_str));
    match run(&argv) {
        Ok(value) => {
            println!("{}", util::pretty(&value));
        }
        Err(Exit::Fail(boundary, message)) => {
            eprintln!("{}", api::fail_line(&boundary, &format!("{message}{hint}")));
            std::process::exit(2);
        }
        Err(Exit::Gate(payload)) => {
            let mut payload = payload;
            if let Some(obj) = payload.as_object_mut() {
                let key = if obj.contains_key("reason") { "reason" } else { "error" };
                let prior = obj.get(key).and_then(Value::as_str).unwrap_or("");
                obj.insert(key.into(), Value::String(format!("{prior}{hint}")));
            }
            eprintln!("{}", util::pretty(&payload));
            std::process::exit(2);
        }
        Err(Exit::Other(err)) => {
            eprintln!("{err}{hint}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn help_covers_every_dispatch_command_with_bounded_output() {
        let source = include_str!("main.rs");
        let dispatch = source.split("match args.command.as_str() {").nth(1).unwrap()
            .split("other => usage_error").next().unwrap();
        let names: Vec<&str> = dispatch.lines().filter_map(|line| {
            let line = line.trim();
            line.strip_prefix('"')?.split_once("\" =>").map(|(name, _)| name)
        }).collect();
        assert_eq!(names.len(), super::help::COMMANDS.len());
        for name in names {
            let output = super::help::render(Some(name)).unwrap();
            assert!(output.lines().count() <= 10, "{name}");
            assert!(output.contains("필수:") && output.contains("선택:") && output.contains("예시:"), "{name}");
        }
        assert_eq!(super::help::render(None).unwrap().lines().count(), super::help::COMMANDS.len());
    }

    #[test]
    fn help_uses_validation_formats() {
        let update = super::help::render(Some("task-update")).unwrap();
        assert!(update.contains(&super::records::error_line_format()));
        let confirm = super::help::render(Some("task-confirm")).unwrap();
        assert!(confirm.contains(super::records::EVIDENCE_FORMAT));
        assert!(confirm.contains(super::records::CAN_FORMAT));
        assert!(super::help::render(Some("flow-wait")).unwrap().contains("task-hidden 뒤 최신 task-confirm"));
        assert_eq!(super::help_hint(Some("task-confirm")), " (grist task-confirm --help)");
        assert_eq!(super::help_hint(Some("unknown")), " (grist --help)");
    }

    #[test]
    fn draft_and_diagnosis_outputs_link_start_card() {
        let card = super::task::STAGE_START_CARD;
        assert_eq!(card, "~/ai/skills/manage-project/references/stage-start.md");
        let mut add = serde_json::json!({"id": 1, "H": "초안"});
        add.as_object_mut().unwrap().insert("next".into(), card.into());
        assert_eq!(add["next"], card);
        let mut update = serde_json::json!({"id": 1, "H": "진단"});
        update.as_object_mut().unwrap().insert("next".into(), card.into());
        assert_eq!(update["next"], card);
    }
    #[test]
    fn row_state_prints_stage_error_cause_goal_line() {
        let rec = serde_json::json!({"id": 1, "fields": {"H": "진단", "I": "1. 오류", "J": "1. [⭕️] a\n2. [🛣] b", "K": "목표", "B": "x"}});
        let line = super::row_state(&rec["fields"]);
        println!("{line}");
        assert_eq!(line, "H=진단 | I=1. 오류 | J=1. [⭕️] a / 2. [🛣] b | K=목표");
    }

    #[test]
    fn row_cache_writes_and_reads_exact_output_line() {
        let project = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test/tmp").join(format!("row-cache-{}", std::process::id()));
        let line = "H=작업 | I= | J= | K=행 읽기 캐시";
        super::write_row_cache(&project, 489, line).unwrap();
        let path = super::row_cache_path(&project, 489);
        assert_eq!(path, project.join(".ai/gate/489.row"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), format!("{line}\n"));
        assert_eq!(super::cached_row_output(&project, 489).unwrap(), Some(line.to_string()));
        assert_eq!(super::cached_row_output(&project, 490).unwrap(), None);
        std::fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn task_cached_flag_needs_no_value() {
        let args = ["task", "489", "--cached"].map(str::to_string);
        let parsed = super::parse_cli(&args).unwrap();
        assert!(parsed.flags.contains_key("cached"));
        assert_eq!(parsed.pos, vec!["489"]);
    }

    #[test]
    fn public_update_commands_cannot_write_confirmation_stage() {
        let fields = "{\"H\":\"확인\"}";
        for args in [
            vec!["task-update", "276", "--fields", fields],
            vec!["update", "Table26", "--id", "276", "--fields", fields],
        ] {
            let result = super::run(&args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>());
            assert!(matches!(result, Err(super::Exit::Fail(_, message)) if message.contains("task check chain")));
        }
    }

    #[test]
    fn task_add_requires_goal_args_with_project() {
        assert!(super::task_add_goal_args(Some("p"), None, Some("s")).is_err());
        assert!(super::task_add_goal_args(Some("p"), Some("f"), None).is_err());
        assert_eq!(super::task_add_goal_args(Some("p"), Some("f"), Some("s")), Ok(Some(("f", "s"))));
        assert_eq!(super::task_add_goal_args(None, None, None), Ok(None));
    }
}
