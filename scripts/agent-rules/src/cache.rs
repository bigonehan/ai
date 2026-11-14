use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

const MAX_AGE: Duration = Duration::from_secs(5 * 24 * 60 * 60);
const MIN_UNITS: usize = 3;

pub fn build_active() -> bool {
    ["cargo", "rustc", "clippy-driver"]
        .iter()
        .any(|name| Command::new("pgrep").args(["-x", name]).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success()))
}

pub fn prune(home: &Path, now: SystemTime) -> std::io::Result<(u64, u64)> {
    let mut roots = Vec::new();
    for base in [home.join("ai/scripts"), home.join("ai/skills")] {
        discover_cargo_targets(&base, &mut roots)?;
    }
    let mut removed = 0;
    let mut bytes = 0;
    for root in roots {
        for profile in [root.join("debug"), root.join("release")] {
            prune_dir(&profile.join("incremental"), now, &mut removed, &mut bytes)?;
            if profile.file_name().is_some_and(|name| name == "debug") {
                prune_debug(&profile, now, &mut removed, &mut bytes)?;
            }
        }
    }
    Ok((removed, bytes))
}

fn discover_cargo_targets(base: &Path, roots: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !base.is_dir() { return Ok(()); }
    let mut dirs = vec![base.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        if dir.join("Cargo.toml").is_file() { roots.push(dir.join("target")); }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() { continue; }
            let name = entry.file_name();
            if name == "target" || name == ".git" || name == "node_modules" { continue; }
            dirs.push(entry.path());
        }
    }
    Ok(())
}

struct Unit {
    paths: Vec<PathBuf>,
    modified: SystemTime,
    bytes: u64,
}

fn file_stats(path: &Path) -> std::io::Result<(Option<SystemTime>, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Ok((metadata.modified().ok(), if metadata.is_file() { metadata.len() } else { 0 }));
    }
    let mut newest = None;
    let mut bytes = 0;
    for entry in fs::read_dir(path)? {
        let (modified, size) = file_stats(&entry?.path())?;
        if let Some(modified) = modified {
            newest = Some(newest.map_or(modified, |old: SystemTime| old.max(modified)));
        }
        bytes += size;
    }
    Ok((newest.or_else(|| metadata.modified().ok()), bytes))
}

fn unit(paths: Vec<PathBuf>) -> std::io::Result<Unit> {
    let mut modified = SystemTime::UNIX_EPOCH;
    let mut bytes = 0;
    for path in &paths {
        let (time, size) = file_stats(path)?;
        if let Some(time) = time { modified = modified.max(time); }
        bytes += size;
    }
    Ok(Unit { paths, modified, bytes })
}

fn remove_old(groups: BTreeMap<String, Vec<Unit>>, now: SystemTime, removed: &mut u64, bytes: &mut u64) -> std::io::Result<()> {
    for (_, mut units) in groups {
        if units.len() < MIN_UNITS { continue; }
        units.sort_by_key(|unit| unit.modified);
        let latest = units.len() - 1;
        for old in units.into_iter().take(latest) {
            if !now.duration_since(old.modified).ok().is_some_and(|age| age > MAX_AGE) { continue; }
            for path in &old.paths {
                if fs::symlink_metadata(path)?.is_dir() { fs::remove_dir_all(path)?; }
                else { fs::remove_file(path)?; }
            }
            *removed += 1;
            *bytes += old.bytes;
        }
    }
    Ok(())
}

fn prune_dir(dir: &Path, now: SystemTime, removed: &mut u64, bytes: &mut u64) -> std::io::Result<()> {
    if !dir.is_dir() { return Ok(()); }
    for crate_entry in fs::read_dir(dir)? {
        let crate_entry = crate_entry?;
        if !crate_entry.file_type()?.is_dir() { continue; }
        let crate_dir = crate_entry.path();
        let mut groups = BTreeMap::new();
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&crate_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() || !entry.file_name().to_string_lossy().starts_with("s-") { continue; }
            let session = entry.path();
            let mut paths = vec![session.clone()];
            // rustc names the lock after the session's first three parts: `s-<a>-<b>.lock` for `s-<a>-<b>-<suffix>`.
            let stem = entry.file_name().to_string_lossy().split('-').take(3).collect::<Vec<_>>().join("-");
            let lock = crate_dir.join(format!("{stem}.lock"));
            if lock.is_file() { paths.push(lock); }
            let mut session_unit = unit(paths)?;
            session_unit.modified = file_stats(&session)?.0.unwrap_or(SystemTime::UNIX_EPOCH);
            sessions.push(session_unit);
        }
        groups.insert(crate_entry.file_name().to_string_lossy().into_owned(), sessions);
        remove_old(groups, now, removed, bytes)?;
        if fs::read_dir(&crate_dir)?.next().is_none() { fs::remove_dir(crate_dir)?; }
    }
    Ok(())
}

fn cache_key(name: &str, deps: bool) -> Option<(String, String)> {
    let stem = if deps {
        let library = name.ends_with(".rlib") || name.ends_with(".rmeta") || name.ends_with(".so");
        let stem = name.strip_suffix(".rlib").or_else(|| name.strip_suffix(".rmeta"))
            .or_else(|| name.strip_suffix(".so")).or_else(|| name.strip_suffix(".d"))
            .unwrap_or(name);
        if library { stem.strip_prefix("lib").unwrap_or(stem) } else { stem }
    } else { name };
    let (crate_name, hash) = stem.rsplit_once('-')?;
    if crate_name.is_empty() || hash.len() != 16 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) { return None; }
    Some((crate_name.to_owned(), format!("{crate_name}-{hash}")))
}

