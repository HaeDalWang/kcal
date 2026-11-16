# 플랫폼 비교 (macOS ↔ Windows)

kcal이 OS마다 다르게 읽거나 다르게 동작해야 하는 것을 모은다. 개발은 macOS, Windows는 실기 테스트(레노버 노트북).

- 이 표는 **항목별 최신 값**만 둔다. 값을 바꾸면 근거 날짜도 바꾼다.
- 실행 기록(명령·exit 코드·원인)은 OS별 로그에 날짜순으로 쌓는다: [`windows-log.md`](windows-log.md), [`macos-log.md`](macos-log.md).
- 표기: **확인됨**(그 기기에서 직접 확인) / **추정**(근거만 있음) / **모름**.

## 에이전트 인증·데이터 위치 (1층 측정·한도 조회에 필요)

| 항목                     | macOS                                               | Windows                                                                                                                                                       | 근거           |
| ------------------------ | --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- |
| Claude Code OAuth 토큰   | Keychain `Claude Code-credentials` (확인됨, 0003)   | **평문 파일** `%USERPROFILE%\.claude\.credentials.json`, 키 `claudeAiOauth.{accessToken, refreshToken, expiresAt, subscriptionType, rateLimitTier}` (확인됨)  | win 2026-11-03 |
| Codex 인증               | `~/.codex/auth.json` (0003)                         | `%USERPROFILE%\.codex\auth.json`, 키 `auth_mode`, `tokens.{access_token, account_id, …}`, `last_refresh` (확인됨)                                             | win 2026-11-03 |
| Codex rollout(세션 로그) | `~/.codex/sessions/` (0003, `.jsonl.zst` 압축 있음) | `%USERPROFILE%\.codex\sessions\YYYY\MM\DD\rollout-*.jsonl`, `token_count` 이벤트에 `rate_limits`(5h·7일, `resets_at` epoch) 있음 (확인됨). `.zst`는 아직 없음 | win 2026-11-03 |
| Codex CLI                | 설치됨                                              | 데스크톱 앱은 `codex` 명령을 주지 않는다 → `npm i -g @openai/codex` (0.166.0), 인증 공유 (확인됨)                                                             | win 2026-11-03 |
| Kiro CLI                 | 동작, `/usage` 나옴(사용자 확인)                    | `kiro-cli` 2.37.1 `%LOCALAPPDATA%\Kiro-Cli\kiro-cli.exe` 동작(확인됨). `/usage` 동작(확인됨)                                                                  | win 2026-11-03 |
| Kiro 데이터              | 모름                                                | `%LOCALAPPDATA%\Kiro-Cli\data.sqlite3`, 설정 `%USERPROFILE%\.kiro\{settings,skills,hooks}` (확인됨)                                                           | win 2026-11-03 |

## 개발 환경

| 항목          | macOS              | Windows                                                                     | 근거           |
| ------------- | ------------------ | --------------------------------------------------------------------------- | -------------- |
| Node          | 25.8.1             | 25.8.1 (winget `OpenJS.NodeJS`)                                             | win 2026-11-03 |
| pnpm          | 12.15.0            | 12.15.0 — Node 25에 corepack이 없어 `npm i -g pnpm@12.15.0`로 설치(확인됨)  | win 2026-11-03 |
| Rust          | 1.99.1             | 1.99.1 stable-x86_64-pc-windows-msvc (winget `Rustlang.Rustup`)             | win 2026-11-03 |
| C++ 빌드 도구 | Xcode CLT          | VS 2022 BuildTools, VCTools 워크로드                                        | win 2026-11-03 |
| WebView       | WKWebView(OS 내장) | WebView2 126.0.2592.113 (기본 설치)                                         | win 2026-11-03 |
| 시간 동기화   | 자동               | `w32time`이 꺼져 있어 시계가 21분 빨랐다 → AWS(Kiro) 로그인 실패. 켜서 해결 | win 2026-11-03 |

## 검증 결과

| 항목                                           | macOS              | Windows                                                                                                                     | 근거           |
| ---------------------------------------------- | ------------------ | --------------------------------------------------------------------------------------------------------------------------- | -------------- |
| `pnpm install --frozen-lockfile`               | 통과               | 통과                                                                                                                        | win 2026-11-03 |
| `typecheck` / `lint` / `format:check` / `test` | 통과               | 통과(테스트 7/7)                                                                                                            | win 2026-11-03 |
| `pnpm rust:check`                              | 통과               | 통과(Rust 테스트 9/9, 첫 빌드 409초)                                                                                        | win 2026-11-03 |
| `pnpm tauri:dev:mcp` 실행·MCP bridge           | 확인               | 확인(127.0.0.1:9223, 첫 dev 빌드 1분 20초)                                                                                  | win 2026-11-03 |
| `compute_stats` IPC(성공·오류)                 | 확인               | 확인(UI 버튼 + MCP `ipc-execute-command`)                                                                                   | win 2026-11-03 |
| 라이트·다크·system 테마                        | 확인               | 확인(system은 OS 다크를 따라감)                                                                                             | win 2026-11-03 |
| WebView 엔진                                   | WKWebView          | WebView2, UA `Edg/131.0.0.0`                                                                                                | win 2026-11-03 |
| 유휴 CPU(dev 앱, 10초)                         | 모름               | 0초, WorkingSet 29.7MB                                                                                                      | win 2026-11-03 |
| Kiro `/usage`                                  | 동작               | 동작, 21초, 일반 텍스트 출력                                                                                                | win 2026-11-03 |
| `pnpm rust:check`(`e30cf91`)                   | 통과               | 125/126 — 임시 폴더가 홈 아래라 테스트 1개가 실제 `~\.claude\CLAUDE.md`를 집음                                              | win 2026-11-05 |
| 트레이 아이콘·툴팁                             | 확인               | 확인(처음엔 숨겨진 아이콘 안, 툴팁에 세 에이전트 %)                                                                         | win 2026-11-05 |
| 팝오버 위치·탭 4개·한도 조회                   | 확인               | 확인(작업 표시줄 위 오른쪽 아래, 실제 값)                                                                                   | win 2026-11-05 |
| 유휴 CPU(`e30cf91`, 10초)                      | 모름               | 0ms, WorkingSet 43.2MB                                                                                                      | win 2026-11-05 |
| 팝오버 반투명                                  | `popover` vibrancy | `acrylic` + `.vibrancy`로 반투명 동작 확인(실험). macOS에서 반영(0004, 2026-11-05), 반영본 Windows 확인(release, `5837040`) | win 2026-11-16 |
| `pnpm tauri:build`(`5837040`)                  | 통과               | 통과(msi 2.95 MiB, nsis 2.12 MiB)                                                                                           | win 2026-11-16 |
| `pnpm check`(`5837040`)                        | 통과               | Vitest 208/208, Rust 327 통과 · 4 실패(kiro-cli·전역 CLAUDE.md가 테스트에 샘)                                               | win 2026-11-16 |
| release 앱 트레이·팝오버·한도                  | 확인               | 확인(툴팁·개요 탭 실제 값, 아이콘은 처음엔 숨겨진 아이콘 안)                                                                | win 2026-11-16 |
| 유휴 CPU(release, 10초)                        | 모름               | 0초, WorkingSet 33MB                                                                                                        | win 2026-11-16 |
