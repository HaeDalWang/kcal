# 0028. 한도가 왜 빨리 닳았나: 주간 요약에 자동 실행·모델 몫과 Codex 환율 비교

- 상태: 채택(2026-11-19 사용자 승인, 구현 전)
- 날짜: 2026-11-19
- 앞선 결정: [0016](0016-health-compare.md) 처방 전후 비교·주간 요약, [0006](0006-exchange-rate.md) 환율, [0007](0007-tuning.md) tuning, [0009](0009-ui-plain-language.md) 쉬운 말
- 근거(사용자 요구, gh api 2026-11-19 확인): anthropics/claude-code#16157(👍697), #38335(👍476), openai/codex#28879(👍364), headless `claude -p`가 5시간 창을 약 1.8배 더 쓴다는 제보 #97074(제보자 측정)

## 벤더와 나눈 몫

Claude Code `/usage`의 "What's contributing to your usage?"(확인됨: claude 2.1.328 바이너리의 스키마와 그리기 코드)는 이 Mac의 세션 기록을 읽어 **지난 24시간·7일**을 보여 준다. 행동 범주 9개(`cache_miss`, `long_context`, `subagent_heavy`, `high_parallel`, `cron`, `max_effort`, `workflow`, `always_on_context`, `large_tool_results`), 에이전트·스킬·플러그인·MCP 서버별 몫, 추천, 고치기. claude.ai 구독자만.

kcal은 벤더가 안 하는 것만 한다: 대화형·자동 실행 구분(벤더 범주에 없음), `compareDays`(최대 30일) 전후 비교, Codex.

## 결정

| 결정                                                                                                                                                                                                | 이유                                                                                 |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| 0016 주간 요약(최근 N일 vs 그 전 N일)에 지표를 더한다. 새 섹션·탭 없음                                                                                                                              | 같은 구간 계산이 이미 있다                                                           |
| 자동 실행 몫 = 자동 실행 요청의 입력 토큰(캐시 포함) ÷ 전체 입력 토큰. Claude `entrypoint == "sdk-cli"`, Codex rollout 첫 줄 `originator == "codex_exec"`. 그 밖의 값·없는 값은 대화형(`ponytail:`) | 화면 이름 "자동 실행(`claude -p`·`codex exec`)". 0016 "요청당 입력 토큰"과 같은 단위 |
| 모델별 몫 = 같은 입력 토큰 기준, 큰 3개 + 기타                                                                                                                                                      | 모델이 바뀌어 닳는 속도가 바뀐 것을 본다                                             |
| Codex 1%p당 토큰 = 전·후 구간 이벤트로 각각 기존 `rate::codex::rate()`. 판정(`min_samples_per_var`·`good_r2`·`range_r2`)이 환율·범위가 아니면 값 없음, 창 수는 같이 낸다                            | "토큰당 소모가 늘었다"를 직접 본다. 새 tuning 없음                                   |
| Claude 1%p당 토큰은 비교하지 않는다                                                                                                                                                                 | R² −0.02로 "믿을 수 없음"(backlog). Claude 줄은 `/usage` 안내 한 줄                  |
| 자동 급변 판정·OS 알림 없음                                                                                                                                                                         | 판정 기준이 없고, 알림은 plugin·capability가 필요하다. 6층 정기 검진 2차로           |
| Kiro는 그대로                                                                                                                                                                                       | 질문당 크레딧 비교가 이미 있다                                                       |

## 확인한 데이터 (2026-11-19, 이 Mac)

- Claude `~/.claude/projects` 최근 300 세션 첫 `entrypoint`: `cli` 212, `sdk-cli` 74, `claude-vscode` 13, `claude-desktop` 1. user·assistant 줄에만 있다. 하위 에이전트(`subagents/*.jsonl`, `isSidechain: true`)는 부모와 같은 값.
- Codex rollout 첫 줄: `codex-tui`(`source: "cli"`) 42, `codex_exec`(`source: "exec"`) 10, `orca_desktop` 3.
- `sdk-cli` = `claude -p`/Agent SDK인지는 추정. `claude -p` 한 번 뒤 새 jsonl로 확인.

## 열린 것

- 자동 실행이 대화형보다 1%p당 더 닳는지(#97074)는 kcal 환율로 아직 말할 수 없다. Claude 환율이 믿을 만해지면 구분해 적합한다.

## 구현하며 정한 것 (2026-11-19)

- 기존 `Event`·`AgentSamples` 생성자가 범위 밖 모듈에도 있어 구조는 유지한다. Codex 파일 메모에 이벤트 인덱스별 모델·자동 실행 정보를 함께 둔다. 기존 `dedup` 결과에 연결하며 환율 계산·기존 토큰 결과는 바꾸지 않는다. Claude는 기존 `Entry`에 intern한 `entrypoint`를 함께 보관한다.
- 주간 요약만 후처리로 확장한다. 기존 처방 지표·계산은 그대로 두고 새 필드는 null. 파일은 기존 메모를 재사용하며 주기 갱신은 추가하지 않는다.
- 비중은 0~100의 백분율. 입력 토큰이 0이면 null. 모델 누락은 `unknown`이라는 항목으로 분모에 포함하고, 기타는 `model: null`로 구별한다. 동률은 모델 이름순, 기타는 네 번째 모델부터의 합계이며 남은 모델이 없으면 생략한다.
- Codex는 해당 구간의 이벤트만 기존 `rate(events, tuning.advanced)`에 넘긴다. 상태·창 수를 보존하고 `estimated`의 점 값·범위 또는 `range_only`의 범위만 내보낸다. UI는 캐시 입력·비캐시 입력·출력 각각의 값을 보여 단일 환율을 새 식으로 만들지 않는다. 숫자는 `~`, 값이 없으면 `—`, 창 수는 항상 함께 보인다.
- 기존 비교 UI의 아래쪽에 자동 실행·모델 비중·Codex 환율 행을 둔다. 비중 증감을 절약/낭비로 판정하지 않는다. Claude에는 `/usage` 한 줄을 둔다. 한국어·영어와 빈 상태를 모두 처리한다.
