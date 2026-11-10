# runtime: cli

- 실제 명령을 실행해 성공 출력을 확인한 뒤 `[verify-step] cli-run cmd=<실행 명령>`을 출력한다. 이 줄이 Stop gate의 실행 기록이다.
- readback은 명령 출력이 아니라 그 명령이 바꾼 최종 소비자(파일·DB·Grist 행 등)에서 직접 읽는다.
