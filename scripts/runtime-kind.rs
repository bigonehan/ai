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
                if ["blender", "eagle", "resolve", "obsidian", "tauri", "데스크톱 앱", "comfyui"]
                    .iter().any(|word| value.contains(word)) { return Ok(RuntimeKind::App); }
                if value.contains("http://") || value.contains("https://") || value.contains("웹")
                    || value.contains("위젯") || value.contains("chrome") { return Ok(RuntimeKind::Web); }
                if value.contains("플러그인") { return Ok(RuntimeKind::App); }
                break;
            }
        }
    }
    Err("AGENTS.md # 상태 > ### 실행의 runtime : 종류를 판정할 수 없어요".into())
}

#[cfg(test)]
mod tests {
    use super::{runtime_kind, RuntimeKind};
    use std::fs;

    #[test]
    fn blender_with_web_screen_uses_desktop_runtime() {
        let project = std::env::temp_dir().join(format!("runtime-kind-{}", std::process::id()));
        fs::create_dir_all(&project).unwrap();
        let agents = project.join("AGENTS.md");
        fs::write(&agents, "# 정보\n### 실행\nruntime : 웹\n# 상태\n### 실행\nruntime : 폰 웹 화면 web/index.html / 설치본 blender_camera_control ZIP / QR 접속 후 Tap to start 탭 / Blender 선택 카메라 readback\n").unwrap();
        assert_eq!(runtime_kind(&project).unwrap(), RuntimeKind::App);
        fs::write(&agents, "# 상태\n### 실행\nruntime : 없음(로컬 CLI) / ~/ai/scripts/bin/grist / Codex hook\n").unwrap();
        assert_eq!(runtime_kind(&project).unwrap(), RuntimeKind::Cli);
        fs::remove_file(agents).unwrap();
        fs::remove_dir(project).unwrap();
    }
}
