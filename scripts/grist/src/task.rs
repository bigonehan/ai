//! Table26 task steps. A step validates once, then `TaskChain` writes its result to every
//! `TaskStore` in a fixed order: the gate test record, `.ai/changes`, and Grist.
//! Each store write is idempotent, so a repeated step heals a partial earlier write.

use crate::api::{self, Exit, R, TASK_TABLE};
use crate::gate;
use crate::records;
use crate::util;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const CONFIRMED_STAGE: &str = "확인";
pub const STAGE_START_CARD: &str = "~/ai/skills/manage-project/references/stage-start.md";
pub const STAGE_WORK_CARD: &str = "~/ai/skills/manage-project/references/stage-work.md";
pub const STAGE_VERIFY_CARD: &str = "~/ai/skills/manage-project/references/stage-verify.md";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Add,
    Ready,
    Submit,
    Check,
}

/// The values one step hands to every store.
#[derive(Default)]
pub struct TaskCtx {
    pub project_dir: Option<PathBuf>,
    pub raw_id: i64,
    /// `$H` read before the step.
    pub stage: String,
    /// add: Table26 fields; Grist fills `raw_id` from the created row.
    pub fields: Map<String, Value>,
    pub kind: String,
    pub judge: String,
    /// ready: `red` or `ready`; submit: the preparation kind that produced the test.
    pub prep_kind: &'static str,
    pub prepared: Map<String, Value>,
    pub purpose: String,
    pub detail: String,
    pub test: String,
    pub output: String,
    pub changed: usize,
    /// The JSON the CLI prints.
    pub result: Value,
}

impl TaskCtx {
    pub fn dir(&self) -> R<&Path> {
        self.project_dir.as_deref().ok_or_else(|| Exit::Other("task step needs --project".into()))
    }
}

/// One place a task step is recorded. A store skips the steps it does not record.
pub trait TaskStore {
    fn name(&self) -> &'static str;
    fn add(&self, _ctx: &mut TaskCtx) -> R<()> { Ok(()) }
    fn ready(&self, _ctx: &mut TaskCtx) -> R<()> { Ok(()) }
    fn submit(&self, _ctx: &mut TaskCtx) -> R<()> { Ok(()) }
    fn check(&self, _ctx: &mut TaskCtx) -> R<()> { Ok(()) }
}

pub struct TaskChain {
    stores: Vec<Box<dyn TaskStore>>,
}

impl TaskChain {
    pub fn new(stores: Vec<Box<dyn TaskStore>>) -> Self {
        Self { stores }
    }

    /// add: gate cleanup reads the changes list before the new item joins it; Grist then creates
    /// the row ID that the changes record uses.
    /// ready/submit: local test record and changes first, Grist `$H` last.
    /// check: changes completion list first, Grist `$H=확인` last.
    pub fn standard(step: Step) -> Self {
        match step {
            Step::Add => Self::new(vec![Box::new(GateStore), Box::new(GristStore), Box::new(ChangesStore)]),
            Step::Ready | Step::Submit => Self::new(vec![Box::new(GateStore), Box::new(ChangesStore), Box::new(GristStore)]),
            Step::Check => Self::new(vec![Box::new(ChangesStore), Box::new(GristStore)]),
        }
    }

    pub fn run(&self, step: Step, ctx: &mut TaskCtx) -> R<()> {
        for store in &self.stores {
            let result = match step {
                Step::Add => store.add(ctx),
                Step::Ready => store.ready(ctx),
                Step::Submit => store.submit(ctx),
                Step::Check => store.check(ctx),
            };
            result.map_err(|e| prefix_exit(store.name(), e))?;
        }
        Ok(())
    }
}

fn prefix_exit(store: &str, error: Exit) -> Exit {
    match error {
        Exit::Fail(boundary, message) => Exit::Fail(boundary, format!("{store}: {message}")),
        Exit::Other(message) => Exit::Other(format!("{store}: {message}")),
        gate @ Exit::Gate(_) => gate,
    }
}

/// `.ai/gate/<id>.json`: focused test command, editable paths and submitted output.
pub struct GateStore;

impl TaskStore for GateStore {
    fn name(&self) -> &'static str { "gate" }

    /// The first row of a new batch (no open item in the latest changes list) clears every gate
    /// file: finished rows' records are no longer read, and no open row needs their paths.
    fn add(&self, ctx: &mut TaskCtx) -> R<()> {
        let Some(dir) = ctx.project_dir.as_deref() else { return Ok(()) };
        if open_ids(dir).is_empty() {
            clear_gate(dir)?;
        }
        Ok(())
    }

    fn ready(&self, ctx: &mut TaskCtx) -> R<()> {
        let dir = ctx.dir()?.to_path_buf();
        let mut state = if ctx.prep_kind == "red" { Map::new() } else { gate::gate_load(&dir, ctx.raw_id) };
        state.remove("flow_wait");
        state.remove("verifier_recorded");
        state.remove(if ctx.prep_kind == "red" { "ready" } else { "red" });
        state.insert(ctx.prep_kind.into(), Value::Object(ctx.prepared.clone()));
        gate::gate_save(&dir, ctx.raw_id, &state)
    }

