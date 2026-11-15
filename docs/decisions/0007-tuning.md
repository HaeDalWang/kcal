# 0007. 조절 가능한 값을 settings `tuning`으로

- 상태: 채택
- 날짜: 2026-11-04
- 관련: [0004](0004-usage-and-tray.md) "설정", [0003](0003-product-scope.md) "주기 측정", [0006](0006-exchange-rate.md) "검증과 표시 기준"

## 결정

| 결정                                                                                                                                                                                                                                                                          | 이유                                                                                                                                                |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| 주기·시간 초과·탐색 범위·추정 비율·환율 판정 기준을 코드 상수에서 `src-tauri/src/tuning.rs`의 `Tuning`으로 옮겼다. `Settings.tuning`으로 들고 `settings.json`의 `tuning` 객체에 저장한다. TS 짝은 `src/lib/ipc.ts`의 `Tuning`·`AdvancedTuning`. **기본값은 예전 상수와 같다** | 사용자가 나중에 설정 패널에서 조절한다. 이번 범위는 값을 한 곳에 모으고 settings로 덮어쓰는 데까지이고 패널은 없다                                  |
| 사용하는 쪽은 상수 대신 호출 시점의 현재 settings를 읽는다. 순수 함수는 값을 인자로 받고, command·스케줄 경계(`schedule.rs`, `rate::rates`, `context::global_context`/`get_context`, `rate::record_claude`)에서 `SettingsState::tuning()`을 읽어 넘긴다. 전역 static은 없다   | 테스트에서 값만 바꿔 넘길 수 있다. 새 polling 없이 기존 흐름(조회 타이머가 설정 변경 `Notify`를 기다림, 캐시는 호출이 들어올 때 판단)을 그대로 쓴다 |
| 파일을 읽을 때 필드 단위 검증: 모르는 필드·형식이 틀린 필드·범위 밖 필드는 그 필드만 기본값으로 두고 `eprintln!`으로 이름과 이유를 남긴다. `reason`(화면 표시)은 건드리지 않는다. `tuning`이 없는 기존 파일은 전부 기본값                                                     | 앱이 죽거나 설정 전체를 버리지 않는다                                                                                                               |
| `set_settings(enabledAgents, tuning?)`: `tuning`을 주면 같은 검증을 해서 하나라도 범위 밖이면 `invalid_input`(필드 이름 포함)으로 거부한다. 생략하면 지금 tuning을 유지한다. 주면 통째로 바꾼다(빠진 필드는 기본값). 저장 파일에는 항상 전체 `tuning`을 쓴다                  | 에이전트 토글(`setSettings(agents)`)이 tuning을 지우지 않는다. 별도 `update_settings` command는 만들지 않았다(기존 `set_settings`를 늘렸다)         |
| 변경은 다음 조회·다음 계산부터 반영한다. 한도 조회: 저장 시 타이머가 깨어 새 값을 읽지만 이미 잡힌 예정 시각은 그대로(간격을 줄여도 직전에 잡은 대기가 끝나야 반영, `ponytail:` 주석). 환율·컨텍스트: 캐시된 값은 TTL이 지난 다음 호출에서 새 기준으로 다시 계산한다          | 즉시 재계산·타이머 재설정은 CPU와 복잡도를 늘린다(성능 원칙)                                                                                        |
| `Cache`는 TTL을 만들 때 받지 않고 `get` 호출마다 받는다                                                                                                                                                                                                                       | 설정이 바뀌면 다음 호출부터 새 TTL이 적용된다                                                                                                       |
| `ContextItem.tokens`는 항목을 만들 때 비우고 `AgentContext::new(…, bytes_per_token)`가 `bytes`로 채운다                                                                                                                                                                       | 추정 비율을 `scan`·`instructions`의 모든 생성 경로로 넘기지 않고 한 곳에서 적용한다                                                                 |
| 프론트의 문자 → 토큰 환산(`ContextList`)은 `settings.tuning.bytesPerToken`을 prop(`charsPerToken`)으로 받는다. TS에 같은 값을 따로 두지 않는다                                                                                                                                | 바이트 ÷ N(Rust)과 문자 ÷ N(화면)이 같은 N을 쓰게 한다                                                                                              |
| HTTP 시간 초과는 클라이언트 생성 때가 아니라 요청마다(`RequestBuilder::timeout`) 건다                                                                                                                                                                                         | 클라이언트는 앱 수명 동안 하나이고 값은 바뀔 수 있다                                                                                                |
| `next_delay`: 429 뒤 늘린 간격은 `min(maxBackoff)` 후 `max(base)`. 예전 `clamp(base, max)`는 `base > max`이면 패닉한다                                                                                                                                                        | 사용자가 조회 간격을 최대 백오프보다 크게 잡을 수 있다. 그때 간격은 base 아래로 내려가지 않는다                                                     |
| capability·`Cargo.toml` 변경 없음                                                                                                                                                                                                                                             | 앱 command와 기존 의존성만 쓴다                                                                                                                     |

