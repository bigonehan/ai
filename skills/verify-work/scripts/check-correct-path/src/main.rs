mod tables;

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

#[cfg(unix)]
extern "C" {
    fn access(pathname: *const i8, mode: i32) -> i32;
}

fn in_range(table: &[(u32, u32)], ch: char) -> bool {
    let c = ch as u32;
    table
        .binary_search_by(|&(lo, hi)| {
            if c < lo {
                Ordering::Greater
            } else if c > hi {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

fn is_py_digit(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| in_range(tables::ISDIGIT, c))
}

fn is_py_space(ch: char) -> bool {
    in_range(tables::ISSPACE, ch)
}

fn is_py_printable(ch: char) -> bool {
    in_range(tables::ISPRINTABLE, ch)
}

fn py_decimal(ch: char) -> Option<u32> {
    let c = ch as u32;
    tables::DECIMAL
        .binary_search_by(|&(lo, hi, _)| {
            if c < lo {
                Ordering::Greater
            } else if c > hi {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })
        .ok()
        .map(|idx| {
            let (lo, _, base) = tables::DECIMAL[idx];
            base + (c - lo)
        })
}

fn parse_py_int(raw: &str) -> Option<i64> {
    let trimmed = raw.trim_matches(is_py_space);
    if trimmed.is_empty() {
        return None;
    }
    let (neg, rest) = if let Some(r) = trimmed.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = trimmed.strip_prefix('+') {
        (false, r)
    } else {
        (false, trimmed)
    };
    if rest.is_empty() {
        return None;
    }
    let mut acc: i128 = 0;
    for ch in rest.chars() {
        if ch == '_' {
            continue;
        }
        let d = py_decimal(ch)? as i128;
        acc = acc.saturating_mul(10).saturating_add(d);
    }
    let signed = if neg { -acc } else { acc };
    i64::try_from(signed).ok()
}

#[derive(Clone, Debug)]
enum JsonVal {
    Null,
    Bool(bool),
    Int(String),
    Float(f64),
    Str(String),
    List(Vec<JsonVal>),
    Dict(Vec<(String, JsonVal)>),
}

impl JsonVal {
    fn py_type_name(&self) -> &'static str {
        match self {
            JsonVal::Null => "NoneType",
            JsonVal::Bool(_) => "bool",
            JsonVal::Int(_) => "int",
            JsonVal::Float(_) => "float",
            JsonVal::Str(_) => "str",
            JsonVal::List(_) => "list",
            JsonVal::Dict(_) => "dict",
        }
    }

    fn py_str(&self) -> String {
        match self {
            JsonVal::Str(s) => s.clone(),
            other => other.py_repr(),
        }
    }

    fn py_repr(&self) -> String {
        match self {
            JsonVal::Null => "None".to_string(),
            JsonVal::Bool(true) => "True".to_string(),
            JsonVal::Bool(false) => "False".to_string(),
            JsonVal::Int(s) => s.clone(),
            JsonVal::Float(f) => py_float_str(*f),
            JsonVal::Str(s) => py_repr_str(s),
            JsonVal::List(items) => {
                let inner: Vec<String> = items.iter().map(|v| v.py_repr()).collect();
                format!("[{}]", inner.join(", "))
            }
            JsonVal::Dict(entries) => {
                let inner: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", py_repr_str(k), v.py_repr()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
        }
    }
}

fn py_float_str(f: f64) -> String {
    if f.is_nan() {
        return "nan".to_string();
    }
    if f.is_infinite() {
        return if f.is_sign_negative() {
            "-inf".to_string()
        } else {
            "inf".to_string()
        };
    }
    if f == 0.0 {
        return if f.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let abs = f.abs();
    if (1e-4..1e16).contains(&abs) {
        let s = format!("{f}");
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            format!("{s}.0")
        } else {
            s
        }
    } else {
        let s = format!("{f:e}");
        if let Some((mant, exp_str)) = s.split_once('e') {
            if let Ok(exp) = exp_str.parse::<i32>() {
                if exp >= 0 {
                    return format!("{mant}e+{exp:02}");
                } else {
                    return format!("{mant}e-{:02}", -exp);
                }
            }
        }
        s
    }
}

fn py_repr_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if !is_py_printable(c) => {
                let u = c as u32;
                if u <= 0xFF {
                    out.push_str(&format!("\\x{u:02x}"));
                } else if u <= 0xFFFF {
                    out.push_str(&format!("\\u{u:04x}"));
                } else {
                    out.push_str(&format!("\\U{u:08x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn parse(mut self) -> Result<JsonVal, &'static str> {
        if self.bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            return Err("Unexpected UTF-8 BOM (decode using utf-8-sig)");
        }
        self.skip_ws();
        let val = self.parse_value()?;
        self.skip_ws();
        if self.pos < self.bytes.len() {
            return Err("Extra data");
        }
        Ok(val)
    }

    fn parse_value(&mut self) -> Result<JsonVal, &'static str> {
        self.skip_ws();
        if self.pos >= self.bytes.len() {
            return Err("Expecting value");
        }
        match self.bytes[self.pos] {
            b'"' => self.parse_string().map(JsonVal::Str),
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b't' if self.bytes[self.pos..].starts_with(b"true") => {
                self.pos += 4;
                Ok(JsonVal::Bool(true))
            }
            b'f' if self.bytes[self.pos..].starts_with(b"false") => {
                self.pos += 5;
                Ok(JsonVal::Bool(false))
            }
            b'n' if self.bytes[self.pos..].starts_with(b"null") => {
                self.pos += 4;
                Ok(JsonVal::Null)
            }
            b'N' if self.bytes[self.pos..].starts_with(b"NaN") => {
                self.pos += 3;
                Ok(JsonVal::Float(f64::NAN))
            }
            b'I' if self.bytes[self.pos..].starts_with(b"Infinity") => {
                self.pos += 8;
                Ok(JsonVal::Float(f64::INFINITY))
            }
            b'-' if self.bytes[self.pos..].starts_with(b"-Infinity") => {
                self.pos += 9;
                Ok(JsonVal::Float(f64::NEG_INFINITY))
            }
            b'-' | b'0'..=b'9' => self.parse_number(),
            _ => Err("Expecting value"),
        }
    }

    fn parse_string(&mut self) -> Result<String, &'static str> {
        self.pos += 1; // skip '"'
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b < 0x20 {
                return Err("Invalid control character at");
            }
            if b == b'\\' {
                self.pos += 1;
                if self.pos >= self.bytes.len() {
                    return Err("Unterminated string starting at");
                }
                let esc = self.bytes[self.pos];
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000C}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let code = self.parse_hex4()?;
                        if (0xD800..=0xDBFF).contains(&code) {
                            if self.pos + 6 <= self.bytes.len()
                                && self.bytes[self.pos] == b'\\'
                                && self.bytes[self.pos + 1] == b'u'
                            {
                                let saved = self.pos;
                                self.pos += 2;
                                let low = self.parse_hex4()?;
                                if (0xDC00..=0xDFFF).contains(&low) {
                                    let combined = 0x10000
                                        + (((code as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                                    if let Some(ch) = char::from_u32(combined) {
                                        out.push(ch);
                                        continue;
                                    }
                                } else {
                                    self.pos = saved;
                                }
                            }
                            out.push('\u{FFFD}');
                        } else if let Some(ch) = char::from_u32(code as u32) {
                            out.push(ch);
                        } else {
                            out.push('\u{FFFD}');
                        }
                    }
                    _ => return Err("Invalid \\escape"),
                }
            } else {
                let s = std::str::from_utf8(&self.bytes[self.pos..]).map_err(|_| "Expecting value")?;
                let ch = s.chars().next().ok_or("Unterminated string starting at")?;
                self.pos += ch.len_utf8();
                out.push(ch);
            }
        }
        Err("Unterminated string starting at")
    }

    fn parse_hex4(&mut self) -> Result<u16, &'static str> {
        if self.pos + 4 > self.bytes.len() {
            return Err("Invalid \\uXXXX escape");
        }
        let slice = &self.bytes[self.pos..self.pos + 4];
        let s = std::str::from_utf8(slice).map_err(|_| "Invalid \\uXXXX escape")?;
        let val = u16::from_str_radix(s, 16).map_err(|_| "Invalid \\uXXXX escape")?;
        self.pos += 4;
        Ok(val)
    }

    fn parse_number(&mut self) -> Result<JsonVal, &'static str> {
        let start = self.pos;
        if self.bytes[self.pos] == b'-' {
            self.pos += 1;
            if self.pos >= self.bytes.len() {
                return Err("Expecting value");
            }
        }
        if self.bytes[self.pos] == b'0' {
            self.pos += 1;
        } else if self.bytes[self.pos].is_ascii_digit() {
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        } else {
            return Err("Expecting value");
        }

        let mut is_float = false;
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'.' {
            is_float = true;
            self.pos += 1;
            if self.pos >= self.bytes.len() || !self.bytes[self.pos].is_ascii_digit() {
                return Err("Expecting value");
            }
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        if self.pos < self.bytes.len() && (self.bytes[self.pos] == b'e' || self.bytes[self.pos] == b'E')
        {
            is_float = true;
            self.pos += 1;
            if self.pos < self.bytes.len()
                && (self.bytes[self.pos] == b'+' || self.bytes[self.pos] == b'-')
            {
                self.pos += 1;
            }
            if self.pos >= self.bytes.len() || !self.bytes[self.pos].is_ascii_digit() {
                return Err("Expecting value");
            }
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|_| "Expecting value")?;
        if is_float {
            let f: f64 = raw.parse().map_err(|_| "Expecting value")?;
            Ok(JsonVal::Float(f))
        } else {
            let normalized = if raw == "-0" { "0" } else { raw };
            Ok(JsonVal::Int(normalized.to_string()))
        }
    }

    fn parse_array(&mut self) -> Result<JsonVal, &'static str> {
        self.pos += 1; // skip '['
        self.skip_ws();
        let mut items = Vec::new();
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b']' {
            self.pos += 1;
            return Ok(JsonVal::List(items));
        }
        loop {
            let val = self.parse_value()?;
            items.push(val);
            self.skip_ws();
            if self.pos >= self.bytes.len() {
                return Err("Expecting ',' delimiter");
            }
            match self.bytes[self.pos] {
                b',' => {
                    self.pos += 1;
                    self.skip_ws();
                }
                b']' => {
                    self.pos += 1;
                    return Ok(JsonVal::List(items));
                }
                _ => return Err("Expecting ',' delimiter"),
            }
        }
    }

    fn parse_object(&mut self) -> Result<JsonVal, &'static str> {
        self.pos += 1; // skip '{'
        self.skip_ws();
        let mut entries: Vec<(String, JsonVal)> = Vec::new();
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'}' {
            self.pos += 1;
            return Ok(JsonVal::Dict(entries));
        }
        loop {
            self.skip_ws();
            if self.pos >= self.bytes.len() || self.bytes[self.pos] != b'"' {
                return Err("Expecting property name enclosed in double quotes");
            }
            let key = self.parse_string()?;
            self.skip_ws();
            if self.pos >= self.bytes.len() || self.bytes[self.pos] != b':' {
                return Err("Expecting ':' delimiter");
            }
            self.pos += 1;
            let val = self.parse_value()?;
            if let Some(existing) = entries.iter_mut().find(|(k, _)| *k == key) {
                existing.1 = val;
            } else {
                entries.push((key, val));
            }
            self.skip_ws();
            if self.pos >= self.bytes.len() {
                return Err("Expecting ',' delimiter");
            }
            match self.bytes[self.pos] {
                b',' => {
                    self.pos += 1;
                    self.skip_ws();
                }
                b'}' => {
                    self.pos += 1;
                    return Ok(JsonVal::Dict(entries));
                }
                _ => return Err("Expecting ',' delimiter"),
            }
        }
    }
}