    fn submit(&self, ctx: &mut TaskCtx) -> R<()> {
        let dir = ctx.dir()?.to_path_buf();
        let mut state = gate::gate_load(&dir, ctx.raw_id);
        state.remove("flow_wait");
        state.remove("verifier_recorded");
        state.insert(ctx.prep_kind.into(), Value::Object(ctx.prepared.clone()));
        state.insert("submit".into(), json!({"purpose": ctx.purpose, "test": ctx.test, "exit": 0, "output": ctx.output, "changed": ctx.changed}));
        state.insert("hidden".into(), json!([]));
        gate::gate_save(&dir, ctx.raw_id, &state)
    }
}

/// `.ai/changes/YYMMDD.<제목>.md`: item block, test records, completion note and list.
pub struct ChangesStore;

impl TaskStore for ChangesStore {
    fn name(&self) -> &'static str { "changes" }

    fn add(&self, ctx: &mut TaskCtx) -> R<()> {
        let Some(dir) = ctx.project_dir.clone() else { return Ok(()) };
        records::register_session_row(ctx.raw_id)?;
        let title = ctx
            .fields
            .get("B")
            .filter(|v| util::truthy(Some(v)))
            .map(util::val_str)
            .unwrap_or_else(|| format!("작업 {}", ctx.raw_id));
        let record = records::register_item(&dir, ctx.raw_id, &title, &ctx.kind, &ctx.judge)?;
        if let Some(obj) = ctx.result.as_object_mut() {
            obj.insert("record".into(), Value::String(record.file_name().unwrap().to_string_lossy().into_owned()));
        }
        records::apply_row(&dir, ctx.raw_id, &ctx.fields)?;
        Ok(())
    }

    fn ready(&self, ctx: &mut TaskCtx) -> R<()> {
        let ok = ctx.prep_kind == "ready";
        records::append_item(ctx.dir()?, ctx.raw_id, &gate::test_record("초안", ctx.raw_id, &ctx.purpose, ok, &ctx.detail))
    }

    fn submit(&self, ctx: &mut TaskCtx) -> R<()> {
        records::append_item(ctx.dir()?, ctx.raw_id, &gate::test_record("검증", ctx.raw_id, &ctx.purpose, true, &ctx.detail))
    }

    fn check(&self, ctx: &mut TaskCtx) -> R<()> {
        let dir = ctx.dir()?;
        records::ensure_completion_note(dir, ctx.raw_id)?;
        if !records::has_completion_note(dir, ctx.raw_id) {
            return api::fail("completion", "completion note requires an implemented step");
        }
        records::complete_item(dir, ctx.raw_id)?;
        Ok(())
    }
}

/// Grist Table26: the row itself and its `$H` stage.
pub struct GristStore;

impl GristStore {
    fn set_stage(ctx: &mut TaskCtx, stage: &str) -> R<()> {
        let mut upd = Map::new();
        upd.insert("H".into(), Value::String(stage.into()));
        api::update(TASK_TABLE, ctx.raw_id, &upd)?;
        ctx.result = json!({"id": ctx.raw_id, "H": stage, "next": if stage == "작업" { STAGE_WORK_CARD } else { STAGE_VERIFY_CARD }});
        Ok(())
    }
}

impl TaskStore for GristStore {
    fn name(&self) -> &'static str { "grist" }

    fn add(&self, ctx: &mut TaskCtx) -> R<()> {
        ctx.result = api::add(TASK_TABLE, &ctx.fields)?;
        ctx.raw_id = ctx.result.get("id").and_then(Value::as_i64).unwrap_or(0);
        Ok(())
    }

    fn ready(&self, ctx: &mut TaskCtx) -> R<()> {
        if ctx.stage == gate::DRAFT_STAGE || ctx.stage == gate::ERROR_STAGE {
            Self::set_stage(ctx, "작업")
        } else {
            ctx.result = json!({"id": ctx.raw_id, "H": ctx.stage, "next": STAGE_WORK_CARD});
            Ok(())
        }
    }

    fn submit(&self, ctx: &mut TaskCtx) -> R<()> {
        Self::set_stage(ctx, api::SUBMITTED_STAGE)
    }

    fn check(&self, ctx: &mut TaskCtx) -> R<()> {
        if ctx.stage == CONFIRMED_STAGE {
            return Ok(());
        }
        let summary = ctx.project_dir.as_deref().and_then(|dir| gate::flow_summary(dir, ctx.raw_id));
        api::update(TASK_TABLE, ctx.raw_id, &gate::confirmed_fields(CONFIRMED_STAGE, summary.as_deref()))?;
        ctx.result = json!({"id": ctx.raw_id, "H": CONFIRMED_STAGE});
        Ok(())
    }
}

