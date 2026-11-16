# Windows 실행 기록

기기: 레노버 노트북, Windows 11 Pro 10.0.22631. 테스트 전용(개발은 macOS).
새 기록은 위에 추가한다. 비교표는 [`README.md`](README.md).

## 2026-11-16 — 공개 버전 release 빌드·실행 (`5837040`)

공개 직전 확인. 원격 히스토리가 공개용으로 새로 쓰여 로컬 main과 공통 조상이 없었다. 로컬은 `backup/pre-public-af78c0d` 브랜치로 남기고 `origin/main`에 맞췄다.

### 검증

| 명령                             | exit | 결과                                                                     |
| -------------------------------- | ---- | ------------------------------------------------------------------------ |
| `pnpm install --frozen-lockfile` | 0    |                                                                          |
| `pnpm check` — 프론트            | 0    | Vitest 208/208                                                           |
| `pnpm check` — Rust 테스트       | 101  | 327 통과 · **4 실패** · 12 ignored(fmt·clippy 통과)                      |
| `pnpm tauri:build`               | 0    | `kcal_0.1.0_x64_en-US.msi` 2.95 MiB, `kcal_0.1.0_x64-setup.exe` 2.12 MiB |

### 실패한 테스트 4건 — 이 기기 환경이 테스트에 샌다(확인됨)

제품 동작 결함은 아니다. 전역 CLAUDE.md·kiro-cli가 없는 CI runner에서는 안 걸린다(추정: 최신 `windows-latest` CI 결과로 확인).

- `usage::sources::tests::detects_installed_agents_from_their_folders_and_kiro_cli`, `settings::tests::first_run_shows_only_installed_agents_and_claude_when_none`: Windows의 `kiro_cli_candidates`는 테스트 home과 상관없이 실제 `%LOCALAPPDATA%\Kiro-Cli\kiro-cli.exe`를 본다(`src-tauri/src/usage/paths.rs`). 이 기기에 kiro-cli가 설치돼 있어 빈 home에서도 Kiro가 "설치됨"이 된다. 고치는 방향: 후보 경로의 LOCALAPPDATA를 주입받게 하거나 테스트에서 home 기준으로 고정.
- `coach::checks::tests::agents_md_hook_in_user_settings_is_removed_with_its_empty_group_and_array`, `…agents_md_hook_in_project_settings_is_instruction_only`(`find_check(...).unwrap()`이 None): 테스트 작업 폴더가 `%TEMP%`(홈 아래)라 `instructions::claude()`의 lineage가 실제 `~\.claude\CLAUDE.md`를 만나 "CLAUDE.md 있음 → AGENTS.md 안 실음"이 된다. 11-05의 lineage 문제와 같은 원인이다. 그때 `claude_in`으로 테스트 lineage를 줄였지만 coach 테스트는 `claude()`를 거쳐 그대로다.
- 이 기기에서는 코드를 고치지 않았다(테스트 전용).

### release 앱 실행(`target/release/kcal.exe`)

- 트레이 아이콘은 새 exe라 다시 숨겨진 아이콘(^) 안에 있었다. 처음 설치한 사용자는 아이콘을 바로 못 볼 수 있다 → README Windows 절에 안내 한 줄 권장.
- 툴팁: `Claude 5h 78% 남음 · 7d 36% 남음 Codex 5h 89% 남음 · 7d 61% 남음 Kiro 월 11% 남음`.
- 팝오버: 개요 탭에 세 에이전트(남은 %, 7일·월 한도 리셋까지, 시작 컨텍스트), Kiro "낮음" 경고, 대시보드·점검(배지 1)·새로고침·설정·종료 메뉴가 나온다.
- 반투명: 팝오버 뒤 화면이 블러로 비친다. Windows acrylic 반영본 확인(11-05 남은 항목 해소).
- 유휴 CPU 10초 0초, WorkingSet 33MB.
- 확인 안 함: msi·nsis 설치 과정(exe를 바로 실행했다), 라이트 모드.

## 2026-11-05 — 트레이·팝오버·한도 실기 확인 (`e30cf91`)

### 검증

| 명령                                           | exit | 결과                                                      |
| ---------------------------------------------- | ---- | --------------------------------------------------------- |
| `git pull --ff-only`                           | 0    | `961ab7f..e30cf91`, 13 커밋                               |
| `pnpm install --frozen-lockfile`               | 0    |                                                           |
| `typecheck` / `lint` / `format:check` / `test` | 0    | 8 files, 48 tests                                         |
| `pnpm rust:check`                              | 101  | fmt·clippy 통과, 테스트 125 통과 · **1 실패** · 2 ignored |
| `pnpm tauri:dev:mcp`                           | 실행 | dev 빌드 1분 23초, MCP bridge 127.0.0.1:9224              |