## 값, 기본값, 범위

범위는 양 끝 포함이고 `src-tauri/src/tuning.rs`에 필드마다 이름 있는 상수(`*_RANGE`)로 있다.

| JSON 필드                   | 예전 상수(위치)                                                                 | 기본값         | 범위               | 의미                                                                           |
| --------------------------- | ------------------------------------------------------------------------------- | -------------- | ------------------ | ------------------------------------------------------------------------------ |
| `apiIntervalSecs`           | `API_INTERVAL` (`usage/schedule.rs`)                                            | 300 (5분)      | 60 ~ 86400         | Claude·Codex 한도 조회 간격                                                    |
| `kiroIntervalSecs`          | `KIRO_INTERVAL` (`usage/schedule.rs`)                                           | 600 (10분)     | 60 ~ 86400         | Kiro 한도 조회 간격                                                            |
| `refreshMinGapSecs`         | `REFRESH_MIN_GAP` (`usage/schedule.rs`)                                         | 30             | 5 ~ 600            | 새로고침을 연달아 눌러도 같은 에이전트를 이보다 자주 조회하지 않음             |
| `maxBackoffSecs`            | `MAX_BACKOFF` (`usage/schedule.rs`)                                             | 3600 (60분)    | 60 ~ 86400         | 429 뒤 간격을 두 배씩 늘릴 때 상한                                             |
| `ratesTtlSecs`              | `RATES_TTL` (`rate/mod.rs`)                                                     | 1800 (30분)    | 60 ~ 86400         | 이만큼 지나 호출되면 환율을 뒤에서 다시 계산                                   |
| `contextTtlSecs`            | `CONTEXT_TTL` (`context/mod.rs`)                                                | 600 (10분)     | 60 ~ 86400         | 이만큼 지나 호출되면 시작 컨텍스트를 다시 측정                                 |
| `httpTimeoutSecs`           | `HTTP_TIMEOUT` (`usage/sources.rs`)                                             | 15             | 1 ~ 300            | Claude·Codex 사용량 API 요청                                                   |
| `keychainTimeoutSecs`       | `KEYCHAIN_TIMEOUT` (`usage/sources.rs`, macOS)                                  | 10             | 1 ~ 300            | `security` 실행                                                                |
| `kiroTimeoutSecs`           | `KIRO_TIMEOUT` (`usage/sources.rs`)                                             | 30             | 1 ~ 300            | `kiro-cli` 실행(Windows 실측 21초)                                             |
| `codexSessionsToSearch`     | `SESSIONS_TO_SEARCH` (`context/codex_session.rs`)                               | 50             | 1 ~ 500            | 작업 폴더로 거를 때 훑는 최근 Codex rollout 수                                 |
| `kiroSessionsToSearch`      | `SESSIONS_TO_SEARCH` (`context/kiro_session.rs`)                                | 50             | 1 ~ 500            | 훑는 최근 Kiro 세션 수                                                         |
| `shapeSessionsToSearch`     | `SESSIONS_TO_SEARCH` (`context/shape.rs`)                                       | 5              | 1 ~ 500            | `api-request-shape`를 찾는 최근 Claude 세션 수                                 |
| `codexRolloutsChecked`      | `CODEX_ROLLOUTS_CHECKED` (`usage/sources.rs`)                                   | 3              | 1 ~ 500            | Codex 한도 대체 경로가 보는 최근 rollout 수                                    |
| `claudeLogTailBytes`        | `CLAUDE_LOG_TAIL_BYTES` (`usage/sources.rs`)                                    | 65536 (64 KiB) | 4096 ~ 16777216    | Claude statusLine 기록에서 끝부터 읽는 양                                      |
| `bytesPerToken`             | `BYTES_PER_TOKEN` (`context/fsutil.rs`), `CHARS_PER_TOKEN` (`context-list.tsx`) | 4              | 1 ~ 16             | 바이트(문자) ÷ 이 값 = 토큰 추정                                               |
| `lowPercent`                | `LOW_PERCENT` (`usage-popover.tsx`)                                             | 20             | 1 ~ 99             | 남은 %가 이 값 이하면 막대를 경고색으로(화면 전용)                             |
| `metricsTtlSecs`            | 없음(새 값, [0010](0010-tray-and-dashboard.md))                                 | 300 (5분)      | 60 ~ 86400         | 이만큼 지나 호출되면 트레이 지표(최근 5시간 요청·캐시 적중률·막대)를 다시 계산 |
| `recentBars`                | 없음(새 값, 0010)                                                               | 8              | 1 ~ 48             | 트레이 막대 그래프의 최근 5시간 구간 수(Claude·Codex)                          |
| `barDays`                   | 없음(새 값, 0010)                                                               | 7              | 1 ~ 90             | 막대 그래프가 보는 최근 기간(일). Kiro는 이 수만큼 하루(24시간) 막대           |
| `claudeLogFiles`            | 없음(새 값, 0010)                                                               | 50             | 1 ~ 1000           | 최근 5시간 요청을 셀 때 여는 Claude 대화 기록 수(최근에 바뀐 순)               |
| `advanced.goodR2`           | `GOOD_R2` (`rate/codex.rs`)                                                     | 0.7            | 0 ~ 1, ≥ `rangeR2` | 검증 R² 이상이면 환율 숫자(`estimated`)                                        |
| `advanced.rangeR2`          | `RANGE_R2` (`rate/codex.rs`)                                                    | 0.3            | 0 ~ 1, ≤ `goodR2`  | 이상이면 범위만(`range_only`), 미만은 `unreliable`                             |
| `advanced.resetTolSecs`     | `RESET_TOL_S` (`rate/codex.rs`)                                                 | 600            | 1 ~ 3600           | 같은 5시간 창으로 묶는 `resets_at` 흔들림                                      |
| `advanced.holdoutTrain`     | `HOLDOUT_TRAIN` (`rate/fit.rs`)                                                 | 0.7            | 0.1 ~ 0.9          | 시간순 holdout에서 학습에 쓰는 앞쪽 창 비율                                    |
| `advanced.minSamplesPerVar` | `MIN_SAMPLES_PER_VAR` (`rate/codex.rs`)                                         | 3              | 1 ~ 20             | 창 수 < 변수 수(3) × 이 값이면 `insufficient`                                  |

