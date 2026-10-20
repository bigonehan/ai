use regex::Regex;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_API_URL: &str = "https://api.typesafe.ai/v1/systemone";
const STATE_LIMIT: usize = 12000;
const TIMEOUT_SECS: u64 = 20;
const MATCH_TYPES: [&str; 3] = ["연결", "기능", "수정"];

const HELP_TOP: &str = "\
usage: jev [-h] {scope,match,stop} ...

TypeSafe Jev judgments for the Table26 gates, without handling the API key.

Called by access-grist `grist` and the Codex completion hook, not by Codex
directly. The key is read from this skill's `.env` (or the environment) and is
never printed. All output is JSON on stdout.

Usage:
  jev scope --request-file F --goal-file G [--row N]   # J3: goal lines grounded in the request, request covered
  jev match --goal-file G --output-file O [--row N]    # J1: actual output satisfies each [연결]/[기능]/[수정] line
  jev stop --message-file M                            # J2: classify the final assistant message

Verdicts: a Noul value (or Choice confidence) >= JEV_PASS is `pass`, < JEV_FLOOR
is `fail`, anything between is `uncertain`. JEV_MODE=shadow (default) tells the
caller to log only; `enforce` lets it reject on `fail`.

positional arguments:
  {scope,match,stop}

options:
  -h, --help          show this help message and exit
";

const USAGE_TOP_SHORT: &str = "usage: jev [-h] {scope,match,stop} ...";

const USAGE_SCOPE: &str = "\
usage: jev scope [-h] --request-file REQUEST_FILE --goal-file GOAL_FILE
                 [--row ROW]";

const HELP_SCOPE: &str = "\
usage: jev scope [-h] --request-file REQUEST_FILE --goal-file GOAL_FILE
                 [--row ROW]

options:
  -h, --help            show this help message and exit
  --request-file REQUEST_FILE
  --goal-file GOAL_FILE
  --row ROW
";

const USAGE_MATCH: &str = "\
usage: jev match [-h] --goal-file GOAL_FILE --output-file OUTPUT_FILE
                 [--row ROW]";

const HELP_MATCH: &str = "\
usage: jev match [-h] --goal-file GOAL_FILE --output-file OUTPUT_FILE
                 [--row ROW]

options:
  -h, --help            show this help message and exit
  --goal-file GOAL_FILE
  --output-file OUTPUT_FILE
  --row ROW
";

const USAGE_STOP: &str = "usage: jev stop [-h] --message-file MESSAGE_FILE";

const HELP_STOP: &str = "\
usage: jev stop [-h] --message-file MESSAGE_FILE

options:
  -h, --help            show this help message and exit
  --message-file MESSAGE_FILE
";

struct Config {
    api_key: String,
    model: String,
    mode: String,
    pass: f64,
    floor: f64,
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn skill_dir() -> PathBuf {
    home_dir().join(".codex/skills/access-jev")
}

fn log_file() -> PathBuf {
    home_dir().join(".codex/tmp/jev/judgments.jsonl")
}

fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn py_strip(s: &str) -> &str {
    s.trim_matches(is_py_space)
}

fn splitlines(s: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                lines.push(std::mem::take(&mut current));
            }
            '\n' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}' => {
                lines.push(std::mem::take(&mut current));
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn head_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn tail_chars(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        s.to_string()
    } else {
        s.chars().skip(count - n).collect()
    }
}

fn parse_py_float(raw: &str) -> f64 {
    match py_strip(raw).parse::<f64>() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("ValueError: could not convert string to float: '{raw}'");
            std::process::exit(1);
        }
    }
}

