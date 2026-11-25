# 백로그

범위·근거는 [`decisions/0003-product-scope.md`](decisions/0003-product-scope.md).

조절 값 규칙: 주기·TTL·시간 초과·탐색 범위·판정 기준은 하드코딩하지 않고 `src-tauri/src/tuning.rs`(settings `tuning`, 기본값 + 범위 검증)에 넣는다(0007).

묶음은 "다음에 누가 움직이나"로 나눈다. 끝낸 항목은 지우고 git 기록(`git log -p -- docs/backlog.md`)에 남긴다.

## 진행 중

- [ ] **다음 주제: 진단하기 구멍 4·6 (핸드오프 2026-11-25).** 결정은 아래 "결정이 필요한 것 → 진단하기 개선"의 2026-11-25 줄. 4 = claude.ai 커넥터를 `claude mcp list` 출력으로 보고 끄는 방법 안내만(설정 안 씀, 근거 문서는 그 항목에 있음). 6 = 최근 세션 폴더 여러 개(개수는 tuning, 0007). 0017 E 틀에 붙인다. 그 다음 순서: Antigravity 읽기 조사(0027 ④) → 프로젝트 단위 켜기(Codex 플러그인 실제 `codex exec` 확인 허락됨, Kiro는 명령·JSON 안내만). 주제마다 새 세션.
  - 직전 주제 끝(0030 세션 현황·끄기, `a6b3577` main push): 실제 끄기 신호는 진짜 프로세스로 안 해 봄(단위 테스트만), Windows 미실행. Codex 샌드박스는 `claude`·`ps`·서버를 막으니 실제 데이터 읽기·화면은 샌드박스 밖에서 다시 확인한다(화면은 Edge `playwright-cli`, `PLAYWRIGHT_MCP_BROWSER=msedge`).
- [ ] **v0.1.4 게시됨(2026-11-21, 태그 `bc4d895`) — Windows 동료 확인 전에 게시(사용자 결정).** 남은 사람 확인: Windows 설치본 첫 실행(아래 "Windows 확인 남음"), 미서명 설치본에서 자동 시작 켠 뒤 재로그인(macOS·Windows). Codex `McpToolCall`이 옛 CLI 버전 rollout에도 있는지는 모름. main의 `55a08da`(Claude 환율 최근 창만)·`7d005e6`(모르는 에이전트 id 건너뛰기)는 다음 릴리스에 들어간다.

## 사람이 확인할 것 (실기·실계정)

- [ ] Keychain: "허용"만 누르면 다시 묻는지 모름. "항상 허용"은 `/usr/bin/security`에 영구 허용이라 README에 설명이 필요할 수 있음. 허용 창에 kcal이 아니라 `security`가 뜰 수 있음(추정). 허용 창 실기는 미확인(dev 빌드에서는 뜨지 않았고 release 서명 빌드는 미확인). 2026-11-20 동료 Mac v0.1.2 설치본에서 허용 창이 뜨지 않았다(사람 보고). Claude Code가 항목을 `security add-generic-password`로 만든다(claude 바이너리, 확인됨) → 같은 `/usr/bin/security`로 읽는 kcal은 묻지 않는 것으로 추정. 다른 앱이 만든 항목이면 뜰 수 있음(추정).
  - 외부 근거(사용자 요구 조사, 2026-11-19): 인접 메뉴바 앱에서 가장 많이 혼난 지점이다. CodexBar#340 "Always Allow인데 몇 시간마다 다시 묻는다"(👍36, 댓글 32). 실기 확인을 앞당긴다.
- [ ] 메뉴 막대가 꽉 차면 아이콘이 안 보인다(README에 안내). 노치에 가려지는지는 추정.
- [ ] Codex 빠른 모드 카드(`D-codex-fast-mode`, `311af5b`, 미릴리스 — v0.1.4에 들어감): 실제 앱에서 본 적 없음(이 Mac `service_tier = "default"`, 미리보기 합성 카드로만 확인). 추천은 Standard(사용자 결정 2026-11-21)라 일부러 빠른 모드를 쓰는 사람은 카드를 숨길 수 없고 알림 점이 남는다 — 불만이 나오면 "그대로 두기" 숨김을 붙인다. Ultrafast(`service_tier` 값이 문서에 없음)·프로젝트 `.codex/config.toml`·profile 층은 안 본다. 배수 표는 2026-11-21 pricing 문서 값이라 문서가 바뀌면 사실표와 함께 다시 본다.
- [ ] Claude Code `/usage`와 겹침 다시 긋기(사용자 요구 조사, 2026-11-19). CHANGELOG(2.1.327 기준)에서 `/usage`는 경고마다 출처를 밝힌 한 줄 고침을 내고 `f`로 적용하며, 같은 고침이 `/doctor`에도 나온다(확인됨). 0017·0022는 `/doctor` 기준으로 범위를 그었으므로 `/usage` 고침과 진단하기 카드가 겹치는지는 모름. 확인: 같은 날 `/usage`를 열어 진단하기 카드와 나란히 비교한다. 겹치는 것은 "이미 있는 점검은 안내만"(0017)으로 돌린다.
- [ ] CLAUDE.md 줄이기(0018 작업 A): Opus 없는 플랜의 오류 모양은 모름. Windows 실행 미확인.
- [ ] `paths` 없는 rules 옮기기(0018 작업 B): 실제 Claude 실행 1회 통과(2026-11-22, 5.3초, 모르는 키 0): 응답 방식 규칙 → 유지, `src/components` 규칙 → `paths`, 릴리스 절차 → 스킬 `cut-release`. 세 제안 모두 내용에 맞았다. 표본 1회라 다른 규칙 모양에서의 품질은 모름. 실제 실행 테스트는 한도 문을 거치지 않으니 남은 한도를 먼저 보고 돌린다: `cd src-tauri && cargo test --lib real_claude_proposes_where_rules_belong -- --ignored --nocapture`. 약점: 스킬로 옮기기 되돌리기 중 SKILL.md 지우기만 실패하면 기록 줄 없이 두 파일이 남고(뒤 되돌리기는 거절하고 수동 명령을 안내), 규칙이 심볼릭 링크면 링크가 끊긴다.
- [ ] 빈 환경 화면: 2026-11-17 빈 HOME 실제 창에서 기록 부족 문구, "estimate only", 빈 날 목표 숨김·단수/복수, Diagnose 제목 한 줄(폭 900), 영어 화면 한글 0, 빨간 글씨 0을 확인했다. Checks 탭 Claude Code 미설치 문구에 실행·설치 안내를 더했다. 2026-11-18 미리보기에서 팝오버 라이트(영어·한국어·빈 상태), 대시보드 한국어 라이트, 진단하기 제목 880·760·640 폭(두 언어·두 모드)도 확인했다: 겹침·잘림·언어 섞임·콘솔 오류 0. 라이트 회색 글씨 대비는 카드 위 4.93:1, 회색 바탕 위 4.52:1로 AA 통과(계산). 실제 창이 아니라 미리보기다. 한도를 못 읽은 에이전트는 링·숫자 대신 "—"와 "한도 못 읽음"을 보인다(2026-11-18, 테스트). `context/scan.rs` "2.1.300 이상 필요"는 이 Mac 기록에서 요청 모양 줄이 남은 가장 낮은 버전이 2.1.301이라 그대로 둔다(그보다 오래된 기록은 없어 하한은 추정).
- [ ] 실제 앱 눈 확인: 줄인 트레이 에이전트 탭의 실제 높이(미리보기 약 335px, 0023), 라이트 모드 대시보드의 선택 날 칸(회색 상자)이 무거운지.
- [ ] 추가 사용량(`extra_usage`)이 켜진 Claude 계정의 실제 응답 모양(0027 ③, 2026-11-19 구현은 CodexBar 소스 기준).

