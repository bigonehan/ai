use std::collections::HashSet;
use std::env;
use std::fs;
use std::process::ExitCode;

const HELP_TEXT: &str = "usage: validate-layout-contract [-h] contract\n\nValidate measured UI sizes against an explicit semantic layout contract.\n\npositional arguments:\n  contract    layout contract JSON path or 'self-test'\n\noptions:\n  -h, --help  show this help message and exit\n";

const SELF_TEST_OUT: &str = "{\n  \"ok\": true,\n  \"checks\": [\n    {\n      \"name\": \"valid contract\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"semantic mismatch rejected\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"out-of-tolerance size rejected\",\n      \"passed\": true\n    }\n  ]\n}\n";

#[derive(Clone, Debug)]
enum JsonVal {
    Null,
    Bool(bool),
    Int { raw: String, f: f64 },
    Float(f64),
    Str(Vec<u32>),
    Arr(Vec<JsonVal>),
    Obj(Vec<(Vec<u32>, JsonVal)>),
}

fn is_negative_number_arg(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('-') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    if rest.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    if let Some((before, after)) = rest.split_once('.') {
        return before.bytes().all(|b| b.is_ascii_digit())
            && !after.is_empty()
            && after.bytes().all(|b| b.is_ascii_digit());
    }
    false
}

enum CliResult {
    Help,
    Error,
    Contract(String),
}

fn parse_cli(args: &[String]) -> CliResult {
    let mut positionals = Vec::new();
    let mut double_dash = false;
    let mut unrecognized = false;

    for arg in args {
        if double_dash {
            positionals.push(arg.clone());
            continue;
        }
        if arg == "--" {
            double_dash = true;
            continue;
        }
        if !arg.starts_with('-') || arg == "-" || arg.contains(' ') || is_negative_number_arg(arg) {
            positionals.push(arg.clone());
            continue;
        }
        if let Some(rest) = arg.strip_prefix("--") {
            if !rest.is_empty() && "help".starts_with(rest) {
                return CliResult::Help;
            }
            unrecognized = true;
        } else {
            let rest = &arg[1..];
            if !rest.is_empty() && rest.bytes().all(|b| b == b'h') {
                return CliResult::Help;
            }
            unrecognized = true;
        }
    }

    if unrecognized || positionals.len() != 1 {
        return CliResult::Error;
    }
    CliResult::Contract(positionals.into_iter().next().unwrap())
}

fn normalize_py_path(raw: &str) -> String {
    if raw.is_empty() {
        return ".".to_string();
    }
    let leading_slashes = raw.bytes().take_while(|&b| b == b'/').count();
    let prefix = match leading_slashes {
        0 => "",
        2 => "//",
        _ => "/",
    };
    let parts: Vec<&str> = raw
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if prefix.is_empty() && parts.is_empty() {
        ".".to_string()
    } else {
        format!("{}{}", prefix, parts.join("/"))
    }
}

fn py_repr_str(s: &str) -> String {
    let use_double = s.contains('\'') && !s.contains('"');
    let quote = if use_double { '"' } else { '\'' };
    let mut out = String::new();
    out.push(quote);
    for c in s.chars() {
        if c == '\\' || c == quote {
            out.push('\\');
            out.push(c);
        } else if c == '\n' {
            out.push_str("\\n");
        } else if c == '\r' {
            out.push_str("\\r");
        } else if c == '\t' {
            out.push_str("\\t");
        } else if (c as u32) < 0x20 || (c as u32) == 0x7f {
            out.push_str(&format!("\\x{:02x}", c as u32));
        } else {
            out.push(c);
        }
    }
    out.push(quote);
    out
}

fn format_os_error(err: &std::io::Error, norm_path: &str) -> String {
    let code = err.raw_os_error().unwrap_or(2);
    let msg = err.to_string();
    let suffix = format!(" (os error {code})");
    let strerror = msg.strip_suffix(&suffix).unwrap_or(&msg);
    format!("[Errno {code}] {strerror}: {}", py_repr_str(norm_path))
}

