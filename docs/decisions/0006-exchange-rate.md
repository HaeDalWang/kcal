# 0006. 2층 환율 (첫 범위)

- 상태: 채택
- 날짜: 2026-11-04
- 범위·규칙: [0003](0003-product-scope.md) "2. 환율", "숫자 표기 규칙". 근거: ctxmeter `develop/decisions/08-agent-health-direction.md` "재회귀 결과", `09-agent-health-vision.md`. 원본 프로토타입: `~/.local/share/agent-usage-audit/codex_rate.py`(회귀 B만 이식)

## 결정

| 결정                                                                                                                                                                                                                                                                                                                                                                                                                                          | 이유                                                                                                                                                        |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust 모듈 `src-tauri/src/rate/`(`codex.rs` 수집·창·판정, `fit.rs` NNLS·R², `kiro.rs`, `history.rs`), command `get_rates()` → `RateSnapshot { codex, kiro, claude }`. TS 짝은 `src/lib/ipc.ts` 같은 이름                                                                                                                                                                                                                                       | 3층 분해가 이 환율을 입력으로 쓴다                                                                                                                          |
| Codex: 5시간(primary) **창 단위** 회귀. 창마다 (비캐시 입력, 캐시 입력, 출력) 토큰 합 → 창 안 최대 `used_percent`. 절편 없는 NNLS(Lawson–Hanson, 열 노름 스케일, 정규방정식 + ridge 1e-10)                                                                                                                                                                                                                                                    | 08 재회귀: 1%p 구간 단위는 Δ%가 거의 다 1이라 검증 R² −1 수준, 창 단위만 0.9대. 모델·effort별 출력(C)은 드문 조합 때문에 과적합. 음수 계수는 뜻이 없어 NNLS |
| 수집·중복 제거·창 배정은 `codex_rate.py`와 연산 순서까지 같게: 앞 400자에 `"token_count"`가 없는 줄은 JSON 파싱하지 않음, `limit_id == "codex"`만, `(timestamp, total_token_usage)` 같은 이벤트 삭제, 같은 rollout에서 total이 직전과 같으면 토큰 0, `resets_at`을 600초(`RESET_TOL_S`) 이내로 묶어 창. 표본 수 < 변수 수 × 3이면 회귀하지 않음                                                                                               | 패리티(같은 입력에서 계수 ±1%, R² ±0.01). 로컬 조사 기록(비공개): 계수·R² 소수 6자리까지 같다                                                               |
| rollout 경로는 `context::paths::codex_root` + `fsutil::walk_files`(`sessions/`, `archived_sessions/`). `.jsonl.zst`는 건너뜀                                                                                                                                                                                                                                                                                                                  | 0003 "열린 것" 그대로                                                                                                                                       |
| Kiro: `messages.jsonl`의 `"payload":{"type":"usage_summary"`(앞 300바이트) 줄만 파싱, `promptTurnSummaries[]` 중 `unit == "credit"`인 `usage`를 턴 1개로 센다. 모델은 같은 폴더 `session.json`의 `modelId`(없으면 `unknown`). 모델별 턴 수·평균 크레딧/턴                                                                                                                                                                                     | 턴 요약에 모델이 없다(로컬 파일로 확인: 키는 `unit`·`unitPlural`·`usage`·`usedTools`). 모델 필드가 있는 `assistant` 줄은 본문 줄이라 읽지 않는다            |
| Claude: 한도 조회가 API(주 경로)로 성공할 때마다 `app_data_dir()/limit-history.jsonl`에 한 줄. 회귀는 하지 않고 기록된 5시간 창 수만 보여 준다                                                                                                                                                                                                                                                                                                | 08: Claude 환율은 표본 부족. statusLine 대체 기록 값은 이미 다른 파일에 있어 다시 쓰지 않는다                                                               |
| 마지막 결과를 `src-tauri/src/cache.rs`의 공용 `Cache`(managed state)에 둔다. `get_rates`는 값이 있으면 즉시 반환하고, 30분(`RATES_TTL`)이 지났으면 뒤에서 `spawn_blocking`으로 한 번만 다시 계산해 `rates-updated` event로 보낸다. 값이 없을 때 계산 중이면 그 결과를 같이 기다린다. 앱 시작 때 한 번 미리 계산한다. 입력 파일 서명 캐시는 없앴다(에이전트가 돌면 거의 매번 바뀌어 쓸모가 없었다). 응답에 `computedAt`(RFC 3339). 타이머 없음 | 팝오버를 열 때 계산을 기다리지 않는다. AGENTS.md 성능 원칙                                                                                                  |
| capability 변경 없음                                                                                                                                                                                                                                                                                                                                                                                                                          | 앱 command만 쓴다                                                                                                                                           |

## 검증과 표시 기준