## 결정이 필요한 것

- [ ] **진단하기(E 끄기) 개선 — `/doctor`와 비교해 찾은 구멍.** 범위는 아직 안 정함.
  - 계기: Claude Code `/doctor`는 끌 것(MCP, 스킬)과 그 밖(스킬 SKILL.md YAML 오류, `defaultMode: auto` 제안, 지침 충돌)을 내는데 kcal 점검 탭 "최적화" 칸은 비어 있었다.
  - "최적화" 칸이 빈 것은 범위 때문이다: 0017 "첫 점검"에서 끄기는 `/doctor` 몫이고 최적화 칸은 A~D만 다룬다.
  - 진단하기 칸의 구멍(코드로 확인):
    1. (해결, 결정 0022) 사용 기록 근거: Claude 플러그인·MCP는 최근 30일 시작 폴더별 사용 횟수를 읽고, 쓴 것은 끄기를 제안하지 않는다. 기록이 부족하면 판단 못 한다고 말한다. 남은 것: Codex·Kiro(위 "끄기 제안 정책").
    2. (해결, 2026-11-17) 상위 폴더 `.mcp.json`: 작업 폴더와 그 위 폴더를 모두 읽고 가까운 폴더가 덮는다. 근거: 공식 MCP 문서(code.claude.com/docs/en/mcp)는 "프로젝트 루트"만 말하지만, Claude Code 2.1.326 `claude mcp list`가 `~/.mcp.json`(두 단계 위)의 서버를 project 서버로 보였다. 덮는 순서와 루트까지 올라가는지는 추정.
    3. (해결, 2026-11-17) 승인 판정: 공식 문서 "Project scope"대로 사용자 설정 `skipDangerousModePermissionPrompt: true`면 승인 기록이 없어도 실리는 쪽으로 센다(`disabledMcpjsonServers`는 계속 막음). 남은 한계: kcal은 세션 모드를 모르므로 이 설정이 켜진 Mac의 bypass 아닌 대화형 세션에서는 더 센다. `claude -p`·SDK 세션도 묻지 않고 싣는다(문서).
    4. **claude.ai 커넥터(Figma·Slack·Miro·Drive 등) 안 읽음.** 코드에 해당 경로가 없다. 공식 문서("Use MCP servers from claude.ai", "Disable claude.ai connectors"): claude.ai 구독 로그인일 때만 Claude Code가 직접 가져온다. 전부 끄기는 설정 `disableClaudeAiConnectors: true`(어느 범위든 true가 이김) 또는 환경 변수 `ENABLE_CLAUDEAI_MCP_SERVERS=false`, 하나씩은 `/mcp` 프로젝트별 끄기(`disabledMcpServers`에 `claude.ai Slack` 같은 표시 이름으로 적힘) 또는 `deniedMcpServers`. 데스크톱 앱 로컬·SSH 세션은 앱이 넣어 주므로 이 설정이 안 닿고 claude.ai 커넥터 설정에서 끊어야 한다. 커넥터 목록 자체는 로컬 파일에 없다(목록은 `claude mcp list` 출력으로만 봄, 추정). `/doctor` 기준 Figma 안내문은 약 0.9k.
    5. ~~**스킬 단위 끄기 없음.**~~ 완료(2026-11-19, 0022 "스킬 단위"): 개인·작업 폴더 프로젝트 스킬의 설명 숨기기(`skillOverrides` `user-invocable-only`) 행을 더했다. 플러그인 스킬은 대상 아님. 플러그인 통째로만 끈다. `/doctor`는 `~/.claude/settings.json` `skillOverrides`에 `"off"`로 스킬만 껐다(동작 여부는 `/doctor`도 추정이라 함. 새 세션 스킬 목록으로 확인).
    6. **작업 폴더 하나만 봄.** 다른 프로젝트의 local MCP 같은 것은 안 보인다(최근 세션 cwd 하나 기준, 0017 "알려진 한계"와 같은 원인).
  - 2026-11-19 사용자 결정: 5부터. 공식 문서(skills "Override skill visibility from settings", 2026-11-19 확인): 값은 `on`·`name-only`·`user-invocable-only`·`off`, **플러그인 스킬은 영향 없음** → 개인·프로젝트 스킬만 대상. 제안 값은 잃는 것 없는 `user-invocable-only`(`/` 메뉴로 계속 부름).
  - 추천 순서(초안): 4(커넥터, 사용자 결정 2026-11-17로 보류) → 5. 기준 일수는 tuning에 넣는다(0007). 틀은 0017 E에 붙인다.
  - 2026-11-25 사용자 결정: 4·6 둘 다 한다. 4는 `claude mcp list` 출력으로 커넥터를 보고 끄는 방법 안내만(설정은 안 씀). 6은 최근 세션 폴더 여러 개(개수는 tuning)를 본다.
