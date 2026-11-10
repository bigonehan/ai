use std::{cmp::Ordering, ffi::OsString, fs, path::{Path, PathBuf}, process::{Command, Output}};

const ROOT: &str = "/mnt/c/Program Files/Blender Foundation";
const SEARCH: &str = "/mnt/c/Program Files/Blender Foundation/Blender */blender.exe";

#[derive(Debug)]
struct Args { project: PathBuf, script: PathBuf, exe: Option<PathBuf>, extra: Vec<OsString>, user_prefs: bool }

fn parse(items: impl IntoIterator<Item=OsString>) -> Result<Args, String> {
    let mut it = items.into_iter();
    let (mut project, mut script, mut exe, mut extra, mut user_prefs) = (None, None, None, vec![], false);
    while let Some(item) = it.next() {
        match item.to_str() {
            Some("--project") => project = Some(PathBuf::from(it.next().ok_or("실패 — --project 값 없음")?)),
            Some("--script") => script = Some(PathBuf::from(it.next().ok_or("실패 — --script 값 없음")?)),
            Some("--exe") => exe = Some(PathBuf::from(it.next().ok_or("실패 — --exe 값 없음")?)),
            Some("--user-prefs") => user_prefs = true,
            Some("--") => { extra.extend(it); break; }
            _ => return Err(format!("실패 — 알 수 없는 인자: {}", item.to_string_lossy())),
        }
    }
    Ok(Args { project: project.ok_or("실패 — --project 필수")?, script: script.ok_or("실패 — --script 필수")?, exe, extra, user_prefs })
}

fn version(path: &Path) -> Option<Vec<u32>> {
    let name = path.file_name()?.to_str()?.strip_prefix("Blender ")?;
    let parts: Vec<u32> = name.split('.').map(|piece| piece.parse().ok()).collect::<Option<_>>()?;
    (!parts.is_empty()).then_some(parts)
}

fn compare_version(a: &[u32], b: &[u32]) -> Ordering {
    (0..a.len().max(b.len())).map(|i| a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)))
        .find(|order| *order != Ordering::Equal).unwrap_or(Ordering::Equal)
}

fn choose_exe(root: &Path) -> Option<PathBuf> {
    fs::read_dir(root).ok()?.flatten().filter_map(|entry| {
        let v = version(&entry.path())?;
        let exe = entry.path().join("blender.exe");
        exe.is_file().then_some((v, exe))
    }).max_by(|(a, pa), (b, pb)| compare_version(a,b).then_with(|| pa.cmp(pb))).map(|(_, exe)| exe)
}

fn validate(args: Args, root: &Path) -> Result<(PathBuf, PathBuf, PathBuf, Vec<OsString>), (String, i32)> {
    let project = fs::canonicalize(&args.project).map_err(|_| (format!("실패 — 프로젝트 없음: {}", args.project.display()), 2))?;
    if !project.is_dir() { return Err((format!("실패 — 프로젝트 디렉터리 아님: {}", project.display()), 2)); }
    let script = if args.script.is_absolute() { args.script } else { project.join(args.script) };
    let script = fs::canonicalize(&script).map_err(|_| (format!("실패 — 스크립트 없음: {}", script.display()), 2))?;
    if !script.is_file() { return Err((format!("실패 — 스크립트 파일 아님: {}", script.display()), 2)); }
    let exe = args.exe.or_else(|| choose_exe(root)).ok_or_else(|| (format!("실패 — blender.exe 없음: {SEARCH}"), 2))?;
    Ok((project, script, exe, args.extra))
}

fn wslpath(path: &Path) -> Result<OsString, String> {
    let out = Command::new("wslpath").arg("-w").arg(path).output().map_err(|e| e.to_string())?;
    if !out.status.success() { return Err(format!("wslpath exit={}", out.status.code().unwrap_or(1))); }
    let text = String::from_utf8(out.stdout).map_err(|e| e.to_string())?;
    let text = text.trim();
    if text.is_empty() { return Err("wslpath 결과 없음".into()); }
    Ok(text.into())
}

/// `--user-prefs`면 설치된 사용자 확장·설정을 쓰도록 `--factory-startup`을 뺀다.
fn blender_args(script: OsString, extra: &[OsString], user_prefs: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["-b".into()];
    if !user_prefs { args.push("--factory-startup".into()); }
    args.extend(["--python-exit-code".into(), "1".into(), "--python".into(), script]);
    if !extra.is_empty() { args.push("--".into()); args.extend_from_slice(extra); }
    args
}

fn display(exe: &Path, args: &[OsString]) -> String {
    std::iter::once(exe.as_os_str()).chain(args.iter().map(OsString::as_os_str)).map(|part| {
        let s = part.to_string_lossy();
        if s.contains([' ', '"']) { format!("\"{}\"", s.replace('"', "\\\"")) } else { s.into_owned() }
    }).collect::<Vec<_>>().join(" ")
}

