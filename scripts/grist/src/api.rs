//! Grist REST access. The key comes from the access-grist skill `.env` (or GRIST_API_KEY) and never
//! appears in any output.

use crate::util;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::time::Duration;

pub const PROJECT_TABLE: &str = "Table7";
pub const TASK_TABLE: &str = "Table26";
pub const TASK_STAGES: [&str; 7] = ["초안", "진단", "작업", "교정", "검증", "확인", "완료"];
pub const SUBMITTED_STAGE: &str = "검증";
pub const DONE_STAGE: &str = "완료";
pub const GOAL_FIELD: &str = "K";

/// A stop of the command: `_fail` (boundary + message, exit 2), `_gate_fail` (JSON payload, exit 2)
/// or an unexpected error (exit 1).
#[derive(Debug)]
pub enum Exit {
    Fail(String, String),
    Gate(Value),
    Other(String),
}

pub type R<T> = Result<T, Exit>;

pub fn fail<T>(boundary: &str, message: impl Into<String>) -> R<T> {
    Err(Exit::Fail(boundary.to_string(), message.into()))
}

pub fn gate_fail<T>(payload: Value) -> R<T> {
    Err(Exit::Gate(payload))
}

pub fn other<E: std::fmt::Display>(error: E) -> Exit {
    Exit::Other(error.to_string())
}

/// The stderr line `_fail` prints.
pub fn fail_line(boundary: &str, message: &str) -> String {
    util::compact(&json!({"ok": false, "boundary": boundary, "error": message}))
}

/// The access-grist skill folder (`SKILL_DIR` of the old script).
pub fn skill_dir() -> PathBuf {
    let codex = util::home().join(".codex").join("skills").join("access-grist");
    if codex.join(".env").is_file() {
        return codex;
    }
    let ai = util::home().join("ai").join("skills").join("access-grist");
    if ai.join(".env").is_file() {
        return ai;
    }
    codex
}

fn load_env() -> R<Map<String, Value>> {
    let mut values = Map::new();
    let env_file = std::env::var_os("GRIST_ENV_FILE").map(PathBuf::from).unwrap_or_else(|| skill_dir().join(".env"));
    match std::fs::read(&env_file) {
        Ok(bytes) => {
            let text = String::from_utf8(bytes).map_err(other)?;
            for line in util::splitlines(&text) {
                let line = util::strip(&line);
                if !line.is_empty() && !line.starts_with('#') && line.contains('=') {
                    let (key, value) = line.split_once('=').unwrap();
                    let value = util::strip(value).trim_matches('"').trim_matches('\'');
                    values.insert(util::strip(key).to_string(), Value::String(value.to_string()));
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(other(e)),
    }
    for key in ["GRIST_API_KEY", "GRIST_BASE_URL", "GRIST_DOC_ID"] {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                values.insert(key.to_string(), Value::String(value));
            }
        }
    }
    Ok(values)
}

fn config() -> R<(String, String, String)> {
    let env = load_env()?;
    let get = |k: &str| env.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let key = get("GRIST_API_KEY");
    if key.is_empty() {
        return fail("config", format!("GRIST_API_KEY is not set (expected in {})", util::path_str(&skill_dir().join(".env"))));
    }
    let base = env.get("GRIST_BASE_URL").and_then(|v| v.as_str()).unwrap_or("http://localhost:8484").trim_end_matches('/').to_string();
    let doc = env.get("GRIST_DOC_ID").and_then(|v| v.as_str()).unwrap_or("au1EbKLtZbrW").to_string();
    Ok((key, base, doc))
}

fn transport_reason(error: &ureq::Transport) -> String {
    let mut source: Option<&dyn std::error::Error> = std::error::Error::source(error);
    while let Some(current) = source {
        if let Some(io) = current.downcast_ref::<std::io::Error>() {
            return match io.kind() {
                std::io::ErrorKind::ConnectionRefused => "[Errno 111] Connection refused".to_string(),
                _ => io.to_string(),
            };
        }
        source = current.source();
    }
    if error.kind() == ureq::ErrorKind::ConnectionFailed {
        return "[Errno 111] Connection refused".to_string();
    }
    error.to_string()
}

fn http_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build())
}