- [ ] **에이전트 선택에 따라 바뀌는 화면 + 에이전트 N개 대비 구조.** 범위는 [0027](decisions/0027-agent-descriptor-and-picker.md).
  - 진행(2026-11-19): ① 설명표 완료 f3618d8·1b19f04, ② 대시보드 고르기 완료 5f1c7a7. ③ 창 `group`(+ 모델별 한도) 완료 e27b824. 남음: ④ Antigravity(별도 결정).
  - 요청: 지금 지원하는 에이전트 3개(Claude·Codex·Kiro) 중 무엇을 고르느냐에 따라 화면·대시보드가 그 에이전트에 맞게 바뀌어야 한다. 나중에 유명한 에이전트를 더 붙이므로 3개 고정이 아니라 N개를 전제로 디자인·데이터 화면을 짠다.
  - 지금 상태(코드로 확인):
    - 대시보드 사용량 탭은 고르는 칸이 없다. 켠 에이전트(`enabledAgents`)를 전부 한 페이지에 쌓는다: 링(`DailyActivity`, 동심원) → 처방 효과(`HealthCompare`) → 에이전트별 상세 카드(`AgentPanels`, `dashboard.tsx:36`). 팝오버만 에이전트 탭이 있다.
    - 목록 고정: TS `Agent` 유니언·`AGENTS`(`src/lib/ipc.ts:9`·`:12`), Rust `Agent::ALL: [Agent; 3]`(`src-tauri/src/usage/mod.rs:29`), 이름 `AGENT_LABEL`(`usage-popover.tsx:22`), 색 `--agent-claude/codex/kiro` 3개(`index.css`, `docs/design.md` "새 색은 결정 기록 없이 더하지 않는다").
    - 이름으로 갈라 그리는 곳: 단위·낱말 `agent === "kiro"`(크레딧·질문: `daily-activity.tsx:55`·`:260`, `agent-metrics.tsx:48`, `rate-line.tsx:239`, `health-compare.tsx:42`), Codex 전용 칸 `agent === "codex"`(`dashboard.tsx:66`, `agent-metrics.tsx:71`, `behavior-section.tsx:156`), 특정 에이전트 값을 따로 넘김(`kiroUsage` `dashboard.tsx:175`, `claudeUsage` `checks-tab.tsx:97`). 환율 IPC 타입도 에이전트마다 고정 필드(`RateSnapshot`의 `codex`·`kiro`·`claude`, `ipc.ts:236`).
  - 고려할 것(설계 전 초안):
    - **화면은 에이전트 이름이 아니라 "가진 데이터"로 칸을 고른다.** 에이전트마다 있는 것이 다르다: 한도 모양(Claude·Codex 5시간·주간 % 창 / Kiro 월 크레딧), 세는 단위(요청 / 질문), 환율·창 구성·한 턴 몫·점검 카드 유무. 이것을 에이전트 설명표 한 곳(이름·색·아이콘·한도 모양·낱말·있는 기능)에 적고, 칸은 "이 기능이 있으면 그린다"로 바꾼다. 목표: 새 에이전트 = 설명표 한 줄 + Rust 수집 모듈, 화면 파일은 안 건드림.
    - **공통 칸과 전용 칸을 나눈다.** 공통(모든 에이전트가 채울 수 있는 것): 남은 %·초기화 시각, 오늘 쓴 양 ÷ 하루 몫, 시간대 요청 수, 시작 컨텍스트 합계. 전용: Codex 창 구성·상위 세션, Kiro 모델별 한 턴 몫, Claude 점검 카드. 대시보드 "전체"는 공통 칸만, "에이전트별"은 공통 + 그 에이전트 전용 칸.
    - **선택 상태는 하나로 흐르게.** 팝오버 탭 → `openDashboard(tab, agent?)`로 그 에이전트 대시보드, 대시보드에서 고른 값은 `?agent=`/event로(지금 `?tab=` 방식과 같게). 점검 탭 알림 점의 "개요 = 전체, 에이전트 탭 = 그 에이전트"(0017 "알림 개수") 규칙을 대시보드 고르기에도 그대로 쓴다.
    - **데이터 모양도 N개로.** IPC는 에이전트마다 고정 필드(`RateSnapshot.codex/kiro/claude`) 대신 `agents: [{ agent, … }]` 목록 + 기능별 선택 필드로. Rust·TS 짝과 fixture를 같은 작업에서 바꾼다(AGENTS.md IPC 규칙).
    - **다음 에이전트 하나로 틀을 검증한다.** 설명표 칸은 상상한 N개가 아니라 실제 4번째 후보 하나를 넣어 보며 정한다. 그 전에 일반화하면 맞지 않는 칸을 만든다(YAGNI).
  - 걱정되는 것:
    - **"전체"에서 단위가 안 맞는다(확인됨, 코드).** Claude·Codex는 5시간 창 %, Kiro는 월 크레딧이라 지금도 링 단위가 갈린다(`daily-activity.tsx:55`). 새 에이전트는 또 다른 모양(요청 수 한도, 달러, 한도 없음)일 수 있다. "전체"에서 무엇을 나란히 보여 줄지(남은 % 하나로 맞출지, 한도 없는 에이전트는 어떻게 그릴지)를 먼저 정해야 한다.
    - **N개에서 화면이 깨진다(추정, 4개 넣은 미리보기로 확인).** 동심원 링은 4개를 넘으면 못 읽는다. 팝오버 탭 줄(폭 310)은 5개쯤에서 넘친다. 메뉴바 막대 아이콘(0012, `tray.rs` `bars`가 켠 에이전트마다 막대 하나)은 막대가 가늘어져 안 보인다. 브랜드색 3개(`index.css`)는 늘릴 때마다 라이트·다크 대비를 다시 봐야 하고 결정 기록이 필요하다(`docs/design.md`).
    - **유휴 CPU·조회 비용이 에이전트 수만큼 는다(확인됨, 코드).** 한도 조회는 에이전트마다 타이머가 있다(`usage/schedule.rs`, `Agent::ALL` 크기 배열). 새 설치 기본값이 "모두 켬"(`Settings::default` = `Agent::ALL`)이라 설치 안 한 에이전트도 조회한다. (정정 2026-11-21: 첫 실행은 이미 찾은 에이전트만 켠다, `settings.rs` `first_run_agents`. `Settings::default`는 첫 실행 경로가 아니다.)
    - **설정 파일 호환(확인됨, `settings.rs` `validate_agents`).** 알 수 없는 id가 하나라도 있으면 목록 전체를 버리고 "모두 표시"로 돌아간다. 새 에이전트를 넣은 버전에서 옛 버전으로 내리면 사용자가 꺼 둔 설정이 사라진다. 결정(2026-11-21): 모르는 id는 건너뛰고 나머지를 살린다(`settings.rs` `load`, 2026-11-21. IPC 저장은 계속 거절). 새로 설치된·안 본 에이전트를 켜는 기존 동작(`seen_agents`)은 유지.
    - **tuning이 에이전트별 필드로 늘어난다(확인됨).** `kiroIntervalSecs`·`codexSessionsToSearch`·`claudeLogFiles`·`kiroDailyCredits` 같은 평평한 필드라, 에이전트가 늘면 설정 패널이 길어진다. 에이전트별로 묶어 접을지 정한다(0007).
    - **공통 칸이 "같은 뜻"인지(모름).** "요청"·"시작 컨텍스트"를 에이전트마다 재는 방법이 다르다(실측 / 바이트 ÷ 4 추정, 0005). 같은 줄에 나란히 놓으면 비교되는 것처럼 보인다. 칸마다 실측·추정 표시를 어떻게 할지 정한다.
    - **바꾸는 범위가 넓다(확인됨).** 이름 분기·고정 필드가 프론트 10곳 넘게, Rust `Agent::ALL` 17곳에 있다. 한 번에 다 바꾸면 화면 회귀를 놓치기 쉽다. 순서 초안: ① 설명표 + 이름 분기 걷기(화면 그대로, 테스트로 확인) → ② 대시보드 고르기 → ③ N개 레이아웃(링·탭·아이콘) → ④ 4번째 에이전트.
  - 시작 전: 구조 변경이라 결정 기록(`docs/decisions/`)을 먼저 쓴다. UI는 `docs/design.md`를 따른다.
  - 2026-11-19 사용자 결정: ① 대시보드에서 하나 고르기(팝오버 탭 → 그 에이전트 대시보드) ② 4번째 후보 Antigravity(처음엔 Gemini CLI였으나 Google 공식 문서상 2026-06-18부터 Gemini CLI는 개인·AI Pro·Ultra 계정을 받지 않고 Antigravity로 옮겨 감, 2026-11-19 확인 developers.google.com/gemini-code-assist/docs/deprecations/code-assist-individuals). 기록 형식·한도 API는 모름 → 읽기 조사 먼저.
  - 정할 것: ① "선택"이 설정의 켠 에이전트인지, 대시보드에서 하나를 고르는 것인지(둘 다인지) ② 다음에 붙일 에이전트 후보(정해지면 설명표 칸을 그 에이전트 기준으로 검증) ③ 팝오버도 같은 방식으로 바꿀지.
