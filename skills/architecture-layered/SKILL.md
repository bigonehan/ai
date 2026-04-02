---
name: architecture-layered
description: presentation -> application -> domain -> infrastructure 계층 구조를 ORC draft/check 단계에 주입하는 architecture skill.
---

# Layered Architecture

## ORC Architecture Contract

### Constraints
- `src/domain/** -> src/infrastructure/** import 금지`
- `src/domain/** -> DB/API/파일시스템 직접 접근 금지`
- `src/presentation/** -> repository 구현체 직접 import 금지`
- `repository 구현체 생성은 bootstrap/di 조립 지점에서만 허용`

### Checks
- `domain -> infrastructure 역참조 0건 확인`
- `presentation -> repository implementation 직접 참조 0건 확인`
- `요청 흐름이 presentation -> application -> domain -> infrastructure 순서를 유지하는지 확인`
