# 0005. 세션 시작 컨텍스트 측정

- 상태: 채택
- 날짜: 2026-11-04
- 범위·규칙: [0003](0003-product-scope.md) "1. 측정", "숫자 표기 규칙". 원본: ctxmeter(비공개 선행 프로젝트) `scanner.js`, `scan-common.js`, `instructions.js`, `claude-prompt.js`, `claude-mcp.js`, `tool-loading.js`, `kiro-scan.js`, `mcp-cost.js`(설정 읽기만)

## 결정

| 결정                                                                                                                                                                         | 이유                                                             |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Rust 모듈 `src-tauri/src/context/`, command 하나 `get_context(workspace: Option<String>)` → `ContextSnapshot { agents: [AgentContext] }`. TS 짝은 `src/lib/ipc.ts` 같은 이름 | 2·3·5층이 이 항목별 토큰을 입력으로 쓴다                         |
| `workspace`는 절대 경로, `..` 없음, 존재하는 폴더만(`invalid_input`). `None`이면 전역 설정만. 이번 UI는 항상 `None`                                                          | 폴더 선택(dialog plugin + capability)은 3b에서 결정 기록과 함께  |
| 항목: `kind`(instructions·rules·skills·agents·hooks·mcp·plugins·system), `name`, `path?`, `bytes`, `tokens`, `source`, `loading`                                             | intent Outcome                                                   |
| `bytes`·`tokens`가 `null`이면 "정적으로 알 수 없음"(hook 출력, 실행하지 않은 MCP 서버 스키마). 0으로 쓰지 않고 `warnings`에 이유를 쓴다                                      | 0003: 모르는 값을 0으로 보여 주지 않는다                         |
| `totalStartupTokens` = `loading == startup`이고 토큰을 아는 항목의 합. 항목은 startup → conditional → deferred, 같은 시점 안에서 토큰 큰 순                                  | 팝오버는 상위 8개 + "나머지 n개"만 보여 준다(시각화는 4층)       |
| 스캔은 `spawn_blocking`. 프론트엔드는 에이전트 탭에 들어올 때와 창 `focus`(팝오버가 다시 보일 때) 한 번씩 부른다. 타이머 없음                                                | AGENTS.md 성능 원칙. 이 Mac 실제 홈에서 세 에이전트 전체 30~40ms |
| 파일 내용은 크기·front matter·@import 해석에만 읽고 저장·로그·IPC에 넣지 않는다. 경로·파일 이름·MCP 서버 이름만 돌려준다                                                     | intent 제약. 서버 이름은 설정이지 내용이 아니다(ctxmeter와 같다) |
| OS별로 다른 것은 에이전트 설정 폴더 루트뿐이라 `context/paths.rs`에 모은다(모두 홈 아래 `.claude`·`.claude.json`·`.codex`·`.kiro`, Windows는 `%USERPROFILE%`)                | Windows 실기 확인은 이번 범위 밖. 하위 구조는 OS와 같다          |

## 실측 / 추정

| 출처                                              | 무엇                                                                                                                                                                                                                         |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **실측**(`measured`) — Claude `api-request-shape` | 시스템 프롬프트(블록 text 바이트), 내장 도구 정의, MCP 서버별 도구 정의, 지연 로딩 자리표시 도구(`defer_loading: true`). 모두 실제로 보낸 요청의 바이트이고, 토큰은 바이트 ÷ 4(ctxmeter와 같다). 요청에 있었으므로 `startup` |
| **실측**(`measured`) — Codex rollout              | `session_meta.payload.base_instructions`(Codex 내장 시스템 지시문, 문자열 또는 `{text}`)의 바이트, 토큰은 ÷ 4. `startup`. 내용은 읽은 즉시 버리고 바이트만 남긴다                                                            |
| **추정**(`estimated`) — 설정 파일 바이트 ÷ 4      | 지시 파일(CLAUDE.md·@import·rules·AGENTS.md·steering), skill 목록(front matter 바이트), agent 정의, 플러그인 skill. Kiro는 전부 추정                                                                                         |
| 크기 모름(`null`)                                 | hook(개수만), shape가 없을 때의 Claude MCP, Codex·Kiro MCP(서버 이름 + 지연 여부만), Kiro power(개수만)                                                                                                                      |