`goodR2 < rangeR2`이면 두 필드를 모두 범위 밖으로 본다. 파일에서는 필드를 키 이름순으로 하나씩 받아들이므로(`goodR2`가 먼저), 쌍이 어긋난 파일은 어긋나게 만든 쪽만 기본값으로 돌아간다.

`holdoutTrain`의 상한이 0.9인 이유: 1.0이면 시험 구간이 비고, 0이면 학습 구간이 빈다. 코드는 인덱스를 마지막 창으로 자르기도 한다.

## 고급 묶음(`tuning.advanced`)을 나눈 이유

`goodR2`·`rangeR2`·`resetTolSecs`·`holdoutTrain`·`minSamplesPerVar`는 "추정" 표기를 붙이는 기준이다(0003 "숫자 표기 규칙", 0006). 값을 바꾸면 같은 데이터에서도 `estimated`와 `range_only`가 갈리므로 "추정"이 가리키는 신뢰 수준이 달라진다. 주기·시간 초과처럼 성능과 편의만 바꾸는 값과 섞어 두면 설정 패널에서 실수로 건드리기 쉽다. 그래서 JSON에서 묶음으로 분리하고, 패널도 기본은 접어 둘 것을 전제로 한다. 판정 기준을 바꾸면 이미 캐시된 환율은 다음 재계산 때 새 기준으로 바뀐다.

## 상수로 남긴 것

| 종류                  | 예                                                                                                          | 이유                                                          |
| --------------------- | ----------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| URL·서비스 이름       | `CLAUDE_USAGE_URL`, `CODEX_USAGE_URL`, `CLAUDE_KEYCHAIN_SERVICE`, `FIRST_PARTY_HOST`                        | 외부 계약. 바꾸면 조회가 깨진다                               |
| 파일 이름·이벤트 이름 | `settings.json`, `limit-history.jsonl`, `*_UPDATED_EVENT`, 창·트레이 id                                     | 앱 내부 계약                                                  |
| 로그 마커 바이트열    | `TOKEN_COUNT_MARK`, `SUMMARY_MARK`, `START_MARKER`, `CODEX_RATE_LIMITS_MARK`, `PREFIX_CHARS`·`PREFIX_BYTES` | 에이전트가 쓰는 로그 형식                                     |
| 수치 해법 허용오차    | `NNLS_TOL`, `NNLS_MAX_ITER`, `RIDGE`, `SINGULAR`                                                            | 파이썬 프로토타입과 패리티(0006). 사용자가 건드릴 값이 아니다 |
| 에이전트 쪽 사실      | `MAX_IMPORT_HOPS`(Claude `@import` 깊이 4), `CODEX_DEFAULT_MAX_BYTES`(Codex 기본 32 KiB), `VARIABLES`       | 에이전트의 동작을 그대로 흉내 낸 값. 바꾸면 측정이 틀린다     |
| 기술 한계             | `HOLDOUT_MIN_GROUPS`(holdout을 하려면 창 4개), 한도 기록 `TAIL_BYTES`                                       | 알고리즘이 성립하는 최소치                                    |
| UI 레이아웃           | 팝오버 높이 범위, `BLUR_CLICK_GUARD`, 창 위치                                                               | 화면 구성. 사용자 설정이 아니다                               |

