# 0016. 6층 정기 검진 1차: 처방 전후 비교와 주간 요약

- 상태: 채택(구현 전, 2026-11-06)
- 후속: [0028](0028-why-limit-burned.md) 주간 요약에 자동 실행·모델 몫, Codex 환율 비교
- 날짜: 2026-11-06
- 앞선 결정: [0003](0003-product-scope.md) "6. 정기 검진", [0014](0014-coaching.md)(적용 기록 `coach-log.jsonl`), [0015](0015-dashboard-daily-activity.md)(대시보드·주간 %p 증가분), [0007](0007-tuning.md). 근거: ctxmeter `develop/execution/15-health-and-compare.md`, `src/health.js`

## 문제와 목표

코칭으로 스위치를 바꿀 수 있지만, 그게 실제로 아꼈는지 볼 방법이 없다. 0003은 처방 전후 비교표를 6층의 핵심 기능으로 정했다.

완료 기준: 대시보드 창 "처방 효과" 섹션에 ① 주간 요약(최근 N일 vs 그 전 N일) ② 코칭 적용 기록마다 적용 전 N일 vs 적용 후 N일이 같은 지표로 나오고, 덜 지난 구간·데이터가 모자란 구간이 그렇게 표시된다.

바뀌지 않는 것: 코칭 적용·되돌리기, 기존 metrics·한도·트레이 화면. capability·의존성 추가 없음.

## 결정 (사용자 선택 2026-11-06)

| 결정                                                                                                                    | 이유                                                                                        |
| ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| 1차 범위 = 전후 비교표 + 주간 요약. 알림·주 1회 타이머는 다음 단계                                                      | 알림은 plugin·capability가 필요하고 ctxmeter에서 macOS 알림이 안 떴다                       |
| 지표 = 요청당 평균 입력 토큰, 하루 평균 주간 한도 %p, Codex 압축 횟수. Kiro는 토큰이 없어 질문당 크레딧·하루 크레딧     | 코칭은 시작 컨텍스트를 줄이는 처방이라 요청당 입력이 직접 효과, %p는 사용자가 체감하는 효과 |
| 구간은 기준 시각 기준 정확히: 전 = [t−N일, t), 후 = [t, t+N일). N = tuning `compareDays`(기본 7, 1~30)                  | ctxmeter는 UTC 0시로 잘라 하나씩 어긋났다                                                   |
| 주간 요약은 같은 계산에 t = 지금 − N일                                                                                  | 함수 하나로 두 기능                                                                         |
| 후 구간이 덜 지났으면 지난 일수(`elapsedDays`)로 나눈 하루 평균 + "N일째" 표시                                          | 기다리는 동안에도 추세를 본다                                                               |
| 데이터가 구간을 다 못 덮으면(첫 기록이 구간 시작보다 늦음) `coverage` < 1로 표시, 값이 없으면 `null`(0으로 그리지 않음) | Claude 주간 %p는 앱이 켜져 있을 때만 쌓인다(0015)                                           |
| 적용 기록은 그 기록의 에이전트 지표만, 되돌린 기록은 "되돌림(시각)" 표시와 함께 후 구간을 되돌린 시각에서 자른다        | 되돌린 뒤까지 섞으면 효과가 흐려진다                                                        |
| 화면은 대시보드 창. 팝오버 코칭 화면의 적용 기록 줄에는 "효과 보기"(대시보드 열기)만                                    | 표가 넓다(팝오버 폭 310)                                                                    |

## 데이터

- 요청당 입력 토큰: Claude 세션 로그(`input + cache_read + cache_creation`, message id 중복 제거, metrics/claude와 같은 읽기), Codex rollout `last_token_usage.input_tokens`(ctxmeter `foldRollout`과 같은 값).
- 주간 %p: 0015와 같은 증가분(Codex `rate_limits.secondary`, Claude `limit-history.jsonl` `sevenDay`). Kiro는 턴 크레딧 합.
- Codex 압축: rollout `type == "compacted"` 줄 수.
- 계산은 Rust(spawn_blocking), 결과는 캐시 + event로 준다. polling 없음.

## IPC

| command      | 입력 | 출력                                                                                                     |
| ------------ | ---- | -------------------------------------------------------------------------------------------------------- |
| `get_health` | 없음 | `HealthReport`(computedAt·compareDays·weekly: `AgentComparison[]`·prescriptions: `PrescriptionEffect[]`) |