fn select(stdout: &[u8], stderr: &[u8]) -> Vec<String> {
    let lines: Vec<String> = [stdout, stderr].into_iter().flat_map(|bytes| String::from_utf8_lossy(bytes).lines().map(str::to_owned).collect::<Vec<_>>()).collect();
    let hits: Vec<String> = lines.iter().filter(|line| ["SMOKE", "Error", "Traceback", "Exception"].iter().any(|word| line.contains(word))).cloned().collect();
    if hits.is_empty() { lines.into_iter().rev().take(10).collect::<Vec<_>>().into_iter().rev().collect() } else { hits }
}

fn log(exit: i32, error: &str) {
    let ack = if exit == 0 { "received" } else { "rejected" };
    eprintln!("00119805521 | win-smoke.run | 01 | ack={ack} body.exit={exit} error={error}");
}

fn run_with<C, R>(args: Args, root: &Path, convert: C, runner: R) -> i32
where C: Fn(&Path)->Result<OsString,String>, R: Fn(&Path,&Path,&[OsString])->std::io::Result<Output> {
    let user_prefs = args.user_prefs;
    let (project, script, exe, extra) = match validate(args, root) {
        Ok(v) => v, Err((message, code)) => { eprintln!("{message}"); return code; }
    };
    let script = match convert(&script) {
        Ok(v) => v, Err(error) => { eprintln!("실패 — 경로 변환 불가: {error}"); log(3,"path_conversion"); return 3; }
    };
    let arguments = blender_args(script, &extra, user_prefs);
    println!("run: {}", display(&exe, &arguments));
    match runner(&exe, &project, &arguments) {
        Ok(output) => {
            for line in select(&output.stdout, &output.stderr) { println!("{line}"); }
            let code = output.status.code().unwrap_or(1);
            println!("exit={code}");
            log(code, if code == 0 { "none" } else { "blender_exit" });
            code
        }
        Err(error) => { eprintln!("실패 — 실행 불가: {error}"); println!("exit=3"); log(3,"spawn_failed"); 3 }
    }
}

const TAURI_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$exe = $env:TAURI_SMOKE_EXE
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=19339'
$p = Start-Process -FilePath $exe -PassThru
try {
  Start-Sleep -Seconds 5
  $console = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($p.Id)" | Where-Object { $_.Name -match 'conhost|OpenConsole|WindowsTerminal' })
  if ($console.Count -gt 0) { throw "console_children=$($console.Count)" }
  $pages = @(Invoke-RestMethod 'http://127.0.0.1:19339/json')
  foreach ($label in @('overlay', 'settings')) {
    $page = $pages | Where-Object { $_.url -match $label } | Select-Object -First 1
    if (-not $page) { throw "missing_page=$label" }
    $ws = [System.Net.WebSockets.ClientWebSocket]::new()
    try {
      $ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait()
      $expr = "window.__TAURI__.event.listen('smoke-check',()=>{}).then(f=>{f();if(/not allowed by ACL|uncaught|exception/i.test(document.body.innerText))throw Error('page error text');return 'ok'})"
      $msg = @{ id=1; method='Runtime.evaluate'; params=@{ expression=$expr; awaitPromise=$true; returnByValue=$true } } | ConvertTo-Json -Compress -Depth 5
      $bytes = [Text.Encoding]::UTF8.GetBytes($msg)
      $ws.SendAsync([ArraySegment[byte]]$bytes, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).Wait()
      do {
        $buf = New-Object byte[] 65536
        $sb = New-Object System.Text.StringBuilder
        do {
          $r = $ws.ReceiveAsync([ArraySegment[byte]]$buf, [Threading.CancellationToken]::None).Result
          [void]$sb.Append([Text.Encoding]::UTF8.GetString($buf, 0, $r.Count))
        } while (-not $r.EndOfMessage)
        $response = $sb.ToString() | ConvertFrom-Json
      } while ($response.id -ne 1)
      if ($response.result.exceptionDetails -or $response.result.result.value -ne 'ok') { throw "event_listen_failed=$label" }
      Write-Output "SMOKE $label event.listen=ok"
    } finally { $ws.Dispose() }
  }
  Write-Output 'SMOKE console_children=0'
} finally {
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
"#;

fn run_tauri(exe: &Path) -> i32 {
    if !exe.is_file() { eprintln!("실패 — exe 없음: {}", exe.display()); return 2; }
    let win_exe = match wslpath(exe) {
        Ok(path) => path, Err(error) => { eprintln!("실패 — 경로 변환 불가: {error}"); return 3; }
    };
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", TAURI_SCRIPT])
        .env("TAURI_SMOKE_EXE", win_exe)
        .output();
    match output {
        Ok(output) => {
            for line in select(&output.stdout, &output.stderr) { println!("{line}"); }
            let code = output.status.code().unwrap_or(1);
            println!("exit={code}");
            log(code, if code == 0 { "none" } else { "tauri_runtime" });
            code
        }
        Err(error) => { eprintln!("실패 — PowerShell 실행 불가: {error}"); 3 }
    }
}

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--tauri-exe")) {
        let exe = std::env::args_os().nth(2).unwrap_or_default();
        std::process::exit(run_tauri(Path::new(&exe)));
    }
    let args = match parse(std::env::args_os().skip(1)) {
        Ok(v) => v, Err(message) => { eprintln!("{message}"); std::process::exit(2); }
    };
    std::process::exit(run_with(args, Path::new(ROOT), wslpath, |exe, project, args| Command::new(exe).current_dir(project).args(args).output()));
}

