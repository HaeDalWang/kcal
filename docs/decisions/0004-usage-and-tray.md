# 0004. 한도 조회와 트레이 Glance

- 상태: 채택
- 날짜: 2026-11-04
- 범위·규칙: [0003](0003-product-scope.md) "한도 출처", "주기 측정". 플랫폼 경로: [`../platform/README.md`](../platform/README.md)

## 결정

| 결정                                                                                                                                                                      | 이유                                                                                                                                                                                                          |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 창은 팝오버 하나(`popover`): 테두리 없음, 시작 시 숨김, 트레이 아이콘 아래에 표시, 포커스를 잃으면 숨김                                                                   | Glance는 트레이다(0003). 템플릿 main 창과 `compute_stats` 예제는 지운다                                                                                                                                       |
| macOS는 `ActivationPolicy::Accessory`(Dock 아이콘 없음)                                                                                                                   | 메뉴바 앱                                                                                                                                                                                                     |
| 트레이 아이콘은 1개(id `kcal`). macOS `set_title`은 켠 에이전트 전체 창 중 가장 적게 **남은 %**(값 없으면 "—"), tooltip은 켠 에이전트마다 한 줄 요약. Windows는 tooltip만 | 사용자 피드백(2026-11-04): CodexBar처럼 아이콘 하나, 에이전트 전환은 팝오버 탭. Windows 트레이는 아이콘 옆 글자를 지원하지 않는다                                                                             |
| 트레이 우클릭 메뉴: "종료"만. 아이콘은 단색 template PNG `src-tauri/icons/tray.png`(원본 `docs/assets/tray.svg`): kcal 불꽃에 심박선을 뚫은 것                            | 창이 없으므로 종료 경로가 필요하다. 상표 로고는 쓰지 않는다                                                                                                                                                   |
| 팝오버는 폭 300, 투명 창 + macOS `popover` vibrancy(radius 10, state active), Windows `acrylic`. `app.macOSPrivateApi: true`(Cargo feature `macos-private-api`)           | 메뉴바 메뉴처럼 반투명하게 보이려면 필요하다. 대가: Mac App Store 배포 불가(비공개 API). 직접 배포는 영향 없음                                                                                                |
| Windows는 `acrylic`(2026-11-05 변경, 전에는 불투명). `theme-init.js`가 macOS·Windows에서 `.vibrancy` class를 붙여 배경을 비우고, 그 외는 불투명 `bg-background`           | 사용자 요청(반투명). Win11 22631 다크에서 실기 확인(platform/windows-log.md). Tauri 문서상 Windows 10/11 지원, 창 끌기·리사이즈 때 느림 — 팝오버는 끌지 않고 높이만 내용에 맞춘다. Win10·라이트 모드는 미확인 |
| 창 높이는 내용에 맞춘다: 프론트엔드 `ResizeObserver`(내용 변경 이벤트만) → command `fit_popover_height`(100~800 논리 px 검증)                                             | 에이전트 수·reason에 따라 높이가 달라진다. 위치는 열 때마다 아이콘 아래(간격 0)로 다시 잡는다                                                                                                                 |
| 팝오버 맨 아래 "설정…", "종료 ⌘Q" 항목과 단축키(Windows는 Ctrl+Q) → command `quit_app`                                                                                    | 팝오버만 보이는 상태에서 바로 끌 수 있게. 트레이 우클릭 "종료"도 유지                                                                                                                                         |
| 테마는 OS를 따른다(토글 제거)                                                                                                                                             | 설정 화면에는 에이전트 표시만 둔다                                                                                                                                                                            |
| dev 포트: Vite 1470(HMR 1471), MCP bridge `base_port(9224)`. loopback 바인드는 유지(0001)                                                                                 | 템플릿 기본 1420·9223은 다른 Tauri 프로젝트(nib)와 겹쳐 `pnpm tauri:dev`가 즉시 종료됐다. CSP는 포트 숫자만 바꿨다                                                                                            |
| HTTP는 Rust에서 한다. 프론트엔드 CSP·capability는 바뀌지 않는다                                                                                                           | 토큰이 웹뷰에 들어가지 않는다                                                                                                                                                                                 |
| 갱신: Rust 타이머 → `usage-updated` event. 처음 그릴 값은 `get_usage` command                                                                                             | 프론트엔드 `setInterval` 금지(AGENTS.md)                                                                                                                                                                      |
| 수동 새로고침 없음                                                                                                                                                        | 클릭마다 호출하면 429를 맞기 쉽다(ctxmeter 10: Orca가 Claude 폴링에서 429). 필요해지면 최소 간격을 두고 추가                                                                                                  |
| Claude 토큰: macOS `security find-generic-password -s "Claude Code-credentials" -w`, Windows `%USERPROFILE%\.claude\.credentials.json`                                    | 새 의존성 없이 읽기 전용. 만료(`expiresAt`)면 refresh하지 않고 대체 경로                                                                                                                                      |
| Kiro: `kiro-cli chat --v2 --no-interactive /usage`, 타임아웃 30초, Windows는 콘솔 창을 띄우지 않는다(`CREATE_NO_WINDOW`)                                                  | Windows 실측 21초(platform/windows-log.md)                                                                                                                                                                    |