pub fn request(method: &str, path: &str, body: Option<&Value>) -> R<Value> {
    let (key, base, doc) = config()?;
    let connect_base = if let Some(rest) = base.strip_prefix("http://localhost:") {
        format!("http://127.0.0.1:{rest}")
    } else if base == "http://localhost" {
        "http://127.0.0.1".to_string()
    } else {
        base.clone()
    };
    let url = format!("{connect_base}/api/docs/{doc}{path}");
    let call = http_agent()
        .request(method, &url)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json");
    let response = match body {
        Some(body) => call.send_string(&util::dumps(body, None, true)),
        None => call.call(),
    };
    let raw = match response {
        Ok(resp) => resp.into_string().map_err(other)?,
        Err(ureq::Error::Status(code, resp)) => {
            let bytes = {
                let mut buf = Vec::new();
                let _ = std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf);
                buf
            };
            let detail = util::head(&String::from_utf8_lossy(&bytes), 300);
            return fail(&format!("http_{}", method.to_lowercase()), format!("HTTP {code}: {detail}"));
        }
        Err(ureq::Error::Transport(t)) => {
            return fail("connection", format!("cannot reach Grist at {base}: {}", transport_reason(&t)));
        }
    };
    if raw.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&raw).map_err(other)
}

pub fn parse_json(text: &str, name: &str) -> R<Value> {
    serde_json::from_str(text).or_else(|e| fail("arguments", format!("{name} is not valid JSON: {e}")))
}

pub fn validate_task_fields(fields: &Map<String, Value>) -> R<()> {
    if let Some(stage) = fields.get("H") {
        if !stage.is_null() && !stage.as_str().map(|s| TASK_STAGES.contains(&s)).unwrap_or(false) {
            return fail("validation", format!("Table26 $H must be one of {}", TASK_STAGES.join(", ")));
        }
        if stage.as_str() == Some(DONE_STAGE) {
            return fail("validation", "Table26 $H=완료 is reserved for the user in Grist");
        }
        if stage.as_str() == Some("확인") {
            return fail("validation", "Table26 $H=확인 is written only by the task check chain (task-complete)");
        }
        if stage.as_str() == Some(SUBMITTED_STAGE) {
            return fail("validation", "Table26 $H=검증 is written only by `task-submit RAW_ID --project DIR`");
        }
    }
    if fields.contains_key(GOAL_FIELD) {
        return fail("validation", "Table26 $K (목표) is written only by `task-add --goal-file F --summary S` or `task-goal RAW_ID --project DIR --goal-file F`");
    }
    Ok(())
}

fn index<'a>(value: &'a Value, key: &str) -> R<&'a Value> {
    value.get(key).ok_or_else(|| Exit::Other(format!("KeyError: '{key}'")))
}

fn list(value: &Value) -> R<&Vec<Value>> {
    value.as_array().ok_or_else(|| Exit::Other("unexpected Grist response".into()))
}

pub fn tables() -> R<Value> {
    let result = request("GET", "/tables", None)?;
    let ids = list(index(&result, "tables")?)?.iter().map(|t| index(t, "id").cloned()).collect::<R<Vec<_>>>()?;
    Ok(Value::Array(ids))
}

pub fn columns(table: &str) -> R<Value> {
    let result = request("GET", &format!("/tables/{table}/columns"), None)?;
    let mut out = Vec::new();
    for column in list(index(&result, "columns")?)? {
        let fields = index(column, "fields")?;
        let mut entry = Map::new();
        entry.insert("id".into(), index(column, "id")?.clone());
        entry.insert("label".into(), fields.get("label").cloned().unwrap_or(Value::Null));
        entry.insert("type".into(), fields.get("type").cloned().unwrap_or(Value::Null));
        let raw = fields.get("widgetOptions").filter(|v| util::truthy(Some(v))).and_then(|v| v.as_str()).unwrap_or("{}");
        let options: Value = serde_json::from_str(raw).unwrap_or(json!({}));
        for key in ["choices", "dateFormat"] {
            if util::truthy(options.get(key)) {
                entry.insert(key.into(), options[key].clone());
            }
        }
        out.push(Value::Object(entry));
    }
    Ok(Value::Array(out))
}

