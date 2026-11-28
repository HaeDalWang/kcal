# 0003. 제품 범위: 반드시 들어갈 기능

- 상태: 채택
- 날짜: 2026-11-02
- 출처: ctxmeter(비공개 선행 프로젝트) `develop/decisions/09-agent-health-vision.md`, `10-replace-codexbar.md`, `execution/15-health-and-compare.md`

## 형태

| 결정                                          | 이유                                                                                                                |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| 데스크톱 앱 하나(Tauri). 별도 CLI 없음        | ctxmeter는 Node CLI + Swift 메뉴바로 런타임이 둘이었고, 설치에 Xcode CLT·Node가 필요했으며 macOS 14+ 전용이었다     |
| macOS, Windows 먼저. Linux는 나중             | Linux 요청 사용자 1명. OS별 경로·인증 읽기는 Rust 한 모듈에 모아 나중에 Linux를 붙이기 쉽게 둔다                    |
| identifier `io.github.haedalwang.kcal`        | 도메인이 없어 GitHub 계정 기반 역도메인을 쓴다. 배포 후 변경 금지(앱 데이터 경로·macOS 권한·업데이트 식별이 바뀐다) |
| Glance는 트레이(macOS 메뉴바, Windows 트레이) | Claude Code statusLine 출력이 필요해지면 같은 실행 파일에 인자 모드를 추가한다(그때 결정)                           |

## 반드시 들어갈 기능 (여섯 층)

| 층           | 하는 일                                                                                               | ctxmeter에서 가져올 것                                                                                   |
| ------------ | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| 1. 측정      | 컨텍스트(시스템·규칙 파일·MCP 도구 스키마·skills·hook), 토큰 사용량, **남은 한도**(Claude·Codex·Kiro) | 항목 측정 로직, MCP 스키마 측정(서버 실행은 사용자 동의 후), 아래 "한도 출처"                            |
| 2. 환율      | 토큰 종류·모델별 "1%p = 몇 토큰"                                                                      | Codex Plus 5시간 회귀(R² 0.92): 캐시 입력 1%p ≈ 742K, 비캐시 ≈ 66K, 출력 ≈ 5.5K. 주간·Claude는 표본 부족 |
| 3. 분해      | 설정 항목·행동 패턴을 각각 %p로                                                                       | `몫 ≈ 항목 토큰 × 창 안 요청 수 × w_cached + 항목 토큰 × 캐시 미스 수 × w_uncached`                      |
| 4. 시각화    | 체성분(설정) · 활동량(턴 × 컨텍스트) · 칼로리(%p)                                                     | 메뉴바 팝오버 구성(에이전트별 5시간·7일·월 막대, 리셋까지 남은 시간)                                     |
| 5. 코칭      | "이걸 바꾸면 주간 한도 X%p 절약" + 적용                                                               | `fix`: 사용자 동의 → 백업 → 변경 → 되돌리기 명령 제시                                                    |
| 6. 정기 검진 | 주 1회 전체 스캔, 변화·새 누수 알림, **처방 전후 비교표**(핵심 기능)                                  | `health --compare`: 처방을 실제 적용한 날 기록, 전 7일 / 후 7일 같은 지표 비교                           |

### 한도 출처

| 에이전트 | 주 경로                                                                    | 대체 경로                     |
| -------- | -------------------------------------------------------------------------- | ----------------------------- |
| Claude   | `GET api.anthropic.com/api/oauth/usage`(Claude Code OAuth 토큰, 읽기 전용) | statusLine `rate_limits` 기록 |
| Codex    | `GET chatgpt.com/backend-api/wham/usage`(`~/.codex/auth.json`, 읽기 전용)  | 최근 rollout의 `rate_limits`  |
| Kiro     | `kiro-cli chat --v2 --no-interactive /usage`                               | 없음(마지막 성공 값 유지)     |

비공식 API를 쓰는 대가로 지키는 규칙(ctxmeter `10` 그대로):

- 토큰은 읽기만 한다. refresh하지 않는다(갱신하면 원래 에이전트가 로그아웃될 수 있다). 401·만료면 대체 경로 + "N분 전 기준" 표시.
- 인증 파일·Keychain에 쓰지 않는다. 토큰을 로그·오류 메시지·IPC 응답에 넣지 않는다.
- 응답 형식이 바뀌면 0을 보여 주지 않고 대체 경로로 내려가 원인을 표시한다.

## 주기 측정: AGENTS.md "polling 금지"의 예외

한도 값은 이벤트로 알 수 없어 주기 호출이 필요하다. 예외 범위를 이것으로 한정한다.

- 한도 조회: Rust 쪽 타이머 하나. Claude·Codex 5분, Kiro 10분. 429면 `Retry-After`, 없으면 두 배씩 최대 60분.
- 정기 검진: 다음 실행 시각까지 한 번 잠드는 타이머(주 1회). 반복 확인 루프가 아니다.
- 세션 로그 변화는 polling이 아니라 파일 변경 이벤트로 받는다.
- 프론트엔드는 여전히 `setInterval`을 쓰지 않는다. 값은 Tauri event로 받는다.

## 숫자 표기 규칙

모든 숫자에 출처를 붙인다(ctxmeter `09` "지키는 장치").

- **실측**: 로그·공식 필드에 있는 값(한도 %, Kiro 크레딧)
- **추정(±오차)**: 회귀로 얻은 환율과 검증 범위
- **추론**: 추정 × 측정 조합(예: "이 skill ≈ 주간 3%p")

로컬 전용, 텔레메트리 없음, transcript 원문 복사·색인 금지. 설정 변경은 사용자 동의 + 백업 + 되돌리기.

## 열린 것

- Windows에서 Claude Code OAuth 토큰 위치(macOS는 Keychain `Claude Code-credentials`) — 모름, Windows 기기에서 확인.
- Kiro CLI의 Windows 지원 여부 — 모름.
- Codex가 오래된 rollout을 `.jsonl.zst`로 압축한다 → zstd 읽기가 필요하다(ctxmeter 측정 구멍).
- 다른 플랜·모델의 환율, 주간 환율 — 데이터가 더 쌓인 뒤.
- 정기 검진 알림: ctxmeter에서 macOS 알림이 안 떴다(권한 추정). Tauri notification plugin으로 확인.

## 범위 밖으로 정한 것 (2026-11-28, 사용자 결정)

Claude Code 2.1.307~332 신기능과 vault 자료에서 나온 후보 4건을 넣지 않는다.

- 보안 설정 안내(`permissions.blockWritesOutsideWorkingDirectories`, `permissions.denyInsteadOfPrompting`, `PreSkillShellCommand` hook): 사용량·비용 진단 범위 밖이다.
- MCP 서버 스킬의 "버전 고정 안 됨" 표시: URL 서버는 판정할 수 없고 패키지 서버는 추정이다. 크기 세기(0005)로 충분하다.
- vault "사용자 미검토" 개수 표시: 배포 앱에 개인 vault 전용 기능을 넣지 않는다.
- "규칙은 늘 넣고 사실은 포인터로" 진단: "MEMORY 2차 점검은 만들지 않는다" 결정을 유지한다.