fn load_config() -> Config {
    let mut values: Map<String, Value> = Map::new();
    let env_path = std::env::var_os("JEV_ENV_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| skill_dir().join(".env"));
    match fs::read(&env_path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            for line in splitlines(&normalized) {
                let trimmed = py_strip(&line);
                if !trimmed.is_empty() && !trimmed.starts_with('#') && trimmed.contains('=') {
                    if let Some((k, v)) = trimmed.split_once('=') {
                        let v_clean = py_strip(v).trim_matches('"').trim_matches('\'');
                        values.insert(py_strip(k).to_string(), Value::String(v_clean.to_string()));
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
    for key in ["TYPESAFE_API_KEY", "JEV_MODEL", "JEV_MODE", "JEV_PASS", "JEV_FLOOR"] {
        if let Ok(v) = std::env::var(key) {
            if !v.is_empty() {
                values.insert(key.to_string(), Value::String(v));
            }
        }
    }
    let get_or = |k: &str, def: &str| -> String {
        values
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or(def)
            .to_string()
    };
    let mode = get_or("JEV_MODE", "shadow");
    let pass = parse_py_float(&get_or("JEV_PASS", "0.9"));
    let floor = parse_py_float(&get_or("JEV_FLOOR", "0.5"));
    let api_key = get_or("TYPESAFE_API_KEY", "");
    let model = get_or("JEV_MODEL", "jev-latest");
    Config {
        api_key,
        model,
        mode,
        pass,
        floor,
    }
}

fn dumps(value: &Value, indent: Option<usize>) -> String {
    let mut out = String::new();
    write_json(&mut out, value, indent, 0);
    out
}

fn newline_indent(out: &mut String, indent: Option<usize>, level: usize) {
    if let Some(step) = indent {
        out.push('\n');
        for _ in 0..step * level {
            out.push(' ');
        }
    }
}

fn write_json(out: &mut String, value: &Value, indent: Option<usize>, level: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if n.is_f64() {
                out.push_str(&float_repr(n.as_f64().unwrap_or(0.0)));
            } else {
                out.push_str(&n.to_string());
            }
        }
        Value::String(s) => write_json_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                    if indent.is_none() {
                        out.push(' ');
                    }
                }
                newline_indent(out, indent, level + 1);
                write_json(out, item, indent, level + 1);
            }
            newline_indent(out, indent, level);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                    if indent.is_none() {
                        out.push(' ');
                    }
                }
                newline_indent(out, indent, level + 1);
                write_json_string(out, k);
                out.push_str(": ");
                write_json(out, v, indent, level + 1);
            }
            newline_indent(out, indent, level);
            out.push('}');
        }
    }
}

fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn float_repr(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let sci = format!("{:e}", v);
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    let sign = if negative { "-" } else { "" };
    if (-4..16).contains(&exp) {
        let point = exp + 1;
        let text = if point <= 0 {
            format!("0.{}{}", "0".repeat((-point) as usize), digits)
        } else if point as usize >= digits.len() {
            format!("{}{}.0", digits, "0".repeat(point as usize - digits.len()))
        } else {
            format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
        };
        format!("{sign}{text}")
    } else {
        let rest = if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        let esign = if exp < 0 { "-" } else { "+" };
        format!("{sign}{}{rest}e{esign}{:02}", &digits[..1], exp.abs())
    }
}

fn fail(cfg: &Config, boundary: &str, message: &str) -> ! {
    let mut obj = Map::new();
    obj.insert("ok".into(), Value::Bool(false));
    obj.insert("boundary".into(), Value::String(boundary.into()));
    obj.insert("error".into(), Value::String(message.into()));
    obj.insert("mode".into(), Value::String(cfg.mode.clone()));
    eprintln!("{}", dumps(&Value::Object(obj), None));
    std::process::exit(2);
}

