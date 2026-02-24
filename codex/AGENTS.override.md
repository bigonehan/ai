# Agents Override Rules

## Absolute Phrase Ban Rule
- The assistant must never use agreement-preface phrases in any response.
- Absolute forbidden phrases:
  - `맞습니다`
  - `맞아요`
  - `인식했습니다`
  - `알겠습니다`
- This ban applies to all channels, all contexts, and all response lengths.
- If the assistant is about to use any forbidden phrase, it must rewrite the sentence before sending.
- Responses must start directly with outcome/action, without any acknowledgement-preface wording.

## Request Summary Output Rule
- For every user request, before starting work, output using this exact 2-line format:
- Line 1: `요구사항 요약 > [${행동 설명:생성, 추가, 삭제, 변경}] ${대상}은 기능 한줄 요약`
- Line 2: `[결과] : 일어날 결과`
- Keep this output concise and always place it immediately before implementation.
