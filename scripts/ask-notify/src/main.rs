use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

fn message(event: &Value) -> Option<String> {
    let folder = Path::new(event.get("cwd")?.as_str()?)
        .file_name()?
        .to_str()?;
    let prefix = format!("[요청] {folder} ");

    match event.get("tool_name")?.as_str()? {
        "AskUserQuestion" | "ask_question" => {
            let question = event
                .pointer("/tool_input/questions/0/question")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    let raw = event.pointer("/tool_input/questions")?.as_str()?;
                    let parsed = serde_json::from_str::<Value>(raw).ok()?;
                    parsed.pointer("/0/question")?.as_str().map(str::to_owned)
                })
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "질문".to_owned());
            Some(format!("{prefix}{}", question.chars().take(80).collect::<String>()))
        }
        "ExitPlanMode" => Some(format!("{prefix}계획 승인 요청")),
        _ => None,
    }
}

/// Stop: the last assistant reply summed up in one line, `[요청] ` when it asks the user for something, `[완료] ` otherwise.
fn stop_message(folder: &str, reply: &str) -> Option<String> {
    let line = reply.lines().map(|line| line.trim().trim_start_matches(['#', '-', '*', '>', ' ']).replace(['*', '`'], ""))
        .find(|line| !line.trim().is_empty())?;
    let tail: String = reply.chars().rev().take(300).collect::<String>().chars().rev().collect();
    let asks = ["주세요", "할까요", "될까요", "괜찮을까요", "알려 주", "부탁"].iter().any(|word| tail.contains(word)) || tail.trim_end().ends_with('?');
    Some(format!("{}{folder} {}", if asks { "[요청] " } else { "[완료] " }, line.trim().chars().take(80).collect::<String>()))
}

fn last_reply(transcript: &str) -> Option<String> {
    let text = std::fs::read_to_string(transcript).ok()?;
    text.lines().rev().filter_map(|line| serde_json::from_str::<Value>(line).ok()).find_map(|entry| {
        match entry.get("type")?.as_str()? {
            "assistant" => {
                let parts = entry.pointer("/message/content")?.as_array()?;
                let joined: Vec<&str> = parts.iter().filter(|part| part.get("type").and_then(Value::as_str) == Some("text")).filter_map(|part| part.get("text")?.as_str()).collect();
                (!joined.is_empty()).then(|| joined.join("\n"))
            }
            "PLANNER_RESPONSE" => {
                let content = entry.get("content")?.as_str()?.trim();
                (!content.is_empty()).then(|| content.to_owned())
            }
            _ => None,
        }
    })
}