- [ ] **프로젝트 단위로 켜기 — 상황을 보여주고 잊지 않게 돕기.** 초안, 범위 안 정함.
  - 2026-11-21 사용자 결정: v0.1.4 게시 다음 큰 기능. 순서 Kiro MCP → Codex 플러그인. 0022에 갱신 절로 붙인다.
    - Kiro MCP: 끄기 제안 + 프로젝트에서만 켜기(Codex MCP와 같은 기준). 근거: `messages.jsonl` `tool_call`의 `_meta.kiro.serverName`(확인됨, 이 Mac 1,074건), 프로젝트 = `session.json` `workspacePaths` 첫 값. 쓰기는 칸 하나만 덮기(프로젝트 `{"disabled": false}`, 사용자 `"disabled": true`). 사용자 항목 전체 복사는 `env` 비밀값이 프로젝트 파일로 가므로 하지 않는다. **선행 확인(사람):** 작업 공간에 `{disabled:false}`만 두고 Kiro `/tools`로 서버가 켜지나. 안 되면 명령·JSON 안내만.
      - 2026-11-22 확인(kiro-cli 3.5.0 `mcp status`, 사용자 파일은 안 건드리고 임시 작업 공간에 `{"playwright":{"disabled":true}}`만 둠): 작업 공간 항목이 사용자 항목을 **칸 단위가 아니라 통째로 대체**한다(Command `npx` → 빈 값). 따라서 `{disabled:false}`만 쓰면 command가 없는 서버가 된다 → "칸 하나만 덮기"는 안 된다(확인됨, CLI 설정 보기 기준. 대화 `/tools`도 같은 로더인지는 추정). 남은 길: 사용자 항목 전체 복사(`env` 비밀값이 프로젝트 파일로 감, 위에서 뺀 방식) 또는 명령·JSON 안내만. 2026-11-24 사용자 결정: **명령·JSON 안내만**(kcal은 Kiro 파일을 쓰지 않는다, `env` 비밀값이 프로젝트 파일로 가지 않게).
    - Codex 플러그인: 프로젝트에서만 켜기만, 끄기 제안 없음. 플러그인 `SKILL.md`를 읽은 `exec` 호출을 사용으로 센다(확인됨, 플러그인 MCP 호출 매칭은 모름). 프로젝트 `.codex/config.toml` `plugins.<id>.enabled`가 덮는지는 추정(확인 = 신뢰한 저장소에서 키 두고 스킬 목록). 2026-11-22: `codex debug prompt-input`(0.171.0)은 `-c plugins."…".enabled=`로 사용자 수준에서 바꿔도 스킬 목록이 그대로라 이 방법으로는 판정 못 한다. 실제 세션(`codex exec`에서 스킬 목록 묻기, 한도 씀)으로 확인해야 한다.
      - 2026-11-25 사용자 결정: 임시 신뢰 저장소에서 실제 `codex exec` 1~2회로 확인 허락. 덮으면 적기까지, 안 덮으면 명령 안내만.
  - 문제: 플러그인·스킬을 프로젝트마다 켤 수 있다는 것 자체를 모르거나, 알아도 매번 사용자 전체로 켜 두고 프로젝트별로 옮기는 걸 잊는다. 그래서 한 프로젝트에서만 쓰는 것이 모든 세션의 기본 컨텍스트에 실린다.
  - kcal이 할 일: 새 저장소를 만들지 않는다. 기억은 Claude Code 내장(`claude plugin enable --scope project|local` → `.claude/settings.json`·`settings.local.json` `enabledPlugins`)이 맡는다. kcal은 ① **상황 보기**: 플러그인·스킬마다 "켠 범위(사용자/프로젝트) × 실제로 쓴 프로젝트(최근 N일)" 한 장, ② **알려주기**: "사용자 전체로 켜져 있는데 A·B 프로젝트에서만 썼다" → "사용자에서 끄고 A·B에서만 켜기" 제안, ③ **대신 적기**: 0014 적용·되돌리기로 프로젝트 파일에 쓴다.
  - "원할 때 주입": 스킬은 `skillOverrides` `user-invocable-only`(부를 때만 본문, 0017 사실표 E), 플러그인은 세션 중 켜고 끄면 캐시가 깨지므로(0017 사실표 D) 세션 시작 전 프로젝트 파일로.
  - 겹치는 것: 진단하기 개선의 구멍 5(스킬 단위)·6(작업 폴더 하나만 봄)과 같은 재료(프로젝트별 사용 기록)를 쓴다. 구멍 1(사용 기록 근거)은 0022로 풀려 ①상황 보기와 ②·③ 중 Claude 플러그인 "프로젝트에서만 켜기"(1~3곳에서만, 3번 이상 쓴 것, `.claude/settings.local.json`에 쓰고 한 번에 되돌리기)는 구현됐다. 스킬 단위(0022)와 Codex MCP 서버 "프로젝트에서만 켜기"(2026-11-19, 0022 "Codex MCP", `.codex/config.toml`, 신뢰한 프로젝트만)도 구현됐다. Claude MCP는 명령 안내로 구현됐다(2026-11-19, 0022, user 설정은 쓰지 않음). 남음: Codex 플러그인, Kiro.
  - 기존 코드(확인됨): `enabledPlugins` 읽기 `context/scan.rs:179`, 고치기 `coach/edit.rs:206`(지금은 이미 적힌 플러그인만, 사용자 settings 기준).
  - 확인할 것: (추정) 프로젝트 `enabledPlugins: false`가 사용자 `true`를 덮는다. 한 프로젝트에서 `claude plugin disable <id> --scope local` 뒤 새 세션 `/context`로 확인. (추정) 세션 기록 `cwd`로 프로젝트별 사용 집계가 된다. `~/.claude/projects/*/*.jsonl`의 `cwd`로 확인. (확인됨 2026-11-19, 소스) Codex는 신뢰한 프로젝트의 `.codex/config.toml`이 사용자 설정 위에 키 단위로 겹친다(실제 세션 확인은 남음). (모름) Kiro의 프로젝트 단위 켜기. Codex `plugins._default.enabled`는 읽지 않고 있고, Kiro는 사실표 확인이 필요하다.
  - 시작 전: 결정 기록(0017 E에 붙일지 새로 쓸지)과 진단하기 개선과의 순서를 정한다.
  - 2026-11-19 사용자 결정: 0017 E에 붙이고, 진단하기 5 다음에 한다.