fn parse_json(text: &str, label: &str) -> Result<JsonVal, String> {
    JsonParser::new(text)
        .parse()
        .map_err(|msg| format!("invalid {label} JSON: {msg}"))
}

#[derive(Clone, Debug)]
enum OutVal {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<OutVal>),
    Map(BTreeMap<String, OutVal>),
}

impl OutVal {
    fn get_ok(&self) -> bool {
        if let OutVal::Map(m) = self {
            if let Some(OutVal::Bool(b)) = m.get("ok") {
                return *b;
            }
        }
        false
    }

    fn format_pretty(&self, indent_level: usize) -> String {
        match self {
            OutVal::Null => "null".to_string(),
            OutVal::Bool(true) => "true".to_string(),
            OutVal::Bool(false) => "false".to_string(),
            OutVal::Int(n) => n.to_string(),
            OutVal::Str(s) => json_escape_str(s),
            OutVal::List(items) => {
                if items.is_empty() {
                    return "[]".to_string();
                }
                let child_indent = "  ".repeat(indent_level + 1);
                let close_indent = "  ".repeat(indent_level);
                let parts: Vec<String> = items
                    .iter()
                    .map(|v| format!("{child_indent}{}", v.format_pretty(indent_level + 1)))
                    .collect();
                format!("[\n{}\n{close_indent}]", parts.join(",\n"))
            }
            OutVal::Map(map) => {
                if map.is_empty() {
                    return "{}".to_string();
                }
                let child_indent = "  ".repeat(indent_level + 1);
                let close_indent = "  ".repeat(indent_level);
                let parts: Vec<String> = map
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{child_indent}{}: {}",
                            json_escape_str(k),
                            v.format_pretty(indent_level + 1)
                        )
                    })
                    .collect();
                format!("{{\n{}\n{close_indent}}}", parts.join(",\n"))
            }
        }
    }
}

