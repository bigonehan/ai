use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeKind { Web, App, Cli }

pub fn runtime_kind(project: &Path) -> Result<RuntimeKind, String> {
    let agents = fs::read_to_string(project.join("AGENTS.md"))
        .map_err(|error| format!("AGENTS.md runtime 읽기 실패: {error}"))?;
    let mut status = false;
    let mut execution = false;
    for line in agents.lines() {
        if line.starts_with("# ") { status = line == "# 상태"; execution = false; }
        else if status && line.starts_with("### ") { execution = line == "### 실행"; }
        if status && execution {
            if let Some(value) = line.strip_prefix("runtime :") {
                let value = value.trim().to_lowercase();
                if value.is_empty() { break; }
                if value.contains("없음(로컬 cli)") || value.contains("local cli")
                    || value.starts_with("cli ") || value.starts_with("~/ai/scripts/bin/")
                    || value.starts_with("npm test") { return Ok(RuntimeKind::Cli); }
                if value.contains("http://") || value.contains("https://") || value.contains("웹")
                    || value.contains("위젯") || value.contains("chrome") { return Ok(RuntimeKind::Web); }
                if ["blender", "eagle", "resolve", "obsidian", "tauri", "데스크톱 앱", "comfyui", "플러그인"]
                    .iter().any(|word| value.contains(word)) { return Ok(RuntimeKind::App); }
                break;
            }
        }
    }
    Err("AGENTS.md # 상태 > ### 실행의 runtime : 종류를 판정할 수 없어요".into())
}