/// 02:00–09:00 Asia/Seoul is unattended time: the agent decides with the safe default instead of asking.
fn quiet_hour(epoch: u64) -> bool {
    (2..9).contains(&((epoch + 9 * 3600) / 3600 % 24))
}

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return;
    }
    let Ok(event) = serde_json::from_str::<Value>(&input) else {
        return;
    };
    let stop = event.get("hook_event_name").and_then(Value::as_str) == Some("Stop");
    let asks = matches!(event.get("tool_name").and_then(Value::as_str), Some("AskUserQuestion" | "ask_question"));
    let epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if !stop && asks && quiet_hour(epoch) {
        eprintln!("새벽 2시~9시(KST)에는 사용자에게 묻지 않습니다: 되돌릴 수 있는 안전한 기본값(사용자 데이터 보존·비파괴)을 골라 진행하고, 사용자만 할 수 있는 동작(로그인·결제·비가역 외부 전송·OS 앱 조작)만 행 $D에 [🏷] 사용자 대기로 기록하세요.");
        std::process::exit(2);
    }
    if stop && event.get("stop_hook_active").and_then(Value::as_bool) == Some(true) { return; }
    let text = if stop {
        let folder = event.get("cwd").and_then(Value::as_str).and_then(|cwd| Path::new(cwd).file_name()?.to_str().map(str::to_owned)).unwrap_or_default();
        event.get("transcript_path").and_then(Value::as_str).and_then(last_reply).and_then(|reply| stop_message(&folder, &reply))
    } else { message(&event) };
    if let Some(text) = text {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let log_path = std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state/ask-notify.log"));
        if !stop {
            if let Some(ref log) = log_path {
                if let Ok(existing) = std::fs::read_to_string(log) {
                    if let Some(last) = existing.lines().rev().find(|l| !l.trim().is_empty()) {
                        let mut parts = last.split_whitespace();
                        if let (Some(ts), Some(_tool), Some("nf_exit=0")) = (
                            parts.next().and_then(|s| s.parse::<u64>().ok()),
                            parts.next(),
                            parts.next(),
                        ) {
                            if now.saturating_sub(ts) <= 15 {
                                return;
                            }
                        }
                    }
                }
            }
        }
        // A hook PATH may lack the Windows interop dirs, so nf (node) is given node and powershell.exe explicitly.
        let path = format!(
            "/home/tree/.local/share/nvm/v25.8.1/bin:/mnt/c/Windows/System32/WindowsPowerShell/v1.0:/mnt/c/Windows/System32:/usr/local/bin:/usr/bin:/bin:{}",
            std::env::var("PATH").unwrap_or_default()
        );
        // 모바일·데스크톱을 따로 보내야 데스크톱 성공(30초 뒤 자동 닫힘)에 모바일 실패가 가려지지 않는다.
        let send = |channel: &str| Command::new("/home/tree/.local/bin/nf")
            .args([channel, "-m", &text])
            .env("PATH", &path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|code| code.code().unwrap_or(-1).to_string())
            .unwrap_or_else(|_| "spawn".into());
        let mobile = send("--mobile-only");
        // 데스크톱 알림은 창이 닫힐 때까지(~30초) 끝나지 않아 hook 제한(20초)에 잘리므로, 자기 프로세스 그룹으로 띄우고 기다리지 않는다.
        let desktop = {
            use std::os::unix::process::CommandExt;
            // 끝난 뒤 exit를 같은 로그에 남겨, 백그라운드 데스크톱 전송의 실제 결과를 확인할 수 있게 한다.
            let log = std::env::var("HOME").map(|h| format!("{h}/.local/state/ask-notify.log")).unwrap_or_default();
            Command::new("/bin/sh")
                .args(["-c", "/home/tree/.local/bin/nf --desktop-only -m \"$1\"; echo \"$(date +%s) desktop_done exit=$?\" >> \"$2\"", "sh", &text, &log])
                .env("PATH", &path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
                .map(|_| "bg".to_string())
                .unwrap_or_else(|_| "spawn".into())
        };
        let tool = if stop { "Stop" } else { event.get("tool_name").and_then(Value::as_str).unwrap_or("-") };
        let result = if mobile != "0" { mobile.clone() } else if desktop == "bg" { "0".to_string() } else { desktop.clone() };
        if let Some(log) = log_path {
            let _ = std::fs::create_dir_all(log.parent().unwrap());
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(log) {
                use std::io::Write;
                let tag = if text.starts_with("[완료] ") { " [완료]" } else if text.starts_with("[요청] ") { " [요청]" } else { "" };
                let _ = writeln!(file, "{now} {tool} nf_exit={result} mobile_exit={mobile} desktop_exit={desktop}{tag}");
            }
        }
        // A question or approval request never goes out without its notification.
        if !stop && result != "0" {
            eprintln!("nf 알림 전송 실패(exit {result}): 질문 전에 nf가 동작하도록 고친 뒤 다시 질문하세요.");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{message, quiet_hour, stop_message};

    #[test]
    fn quiet_hours_are_two_to_nine_kst() {
        // 2026-11-06 17:00 UTC = 11-07 02:00 KST.
        let two_am = 1_793_984_400;
        assert!(!quiet_hour(two_am - 1));
        assert!(quiet_hour(two_am));
        assert!(quiet_hour(two_am + 7 * 3600 - 1));
        assert!(!quiet_hour(two_am + 7 * 3600));
    }

    #[test]
    fn stop_reply_marks_requests() {
        assert_eq!(stop_message("cartoon", "## 결과\n저장했어요.").as_deref(), Some("[완료] cartoon 결과"));
        assert_eq!(stop_message("cartoon", "Eagle을 다시 켜 주세요.\n이어서 할게요. 다시 켜 주세요.").as_deref(), Some("[요청] cartoon Eagle을 다시 켜 주세요."));
    }
    use serde_json::json;

    #[test]
    fn ask_user_question_uses_first_question_and_folder() {
        let event = json!({
            "tool_name": "AskUserQuestion",
            "tool_input": {"questions": [{"question": "어느 색을 고를까요?"}, {"question": "두 번째"}]},
            "cwd": "/home/tree/extensions"
        });
        assert_eq!(message(&event).as_deref(), Some("[요청] extensions 어느 색을 고를까요?"));
    }

    #[test]
    fn exit_plan_mode_requests_approval() {
        let event = json!({
            "tool_name": "ExitPlanMode",
            "tool_input": {},
            "cwd": "/home/tree/extensions"
        });
        assert_eq!(message(&event).as_deref(), Some("[요청] extensions 계획 승인 요청"));
    }

    #[test]
    fn other_tool_is_ignored() {
        let event = json!({
            "tool_name": "Read",
            "tool_input": {},
            "cwd": "/home/tree/extensions"
        });
        assert_eq!(message(&event), None);
    }

    #[test]
    fn question_is_truncated_by_characters() {
        let question = format!("{}끝", "가".repeat(80));
        let event = json!({
            "tool_name": "AskUserQuestion",
            "tool_input": {"questions": [{"question": question}]},
            "cwd": "/home/tree/extensions"
        });
        assert_eq!(message(&event).as_deref(), Some(format!("[요청] extensions {}", "가".repeat(80)).as_str()));
    }

    #[test]
    fn missing_question_uses_generic_label() {
        let event = json!({
            "tool_name": "AskUserQuestion",
            "tool_input": {"questions": []},
            "cwd": "/home/tree/extensions"
        });
        assert_eq!(message(&event).as_deref(), Some("[요청] extensions 질문"));
    }

    #[test]
    fn agy_ask_question_uses_first_question_and_folder() {
        let event = json!({
            "tool_name": "ask_question",
            "tool_input": {"questions": [{"question": "프로젝트 타입을 선택해 주세요."}]},
            "cwd": "/home/tree/extensions"
        });
        assert_eq!(
            message(&event).as_deref(),
            Some("[요청] extensions 프로젝트 타입을 선택해 주세요.")
        );
    }
}
