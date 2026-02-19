---
name: build_feature_add
description: "기존 프로젝트에서 input.txt 기반으로 기능 추가를 수행하는 스킬."
version: 2026-02-19
---

# Build Feature Add

## Trigger
- 사용자가 기존 프로젝트에 기능 추가를 요청하면 실행한다.
- 예: "기능 추가해", "기존 프로젝트에 ~~ 기능 넣어줘", "input.txt로 기능 반영해줘"

## Goal
- 기존 프로젝트를 재초기화(bootstrap)하지 않고 기능 추가만 수행한다.
- `input.txt`(또는 `input.yaml`) 기반으로 `tasks.yaml`을 생성/검토하고 구현까지 연결한다.

## Shared Skill Dependency
- 공통 규칙은 `/home/tree/ai/skills/feature_architecture_rules/SKILL.md`를 반드시 함께 적용한다.
- 이 스킬에서는 기존 프로젝트 기능 추가 흐름(`input -> save_request_by_file -> build_task`)만 다룬다.

## Scope Rule
- 이 스킬은 **기존 프로젝트 기능 추가 전용**이다.
- 신규 프로젝트 bootstrap이 필요하면 `build_auto`를 사용한다.

## Required Flow
1. 프로젝트 기본 설정 확인
```bash
mono init -l "<language>"
# 또는 mono config로 main/language 확인
```

2. 사전 점검 (유사 기능/중복 구현 탐색)
```bash
rg -n "<기능 키워드>" src
```
- 기존 구현이 있으면 확장/재사용 우선으로 설계한다.

3. 입력 파일 준비
- 기본: `./input.txt`
- 우선순위: `./input.yaml`이 있으면 yaml을 먼저 사용
- `input.txt`를 yaml로 바꾸려면:
```bash
mono input_convert ./input.txt ./input.yaml
```

4. domain/tasks 생성
```bash
mono save_request_by_file
```

5. build 실행
```bash
mono build_task
```

## Hard Rules
- bootstrap 단계(`bun create`, `cargo new` 등)를 수행하지 않는다.
- `mono save_request_by_file` 전에 `input` 파일을 반드시 준비한다.
- `mono build_task`는 `mono save_request_by_file` 완료 후에만 실행한다.
- `tasks.yaml`은 재생성될 수 있으므로 기존 파일 보존이 필요하면 사전 백업한다.

## Completion Check
- `tasks.yaml`이 생성/갱신되었는가?
- `language/main`이 프로젝트 기대값과 일치하는가?
- 기능 구현 후 엔트리 wiring 점검 단계까지 완료되었는가?