## 남은 % 표기, 팝오버 탭 (2026-11-04 갱신)

| 결정                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | 이유                                                                                                                                         |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| 모든 화면(팝오버·트레이 title·tooltip)은 남은 % = 100 − 사용률. 막대 길이도 남은 %. 문구 "5시간 98% 남음", 오른쪽 "4h 35m 후 재설정". Kiro는 막대 아래 "3,341 / 5,000 크레딧 남음"                                                                                                                                                                                                                                                                                                            | 사용자 피드백: 사용 %로 차오르는 막대가 헷갈린다(CodexBar처럼 쓸수록 줄게). Kiro 크레딧을 같은 줄에 두면 폭 300에서 재설정 시각이 줄바꿈된다 |
| 경고색: 남은 20% 이하 빨강. 리셋 시각이 지난 창은 100% 남음                                                                                                                                                                                                                                                                                                                                                                                                                                   | 기존 "80% 이상 사용"과 같은 기준을 뒤집은 것                                                                                                 |
| Rust 데이터(`usedPercent`)는 그대로. 변환은 표시 계층(TS `remainingPercent`, Rust `LimitWindow::remaining_percent`)에서                                                                                                                                                                                                                                                                                                                                                                       | IPC 계약을 바꾸지 않는다. 트레이 title은 Rust가 계산한다                                                                                     |
| 팝오버는 창 1개. 위쪽 탭(shadcn Tabs): `개요` + 켠 에이전트. 탭마다 작은 아이콘 + 이름 + 이름 아래 미니 막대(그 에이전트의 가장 급한 창 남은 %), 선택된 탭은 accent 배경. 기본은 개요(켠 에이전트마다 이름·메타 + 창별 "5시간 98% 남음 · 4h 35m 후" + 얇은 막대), 에이전트 탭은 상세 화면. 마지막으로 보던 탭은 메모리에만 둔다(저장 안 함). 탭 아이콘은 불꽃에 머리글자 C·X·K를 뚫은 인라인 SVG(`agent-icon.tsx`) — 2026-11-18 [0025](0025-agent-brand-marks.md)에서 회사 마크 단색으로 바뀜 | 창을 여러 개 두면 메모리·높이 맞춤이 늘어난다. 상표 로고는 쓰지 않는다                                                                       |
| 설정: 팝오버 안에서 화면 전환(별도 창 없음). 에이전트별 체크박스 "표시할 에이전트"(팝오버 탭·개요와 트레이 값). 마지막 하나는 끌 수 없다(UI 비활성 + Rust 검증)                                                                                                                                                                                                                                                                                                                               | 메뉴바 앱에 창을 늘리지 않는다                                                                                                               |
| 저장: Rust가 `app_config_dir()/settings.json`(`{"enabledAgents": [...]}`)에 임시 파일 → rename으로 쓴다. command `get_settings`·`set_settings`(빈 목록·알 수 없는 id는 `invalid_input` 오류), 파일이 깨졌으면 기본값(모두) + `reason`. 바뀌면 트레이 title·tooltip을 다시 계산하고 `settings-updated` event                                                                                                                                                                                   | 새 의존성·plugin 없이(plugin-store 대신 serde_json). capability 변경 없음                                                                    |
| 꺼진 에이전트는 조회하지 않는다. 다시 켜면 타이머를 깨워(`tokio::sync::Notify`, tokio feature `sync` 추가) 즉시 1회 조회 후 기존 주기                                                                                                                                                                                                                                                                                                                                                         | 호출 절약. `sync`는 tauri가 이미 켜 둔 tokio feature라 새 컴파일 대상이 없다                                                                 |