- Claude: 최근 transcript 5개(`workspace`가 `None`이면 `~/.claude/projects/*/` 전체에서 mtime 순) 중 shape가 있는 가장 새 파일의 마지막 shape를 쓴다. 있으면 설정 기반 MCP 항목 대신 shape의 MCP 항목을 쓴다(설정 파일·skill 추정은 그대로 같이 보인다. shape의 system 블록에는 CLAUDE.md·skill 목록이 들어 있지 않다). 없으면 경고 + 설정 기반 MCP 항목.
- transcript는 한 줄씩 읽고 바이트 substring `"type":"api-request-shape"`가 없는 줄은 JSON으로 파싱하지 않는다. `api-request-blob` 줄(메시지 원문)은 여기서 걸러진다(원문 안의 같은 글자는 JSON 이스케이프돼 맞지 않는다).
- `None`일 때 고른 transcript는 그 프로젝트의 MCP 설정을 반영한다. 프로젝트마다 다른 MCP 차이는 3b(폴더 선택)에서 그 폴더 transcript로 본다.
  그래서 `None`이면 그 세션의 작업 폴더(transcript 일반 줄의 `cwd` 필드만 정규식으로 추출, blob 줄 제외)를 경고 목록에 "실측값은 가장 최근 Claude 세션(작업 폴더 X) 기준"으로 밝힌다(2026-11-04 사용자 결정).

### Codex 실측 (2026-11-04 사용자 결정)