### 실패한 테스트: `claude_reads_agents_md_only_when_no_claude_md_in_lineage`

- 결과: 기대 `["AGENTS.md"]`, 실제 `["CLAUDE.md", "working-with-me.md", …]`(이 기기 사용자의 전역 `~\.claude\CLAUDE.md`와 import한 rule들).
- 원인(확인됨): Windows `temp_dir()`이 `C:\Users\<user>\AppData\Local\Temp`라 **홈 아래**에 있다. 테스트 작업 폴더의 lineage를 거슬러 올라가면 실제 홈의 `.claude\CLAUDE.md`를 만난다. 테스트용 가짜 home의 `user_file`과 경로가 달라 제외되지 않는다. macOS `TMPDIR`(`/var/folders/…`)은 홈 밖이라 안 걸린다. 전역 CLAUDE.md가 없는 CI runner에서도 통과한다.
- 제품 코드 쪽 의심(추정): `has_claude_md`의 `file != user_file`은 경로 문자열을 비교한다. 작업 폴더 경로가 `\\?\C:\…`(canonicalize 결과)로 들어오면, 홈 아래 작업 폴더에서 전역 `~\.claude\CLAUDE.md`가 프로젝트 CLAUDE.md로 잘못 세지고 AGENTS.md를 건너뛸 수 있다. 확인 방법: 홈 아래 폴더(예: `C:\Users\<user>\x`)에 AGENTS.md만 두고 측정 → AGENTS.md가 나오는지 본다. 고치는 방향: 비교를 `real_file` 기준으로 하고, 테스트는 lineage를 home에서 끊거나 홈 밖 임시 폴더를 쓴다.
- 이 기기에서는 코드를 고치지 않았다(테스트 전용).

### 트레이·팝오버(MCP CLI + `orca computer`)

- 트레이 아이콘: 처음에는 숨겨진 아이콘(오버플로) 안에 있었다. 툴팁 = `Claude 5h 91% 남음 · 7d 87% 남음 Codex 5h 48% 남음 · 7d 9% 남음 Kiro 월 44% 남음`. Windows 트레이는 글자를 못 띄우므로 남은 %는 툴팁에만 있다.
- 아이콘 클릭 → 팝오버 표시(visible true). 위치: 작업 표시줄 바로 위 오른쪽 아래, 화면 안(1920×1080, 배율 125%).
- 포커스를 잃으면 닫힌다(의도된 동작). 캡처 도구가 포커스를 가져가면 닫히므로 연 직후 캡처해야 한다.
- 탭 4개 모두 실제 값으로 나온다:
  - 개요: Claude·Codex(API), Kiro(CLI) 한도와 리셋까지 남은 시간
  - Claude: 환율 "기록 쌓는 중(창 0개)", 시작 컨텍스트 27,369 토큰(실측 19,767·추정 7,602), 작업 폴더 kcal
  - Codex: 환율 "표본 부족(창 1개)", 시작 합계 6,533, 첫 요청 실측 17,024(11-03에 잰 값과 같다)
  - Kiro: 2,208 / 5,000 크레딧 남음, 시작 합계 356(실측 0 — Windows에서 Kiro 세션 실측을 못 찾았는지는 모름)
- 키보드 포커스 링이 선택 안 된 탭(Codex)에 남아 있다. 선택은 Kiro였다. 사소하다.
- Kiro 리셋 "25d 22h 후"(10:14 KST 기준) → 12-01 00:00 **UTC** 기준으로 계산한 것과 맞다(추정).
- 유휴 CPU(`kcal.exe`, 10초): 0ms, WorkingSet 43.2MB(11-03의 29.7MB보다 늘었다. 한도·컨텍스트 캐시 때문으로 추정).
- 스크린샷은 세션 scratchpad에 있다(`win2-개요.png`, `win2-Claude.png`, `win2-overview.png`(Codex), `win2-Kiro.png`, `win2-desktop.png`, `win2-acrylic.png`). 레포에는 넣지 않았다.

### 팝오버 반투명 실험(acrylic)

- 지금 불투명한 이유(확인됨, 결정 0004): `windowEffects.effects`가 `["popover"]`뿐인데 이 값은 macOS 전용이다. `public/theme-init.js`가 macOS에서만 `.vibrancy`를 붙여 웹뷰 배경을 비운다. Windows는 일부러 불투명 `bg-background`를 쓴다(배경을 비우면 뒤가 그대로 비치기 때문).
- 임시로 두 줄을 바꿔 dev 앱에서 봤다:
  - `tauri.conf.json`: `"effects": ["popover", "acrylic"]`
  - `theme-init.js`: UA에 `Windows`가 있어도 `.vibrancy`를 붙인다
