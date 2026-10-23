use grist::api::{self, Exit, R, PROJECT_TABLE, TASK_TABLE};
use grist::{gate, records, util};
use serde_json::{Map, Value};
use std::collections::HashMap;

struct ParsedArgs {
    command: String,
    pos: Vec<String>,
    flags: HashMap<String, Vec<String>>,
}

fn usage_error<T>(msg: &str) -> R<T> {
    eprintln!("error: {msg}");
    std::process::exit(2);
}

fn parse_cli(argv: &[String]) -> R<ParsedArgs> {
    if argv.is_empty() || argv.iter().any(|a| a == "-h" || a == "--help") {
        if argv.iter().any(|a| a == "-h" || a == "--help") {
            println!("usage: grist <command> [options]");
            std::process::exit(0);
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
            let recs = api::get(TASK_TABLE, Some(&[row_id]), None, None)?;
            Ok(Value::Array(recs))
        }
        "task-update" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let fields = parse_fields_obj(args.flag_req("fields")?, "--fields")?;
            api::validate_task_fields(&fields)?;
            records::check_symbol_fields(&fields)?;
            records::check_diagnosis_fields(&fields)?;
            let mut record = api::update(TASK_TABLE, row_id, &fields)?;
            if fields.get("H").and_then(|v| v.as_str()) == Some(gate::ERROR_STAGE) {
                if let Some(project_dir) = records::hook_project(row_id) {
                    if let Some(rewritten) = records::write_goal_stage(&project_dir, row_id, gate::ERROR_STAGE)? {
                        record = rewritten;
                    }
                }
            }
            Ok(with_row_state(record))
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
                        obj.insert("row".into(), Value::String(row_state(f)));
                    }
                }
            }
            records::attach_warnings(&mut added, warnings);
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
            Ok(with_row_state(gate::task_goal(row_id, project, goal_file, summary)?))
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
            let evidence_file = args.flag_req("evidence")?;
            let evidence = match std::fs::read_to_string(evidence_file) {
                Ok(text) => text,
                Err(e) => return api::fail("validation", format!("--evidence {evidence_file}: {e}")),
            };
            records::task_confirm(project, row_id, summary, check, detail, &evidence)
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
fn with_row_state(mut record: Value) -> Value {
    let line = record.get("fields").map(row_state);
    if let (Some(line), Some(obj)) = (line, record.as_object_mut()) {
        obj.insert("row".into(), Value::String(line));
    }
    record
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match run(&argv) {
        Ok(value) => {
            println!("{}", util::pretty(&value));
        }
        Err(Exit::Fail(boundary, message)) => {
            eprintln!("{}", api::fail_line(&boundary, &message));
            std::process::exit(2);
        }
        Err(Exit::Gate(payload)) => {
            eprintln!("{}", util::pretty(&payload));
            std::process::exit(2);
        }
        Err(Exit::Other(err)) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn row_state_prints_stage_error_cause_goal_line() {
        let rec = serde_json::json!({"id": 1, "fields": {"H": "진단", "I": "1. 오류", "J": "1. [⭕️] a\n2. [🛣] b", "K": "목표", "B": "x"}});
        let line = super::with_row_state(rec)["row"].as_str().unwrap().to_string();
        println!("{line}");
        assert_eq!(line, "H=진단 | I=1. 오류 | J=1. [⭕️] a / 2. [🛣] b | K=목표");
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