// --- check: completion judgment ----------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    /// Listed in `# 완료` and already `확인`/`완료`; nothing judged or written.
    Skipped,
    /// J1 passed and the chain wrote the completion note, list move and `$H=확인`.
    Completed,
}

/// One row's completion: evidence → J1 goal/output match → check chain.
/// A row already in `# 완료` whose `$H` is `확인` or `완료` is skipped; any other `$H`
/// (a repair reopened it, or a previous `$H` write failed) is judged again.
pub fn check(project_dir: &Path, raw_id: i64) -> R<CheckOutcome> {
    let row = gate::rows(&[raw_id])?.into_iter().next()
        .ok_or_else(|| Exit::Other(format!("Table26 #{raw_id} not found")))?;
    let stage = row.h_str().to_string();
    let in_done = completed_ids(project_dir).contains(&raw_id);
    if stage == api::DONE_STAGE || (in_done && stage == CONFIRMED_STAGE) {
        return Ok(CheckOutcome::Skipped);
    }
    let state = gate::gate_load(project_dir, raw_id);
    let output = if gate::preparation_of(&state).is_some() {
        code_evidence(project_dir, raw_id, &stage, &state)?
    } else if text_only_row(project_dir, raw_id) {
        confirmation_output(project_dir, raw_id)?
    } else {
        return api::fail("TEST", format!("#{raw_id} document-only confirmation is required"));
    };
    let goal = records::changes_goal(project_dir, raw_id)
        .ok_or_else(|| Exit::Fail("TEST".into(), format!("#{raw_id} empty goal")))?;
    let goal_count = match_goal_count(&goal);
    let judgment = gate::jev("match", Some(raw_id), &[("goal", &goal), ("output", &output)]);
    require_j1_pass(&judgment, goal_count).map_err(|e| Exit::Fail("COMPLETE_TASK".into(), format!("#{raw_id} {e}")))?;
    let mut ctx = TaskCtx { project_dir: Some(project_dir.to_path_buf()), raw_id, stage, ..Default::default() };
    TaskChain::standard(Step::Check).run(Step::Check, &mut ctx)?;
    Ok(CheckOutcome::Completed)
}

fn code_evidence(project_dir: &Path, raw_id: i64, stage: &str, state: &Map<String, Value>) -> R<String> {
    if stage != api::SUBMITTED_STAGE && stage != CONFIRMED_STAGE {
        return api::fail("TEST", format!("#{raw_id} $H={stage}"));
    }
    let light_row = matches!(state.get("submit").and_then(|s| s.get("changed")).and_then(Value::as_u64), Some(1 | 2));
    if !light_row && !util::truthy(state.get("flow_wait")) {
        return api::fail("TEST", format!("#{raw_id} flow-wait evidence missing"));
    }
    state
        .get("submit")
        .filter(|s| s.get("exit").and_then(Value::as_i64) == Some(0))
        .and_then(|s| s.get("output"))
        .and_then(Value::as_str)
        .ok_or_else(|| Exit::Fail("TEST".into(), format!("#{raw_id} submitted test output missing; run task-submit")))?;
    let flow = flow_confirmation_in_text(&changes_text(project_dir), raw_id)
        .map_err(|e| Exit::Fail("TEST".into(), format!("#{raw_id} {e}")))?
        .ok_or_else(|| Exit::Fail("TEST".into(), format!("#{raw_id} missing flow confirmation after latest verification")))?;
    let (evidence, rest) = split_goal_evidence(flow.lines())
        .ok_or_else(|| Exit::Fail("TEST".into(), format!("#{raw_id} {EVIDENCE_MISSING}")))?;
    if let Some(marker) = runtime_evidence_marker(project_dir, raw_id, &flow) {
        return api::fail("TEST", format!("#{raw_id} runtime confirmation contains unverified evidence: {marker}"));
    }
    Ok(format!("# 목표별 근거\n{evidence}\n# 전체 흐름 확인 증거\n{rest}"))
}

/// Reject incomplete runtime checks only for submitted code rows in projects with a runtime.
pub(crate) fn runtime_evidence_marker(project_dir: &Path, raw_id: i64, evidence: &str) -> Option<&'static str> {
    let agents = std::fs::read_to_string(project_dir.join("AGENTS.md")).ok()?;
    let runtime = agents.lines().find_map(|line| line.trim().strip_prefix("runtime :").map(str::trim))?;
    if runtime.starts_with("없음") || text_only_row(project_dir, raw_id) { return None; }
    const MARKERS: [&str; 9] = [
        "단위 테스트", "경계·통합 테스트", "통합 테스트만", "로그인 필요", "로그인이 필요",
        "검증 중단", "확인하지 못", "미검증", "mock",
    ];
    let lower = evidence.to_lowercase();
    MARKERS.into_iter().find(|marker| lower.contains(marker))
}

const EVIDENCE_MISSING: &str = "per-goal evidence missing; rerun grist task-confirm with --evidence";

