# A. TASK COMPLETION GATE
작업 완료 시에만 적용. 완료 후 final 응답 전에 실행한다.
STEP 1. nf -m "<task-name> complete" 
실행 출력: [GATE A-1] exit: <code>
- code != 0 → [GATE A-1] HALT 출력 후 중단. final 응답 없음.
- code = 0 → STEP 2

STEP 2. → DENIAL SCAN (B) 실행

# B. DENIAL SCAN
모든 final 응답 전에 적용.
draft에서 아래 표현을 검사하고 결과를 출력한다:
[GATE B]
맞습니다   : 있음 / 없음
맞아요     : 있음 / 없음
인식했습니다: 있음 / 없음
알겠습니다  : 있음 / 없음
그렇습니다  : 있음 / 없음

"있음" 항목 존재 → 해당 문장 전체 재작성 후 B 재실행
모두 "없음" → [GATE B] PASS 출력 후 final 응답 전송