#[cfg(test)]
mod tests {
    #[test]
    fn user_prefs_drops_factory_startup() {
        let a = super::blender_args("C:\\s.py".into(), &[], true);
        assert!(!a.iter().any(|x| x == "--factory-startup"));
        assert!(super::blender_args("C:\\s.py".into(), &[], false).iter().any(|x| x == "--factory-startup"));
        assert!(super::parse(["--project","p","--script","s","--user-prefs"].map(std::ffi::OsString::from)).unwrap().user_prefs);
    }

    use super::*;
    use std::sync::atomic::{AtomicUsize,Ordering as AtomicOrdering};
    fn temp() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!("win-smoke-{}-{}", std::process::id(), NEXT.fetch_add(1, AtomicOrdering::Relaxed)));
        fs::create_dir_all(&path).unwrap(); path
    }
    #[test]
    fn args_and_relative_path() {
        assert_eq!(parse(Vec::<OsString>::new()).unwrap_err(), "실패 — --project 필수");
        assert_eq!(parse(["--project","x"].map(OsString::from)).unwrap_err(), "실패 — --script 필수");
        let p=temp(); fs::write(p.join("smoke.py"), "pass").unwrap();
        let args=Args{project:p.clone(),script:"smoke.py".into(),exe:Some("x".into()),extra:vec![],user_prefs:false};
        assert_eq!(validate(args,Path::new("unused")).unwrap().1,p.join("smoke.py"));
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn numeric_versions() {
        let p=temp(); for v in ["5.2","5.10","5.9"] { let d=p.join(format!("Blender {v}")); fs::create_dir(&d).unwrap(); fs::write(d.join("blender.exe"),"").unwrap(); }
        assert_eq!(choose_exe(&p),Some(p.join("Blender 5.10/blender.exe")));
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn command_and_runner() {
        let p=temp(); fs::write(p.join("smoke.py"),"pass").unwrap();
        let a=Args{project:p.clone(),script:"smoke.py".into(),exe:Some("blender.exe".into()),extra:vec!["two words".into()],user_prefs:false};
        let code=run_with(a,Path::new("unused"), |_| Ok("C:\\smoke.py".into()), |exe,cwd,args| {
            assert_eq!(exe,Path::new("blender.exe")); assert_eq!(cwd,p);
            assert_eq!(args,&blender_args("C:\\smoke.py".into(), &["two words".into()], false));
            Command::new("true").output()
        });
        assert_eq!(code,0);
        assert!(display(Path::new("blender.exe"),&blender_args("C:\\smoke.py".into(), &["two words".into()], false)).contains("--python-exit-code 1 --python C:\\smoke.py -- \"two words\""));
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn filtering_and_failures() {
        assert_eq!(select(b"noise\nSMOKE ok\n",b"Traceback\nError\nException\n"),["SMOKE ok","Traceback","Error","Exception"]);
        let many=(0..12).map(|i| format!("line{i}\n")).collect::<String>();
        assert_eq!(select(many.as_bytes(),b"").first().unwrap(),"line2");
        let p=temp(); fs::write(p.join("smoke.py"),"pass").unwrap();
        let make=|script:&str,exe:Option<PathBuf>| Args{project:p.clone(),script:script.into(),exe,extra:vec![],user_prefs:false};
        assert_eq!(validate(make("smoke.py",None),Path::new("absent")).unwrap_err().0,format!("실패 — blender.exe 없음: {SEARCH}"));
        assert!(validate(make("missing.py",Some("x".into())),Path::new("unused")).unwrap_err().0.starts_with("실패 — 스크립트 없음:"));
        let absent=Args{project:p.join("absent"),script:"smoke.py".into(),exe:Some("x".into()),extra:vec![],user_prefs:false};
        assert!(validate(absent,Path::new("unused")).unwrap_err().0.starts_with("실패 — 프로젝트 없음:"));
        assert_eq!(run_with(make("smoke.py",Some("x".into())),Path::new("unused"), |_| Err("wslpath failed".into()), |_,_,_| unreachable!()),3);
        assert_eq!(run_with(make("smoke.py",Some("x".into())),Path::new("unused"), |_| Ok("C:\\smoke.py".into()), |_,_,_| Err(std::io::Error::new(std::io::ErrorKind::NotFound,"exe missing"))),3);
        fs::remove_dir_all(p).unwrap();
    }
}