- 출처: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`을 mtime 최신순으로 최대 50개 열어 첫 후보를 쓴다. `.jsonl.zst`는 건너뛴다. `workspace`가 `Some`이면 `session_meta.payload.cwd`가 그 폴더 안(경로 단위 비교)인 것만, `None`이면 가장 최근 것. 인증 파일은 읽지 않는다.
- 줄은 바이트 substring `"type":"session_meta"` / `"type":"token_count"`가 있는 것만 JSON으로 파싱한다. 메시지·도구 출력 줄은 파싱하지 않는다(안의 같은 글자는 이스케이프돼 맞지 않는다). `session_meta`가 맞지 않는 폴더면 그 파일은 더 읽지 않는다.
- 두 값:
  - 항목 "시스템 지시문"(System, `measured`, startup) = `base_instructions` 바이트(÷ 4 = 토큰). 합계(`totalStartupTokens`)에 들어간다.
  - `AgentContext.firstRequestTokens`(Rust `first_request_tokens: Option<u64>`, TS `number | null`) = 첫 `event_msg`/`token_count` 중 `info`가 있는 줄의 `info.last_token_usage.input_tokens`. **실제 토크나이저 수이고 첫 사용자 메시지가 들어 있다.** 시작 합계(바이트 ÷ 4 추정 + 시스템 지시문)와 같은 양이 아니라서 합치지 않고 UI에 "첫 요청 실측 N 토큰(첫 메시지 포함)"으로 따로 보여 준다. Claude·Kiro는 `null`.
- 겹침 확인: 추정 항목은 AGENTS.md 체인·skill 목록·agent 정의·플러그인 skill·hook·MCP 이름이다. `base_instructions`는 Codex가 모델에 싣는 내장 프롬프트(AGENTS.md 등은 사용자·developer 메시지로 따로 들어간다)라 겹치지 않아 추정 항목을 빼지 않았다. 이 Mac의 최신 rollout: `base_instructions` 21,534바이트(5,384토큰), `firstRequestTokens` 29,861 — python으로 직접 뽑은 값과 같다.
- `workspace`가 `None`이면 경고 목록에 "실측값은 가장 최근 Codex 세션(작업 폴더 X) 기준입니다."(Claude와 같은 문장 생성기 `global_session_note`).
- 새 crate 없음. 첫 줄에 `base_instructions`가 들어 있어 첫 줄이 수십 KB이지만 한 줄씩 읽으므로 메모리는 줄 하나 크기다.

### Kiro 실측: 시작 조립 문자 수 (2026-11-04 사용자 결정)

- 토큰 수는 없다. `~/.kiro/sessions/<hash>/sess_*/messages.jsonl`의 `session_metadata`/`contextUsage`는 사용률 %뿐이고 창 크기를 알 수 없어 토큰으로 바꾸지 않는다(조사: 로컬 조사 기록(비공개)). 대신 `session_start` 줄의 문자 수를 **실측(문자)**으로 보여 준다. 크기는 실측이고 토큰 환산만 추정이다.
- 출처: `messages.jsonl`을 mtime 최신순 최대 50개 열어 첫 후보를 쓴다. `workspace`가 `Some`이면 같은 폴더의 `session.json` `workspacePaths` 중 하나가 그 폴더 안(경로 단위 비교)인 것만. 줄은 바이트 substring `"session_start"`가 있고 `payload.type == "session_start"`인 첫 줄만 JSON으로 파싱한다. 내용(`content`, steering 본문)은 문자 수만 남기고 버린다.
- 포함 범위(키·숫자로 확인, 최신 8개 세션 모두 같다): `composition.contentChars`는 `payload.content` 문자 수와 같다. 작업 폴더 file tree가 이 content 안에 들어 있어(`fileTreeChars`는 그 일부) 따로 더하지 않는다. `steeringDocuments[].content`는 `content`의 부분 문자열이 아니라 따로 실린다. 그래서 `firstRequestChars` = `contentChars` + Σ steering 문서 문자 수. `composition`이 없으면 `content` 문자 수로 대신한다.
- 값: `AgentContext.firstRequestChars`(Rust `first_request_chars: Option<u64>`, TS `number | null`). Claude·Codex는 `null`. 시작 합계(`totalStartupTokens`)에는 넣지 않는다: 추정 항목(steering 파일 바이트 ÷ 4)과 같은 steering이 겹치고, 사용자 시스템 조립문은 그 항목들과 범위가 다르다.
- UI: "시작 조립 실측 N자(≈M 토큰, 추정)". M = round(N ÷ 4). 문자 ÷ 4는 바이트 ÷ 4와 같은 환율이며 실제 토크나이저 수가 아니다.
- `workspace`가 `None`이면 `global_session_note("Kiro", …)`로 "실측값은 가장 최근 Kiro 세션(작업 폴더 X) 기준입니다." 경고. X는 그 세션의 `workspacePaths[0]` 폴더 이름.
- 대조(이 Mac, 최신 세션): python 27079 + 21957 = 49036, kcal `firstRequestChars` 49036. 로컬 조사 기록(비공개).
- 한계: 이 문자 수는 시스템 조립문 + steering이다. 도구 정의·첫 사용자 메시지는 이 줄에 없어 실제 첫 요청 전체와 같지 않다.

## 로딩 시점 판정

| 항목                                                  | 판정                                                                                                                                                                                                                                                                                                    |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Claude rule                                           | front matter에 `paths`가 있으면 `conditional`(크기는 front matter를 뺀 바이트), 없으면 `startup`                                                                                                                                                                                                        |
| Kiro steering                                         | `inclusion` 없음·`always`·파일 이름 AGENTS.md → `startup`, `fileMatch` → `conditional`, 그 밖(`manual`·`auto`) → `deferred`                                                                                                                                                                             |
| agent 정의(Claude `.md`, Codex `.toml`, Kiro `.json`) | `conditional`(고를 때 들어감)                                                                                                                                                                                                                                                                           |
| hook                                                  | `conditional`(이벤트 때 출력이 들어감)                                                                                                                                                                                                                                                                  |
| MCP(설정 기준)                                        | Claude: `alwaysLoad`·`CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS`·`ENABLE_TOOL_SEARCH`가 `false`·`auto`·서드파티 `ANTHROPIC_BASE_URL` → `startup`, 그 밖은 `deferred`. Codex: 항상 `deferred`. Kiro: `chat.agentEngine`이 v1·v2가 아니면 `deferred`, v2는 `toolSearch.enabled` + 임계값 0일 때만 `deferred` |

- 서버를 실행하지 않아 스키마 총량을 모르므로, 총량이 필요한 판정(`ENABLE_TOOL_SEARCH=auto`, Kiro V2 임계값)은 ctxmeter가 총량을 모를 때처럼 `startup`으로 둔다.
- Claude env는 앱 프로세스 env 위에 `~/.claude/settings.json`(+ 작업 폴더 `.claude/settings(.local).json`)의 `env`를 겹친다. 메뉴바 앱은 셸 `export`를 물려받지 않으므로 셸에서만 켠 값은 반영되지 않는다.

## skill 위치 (2026-11-07)

Codex·Kiro skill 항목은 아래 위치를 센다. 위치 안에서만 찾고(새 전체 탐색 없음), 같은 이름은 에이전트마다 한 번만 센다. 이름은 front matter `name`, 없으면 SKILL.md가 든 폴더 이름이다. 먼저 나온 위치가 이기고, 진 위치의 항목에서는 빠진다. 깨진 링크는 건너뛰고, 같은 폴더를 가리키는 링크는 한 번만 본다. 모두 `startup`.

| 에이전트 | 위치(이기는 순서, 항목 id)                                                                                                                                                                                                                                     |
| -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Codex    | 작업 폴더 → git 루트까지 폴더마다 `.agents/skills`(`repo-skills`, git 루트는 AGENTS.md 체인과 같은 코드), `$HOME/.agents/skills`(`home-agent-skills`), `~/.codex/.agents/skills`(`agent-skills`), `~/.codex/skills`(`user-and-system-skills`, `.system/` 포함) |
| Kiro     | workspace `.kiro/skills`(`workspace-skills`), `~/.kiro/skills`(`user-skills`)                                                                                                                                                                                  |

- 근거(확인 2026-11-07, Codex 0.166.0): 작업 폴더 `sub/`에서 연 세션 기록의 스킬 목록에 `sub/.agents/skills`와 git 루트 `.agents/skills`의 스킬이 모두 있고, `$HOME/.agents/skills`에만 있는 스킬 4개와 `~/.codex/skills`(`.system/` 포함)의 스킬이 실렸다. 같은 이름(`shadcn`, `find-skills`, `orchestration`)은 두 폴더에 있어도 목록에 한 번이다. 깨진 링크는 실리지 않았다. 이 Mac에서 kcal이 센 이름은 이 목록(플러그인 2개 제외 14개)과 모두 같고, `review-agent` 하나만 kcal에만 있다(`config.toml`의 `[skills] max_context_tokens` 예산 때문일 수 있음, 추정).
- `~/.codex/.agents/skills`는 문서에 없다. 안의 스킬이 모두 다른 폴더에도 있어 Codex가 읽는지 실측으로 가를 수 없었고, 지우지 않고 계속 센다.
- `/etc/codex/skills`(관리자 위치)는 이 Mac에 없어 넣지 않았다. 필요해지면 위치 목록 끝에 추가한다.
- Kiro(확인 2026-11-07, kiro-cli 2.32.0 V3): workspace `.kiro/skills`의 스킬이 시작 때 실린다. 같은 이름이 workspace·global 둘 다 실리는지는 문서에 없어, 문서의 "workspace 우선"을 따라 한 번만 센다(추정).
- Kiro 하위 폴더 AGENTS.md(확인 2026-11-07, 같은 버전): `sub/`, `pkg/deep/er/`, `.agents/skills/x/`의 AGENTS.md는 시작 때 실리지 않았고 루트 AGENTS.md만 실렸다. 문서는 하위 폴더 AGENTS.md도 발견한다고 적지만 시작 목록에는 없으므로 코드는 그대로 루트와 steering 폴더만 읽는다.

## ctxmeter와 다르게 한 것

- Claude rule의 `paths` 판정, Kiro steering의 비-`always` 항목: ctxmeter는 개수(`onDemand`)만 세지만 kcal은 항목(`conditional`·`deferred`, 바이트·토큰)으로 보여 준다. 합계에는 넣지 않으므로 startup 합계는 같다.
- Kiro custom agent: ctxmeter는 개수만, kcal은 파일마다 항목(파일 바이트, `conditional`).
- 지시 파일 합계: ctxmeter는 바이트 합을 한 번에 ÷ 4, kcal은 항목마다 ÷ 4 반올림 후 합. 1토큰 안팎 차이가 날 수 있다.
- Codex command rule(`~/.codex/rules/*.rules`)은 넣지 않는다. execpolicy 규칙이라 프롬프트에 들어가지 않는다(ctxmeter도 토큰은 `null`).
- Kiro `crew/skills`는 ctxmeter처럼 세지 않는다(시작 목록에 들어가지 않음).

대조 결과(이 Mac, 2026-11-04): 로컬 조사 기록(비공개). 위 차이 말고는 항목·바이트·토큰이 모두 같았다.

## 옮기지 않은 것

- 세션 토큰 사용량 telemetry 중 첫 요청 토큰 외의 것(`*SessionTelemetry`의 누적·모델·compaction, `*Runtime`, model catalog), OpenCode 스캔, Kiro Crew 사용량 — intent 범위 밖.
- `mcp-cost.js`의 서버 실행(`mcp-scan`): 서버를 띄워 `tools/list`를 받는 경로. 0003 "서버 실행은 사용자 동의 후"라 별도 결정이 필요하다. kcal은 설정된 서버 이름과 지연 여부만 읽는다.
- Kiro 작업 폴더 `.kiro/settings/mcp.json`(ctxmeter도 전역만 본다), `CLAUDE_CONFIG_DIR`·`CODEX_HOME` 재지정 — 3b 또는 요청이 있을 때.
- skill 파일 해시(`sha256`)·수정 시각·`declaredName`, 제외된 skill 수(`countExcludedSkills`) — 화면에서 쓰지 않는다.

## 캐시 (2026-11-04)

- 전역 측정(`workspace == None`)의 마지막 결과를 `cache::Cache`(managed state)에 둔다. `get_context`는 즉시 반환하고, 10분(`CONTEXT_TTL`)이 지났으면 뒤에서 한 번만 다시 재서 `context-updated` event로 보낸다. 값이 없을 때 계산 중이면 같이 기다린다. 앱 시작 때 한 번 미리 잰다. 응답에 `computedAt`.
- 작업 폴더를 준 호출은 캐시하지 않고 바로 잰다(현재 UI는 `None`만 쓴다).
- 프론트엔드는 마지막 값을 메모리(`src/lib/snapshot-store.ts`)에 두고 탭에 들어올 때·팝오버가 다시 보일 때 command를 한 번 부른다(Rust가 캐시를 바로 준다). 갱신은 event로 받는다. 값 옆에 "N분 전 기준".

## 의존성

새 crate 없음. `regex`(문자열·`regex::bytes`로 transcript 줄 필터), `serde_json`, 표준 라이브러리(`std::env::home_dir`, `LazyLock`)만 쓴다. capabilities·CSP 변경 없음.

## 열린 것

- Windows에서 같은 홈 아래 경로가 맞는지, Claude transcript 폴더 이름 규칙(드라이브 문자·`\`)이 같은지 — Windows 기기에서 확인.
- shape의 토큰도 바이트 ÷ 4다. 실제 토크나이저 수와의 오차는 재지 않았다(2층 환율에서 다시 본다).
- Claude 플러그인이 제공하는 MCP 서버(`plugin_*`)는 shape가 있을 때만 보인다. 설정 기준 목록은 `~/.claude.json`·`.mcp.json`만 읽는다(ctxmeter와 같다).

## `/context` 묶음과 창 크기 (2026-11-11)

목표: README 헤드라인 문장 "Claude Code는 입력 전에 컨텍스트의 N%를 쓴다"를 첫 화면에서 읽을 수 있게 하고, Claude Code `/context`와 같은 묶음·분모로 맞춘다. 대조: 이 Mac, 작업 폴더는 이 저장소 폴더, Claude Code 2.1.322, `claude -p "/context" --no-session-persistence` 출력(`/tmp/hl-context-cli.txt`)과 사용자가 대화형 세션에서 본 값(skills 9.3k/52개, memory 6.3k/9개, tools 13.6k). 근거 표는 `/tmp/hl-evidence.md`.

### 왜 달랐나 (원인과 처리)

| `/context` 묶음         | 예전 kcal                        | 원인                                                                                                                 | 처리                                                                                                                                                                            |
| ----------------------- | -------------------------------- | -------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Memory files            | ≈2.5k, 파일 6개                  | 버그: `workspace == None`이라 프로젝트 CLAUDE.md·`@AGENTS.md`·자동 메모리(`projects/<p>/memory/MEMORY.md`)를 안 셌다 | transcript `attachment.type == "instructions"`의 `files[]`(경로·내용 바이트)를 실측으로 쓴다. 9개 모두 같은 경로                                                                |
| Skills                  | 5.3k, 29개                       | 버그: 사용자·플러그인 skill front matter만 셌다. 프로젝트 skill(7), 내장(13), claude.ai 동기화(13)가 빠졌다          | `skill_listing`(`skillCount`, `content`, `names`)을 실측. 켠 플러그인 접두사(`my-skills:`)는 `<id> (스킬 N개)` 항목으로 따로(코칭 0014가 이 이름으로 찾는다). 52개 = 44 + 6 + 2 |
| Custom agents           | 없음(agent 정의는 `conditional`) | 버그: 목록 줄(이름·설명)은 시작 때 실린다                                                                            | `agent_listing_delta`에서 `builtInTypes`·`builtInPluginTypes`가 아닌 줄을 "에이전트 목록 (N개)" 실측 항목으로. agent 본문 파일 항목은 `conditional` 그대로                      |
| MCP server instructions | 없음                             | 버그: 안 셌다                                                                                                        | `mcp_instructions_delta`(`addedNames[i]` ↔ `addedBlocks[i]`)를 서버마다 `McpInstructions` 실측 항목으로                                                                         |
| System tools            | 17.1k vs 13.6k                   | 버그 아님: 바이트(68,529 B)는 요청 그대로다. ÷ 4가 실제 토크나이저보다 많다(이 값에서 5.0 B/토큰)                    | 그대로(실측 바이트, 토큰 환산은 추정). 새 kind `Tools`로 시스템 프롬프트와 나눴다                                                                                               |
| (지연 로딩 도구)        | 시작 합계에 74 토큰              | 버그: `defer_loading: true` 도구는 찾을 때만 컨텍스트에 들어가고 `/context`도 "(deferred)"로 합계에서 뺀다           | `Loading::Deferred`                                                                                                                                                             |

- 첨부는 각 종류의 **첫 줄**(세션 시작)만 본다. 세션 중간 delta(늦게 붙은 MCP 서버 등)는 시작 비용이 아니다.
- 줄 고르기: 바이트 substring `"type":"<첨부 종류>"`가 있는 줄만 JSON으로 읽고 바깥 `type == "attachment"`·안쪽 `attachment.type`을 확인한다. 내용 문자열은 길이만 남기고 버린다(위 "파일 내용은 저장·로그·IPC에 넣지 않는다"와 같다). blob 표시는 `"type":"api-request-blob"`로 좁혔다(지시 파일 내용에 `api-request-blob` 글자가 있어도 첨부를 버리지 않게).
- 첨부가 없는 transcript(옛 버전)는 예전 설정 파일 추정을 그대로 쓴다.
- 남은 차이는 토큰 환산(바이트 ÷ 4)뿐이다. 실제 토크나이저 대비: 지시 파일 2.4 B/토큰(한국어·markdown), 스킬 3.1, 에이전트 3.1, 내장 도구 5.0. 바이트 ÷ 4는 한국어가 많은 지시 파일을 ~40% 적게, 도구 JSON을 ~25% 많게 센다. 새 의존성 없이는 고칠 수 없어 툴팁에 "토큰 = 바이트 ÷ 4"를 적는다(추정).

### 작업 폴더 맞춤

- `workspace`가 `Some`이면 그 프로젝트 폴더 transcript 중 `cwd`가 그 경로와 **같은** 세션의 shape를 쓴다(폴더 이름 규칙상 `/a-b`와 `/a/b`가 한 폴더에 섞일 수 있다).
- 없으면 가장 최근 세션(전체)으로 대신하고 경고 "이 작업 폴더(X)의 Claude 세션 기록이 없어 실측값은 가장 최근 Claude 세션(작업 폴더 Y) 기준입니다…". 이때 시작 첨부(지시 파일·스킬)는 다른 프로젝트 것이라 쓰지 않고 설정 파일 추정을 둔다. 시스템 프롬프트·도구만 그 세션 실측이다.
- `None`(지금 UI)은 예전처럼 가장 최근 세션이고, 첨부도 그 세션 것을 쓴다(경고가 출처를 밝힌다).

### 창 크기(분모)

- `AgentContext.model`·`contextWindow`(TS `string | null`·`number | null`). 모르면 `null`이고 화면은 %를 숨긴다.
- Claude 모델: 같은 transcript의 마지막 `api-request` 줄 `params.model`. 창: `tuning::CLAUDE_CONTEXT_WINDOWS`(출처 code.claude.com/docs/en/model-config "Extended context", 2026-11-11: Opus 4.7 이상·Sonnet 5 이상·Haiku 5.5·Fable 1M, Opus 4.6·Sonnet 4.6 200K, `[1m]` 1M). 표에 없는 모델은 `None`(Claude Code는 200K로 가정하지만 kcal은 지어내지 않는다).
- `/context`의 분모는 모델 창이 아니라 **자동 압축 창**이다(확인: 이 Mac `/context` "20k / 400k", `~/.claude/settings.json` `modelSettings.claude-opus-5-5.autoCompactWindow: 400000`, 모델 창은 문서상 1M). 그래서 같은 문서 "Set the auto-compact window"를 따른다: `CLAUDE_CODE_AUTO_COMPACT_WINDOW`(env, settings `env` 포함) > `modelSettings.<model>.autoCompactWindow` > `autoCompactWindow`(settings는 local > project > user), 100K~1M로 자르고 모델 창을 넘지 않는다. `CLAUDE_CODE_DISABLE_1M_CONTEXT=1`이면 1M 모델을 200K로. `--autocompact` 플래그는 실행마다 달라 읽을 수 없다(모름).
- 설정이 없을 때 `/context`가 1M을 쓰는지 967K(기본 압축 시점)를 쓰는지는 확인하지 못했다(모름). kcal은 모델 창(1M)을 쓴다.
- Codex: 첫 `token_count`의 `info.model_context_window`(기록값, 이 Mac `gpt-6.1-sol` 475,000)와 첫 `turn_context.payload.model`.
- Kiro: 창 기록이 없어 `None`(토큰만).

### 화면

- 개요 카드마다 "시작 컨텍스트 N% (X / Y 토큰)", 창을 모르면 "시작 컨텍스트 X 토큰". 툴팁: 정확/예상 · "토큰 = 바이트 ÷ 4" · "<모델> 컨텍스트 창 Y 토큰".
- 에이전트 탭 카드: 같은 값 + `/context` 순서의 묶음 줄(시스템 프롬프트·기본 도구·MCP 도구·MCP 서버 안내·에이전트·지시 파일·스킬, 예상이 섞이면 `~`, 툴팁에 `/context` 영어 이름). 5시간 한도 몫은 0009 문구 "요청마다 다시 실려서 5시간 한도의 ~N%"로 따로 둔다(예전 "~11.9K 토큰 (~2.7%)"는 창 몫으로 읽혔다).
- 0009 용어표의 "세션 시작 토큰"을 "시작 컨텍스트"로, 묶음 이름 7개를 새 용어로 쓴다. 0009 표 갱신은 이 작업 범위 밖이라 열린 것으로 남긴다.

## MCP 서버 스킬 (2026-11-28)

- Claude Code 2.1.332부터 MCP 서버가 스킬을 내준다(베타). `skill_listing`에서 `server:skill` 이름으로 오고, 접두사는 도구 이름 `mcp__<server>__`의 서버 이름과 같다(실측: `plugin_context7_context7:context7-mcp`, 줄 306바이트).
- 결정: 접두사가 그 세션 shape의 MCP 서버(바로 보낸 도구 또는 지연 로딩 도구의 서버)와 같으면, 그 스킬을 "스킬 목록"에서 빼서 `<server> (스킬 N개)` MCP 항목으로 센다. 그래서 MCP 끄기 제안의 비용에 그 서버 스킬도 들어간다(`switches::measured`가 `<name> (`로 시작하는 항목을 더한다). 플러그인 id가 먼저 이긴다.
- 안 한 것: 서버가 내용을 바꿔도 알려 주는 "버전 고정 안 됨" 표시. URL 서버는 고정할 방법이 없고 패키지 버전 판정은 추정이라, 백로그의 카탈로그 범위 결정과 함께 정한다.
- 한계: shape에 도구가 하나도 없는 서버(도구 없이 스킬만 내주는 서버)는 묶지 못하고 "스킬 목록"에 남는다.
