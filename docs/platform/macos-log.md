# macOS 실행 기록

개발 기기. 새 기록은 위에 추가한다. 비교표는 [`README.md`](README.md).
Windows 기록과 같은 항목(명령·exit 코드·경로)을 남겨 비교할 수 있게 한다.

## 미확인 항목 (Windows에서 확인했고 macOS 값이 비어 있는 것)

- `kiro-cli chat --v2 --no-interactive /usage` 소요 시간과 출력 형식(Windows: 21초, 일반 텍스트)
- Kiro 데이터 위치(`data.sqlite3`에 해당하는 파일)
- Codex rollout 실제 경로와 `.jsonl.zst` 파일 존재 여부, `token_count.rate_limits` 형식이 Windows와 같은지
- 같은 프롬프트(`codex exec "Reply with exactly: ok"`)의 `total_token_usage` — Windows: input 17,024 / cached 13,440 / output 5
- corepack 사용 여부(Node 25에는 들어 있지 않다. README "Corepack 사용 가능"과 맞는지)