fn prune_debug(profile: &Path, now: SystemTime, removed: &mut u64, bytes: &mut u64) -> std::io::Result<()> {
    let mut by_key: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut names = BTreeMap::new();
    for location in ["deps", ".fingerprint", "build"] {
        let dir = profile.join(location);
        if !dir.is_dir() { continue; }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if location != "deps" && !entry.file_type()?.is_dir() { continue; }
            let name = entry.file_name();
            if let Some((crate_name, key)) = cache_key(&name.to_string_lossy(), location == "deps") {
                by_key.entry(key.clone()).or_default().push(entry.path());
                names.insert(key, crate_name);
            }
        }
    }
    let mut groups: BTreeMap<String, Vec<Unit>> = BTreeMap::new();
    for (key, paths) in by_key {
        groups.entry(names.remove(&key).unwrap()).or_default().push(unit(paths)?);
    }
    remove_old(groups, now, removed, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_time(path: &Path, time: SystemTime) {
        fs::File::options().write(true).open(path).unwrap()
            .set_times(fs::FileTimes::new().set_modified(time)).unwrap();
    }

    #[test]
    fn prunes_old_cache_files_and_preserves_recent_and_release_binary() {
        let home = std::env::temp_dir().join(format!("agent-rules-cache-{}", std::process::id()));
        let crate_dir = home.join("ai/scripts/tool");
        fs::create_dir_all(&crate_dir).unwrap();
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname=\"tool\"\nversion=\"0.1.0\"\n").unwrap();
        let now = SystemTime::now() + Duration::from_secs(10 * 24 * 60 * 60);
        let debug = crate_dir.join("target/debug");
        let release = crate_dir.join("target/release");
        let make_session = |profile: &Path, crate_key: &str, session: &str, age: u64| {
            let dir = profile.join("incremental").join(crate_key).join(session);
            fs::create_dir_all(&dir).unwrap();
            let file = dir.join("cache.pre-lto.bc");
            fs::write(&file, b"cache").unwrap();
            set_time(&file, now - Duration::from_secs(age * 86400));
            // real layout: dir `s-<a>-<b>-<suffix>`, lock `s-<a>-<b>.lock`
            let stem = session.split('-').take(3).collect::<Vec<_>>().join("-");
            let lock = dir.parent().unwrap().join(format!("{stem}.lock"));
            fs::write(&lock, b"lock").unwrap();
            (dir, lock, file)
        };
        let a = make_session(&debug, "tool-abc", "s-a-1-work", 6);
        let b = make_session(&debug, "tool-abc", "s-b-2-work", 6);
        let c = make_session(&debug, "tool-abc", "s-c-3-work", 6);
        let recent = make_session(&debug, "tool-abc", "s-d-4-work", 1);
        let kept_old = make_session(&debug, "small-abc", "s-a-1-work", 6);
        let kept_old2 = make_session(&debug, "small-abc", "s-b-2-work", 6);
        let mixed_old = make_session(&release, "mixed-abc", "s-a-1-work", 6);
        let mixed_mid = make_session(&release, "mixed-abc", "s-b-2-work", 3);
        let mixed_new = make_session(&release, "mixed-abc", "s-c-3-work", 1);
        let executable = release.join("tool");
        fs::write(&executable, b"binary").unwrap();
        for (hash, age) in [("aaaaaaaaaaaaaaaa", 6), ("bbbbbbbbbbbbbbbb", 6), ("cccccccccccccccc", 1)] {
            let deps = debug.join("deps");
            fs::create_dir_all(&deps).unwrap();
            let artifact = deps.join(format!("libtool-{hash}.rlib"));
            fs::write(&artifact, b"cache").unwrap();
            set_time(&artifact, now - Duration::from_secs(age * 86400));
            let fingerprint = debug.join(".fingerprint").join(format!("tool-{hash}"));
            fs::create_dir_all(&fingerprint).unwrap();
            let marker = fingerprint.join("metadata");
            fs::write(&marker, b"cache").unwrap();
            set_time(&marker, now - Duration::from_secs(age * 86400));
            let build = debug.join("build").join(format!("tool-{hash}"));
            fs::create_dir_all(&build).unwrap();
            let marker = build.join("output");
            fs::write(&marker, b"cache").unwrap();
            set_time(&marker, now - Duration::from_secs(age * 86400));
        }
        let (count, bytes) = prune(&home, now).unwrap();
        assert_eq!((count, bytes), (6, 66));
        for (dir, lock, _) in [&a, &b, &c, &mixed_old] { assert!(!dir.exists() && !lock.exists()); }
        for (dir, lock, file) in [&recent, &kept_old, &kept_old2, &mixed_mid, &mixed_new] {
            assert!(dir.exists() && lock.exists() && file.exists());
        }
        assert!(executable.exists());
        for hash in ["aaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbbb"] {
            assert!(!debug.join("deps").join(format!("libtool-{hash}.rlib")).exists());
            assert!(!debug.join(".fingerprint").join(format!("tool-{hash}")).exists());
            assert!(!debug.join("build").join(format!("tool-{hash}")).exists());
        }
        assert!(debug.join("deps/libtool-cccccccccccccccc.rlib").exists());
        fs::remove_dir_all(home).unwrap();
    }
}
