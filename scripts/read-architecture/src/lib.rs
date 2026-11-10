use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct StructureDifference {
    pub documented_only: Vec<String>,
    pub undocumented: Vec<String>,
}

impl StructureDifference {
    pub fn is_empty(&self) -> bool {
        self.documented_only.is_empty() && self.undocumented.is_empty()
    }
}

pub fn structure_part(doc: &str) -> &str {
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

pub fn folder_labels(structure: &str) -> Vec<String> {
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

pub fn structure_difference(project: &Path, doc: &str) -> Result<StructureDifference, String> {
    let named = folder_labels(structure_part(doc));
    let documented_only = named
        .iter()
        .filter(|name| !project.join(name).exists())
        .cloned()
        .collect();
    let named_top: HashSet<&str> = named
        .iter()
        .map(|name| name.split('/').next().unwrap())
        .collect();
    let mut undocumented = Vec::new();
    for entry in fs::read_dir(project).map_err(|e| format!("project 읽기 실패: {e}"))? {
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
            undocumented.push(name);
        }
    }
    undocumented.sort();
    Ok(StructureDifference { documented_only, undocumented })
}
