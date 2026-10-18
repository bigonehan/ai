# ~/ai/scripts

에이전트가 쓰는 공용 스크립트(Rust). 여러 Skill·hook·전역 규칙이 함께 부르는 것만 둔다.
한 Skill 안에서만 쓰는 스크립트는 그 Skill의 `scripts/<이름>/`(crate)와 `scripts/bin/<이름>`(바이너리)에 둔다.

- 빌드: `cargo build --release` (이 폴더) → `bin/<이름>`은 `target/release/<이름>` 링크
- 호출: `~/ai/scripts/bin/<이름> …`
- 예외: access-stagehand는 Node Stagehand 라이브러리라 `stagehand.mjs`로 남는다