- 검증 R² = min(창 단위 leave-one-out R², 시간순 holdout R²). holdout은 앞 70% 창으로 학습, 뒤 30%로 평가(창 4개 이상일 때). holdout이 없으면 LOO.
- 판정(프로토타입과 같다): R² ≥ 0.7 → `estimated`(1%p당 토큰 숫자), 0.3 ≤ R² < 0.7 → `range_only`(LOO fold 범위만), 그 아래 → `unreliable`, 창 < 9개 → `insufficient`, 이벤트 없음 → `no_data`.
- 결과: 1%p당 토큰(캐시 입력·비캐시 입력·출력, 계수 0이면 `null`), LOO fold 범위(어떤 fold에서 0이면 상한 `null`), 검증 R², 창 수, 제외한 창 수(`used_percent`가 없어 빠진 묶음).
- 표기(0003): Codex는 "(추정, R² 0.94, 창 28개)", Kiro는 "(실측)". Claude는 "환율: 기록 쌓는 중 (창 N개)".

## 한도 기록 파일

- 위치: `app_data_dir()/limit-history.jsonl`(macOS `~/Library/Application Support/io.github.haedalwang.kcal/`).
- 한 줄: `{"at": RFC3339, "fiveHour": {"usedPercent": n, "resetsAt": RFC3339|null}, "sevenDay": {...}}`. 시각과 %만. 토큰·인증 정보·계정 id 없음.
- 직전 줄과 두 창의 %가 같고 리셋 시각이 600초 이내(같은 창)면 쓰지 않는다. 쓰기 실패는 stderr 로그만 남기고 조회·트레이 갱신은 계속한다.
- 크기 상한 없음(`ponytail:` 주석, 하루 최대 약 43KB). 커지면 회전을 넣는다.

## 실측 (이 Mac, 2026-11-04)

- Codex: rollout 58개, 이벤트 6,673개, 5시간 창 27~28개. 검증 R² 0.93~0.94, 1%p ≈ 캐시 입력 1.9M · 비캐시 입력 50K · 출력 3.0K. 08의 값(창 40개, 742K·66K·5.5K)과 다른 것은 디스크의 rollout 집합이 바뀐 탓이다(같은 입력에서 Python과 일치).
- Kiro: 턴당 크레딧 auto 0.99 · sonnet-5.5 9.50 · opus-5.5 16.05 · sonnet-5 2.00 · opus-5 39.19.
- 첫 계산은 debug 빌드에서 약 4.7초(rollout·Kiro 세션 약 1.4GB를 줄 단위로 훑음). 이후 호출은 캐시를 바로 돌려준다.

## 이번에 안 한 것

- Claude 환율 회귀, Codex·Claude 주간(secondary) 회귀: 표본 부족(이 Mac Codex 주간 창 5개). 기록이 쌓인 뒤 별도 작업.
- 회귀 A(1%p 구간)·C(모델·effort별 출력): 08에서 검증 실패.
- 파일별 증분 캐시: 다시 계산할 때 전체를 읽는다(`ponytail:` 주석). 30분마다 한 번 CPU·디스크가 부담되면 파일별 (mtime, 크기) 캐시.
- `.jsonl.zst` 압축 rollout(0003 열린 것), 세션 중 모델 변경의 Kiro 턴별 모델 귀속.

## 2026-11-11 Claude 5시간 환율

Claude도 Codex와 같은 창 단위 NNLS·검증 R²·판정(`fit.rs`, `codex::judged`)으로 계산한다. `RateSnapshot.claude`는 `ClaudeHistory { windows }` 대신 `ClaudeRate`(= `CodexRate`, `composition`은 `None`, `sessions`는 빈 목록). 모듈 `rate/claude.rs`.