fn universal_newlines(s: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

fn json_err(doc: &[char], pos: usize, msg: &str) -> String {
    let slice = &doc[..pos.min(doc.len())];
    let lineno = slice.iter().filter(|&&c| c == '\n').count() + 1;
    let colno = match slice.iter().rposition(|&c| c == '\n') {
        Some(idx) => pos - idx,
        None => pos + 1,
    };
    format!("{msg}: line {lineno} column {colno} (char {pos})")
}

enum ParseFailure {
    DecodeError(String),
    Crash,
}

struct Parser<'a> {
    s: &'a [char],
    i: usize,
}

impl<'a> Parser<'a> {
    fn is_ws(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\n' | '\r')
    }

    fn skip_ws(&mut self) {
        while self.i < self.s.len() && Self::is_ws(self.s[self.i]) {
            self.i += 1;
        }
    }

    fn starts_with(&self, lit: &str) -> bool {
        let mut idx = self.i;
        for ch in lit.chars() {
            if idx >= self.s.len() || self.s[idx] != ch {
                return false;
            }
            idx += 1;
        }
        true
    }

    fn parse_doc(&mut self) -> Result<JsonVal, ParseFailure> {
        if self.s.first() == Some(&'\u{feff}') {
            return Err(ParseFailure::DecodeError(json_err(
                self.s,
                0,
                "Unexpected UTF-8 BOM (decode using utf-8-sig)",
            )));
        }
        self.skip_ws();
        let val = self.scan_once()?;
        self.skip_ws();
        if self.i != self.s.len() {
            return Err(ParseFailure::DecodeError(json_err(
                self.s,
                self.i,
                "Extra data",
            )));
        }
        Ok(val)
    }

    fn scan_once(&mut self) -> Result<JsonVal, ParseFailure> {
        if self.i >= self.s.len() {
            return Err(ParseFailure::DecodeError(json_err(
                self.s,
                self.i,
                "Expecting value",
            )));
        }
        match self.s[self.i] {
            '"' => self.parse_string().map(JsonVal::Str),
            '{' => self.parse_object(),
            '[' => self.parse_array(),
            'n' if self.starts_with("null") => {
                self.i += 4;
                Ok(JsonVal::Null)
            }
            't' if self.starts_with("true") => {
                self.i += 4;
                Ok(JsonVal::Bool(true))
            }
            'f' if self.starts_with("false") => {
                self.i += 5;
                Ok(JsonVal::Bool(false))
            }
            'N' if self.starts_with("NaN") => {
                self.i += 3;
                Ok(JsonVal::Float(f64::NAN))
            }
            'I' if self.starts_with("Infinity") => {
                self.i += 8;
                Ok(JsonVal::Float(f64::INFINITY))
            }
            '-' if self.starts_with("-Infinity") => {
                self.i += 9;
                Ok(JsonVal::Float(f64::NEG_INFINITY))
            }
            '-' | '0'..='9' => self.parse_number(),
            _ => Err(ParseFailure::DecodeError(json_err(
                self.s,
                self.i,
                "Expecting value",
            ))),
        }
    }

    fn parse_hex4(&self, pos: usize) -> Option<u32> {
        if pos + 4 > self.s.len() {
            return None;
        }
        let mut val = 0u32;
        for k in 0..4 {
            let d = self.s[pos + k].to_digit(16)?;
            val = (val << 4) | d;
        }
        Some(val)
    }

