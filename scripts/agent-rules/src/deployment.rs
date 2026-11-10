use crate::rules::{Error, Paths, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn err(message: impl Into<String>) -> Error { Error { code: 1, msg: message.into() } }
fn manifest(p: &Paths) -> Result<Option<Value>> {
    let path = p.home.join("ai/codex/hook-manifest.json");
    if !path.exists() { return Ok(None); }
    let bytes = fs::read(&path).map_err(|e| err(format!("{}: {e}", path.display())))?;
    serde_json::from_slice(&bytes).map(Some).map_err(|e| err(format!("{}: {e}", path.display())))
}

fn hooks(source: &Value, target: &str) -> Result<Value> {
    let mut events = Map::new();
    for item in source["hooks"].as_array().ok_or_else(|| err("manifest hooks 배열 없음"))? {
        let Some(spec) = item["targets"].get(target) else { continue };
        let event = spec["event"].as_str().or_else(|| item["event"].as_str()).ok_or_else(|| err("hook event 없음"))?;
        let matcher = spec["matcher"].as_str().or_else(|| item["matcher"].as_str());
        let id = item["id"].as_str().ok_or_else(|| err("hook id 없음"))?;
        let raw = item["command"].as_str().ok_or_else(|| err("hook command 없음"))?;
        let command = if target == "claude" || target == "codex" {
            format!("{}/ai/scripts/bin/{raw}", "~")
        } else {
            format!("~/ai/scripts/bin/agy-hook manifest {target} {id}")
        };
        let timeout = item["timeout"].as_u64().unwrap_or(20);
        let timeout = if target == "gemini" { timeout * 1000 } else { timeout };
        let mut hook = json!({"type":"command","command":command,"timeout":timeout});
        if target == "gemini" { hook["name"] = json!(format!("ai-{id}")); }
        let mut row = if target == "agy" && matches!(event, "SessionStart" | "PreInvocation" | "Stop") {
            hook
        } else { json!({"hooks":[hook]}) };
        if let Some(m) = matcher { row["matcher"] = json!(m); }
        events.entry(event.to_string()).or_insert_with(|| json!([])).as_array_mut().unwrap().push(row);
    }
    Ok(Value::Object(events))
}

fn hook_targets(p: &Paths, manifest: &Value) -> Result<Vec<(String, PathBuf, Value, bool)>> {
    let mut out = Vec::new();
    for (name, rel, merged) in [
        ("claude-hooks", ".claude/settings.json", true),
        ("codex-hooks", "ai/codex/hooks.json", false),
        ("gemini-hooks", ".gemini/settings.json", true),
        ("antigravity-cli-hooks", ".gemini/antigravity-cli/settings.json", true),
        ("agy-hooks", "ai/codex/hooks.agy.json", false),
    ] {
        let agent = match name {
            "claude-hooks" => "claude", "codex-hooks" => "codex", "agy-hooks" => "agy", _ => "gemini",
        };
        let mut value = if name == "agy-hooks" { json!({"extensions-gates":hooks(manifest, agent)?}) }
            else { json!({"hooks":hooks(manifest, agent)?}) };
        let path = p.home.join(rel);
        if merged && path.exists() {
            let existing: Value = serde_json::from_slice(&fs::read(&path).map_err(|e| err(e.to_string()))?)
                .map_err(|e| err(format!("{}: {e}", path.display())))?;
            let key = "hooks";
            let mut existing = existing.as_object().cloned().ok_or_else(|| err(format!("{}: JSON object 아님", path.display())))?;
            existing.insert(key.into(), value[key].take());
            value = Value::Object(existing);
        }
        out.push((name.into(), path, value, merged));
    }
    Ok(out)
}

fn backup(p: &Paths, name: &str, path: &Path) -> Result<()> {
    if !path.exists() { return Ok(()); }
    let dir = p.state.join("backup");
    fs::create_dir_all(&dir).map_err(|e| err(e.to_string()))?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |x| x.as_nanos());
    let safe = name.replace('/', "-");
    fs::copy(path, dir.join(format!("{safe}-{stamp}.bak"))).map_err(|e| err(format!("{}: {e}", path.display())))?;
    Ok(())
}

fn skill_dirs(p: &Paths) -> Result<Vec<String>> {
    let dir = p.home.join("ai/skills");
    if !dir.exists() { return Ok(vec![]); }
    Ok(fs::read_dir(dir).map_err(|e| err(e.to_string()))?.filter_map(|e| e.ok())
        .filter(|e| e.path().join("SKILL.md").is_file())
        .filter_map(|e| e.file_name().into_string().ok()).collect())
}

fn binaries(p: &Paths) -> Result<Vec<String>> {
    let path = p.home.join("ai/scripts/Cargo.toml");
    if !path.exists() { return Ok(vec![]); }
    let text = fs::read_to_string(path).map_err(|e| err(e.to_string()))?;
    let start = text.find("members = [").ok_or_else(|| err("workspace members 없음"))?;
    let end = text[start..].find(']').ok_or_else(|| err("workspace members 닫기 없음"))? + start;
    Ok(text[start..end].split('"').skip(1).step_by(2).map(str::to_string).collect())
}

fn newest(path: &Path) -> std::io::Result<SystemTime> {
    let mut result = fs::metadata(path)?.modified()?;
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let next = newest(&entry.path())?;
            if next > result { result = next; }
        }
    }
    Ok(result)
}

fn source_newer(base: &Path, name: &str, release: &Path) -> bool {
    let Ok(built) = fs::metadata(release).and_then(|m| m.modified()) else { return true };
    [base.join(name).join("src"), base.join(name).join("Cargo.toml")]
        .iter().filter_map(|p| newest(p).ok()).any(|modified| modified > built)
}

