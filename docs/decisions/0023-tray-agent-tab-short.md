# 0023. 트레이 에이전트 탭 줄이기

- 상태: 채택(2026-11-17 사용자 확인: 최근 구간 막대는 트레이에 남기고, 커서를 올린 막대의 값을 바로 보인다)
- 날짜: 2026-11-18
- 앞선 결정: [0010](0010-tray-and-dashboard.md), [0011](0011-visual-design.md), [0019](0019-design-review-fixes.md) D-06
- 범위: 트레이 에이전트 탭이 길다(Claude 탭 + 메뉴 약 720px). `design.md` 1·3절("트레이는 결론만, 한 덩어리 = 제목 → 큰 값 → 막대 → 회색 한 줄")에 맞춰 대시보드에 이미 있는 값을 트레이에서 뺀다. 5절 CodexBar 크기·색·계산·IPC는 그대로다.

## 뺀 것 (같은 값이 대시보드에 이미 있다)

| 트레이에서 뺀 것                        | 대시보드 위치                                                       |
| --------------------------------------- | ------------------------------------------------------------------- |
| 시작 컨텍스트 구성 목록(묶음별 토큰)    | `ContextList` 항목별 목록(`context-list.tsx` `ItemRow`)             |
| "요청마다 다시 실려서 5시간 한도의 ~X%" | `ContextList` 설명 줄(`context-list.tsx` `ContextBody` description) |
| 최근 5시간 요청·캐시 적중률 카드        | `MetricLines`(`dashboard.tsx` 에이전트 카드)                        |

남긴 것: 창별 큰 값·막대·페이스 줄, 줄일 수 있는 것 입구 줄, 최근 구간 막대, 시작 컨텍스트 한 줄(라벨 → 값).

최근 구간 막대는 커서를 올린 막대의 시각·값을 제목 줄 오른쪽("최고 …" 자리)에 바로 보인다. 브라우저 `title` 툴팁은 늦게 떠서 뺐다. 지표 조회는 `UsageBars`가 한다(트레이엔 `MetricLines`가 없다).

컴포넌트가 에이전트 공용이라 Codex·Kiro 탭도 같이 줄어든다(Kiro 일별 크레딧 막대, Kiro 요청 수 포함, 모두 대시보드에 있다).

## 코드

- `StartupSummary`(트레이 한 줄) 신설. `MetricLines`는 지표 카드만 그리고 `hasStartup`을 없앴다.
- 쓰는 곳이 없어진 `CONTEXT_GROUPS`·`startupGroups`·`startupLine`과 번역 `metrics.groups`·`startupParts`를 지웠다.