fn json_escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn emit(result: OutVal) -> u8 {
    println!("{}", result.format_pretty(0));
    if result.get_ok() {
        0
    } else {
        1
    }
}

fn masked(value: &str, show: bool) -> String {
    if show {
        return value.to_string();
    }
    if value.is_empty() {
        return String::new();
    }
    let name = value.rsplit(['\\', '/']).next().unwrap_or("");
    if !name.is_empty() {
        format!("<redacted>/{name}")
    } else {
        "<redacted>".to_string()
    }
}

fn is_error_shape(value: &JsonVal) -> bool {
    match value {
        JsonVal::List(items) if !items.is_empty() => {
            if let JsonVal::Str(first) = &items[0] {
                let up = first.to_uppercase();
                matches!(up.as_str(), "E" | "ERROR" | "ERR" | "P" | "PENDING")
            } else {
                false
            }
        }
        JsonVal::Dict(entries) => entries
            .iter()
            .any(|(k, _)| matches!(k.as_str(), "error" | "errors" | "exception")),
        _ => false,
    }
}

fn validate_value(value: &JsonVal, expected_type: &str) -> (bool, Vec<String>) {
    let mut issues = Vec::new();
    if is_error_shape(value) {
        issues.push("structured error value reached the consumer".to_string());
    }
    let valid = match expected_type {
        "integer" => matches!(value, JsonVal::Int(_)),
        "opaque-string" => matches!(value, JsonVal::Str(_)),
        "path" => matches!(value, JsonVal::Str(s) if !s.is_empty()),
        "object" => matches!(value, JsonVal::Dict(_)),
        "array" => matches!(value, JsonVal::List(_)),
        _ => false,
    };
    if !valid {
        issues.push(format!(
            "expected {expected_type}, received {}",
            value.py_type_name()
        ));
    }
    if expected_type == "integer" {
        if let JsonVal::Str(s) = value {
            if is_py_digit(s) {
                issues.push("numeric string was not implicitly coerced".to_string());
            }
        }
    }
    (issues.is_empty(), issues)
}