fn clean(text: &str) -> String {
    let secret_re = Regex::new(r"(?i)(Bearer\s+\S+|\b[A-Z0-9_]*(?:KEY|TOKEN|SECRET)[A-Z0-9_]*\s*=\s*\S+)").unwrap();
    let masked = secret_re.replace_all(text, "[masked]").into_owned();
    let home = home_dir().to_string_lossy().into_owned();
    let replaced = masked.replace(&home, "~");
    tail_chars(&replaced, STATE_LIMIT)
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

fn call_api(cfg: &Config, state: &str, questions: Value) -> Value {
    if let Ok(fake) = std::env::var("JEV_FAKE_RESPONSE") {
        if !fake.is_empty() {
            let text = match fs::read_to_string(&fake) {
                Ok(t) => t.replace("\r\n", "\n").replace('\r', "\n"),
                Err(e) => {
                    eprintln!("FileNotFoundError: {fake}: {e}");
                    std::process::exit(1);
                }
            };
            match serde_json::from_str::<Value>(&text) {
                Ok(v) => return v,
                Err(e) => {
                    eprintln!("JSONDecodeError: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
    if cfg.api_key.is_empty() {
        let expected_env = skill_dir().join(".env");
        fail(
            cfg,
            "config",
            &format!("TYPESAFE_API_KEY is not set (expected in {})", expected_env.to_string_lossy()),
        );
    }
    let mut body = Map::new();
    body.insert("model".into(), Value::String(cfg.model.clone()));
    body.insert("state".into(), Value::String(state.into()));
    body.insert("questions".into(), questions);
    let payload = dumps(&Value::Object(body), None);

    let api_url = std::env::var("JEV_API_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_API_URL.to_string());

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .build();
    let resp = agent
        .post(&api_url)
        .set("Authorization", &format!("Bearer {}", cfg.api_key))
        .set("Content-Type", "application/json")
        .send_bytes(payload.as_bytes());

    let raw = match resp {
        Ok(r) => {
            let mut bytes = Vec::new();
            if let Err(e) = std::io::Read::read_to_end(&mut r.into_reader(), &mut bytes) {
                fail(cfg, "connection", &head_chars(&e.to_string(), 300));
            }
            match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("UnicodeDecodeError: {e}");
                    std::process::exit(1);
                }
            }
        }
        Err(ureq::Error::Status(code, r)) => {
            let mut bytes = Vec::new();
            let _ = std::io::Read::read_to_end(&mut r.into_reader(), &mut bytes);
            let detail = head_chars(&String::from_utf8_lossy(&bytes), 300);
            fail(cfg, &format!("http_{code}"), &detail);
        }
        Err(ureq::Error::Transport(t)) => {
            let reason = head_chars(&transport_reason(&t), 300);
            fail(cfg, "connection", &reason);
        }
    };

    match serde_json::from_str::<Value>(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("JSONDecodeError: {e}");
            std::process::exit(1);
        }
    }
}

fn grade(cfg: &Config, value: &Value) -> String {
    let num = match value {
        Value::Null => return "uncertain".into(),
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => {
            eprintln!("TypeError: '>=' not supported between instances of value and 'float'");
            std::process::exit(1);
        }
    };
    if num >= cfg.pass {
        "pass".into()
    } else if num < cfg.floor {
        "fail".into()
    } else {
        "uncertain".into()
    }
}

fn overall(items: &[Value]) -> String {
    let mut has_fail = false;
    let mut has_uncertain = false;
    for item in items {
        if let Some(v) = item.get("verdict").and_then(|v| v.as_str()) {
            if v == "fail" {
                has_fail = true;
            } else if v == "uncertain" {
                has_uncertain = true;
            }
        }
    }
    if has_fail {
        "fail".into()
    } else if has_uncertain {
        "uncertain".into()
    } else {
        "pass".into()
    }
}

fn as_dict<'a>(val: &'a Value, empty: &'a Map<String, Value>) -> &'a Map<String, Value> {
    match val {
        Value::Object(m) => m,
        _ => {
            let _ = empty;
            eprintln!("AttributeError: object has no attribute 'get'");
            std::process::exit(1);
        }
    }
}

fn noul_items(cfg: &Config, response: &Value, names: &[(String, String)]) -> Vec<Value> {
    let empty = Map::new();
    let resp_obj = as_dict(response, &empty);
    let answers = match resp_obj.get("answers") {
        Some(v) => as_dict(v, &empty),
        None => &empty,
    };
    let mut items = Vec::new();
    for (name, label) in names {
        let entry = match answers.get(name) {
            Some(v) => as_dict(v, &empty),
            None => &empty,
        };
        let value = entry.get("noul").cloned().unwrap_or(Value::Null);
        let verdict = grade(cfg, &value);
        let mut item = Map::new();
        item.insert("name".into(), Value::String(name.clone()));
        item.insert("line".into(), Value::String(label.clone()));
        item.insert("value".into(), value);
        item.insert("verdict".into(), Value::String(verdict));
        items.push(Value::Object(item));
    }
    items
}

fn goal_lines(goal: &str, filter_types: Option<&[&str]>) -> Vec<(String, String)> {
    let re = Regex::new(r"^\s*(?:-\s*|\d+\.\s*)\[(연결|기능|수정)\]\s*(.+?)\s*$").unwrap();
    let mut out = Vec::new();
    for line in splitlines(goal) {
        if let Some(caps) = re.captures(&line) {
            let kind = caps.get(1).unwrap().as_str();
            let text = py_strip(caps.get(2).unwrap().as_str());
            if filter_types.map(|t| t.contains(&kind)).unwrap_or(true) {
                out.push((kind.to_string(), text.to_string()));
            }
        }
    }
    out
}

fn run_scope(cfg: &Config, request: &str, goal: &str) -> (String, Vec<Value>, Option<String>, Value) {
    let lines = goal_lines(goal, None);
    let mut names: Vec<(String, String)> = lines
        .into_iter()
        .enumerate()
        .map(|(i, (kind, text))| (format!("g{}", i + 1), format!("[{kind}] {text}")))
        .collect();

    let mut questions = Map::new();
    for (name, label) in &names {
        let mut q = Map::new();
        q.insert("type".into(), Value::String("noul".into()));
        q.insert(
            "instructions".into(),
            Value::String(format!(
                "이 목표 조건은 위 사용자 요청에 명시되었거나 요청에서 직접 따라 나온다: {label}"
            )),
        );
        questions.insert(name.clone(), Value::Object(q));
    }
    let mut cov = Map::new();
    cov.insert("type".into(), Value::String("noul".into()));
    cov.insert(
        "instructions".into(),
        Value::String(format!(
            "사용자 요청의 모든 요구가 아래 목표 조건들에 빠짐없이 반영되어 있다:\n{goal}"
        )),
    );
    questions.insert("coverage".into(), Value::Object(cov));

    let state = clean(&format!("# 사용자 요청\n{request}\n\n# 목표\n{goal}"));
    let response = call_api(cfg, &state, Value::Object(questions));
    names.push(("coverage".into(), "요청 전체 반영".into()));
    let items = noul_items(cfg, &response, &names);
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    (state, items, None, usage)
}

fn run_match(cfg: &Config, goal: &str, output: &str) -> (String, Vec<Value>, Option<String>, Value) {
    let lines = goal_lines(goal, Some(&MATCH_TYPES));
    let names: Vec<(String, String)> = lines
        .into_iter()
        .enumerate()
        .map(|(i, (kind, text))| (format!("g{}", i + 1), format!("[{kind}] {text}")))
        .collect();
    if names.is_empty() {
        return (String::new(), Vec::new(), None, Value::Null);
    }
    let mut questions = Map::new();
    for (name, label) in &names {
        let mut q = Map::new();
        q.insert("type".into(), Value::String("noul".into()));
        q.insert(
            "instructions".into(),
            Value::String(format!("위 실제 출력은 이 목표 조건을 충족한다: {label}")),
        );
        questions.insert(name.clone(), Value::Object(q));
    }
    let state = clean(&format!("# 실제 출력\n{output}"));
    let response = call_api(cfg, &state, Value::Object(questions));
    let items = noul_items(cfg, &response, &names);
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    (state, items, None, usage)
}

fn run_stop(cfg: &Config, message: &str) -> (String, Vec<Value>, Option<String>, Value) {
    let mut criteria = Map::new();
    criteria.insert(
        "완료 보고".into(),
        Value::String("The work is finished and the message reports the verified result.".into()),
    );
    criteria.insert(
        "사용자 질문".into(),
        Value::String("The message asks the user a question or asks the user to perform an action.".into()),
    );
    criteria.insert(
        "실패를 결론으로 종료".into(),
        Value::String(
            "The message stops because a tool, browser, CDP or runtime step failed and presents that failure as the outcome."
                .into(),
        ),
    );
    criteria.insert(
        "진행 보고".into(),
        Value::String(
            "The message reports progress and says or implies the agent's own work is still continuing (next steps it will do itself), without a finished result or a user-only blocker."
                .into(),
        ),
    );
    criteria.insert(
        "질문 답변".into(),
        Value::String(
            "The message answers the user's latest question or request for explanation, advice or information; it does not report task work as finished or continuing."
                .into(),
        ),
    );
    criteria.insert(
        "대기 보고".into(),
        Value::String(
            "The task is waiting on a user-only action, user-provided data or a user decision, and the message states exactly what the user must do or provide next."
                .into(),
        ),
    );

    let mut cat_q = Map::new();
    cat_q.insert("type".into(), Value::String("choice".into()));
    cat_q.insert(
        "instructions".into(),
        Value::String("이 AI 작업 에이전트의 마지막 메시지는 어떤 종류인가?".into()),
    );
    cat_q.insert("criteria".into(), Value::Object(criteria));

    let mut act_q = Map::new();
    act_q.insert("type".into(), Value::String("noul".into()));
    act_q.insert(
        "instructions".into(),
        Value::String(
            "메시지가 사용자에게 요청하는 것은 다음 셋 중 하나다: \
(1) 로그인·인증, 결제·비용, 되돌릴 수 없는 외부 전송, 브라우저 밖 OS 앱 조작 중 하나의 동작; \
(2) 대상 .py 파일 경로와 수정 이유를 밝힌 .py 생성·수정 동의; \
(3) 수정을 시작하기 전에 구체적인 해석 둘 이상과 각 해석의 결과 차이를 제시한 선택 요청. \
'진행할까요', '확인해 주세요', 결과 검증·UI 확인 요청처럼 이 형식을 갖추지 않은 질문은 해당하지 않는다"
                .into(),
        ),
    );

    let mut questions = Map::new();
    questions.insert("category".into(), Value::Object(cat_q));
    questions.insert("allowed_action".into(), Value::Object(act_q));

    let state = clean(&format!("# 마지막 메시지\n{message}"));
    let response = call_api(cfg, &state, Value::Object(questions));

    let empty = Map::new();
    let resp_obj = as_dict(&response, &empty);
    let answers = match resp_obj.get("answers") {
        Some(v) => as_dict(v, &empty),
        None => &empty,
    };
    let answer = match answers.get("category") {
        Some(v) => as_dict(v, &empty),
        None => &empty,
    };
    let choice = answer.get("choice").cloned().unwrap_or(Value::Null);
    let confidence = answer.get("confidence").cloned().unwrap_or(Value::Null);
    let verdict = grade(cfg, &confidence);

    let mut cat_item = Map::new();
    cat_item.insert("name".into(), Value::String("category".into()));
    cat_item.insert("value".into(), choice.clone());
    cat_item.insert("confidence".into(), confidence);
    cat_item.insert("verdict".into(), Value::String(verdict));

    let mut items = vec![Value::Object(cat_item)];
    if choice.as_str() == Some("사용자 질문") {
        items.extend(noul_items(
            cfg,
            &response,
            &[("allowed_action".into(), "허용된 사용자 동작".into())],
        ));
    }
    let cat_opt = match &choice {
        Value::String(s) => Some(s.clone()),
        _ => None,
    };
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    (state, items, cat_opt, usage)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn utc_iso_seconds() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let h = rem / 3600;
    let min = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}+00:00")
}

fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn write_log(command: &str, row: Option<i64>, state: &str, mode: &str, verdict: &str, items: &[Value]) {
    let path = log_file();
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let mut entry = Map::new();
    entry.insert("at".into(), Value::String(utc_iso_seconds()));
    entry.insert("command".into(), Value::String(command.into()));
    entry.insert(
        "row".into(),
        row.map(|r| Value::Number(r.into())).unwrap_or(Value::Null),
    );
    entry.insert("state_sha256".into(), Value::String(sha256_hex(state)));
    entry.insert("mode".into(), Value::String(mode.into()));
    entry.insert("verdict".into(), Value::String(verdict.into()));
    entry.insert("items".into(), Value::Array(items.to_vec()));

    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", dumps(&Value::Object(entry), None));
    }
}

fn read_text_file(cfg: &Config, path: &str) -> String {
    match fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => s.replace("\r\n", "\n").replace('\r', "\n"),
            Err(e) => {
                eprintln!("UnicodeDecodeError: {e}");
                std::process::exit(1);
            }
        },
        Err(err) => {
            let mut msg = err.to_string();
            if let Some(code) = err.raw_os_error() {
                let suffix = format!(" (os error {code})");
                if msg.ends_with(&suffix) {
                    msg.truncate(msg.len() - suffix.len());
                }
            }
            fail(cfg, "arguments", &format!("cannot read {path}: {msg}"));
        }
    }
}