/// `urllib.parse.quote(text)`: everything but unreserved characters and `/` percent-encoded.
pub fn quote(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"_.-~/".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

pub fn get(table: &str, ids: Option<&[i64]>, filter: Option<&Map<String, Value>>, fields: Option<&[String]>) -> R<Vec<Value>> {
    let mut query = filter.cloned().unwrap_or_default();
    if let Some(ids) = ids {
        if !ids.is_empty() {
            query.insert("id".into(), json!(ids));
        }
    }
    let mut path = format!("/tables/{table}/records");
    if !query.is_empty() {
        path += &format!("?filter={}", quote(&util::compact(&Value::Object(query))));
    }
    let result = request("GET", &path, None)?;
    let records = list(index(&result, "records")?)?.clone();
    match fields {
        Some(fields) if !fields.is_empty() => records
            .iter()
            .map(|r| {
                let source = index(r, "fields")?;
                let mut picked = Map::new();
                for k in fields {
                    picked.insert(k.clone(), source.get(k).cloned().unwrap_or(Value::Null));
                }
                Ok(json!({"id": index(r, "id")?.clone(), "fields": Value::Object(picked)}))
            })
            .collect(),
        _ => Ok(records),
    }
}

pub fn add(table: &str, fields: &Map<String, Value>) -> R<Value> {
    let result = request("POST", &format!("/tables/{table}/records"), Some(&json!({"records": [{"fields": fields}]})))?;
    let row_id = list(index(&result, "records")?)?
        .first()
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_i64())
        .ok_or_else(|| Exit::Other("IndexError: list index out of range".into()))?;
    if table == TASK_TABLE {
        crate::records::on_task_write(row_id, fields);
    }
    Ok(json!({"id": row_id}))
}

pub fn update(table: &str, row_id: i64, fields: &Map<String, Value>) -> R<Value> {
    request("PATCH", &format!("/tables/{table}/records"), Some(&json!({"records": [{"id": row_id, "fields": fields}]})))?;
    if table == TASK_TABLE {
        crate::records::on_task_write(row_id, fields);
    }
    get(table, Some(&[row_id]), None, None)?
        .into_iter()
        .next()
        .ok_or_else(|| Exit::Other("IndexError: list index out of range".into()))
}

pub fn delete(table: &str, row_id: i64) -> R<Value> {
    request("POST", &format!("/tables/{table}/data/delete"), Some(&json!([row_id])))?;
    let gone = get(table, Some(&[row_id]), None, None)?.is_empty();
    Ok(json!({"id": row_id, "deleted": gone}))
}

pub fn column_update(table: &str, column: &str, fields: &Map<String, Value>) -> R<Value> {
    request("PATCH", &format!("/tables/{table}/columns"), Some(&json!({"columns": [{"id": column, "fields": fields}]})))?;
    let result = request("GET", &format!("/tables/{table}/columns"), None)?;
    let found: Vec<&Value> = list(index(&result, "columns")?)?.iter().filter(|c| c.get("id").and_then(|v| v.as_str()) == Some(column)).collect();
    let Some(found) = found.first() else {
        return fail("validation", format!("{table} has no column {column}"));
    };
    let mut picked = Map::new();
    for k in fields.keys() {
        picked.insert(k.clone(), found.get("fields").and_then(|f| f.get(k)).cloned().unwrap_or(Value::Null));
    }
    Ok(json!({"id": column, "fields": Value::Object(picked)}))
}

/// `get(...)` turned into `{id: fields}` in server order.
pub fn rows_by_id(records: &[Value]) -> Vec<(i64, Map<String, Value>)> {
    records
        .iter()
        .filter_map(|r| Some((r.get("id")?.as_i64()?, r.get("fields")?.as_object()?.clone())))
        .collect()
}