## 데이터가 쌓여야 하는 것

- [ ] Claude 환율: 2026-11-21 최근 `advanced.claudeRateDays`일(기본 7일, 3~90일) 창만 회귀하도록 변경(0006). 11-17 플랜 업그레이드 전후 혼합을 줄인다. 플랜·한도 변경 후 N일 동안 다시 모으며, 신뢰도가 안정되는지 재확인한다.
- [ ] Codex 환율이 측정 시점에 따라 크게 달랐다(창 40개에서 캐시 1%p ≈ 742K, 창 27개에서 ≈ 1.9M). 원인은 디스크의 rollout 집합 변화이고 어느 쪽이 실제에 가까운지는 모름. 2026-11-18: Codex가 열흘쯤 지난 rollout을 `.jsonl.zst`로 압축해 kcal이 건너뛰고 있었다. 이제 풀어 읽는다(0026, 창 17→103개). 값이 안정되는지 며칠 뒤 다시 잰다. 2026-11-22 재측정(`codex_parity_on_real_rollouts`, rollout 184개·창 109개): 검증 R² 0.873, 1%p ≈ 캐시 입력 1.41M(fold 1.14M~1.53M)·비캐시 입력 38K·출력 3.5K. 11-18 0026 직후 값은 기록이 없어 비교 못 함 → 다음 측정과 이 값을 비교한다. 창이 더 쌓이면 재확인하고, 3층에서 이 값을 쓸 때 오차를 표기한다.
- [ ] 3층: Claude %p(5시간 창 20개 이상 쌓인 뒤), 항목 단위 A/B 검증.

## 알려진 한계 (고칠 계획 없음, 바뀌면 다시 본다)

- 프론트 번들(2026-11-18, 확인됨): JS 한 파일 482KB(gzip 150KB), 런타임 의존성 7개, 차트 라이브러리 없음. 두 창이 같은 진입점이라 팝오버도 대시보드 코드를 읽는다. 대시보드를 `lazy`로 나누면 팝오버 파싱이 줄지만 이득은 몇 ms로 추정(측정 안 함)이고 대시보드 첫 열기에 빈 화면 위험이 있어 하지 않았다. 확인 = release 앱 시작에서 팝오버 웹뷰 로드 시간을 재고 나눈 뒤와 비교. 닫음(2026-11-21 결정): 이득 대비 첫 화면 빈 위험.