enum ParsedCommand {
    Scope {
        request_file: String,
        goal_file: String,
        row: Option<i64>,
    },
    Match {
        goal_file: String,
        output_file: String,
        row: Option<i64>,
    },
    Stop {
        message_file: String,
    },
}

fn match_option<'a>(given: &str, allowed: &[&'a str]) -> Option<&'a str> {
    if !given.starts_with("--") || given.len() < 3 {
        return None;
    }
    let matches: Vec<&'a str> = allowed.iter().copied().filter(|opt| opt.starts_with(given)).collect();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

fn parse_args(argv: &[String]) -> ParsedCommand {
    if argv.is_empty() {
        eprintln!("{USAGE_TOP_SHORT}\njev: error: the following arguments are required: command");
        std::process::exit(2);
    }
    if argv[0] == "-h" || argv[0] == "--help" {
        print!("{HELP_TOP}");
        std::process::exit(0);
    }
    let cmd = argv[0].as_str();
    let (usage_sub, help_sub, allowed, required): (&str, &str, &[&str], &[&str]) = match cmd {
        "scope" => (
            USAGE_SCOPE,
            HELP_SCOPE,
            &["--request-file", "--goal-file", "--row"],
            &["--request-file", "--goal-file"],
        ),
        "match" => (
            USAGE_MATCH,
            HELP_MATCH,
            &["--goal-file", "--output-file", "--row"],
            &["--goal-file", "--output-file"],
        ),
        "stop" => (USAGE_STOP, HELP_STOP, &["--message-file"], &["--message-file"]),
        other => {
            eprintln!(
                "{USAGE_TOP_SHORT}\njev: error: argument command: invalid choice: '{other}' (choose from 'scope', 'match', 'stop')"
            );
            std::process::exit(2);
        }
    };

    for a in &argv[1..] {
        if a == "-h" || a == "--help" {
            print!("{help_sub}");
            std::process::exit(0);
        }
    }

    let mut values: Map<String, Value> = Map::new();
    let mut extras: Vec<String> = Vec::new();
    let mut i = 1;
    while i < argv.len() {
        let arg = &argv[i];
        if let Some((flag, inline_val)) = arg.split_once('=') {
            if let Some(canon) = match_option(flag, allowed) {
                values.insert(canon.to_string(), Value::String(inline_val.to_string()));
                i += 1;
                continue;
            }
        }
        if let Some(canon) = match_option(arg, allowed) {
            if i + 1 >= argv.len() {
                eprintln!("{usage_sub}\njev {cmd}: error: argument {canon}: expected one argument");
                std::process::exit(2);
            }
            values.insert(canon.to_string(), Value::String(argv[i + 1].clone()));
            i += 2;
            continue;
        }
        extras.push(arg.clone());
        i += 1;
    }

    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|req| !values.contains_key(*req))
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "{usage_sub}\njev {cmd}: error: the following arguments are required: {}",
            missing.join(", ")
        );
        std::process::exit(2);
    }

    let row = if let Some(Value::String(r_str)) = values.get("--row") {
        match py_strip(r_str).parse::<i64>() {
            Ok(n) => Some(n),
            Err(_) => {
                eprintln!("{usage_sub}\njev {cmd}: error: argument --row: invalid int value: '{r_str}'");
                std::process::exit(2);
            }
        }
    } else {
        None
    };

    if !extras.is_empty() {
        eprintln!("{USAGE_TOP_SHORT}\njev: error: unrecognized arguments: {}", extras.join(" "));
        std::process::exit(2);
    }

    let get_s = |k: &str| values.get(k).and_then(|v| v.as_str()).unwrap().to_string();
    match cmd {
        "scope" => ParsedCommand::Scope {
            request_file: get_s("--request-file"),
            goal_file: get_s("--goal-file"),
            row,
        },
        "match" => ParsedCommand::Match {
            goal_file: get_s("--goal-file"),
            output_file: get_s("--output-file"),
            row,
        },
        "stop" => ParsedCommand::Stop {
            message_file: get_s("--message-file"),
        },
        _ => unreachable!(),
    }
}

