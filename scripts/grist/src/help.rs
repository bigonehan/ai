use grist::{gate, records};

// Required, optional and example are kept together so the command list and detail stay aligned.
pub struct CommandHelp {
    pub name: &'static str,
    pub required: &'static str,
    pub optional: &'static str,
    pub example: &'static str,
}

pub const COMMANDS: &[CommandHelp] = &[
    CommandHelp { name: "tables", required: "없음", optional: "없음", example: "grist tables" },
    CommandHelp { name: "columns", required: "TABLE", optional: "없음", example: "grist columns Table26" },
    CommandHelp { name: "get", required: "TABLE", optional: "--id INT (반복 가능), --filter JSON 객체, --fields 쉼표 구분 필드", example: "grist get Table26 --id 518 --fields H,K" },
    CommandHelp { name: "add", required: "TABLE --fields JSON 객체", optional: "없음", example: "grist add Table7 --fields '{\"C\":\"예시\"}'" },
    CommandHelp { name: "update", required: "TABLE --id INT --fields JSON 객체", optional: "없음", example: "grist update Table7 --id 1 --fields '{\"C\":\"예시\"}'" },
    CommandHelp { name: "delete", required: "TABLE --id INT", optional: "--project DIR (Table26 행이면 그 프로젝트 changes의 행 줄·블록도 지움)", example: "grist delete Table26 --id 690 --project ." },
    CommandHelp { name: "column-update", required: "TABLE COLUMN --fields JSON 객체", optional: "없음", example: "grist column-update Table7 C --fields '{\"label\":\"이름\"}'" },
    CommandHelp { name: "project", required: "ROW_ID (정수)", optional: "없음", example: "grist project 1198" },
    CommandHelp { name: "task", required: "ROW_ID (정수)", optional: "--project DIR, --cached (값 없음)", example: "grist task 518 --project /home/tree/extensions" },
    CommandHelp { name: "task-update", required: "ROW_ID (정수) --fields JSON 객체", optional: "--project DIR", example: "grist task-update 518 --project . --fields '{\"D\":\"[🚥] 점검\"}'" },
    CommandHelp { name: "task-add", required: "--fields JSON 객체", optional: "--project DIR (지정 시 --goal-file FILE --summary 한 줄 --type 기능 추가|개선|오류 해결 필수), --judge 한 줄", example: "grist task-add --project . --fields '{\"B\":\"작업\"}' --type 개선 --goal-file goals.txt --summary '한 줄'" },
    CommandHelp { name: "gate", required: "없음", optional: "--cwd DIR, --row INT", example: "grist gate --cwd . --row 518" },
    CommandHelp { name: "lists", required: "--project DIR", optional: "없음", example: "grist lists --project ." },
    CommandHelp { name: "request", required: "--project DIR", optional: "없음", example: "grist request --project ." },
    CommandHelp { name: "goal", required: "ROW_ID (정수) --project DIR", optional: "없음", example: "grist goal 518 --project ." },
    CommandHelp { name: "task-goal", required: "ROW_ID (정수) --project DIR --goal-file FILE --summary 한 줄", optional: "없음", example: "grist task-goal 518 --project . --goal-file goals.txt --summary '한 줄'" },
    CommandHelp { name: "task-red", required: "ROW_ID (정수) --project DIR --purpose 한 줄 --test CMD --tests 경로 목록", optional: "--paths 최상위 경로 목록, --detail 한 줄, --agent-id ID", example: "grist task-red 518 --project . --purpose 재현 --test 'cargo test help' --tests src/main.rs --paths src" },
    CommandHelp { name: "task-ready", required: "ROW_ID (정수) --project DIR --purpose 한 줄 --test CMD --tests 경로 목록 --paths 최상위 경로 목록", optional: "--agent-id ID", example: "grist task-ready 518 --project . --purpose 확인 --test 'cargo test help' --tests src/main.rs --paths src" },
    CommandHelp { name: "task-hidden", required: "ROW_ID (정수) --project DIR --purpose 한 줄 --test CMD", optional: "--detail 한 줄", example: "grist task-hidden 518 --project . --purpose 숨은 사례 --test 'cargo test help'" },
    CommandHelp { name: "task-submit", required: "ROW_ID (정수) --project DIR", optional: "없음", example: "grist task-submit 518 --project ." },
    CommandHelp { name: "flow-wait", required: "--project DIR", optional: "--row ROW_ID (지정 행만 확인)", example: "grist flow-wait --project . --row 518" },
    CommandHelp { name: "task-show", required: "ROW_ID (정수) --project DIR", optional: "--section SECTION", example: "grist task-show 518 --project . --section 목표" },
    CommandHelp { name: "task-memo", required: "ROW_ID (정수) --project DIR --line '<11자리 ID> · <세션 식별자>'", optional: "없음", example: "grist task-memo 518 --project . --line '00119805181 · 4910ec9c'" },
    CommandHelp { name: "improve-entry", required: "append|done|mark|fail ROW_ID (정수) --project DIR (append·done: --text 한 줄)", optional: "append: --recurrence 이전 행", example: "grist improve-entry append 712 --project . --text '- [ ] 원인 (from /repo#29)'" },
    CommandHelp { name: "task-step", required: "ROW_ID (정수) --project DIR --step ▷|▶ --text 한 줄 --goal 목표 줄 번호", optional: "없음", example: "grist task-step 518 --project . --step ▶ --text '구현 확인' --goal 1" },
    CommandHelp { name: "task-confirm", required: "ROW_ID (정수) --project DIR --summary 한 줄 --check 한 줄 --detail 한 줄 --evidence FILE", optional: "--can 한 줄, --partial --reason 사유, --docs 쉼표 구분 .md 경로", example: "grist task-confirm 518 --project . --summary '[✒️] 입력 ⇒ 결과' --check 확인 --detail 성공 --evidence evidence.txt" },
    CommandHelp { name: "request-record", required: "--project DIR --input-file FILE --title 제목 --summary 한 줄", optional: "없음", example: "grist request-record --project . --input-file request.txt --title 도움말 --summary '요청 요약'" },
    CommandHelp { name: "sync", required: "없음", optional: "--project DIR, --cwd DIR", example: "grist sync --project ." },
];

