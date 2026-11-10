# 사용자 입력 (버튼·텍스트·업로드·재생·다운로드)

- Start from the real visible control (button, editor, upload, playback, download) rather than a popup, helper endpoint, synthetic event or internal function.
- For a download or file consumer, list the destination before the flow and require a new file afterwards with the expected name, positive size, settled modification time and readable bytes. Resolve the real download folder from the running profile; an existing file with a matching name is not a result.