- 결과(확인됨, 다크 모드, Windows 11 22631): 반투명 블러로 보인다. 뒤의 창이 흐리게 비치고 글자는 잘 읽힌다. 색은 다크 테마보다 밝은 회색 톤이다. dev 빌드가 다시 뜨는 데 17초 걸렸고 오류는 없었다.
- 실험 후 `git checkout`으로 되돌렸다. 레포에는 반영하지 않았다.
- macOS에서 반영할 때 볼 것:
  - Windows 10에서도 되는지(모름)
  - 라이트 모드 색
  - `mica`와 비교
  - acrylic은 창을 끌거나 크기를 바꿀 때 느리다고 알려져 있다(추정, Tauri 문서로 확인 필요). 팝오버는 둘 다 하지 않는다.
  - 결정 0004의 "Windows는 vibrancy 없음" 줄을 바꿔야 한다.

### 특이사항 정리(사용자 확인 2026-11-05)

| 항목                      | 상태                                                                            |
| ------------------------- | ------------------------------------------------------------------------------- |
| 트레이에 % 글자 없음      | Windows 트레이 한계. 툴팁으로 대신한다(0004 그대로)                             |
| 팝오버 불투명             | 의도된 동작(0004). 반투명을 원함 → `acrylic`이 동작함. macOS에서 반영 예정      |
| Kiro 시작 컨텍스트 실측 0 | 이 기기에서 Kiro를 쓰지 않아 세션 기록이 없다. 정상                             |
| 라이트 모드               | 확인하지 않음. 사용자가 생략해도 된다고 함                                      |
| Rust 테스트 1개 실패      | 테스트가 실제 홈을 읽는 문제(위). 제품 코드도 같은 비교를 쓰는지 macOS에서 확인 |
| 탭 포커스 링              | 선택하지 않은 탭에 남는다. 사소함                                               |

## 2026-11-03 — 첫 실기 확인 (backlog 2번)

### 시작 상태

- 있음: git 2.37.0, gh 2.103.3, claude 2.1.317, kiro-cli 2.37.1, Codex 데스크톱 앱
- 없음: Node, pnpm, Rust, MSVC, codex CLI

### 툴체인 설치

| 명령                                                                                                                                             | exit | 비고                        |
| ------------------------------------------------------------------------------------------------------------------------------------------------ | ---- | --------------------------- |
| `winget install --id OpenJS.NodeJS --version 25.8.1 --exact`                                                                                     | 0    | UAC 필요                    |
| `winget install --id Rustlang.Rustup --exact`                                                                                                    | 0    | rustc 1.99.1                |
| `winget install --id Microsoft.VisualStudio.2022.BuildTools --override "... --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` | 0    | 약 15분 걸림                |
| `npm install -g pnpm@12.15.0`                                                                                                                    | 0    | Node 25에는 corepack이 없다 |
| `rustup default stable; rustup component add clippy rustfmt`                                                                                     | 0    |                             |

새로 설치한 뒤 같은 셸에서는 PATH가 갱신되지 않는다. 터미널을 새로 열거나 Machine·User PATH를 다시 읽는다.

### 프론트엔드 검증

| 명령                             | exit                 |
| -------------------------------- | -------------------- |
| `pnpm install --frozen-lockfile` | 0                    |
| `pnpm typecheck`                 | 0                    |
| `pnpm lint`                      | 0                    |
| `pnpm format:check`              | 0                    |
| `pnpm test`                      | 0 (3 files, 7 tests) |

### Rust 검증·앱 실행

| 명령                                          | exit    | 결과                                                                |
| --------------------------------------------- | ------- | ------------------------------------------------------------------- |
| `pnpm rust:check`                             | 0       | fmt·clippy 통과, 테스트 9/9. 첫 빌드 409초                          |
| `pnpm tauri:dev:mcp`                          | 실행 중 | dev 빌드 1분 20초, `WebSocket server listening on: 127.0.0.1:9223`  |
| `tauri-mcp driver-session start`              | 0       | 앱에 연결됨                                                         |
| `ipc-execute-command compute_stats [1,2,3,4]` | 0       | `{count:4, mean:2.5, median:2.5, …}`                                |
| `ipc-execute-command compute_stats []`        | 0       | `입력이 비어 있습니다.` 오류가 정상적으로 돌아옴                    |
| UI "계산" 버튼 클릭                           | —       | 개수 8, 합계 31, 평균 3.875, 중앙값 3.5                             |
| 테마 버튼 3회 클릭                            | —       | system(dark) → light → dark → system. `localStorage.theme`에 저장됨 |