fn main() {
    let cfg = load_config();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let parsed = parse_args(&argv);

    let (cmd_name, row, state, items, category, is_stop, usage) = match parsed {
        ParsedCommand::Scope {
            request_file,
            goal_file,
            row,
        } => {
            let req = read_text_file(&cfg, &request_file);
            let goal = read_text_file(&cfg, &goal_file);
            let (s, it, cat, u) = run_scope(&cfg, &req, &goal);
            ("scope", row, s, it, cat, false, u)
        }
        ParsedCommand::Match {
            goal_file,
            output_file,
            row,
        } => {
            let goal = read_text_file(&cfg, &goal_file);
            let out = read_text_file(&cfg, &output_file);
            let (s, it, cat, u) = run_match(&cfg, &goal, &out);
            ("match", row, s, it, cat, false, u)
        }
        ParsedCommand::Stop { message_file } => {
            let msg = read_text_file(&cfg, &message_file);
            let (s, it, cat, u) = run_stop(&cfg, &msg);
            ("stop", None, s, it, cat, true, u)
        }
    };

    let verdict = if items.is_empty() {
        "pass".to_string()
    } else {
        overall(&items)
    };

    write_log(cmd_name, row, &state, &cfg.mode, &verdict, &items);

    let mut out = Map::new();
    out.insert("ok".into(), Value::Bool(true));
    out.insert("mode".into(), Value::String(cfg.mode));
    out.insert("verdict".into(), Value::String(verdict));
    out.insert("command".into(), Value::String(cmd_name.into()));
    out.insert("items".into(), Value::Array(items));
    if is_stop {
        out.insert(
            "category".into(),
            category.map(Value::String).unwrap_or(Value::Null),
        );
    }
    out.insert("usage".into(), usage);

    println!("{}", dumps(&Value::Object(out), Some(1)));
}

#[cfg(test)]
mod tests {
    use super::{goal_lines, MATCH_TYPES};

    #[test]
    fn parses_current_goal_tags_only() {
        let input = "1. [연결] API 연동 ▷ 200 OK\n- [기능] 새 기능 ▷ True\n2. [수정] 오류 수정 ▷ 정상";
        let parsed = goal_lines(input, Some(&MATCH_TYPES));
        assert_eq!(
            parsed,
            vec![
                ("연결".to_string(), "API 연동 ▷ 200 OK".to_string()),
                ("기능".to_string(), "새 기능 ▷ True".to_string()),
                ("수정".to_string(), "오류 수정 ▷ 정상".to_string()),
            ]
        );
    }
}