pub fn render(command: Option<&str>) -> Result<String, String> {
    let Some(command) = command else {
        return Ok(COMMANDS.iter().map(|c| c.name).collect::<Vec<_>>().join("\n"));
    };
    let Some(spec) = COMMANDS.iter().find(|c| c.name == command) else {
        return Err(format!("invalid choice: '{command}'"));
    };
    let mut lines = vec![
        format!("usage: grist {} [options]", spec.name),
        format!("필수: {}", spec.required),
        format!("선택: {}", spec.optional),
    ];
    match command {
        "task-update" => {
            lines.push(format!("값 형식: $D/$J: {} 또는 $J: [추정] <한 줄>; $I: 한 줄; $H: 초안|진단|작업|교정|검증|확인|완료", records::error_line_format()));
            lines.push("규칙: J(원인)를 기록할 때 같은 --fields에 비어 있지 않은 C(근거)가 필요".into());
        }
        "task-add" => lines.push("규칙: J(원인)를 기록할 때 같은 --fields에 비어 있지 않은 C(근거)가 필요".into()),
        "task-confirm" => {
            lines.push(format!("--can: {}", records::CAN_FORMAT));
            lines.push(format!("--evidence: {}", records::EVIDENCE_FORMAT));
        }
        "flow-wait" => lines.push("조건: 각 검증 행에 task-hidden 뒤 최신 task-confirm 필요".into()),
        "task-red" | "task-ready" => lines.push(format!("값 형식: --purpose {}; --tests 쉼표 구분 파일; --paths {}", gate::PURPOSE_FORMAT, gate::PATHS_FORMAT)),
        "task-hidden" => lines.push(format!("값 형식: --purpose {}", gate::PURPOSE_FORMAT)),
        "task-submit" => {
            lines.push("[🌍] Bridge 연동 소스 변경 시 테스트에서 <bridge>/contracts/samples/<계약>/NN-<설명>.json 참조 필요".into());
            lines.push("Grist records/columns 코드 변경 시 tests/fixtures/grist-<table>-columns.json 필요: {docId,tableId,capturedAt,columns:GET /api/docs/<doc>/tables/<table>/columns 응답의 columns,payload:{records:[{fields:{...}}]}}; 등록 테스트에서 샘플을 읽어 실제 전송 fields와 대조. 제출 때 실제 Grist 열·타입과 다시 비교".into());
        },
        "task-step" => lines.push("값 형식: --step ▷|▶, --goal 1..목표 줄 수".into()),
        _ => {}
    }
    lines.push(format!("예시: {}", spec.example));
    Ok(lines.join("\n"))
}