/// Splits a 확인 block into its `- 근거 N : ...` lines and the remaining lines; `None` when no evidence line exists.
fn split_goal_evidence<'a>(lines: impl Iterator<Item = &'a str>) -> Option<(String, String)> {
    let (evidence, rest): (Vec<&str>, Vec<&str>) = lines.partition(|line| line.trim_start().starts_with("- 근거 "));
    if evidence.is_empty() { return None; }
    Some((evidence.join("\n"), rest.join("\n")))
}

fn changes_files(project_dir: &Path) -> Vec<PathBuf> {
    util::glob(&project_dir.join(".ai").join("changes"), "", ".md")
}

fn changes_text(project_dir: &Path) -> String {
    changes_files(project_dir)
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The latest changes record that lists any work; an empty file made by `request-record` is skipped.
fn latest_listing(project_dir: &Path) -> String {
    for record in changes_files(project_dir).iter().rev() {
        let Ok(text) = std::fs::read_to_string(record) else { continue };
        if !section_ids_in_text(&text, "# 완료").is_empty() || !section_ids_in_text(&text, "# 작업").is_empty() {
            return text;
        }
    }
    String::new()
}

/// `# 작업`·`# 완료` headings and their non-empty list lines from `text`.
fn lists_in_text(text: &str) -> String {
    let mut keep = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("# ") {
            keep = matches!(name.trim(), "작업" | "완료");
            if keep {
                out.push(line.trim_end());
            }
        } else if keep && !line.trim().is_empty() {
            out.push(line.trim_end());
        }
    }
    out.join("\n")
}

/// The work and completion lists of the latest changes record that lists any work (`grist lists`).
pub fn lists_text(project_dir: &Path) -> String {
    lists_in_text(&latest_listing(project_dir))
}

/// `# 완료` IDs of the latest changes record that lists any work.
pub fn completed_ids(project_dir: &Path) -> Vec<i64> {
    section_ids_in_text(&latest_listing(project_dir), "# 완료")
}

/// `# 작업` (still open) IDs of the latest changes record that lists any work.
pub fn open_ids(project_dir: &Path) -> Vec<i64> {
    section_ids_in_text(&latest_listing(project_dir), "# 작업")
}

fn clear_gate(project_dir: &Path) -> R<()> {
    for file in util::glob(&project_dir.join(".ai").join("gate"), "", ".json") {
        std::fs::remove_file(&file).map_err(api::other)?;
    }
    Ok(())
}

pub fn section_ids_in_text(text: &str, section: &str) -> Vec<i64> {
    let mut inside = false;
    let mut ids = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == section { inside = true; continue; }
        if trimmed.starts_with("# ") { inside = false; }
        if !inside || !trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) { continue; }
        let Some((_, id)) = trimmed.strip_suffix(')').and_then(|line| line.rsplit_once("(#")) else { continue };
        if let Ok(id) = id.parse::<i64>() {
            if !ids.contains(&id) { ids.push(id); }
        }
    }
    ids
}

/// A `[✒️]` rule/Skill-text row that never reached `task-submit`.
fn text_only_row(project_dir: &Path, raw_id: i64) -> bool {
    text_only_in(&changes_text(project_dir), raw_id)
}

fn text_only_in(text: &str, raw_id: i64) -> bool {
    let suffix = format!("(#{raw_id})");
    let submitted = format!("#### 검증 (#{raw_id})");
    if text.lines().any(|line| line.trim() == submitted) { return false; }
    text.lines()
        .filter(|line| line.starts_with("## [구현] ") && line.trim_end().ends_with(&suffix))
        .last()
        .is_some_and(|line| line.contains("[✒️]"))
}

fn confirmation_output(project_dir: &Path, raw_id: i64) -> R<String> {
    let mut latest = None;
    for path in changes_files(project_dir) {
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        match confirmation_output_in_text(&text, raw_id) {
            Ok(Some(output)) => latest = Some(Ok(output)),
            Err(error) => latest = Some(Err(error)),
            Ok(None) => {}
        }
    }
    latest
        .unwrap_or_else(|| Err("missing confirmation evidence".into()))
        .map_err(|e| Exit::Fail("TEST".into(), format!("#{raw_id} {e}")))
}

fn latest_block(text: &str, heading: &str, reset: Option<&str>) -> Option<Vec<String>> {
    let mut armed = reset.is_none();
    let mut current: Option<Vec<String>> = None;
    let mut latest = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if Some(trimmed) == reset {
            current = None;
            latest = None;
            armed = true;
        } else if trimmed == heading && armed {
            if let Some(lines) = current.take() { latest = Some(lines); }
            current = Some(Vec::new());
        } else if trimmed.starts_with('#') {
            if let Some(lines) = current.take() { latest = Some(lines); }
        } else if let Some(lines) = current.as_mut() {
            if !trimmed.is_empty() { lines.push(trimmed.to_string()); }
        }
    }
    current.or(latest)
}

