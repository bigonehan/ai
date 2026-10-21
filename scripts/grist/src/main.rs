pub mod api;
pub mod gate;
pub mod records;
pub mod util;

use api::{Exit, R, PROJECT_TABLE, TASK_TABLE};
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
            api::update(TASK_TABLE, row_id, &fields)
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
            records::task_add(fields, project, kind, judge)
        }
        "gate" => {
            let default_cwd = cwd_str();
            let cwd = args.flag_opt("cwd").unwrap_or(&default_cwd);
            let row = args.flag_i64_opt("row")?;
            gate::gate(cwd, row)
        }
        "task-goal" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let goal_file = args.flag_req("goal-file")?;
            let summary = args.flag_req("summary")?;
            gate::task_goal(row_id, project, goal_file, summary)
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
        "task-complete" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            gate::task_complete(row_id, project)
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
            records::task_step(project, row_id, step, text)
        }
        "task-confirm" => {
            let row_id = args.pos_i64(0, "row_id")?;
            let project = args.flag_req("project")?;
            let summary = args.flag_req("summary")?;
            let check = args.flag_req("check")?;
            let detail = args.flag_req("detail")?;
            records::task_confirm(project, row_id, summary, check, detail)
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
