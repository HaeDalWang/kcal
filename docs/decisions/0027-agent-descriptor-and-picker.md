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

## ③ 갱신 2026-11-19: 모델별 한도·추가 사용량·초기화권

새 엔드포인트는 없다. kcal이 이미 부르는 두 응답에서 버리던 필드만 읽는다(실제 응답의 키·타입은 2026-11-19에 확인, 값은 기록하지 않음).

- **Claude** `api/oauth/usage`: `limits[]` 중 `kind: weekly_scoped`이고 `scope.model.display_name`이 있는 항목 → 그 모델의 주간 한도(예: Fable). `session`·`weekly_all`은 기본 5시간·주간 창과 같아 더하지 않는다. `seven_day_opus`·`seven_day_sonnet`은 null이 아니면 "Opus"·"Sonnet"으로 더하되 같은 이름이 이미 있으면 건너뛴다. `extra_usage`는 `is_enabled`가 true이고 `used_credits`·`monthly_limit`이 둘 다 있을 때만 "추가 사용량 $12.50 / $100.00". 금액은 센트(CodexBar `ClaudeUsageFetcher` 소스)라 100으로 나눈다. 숫자는 JSON 숫자와 숫자 문자열을 다 받는다. 켜져 있을 때의 실제 모양은 이 계정(꺼짐)에서 확인하지 못했다(모름).
- **Codex** `wham/usage`: `additional_rate_limits[]`의 `limit_name`과 `rate_limit.primary_window`·`secondary_window` → 모델별 한도(없으면 줄 없음). `rate_limit_reset_credits.available_count` → "한도 초기화권 N개". 같은 응답에 있어 `/wham/rate-limit-reset-credits`는 부르지 않는다(backlog의 추정은 틀렸다). Codex 잔액 `credits`는 범위 밖.
- **계약**: `LimitWindow.group`(선택)을 더했다. 모델 창은 `windows`가 아니라 `AgentUsage.modelWindows`에 둔다(없으면 키 없음). `credits`·`resetPasses`도 선택 필드다. Rust `usage::Extras`(`#[serde(flatten)]`)와 `ipc.ts`, fixture `usage-snapshot.json`을 함께 바꿨다. 창 `id`는 기본 창과 같은 `5h`·`7d`를 쓰고(페이스 길이 계산이 맞는다) 구분은 `group`이 한다.
- **왜 `windows`와 따로**: 트레이 막대·툴팁·낮음 알림·핸드오프 남은 %·페이스 기록은 모두 `windows`를 읽는다(`min_remaining`, `summary`, `HistoryEntry::from_windows`, TS `minRemaining`). 모델 창을 거기 섞으면 Fable 한도가 낮다는 이유로 트레이가 경고색이 된다. 따로 두면 그 코드를 하나도 고치지 않아도 기본 창만 읽는다.
- **화면**: 팝오버 에이전트 탭과 대시보드 에이전트 카드(`AgentSection`)에서 기본 창 아래에 모델 창마다 막대 한 줄(이름 "Fable 주간"), 그 아래 글 한 줄씩(추가 사용량, 초기화권). 개요 카드·트레이 막대·알림은 그대로. 새 색·새 크기 없음(개요의 보조 창 줄과 같은 10px 줄).
- 미리보기: `popover.html`·`dashboard.html`에 `?s=extras`.