Event `health-updated`(payload `HealthReport`). Rust `src-tauri/src/health/mod.rs` ↔ TS `src/lib/ipc.ts` 같은 이름, fixture `src/test/fixtures/ipc/health-report.json`.

- `AgentComparison` = `{ agent, before: PeriodStats, after: PeriodStats }`(주간 요약 한 줄, 켠 에이전트, `Agent::ALL` 순서).
- `PrescriptionEffect` = `{ id, at, agent, name, enabled, undoneAt: string | null, before, after }`(적용 기록 하나, 최근순).
- `PeriodStats` = `from`·`to`(끝 미포함)·`elapsedDays`·`coverage: number | null`·`requests: number | null`·`avgInputTokens: number | null`(Claude·Codex)·`avgCredits: number | null`(Kiro, 질문당)·`dailyUsed: number | null`·`usedUnit: BarUnit`(percent = 주간 %p, credits)·`compactions: number | null`(Codex만).

## 구현하며 정한 것 (2026-11-06)

- **Kiro 질문당 크레딧은 `avgCredits`** 로 따로 둔다. `avgInputTokens`는 Kiro에서 늘 null. 한 필드에 단위가 바뀌는 값을 넣지 않으려고.
- **coverage는 기록마다 잰 뒤 작은 쪽**이다. Claude는 요청(세션 로그)과 주간 %p(`limit-history.jsonl`)의 시작이 달라, 각자 덮은 비율을 재고 `coverage` = 둘 중 작은 값. 덮은 비율이 0인 기록의 값만 null로 둔다(예: 한도 기록이 구간 뒤에 시작하면 `dailyUsed`만 null, 요청 값은 남는다). 기록이 하나도 없으면 coverage도 null.
- **"가장 이른 기록"**: 읽는 기간(`since`) 전에 바뀐 파일이 있으면 기록이 그 전부터 있었다고 본다(덮은 비율 1). 기간 안 파일만 열기 때문에, 이렇게 하지 않으면 쉬던 날이 "기록 없음"으로 보인다.
- 하루 평균(`dailyUsed`)은 쓴 양 기록이 덮은 일수(`elapsedDays` × 그 기록의 덮은 비율)로 나눈다. 구간 일수로 나누면 기록이 일부뿐인 쪽이 작게 보여 전후가 틀어진다(2026-11-06 실데이터: Claude 덮은 비율 0.27에서 하루 %p가 약 1/4로 보였다). coverage < 1이면 화면이 "기록 일부만"을 붙인다.
- `compactions`는 압축 줄 수(같은 원문 timestamp는 fork rollout끼리 한 번만). 요청 기록이 없으면 null. 화면은 구간 길이가 달라도 비교되게 "하루 압축"(÷ `elapsedDays`)으로 보인다.
- 처방 카드는 **켠 에이전트의 적용 기록만**(꺼진 에이전트는 로그를 읽지 않는다). 처방 시각이 지금보다 늦으면(시계 차이) 후 구간 길이 0 → 값 null.
- Claude 요청은 기간 안에 바뀐 세션 로그를 **전부** 읽는다(`claudeLogFiles` 상한을 쓰지 않음). 상한 50개로는 14일을 못 덮어 전 구간이 늘 "일부만"이 됐다. 이 Mac 실측 계산 2.3초(release, 세 에이전트). 무거워지면 파일별 결과 캐시(`ponytail:` 표시).
- 캐시 TTL은 `metricsTtlSecs`를 같이 쓴다. 적용 기록 줄 수·`compareDays`·켠 에이전트가 캐시와 다르면 옛 값을 주고 뒤에서 다시 계산한다(적용·되돌리기 직후 대시보드를 열어도 새 처방이 event로 들어온다). 새로고침(`refresh_all`)도 다시 계산한다. 앱 시작 때 미리 계산하지 않는다(대시보드를 열 때 처음 계산).
- 기존 코드는 열기만 했다: `metrics::{DAY_SECS, at, unix_secs, modified_secs, weekly_increments}`·`metrics::claude`(`weekly_used`, 새 `request_inputs`는 기존 `read_requests`·`log_files`와 dedup을 묶은 것), `rate::codex::{char_prefix, contains}`.
- 화면: 대시보드 하루 활동 카드 아래 "처방 효과" 카드. 변화는 "↓20%"(적을수록 아낀 것, 굵게)·"↑50%"(`text-warn`)·1% 미만 "비슷". 팝오버 진단하기의 "최근 바꾼 것" 줄에 "효과 보기"(대시보드 열기). tuning `compareDays`는 설정 "탐색 범위" 절.