fn find_placeholders(text: &str) -> Vec<String> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0;

    while i < n {
        let (start_byte, ch) = chars[i];
        // Branch 1: \$\{[^{}]+\}
        if ch == '$' && i + 1 < n && chars[i + 1].1 == '{' {
            let mut j = i + 2;
            while j < n && chars[j].1 != '{' && chars[j].1 != '}' {
                j += 1;
            }
            if j > i + 2 && j < n && chars[j].1 == '}' {
                let end_byte = chars[j].0 + chars[j].1.len_utf8();
                out.push(text[start_byte..end_byte].to_string());
                i = j + 1;
                continue;
            }
        }
        // Branch 2: \{\{[^{}]+\}\}
        if ch == '{' && i + 1 < n && chars[i + 1].1 == '{' {
            let mut j = i + 2;
            while j < n && chars[j].1 != '{' && chars[j].1 != '}' {
                j += 1;
            }
            if j > i + 2 && j + 1 < n && chars[j].1 == '}' && chars[j + 1].1 == '}' {
                let end_byte = chars[j + 1].0 + chars[j + 1].1.len_utf8();
                out.push(text[start_byte..end_byte].to_string());
                i = j + 2;
                continue;
            }
        }
        // Branch 3: (?<!\{)\{[A-Za-z_][^{}]*\}(?!\})
        if ch == '{' && (i == 0 || chars[i - 1].1 != '{') {
            if i + 1 < n && (chars[i + 1].1.is_ascii_alphabetic() || chars[i + 1].1 == '_') {
                let mut j = i + 2;
                while j < n && chars[j].1 != '{' && chars[j].1 != '}' {
                    j += 1;
                }
                if j < n && chars[j].1 == '}' && (j + 1 >= n || chars[j + 1].1 != '}') {
                    let end_byte = chars[j].0 + chars[j].1.len_utf8();
                    out.push(text[start_byte..end_byte].to_string());
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn url_quote(text: &str, safe: &[u8]) -> String {
    let mut out = String::new();
    for &b in text.as_bytes() {
        let is_unreserved =
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'~') || safe.contains(&b);
        if is_unreserved {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn url_unquote(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut raw = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h1), Some(h2)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                raw.push(((h1 << 4) | h2) as u8);
                i += 3;
                continue;
            }
        }
        raw.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&raw).into_owned()
}

fn inspect_template(
    source: &str,
    rendered: &str,
    variables: Option<&[(String, JsonVal)]>,
) -> BTreeMap<String, OutVal> {
    let placeholders = find_placeholders(source);
    let unresolved = find_placeholders(rendered);
    let mut issues = Vec::new();
    if !placeholders.is_empty() && source == rendered {
        issues.push("template output is unchanged".to_string());
    }
    if !unresolved.is_empty() {
        issues.push("rendered output contains unresolved placeholders".to_string());
    }

    let mut missing_variables = Vec::new();
    let mut error_variables = Vec::new();
    if let Some(vars) = variables {
        for (key, value) in vars {
            if is_error_shape(value) {
                error_variables.push(key.clone());
                continue;
            }
            let text = value.py_str();
            let c1 = text.clone();
            let c2 = url_quote(&text, b"");
            let c3 = url_quote(&text, b"/\\:");
            if !text.is_empty()
                && !rendered.contains(&c1)
                && !rendered.contains(&c2)
                && !rendered.contains(&c3)
            {
                missing_variables.push(key.clone());
            }
        }
    }
    if !missing_variables.is_empty() {
        issues.push("expected variable values are absent from rendered output".to_string());
    }
    if !error_variables.is_empty() {
        issues.push("structured error values were supplied as template variables".to_string());
    }
    let applied = !placeholders.is_empty()
        && source != rendered
        && unresolved.is_empty()
        && issues.is_empty();

    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), OutVal::Bool(issues.is_empty()));
    map.insert("applied".to_string(), OutVal::Bool(applied));
    map.insert(
        "placeholder_count".to_string(),
        OutVal::Int(placeholders.len() as i64),
    );
    map.insert(
        "unresolved".to_string(),
        OutVal::List(unresolved.into_iter().map(OutVal::Str).collect()),
    );
    map.insert(
        "missing_variables".to_string(),
        OutVal::List(missing_variables.into_iter().map(OutVal::Str).collect()),
    );
    map.insert(
        "error_variables".to_string(),
        OutVal::List(error_variables.into_iter().map(OutVal::Str).collect()),
    );
    map.insert(
        "issues".to_string(),
        OutVal::List(issues.into_iter().map(OutVal::Str).collect()),
    );
    map
}

fn has_bad_percent(s: &str) -> bool {
    let b = s.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'%' {
            let ok = i + 2 < b.len()
                && b[i + 1].is_ascii_hexdigit()
                && b[i + 2].is_ascii_hexdigit();
            if !ok {
                return true;
            }
        }
    }
    false
}

fn has_percent(s: &str) -> bool {
    let b = s.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && b[i + 1].is_ascii_hexdigit()
            && b[i + 2].is_ascii_hexdigit()
        {
            return true;
        }
    }
    false
}

fn decode_once(value: &str, mode: &str) -> (String, bool, bool, Vec<String>) {
    let mut issues = Vec::new();
    if has_bad_percent(value) {
        return (
            value.to_string(),
            false,
            false,
            vec!["malformed percent escape".to_string()],
        );
    }
    let encoded = has_percent(value);
    let should_decode = mode == "once" || (mode == "auto" && encoded);
    let decoded = if should_decode {
        url_unquote(value)
    } else {
        value.to_string()
    };
    let possible_double = should_decode && has_percent(&decoded);
    if possible_double {
        issues.push("percent escapes remain after one decode; possible double encoding".to_string());
    }
    (decoded, should_decode, possible_double, issues)
}

fn py_regex_line(s: &str) -> Option<&str> {
    if let Some(without_trailing_nl) = s.strip_suffix('\n') {
        if without_trailing_nl.contains('\n') {
            None
        } else {
            Some(without_trailing_nl)
        }
    } else if s.contains('\n') {
        None
    } else {
        Some(s)
    }
}

fn match_windows_re(s: &str) -> Option<(char, String)> {
    let line = py_regex_line(s)?;
    let bytes = line.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/') {
        Some((bytes[0] as char, line[3..].to_string()))
    } else {
        None
    }
}

fn match_wsl_re(s: &str) -> Option<(char, Option<String>)> {
    let line = py_regex_line(s)?;
    let rest = line.strip_prefix("/mnt/")?;
    let bytes = rest.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    let drive = bytes[0] as char;
    if bytes.len() == 1 {
        Some((drive, None))
    } else if bytes[1] == b'/' {
        Some((drive, Some(rest[2..].to_string())))
    } else {
        None
    }
}