    fn parse_string(&mut self) -> Result<Vec<u32>, ParseFailure> {
        let start = self.i;
        self.i += 1; // skip opening '"'
        let mut out = Vec::new();
        loop {
            if self.i >= self.s.len() {
                return Err(ParseFailure::DecodeError(json_err(
                    self.s,
                    start,
                    "Unterminated string starting at",
                )));
            }
            let c = self.s[self.i];
            self.i += 1;
            if c == '"' {
                return Ok(out);
            }
            if (c as u32) < 0x20 {
                return Err(ParseFailure::DecodeError(json_err(
                    self.s,
                    self.i - 1,
                    "Invalid control character at",
                )));
            }
            if c == '\\' {
                if self.i >= self.s.len() {
                    return Err(ParseFailure::DecodeError(json_err(
                        self.s,
                        start,
                        "Unterminated string starting at",
                    )));
                }
                let esc = self.s[self.i];
                self.i += 1;
                match esc {
                    '"' | '\\' | '/' => out.push(esc as u32),
                    'b' => out.push(0x08),
                    'f' => out.push(0x0c),
                    'n' => out.push(0x0a),
                    'r' => out.push(0x0d),
                    't' => out.push(0x09),
                    'u' => {
                        let u_pos = self.i - 1;
                        let Some(uni) = self.parse_hex4(self.i) else {
                            return Err(ParseFailure::DecodeError(json_err(
                                self.s,
                                u_pos,
                                "Invalid \\uXXXX escape",
                            )));
                        };
                        self.i += 4;
                        if (0xD800..=0xDBFF).contains(&uni)
                            && self.i + 2 <= self.s.len()
                            && self.s[self.i] == '\\'
                            && self.s[self.i + 1] == 'u'
                        {
                            if let Some(uni2) = self.parse_hex4(self.i + 2) {
                                if (0xDC00..=0xDFFF).contains(&uni2) {
                                    self.i += 6;
                                    let combined =
                                        0x10000 + (((uni - 0xD800) << 10) | (uni2 - 0xDC00));
                                    out.push(combined);
                                    continue;
                                }
                            }
                        }
                        out.push(uni);
                    }
                    _ => {
                        return Err(ParseFailure::DecodeError(json_err(
                            self.s,
                            self.i - 2,
                            "Invalid \\escape",
                        )));
                    }
                }
            } else {
                out.push(c as u32);
            }
        }
    }

    fn parse_object(&mut self) -> Result<JsonVal, ParseFailure> {
        self.i += 1; // skip '{'
        self.skip_ws();
        let mut items: Vec<(Vec<u32>, JsonVal)> = Vec::new();
        if self.i < self.s.len() && self.s[self.i] == '}' {
            self.i += 1;
            return Ok(JsonVal::Obj(items));
        }
        if self.i >= self.s.len() || self.s[self.i] != '"' {
            return Err(ParseFailure::DecodeError(json_err(
                self.s,
                self.i,
                "Expecting property name enclosed in double quotes",
            )));
        }
        loop {
            let key = self.parse_string()?;
            self.skip_ws();
            if self.i >= self.s.len() || self.s[self.i] != ':' {
                return Err(ParseFailure::DecodeError(json_err(
                    self.s,
                    self.i,
                    "Expecting ':' delimiter",
                )));
            }
            self.i += 1;
            self.skip_ws();
            let val = self.scan_once()?;
            if let Some(existing) = items.iter_mut().find(|(k, _)| *k == key) {
                existing.1 = val;
            } else {
                items.push((key, val));
            }
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == '}' {
                self.i += 1;
                return Ok(JsonVal::Obj(items));
            }
            if self.i >= self.s.len() || self.s[self.i] != ',' {
                return Err(ParseFailure::DecodeError(json_err(
                    self.s,
                    self.i,
                    "Expecting ',' delimiter",
                )));
            }
            let comma_pos = self.i;
            self.i += 1;
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == '}' {
                return Err(ParseFailure::DecodeError(json_err(
                    self.s,
                    comma_pos,
                    "Illegal trailing comma before end of object",
                )));
            }
            if self.i >= self.s.len() || self.s[self.i] != '"' {
                return Err(ParseFailure::DecodeError(json_err(
                    self.s,
                    self.i,
                    "Expecting property name enclosed in double quotes",
                )));
            }
        }
    }

    fn parse_array(&mut self) -> Result<JsonVal, ParseFailure> {
        self.i += 1; // skip '['
        self.skip_ws();
        let mut items = Vec::new();
        if self.i < self.s.len() && self.s[self.i] == ']' {
            self.i += 1;
            return Ok(JsonVal::Arr(items));
        }
        loop {
            let val = self.scan_once()?;
            items.push(val);
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == ']' {
                self.i += 1;
                return Ok(JsonVal::Arr(items));
            }
            if self.i >= self.s.len() || self.s[self.i] != ',' {
                return Err(ParseFailure::DecodeError(json_err(
                    self.s,
                    self.i,
                    "Expecting ',' delimiter",
                )));
            }
            let comma_pos = self.i;
            self.i += 1;
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == ']' {
                return Err(ParseFailure::DecodeError(json_err(
                    self.s,
                    comma_pos,
                    "Illegal trailing comma before end of array",
                )));
            }
        }
    }