| 결정                                                                                                                                                                                                   | 이유                                                                                                                                                           |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 요청: `~/.claude/projects/**/*.jsonl` assistant 줄 `message.usage`(`metrics::claude`와 같은 표식·`log_files`·`read_lines`). 같은 `message.id`는 `output_tokens`가 가장 큰 줄 하나(같으면 먼저 읽은 줄) | Claude Code는 메시지 하나를 스트리밍 중간 줄 여러 개로 쓴다. 입력 값은 같고 출력만 늘어난다                                                                    |
| 창: 한도 기록 `fiveHour.resetsAt`을 `resetTolSecs`로 묶는다. y = 창 안 최대 %. 토큰 = [max(리셋 − 5시간, 직전 창 리셋), 그 최대 %를 처음 기록한 시각] 안 요청의 합                                     | 기록은 5분 조회라 최대 % 기록 뒤의 요청은 y에 들어갈 수 없다. 한도가 일찍 초기화되면 창이 겹친다(이 Mac 11-10 한 번): 직전 창 리셋 앞 요청은 두 번 세지 않는다 |
| 기록된 요청이 없는 창은 회귀에서 뺀다(`excludedWindows`)                                                                                                                                               | Codex는 이벤트에서 창을 만들어 이런 창이 없다. (0, 0) 행은 R²만 공짜로 올린다(이 Mac 4변수 시험에서 0.12 → 0.42)                                               |
| 변수는 Codex와 같은 3개: 비캐시 입력 = `input_tokens` + `cache_creation_input_tokens`, 캐시 입력 = `cache_read_input_tokens`, 출력                                                                     | Codex의 비캐시 입력도 "캐시에서 읽지 않은 입력"이다. Claude Code의 `input_tokens`는 창당 1K 안팎으로 캐시 쓰기의 0.1% 미만. `TokenRate`·몫 계산을 그대로 쓴다  |
| 판정 기준·표본 하한은 Codex와 같은 tuning(`goodR2` 0.7, `rangeR2` 0.3, 변수 × `minSamplesPerVar`). 새 tuning 없음                                                                                      | 같은 숫자 규칙(0003)                                                                                                                                           |
| 화면: `estimated`면 Codex와 같은 "1%당 토큰"·"사용 패턴" 섹션. 아니면 "기록 모으는 중 (5시간 구간 N개)"(N = 회귀에 쓴 창), 자세히에 "R² … · 5시간 구간 N개"                                            | 판정을 통과하지 못한 숫자는 보이지 않는다                                                                                                                      |
| 모델별 변수·이상치 제외는 하지 않는다                                                                                                                                                                  | 아래 실측: 결과를 보고 고른 제외(새 모델 창 2개, 99% 창 1개)만 R²를 0.97로 올리고, 그때 계수도 뜻이 없다(비캐시 입력 1%당 44 토큰, 캐시 읽기 0)                |

패리티: `~/.local/share/agent-usage-audit/claude_rate.py`(`codex_rate.py`의 `fit_eval` 사용)와 Rust `#[ignore]` 테스트 `claude_parity_on_real_logs`(`KCAL_PARITY_NOW`). 같은 입력에서 요청 수·창 수·R²·계수·창 패턴이 소수 6자리까지 같다.

### 실측 (이 Mac, 2026-11-11)

- 요청 13,316개, 한도 기록 5시간 창 27개(리셋 시각 서로 다른 값 41개를 600초로 묶음). 회귀 창 18개, 뺀 창 9개.
- 학습 R² 0.49, LOO 0.23, holdout −0.02 → 검증 R² −0.02 → `unreliable`. 화면은 "기록 모으는 중 (5시간 구간 18개)".
- 창 패턴(최근 14일, 실측): 창 18개, 창당 요청 중앙값 170 (p90 296.6), 캐시 미스 중앙값 4 (p90 11).
- 맞지 않는 창: 다른 창은 대체로 입력 23~26M에 43~46%인데, 11-09 10:29 창은 20M에 99%, 11-10·11-11 창은 7M대에 93%·69%(이 두 창만 `claude-fable-5-2` 모델이 입력의 약 11%). 로그에 없는 사용(claude.ai·다른 기기)인지 모델 가중치인지는 모른다.

### 이번에 안 한 것

- 주간(7일) 회귀: 주간 창 4개.
- 모델별 환율: 새 모델이 든 창이 2개뿐이라 LOO에서 그 계수가 서지 않는다. 창이 쌓이면 다시 본다.
- Claude 기록 읽기 캐시: 다시 계산할 때 630MB를 전부 읽는다(위 `ponytail:`).

## 2026-11-16 Claude도 Codex와 같은 화면

Claude 환율은 `estimated`가 아니면 "기록 모으는 중 (구간 N개)"만 보였다. 그래서 구간이 35개라 수는 충분하고 R²가 0.64(`range_only`)인데도 "기록이 더 쌓이면 보여 줘요"라고 안내했다. 이제 Claude도 Codex와 같은 화면을 쓴다(구간이 0개일 때만 따로 안내). `range_only`이면 맨 위 값에 fold 범위로 계산한 1%당 토큰 범위(`~하한~상한`, 두 항 모두 상한이 없으면 `≥하한`)를 보여 준다. 이 범위 표시는 Codex에도 똑같이 적용된다.

## 2026-11-21 Claude 최근 N일 창

2026-11-17 Pro → Max 5x 플랜 업그레이드로 %당 토큰이 약 5배 바뀌었다. 전후 창을 섞으면 검증 R² −38.7, 전만 0.71, 후만 0.56이다.

Claude 환율은 리셋 시각이 `now - advanced.claudeRateDays일` 이상인 창만 회귀한다(기본 7일, 범위 3..=90일). 기간 밖 창은 `excludedWindows`에 세지 않는다. 이 값은 기간 안에서 기록된 요청이 없는 창 수를 유지한다. 창 패턴의 `breakdownDays`와 Codex 환율은 그대로 둔다.

한계: 플랜·한도가 바뀌면 N일 동안 다시 모으는 중이며, 기간 필터가 회귀 신뢰도 통과를 보장하지는 않는다.