## 의존성

- `tauri` feature `tray-icon`(트레이), `image-png`(macOS template 아이콘 `src-tauri/icons/tray.png`, 원본 `docs/assets/tray.svg`).
- HTTPS 클라이언트, 시각 파싱, 프로세스 타임아웃에 필요한 crate는 구현 때 아래에 이름·버전·이유를 적는다. 이미 `Cargo.lock`에 있는 crate를 먼저 쓴다.

직접 의존성으로 올린 crate. 모두 tauri를 통해 이미 `Cargo.lock`에 있던 것이다.

| crate        | 버전(lock) | feature                                    | 이유                                                                                                                        |
| ------------ | ---------- | ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| `reqwest`    | 0.13.5     | `default-features = false`, `native-tls`   | Claude·Codex API HTTPS. OS 인증서 저장소·TLS(macOS Security.framework, Windows SChannel)를 써서 rustls·aws-lc 빌드를 피한다 |
| `tokio`      | 1.54.1     | `process`, `time`                          | `security`·`kiro-cli` 실행 + 타임아웃(`kill_on_drop`), 조회 타이머 `sleep_until`. 런타임은 tauri 것                         |
| `time`       | 0.3.55     | `parsing`, `formatting`, `serde`, `macros` | ISO·epoch·HTTP date(`Retry-After`) 파싱, IPC 시각 RFC 3339 직렬화                                                           |
| `regex`      | 1.13.1     | 기본                                       | Kiro `/usage` 출력의 ANSI 제거·줄 파싱                                                                                      |
| `serde_json` | 1.0.154    | 기본                                       | dev-dependency에서 일반 의존성으로. API 응답·기록 줄 파싱                                                                   |

`native-tls`가 끌어온 새 lock 항목: `native-tls`, `hyper-tls`, `tokio-native-tls`, `security-framework(-sys)`(macOS), `schannel`(Windows), `openssl*`·`vcpkg`(Linux 전용, macOS·Windows 빌드에는 컴파일되지 않음), `tempfile`, `rustls-pki-types`, `zeroize` 등.

## 열린 것

- 시계 틀어짐: 리셋까지 남은 시간을 로컬 시계로 계산한다. Windows에서 21분 틀린 기기가 있었다. 서버 `Date` 헤더 보정은 나중.
- Codex `.jsonl.zst` rollout은 대체 경로에서 읽지 않는다.
- macOS에서 Keychain을 처음 읽을 때 허용 창이 뜨는가 — 모름, 첫 실행에서 확인.
- 메뉴바 아이콘이 Control Center allow-list 때문에 숨는 문제(ctxmeter `develop/execution/08`)가 kcal identifier에서도 생기는가 — 모름.
- 탭 아이콘(14px)에서 머리글자 C·X·K가 작아 잘 안 보인다. 구분은 이름 글자가 한다.