pub fn confirmation_output_in_text(text: &str, raw_id: i64) -> Result<Option<String>, String> {
    let Some(lines) = latest_block(text, &format!("#### 확인 (#{raw_id})"), None) else { return Ok(None) };
    let Some((evidence, _)) = split_goal_evidence(lines.iter().map(String::as_str)) else { return Err(EVIDENCE_MISSING.into()) };
    let summary = lines.iter().find_map(|line| line.strip_prefix("- 요약 :").map(str::trim))
        .filter(|line| !line.is_empty() && line.contains('⇒'));
    let check = lines.iter().find_map(|line| line.strip_prefix("- [x]").map(str::trim))
        .filter(|line| !line.is_empty());
    let success = lines.iter().any(|line| line == "결과 : 성공");
    let detail = lines.iter().find_map(|line| line.strip_prefix("상세 :").map(str::trim))
        .filter(|line| !line.is_empty() && *line != "-");
    match (summary, check, success, detail) {
        (Some(summary), Some(check), true, Some(detail)) if check.contains("충돌") || detail.contains("충돌") => {
            Ok(Some(format!("# 목표별 근거\n{evidence}\n# 확인 증거\n요약 : {summary}\n검사 : {check}\n결과 : 성공\n상세 : {detail}")))
        }
        _ => Err("confirmation evidence needs summary, successful check, detail, and conflict inspection".into()),
    }
}

pub fn flow_confirmation_in_text(text: &str, raw_id: i64) -> Result<Option<String>, String> {
    let verification = format!("#### 검증 (#{raw_id})");
    if !text.lines().any(|line| line.trim() == verification) { return Ok(None); }
    let Some(lines) = latest_block(text, &format!("#### 확인 (#{raw_id})"), Some(&verification)) else { return Ok(None) };
    let valid = lines.iter().any(|line| line.starts_with("- 요약 :") && line.contains('⇒'))
        && lines.iter().any(|line| line.starts_with("- [x]"))
        && lines.iter().any(|line| line == "결과 : 성공")
        && lines.iter().any(|line| line.starts_with("상세 :") && line.len() > "상세 :".len())
        && !lines.iter().any(|line| line == "결과 : 실패");
    if !valid { return Err("latest flow confirmation lacks successful summary, check, or detail".into()); }
    Ok(Some(lines.join("\n")))
}

pub fn match_goal_count(goal: &str) -> usize {
    goal.lines().filter(|line| {
        let line = line.trim_start();
        let body = if let Some(body) = line.strip_prefix("- ") { body } else if let Some((number, body)) = line.split_once(". ") {
            if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) { return false; }
            body
        } else { return false; };
        ["[연결]", "[기능]", "[수정]"].iter().any(|tag| body.strip_prefix(tag).is_some_and(|text| !text.trim().is_empty()))
    }).count()
}

pub fn require_j1_pass(judgment: &Value, goal_count: usize) -> Result<(), String> {
    if goal_count == 0 { return Err("J1 recognized no goals".into()); }
    let mode = judgment.get("mode").and_then(Value::as_str);
    let valid_shape = judgment.get("ok") == Some(&Value::Bool(true))
        && judgment.get("command").and_then(Value::as_str) == Some("match")
        && matches!(mode, Some("shadow" | "enforce"));
    if !valid_shape { return Err("invalid J1 response".into()); }
    if mode == Some("shadow") { return Ok(()); }
    let Some(items) = judgment.get("items").and_then(Value::as_array) else { return Err("invalid J1 response".into()) };
    if items.len() != goal_count { return Err("invalid J1 response".into()); }
    if judgment.get("verdict").and_then(Value::as_str) != Some("pass") {
        return Err("J1 goal/output mismatch".into());
    }
    for index in 1..=goal_count {
        let name = format!("g{index}");
        let named: Vec<&Value> = items.iter().filter(|item| item.get("name").and_then(Value::as_str) == Some(&name)).collect();
        if named.len() != 1 || named[0].get("verdict").and_then(Value::as_str) != Some("pass") {
            return Err("J1 goal/output mismatch".into());
        }
    }
    Ok(())
}

