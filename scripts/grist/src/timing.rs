//! Best-effort, credential-free timing for one CLI invocation.

use crate::util;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Invocation {
    started: Instant,
    command: String,
    api_calls: AtomicU64,
    api_ns: AtomicU64,
    jev_ns: AtomicU64,
}

static INVOCATION: OnceLock<Invocation> = OnceLock::new();

fn command_name<'a>(first: Option<&'a str>, known: &[&str]) -> &'a str {
    first.filter(|name| known.contains(name)).unwrap_or("unknown")
}

pub fn start(first: Option<&str>, known: &[&str]) {
    let command = command_name(first, known);
    let _ = INVOCATION.set(Invocation {
        started: Instant::now(),
        command: command.to_string(),
        api_calls: AtomicU64::new(0),
        api_ns: AtomicU64::new(0),
        jev_ns: AtomicU64::new(0),
    });
}

pub fn api_call(elapsed: Duration) {
    if let Some(invocation) = INVOCATION.get() {
        invocation.api_calls.fetch_add(1, Ordering::Relaxed);
        invocation.api_ns.fetch_add(elapsed.as_nanos().min(u64::MAX as u128) as u64, Ordering::Relaxed);
    }
}

pub fn jev_call(elapsed: Duration) {
    if let Some(invocation) = INVOCATION.get() {
        invocation.jev_ns.fetch_add(elapsed.as_nanos().min(u64::MAX as u128) as u64, Ordering::Relaxed);
    }
}

fn line(command: &str, elapsed: Duration, calls: u64, api_ns: u64, jev_ns: u64, exit: i32, now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64 + 9 * 3600;
    let (year, month, day) = util::civil_from_days(secs.div_euclid(86400));
    let hour = secs.rem_euclid(86400) / 3600;
    let minute = secs.rem_euclid(3600) / 60;
    let second = secs.rem_euclid(60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}+09:00 cmd={command} ms={} api={calls} api_ms={} jev_ms={} exit={exit}\n",
        elapsed.as_millis(), api_ns as u128 / 1_000_000, jev_ns as u128 / 1_000_000)
}

pub fn finish(exit: i32) {
    let Some(invocation) = INVOCATION.get() else { return; };
    let path = util::home().join(".cache/grist/timing.log");
    let Some(parent) = path.parent() else { return; };
    if fs::create_dir_all(parent).is_err() { return; }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else { return; };
    let record = line(&invocation.command, invocation.started.elapsed(),
        invocation.api_calls.load(Ordering::Relaxed), invocation.api_ns.load(Ordering::Relaxed),
        invocation.jev_ns.load(Ordering::Relaxed), exit, SystemTime::now());
    let _ = file.write_all(record.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_line_has_only_command_and_aggregate_values() {
        let value = line("task", Duration::from_millis(123), 2, 12_900_000, 4_900_000, 2, UNIX_EPOCH);
        assert_eq!(value, "1970-01-01T09:00:00+09:00 cmd=task ms=123 api=2 api_ms=12 jev_ms=4 exit=2\n");
        assert!(!value.contains("--project"));
        assert!(!value.contains("/home/"));
    }

    #[test]
    fn timing_command_rejects_arbitrary_first_argument() {
        let known = ["task", "get"];
        assert_eq!(command_name(Some("task"), &known), "task");
        assert_eq!(command_name(Some("/private/path"), &known), "unknown");
        assert_eq!(command_name(Some("secret-token"), &known), "unknown");
        assert_eq!(command_name(None, &known), "unknown");
    }
}
