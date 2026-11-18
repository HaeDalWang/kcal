# 0027. 에이전트 설명표와 대시보드 에이전트 고르기

- 상태: 채택(2026-11-19 사용자 승인: ①~③ 진행, ④는 별도 결정 기록. `전체` 링은 지금처럼 단위가 다른 에이전트도 같은 링에 둔다)
- 날짜: 2026-11-19
- 앞선 결정: [0010](0010-tray-and-dashboard.md) 트레이·대시보드, [0011](0011-visual-design.md) 크기·색, [0012](0012-tray-bars-icon.md) 메뉴바 막대, [0017](0017-optimization-framework.md) 알림 개수 규칙, [0025](0025-agent-brand-marks.md) 브랜드 마크
- 범위: 에이전트 목록(지금 Claude·Codex·Kiro 3개 고정)을 설명표 한 곳으로 모으고, 대시보드에서 에이전트 하나를 고를 수 있게 한다. 4번째 에이전트(Antigravity)를 붙이는 일은 이 결정의 검증 대상일 뿐 범위 밖이다.

## 사용자 결정 (2026-11-19)

1. "에이전트 선택" = 대시보드에서 하나 고르기. 팝오버 에이전트 탭을 누르면 그 에이전트 대시보드가 열린다. 설정의 켜기·끄기는 지금처럼 따로 둔다.
2. 4번째 후보 = Antigravity. 처음 후보는 Gemini CLI였으나 Google 공식 문서(developers.google.com/gemini-code-assist/docs/deprecations/code-assist-individuals, 2026-11-19 확인)상 2026-06-18부터 Gemini CLI는 개인·AI Pro·Ultra 계정 요청을 받지 않고 Antigravity로 옮겨 갔다.

## 지금 상태 (코드로 확인)

- 목록 고정: TS `Agent`·`AGENTS`(`src/lib/ipc.ts`), Rust `Agent::ALL: [Agent; 3]`(`src-tauri/src/usage/mod.rs`), 이름 `AGENT_LABEL`, 색 `--agent-*` 3개.
- 이름으로 갈라 그리는 곳: `agent === "kiro"`(단위·낱말), `agent === "codex"`(전용 칸), `kiroUsage`·`claudeUsage` 따로 넘기기, `RateSnapshot`의 에이전트별 고정 필드. 프론트 10곳 넘게, Rust `Agent::ALL` 17곳.
- 한도는 이미 일반형이다: `windows: Vec<LimitWindow>`(창마다 `id`·사용 %·초기화 시각). 새 한도 모양은 창 `id`를 더하면 된다.

## Antigravity로 본 설명표 칸 (조사 2026-11-19, 읽기 전용)

| 칸                        | Antigravity                                                                                     | 라벨                                                           |
| ------------------------- | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| 한도 모양                 | 모델 그룹 2개(Gemini / Claude·GPT) × (5시간, 주간). 요금제에 따라 5시간 창이 없다               | 확인됨(공식 plans 문서는 창 구성만, 그룹·필드는 CodexBar 문서) |
| 남은 %·초기화 시각        | `remainingFraction` + reset. 공식 API 없음(내부 API 또는 `agy -p /usage --output-format json`)  | 확인됨(de-facto)                                               |
| 오늘 쓴 양·시간대 요청 수 | 로컬 대화 파일(`.pb`)을 읽을 수 없는 형식이라 못 채움                                           | 확인됨(이 Mac)                                                 |
| 시작 컨텍스트             | 규칙(`AGENTS.md`·`GEMINI.md`)·MCP(`mcp_config.json`)·스킬 파일로 계산 가능, CLI·IDE 경로가 갈림 | 확인됨 / 경로 일부 추정                                        |
| 점검 카드                 | 사용 기록 근거가 없어 끄기 제안 불가(0022와 같음)                                               | 확인됨                                                         |

여기서 나온 요구: ① 창이 "모델 그룹"으로 묶일 수 있다(동료 요청 "모델별 한도"와 같은 모양) ② 에이전트마다 채울 수 있는 칸이 다르다 → 칸은 이름이 아니라 "가진 기능"으로 고른다.

## 결정 (초안)

1. **설명표 한 곳.** 에이전트마다 한 줄: id, 이름, 색 토큰, 마크, 세는 단위 낱말(요청/질문/크레딧), 기능 목록(`limits`·`dailyUse`·`hourly`·`startupContext`·`checks`·`windowMix` 같은 이름). TS는 `src/lib/agents.ts`, Rust는 `usage/mod.rs` `Agent`의 메서드. 둘의 짝은 테스트로 맞춘다.
2. **칸은 기능으로 고른다.** `agent === "kiro"` 같은 이름 분기를 설명표 조회로 바꾼다. 목표: 새 에이전트 = 설명표 한 줄 + Rust 수집 모듈, 화면 파일은 안 건드린다.
3. **창 묶음.** `LimitWindow`에 선택 필드 `group`(예: `gemini`, `claude-gpt`)을 더한다. 없으면 지금과 같다. 트레이·대시보드는 그룹이 있으면 그룹 제목 아래 창을 그린다. (IPC 계약 변경: Rust·TS·fixture를 같은 작업에서.)
4. **대시보드 고르기.** 대시보드 맨 위에 `전체 / <켠 에이전트들>` 탭. `전체` = 공통 칸만(남은 %·초기화, 오늘 쓴 양 ÷ 하루 몫, 시작 컨텍스트 합계). 에이전트 탭 = 공통 + 그 에이전트 전용 칸. 고른 값은 `openDashboard(tab, agent?)`와 `?agent=`로 흐른다(지금 `?tab=`과 같은 방식). 팝오버 에이전트 탭 → 그 에이전트 대시보드.
5. **순서.** ① 설명표 + 이름 분기 걷기(화면 그대로, 기존 테스트 + 설명표 테스트) → ② 대시보드 고르기 → ③ 창 `group` → ④ Antigravity 수집(별도 결정 기록: 내부 API·바이너리 실행 의존이라 보안 경계 검토 필요).

## 하지 않는 것

- 상상한 N개를 위한 칸을 미리 만들지 않는다. 칸은 실제 에이전트(지금 3개 + Antigravity)가 채우는 것만.
- 새 브랜드색은 이 결정에서 더하지 않는다(design.md: 결정 기록 필요, ④에서).
- 설치 안 한 에이전트 기본 끄기(`Settings::default`)·모르는 id 처리(`validate_agents`)는 ④ 때 정한다.
