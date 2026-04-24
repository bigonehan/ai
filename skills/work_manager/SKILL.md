---
name: work-manager
description: "work_helper 프로젝트에서 src/cli.ts의 단계별 함수와 handleRequest를 사용해 init, project.md, job.md, draft, tmux build/check, improve 루프를 오케스트레이션하는 skill"
---

# Work Manager

`/home/tree/project/work_helper` 저장소에서 작업 관리 흐름을 실행하거나 조합할 때 이 스킬을 사용한다.
이 스킬의 목적은 사람이 직접 세부 단계를 설명하지 않아도, LLM이 `src/cli.ts`에 공개된 함수들을 이용해 `request -> init -> plan -> analyze -> build -> check -> improve` 흐름을 안전하게 다루게 하는 것이다.

다른 저장소에서는 기본적으로 사용하지 않는다.

## 언제 사용할지
- 새 작업 요청을 받아 `.project/project.md`를 만들고 bootstrap까지 이어야 할 때
- 기존 요청에서 `job.md`를 만들고 draft 분해 후 tmux 작업을 실행해야 할 때
- build/check를 개별 단계로 제어하거나, `check` 뒤 `improve` 재진입 여부를 판단해야 할 때
- 상위 오케스트레이터 `handleRequest` 하나로 전체 흐름을 돌리고 싶을 때

## 기본 원칙
- 우선 `src/cli.ts`와 `src/types.ts`를 source of truth로 본다.
- 함수 시그니처나 반환 구조가 애매하면 추측하지 말고 해당 파일을 다시 읽는다.
- `runAnalyzeStep`은 draft 생성까지만 한다. build를 시작하지 않는다.
- `runBuildStep`은 draft 의존성과 priority를 반영해 tmux 작업을 제출한다.
- `runCheckStep`은 별도 세션에서 최종 검증을 수행한다.
- `runImproveStep`은 `job.md` 보고를 읽고 개선 루프 재진입용 요청을 만든다.
- 전체 자동 실행이 필요하면 `handleRequest`를 우선 고려한다.

## 권장 호출 순서
1. workspace 상태를 기준으로 `runRequestStep(request, workspaceDir)`를 실행한다.
2. 결과가 `request->init` 또는 `request->import-project`면 `runInitStep(...)`을 실행한다.
3. 이후 `runPlanStep(...)`으로 `job.md`를 만든다.
4. `runAnalyzeStep(...)`으로 draft YAML들을 생성한다.
5. `runBuildStep(...)`으로 draft tmux 작업들을 실행한다.
6. build가 통과하면 `runCheckStep(...)`으로 최종 검증을 수행한다.
7. check가 멈추면 `runImproveStep(...)`으로 후속 개선 요청을 만든다.
8. 전체를 자동화하려면 위 단계들을 수동 조합하는 대신 `handleRequest(...)`를 사용한다.

## 함수 선택 가이드
- 새 프로젝트를 가능한 적은 호출로 진행:
  - `handleRequest`
- 개별 단계 디버깅:
  - `runPlanStep`, `runAnalyzeStep`, `runBuildStep`, `runCheckStep`
- bootstrap만 재실행:
  - `runProjectBootstrapStep`
- check 이후 개선 요청 텍스트만 만들기:
  - `runImproveStep`

## 읽어야 할 reference
- 함수별 입력/출력과 사용 예시는 `references/cli-api.md`를 읽는다.
- export 경로는 `/home/tree/project/work_helper/src/index.ts`를 확인한다.

## 최소 사용 예시
```ts
import { handleRequest } from "/home/tree/project/work_helper/src/cli";

const result = await handleRequest({
  projectId: "demo-project",
  projectType: "code",
  request: "게시물 삭제 기능 추가",
  workspaceDir: "/home/tree/project/work_helper",
  provider: "codex",
  bootstrap: false,
  maxImproveIterations: 1,
});
```

## 주의사항
- 이 스킬은 설명서다. 실제 실행 전에는 provider, workspaceDir, projectType이 현재 작업에 맞는지 확인한다.
- 실제 tmux를 쓰지 않는 검증이나 단위 테스트에서는 `runner`를 주입할 수 있다.
- `handleRequest`는 기본적으로 개선 루프를 1회만 더 돈다. 더 많은 반복이 필요하면 `maxImproveIterations`를 명시한다.
