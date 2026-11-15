# 0015. 대시보드: 오늘 쓴 양을 링으로 (일일 활동 구조)

- 상태: 채택(구현 중, 2026-11-06)
- 날짜: 2026-11-06
- 앞선 결정: [0010](0010-tray-and-dashboard.md)(대시보드 창), [0011](0011-visual-design.md)(에이전트 색). 트레이는 바꾸지 않는다.

## 배경

지금 대시보드는 에이전트별 상세 카드를 쌓은 것이라 "오늘 얼마나 썼나, 언제 몰렸나"가 한눈에 안 보인다. 사용자가 건강 앱의 '일일 활동' 화면(목표 대비 오늘 양을 동심원 링으로, 시간대별 막대, 요일 줄)을 참고로 주었다. 구조만 가져오고 모양(하트)·이름은 가져오지 않는다.

## 결정 (사용자 선택 2026-11-06)

1. **링 = 오늘 쓴 양 ÷ 하루 몫.** 에이전트 하나가 링 하나(색은 0011 `AGENT_COLOR`). 동심원, 바깥부터 켠 에이전트 순서.
   - Claude·Codex: 오늘 쓴 양 = 주간(7d) 한도 사용 %의 증가분 합(%p). 하루 몫 기본 = 100/7 → tuning `dailyGoalPercent`(기본 14).
   - Kiro: 오늘 쓴 크레딧. 하루 몫 기본 = 월 한도 ÷ 그 달 일수 → tuning `kiroDailyCredits`(0 = 자동).
   - 100%를 넘으면 링이 한 바퀴를 넘는 대신 꽉 차고 숫자 칸에 넘친 양이 `--warn`색으로 보인다.
2. **날짜: 오늘 + 최근 7일 줄.** 요일마다 작은 링, 누르면 그날의 링·숫자 칸·시간대 그래프로 바뀐다. 달력(월 보기)은 다음 단계.
3. **시간대 그래프 = 시간별 요청 수**(Kiro는 질문 수). 0~24시 막대, "가장 바빴던 시간대: 오후 1:00–2:00".
4. **기존 상세는 아래 카드로 유지**(0010 대시보드 내용 전부). 날짜를 바꿔도 상세 카드는 지금 값이다.

## 데이터

- Rust `metrics`가 에이전트마다 UTC 정시 기준 시간별 버킷(최근 `barDays + 1`일)을 낸다: 요청 수, 쓴 양(%p 또는 크레딧). 같은 캐시·`metrics-updated` event를 쓴다. polling 없음.
- 지역 날짜·시각으로 자르는 것은 프론트가 한다(Rust `time` local-offset 꺼짐, 0010). `ponytail:` 30·45분 단위 시간대는 시간 버킷 경계가 지역 정시와 어긋난다. 필요하면 버킷을 15분으로.
- 주간 %p 증가분: 같은 창(`resetsAt` 묶음) 안에서는 직전 표본보다 오른 만큼, 새 창의 첫 표본은 0에서 오른 것으로 본다. 증가분은 뒤 표본의 시각에 넣는다.
  - Codex: rollout `rate_limits.secondary`.
  - Claude: `limit-history.jsonl`의 `sevenDay`. 앱이 켜져 있을 때만 기록되므로 꺼졌던 시간의 사용은 다음 기록 시각에 몰린다.

## 이유

- 한도는 "남은 %"(트레이)가 결론이지만, 습관을 보려면 "하루에 얼마, 언제"가 필요하다. 하루 몫은 주간 한도를 고르게 나눈 값이라 새 판정 상수가 아니다.
- 새 의존성 없음: 링·막대는 SVG/CSS로 그린다.

## 구현 (2026-11-06)

