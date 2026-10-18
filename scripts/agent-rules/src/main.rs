//! agent-rules: 전역 규칙 원본 `~/ai/rules/Agents.md`를 읽고 고치고 Codex·Claude·Antigravity에 동기화한다.

use agent_rules::{self as rules, Paths, State, SIZE_LIMIT, TARGETS};
use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "사용법: agent-rules <명령>
  path                                원본 경로
  read [--section <제목>]             원본 전체 또는 한 구역 출력
  sections                            제목 목록(줄 번호·수준)
  add --section <제목> --text <줄>    구역 끝에 한 줄 추가
  replace --old <A> --new <B>         원본에 한 번만 있는 A를 B로 교체
  remove --text <A>                   A를 포함한 유일한 줄 삭제
  write --file <F|->                  원본 전체를 F(또는 표준입력) 내용으로 교체
  sync [--target <이름>] [--force] [--quiet]
                                      원본을 활성본으로 생성(drift면 백업 후 exit 3)
  status                              활성본 상태와 SHA-256
수정 명령(add·replace·remove·write)은 끝나면 모든 대상을 sync한다.
대상: codex(~/.codex/AGENTS.md), claude(~/.claude/CLAUDE.md),
      antigravity(~/.gemini/GEMINI.md), antigravity-cli(~/.gemini/antigravity-cli/GEMINI.md)";

struct Args {
    rest: Vec<String>,
}

impl Args {
    fn value(&mut self, flag: &str) -> rules::Result<Option<String>> {
        match self.rest.iter().position(|a| a == flag) {
            None => Ok(None),
            Some(i) if i + 1 < self.rest.len() => {
                let v = self.rest.remove(i + 1);
                self.rest.remove(i);
                Ok(Some(v))
            }
            Some(_) => Err(usage(&format!("{flag} 뒤에 값이 없다"))),
        }
    }

    fn need(&mut self, flag: &str) -> rules::Result<String> {
        self.value(flag)?.ok_or_else(|| usage(&format!("{flag}가 필요하다")))
    }

    fn flag(&mut self, flag: &str) -> bool {
        let before = self.rest.len();
        self.rest.retain(|a| a != flag);
        before != self.rest.len()
    }

    fn done(&self) -> rules::Result<()> {
        match self.rest.first() {
            Some(a) => Err(usage(&format!("알 수 없는 인자 `{a}`"))),
            None => Ok(()),
        }
    }
}

fn usage(msg: &str) -> rules::Error {
    rules::Error { code: 2, msg: format!("{msg}\n\n{USAGE}") }
}

fn main() -> ExitCode {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || argv[0] == "-h" || argv[0] == "--help" || argv[0] == "help" {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let cmd = argv.remove(0);
    match run(&cmd, Args { rest: argv }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("agent-rules {cmd}: {e}");
            ExitCode::from(e.code as u8)
        }
    }
}

fn run(cmd: &str, mut a: Args) -> rules::Result<()> {
    let p = Paths::from_env();
    match cmd {
        "path" => {
            a.done()?;
            println!("{}", p.source.display());
        }
        "read" => {
            let title = a.value("--section")?;
            a.done()?;
            let text = rules::read_source(&p)?;
            match title {
                Some(t) => print!("{}", rules::section(&text, &t)?),
                None => print!("{text}"),
            }
        }
        "sections" => {
            a.done()?;
            for (line, level, title) in rules::headings(&rules::read_source(&p)?) {
                println!("{:>4}  {} {title}", line + 1, "#".repeat(level));
            }
        }
        "add" | "replace" | "remove" | "write" => {
            let new = match cmd {
                "add" => {
                    let (title, line) = (a.need("--section")?, a.need("--text")?);
                    rules::add_line(&rules::read_source(&p)?, &title, &line)?
                }
                "replace" => {
                    let (old, new) = (a.need("--old")?, a.need("--new")?);
                    rules::replace_unique(&rules::read_source(&p)?, &old, &new)?
                }
                "remove" => rules::remove_line(&rules::read_source(&p)?, &a.need("--text")?)?,
                _ => read_input(&a.need("--file")?)?,
            };
            a.done()?;
            rules::write_source(&p, &new)?;
            println!("원본 갱신: {} ({}바이트)", p.source.display(), new.len());
            report(&rules::sync(&p, None, false)?, false)?;
        }
        "sync" => {
            let target = a.value("--target")?;
            let (force, quiet) = (a.flag("--force"), a.flag("--quiet"));
            a.done()?;
            report(&rules::sync(&p, target.as_deref(), force)?, quiet)?;
        }
        "status" => {
            a.done()?;
            let text = rules::read_source(&p)?;
            println!("{:<16} {:<12} {}  {}", "원본", "", rules::sha256_hex(text.as_bytes()), p.source.display());
            for t in &TARGETS {
                let (state, _) = rules::inspect(&p, &text, t);
                let sha = std::fs::read(p.target(t)).map(|b| rules::sha256_hex(&b)).unwrap_or_else(|_| "-".into());
                println!("{:<16} {:<12} {sha}  {}", t.name, state.label(), p.target(t).display());
            }
        }
        _ => return Err(usage(&format!("알 수 없는 명령 `{cmd}`"))),
    }
    Ok(())
}

fn read_input(file: &str) -> rules::Result<String> {
    let mut s = String::new();
    let r = if file == "-" {
        std::io::stdin().read_to_string(&mut s).map(|_| ())
    } else {
        std::fs::read_to_string(file).map(|t| s = t)
    };
    r.map_err(|e| rules::Error { code: 1, msg: format!("{file}: {e}") })?;
    Ok(s)
}

fn report(reports: &[rules::Report], quiet: bool) -> rules::Result<()> {
    let mut drift = Vec::new();
    for r in reports {
        let what = match (&r.state, r.written) {
            (State::Drift, false) => "건너뜀(활성본 직접 수정됨)",
            (State::Drift, true) => "덮어씀(--force)",
            (State::Synced, _) => "변경 없음",
            (_, _) => "생성",
        };
        if !quiet || r.state == State::Drift {
            println!("{:<16} {what}  {}  {}", r.name, r.sha, r.path.display());
        }
        if let Some(b) = &r.backup {
            println!("{:<16} 백업  {}", "", b.display());
        }
        if r.bytes > SIZE_LIMIT {
            eprintln!("경고: {} 활성본이 {}바이트로 {SIZE_LIMIT}바이트를 넘는다", r.name, r.bytes);
        }
        if r.state == State::Drift && !r.written {
            drift.push(r.name);
        }
    }
    if drift.is_empty() {
        return Ok(());
    }
    Err(rules::Error {
        code: 3,
        msg: format!(
            "{}은 마지막 sync 뒤 직접 수정됐다. 백업 내용을 원본에 병합한 뒤 `agent-rules sync --force`로 덮어쓴다",
            drift.join(", ")
        ),
    })
}