fn normalize_pure_windows_path(drive_upper: char, suffix: &str) -> String {
    let parts: Vec<&str> = suffix
        .split(['\\', '/'])
        .filter(|seg| !seg.is_empty() && *seg != ".")
        .collect();
    if parts.is_empty() {
        format!("{drive_upper}:\\")
    } else {
        format!("{drive_upper}:\\{}", parts.join("\\"))
    }
}

fn map_path(mut value: &str, target: &str) -> (String, Vec<String>) {
    let mut issues = Vec::new();
    if let Some(stripped) = value.strip_prefix("file://") {
        value = stripped;
        let b = value.as_bytes();
        if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
            value = &value[1..];
        }
    }

    let windows = match_windows_re(value);
    let wsl = match_wsl_re(value);
    let effective_target = if target == "auto" {
        if !cfg!(windows) && windows.is_some() {
            "wsl"
        } else {
            "native"
        }
    } else {
        target
    };

    if matches!(effective_target, "native" | "linux") {
        return (value.to_string(), issues);
    }
    if effective_target == "wsl" {
        if let Some((drive, rest)) = windows {
            return (
                format!(
                    "/mnt/{}/{}",
                    drive.to_ascii_lowercase(),
                    rest.replace('\\', "/")
                ),
                issues,
            );
        }
        return (value.to_string(), issues);
    }
    if effective_target == "windows" {
        if let Some((drive, rest)) = wsl {
            let suffix = rest.unwrap_or_default().replace('/', "\\");
            return (
                normalize_pure_windows_path(drive.to_ascii_uppercase(), &suffix),
                issues,
            );
        }
        return (value.to_string(), issues);
    }
    issues.push(format!("unsupported target: {effective_target}"));
    (value.to_string(), issues)
}

fn is_readable_file(path_str: &str) -> bool {
    let p = Path::new(path_str);
    if !p.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        if let Ok(cstr) = std::ffi::CString::new(path_str) {
            return unsafe { access(cstr.as_ptr(), 4) == 0 };
        }
        false
    }
    #[cfg(not(unix))]
    {
        File::open(p).is_ok()
    }
}

fn inspect_path(
    value: &str,
    decode: &str,
    target: &str,
    require_exists: bool,
    require_readable: bool,
    read_bytes: i64,
    show: bool,
) -> BTreeMap<String, OutVal> {
    let (decoded, decoded_once, possible_double, mut issues) = decode_once(value, decode);
    let (mapped, mapping_issues) = map_path(&decoded, target);
    issues.extend(mapping_issues);

    let exists = !mapped.contains('\0') && Path::new(&mapped).exists();
    let readable = !mapped.contains('\0') && is_readable_file(&mapped);
    let mut bytes_read: i64 = 0;

    if require_exists && !exists {
        issues.push("mapped path does not exist".to_string());
    }
    if require_readable && !readable {
        issues.push("mapped path is not a readable file".to_string());
    }
    if read_bytes != 0 {
        if readable {
            match File::open(&mapped) {
                Ok(f) => {
                    let mut buf = Vec::new();
                    match f.take(read_bytes as u64).read_to_end(&mut buf) {
                        Ok(n) => {
                            bytes_read = n as i64;
                            if bytes_read == 0 {
                                issues.push("file read returned zero bytes".to_string());
                            }
                        }
                        Err(e) => {
                            issues.push(format!("file read failed: {}", os_error_class(&e)));
                        }
                    }
                }
                Err(e) => {
                    issues.push(format!("file read failed: {}", os_error_class(&e)));
                }
            }
        } else {
            issues.push("cannot read bytes from an unreadable path".to_string());
        }
    }

    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), OutVal::Bool(issues.is_empty()));
    map.insert("raw".to_string(), OutVal::Str(masked(value, show)));
    map.insert("decoded".to_string(), OutVal::Str(masked(&decoded, show)));
    map.insert("mapped".to_string(), OutVal::Str(masked(&mapped, show)));
    map.insert("decoded_once".to_string(), OutVal::Bool(decoded_once));
    map.insert(
        "possible_double_encoding".to_string(),
        OutVal::Bool(possible_double),
    );
    map.insert("exists".to_string(), OutVal::Bool(exists));
    map.insert("readable".to_string(), OutVal::Bool(readable));
    map.insert("bytes_read".to_string(), OutVal::Int(bytes_read));
    map.insert(
        "issues".to_string(),
        OutVal::List(issues.into_iter().map(OutVal::Str).collect()),
    );
    map
}

fn os_error_class(err: &std::io::Error) -> &'static str {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::NotFound => "FileNotFoundError",
        ErrorKind::PermissionDenied => "PermissionError",
        ErrorKind::AlreadyExists => "FileExistsError",
        ErrorKind::Interrupted => "InterruptedError",
        _ => "OSError",
    }
}

