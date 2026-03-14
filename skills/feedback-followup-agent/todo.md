# problem
`feedback-followup-agent`의 `SKILL.md`가 invalid YAML로 판정되어 스킬 로더에서 건너뛰어진다.

# tasks
- `.../feedback-followup-agent/SKILL.md`의 front matter와 정상적으로 로드되는 다른 skill의 헤더를 비교해 파싱 실패 지점을 찾는다.
- YAML front matter만 최소 수정해 로더가 읽을 수 있게 고친다.
- 가능한 범위에서 YAML 파싱 또는 로더 재실행으로 스킬이 더 이상 skip되지 않는지 검증한다.

# check
- `sed -n '1,20p' /home/tree/ai/skills/feedback-followup-agent/SKILL.md`
- `python` 또는 기존 로더 경로로 front matter 파싱 재검증
