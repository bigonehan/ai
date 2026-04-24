# CLI API Reference

대상 파일:
- `/home/tree/project/work_helper/src/cli.ts`
- `/home/tree/project/work_helper/src/types.ts`
- `/home/tree/project/work_helper/src/index.ts`

## Public functions

### `runRequestStep(request, workspaceDir)`
- 목적: workspace 상태를 읽고 요청을 `request->init`, `request->import-project`, `request->plan`, `request->check` 중 하나로 분기한다.
- 입력:
  - `request: string`
  - `workspaceDir: string`
- 반환:
  - `stage`
  - `transition`
  - `hasProjectMetadata`
  - `workspaceEmpty`
  - `hasSourceFiles`
- 사용 시점:
  - 전체 흐름 시작점
  - `init` 필요 여부 판단

### `runInitStep(input)`
- 목적: `.project/project.md`를 만들고 필요하면 bootstrap까지 실행한다.
- 주요 입력:
  - `projectId`
  - `projectType`
  - `request`
  - `workspaceDir`
  - `provider`
  - `bootstrap?: boolean`
- 반환:
  - `projectFilePath`
  - `projectDocument`
  - `projectSpec`
  - `bootstrap`
- 주의:
  - `bootstrap: false`면 metadata만 만들고 bootstrap은 생략한다.

### `runProjectBootstrapStep(input)`
- 목적: 이미 존재하는 `project.md` 기준으로 bootstrap prompt를 만들고 tmux bootstrap job을 실행한다.
- 반환:
  - `metadata`
  - `snapshot`
  - `verification`
- 사용 시점:
  - bootstrap만 다시 돌릴 때

### `runPlanStep(input)`
- 목적: 요청 기반으로 `job.md`를 생성한다.
- 주요 반환:
  - `timestamp`
  - `summary`
  - `projectSpec`
  - `projectFilePath`
  - `jobFilePath`
  - `jobDocument`
- 사용 시점:
  - `job.md`를 만들고 이후 analyze/build를 수동 제어할 때

### `runAnalyzeStep(input)`
- 목적: `job.md`를 읽고 draft yaml 파일들을 생성한다.
- 주요 입력:
  - `timestamp`
  - `summary`
  - `jobFilePath`
  - `jobDocument?`
  - `projectSpec?`
- 주요 반환:
  - `draftsDir`
  - `drafts`
- 중요:
  - 이 함수는 build를 실행하지 않는다.

### `runBuildStep(input)`
- 목적: analyze 결과 draft들을 dependency/priority 순서로 tmux 작업에 제출한다.
- 주요 입력:
  - `timestamp`
  - `summary`
  - `jobFilePath`
  - `drafts`
  - `runner?`
- 주요 반환:
  - `executions`
  - `failedExecution`
  - `decision`
  - `reason`
- 중요:
  - 실제 tmux 대신 테스트 더블 runner를 주입할 수 있다.

### `runCheckStep(input)`
- 목적: 별도 tmux 세션에서 최종 check를 수행한다.
- 주요 입력:
  - `timestamp`
  - `summary`
  - `jobFilePath`
  - `verifyCompletion?`
  - `runner?`
- 주요 반환:
  - `jobId`
  - `prompt`
  - `snapshot`
  - `verification`
  - `decision`
  - `reason`

### `runImproveStep(input)`
- 목적: `check` 결과와 `job.md` 보고를 바탕으로 개선 루프 재진입용 요청을 만든다.
- 주요 입력:
  - `jobFilePath`
  - `check`
- 주요 반환:
  - `decision`
  - `reason`
  - `nextRequest`
  - `report`
- 규칙:
  - `check.decision === "complete"`면 `halt`
  - 아니면 `continue`와 함께 후속 개선 요청 문자열 생성

### `handleRequest(input)`
- 목적: 상위 오케스트레이터. `request -> init -> plan -> analyze -> build -> check -> improve`를 실행한다.
- 주요 입력:
  - `projectId`
  - `projectType`
  - `request`
  - `workspaceDir`
  - `provider`
  - `bootstrap?`
  - `runner?`
  - `verifyCompletion?`
  - `maxImproveIterations?`
- 주요 반환:
  - `request`
  - `init`
  - `cycles`
  - `finalDecision`
  - `finalReason`
- 권장 상황:
  - 사람이 "이 요청 처리해"라고 말했을 때 LLM이 내부적으로 전체 흐름을 수행해야 하는 경우

## Typical patterns

### 1. Full request handling
```ts
const result = await handleRequest({
  projectId,
  projectType: "code",
  request,
  workspaceDir,
  provider: "codex",
  bootstrap: true,
  maxImproveIterations: 1,
});
```

### 2. Manual stage control
```ts
const requestStep = await runRequestStep(request, workspaceDir);
if (requestStep.transition === "request->init") {
  await runInitStep({ projectId, projectType: "code", request, workspaceDir, provider: "codex" });
}

const plan = await runPlanStep({ projectId, projectType: "code", request, workspaceDir, provider: "codex" });
const analyze = await runAnalyzeStep({
  projectId,
  projectType: "code",
  request,
  workspaceDir,
  provider: "codex",
  timestamp: plan.timestamp,
  summary: plan.summary,
  projectSpec: plan.projectSpec,
  jobFilePath: plan.jobFilePath,
  jobDocument: plan.jobDocument,
});
const build = await runBuildStep({
  projectId,
  projectType: "code",
  request,
  workspaceDir,
  provider: "codex",
  timestamp: analyze.timestamp,
  summary: analyze.summary,
  jobFilePath: analyze.jobFilePath,
  drafts: analyze.drafts,
});
```

## Decision guide
- init 필요 여부만 알고 싶다:
  - `runRequestStep`
- `project.md`만 만들고 bootstrap은 미룬다:
  - `runInitStep({ ..., bootstrap: false })`
- `job.md`와 draft만 만들고 실제 실행은 나중에 한다:
  - `runPlanStep` + `runAnalyzeStep`
- check 실패 후 후속 개선용 프롬프트를 만든다:
  - `runImproveStep`
- 전체를 한 번에 관리한다:
  - `handleRequest`