- WebView2 UA는 `Edg/131.0.0.0`이다. 레지스트리 `EdgeUpdate` pv는 126.0.2592.113으로 다르다. 버전은 UA 값을 기준으로 한다.
- 유휴 상태에서 `kcal.exe`의 10초 CPU 시간은 0초, WorkingSet 29.7MB다.
- 폰트: `system-ui` → Segoe UI 계열로 렌더링된다. 한글이 깨지지 않는다.
- 스크린샷은 레포에 넣지 않았다(세션 scratchpad `win-click1.png` light, `win-click2.png` dark).
- 세션 시작 때 Claude Code의 `tauri` MCP 서버 연결이 실패했다(`CONNECTION_CLOSED`). 앱이 떠 있지 않았기 때문이다. 대신 CLI(`@hypothesi/tauri-mcp-cli@0.13.2`)로 확인했다.

### 에이전트 인증·데이터 위치

토큰 값은 읽지 않고 JSON 키 이름만 확인했다.

- Claude Code: `%USERPROFILE%\.claude\.credentials.json`(평문). Keychain에 해당하는 Windows 자격 증명 관리자를 쓰지 않는다. 같은 파일에 `mcpOAuth`(MCP 서버 OAuth)도 있다.
- Codex: `%USERPROFILE%\.codex\auth.json`. 세션 rollout 디렉터리는 아직 없다(Codex 세션을 한 번도 돌리지 않음).
- Kiro: `kiro-cli chat --v2 --no-interactive /usage` → exit 1, `Not logged in`. 0.09초 만에 실패해서 한도 조회 타이머에 넣어도 비용이 작다.

### Codex CLI·rollout

- 데스크톱 앱만으로는 `codex` 명령이 생기지 않는다. WindowsApps에는 `codex-chrome-native-host.exe`, `codex-core-command-runner.exe`만 있다.
- `npm install -g @openai/codex@0.166.0` → exit 0, `%APPDATA%\npm\codex.ps1`. 데스크톱 앱과 `~\.codex\auth.json`을 같이 써서 `codex login status`가 바로 `Logged in using ChatGPT`(exit 0)였다.
- `codex exec --skip-git-repo-check "Reply with exactly: ok"` → exit 0. CLI가 표시한 `tokens used`는 3,589였다.
- rollout: `%USERPROFILE%\.codex\sessions\YYYY\MM\DD\rollout-<로컬시각>-<uuid>.jsonl`. 18줄, 124,875 bytes. `.zst` 파일은 아직 없다(오래된 세션이 없어서).
- 마지막 `token_count` 이벤트에 한도 대체 경로로 쓸 값이 있다:
  - `rate_limits.primary`: `{used_percent, window_minutes: 300, resets_at: <epoch 초>}`
  - `rate_limits.secondary`: `window_minutes: 10080`(7일)
  - `credits`도 있다.
  - `resets_at`이 서버가 준 절대 시각이라 로컬 시계가 틀어져도 리셋 시각 자체는 맞다(남은 시간 계산만 로컬 시계에 영향받는다).
- `total_token_usage`: input 17,024 (cached 13,440), output 5. "ok" 한 줄에 입력이 17k 토큰이다. 이것이 Codex의 기초대사량 실측 예다. CLI가 표시한 3,589는 비캐시 입력 + 출력과 같다.

### Kiro `/usage` (로그인 후)

- `kiro-cli whoami` → exit 0, IAM Identity Center 로그인, profile 리전 `us-east-1`.
- `kiro-cli chat --v2 --no-interactive /usage` → exit 0, **21.05초**. 미로그인일 때는 0.09초 만에 실패했다. 출력 형식(값은 예시):

  ```
  Estimated Usage | resets on 2026-12-01 | KIRO PRO MAX
  Credits (995.38 of 5000 covered in plan), 19.9%
  Overages: Enabled
  Overages credits: 0.00 credits
  Your plan is managed by your organization's administrator.
  ```

- 시사점: 일반 텍스트라 줄 단위로 파싱해야 한다. 리셋은 날짜만 있고 시각이 없다. 21초가 걸리므로 반드시 비동기·타임아웃으로 호출한다. macOS 소요 시간은 모름 → `macos-log.md`에서 비교한다.

### 문제: Kiro(AWS Identity Center) 로그인 실패

- 증상: 브라우저 로그인 화면에 "시스템에 문제가 발생했습니다". 같은 계정이 macOS에서는 로그인된다.
- 원인(확인됨): 시스템 시계가 21분 빨랐다. HTTP `Date` 헤더 02:04:05Z, 로컬 02:25:21Z. `w32time` 서비스가 Stopped였다.
- 조치: `Set-Service w32time -StartupType Automatic; Start-Service w32time; w32tm /config /manualpeerlist:time.windows.com /syncfromflags:manual /update; w32tm /resync /force` (관리자). 이후 차이 < 1초.
- kcal에 주는 시사점: 한도 리셋까지 남은 시간이나 "N분 전 기준"을 로컬 시계로 계산하면 이런 기기에서 틀린다. 서버 응답 시각을 쓰는 것을 검토한다.