/// The message a caller outside the CLI reports for a failed step.
pub fn exit_message(error: &Exit) -> String {
    match error {
        Exit::Fail(boundary, message) => format!("{boundary}: {message}"),
        Exit::Gate(payload) => util::compact(payload),
        Exit::Other(message) => message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_keep_only_work_and_done_lines() {
        let text = "# 요청\n## r\n- [✒️] a\n\n# 작업\n1. [📐] b (#6)\n\n# 완료\n1. [✒️] c (#7)\n\n# 진행\n## [구현] [📐] b (#6)\n- 유형 : 개선\n";
        assert_eq!(super::lists_in_text(text), "# 작업\n1. [📐] b (#6)\n# 완료\n1. [✒️] c (#7)");
    }
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Probe {
        name: &'static str,
        log: Rc<RefCell<Vec<String>>>,
        fail_on: Option<Step>,
    }

    impl Probe {
        fn hit(&self, step: Step) -> R<()> {
            self.log.borrow_mut().push(format!("{}:{step:?}", self.name));
            if self.fail_on == Some(step) { api::fail("probe", "down") } else { Ok(()) }
        }
    }

    impl TaskStore for Probe {
        fn name(&self) -> &'static str { self.name }
        fn add(&self, _: &mut TaskCtx) -> R<()> { self.hit(Step::Add) }
        fn ready(&self, _: &mut TaskCtx) -> R<()> { self.hit(Step::Ready) }
        fn submit(&self, _: &mut TaskCtx) -> R<()> { self.hit(Step::Submit) }
        fn check(&self, _: &mut TaskCtx) -> R<()> { self.hit(Step::Check) }
    }

    fn chain(log: &Rc<RefCell<Vec<String>>>, fail: Option<(&'static str, Step)>) -> TaskChain {
        let probe = |name| Box::new(Probe { name, log: log.clone(), fail_on: fail.filter(|f| f.0 == name).map(|f| f.1) }) as Box<dyn TaskStore>;
        TaskChain::new(vec![probe("changes"), probe("grist")])
    }

    #[test]
    fn chain_calls_every_store_in_order_for_each_step() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut ctx = TaskCtx::default();
        for step in [Step::Add, Step::Ready, Step::Submit, Step::Check] {
            chain(&log, None).run(step, &mut ctx).unwrap();
        }
        assert_eq!(log.borrow().as_slice(), [
            "changes:Add", "grist:Add", "changes:Ready", "grist:Ready",
            "changes:Submit", "grist:Submit", "changes:Check", "grist:Check",
        ]);
    }

    #[test]
    fn chain_stops_at_failed_store_and_names_it() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let err = chain(&log, Some(("changes", Step::Check))).run(Step::Check, &mut TaskCtx::default()).unwrap_err();
        assert_eq!(log.borrow().as_slice(), ["changes:Check"]);
        assert_eq!(exit_message(&err), "probe: changes: down");
    }

    #[test]
    fn standard_chain_orders_stores_per_step() {
        let names = |step| TaskChain::standard(step).stores.iter().map(|s| s.name()).collect::<Vec<_>>();
        assert_eq!(names(Step::Add), ["gate", "grist", "changes"]);
        assert_eq!(names(Step::Ready), ["gate", "changes", "grist"]);
        assert_eq!(names(Step::Submit), ["gate", "changes", "grist"]);
        assert_eq!(names(Step::Check), ["changes", "grist"]);
    }

    #[test]
    fn ready_and_submit_results_link_stage_cards() {
        assert_eq!(super::STAGE_WORK_CARD, "~/ai/skills/manage-project/references/stage-work.md");
        assert_eq!(super::STAGE_VERIFY_CARD, "~/ai/skills/manage-project/references/stage-verify.md");
        let mut ctx = TaskCtx { raw_id: 500, stage: "교정".into(), ..Default::default() };
        super::GristStore.ready(&mut ctx).unwrap();
        assert_eq!(ctx.result["next"], super::STAGE_WORK_CARD);
    }

    fn tmp_project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("grist-task-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ai/changes")).unwrap();
        dir
    }

    #[test]
    fn gate_store_never_writes_completion_marker() {
        let dir = tmp_project("gate");
        let mut ctx = TaskCtx { project_dir: Some(dir.clone()), raw_id: 7, prep_kind: "ready", ..Default::default() };
        ctx.prepared.insert("test".into(), json!("cargo test"));
        GateStore.ready(&mut ctx).unwrap();
        ctx.output = "ok".into();
        ctx.changed = 2;
        GateStore.submit(&mut ctx).unwrap();
        let state = gate::gate_load(&dir, 7);
        assert!(!state.contains_key("completed"));
        assert_eq!(state["submit"]["output"], json!("ok"));
        assert_eq!(state["submit"]["changed"], json!(2));
        assert_eq!(state["ready"]["test"], json!("cargo test"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn red_preparation_replaces_previous_gate_state() {
        let dir = tmp_project("red");
        let mut ctx = TaskCtx { project_dir: Some(dir.clone()), raw_id: 8, prep_kind: "ready", ..Default::default() };
        GateStore.ready(&mut ctx).unwrap();
        GateStore.submit(&mut ctx).unwrap();
        ctx.prep_kind = "red";
        GateStore.ready(&mut ctx).unwrap();
        let state = gate::gate_load(&dir, 8);
        assert!(state.contains_key("red") && !state.contains_key("ready") && !state.contains_key("submit"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn completed_ids_use_last_record_that_lists_work() {
        let dir = tmp_project("done");
        std::fs::write(dir.join(".ai/changes/261022.a.md"), "# 작업\n\n# 완료\n1. [✒️] a (#5)\n\n# 진행\n").unwrap();
        std::fs::write(dir.join(".ai/changes/261023.b.md"), "# 작업\n1. [📐] b (#6)\n\n# 완료\n1. [✒️] c (#7)\n\n# 진행\n").unwrap();
        assert_eq!(completed_ids(&dir), vec![7]);
        std::fs::write(dir.join(".ai/changes/261024.c.md"), "# 작업\n\n# 완료\n\n# 진행\n").unwrap();
        assert_eq!(completed_ids(&dir), vec![7]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn gate_add_clears_only_when_latest_listing_has_no_open_item() {
        let dir = tmp_project("clear");
        std::fs::create_dir_all(dir.join(".ai/gate")).unwrap();
        let gate_file = dir.join(".ai/gate/5.json");
        let mut ctx = TaskCtx { project_dir: Some(dir.clone()), ..Default::default() };
        std::fs::write(dir.join(".ai/changes/261022.a.md"), "# 작업\n1. [📐] a (#5)\n\n# 완료\n\n# 진행\n").unwrap();
        std::fs::write(dir.join(".ai/changes/261023.b.md"), "# 작업\n\n# 완료\n\n# 진행\n").unwrap();
        std::fs::write(&gate_file, "{}").unwrap();
        GateStore.add(&mut ctx).unwrap();
        assert!(gate_file.is_file(), "open #5 in the previous day's list keeps gate files");
        std::fs::write(dir.join(".ai/changes/261022.a.md"), "# 작업\n\n# 완료\n1. [📐] a (#5)\n\n# 진행\n").unwrap();
        GateStore.add(&mut ctx).unwrap();
        assert!(!gate_file.exists(), "no open item clears the gate folder");
        GateStore.add(&mut TaskCtx::default()).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn text_only_row_requires_document_symbol_and_no_submit() {
        let doc = "## [구현] [✒️] 규칙 정리 (#9)\n";
        assert!(text_only_in(doc, 9));
        assert!(!text_only_in(&format!("{doc}#### 검증 (#9)\n"), 9));
        assert!(!text_only_in("## [구현] [📐] 체인 (#9)\n", 9));
    }

    #[test]
    fn flow_evidence_uses_latest_successful_confirmation_after_verification() {
        let old = "#### 검증 (#276)\n#### 확인 (#276)\n- 요약 : [🏗️] 이전 ⇒ 결과\n- [x] 이전 검사\n  결과 : 성공\n  상세 : 이전 값\n";
        let current = "#### 검증 (#276)\n#### 확인 (#276)\n- 요약 : [🏗️] hook 입력 ⇒ 판정\n- [x] Stop 연결 검사\n  결과 : 성공\n  상세 : 설치 CLI 실행\n";
        let output = flow_confirmation_in_text(&format!("{old}{current}"), 276).unwrap().unwrap();
        assert!(output.contains("Stop 연결 검사") && !output.contains("이전 검사"));
        assert_eq!(flow_confirmation_in_text(old, 277).unwrap(), None);
        let failed = "#### 검증 (#1)\n#### 확인 (#1)\n- 요약 : [🏗️] a ⇒ b\n- [x] c\n  결과 : 실패\n  상세 : d\n";
        assert!(flow_confirmation_in_text(failed, 1).is_err());
    }

    fn code_state(changed: Option<u64>) -> Map<String, Value> {
        let mut submit = json!({"exit": 0, "output": "focused test passed"});
        if let Some(count) = changed { submit["changed"] = json!(count); }
        json!({"submit": submit}).as_object().unwrap().clone()
    }

    #[test]
    fn light_row_skips_flow_wait_with_confirmation() {
        let dir = tmp_project("light-confirm");
        std::fs::write(dir.join(".ai/changes/261023.light.md"),
            "#### 검증 (#303)\n#### 확인 (#303)\n- 요약 : [🏗️] 입력 ⇒ 결과\n- [x] 최종 소비 확인\n  결과 : 성공\n  상세 : 실제 값 확인\n- 근거 1 : 첫 목표 값\n- 근거 2 : 둘째 목표 값\n").unwrap();
        for count in [1, 2] {
            let evidence = code_evidence(&dir, 303, api::SUBMITTED_STAGE, &code_state(Some(count))).unwrap();
            assert!(evidence.starts_with("# 목표별 근거\n- 근거 1 : 첫 목표 값\n- 근거 2 : 둘째 목표 값\n# 전체 흐름 확인 증거\n"));
            assert!(evidence.contains("최종 소비 확인"));
            assert!(!evidence.contains("focused test passed"));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn code_evidence_rejects_confirmation_without_goal_evidence() {
        let dir = tmp_project("no-goal-evidence");
        std::fs::write(dir.join(".ai/changes/261023.light.md"),
            "#### 검증 (#303)\n#### 확인 (#303)\n- 요약 : [🏗️] 입력 ⇒ 결과\n- [x] 최종 소비 확인\n  결과 : 성공\n  상세 : 실제 값 확인\n").unwrap();
        let err = code_evidence(&dir, 303, api::SUBMITTED_STAGE, &code_state(Some(1))).unwrap_err();
        assert!(exit_message(&err).contains("per-goal evidence missing"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn code_evidence_rejects_unverified_runtime_confirmation() {
        let dir = tmp_project("unverified-runtime");
        std::fs::write(dir.join("AGENTS.md"), "runtime : 브라우저 확장\n").unwrap();
        let path = dir.join(".ai/changes/261023.light.md");
        for marker in ["단위 테스트", "경계·통합 테스트", "통합 테스트만", "로그인 필요", "로그인이 필요", "검증 중단", "확인하지 못", "미검증", "MOCK"] {
            std::fs::write(&path, format!(
                "## [구현] [📐] 코드 (#303)\n#### 검증 (#303)\n#### 확인 (#303)\n- 요약 : 입력 ⇒ 결과\n- [x] 최종 소비 확인\n  결과 : 성공\n  상세 : 실제 값 확인\n- 근거 1 : {marker}\n"
            )).unwrap();
            let err = code_evidence(&dir, 303, api::SUBMITTED_STAGE, &code_state(Some(1))).unwrap_err();
            assert!(exit_message(&err).contains("unverified evidence"), "{marker}");
        }
        std::fs::write(&path,
            "## [구현] [📐] 코드 (#303)\n#### 검증 (#303)\n#### 확인 (#303)\n- 요약 : 입력 ⇒ 결과\n- [x] 최종 소비 확인\n  결과 : 성공\n  상세 : 로그인 필요\n- 근거 1 : 실제 파일 확인\n").unwrap();
        assert!(exit_message(&code_evidence(&dir, 303, api::SUBMITTED_STAGE, &code_state(Some(1))).unwrap_err()).contains("unverified evidence"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn three_files_still_need_flow_wait() {
        let dir = tmp_project("three-files");
        let err = code_evidence(&dir, 303, api::SUBMITTED_STAGE, &code_state(Some(3))).unwrap_err();
        assert!(exit_message(&err).contains("flow-wait evidence missing"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn zero_files_uses_normal_path() {
        let dir = tmp_project("zero-files");
        for count in [Some(0), None] {
            let err = code_evidence(&dir, 303, api::SUBMITTED_STAGE, &code_state(count)).unwrap_err();
            assert!(exit_message(&err).contains("flow-wait evidence missing"));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn confirmation_output_requires_successful_conflict_evidence() {
        let ok = "#### 확인 (#3)\n- 요약 : [✒️] 규칙 ⇒ 갱신\n- [x] 충돌 검사\n  결과 : 성공\n  상세 : 하위 AGENTS.md 대조\n- 근거 1 : 규칙 줄 갱신\n";
        let output = confirmation_output_in_text(ok, 3).unwrap().unwrap();
        assert!(output.starts_with("# 목표별 근거\n- 근거 1 : 규칙 줄 갱신\n# 확인 증거\n요약 : ") && output.contains("충돌 검사"));
        let no_evidence = "#### 확인 (#3)\n- 요약 : [✒️] 규칙 ⇒ 갱신\n- [x] 충돌 검사\n  결과 : 성공\n  상세 : 하위 AGENTS.md 대조\n";
        assert!(confirmation_output_in_text(no_evidence, 3).unwrap_err().contains("per-goal evidence missing"));
        let no_conflict = "#### 확인 (#3)\n- 요약 : [✒️] 규칙 ⇒ 갱신\n- [x] 문구 검사\n  결과 : 성공\n  상세 : 확인\n- 근거 1 : 값\n";
        assert!(confirmation_output_in_text(no_conflict, 3).is_err());
        assert_eq!(confirmation_output_in_text(ok, 4).unwrap(), None);
    }

    #[test]
    fn j1_shadow_passes_and_enforce_requires_each_goal_pass() {
        assert_eq!(match_goal_count("- [기능] a\n2. [수정] b\n- 메모\n3. [연결]"), 2);
        let base = |mode: &str, verdict: &str, items: Value| json!({"ok": true, "command": "match", "mode": mode, "verdict": verdict, "items": items});
        assert!(require_j1_pass(&base("shadow", "uncertain", json!([])), 1).is_ok());
        let pass = json!([{"name": "g1", "verdict": "pass"}, {"name": "g2", "verdict": "pass"}]);
        assert!(require_j1_pass(&base("enforce", "pass", pass), 2).is_ok());
        let one_fail = json!([{"name": "g1", "verdict": "pass"}, {"name": "g2", "verdict": "fail"}]);
        assert!(require_j1_pass(&base("enforce", "pass", one_fail), 2).is_err());
        assert!(require_j1_pass(&json!({"ok": false, "boundary": "jev-timeout"}), 1).is_err());
        assert!(require_j1_pass(&base("enforce", "pass", json!([])), 0).is_err());
    }
}
