const REQUIRED: [&str; 7] = ["재현", "수치", "배제한 가설", "원인 후보", "확신도", "해결책", "확인 방법"];

fn section<'a>(report: &'a str, name: &str) -> Option<String> {
    let heading = format!("# {name}");
    let mut lines = report.lines().skip_while(|line| line.trim() != heading);
    lines.next()?;
    Some(lines.take_while(|line| !line.starts_with("# ")).collect::<Vec<_>>().join("\n").trim().to_owned())
}

/// First reason the `resolve-deadlock` report is unusable, or None when it has every required section.
pub fn analysis_report_problem(report: &str) -> Option<String> {
    for name in REQUIRED {
        match section(report, name) {
            None => return Some(format!("`# {name}` 절이 없어요")),
            Some(body) if body.is_empty() => return Some(format!("`# {name}` 절이 비었어요")),
            Some(_) => {}
        }
    }
    if !section(report, "수치")?.chars().any(|c| c.is_ascii_digit()) {
        return Some("`# 수치` 절에 숫자가 없어요".into());
    }
    if !matches!(section(report, "확신도")?.as_str(), "높음" | "중간" | "낮음") {
        return Some("`# 확신도`는 높음·중간·낮음 중 하나여야 해요".into());
    }
    None
}