fn command_self_test() -> u8 {
    let mut checks: Vec<(&str, bool)> = Vec::new();
    checks.push((
        "integer",
        validate_value(&JsonVal::Int("17".to_string()), "integer").0,
    ));
    checks.push((
        "numeric string rejected",
        !validate_value(&JsonVal::Str("17".to_string()), "integer").0,
    ));
    checks.push((
        "error tuple rejected",
        !validate_value(
            &JsonVal::List(vec![
                JsonVal::Str("E".to_string()),
                JsonVal::Str("failure".to_string()),
            ]),
            "array",
        )
        .0,
    ));
    let opaque = "a34721a6-a405-42d9-9380-cb5bcf05bbfc";
    checks.push((
        "opaque string preserved",
        validate_value(&JsonVal::Str(opaque.to_string()), "opaque-string").0,
    ));
    let vars_one = vec![("name".to_string(), JsonVal::Str("장면 1".to_string()))];
    checks.push((
        "template applied",
        matches!(
            inspect_template("결과/{{name}}.png", "결과/장면 1.png", Some(&vars_one)).get("ok"),
            Some(OutVal::Bool(true))
        ),
    ));
    checks.push((
        "template unresolved",
        !matches!(
            inspect_template("결과/{{name}}.png", "결과/{{name}}.png", Some(&vars_one)).get("ok"),
            Some(OutVal::Bool(true))
        ),
    ));
    let vars_two = vec![
        ("dir".to_string(), JsonVal::Str("결과".to_string())),
        ("name".to_string(), JsonVal::Str("장면 1".to_string())),
    ];
    checks.push((
        "template partial",
        !matches!(
            inspect_template("{{dir}}/{{name}}.png", "결과/{{name}}.png", Some(&vars_two)).get("ok"),
            Some(OutVal::Bool(true))
        ),
    ));
    checks.push((
        "malformed percent rejected",
        !matches!(
            inspect_path("C:%ZZ\\x", "auto", "wsl", false, false, 0, true).get("ok"),
            Some(OutVal::Bool(true))
        ),
    ));
    let double = inspect_path("C:%252Ftemp", "once", "wsl", false, false, 0, true);
    let double_ok = matches!(
        double.get("possible_double_encoding"),
        Some(OutVal::Bool(true))
    ) && matches!(double.get("ok"), Some(OutVal::Bool(false)));
    checks.push(("double encoding reported", double_ok));
    let (plus, _, _, _) = decode_once("장면+1%20완료", "once");
    checks.push(("plus preserved", plus == "장면+1 완료"));
    let (mapped_wsl, _) = map_path("D:\\한글 폴더\\장면 1.png", "wsl");
    checks.push(("windows to WSL", mapped_wsl == "/mnt/d/한글 폴더/장면 1.png"));
    let (mapped_windows, _) = map_path("/mnt/d/한글 폴더/장면 1.png", "windows");
    checks.push((
        "WSL to windows",
        mapped_windows == "D:\\한글 폴더\\장면 1.png",
    ));

    let temp_dir = env::temp_dir().join(format!("check-correct-path-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let unicode_passed = if fs::create_dir_all(&temp_dir).is_ok() {
        let file_path = temp_dir.join("한글 장면.png");
        let _ = fs::write(&file_path, b"image-bytes");
        let encoded = url_quote(&file_path.to_string_lossy(), b"/");
        let report = inspect_path(&encoded, "once", "linux", true, true, 1, false);
        let ok = matches!(report.get("ok"), Some(OutVal::Bool(true)))
            && matches!(report.get("bytes_read"), Some(OutVal::Int(1)));
        let _ = fs::remove_dir_all(&temp_dir);
        ok
    } else {
        false
    };
    checks.push(("unicode file byte read", unicode_passed));

    let failures: Vec<OutVal> = checks
        .iter()
        .filter(|(_, passed)| !*passed)
        .map(|(name, _)| OutVal::Str((*name).to_string()))
        .collect();
    let check_items: Vec<OutVal> = checks
        .iter()
        .map(|(name, passed)| {
            let mut m = BTreeMap::new();
            m.insert("name".to_string(), OutVal::Str((*name).to_string()));
            m.insert("passed".to_string(), OutVal::Bool(*passed));
            OutVal::Map(m)
        })
        .collect();
    let mut out = BTreeMap::new();
    out.insert("ok".to_string(), OutVal::Bool(failures.is_empty()));
    out.insert("checks".to_string(), OutVal::List(check_items));
    out.insert("failures".to_string(), OutVal::List(failures));
    emit(OutVal::Map(out))
}

#[derive(Default)]
struct PathArgs {
    decode: String,
    target: String,
    require_exists: bool,
    require_readable: bool,
    read_bytes: i64,
    show_value: bool,
}

impl PathArgs {
    fn new() -> Self {
        Self {
            decode: "auto".to_string(),
            target: "auto".to_string(),
            require_exists: false,
            require_readable: false,
            read_bytes: 0,
            show_value: false,
        }
    }
}

enum CliError {
    Help,
    Argparse(String),
    Usage(String),
}

fn next_opt_val(
    args: &[String],
    i: &mut usize,
    flag: &str,
    inline: Option<&str>,
) -> Result<String, CliError> {
    if let Some(v) = inline {
        return Ok(v.to_string());
    }
    *i += 1;
    if *i >= args.len() {
        return Err(CliError::Argparse(format!(
            "argument {flag}: expected one argument"
        )));
    }
    Ok(args[*i].clone())
}

fn check_choice(flag: &str, val: &str, choices: &[&str]) -> Result<(), CliError> {
    if choices.contains(&val) {
        Ok(())
    } else {
        Err(CliError::Argparse(format!(
            "argument {flag}: invalid choice: '{val}'"
        )))
    }
}

fn parse_path_flag(
    args: &[String],
    i: &mut usize,
    name: &str,
    inline: Option<&str>,
    pa: &mut PathArgs,
) -> Result<bool, CliError> {
    match name {
        "--decode" => {
            let v = next_opt_val(args, i, "--decode", inline)?;
            check_choice("--decode", &v, &["auto", "never", "once"])?;
            pa.decode = v;
            Ok(true)
        }
        "--target" => {
            let v = next_opt_val(args, i, "--target", inline)?;
            check_choice("--target", &v, &["auto", "native", "linux", "windows", "wsl"])?;
            pa.target = v;
            Ok(true)
        }
        "--require-exists" => {
            if inline.is_some() {
                return Err(CliError::Argparse(
                    "argument --require-exists: ignored explicit argument".to_string(),
                ));
            }
            pa.require_exists = true;
            Ok(true)
        }
        "--require-readable" => {
            if inline.is_some() {
                return Err(CliError::Argparse(
                    "argument --require-readable: ignored explicit argument".to_string(),
                ));
            }
            pa.require_readable = true;
            Ok(true)
        }
        "--read-bytes" => {
            let v = next_opt_val(args, i, "--read-bytes", inline)?;
            let parsed = parse_py_int(&v).ok_or_else(|| {
                CliError::Argparse(format!("argument --read-bytes: invalid int value: '{v}'"))
            })?;
            pa.read_bytes = parsed;
            Ok(true)
        }
        "--show-value" => {
            if inline.is_some() {
                return Err(CliError::Argparse(
                    "argument --show-value: ignored explicit argument".to_string(),
                ));
            }
            pa.show_value = true;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn split_flag(arg: &str) -> (&str, Option<&str>) {
    if arg.starts_with("--") {
        if let Some((k, v)) = arg.split_once('=') {
            return (k, Some(v));
        }
    }
    (arg, None)
}

fn run_cli(args: &[String]) -> Result<u8, CliError> {
    if args.is_empty() {
        return Err(CliError::Argparse(
            "the following arguments are required: command".to_string(),
        ));
    }
    let sub = args[0].as_str();
    if matches!(sub, "-h" | "--help") {
        return Err(CliError::Help);
    }
    match sub {
        "self-test" => {
            for a in &args[1..] {
                if matches!(a.as_str(), "-h" | "--help") {
                    return Err(CliError::Help);
                }
                return Err(CliError::Argparse(format!("unrecognized arguments: {a}")));
            }
            Ok(command_self_test())
        }
        "value" => {
            let mut value_json: Option<String> = None;
            let mut expected_type: Option<String> = None;
            let mut expect: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                if matches!(args[i].as_str(), "-h" | "--help") {
                    return Err(CliError::Help);
                }
                let (name, inline) = split_flag(&args[i]);
                match name {
                    "--value-json" => {
                        value_json = Some(next_opt_val(args, &mut i, "--value-json", inline)?);
                    }
                    "--expected-type" => {
                        let v = next_opt_val(args, &mut i, "--expected-type", inline)?;
                        check_choice(
                            "--expected-type",
                            &v,
                            &["integer", "opaque-string", "path", "object", "array"],
                        )?;
                        expected_type = Some(v);
                    }
                    "--expect" => {
                        let v = next_opt_val(args, &mut i, "--expect", inline)?;
                        check_choice("--expect", &v, &["valid", "invalid"])?;
                        expect = Some(v);
                    }
                    _ => {
                        return Err(CliError::Argparse(format!(
                            "unrecognized arguments: {}",
                            args[i]
                        )))
                    }
                }
                i += 1;
            }
            let vj = value_json.ok_or_else(|| {
                CliError::Argparse("the following arguments are required: --value-json".to_string())
            })?;
            let et = expected_type.ok_or_else(|| {
                CliError::Argparse(
                    "the following arguments are required: --expected-type".to_string(),
                )
            })?;
            let ex = expect.ok_or_else(|| {
                CliError::Argparse("the following arguments are required: --expect".to_string())
            })?;
            let val = parse_json(&vj, "value").map_err(CliError::Usage)?;
            let (valid, issues) = validate_value(&val, &et);
            let expectation_met = valid == (ex == "valid");
            let mut out = BTreeMap::new();
            out.insert("ok".to_string(), OutVal::Bool(expectation_met));
            out.insert("actual_valid".to_string(), OutVal::Bool(valid));
            out.insert("expected".to_string(), OutVal::Str(ex));
            out.insert(
                "received_type".to_string(),
                OutVal::Str(val.py_type_name().to_string()),
            );
            out.insert(
                "issues".to_string(),
                OutVal::List(issues.into_iter().map(OutVal::Str).collect()),
            );
            Ok(emit(OutVal::Map(out)))
        }
        "template" => {
            let mut source: Option<String> = None;
            let mut rendered: Option<String> = None;
            let mut variables_json: Option<String> = None;
            let mut expect_applied: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                if matches!(args[i].as_str(), "-h" | "--help") {
                    return Err(CliError::Help);
                }
                let (name, inline) = split_flag(&args[i]);
                match name {
                    "--source" => {
                        source = Some(next_opt_val(args, &mut i, "--source", inline)?);
                    }
                    "--rendered" => {
                        rendered = Some(next_opt_val(args, &mut i, "--rendered", inline)?);
                    }
                    "--variables-json" => {
                        variables_json =
                            Some(next_opt_val(args, &mut i, "--variables-json", inline)?);
                    }
                    "--expect-applied" => {
                        let v = next_opt_val(args, &mut i, "--expect-applied", inline)?;
                        check_choice("--expect-applied", &v, &["yes", "no"])?;
                        expect_applied = Some(v);
                    }
                    _ => {
                        return Err(CliError::Argparse(format!(
                            "unrecognized arguments: {}",
                            args[i]
                        )))
                    }
                }
                i += 1;
            }
            let src = source.ok_or_else(|| {
                CliError::Argparse("the following arguments are required: --source".to_string())
            })?;
            let rnd = rendered.ok_or_else(|| {
                CliError::Argparse("the following arguments are required: --rendered".to_string())
            })?;
            let ea = expect_applied.ok_or_else(|| {
                CliError::Argparse(
                    "the following arguments are required: --expect-applied".to_string(),
                )
            })?;
            let vars = if let Some(vj) = variables_json {
                let parsed = parse_json(&vj, "variables").map_err(CliError::Usage)?;
                match parsed {
                    JsonVal::Dict(entries) => Some(entries),
                    _ => return Err(CliError::Usage("variables JSON must be an object".to_string())),
                }
            } else {
                None
            };
            let mut result = inspect_template(&src, &rnd, vars.as_deref());
            let expected = ea == "yes";
            let applied = matches!(result.get("applied"), Some(OutVal::Bool(true)));
            let diag_ok = matches!(result.get("ok"), Some(OutVal::Bool(true)));
            result.insert("actual_applied".to_string(), OutVal::Bool(applied));
            result.insert("expected_applied".to_string(), OutVal::Bool(expected));
            result.insert("diagnostic_ok".to_string(), OutVal::Bool(diag_ok));
            result.insert("ok".to_string(), OutVal::Bool(applied == expected));
            Ok(emit(OutVal::Map(result)))
        }
        "path" => {
            let mut value: Option<String> = None;
            let mut pa = PathArgs::new();
            let mut i = 1;
            while i < args.len() {
                if matches!(args[i].as_str(), "-h" | "--help") {
                    return Err(CliError::Help);
                }
                let (name, inline) = split_flag(&args[i]);
                if name == "--value" {
                    value = Some(next_opt_val(args, &mut i, "--value", inline)?);
                } else if !parse_path_flag(args, &mut i, name, inline, &mut pa)? {
                    return Err(CliError::Argparse(format!(
                        "unrecognized arguments: {}",
                        args[i]
                    )));
                }
                i += 1;
            }
            let val = value.ok_or_else(|| {
                CliError::Argparse("the following arguments are required: --value".to_string())
            })?;
            if pa.read_bytes < 0 {
                return Err(CliError::Usage("read-bytes must be non-negative".to_string()));
            }
            let report = inspect_path(
                &val,
                &pa.decode,
                &pa.target,
                pa.require_exists,
                pa.require_readable,
                pa.read_bytes,
                pa.show_value,
            );
            Ok(emit(OutVal::Map(report)))
        }
        "pipeline" => {
            let mut template: Option<String> = None;
            let mut rendered: Option<String> = None;
            let mut variables_json: Option<String> = None;
            let mut pa = PathArgs::new();
            let mut i = 1;
            while i < args.len() {
                if matches!(args[i].as_str(), "-h" | "--help") {
                    return Err(CliError::Help);
                }
                let (name, inline) = split_flag(&args[i]);
                match name {
                    "--template" => {
                        template = Some(next_opt_val(args, &mut i, "--template", inline)?);
                    }
                    "--rendered" => {
                        rendered = Some(next_opt_val(args, &mut i, "--rendered", inline)?);
                    }
                    "--variables-json" => {
                        variables_json =
                            Some(next_opt_val(args, &mut i, "--variables-json", inline)?);
                    }
                    _ => {
                        if !parse_path_flag(args, &mut i, name, inline, &mut pa)? {
                            return Err(CliError::Argparse(format!(
                                "unrecognized arguments: {}",
                                args[i]
                            )));
                        }
                    }
                }
                i += 1;
            }
            let tpl = template.ok_or_else(|| {
                CliError::Argparse("the following arguments are required: --template".to_string())
            })?;
            let rnd = rendered.ok_or_else(|| {
                CliError::Argparse("the following arguments are required: --rendered".to_string())
            })?;
            if pa.read_bytes < 0 {
                return Err(CliError::Usage("read-bytes must be non-negative".to_string()));
            }
            let vars = if let Some(vj) = variables_json {
                let parsed = parse_json(&vj, "variables").map_err(CliError::Usage)?;
                match parsed {
                    JsonVal::Dict(entries) => Some(entries),
                    _ => return Err(CliError::Usage("variables JSON must be an object".to_string())),
                }
            } else {
                None
            };
            let tpl_report = inspect_template(&tpl, &rnd, vars.as_deref());
            let path_report = inspect_path(
                &rnd,
                &pa.decode,
                &pa.target,
                pa.require_exists,
                pa.require_readable,
                pa.read_bytes,
                pa.show_value,
            );
            let tpl_ok = matches!(tpl_report.get("ok"), Some(OutVal::Bool(true)));
            let path_ok = matches!(path_report.get("ok"), Some(OutVal::Bool(true)));
            let first_failure = if !tpl_ok {
                OutVal::Str("template".to_string())
            } else if !path_ok {
                OutVal::Str("path".to_string())
            } else {
                OutVal::Null
            };
            let ok = matches!(first_failure, OutVal::Null);
            let mut out = BTreeMap::new();
            out.insert("ok".to_string(), OutVal::Bool(ok));
            out.insert("first_failure".to_string(), first_failure);
            out.insert("template".to_string(), OutVal::Map(tpl_report));
            out.insert("path".to_string(), OutVal::Map(path_report));
            Ok(emit(OutVal::Map(out)))
        }
        other => Err(CliError::Argparse(format!(
            "argument command: invalid choice: '{other}'"
        ))),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run_cli(&args) {
        Ok(code) => ExitCode::from(code),
        Err(CliError::Help) => {
            println!("usage: check-correct-path {{self-test,value,template,path,pipeline}} ...");
            ExitCode::from(0)
        }
        Err(CliError::Argparse(msg)) => {
            eprintln!("usage: check-correct-path [-h] {{self-test,value,template,path,pipeline}} ...\ncheck-correct-path: error: {msg}");
            ExitCode::from(2)
        }
        Err(CliError::Usage(msg)) => {
            eprintln!(
                "{{\"ok\": false, \"usage_error\": {}}}",
                json_escape_str(&msg)
            );
            ExitCode::from(2)
        }
    }
}