- 시작 컨텍스트의 바이트 ÷ 4 추정 오차(도구 +26%, 메모리 −39%). Codex는 kcal 작업 공간 %가 없다.
- 기록 맨 첫 주간 표본은 기준점이라 그 시간 사용은 안 잡힌다. Claude 주간 %p는 앱이 켜져 있을 때만 쌓인다.
- dev 앱에서 HMR 뒤 팝오버가 안 열리는 일이 있다. `hidden_by_blur_at` 미삭제는 원인이 아님을 테스트로 확인했고, 원인은 아직 모름.
- 미룬 것(0004 "열린 것"): 시계 틀어짐 보정. (Codex `.jsonl.zst`는 0026에서 읽는다.) (수동 새로고침은 0010 대시보드 "새로고침"으로 있고 429 backoff를 지킨다, `lib.rs` `refresh_all`.)
- 첫 점검의 한계: hook 점검은 최근 세션 폴더만 본다.
- Codex·Kiro 사실표(0017)와 코드의 차이 중 고치지 않은 것: (Kiro 세션 기록 위치는 2026-11-18 실측으로 맞음 확인, 0017.) Codex `project_root_markers`(한 줄 배열만)·`plugins._default.enabled`는 2026-11-18에 읽게 고쳤다. config에 section이 없는 설치된 Codex 플러그인은 아직 모른다(설치 목록 출처를 찾아야 함). 2026-11-22: 출처는 `codex plugin list`(설치·켜짐 상태를 보임, 원격 마켓플레이스까지 읽어 60초 넘게 걸림). 이 Mac에서 `chrome@openai-bundled`는 section이 없는데 "installed, enabled"이고 실제 프롬프트에 스킬이 실린다(확인됨) → kcal 시작 컨텍스트에서 빠진다. 같은 마켓플레이스의 `computer-use`·`unified-computer-use`는 section 없이 disabled라 로컬 파일(`marketplace.json` policy는 셋 다 같음)로는 켜짐 규칙을 못 가린다(모름). 고치려면 CLI를 부를지 정해야 한다. Codex·Kiro의 다음 점검 후보: 2026-11-21 사실표 갱신에서 각 3개씩 6개가 나왔고(두 `refresh.md`의 "점검 후보"), Codex 빠른 모드 1개(`D-codex-fast-mode`)를 만들었다. 2026-11-22에 남은 5개와 Codex Max·Ultra effort를 더한 6개 안내 카드의 판정·문구·테스트를 추가했다(0017 "첫 점검 (Codex) 2차"·"첫 점검 (Kiro)"). Kiro V3 검색·자동 압축 2개는 저장 설정에 후보가 있을 때만 CLI `--version`으로 하한을 확인하며, 버전을 모르면 숨긴다.
- Codex·Kiro 끄기 제안 정책(Codex MCP는 위 "진행 중"으로 풀림, 2026-11-21): 두 에이전트는 사용 기록을 읽는 출처가 없어(근거 없음) 플러그인·MCP 끄기를 제안하지 않고, 사용자가 직접 끌 수만 있다. Claude는 최근 30일 기록(세션 10개 이상)에서 한 번도 안 쓴 것만 제안한다. 같은 기준을 Codex·Kiro에 쓰려면 사용 기록 출처를 먼저 찾아야 한다(결정 0017·0022).
- Codex 플러그인 매니페스트(2026-11-18, `plugins/build/plugins` 문서): skill 폴더는 기본 `skills/`, 호환 매니페스트 `.codex-plugin/plugin.json`의 `skills` 경로가 있으면 그것을 따르게 고쳤다(이 Mac 설치본 20개는 모두 `./skills/`이거나 없음, 확인됨). 플러그인 MCP 서버도 시작 컨텍스트에 센다(`ba4b9e3`): 매니페스트 `mcpServers`가 있으면 그것(이 Mac 설치본은 `"./.mcp.json"`), 없으면 루트 `mcp.json`(문서 "Fixed package paths", 2026-11-19 확인). 매니페스트가 안 가리키는 `.mcp.json`을 Codex가 읽는지는 모름. 남은 것: 명령(prompts) 필드는 문서에 없다. 최적화 칸의 "고친 것" 줄 문구는 적용한 때의 언어로 남는다.
- 영어 UI: 예전 변경 기록은 저장된 언어 그대로 보인다. pace ahead/behind와 "Setup"은 0009 용어표 그대로 남겼다.
- 2차 점검(MEMORY.md 200줄·25KB 초과, 스킬 설명 1,536자·1% 예산)은 만들지 않는다(2026-11-21). 스킬 비용은 `/skill-doctor`가 맡고(0017 "범위": 이미 있는 점검은 안내만), MEMORY.md는 Claude Code가 쓸 때 검사하므로 사람이 손으로 고쳐 넘친 경우만 남는다. 그런 사용자 보고가 오면 다시 본다.
- 문서 주소는 글자로만 보인다. opener 권한을 늘리지 않기로 했다(사용자 결정 2026-11-21).
- 개인정보 표의 `agent-usage-audit` 줄과 코드 대체 경로(`usage/paths.rs` `claude_statusline_log`)는 유지한다(사용자 결정 2026-11-21: 개인 도구가 아직 기록 중이고, 파일이 없으면 아무 일도 안 한다).
- 보안 점검(2026-11-17)에서 남긴 낮은 위험(추정): `coach-log.jsonl` 위조 되돌리기(같은 계정 쓰기 권한이 전제), 핸드오프 프롬프트 인젝션(도구 꺼짐·스키마 검증·미리보기로 방어).
- 유휴 CPU·메모리: 빈 HOME 릴리스 빌드에서 5분 CPU 0.02초, 메모리 약 65MB(앱 24 + WebKit 40)를 확인함. 에이전트가 있는 경우(2026-11-18, 이 Mac 실제 HOME, debug 빌드 dev 앱을 `ps`로 10초마다 6분): 평소 10초당 0~0.09초로 사실상 0, 그 사이 두 번 몰림(약 5초, 약 23초, 약 3분 간격). 몰리는 원인은 추정(한도 갱신 뒤 지표·환율 다시 계산). release 측정(2026-11-17, 실제 HOME, 시작 20초 뒤부터 10초마다 6분, 앱 프로세스만): 6분 합계 7.3초(평균 2.1%). 처음 3분은 10초당 약 0.09초, 그 뒤 두 번 몰림(약 0.8초, 약 4.8초, 50초 간격), 마지막 2분은 10초당 0.01초 안팎. 2차(2026-11-17, 기호 포함 release, 2초마다 12분, 몰릴 때 `sample` 3초): 시작 12초에 약 18.5초, 그 뒤 몰림 세 번(+140~172초 약 7초, +333초 약 2.3초, +537초 약 5.9초), 그 사이는 2초당 0~0.02초. 몰림 때 스택(확인됨): +152초 health 계산·get_checks·`metrics::claude::read_requests`·`context::fsutil::walk_files`, +335초 지표 다시 계산(`metrics::kiro`·`rate::kiro::timed_credits`), +537초 `coach::checks::read_records`·`metrics::claude::model_requests`. 백엔드에는 지표·점검을 다시 부르는 타이머가 없다(코드 확인: 캐시는 호출이 올 때만 다시 계산, 한도 조회 5·10분은 트레이 아이콘만 바꿈). 그래서 +335·+537초에 누가 get_metrics·get_checks를 불렀는지가 모름 — 확인 = `pnpm tauri:dev:mcp`로 띄워 `ipc_monitor`로 숨은 팝오버 webview가 부르는 command와 시각을 본다(사용자 dev 앱을 바꿔야 해서 승인 필요). 한도 조회 시각(+300·+600초)과는 안 맞는다. 3차(2026-11-17, `tauri:dev:mcp` debug, 시작 약 26분 뒤부터 약 64분): 숨은 팝오버의 `invoke`를 감싸 기록했더니 화면 쪽 호출 0개(확인됨). 같은 동안 앱 CPU 합계 6.6초(평균 약 0.2%), 2초에 0.3초 넘는 몰림 0번. 몰림은 시작 뒤 약 12분까지만 있었다(17:08 한 번, 그때는 호출 기록 전이라 부른 쪽 모름). 시작 직후 몰림이 화면 호출인지 Rust 쪽 예열인지 가리려면 앱 시작 즉시 감싸야 한다(지금 방법은 창이 뜬 뒤라 못 함). WebKit 프로세스 CPU는 안 잼(모름). 기록 664MiB·Kiro 파일 6.2만 개에서 릴리스 빌드(데운 캐시) 단계 최대 2.6초, 다 합쳐 7.5초로 30초 기준 안이었다. 찬 캐시는 모름.
  - 외부 근거(사용자 요구 조사, 2026-11-19): 인접 앱이 기능을 늘리다 혼났다. CodexBar#1364 "becoming bloated"(👍37), #1274 지연(👍35). 기능을 더할 때마다 이 측정을 다시 본다.