    fn parse_number(&mut self) -> Result<JsonVal, ParseFailure> {
        let start = self.i;
        let mut p = self.i;
        if p < self.s.len() && self.s[p] == '-' {
            p += 1;
        }
        if p >= self.s.len() || !self.s[p].is_ascii_digit() {
            return Err(ParseFailure::DecodeError(json_err(
                self.s,
                start,
                "Expecting value",
            )));
        }
        if self.s[p] == '0' {
            p += 1;
        } else {
            while p < self.s.len() && self.s[p].is_ascii_digit() {
                p += 1;
            }
        }
        let mut is_float = false;
        if p + 1 < self.s.len() && self.s[p] == '.' && self.s[p + 1].is_ascii_digit() {
            is_float = true;
            p += 2;
            while p < self.s.len() && self.s[p].is_ascii_digit() {
                p += 1;
            }
        }
        if p < self.s.len() && (self.s[p] == 'e' || self.s[p] == 'E') {
            let mut ep = p + 1;
            if ep < self.s.len() && (self.s[ep] == '+' || self.s[ep] == '-') {
                ep += 1;
            }
            if ep < self.s.len() && self.s[ep].is_ascii_digit() {
                is_float = true;
                ep += 1;
                while ep < self.s.len() && self.s[ep].is_ascii_digit() {
                    ep += 1;
                }
                p = ep;
            }
        }
        let raw: String = self.s[start..p].iter().collect();
        self.i = p;
        if is_float {
            let f = raw.parse::<f64>().unwrap_or(if raw.starts_with('-') {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            });
            Ok(JsonVal::Float(f))
        } else {
            let digits = raw.strip_prefix('-').unwrap_or(&raw);
            if digits.len() > 4300 {
                return Err(ParseFailure::Crash);
            }
            let f = raw.parse::<f64>().unwrap_or(if raw.starts_with('-') {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            });
            Ok(JsonVal::Int { raw, f })
        }
    }
}

fn u32_from_str(s: &str) -> Vec<u32> {
    s.chars().map(|c| c as u32).collect()
}

fn u32_to_ascii(v: &[u32]) -> Option<String> {
    let mut s = String::with_capacity(v.len());
    for &cp in v {
        if cp < 128 {
            s.push(cp as u8 as char);
        } else {
            return None;
        }
    }
    Some(s)
}

fn is_py_space(cp: u32) -> bool {
    matches!(
        cp,
        0x0009..=0x000D
            | 0x001C..=0x0020
            | 0x0085
            | 0x00A0
            | 0x1680
            | 0x2000..=0x200A
            | 0x2028..=0x2029
            | 0x202F
            | 0x205F
            | 0x3000
    )
}

fn nonempty_text(val: Option<&JsonVal>) -> bool {
    let Some(JsonVal::Str(chars)) = val else {
        return false;
    };
    chars.iter().any(|&cp| !is_py_space(cp))
}

fn get_field<'a>(obj: &'a [(Vec<u32>, JsonVal)], key: &str) -> Option<&'a JsonVal> {
    let kb = u32_from_str(key);
    obj.iter().find(|(k, _)| *k == kb).map(|(_, v)| v)
}