- Rust `metrics`: `AgentMetrics.hourly: Vec<HourBucket>`(`startsAt` UTC 정시, `requests`, `used`), 오래된 순 `(barDays + 1) × 24`칸, 마지막 칸이 지금 시간. 공용 함수 `hourly_start`·`hourly`(버킷 채우기)·`weekly_increments`(주간 증가분)는 `metrics/mod.rs`. `used` 단위는 기존 `barUnit`을 따른다(percent = 주간 %p, credits = 크레딧). 새 enum은 만들지 않았다.
  - Codex: `rate::codex::Event`에 `weekly_used_percent`·`weekly_resets_at`(`rate_limits.secondary`)만 더했다. 요청 = `input > 0`(dedup 뒤라 반복 보고 제외). 읽는 rollout 범위를 `barDays + 1`일로 넓혔다(막대는 계속 `barDays`일만 쓴다).
  - Claude: 요청 = assistant `usage`(같은 `message.id` 한 번, 입력 0 제외), `claudeLogFiles`개를 시간 버킷 기간으로 한 번 읽어 지금 5시간 구간 집계에도 같이 쓴다. 쓴 양 = `limit-history.jsonl`의 `sevenDay`.
  - Kiro: 턴 수와 크레딧 합. 읽는 기간을 `barDays + 1`일로 넓혔다.
- tuning(0007 표): `dailyGoalPercent`(14, 1–100), `kiroDailyCredits`(0 = 자동, 0–100000). 설정 화면 "화면" 절에 보인다(`tuning-form.tsx`, 단위 `%p`·`크레딧`).
- 프론트: `src/lib/daily.ts`(지역 날짜 묶기, 하루 몫, 가장 바빴던 시간, 시간대 문구) + `daily.test.ts`. 컴포넌트 `activity-rings.tsx`(SVG 동심원, 트랙 = 에이전트 색과 `--muted`를 섞은 색), `hourly-chart.tsx`(SVG 24칸 막대), `daily-activity.tsx`(날짜 알약·요일 줄·큰 카드·시간대 카드). `dashboard.tsx`는 헤더 + 위 두 카드 + 기존 상세 카드. 너비 ≥ 900px에서 2열.
- 날짜 상태는 "오늘에서 며칠 전"으로 둔다. 오늘 기준 `now`는 usage 수신·창 포커스 때만 바뀐다(타이머 없음).
- 화면 확인용 미리보기 `src/test/preview/dashboard.html`(IPC fixture + 합성 시간 버킷, `?empty`면 빈 상태). 앱 진입점과 별개라 번들에 들어가지 않는다.

## 명세와 다르게 한 것

- **맨 첫 주간 표본은 기준점(증가분 0)**이다. 앞 표본이 없으면 그 값이 언제 쌓였는지 모르므로 "새 창의 첫 표본 = 0에서 오름"은 리셋 시각이 앞 창보다 늦어진 경우에만 쓴다. 그래서 기록 맨 앞 시간(Codex는 읽은 rollout의 첫 이벤트, Claude는 기록 파일 첫 줄)의 사용은 잡히지 않는다.
- **같은 창 안의 증가분은 "그 창에서 본 최댓값보다 오른 만큼"**이다(직전 표본이 아니라). 여러 rollout의 표본이 섞여 값이 잠깐 내려갔다 오르면 같은 %p를 두 번 세지 않게 했다. 이미 지난 창(리셋이 더 이른)의 늦은 표본은 0.
- **Kiro 월 한도를 모르면** 링은 빼고 숫자 칸은 값만 두고 몫 자리에 "월 한도 모름"을 쓴다(칸 높이를 맞추려고).
- **점선 줄**: "총 요청 수"(Kiro 질문 포함), "가장 바빴던 시간대"(켠 에이전트 합), "요청이 있던 시간"(요청 1회 이상인 시간 수).
- `Event`에 주간 필드를 더하자 테스트 헬퍼 두 곳(`rate/codex_tests.rs`, `rate/behavior.rs`)의 `Event` 리터럴이 컴파일되지 않아 `None` 두 줄씩 더했다(테스트 코드만, 작업 범위 밖 파일).
- tuning spec 테스트는 정수 필드의 "최소 − 1"이 음수면 건너뛴다(`kiroDailyCredits` 최소 0, 음수는 u64 형식에서 이미 거부된다. 따로 테스트함).