## 코드를 읽다 더 찾은 것

- 추가로 `tuning`에 넣음: `advanced.minSamplesPerVar`(`rate/codex.rs`의 `MIN_SAMPLES_PER_VAR`). 환율을 낼지 말지 정하는 임계값이라 같은 고급 묶음에 넣었다.
- 설정 패널 작업에서 넣음(2026-11-05): `lowPercent`(예전 `LOW_PERCENT = 20`, `usage-popover.tsx`). Rust가 다른 필드와 같이 검증·저장하고, `App`이 `settings.tuning.lowPercent`를 `UsagePopover`의 `lowPercent` prop으로 내려 막대·탭 막대가 쓴다.

## 범위·기본값을 화면에 내보내는 방법 (2026-11-05)

- `Settings.tuningSpec`(`get_settings` 응답과 `settings-updated` event에 같이 온다): 필드마다 `{ key, min, max, default, isInteger }`. 값은 저장 단위(초·바이트). TS 짝은 `ipc.ts`의 `TuningFieldSpec`·`TuningKey`. 새 command를 두지 않은 이유: 설정 화면은 이미 `settings`를 받으므로 따로 불러오고 기다릴 상태가 생기지 않는다. 파일에는 저장하지 않는다.
- `Tuning::fields()` 표 하나가 검증(`invalid_fields`)과 spec(`Tuning::spec()` = 기본값으로 만든 표)에 같이 쓰인다. 범위는 위의 `*_RANGE` 상수 그대로. 테스트가 spec의 키가 JSON 필드 전부와 같은지, `default`가 `Tuning::default()`와 같은지, `min`·`max`가 검증이 받는 양 끝인지 확인한다.
- TS에는 숫자 범위가 없다. 테스트 입력도 IPC fixture(`settings.json`의 `tuningSpec`)에서 읽는다.

## 설정 패널 (2026-11-05)

- 위치: 팝오버 "설정" 화면, 에이전트 토글 아래. 섹션: 한도 조회(간격 2개, 429 뒤 최대 간격), 캐시(환율·컨텍스트 TTL), 시간 초과(API, 키체인 — macOS에서만 씀, kiro-cli), 탐색 범위(세션 수들, rollout, Claude 기록 끝 읽기, 창 패턴 기간), 추정(토큰당 바이트), 화면(`lowPercent`), 고급(`<details>`, 기본 접힘, "바꾸면 '추정' 표기의 뜻이 달라집니다" 안내).
- 표시 단위(`tuning-form.tsx`의 `UNITS` 표 하나): 간격·TTL·백오프는 분, 시간 초과·`resetTolSecs`는 초, `claudeLogTailBytes`는 KiB. 저장할 때 × 60, × 1024로 원래 단위로 바꾼다. 정수 필드는 표시 단위로도 정수만 받는다(정수 분). 필드 옆에 범위·기본값을 같은 단위로 보인다.
- 바꾼 필드만 문자열로 들고 있다. "저장"은 바뀐 것이 있을 때만 켜지고 저장 중에는 꺼진다. 저장 전에 spec으로 범위·정수를 검사해 틀리면 저장하지 않고 그 필드에 오류를 보인다. `goodR2 ≥ rangeR2` 같은 짝 조건은 Rust만 본다. `invalid_input` 메시지에 나온 필드 이름을 해당 필드에 표시하고, 고급 필드면 고급을 연다.
- "기본값으로"는 입력란을 spec의 기본값으로 채울 뿐 저장은 "저장"으로 한다. 저장은 `setSettings(enabledAgents, tuning)` 하나로, 에이전트 토글과 같은 저장 중 상태를 쓴다(서로 덮어쓰지 않게).
- 팝오버 높이 상한(800) 안에서 필드 영역만 스크롤하고, 뒤로·저장 줄은 고정이다. 창 높이는 기존 `ResizeObserver` 흐름 그대로.

## 이번에 안 한 것

- (2026-11-05 해소) 설정 패널. 위 "설정 패널".
- 간격을 줄였을 때 이미 잡힌 대기 시간을 줄이는 것(`schedule.rs` `ponytail:` 주석).
- 설정이 바뀌면 캐시된 환율·컨텍스트를 즉시 다시 계산하는 것.