pub fn unsupported(p: &Paths) -> Result<Vec<String>> {
    let Some(source) = manifest(p)? else { return Ok(Vec::new()) };
    Ok(source["unsupported"].as_array().into_iter().flatten().map(|row| {
        format!("지원 안 됨: {} {} — {}", row["target"].as_str().unwrap_or("?"), row["event"].as_str().unwrap_or("?"), row["reason"].as_str().unwrap_or("?"))
    }).collect())
}

pub fn inspect(p: &Paths) -> Result<Vec<String>> {
    let mut drift = Vec::new();
    if let Some(source) = manifest(p)? {
        for (name, path, want, _) in hook_targets(p, &source)? {
            let have = fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
            if have.as_ref() != Some(&want) { drift.push(format!("{name}: {}", path.display())); }
        }
    } else if p.home.join("ai/scripts/Cargo.toml").exists() {
        drift.push(format!("hook 원본 없음: {}", p.home.join("ai/codex/hook-manifest.json").display()));
    }
    for name in skill_dirs(p)? {
        let src = p.home.join("ai/skills").join(&name);
        for rel in [".claude/skills", ".codex/skills", ".gemini/config/skills"] {
            let link = p.home.join(rel).join(&name);
            if fs::read_link(&link).ok().as_ref() != Some(&src) { drift.push(format!("Skill 링크: {}", link.display())); }
        }
    }
    for rel in [".claude/skills", ".codex/skills", ".gemini/config/skills"] {
        let dir = p.home.join(rel);
        if !dir.exists() { continue; }
        for entry in fs::read_dir(&dir).map_err(|e| err(e.to_string()))? {
            let entry = entry.map_err(|e| err(e.to_string()))?;
            let name = entry.file_name();
            if name == ".system" || name == "synced" { continue; }
            let path = entry.path();
            if !fs::symlink_metadata(&path).map_err(|e| err(e.to_string()))?.file_type().is_symlink() {
                println!("사용자 Skill 보존 (링크 아님): {}", path.display());
            }
        }
    }
    for name in binaries(p)? {
        let base = p.home.join("ai/scripts");
        let release = base.join("target/release").join(&name);
        let link = base.join("bin").join(&name);
        if source_newer(&base, &name, &release) {
            drift.push(format!("release 바이너리: {}", release.display()));
        }
        if fs::read_link(&link).ok().as_ref() != Some(&PathBuf::from(format!("../target/release/{name}"))) {
            drift.push(format!("바이너리 링크: {}", link.display()));
        }
    }
    Ok(drift)
}

pub fn sync(p: &Paths) -> Result<()> {
    if let Some(source) = manifest(p)? {
        for (name, path, value, _) in hook_targets(p, &source)? {
            let have = fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
            if have.as_ref() == Some(&value) { continue; }
            backup(p, &name, &path)?;
            fs::create_dir_all(path.parent().unwrap()).map_err(|e| err(e.to_string()))?;
            fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&value).map_err(|e| err(e.to_string()))?))
                .map_err(|e| err(format!("{}: {e}", path.display())))?;
        }
    }
    for name in skill_dirs(p)? {
        let src = p.home.join("ai/skills").join(&name);
        for rel in [".claude/skills", ".codex/skills", ".gemini/config/skills"] {
            let link = p.home.join(rel).join(&name);
            if fs::read_link(&link).ok().as_ref() == Some(&src) { continue; }
            if let Ok(meta) = fs::symlink_metadata(&link) {
                if !meta.file_type().is_symlink() { continue; }
                let old = fs::read_link(&link).map_err(|e| err(e.to_string()))?;
                let dir = p.state.join("backup");
                fs::create_dir_all(&dir).map_err(|e| err(e.to_string()))?;
                fs::write(dir.join(format!("skill-{name}-{}.link", rel.replace('/', "-"))), old.to_string_lossy().as_bytes()).map_err(|e| err(e.to_string()))?;
                fs::remove_file(&link).map_err(|e| err(e.to_string()))?;
            }
            fs::create_dir_all(link.parent().unwrap()).map_err(|e| err(e.to_string()))?;
            std::os::unix::fs::symlink(&src, &link).map_err(|e| err(format!("{}: {e}", link.display())))?;
        }
    }
    let names = binaries(p)?;
    if names.iter().any(|n| {
        let base = p.home.join("ai/scripts");
        let release = base.join("target/release").join(n);
        source_newer(&base, n, &release)
    }) {
        let status = Command::new("cargo").args(["build", "--manifest-path"]).arg(p.home.join("ai/scripts/Cargo.toml"))
            .args(["--workspace", "--release"]).status().map_err(|e| err(format!("cargo build: {e}")))?;
        if !status.success() { return Err(err(format!("cargo build 실패: {status}"))); }
    }
    for name in names {
        let link = p.home.join("ai/scripts/bin").join(&name);
        let want = PathBuf::from(format!("../target/release/{name}"));
        if fs::read_link(&link).ok().as_ref() == Some(&want) { continue; }
        if fs::symlink_metadata(&link).is_ok() {
            backup(p, &format!("bin-{name}"), &link)?;
            fs::remove_file(&link).map_err(|e| err(e.to_string()))?;
        }
        fs::create_dir_all(link.parent().unwrap()).map_err(|e| err(e.to_string()))?;
        std::os::unix::fs::symlink(want, link).map_err(|e| err(e.to_string()))?;
    }
    Ok(())
}