## 갱신 2026-11-28: Claude 로그인 만료

코디네이터가 이 Mac에서 확인한 사실: Claude Code 2.1.318 credentials의
`claudeAiOauth.refreshTokenExpiresAt`(epoch milliseconds)는 `claude auth status --json`의
`loginExpiresAt`과 같다. `expiresAt`은 더 짧은 access token 수명이다. kcal은 기존 credentials만
읽고 `claude auth status` 실행이나 token 갱신을 하지 않는다. 종료 필드가 없으면 모르는 값으로
두어 기존 동작을 유지한다.

- refresh token 종료가 먼저다: KO "Claude 로그인 기간이 끝났어요. 터미널에서 claude를 실행해 /login으로 다시 로그인한 뒤 새로고침하세요."
  EN "Your Claude login period has ended. Run claude in a terminal, sign in again with /login, then press Refresh."
- refresh token이 아직 유효하고 access token만 끝났으면 기존 `login_expired(&CLAUDE)` 안내다:
  KO "Claude Code 로그인이 만료됐어요. claude를 한 번 실행한 뒤 새로고침하세요."
  EN "Your Claude Code login expired. Run claude once, then press Refresh."
- Claude API 401/403은 KO "Claude가 로그인 정보를 받아 주지 않았어요. claude를 한 번 실행한 뒤 새로고침하세요. 그래도 같으면 claude에서 /login으로 다시 로그인하세요."
  EN "Claude didn't accept the saved login. Run claude once, then press Refresh. If it still fails, sign in again with /login in claude."
  Codex의 401/403 안내는 유지한다.

`loginWarnDays`(`login_warn_days`, 기본 3일, 정수 0–30, 0은 끔) 이내이며 미래인 종료 시각만
Claude API reading의 `Extras.login_expires_at` / `AgentUsage.loginExpiresAt`으로 보낸다.
시각은 기존처럼 RFC 3339이며 모르면 키를 생략한다. log fallback에는 넣지 않는다.
Popover의 기존 `ExtraLines`에 초기화권과 같은 스타일로 한 줄을 더한다:
KO "Claude 로그인 {n}일 후 만료 · claude에서 /login", n=0은 "Claude 로그인 오늘 만료 · claude에서 /login".
EN "Claude login ends in {n} days · /login in claude", 1은 "1 day", 0은 "Claude login ends today · /login in claude".
남은 일수는 24시간 단위를 내림한다. 기존 `now`를 사용하므로 수신·포커스·Popover 렌더 시점에
계산하며 별도 타이머는 없다. 열린 화면에서 시간이 흘러도 실시간으로 내려가지 않는다.
Codex/Kiro, log fallback, tray bars/alerts의 `windows`는 유지한다.

Ruling: `Service.rejected: Option<fn() -> String>`으로 Claude 거절 문구만 교체한다 — 공유 HTTP 경로와 Codex 동작을 보존한다 — 잘못되면 서비스 안내가 바뀌므로 두 서비스의 401/403을 테스트한다.

Ruling: 경고 설정은 기존 `limits` 섹션에 둔다 — 한도 조회와 관련된 사용자 설정이다 — 잘못되면 설정 위치만 조정한다.

Ruling: Popover는 만료 시각이 지났거나 잘못된 값이면 경고 줄을 숨긴다 — 사전 경고를 지난 날짜로 표시하지 않는다 — 다음 조회 전에는 경고 줄이 없어지며 재로그인 안내는 조회 오류가 맡는다.

Ruling: 종료 시각은 signed integer milliseconds와 유효한 `OffsetDateTime`만 받는다 — credentials의 epoch-ms 계약을 따르고 잘못된 날짜를 UI에 보내지 않는다 — 공급자가 문자열·소수 형식으로 바꾸면 종료 시각은 모르는 값이 된다.