fn finite_number(val: Option<&JsonVal>) -> Result<Option<f64>, ()> {
    match val {
        Some(JsonVal::Int { f, .. }) => {
            if f.is_infinite() {
                Err(())
            } else {
                Ok(Some(*f))
            }
        }
        Some(JsonVal::Float(f)) => {
            if f.is_finite() {
                Ok(Some(*f))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

fn py_format_float(f: f64) -> String {
    if f == 0.0 {
        return if f.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let neg = f.is_sign_negative();
    let abs_s = f.abs().to_string();
    let (sig, exp): (String, i32) = if let Some((int_part, frac_part)) = abs_s.split_once('.') {
        if int_part == "0" {
            let zeros = frac_part.bytes().take_while(|&b| b == b'0').count();
            let sig = frac_part[zeros..].trim_end_matches('0').to_string();
            let exp = -(zeros as i32) - 1;
            (sig, exp)
        } else {
            let combined = format!("{int_part}{frac_part}");
            let sig = combined.trim_end_matches('0').to_string();
            let exp = (int_part.len() as i32) - 1;
            (sig, exp)
        }
    } else {
        let sig = abs_s.trim_end_matches('0').to_string();
        let exp = (abs_s.len() as i32) - 1;
        (sig, exp)
    };

    let sign_prefix = if neg { "-" } else { "" };
    if (-4..16).contains(&exp) {
        if abs_s.contains('.') {
            format!("{sign_prefix}{abs_s}")
        } else {
            format!("{sign_prefix}{abs_s}.0")
        }
    } else {
        let exp_sign = if exp >= 0 { '+' } else { '-' };
        let exp_abs = exp.unsigned_abs();
        let mantissa = if sig.len() == 1 {
            sig
        } else {
            format!("{}.{}", &sig[..1], &sig[1..])
        };
        format!("{sign_prefix}{mantissa}e{exp_sign}{exp_abs:02}")
    }
}

fn py_format_num(val: &JsonVal) -> String {
    match val {
        JsonVal::Int { raw, .. } => raw.clone(),
        JsonVal::Float(f) => py_format_float(*f),
        _ => unreachable!(),
    }
}

fn validate(contract: &JsonVal) -> Result<Vec<Vec<u32>>, ()> {
    let JsonVal::Obj(root) = contract else {
        return Ok(vec![u32_from_str("contract root must be an object")]);
    };
    let mut errors: Vec<Vec<u32>> = Vec::new();
    let version_ok = match get_field(root, "version") {
        Some(JsonVal::Int { raw, .. }) => raw == "1",
        Some(JsonVal::Float(f)) => *f == 1.0,
        Some(JsonVal::Bool(true)) => true,
        _ => false,
    };
    if !version_ok {
        errors.push(u32_from_str("version must be 1"));
    }

    let Some(JsonVal::Arr(targets)) = get_field(root, "targets") else {
        errors.push(u32_from_str("targets must be a non-empty array"));
        return Ok(errors);
    };
    if targets.is_empty() {
        errors.push(u32_from_str("targets must be a non-empty array"));
        return Ok(errors);
    }

    const KINDS: &[&str] = &[
        "cell",
        "column",
        "container",
        "content",
        "gap",
        "row",
        "thumbnail",
    ];
    const METHODS: &[&str] = &["computedStyle", "getBoundingClientRect"];
    const AXES: &[&str] = &["both", "height", "width"];

    let mut names: HashSet<Vec<u32>> = HashSet::new();

    for (index, target_val) in targets.iter().enumerate() {
        let field = format!("targets[{index}]");
        let JsonVal::Obj(target) = target_val else {
            errors.push(u32_from_str(&format!("{field} must be an object")));
            continue;
        };

        let name_val = get_field(target, "name");
        if !nonempty_text(name_val) {
            errors.push(u32_from_str(&format!("{field}.name must be non-empty text")));
        } else if let Some(JsonVal::Str(name_chars)) = name_val {
            if names.contains(name_chars) {
                let mut msg = u32_from_str("duplicate target name: ");
                msg.extend_from_slice(name_chars);
                errors.push(msg);
            } else {
                names.insert(name_chars.clone());
            }
        }

        let req_val = get_field(target, "requested_kind");
        if matches!(req_val, Some(JsonVal::Arr(_) | JsonVal::Obj(_))) {
            return Err(());
        }
        let req_kind = match req_val {
            Some(JsonVal::Str(s)) => u32_to_ascii(s).filter(|k| KINDS.contains(&k.as_str())),
            _ => None,
        };

        let meas_val = get_field(target, "measured_kind");
        if matches!(meas_val, Some(JsonVal::Arr(_) | JsonVal::Obj(_))) {
            return Err(());
        }
        let meas_kind = match meas_val {
            Some(JsonVal::Str(s)) => u32_to_ascii(s).filter(|k| KINDS.contains(&k.as_str())),
            _ => None,
        };

        if req_kind.is_none() {
            errors.push(u32_from_str(&format!(
                "{field}.requested_kind must be one of ['cell', 'column', 'container', 'content', 'gap', 'row', 'thumbnail']"
            )));
        }
        if meas_kind.is_none() {
            errors.push(u32_from_str(&format!(
                "{field}.measured_kind must be one of ['cell', 'column', 'container', 'content', 'gap', 'row', 'thumbnail']"
            )));
        } else if let (Some(rk), Some(mk)) = (&req_kind, &meas_kind) {
            if mk != rk {
                errors.push(u32_from_str(&format!(
                    "{field} measures {mk}, not requested {rk}"
                )));
            }
        }

        for key in ["selector", "layout_owner", "runtime_context"] {
            if !nonempty_text(get_field(target, key)) {
                errors.push(u32_from_str(&format!("{field}.{key} must be non-empty text")));
            }
        }

        let constraints_ok = match get_field(target, "conflicting_constraints") {
            Some(JsonVal::Arr(items)) => items.iter().all(|it| nonempty_text(Some(it))),
            _ => false,
        };
        if !constraints_ok {
            errors.push(u32_from_str(&format!(
                "{field}.conflicting_constraints must be a text array"
            )));
        }

        let method_val = get_field(target, "measurement_method");
        if matches!(method_val, Some(JsonVal::Arr(_) | JsonVal::Obj(_))) {
            return Err(());
        }
        let method_ok = match method_val {
            Some(JsonVal::Str(s)) => u32_to_ascii(s)
                .map(|m| METHODS.contains(&m.as_str()))
                .unwrap_or(false),
            _ => false,
        };
        if !method_ok {
            errors.push(u32_from_str(&format!(
                "{field}.measurement_method must be one of ['computedStyle', 'getBoundingClientRect']"
            )));
        }

        let axis_val = get_field(target, "axis");
        if matches!(axis_val, Some(JsonVal::Arr(_) | JsonVal::Obj(_))) {
            return Err(());
        }
        let axis_str = match axis_val {
            Some(JsonVal::Str(s)) => u32_to_ascii(s).filter(|a| AXES.contains(&a.as_str())),
            _ => None,
        };
        let Some(axis) = axis_str else {
            errors.push(u32_from_str(&format!(
                "{field}.axis must be one of ['both', 'height', 'width']"
            )));
            continue;
        };

        let dims: &[&str] = match axis.as_str() {
            "width" => &["width"],
            "height" => &["height"],
            "both" => &["width", "height"],
            _ => unreachable!(),
        };

        let default_tol = JsonVal::Int {
            raw: "1".to_string(),
            f: 1.0,
        };
        let tol_val = get_field(target, "tolerance_px").unwrap_or(&default_tol);
        let Some(tol_f) = finite_number(Some(tol_val))? else {
            errors.push(u32_from_str(&format!(
                "{field}.tolerance_px must be a non-negative finite number"
            )));
            continue;
        };
        if tol_f < 0.0 {
            errors.push(u32_from_str(&format!(
                "{field}.tolerance_px must be a non-negative finite number"
            )));
            continue;
        }

        let before = get_field(target, "before_px");
        let goal = get_field(target, "target_px");
        let measured = get_field(target, "runtime_px");

        for (label, value) in [
            ("before_px", before),
            ("target_px", goal),
            ("runtime_px", measured),
        ] {
            let Some(JsonVal::Obj(obj)) = value else {
                errors.push(u32_from_str(&format!("{field}.{label} must be an object")));
                continue;
            };
            for &dim in dims {
                let dval = get_field(obj, dim);
                match finite_number(dval)? {
                    Some(f) if f >= 0.0 => {}
                    _ => {
                        errors.push(u32_from_str(&format!(
                            "{field}.{label}.{dim} must be a non-negative finite number"
                        )));
                    }
                }
            }
        }

        if let (Some(JsonVal::Obj(goal_obj)), Some(JsonVal::Obj(meas_obj))) = (goal, measured) {
            for &dim in dims {
                let g_val = get_field(goal_obj, dim);
                let m_val = get_field(meas_obj, dim);
                if let (Some(gf), Some(mf)) = (finite_number(g_val)?, finite_number(m_val)?) {
                    let diff = (mf - gf).abs();
                    if diff > tol_f {
                        let g_str = py_format_num(g_val.unwrap());
                        let m_str = py_format_num(m_val.unwrap());
                        let t_str = py_format_num(tol_val);
                        errors.push(u32_from_str(&format!(
                            "{field}.{dim} differs by {diff:.3}px (target {g_str}px, measured {m_str}px, tolerance {t_str}px)"
                        )));
                    }
                }
            }
        }
    }

    Ok(errors)
}

fn encode_json_string(chars: &[u32]) -> Option<String> {
    let mut out = String::from("\"");
    for &cp in chars {
        if (0xD800..=0xDFFF).contains(&cp) {
            return None;
        }
        match cp {
            0x22 => out.push_str("\\\""),
            0x5c => out.push_str("\\\\"),
            0x08 => out.push_str("\\b"),
            0x0c => out.push_str("\\f"),
            0x0a => out.push_str("\\n"),
            0x0d => out.push_str("\\r"),
            0x09 => out.push_str("\\t"),
            0x00..=0x1f => out.push_str(&format!("\\u{cp:04x}")),
            _ => out.push(char::from_u32(cp)?),
        }
    }
    out.push('"');
    Some(out)
}

fn format_output(ok: bool, errors: &[Vec<u32>]) -> Option<String> {
    let ok_str = if ok { "true" } else { "false" };
    if errors.is_empty() {
        return Some(format!("{{\n  \"ok\": {ok_str},\n  \"errors\": []\n}}\n"));
    }
    let mut encoded = Vec::with_capacity(errors.len());
    for err in errors {
        encoded.push(format!("    {}", encode_json_string(err)?));
    }
    Some(format!(
        "{{\n  \"ok\": {ok_str},\n  \"errors\": [\n{}\n  ]\n}}\n",
        encoded.join(",\n")
    ))
}

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let contract_arg = match parse_cli(&raw_args) {
        CliResult::Help => {
            print!("{HELP_TEXT}");
            return ExitCode::from(0);
        }
        CliResult::Error => {
            return ExitCode::from(2);
        }
        CliResult::Contract(c) => c,
    };

    if contract_arg == "self-test" {
        print!("{SELF_TEST_OUT}");
        return ExitCode::from(0);
    }

    let norm_path = normalize_py_path(&contract_arg);
    let bytes = match fs::read(&norm_path) {
        Ok(b) => b,
        Err(e) => {
            let msg = u32_from_str(&format_os_error(&e, &norm_path));
            if let Some(s) = format_output(false, &[msg]) {
                print!("{s}");
            }
            return ExitCode::from(2);
        }
    };

    let Ok(text) = std::str::from_utf8(&bytes) else {
        return ExitCode::from(1);
    };

    let chars = universal_newlines(text);
    let mut parser = Parser { s: &chars, i: 0 };
    let val = match parser.parse_doc() {
        Ok(v) => v,
        Err(ParseFailure::DecodeError(msg)) => {
            let m = u32_from_str(&msg);
            if let Some(s) = format_output(false, &[m]) {
                print!("{s}");
            }
            return ExitCode::from(2);
        }
        Err(ParseFailure::Crash) => {
            return ExitCode::from(1);
        }
    };

    let Ok(errors) = validate(&val) else {
        return ExitCode::from(1);
    };

    let ok = errors.is_empty();
    let Some(out) = format_output(ok, &errors) else {
        return ExitCode::from(1);
    };
    print!("{out}");
    ExitCode::from(if ok { 0 } else { 1 })
}