## Windows 확인 남음 (Windows 동료 PC)

빌드는 GitHub CI(`windows-latest`)에 둔다. 공개 저장소에서는 self-hosted runner를 붙이지 않는다(fork PR이 자체 서버에서 실행될 수 있음).

- [ ] 릴리스 설치본: SmartScreen 경고 뒤 설치, 첫 실행에 트레이·첫 값이 뜨는가, 숨겨진 트레이 아이콘 고정 안내가 맞는가.
- [ ] Windows: 반영본 acrylic(다크·라이트), `rust:check` 전체 통과.
- [ ] 경로 대소문자: 세션 `cwd`와 작업 폴더 비교는 Windows에서 대소문자를 가리지 않게 고쳤다(`fsutil::is_inside`, 2026-11-18, macOS 단위 테스트만). Claude 프로젝트 폴더 이름은 입력 경로 글자 그대로 만들지만 NTFS가 대소문자를 가리지 않아 찾을 수 있다(추정). 확인 = Windows에서 작업 폴더를 소문자 드라이브로 넣고 시작 컨텍스트·세션 수가 나오는지 본다.
- [ ] 32px 메뉴바 막대 화면 확인.
- [ ] 배율이 다른 다중 모니터에서 팝오버 위치.
- [ ] 코칭 경로·권한, 6층 검진.
- [ ] 실제 rollout `payload.cwd` 형태(`\\?\C:\…`인지). `fsutil::is_inside`가 앞머리를 떼는 것은 단위 테스트만(`bad67dc`).
- [x] kiro-cli·전역 CLAUDE.md가 있는 Windows PC에서 테스트 다시 돌리기(테스트 격리 `bad67dc` 뒤). 2026-11-25 `b58ae23` Rust 452 통과·0 실패(`windows-log.md`).
- [ ] 개발 도구: `tauri.mcp.conf.json` `mcp-dev`가 `popover` 창만 허용해 대시보드 창 MCP 스크린샷·JS가 시간 초과된다(`dashboard` 추가).
- [ ] Windows 개발 PC에서 `pnpm check`가 `installer:test`(`sh install.test.sh`)에서 멈춘다: cmd PATH에 `sh` 없음. CI는 통과.
- [ ] Windows 진단하기 적용·Undo 실기(미리보기는 2026-11-25 확인). `get_checks` 경로·id의 `\\?\` 앞머리가 화면에 보이는지도 함께 본다.

## 나중에

- 6층 정기 검진 2차: 주 1회 검진·알림(notification plugin·capability 결정 기록 필요). 1차(대시보드 처방 효과)는 0016.
- 3b 폴더 선택 측정: 사용자가 폴더를 고르면 그 폴더 기준으로 측정(dialog plugin + capability → 결정 기록).
- `CLAUDE_CONFIG_DIR`·`CODEX_HOME` 재지정 읽기(사용자 요구 조사, 2026-11-19). 0005 "옮기지 않은 것"에서 "요청이 있을 때"로 미룬 것이다. 외부 요청 근거: CodexBar#81 여러 계정(👍28, 댓글 32), openai/codex#4432(👍134, open). 인접 도구 8곳 넘게 같은 요청이 반복된다(조사 보고, 개별 대조는 안 함). kcal 사용자 요청은 아직 없다. 하게 되면 0005를 고치는 결정 기록이 필요하다. README에 "지원 안 함"을 적었다(2026-11-21). 하게 되면 Finder에서 연 앱은 셸 환경 변수를 못 물려받으므로 설정 칸에 경로를 넣는 방식(추정, 확인 = release 앱에서 `std::env::var` 값 로그).
- 대시보드 달력·목표 달성 일수(0015 다음).
- (행 %의 분모는 `f937dcc`에서 툴팁·단위 줄로 드러냄) 시작 컨텍스트 누적 막대, 데모 데이터 스위치(에이전트 아이콘은 회사 마크로 해결, 0025). 메뉴바 링 + 숫자 아이콘은 0012와 충돌해 버림(2026-11-21).
- **카탈로그 — 잊지 않게 보관, 필요할 때 꺼내 쓰고 다 쓰면 치우기.** 초안, 범위 안 정함.
  - 문제: 한 달·분기에 한 번 쓰는 스킬·플러그인(예: k8s 업그레이드)은 전역에 두면 매 세션 컨텍스트를 먹고, 지우면 있었다는 걸 잊는다. 프로젝트에 고정해 두기도 아깝다.
  - 흐름: 보관(목록에만, 컨텍스트 0) → 에이전트에게 "카탈로그에서 k8s upgrade 찾아서 설치" → 쓰고 나서 "다 썼다 지워" → 다시 카탈로그로. 원본은 카탈로그에 남는다.
  - 틀(0017): "끄기"가 아니라 **잃는 것 없는 옮기기**다. "이걸 왜 꺼? 나 쓰는 건데"에 대한 답이 된다. 진단하기의 끄기 제안 옆에 "카탈로그로 보내기"를 둘 수 있다.
  - 새로 만들지 않을 것(추정): Claude 플러그인은 로컬 마켓플레이스(`claude plugin marketplace add <폴더>`)가 카탈로그 역할을 하고, 꺼내기·치우기는 `claude plugin install|uninstall --scope project|local`. kcal은 ① 카탈로그 목록 보기 ② 보내기·꺼내기를 대신 적기(0014 적용·되돌리기) ③ 꺼낸 뒤 N일 안 쓰면 "치울까요" 알려주기(조절 값, 0007)만 한다.
  - 확인할 것: (추정) 설치 안 한 마켓플레이스 플러그인은 시작 컨텍스트 0 — 로컬 마켓플레이스 추가 후 새 세션 `/context`로 확인. (모름) 에이전트가 카탈로그 존재를 어떻게 아나 — 전역 한 줄 안내(규칙·스킬 설명) 비용 vs 사용자가 매번 "카탈로그" 언급. (모름) 플러그인이 아닌 낱개 스킬 폴더, Codex·Kiro의 같은 방식.
  - 영감: [xingkongliang/skills-manager](https://github.com/xingkongliang/skills-manager)(사용자가 알려 줌, 2026-11-18). 시작 전에 이 저장소가 무엇을 하고 kcal과 어디서 겹치는지 먼저 읽는다.
  - 겹치는 것: 아래 "프로젝트 단위로 켜기"(계속 켜 둠)와 짝. 카탈로그는 "잠깐 켰다 치움"이다. 시작 전 한 결정 기록으로 묶을지 정한다.
- Claude Code `/insights` 심층 분석 → kcal "사용자 패턴 평가" 기능 검토. `/doctor`(설치·설정 점검)와 다른 기능이다. `/insights`는 최근 세션 로그로 사용 방식·마찰·CLAUDE.md 개선안을 HTML 리포트로 낸다.
  - 할 일: `/insights`를 돌려 리포트 항목을 정리하고, kcal이 하려는 사람 쪽 진단(무검토 승인, 확증편향·유도형 질문, 근거 요구 빈도)과 겹치는 것·빠진 것을 표로 비교한다.
  - 정할 것: kcal에 평가 기능을 넣을지. 넣는다면 "이미 있는 점검은 다시 만들지 않고 안내만" 원칙에 따라 `/insights`가 하는 부분은 안내로 두고 빠진 부분만 만든다. 0017 틀(유형·카드)에 맞는 칸이 있는지 먼저 본다.
  - 참고: 다중 에이전트판 [awesome-skills/insights](https://github.com/awesome-skills/insights).

- Linux 지원(요청 1명). OS별 경로·인증 읽기는 Rust 한 모듈에 모아 둔다.
- Claude Code statusLine 출력 모드(같은 실행 파일 인자) — 필요해지면 결정.
- 저장소 소셜 프리뷰 이미지(1280×640) 업로드와 GitHub topics 설정(macos, menu-bar, claude-code, codex, kiro, tauri). 저장소 설정에서 관리자가 한다.

## 조사 기록 (2026-11-21, 읽기만)

- Kiro 프로젝트별 켜기: MCP는 된다. 작업 공간 `.kiro/settings/mcp.json`이 사용자 `~/.kiro/settings/mcp.json`과 합쳐지고, 같은 이름이면 작업 공간이 이긴다. agent 설정 `mcpServers`가 그 위다. 서버마다 `disabled`가 있다(확인됨: kiro.dev/docs/mcp/configuration). 전역 서버를 작업 공간에서 같은 이름 + `disabled: true`로 끌 수 있다는 것은 추정(확인: 그렇게 두고 `/tools`). 스킬·powers의 작업 공간 단위 끄기는 문서에 없다. Kiro 세션 기록에서 MCP 사용을 셀 수 있는지는 모름 → "프로젝트에서만 켜기" Kiro는 이 확인 뒤.
- Antigravity(0027 ④): 로컬 기록은 `~/.gemini/antigravity/conversations/*.pb`(공개 안 된 protobuf)이고, 남은 쿼터를 주는 API·로컬 파일은 공식 문서에서 못 찾았다(쿼터는 작업량 기준 5시간 갱신, Settings 화면에서만, antigravity.google/docs/plans). 한도 표시 근거가 없어 4번째 에이전트로 붙이기는 이르다(추정). 붙인다면 세션 수만 보이는 "기록만" 지원이 한계다.
