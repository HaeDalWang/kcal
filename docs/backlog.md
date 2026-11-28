# 백로그

범위·근거는 [`decisions/0003-product-scope.md`](decisions/0003-product-scope.md).

조절 값 규칙: 주기·TTL·시간 초과·탐색 범위·판정 기준은 하드코딩하지 않고 `src-tauri/src/tuning.rs`(settings `tuning`, 기본값 + 범위 검증)에 넣는다(0007).

묶음은 "다음에 누가 움직이나"로 나눈다. 끝낸 항목은 지우고 git 기록(`git log -p -- docs/backlog.md`)에 남긴다.

## 진행 중

- [ ] 미릴리스(v0.1.5 뒤 main): 한도 문구 "주간"·"N일 N시간 후"(`a7dc47d`), 메뉴바 아이콘 모양 고르기(`3e8ea82`), Codex 플러그인 프로젝트에서만 켜기(`f93737f`·`d4bc061`), Codex 환율 주간 창 섞임 고침(`e11058a`), Kiro MCP 사용 횟수·끄기 제안·프로젝트 이동 안내(`bd1d17c`), Claude MCP 안내 JSON을 stdin으로(세션 밖 터미널 실행 흐름 미확인). 릴리스는 사람이 요청할 때.

## 사람이 확인할 것 (실기·실계정)

- [ ] v0.1.5(2026-11-25 게시) 남은 확인(v0.1.4부터 이어짐): Windows 설치본 첫 실행(아래 "Windows 확인 남음"), 미서명 설치본에서 자동 시작 켠 뒤 재로그인(macOS·Windows). Codex `McpToolCall`이 옛 CLI 버전 rollout에도 있는지는 모름.
- [ ] 메뉴바 아이콘 모양(0012 갱신 절): 숫자 모드는 macOS 실제 메뉴바에서 불꽃 + "2%" 확인됨(2026-11-27). 남음: 모드를 바꿀 때 숫자 title이 지워지는지(실제 앱, 지금은 단위 테스트만). Windows는 아래 "Windows 확인 남음".

- [ ] Keychain: "허용"만 누르면 다시 묻는지 모름. "항상 허용"은 `/usr/bin/security`에 영구 허용이라 README에 설명이 필요할 수 있음. 허용 창에 kcal이 아니라 `security`가 뜰 수 있음(추정). 허용 창 실기는 미확인(dev 빌드에서는 뜨지 않았고 release 서명 빌드는 미확인). 2026-11-20 동료 Mac v0.1.2 설치본에서 허용 창이 뜨지 않았다(사람 보고). Claude Code가 항목을 `security add-generic-password`로 만든다(claude 바이너리, 확인됨) → 같은 `/usr/bin/security`로 읽는 kcal은 묻지 않는 것으로 추정. 다른 앱이 만든 항목이면 뜰 수 있음(추정).
  - 외부 근거(사용자 요구 조사, 2026-11-19): 인접 메뉴바 앱에서 가장 많이 혼난 지점이다. CodexBar#340 "Always Allow인데 몇 시간마다 다시 묻는다"(👍36, 댓글 32). 실기 확인을 앞당긴다.
- [ ] 메뉴 막대가 꽉 차면 아이콘이 안 보인다(README에 안내). 노치에 가려지는지는 추정.
- [ ] Codex 빠른 모드 카드(`D-codex-fast-mode`, `311af5b`, 미릴리스 — v0.1.4에 들어감): 실제 앱에서 본 적 없음(이 Mac `service_tier = "default"`, 미리보기 합성 카드로만 확인). 추천은 Standard(사용자 결정 2026-11-21)라 일부러 빠른 모드를 쓰는 사람은 카드를 숨길 수 없고 알림 점이 남는다 — 불만이 나오면 "그대로 두기" 숨김을 붙인다. Ultrafast(`service_tier` 값이 문서에 없음)·프로젝트 `.codex/config.toml`·profile 층은 안 본다. 배수 표는 2026-11-21 pricing 문서 값이라 문서가 바뀌면 사실표와 함께 다시 본다.
- [ ] Claude Code `/usage`와 겹침 다시 긋기(사용자 요구 조사, 2026-11-19). CHANGELOG(2.1.327 기준)에서 `/usage`는 경고마다 출처를 밝힌 한 줄 고침을 내고 `f`로 적용하며, 같은 고침이 `/doctor`에도 나온다(확인됨). 0017·0022는 `/doctor` 기준으로 범위를 그었으므로 `/usage` 고침과 진단하기 카드가 겹치는지는 모름. 확인: 같은 날 `/usage`를 열어 진단하기 카드와 나란히 비교한다. 겹치는 것은 "이미 있는 점검은 안내만"(0017)으로 돌린다.
- [ ] CLAUDE.md 줄이기(0018 작업 A): Opus 없는 플랜의 오류 모양은 모름. Windows 실행 미확인.
- [ ] `paths` 없는 rules 옮기기(0018 작업 B): 실제 Claude 실행 1회 통과(2026-11-22, 5.3초, 모르는 키 0): 응답 방식 규칙 → 유지, `src/components` 규칙 → `paths`, 릴리스 절차 → 스킬 `cut-release`. 세 제안 모두 내용에 맞았다. 표본 1회라 다른 규칙 모양에서의 품질은 모름. 실제 실행 테스트는 한도 문을 거치지 않으니 남은 한도를 먼저 보고 돌린다: `cd src-tauri && cargo test --lib real_claude_proposes_where_rules_belong -- --ignored --nocapture`. 약점: 스킬로 옮기기 되돌리기 중 SKILL.md 지우기만 실패하면 기록 줄 없이 두 파일이 남고(뒤 되돌리기는 거절하고 수동 명령을 안내), 규칙이 심볼릭 링크면 링크가 끊긴다.
- [ ] 빈 환경 화면을 **실제 창**에서 보기(2026-11-18까지는 미리보기로만 팝오버 라이트·대시보드 한국어·진단하기 880/760/640 폭 확인: 겹침·잘림·언어 섞임·콘솔 오류 0, 회색 글씨 대비 AA). 실제 창으로 본 것은 2026-11-17 빈 HOME 한 번뿐이다.
- [ ] 실제 앱 눈 확인: 줄인 트레이 에이전트 탭의 실제 높이(미리보기 약 335px, 0023), 라이트 모드 대시보드의 선택 날 칸(회색 상자)이 무거운지.
- [ ] Kiro MCP 안내(`bd1d17c`)를 실제 앱에서 보기: 진단하기 Kiro 탭의 사용 횟수 줄·JSON 복사·되돌리기 안내. 이 Mac은 최근 30일 Kiro 세션이 7,327개(대부분 kiro-run 작업자)라 `diagnoseUsageFiles` 기본 1,000개로는 약 일주일만 읽혀 `partial` → 제안이 안 뜬다(확인됨, 2026-11-28 실측). 보려면 설정에서 이 값을 올린다. 붙여 넣은 JSON으로 Kiro `/tools`에 서버가 실제로 뜨는지도 본다(추정: 작업 공간 항목이 통째로 대체하므로 뜬다).
- [ ] 추가 사용량(`extra_usage`)이 켜진 Claude 계정의 실제 응답 모양(0027 ③, 2026-11-19 구현은 CodexBar 소스 기준).

## 결정이 필요한 것

- **Claude Code 2.1.307~332 신기능·11/27 vault 자료에서 나온 후보 중 범위 결정이 필요한 것** (2026-11-27 정리, 0003 범위와 맞는지 먼저 정한다)
  - 보안 설정 안내 묶음: `permissions.blockWritesOutsideWorkingDirectories`(2.1.314), `permissions.denyInsteadOfPrompting`(2.1.319), `PreSkillShellCommand` hook(2.1.332). 사용량 진단 범위 밖일 수 있다.
  - MCP 서버가 내주는 스킬(`server:skill`, 2.1.332)을 카탈로그 대상에 넣을지. 서버가 내용을 바꿔도 사용자는 모른다(context7은 서버 URL에 버전 없음, shadcn `@latest`, rea는 6.7.0 고정 — 2026-11-27 확인).
  - vault "사용자 미검토" 누적 개수·최고령 표시(vault lint 규칙: 10건 초과 또는 2주). kcal이 vault를 읽게 되므로 범위 결정.
  - "규칙은 늘 넣고 사실은 포인터로" 진단(세컨드브레인 자료, vault에서 보류). "MEMORY 2차 점검은 만들지 않는다" 결정과 충돌하므로, 하려면 그 결정을 다시 연다.

## 데이터가 쌓여야 하는 것

- [ ] Claude 환율: 2026-11-21 최근 `advanced.claudeRateDays`일(기본 7일, 3~90일) 창만 회귀하도록 변경(0006). 11-17 플랜 업그레이드 전후 혼합을 줄인다. 플랜·한도 변경 후 N일 동안 다시 모으며, 신뢰도가 안정되는지 재확인한다.
- [ ] 2026-11-28 재측정(실제 기록, `cargo test --lib -- --ignored --nocapture codex_parity_on_real_rollouts claude_parity_on_real_logs`, exit 0):
  - Claude: 창 89개 중 21개 사용, 검증 R² 0.714 → Estimated, 1%p ≈ 비캐시 입력 990K·출력 14.4K.
  - Codex: 처음엔 창 149개·R² −0.44(Unreliable). 원인 확인됨: 8월 옛 rollout은 주간 한도가 `primary`(`window_minutes` 10080)에 있어 5시간 회귀에 섞였다. 창 길이로 고르게 고침(`fix/codex-rate-unreliable` `e11058a`) → 창 143개·R² 0.636(RangeOnly, 1%p 단일값은 아직 안 냄). 월별 R² 9월 −0.33 / 10월 0.73 / 11월 0.97이라 남은 오차는 9월 데이터 쪽(원인 모름, 플랜·모델 변화 추정). 최근 N일 창만 쓰는 방식(Claude `claudeRateDays`처럼)은 제안만 해 둠. 다시 잴 때 비교 기준은 이 값.
- [ ] 3층: Claude %p(5시간 창 20개 이상 쌓인 뒤), 항목 단위 A/B 검증.

## 알려진 한계 (고칠 계획 없음, 바뀌면 다시 본다)

- 프론트 번들(2026-11-18, 확인됨): JS 한 파일 482KB(gzip 150KB), 런타임 의존성 7개, 차트 라이브러리 없음. 두 창이 같은 진입점이라 팝오버도 대시보드 코드를 읽는다. 대시보드를 `lazy`로 나누면 팝오버 파싱이 줄지만 이득은 몇 ms로 추정(측정 안 함)이고 대시보드 첫 열기에 빈 화면 위험이 있어 하지 않았다. 확인 = release 앱 시작에서 팝오버 웹뷰 로드 시간을 재고 나눈 뒤와 비교. 닫음(2026-11-21 결정): 이득 대비 첫 화면 빈 위험.

- 시작 컨텍스트의 바이트 ÷ 4 추정 오차(도구 +26%, 메모리 −39%). Codex는 kcal 작업 공간 %가 없다.
- 기록 맨 첫 주간 표본은 기준점이라 그 시간 사용은 안 잡힌다. Claude 주간 %p는 앱이 켜져 있을 때만 쌓인다.
- dev 앱에서 HMR 뒤 팝오버가 안 열리는 일이 있다. `hidden_by_blur_at` 미삭제는 원인이 아님을 테스트로 확인했고, 원인은 아직 모름.
- 미룬 것(0004 "열린 것"): 시계 틀어짐 보정. (Codex `.jsonl.zst`는 0026에서 읽는다.) (수동 새로고침은 0010 대시보드 "새로고침"으로 있고 429 backoff를 지킨다, `lib.rs` `refresh_all`.)
- 첫 점검의 한계: hook 점검은 최근 세션 폴더만 본다.
- 진단하기 MCP 판정의 추정(2026-11-17): 상위 폴더 `.mcp.json`은 가까운 폴더가 덮는 쪽으로 읽지만 덮는 순서와 루트까지 올라가는지는 추정(문서는 "프로젝트 루트"만, 2.1.326 `claude mcp list`가 두 단계 위 `~/.mcp.json`을 project 서버로 보임). `skipDangerousModePermissionPrompt: true`면 승인 기록 없이도 실리는 쪽으로 세므로, 그 설정이 켜진 Mac의 bypass 아닌 대화형 세션에서는 더 센다(kcal은 세션 모드를 모름).
- Codex·Kiro 사실표(0017)와 코드의 차이 중 고치지 않은 것: (Kiro 세션 기록 위치는 2026-11-18 실측으로 맞음 확인, 0017.) Codex `project_root_markers`(한 줄 배열만)·`plugins._default.enabled`는 2026-11-18에 읽게 고쳤다. config에 section이 없는 설치된 Codex 플러그인은 아직 모른다(설치 목록 출처를 찾아야 함). 2026-11-22: 출처는 `codex plugin list`(설치·켜짐 상태를 보임, 원격 마켓플레이스까지 읽어 60초 넘게 걸림). 이 Mac에서 `chrome@openai-bundled`는 section이 없는데 "installed, enabled"이고 실제 프롬프트에 스킬이 실린다(확인됨) → kcal 시작 컨텍스트에서 빠진다. 같은 마켓플레이스의 `computer-use`·`unified-computer-use`는 section 없이 disabled라 로컬 파일(`marketplace.json` policy는 셋 다 같음)로는 켜짐 규칙을 못 가린다(모름). 고치려면 CLI를 불러야 하는데 2026-11-27 사용자 결정으로 그대로 둔다(느린 CLI·네트워크를 앱에 넣지 않음). Codex·Kiro의 다음 점검 후보: 2026-11-21 사실표 갱신에서 각 3개씩 6개가 나왔고(두 `refresh.md`의 "점검 후보"), Codex 빠른 모드 1개(`D-codex-fast-mode`)를 만들었다. 2026-11-22에 남은 5개와 Codex Max·Ultra effort를 더한 6개 안내 카드의 판정·문구·테스트를 추가했다(0017 "첫 점검 (Codex) 2차"·"첫 점검 (Kiro)"). Kiro V3 검색·자동 압축 2개는 저장 설정에 후보가 있을 때만 CLI `--version`으로 하한을 확인하며, 버전을 모르면 숨긴다.
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
- [ ] 32px 메뉴바 막대 화면 확인, 숫자 모드의 32px 숫자 그림이 16px에서 읽히는지(`3e8ea82`).
- [ ] 배율이 다른 다중 모니터에서 팝오버 위치.
- [ ] 코칭 경로·권한, 6층 검진.
- [ ] 실제 rollout `payload.cwd` 형태(`\\?\C:\…`인지). `fsutil::is_inside`가 앞머리를 떼는 것은 단위 테스트만(`bad67dc`).
- [ ] 릴리스 Windows `.sha256`을 LF로 쓰게 고침(2026-11-25, v0.1.4·v0.1.5 파일은 CRLF라 macOS `shasum -c`가 실패, 해시 자체는 일치 확인). 다음 릴리스에서 `shasum -a 256 -c kcal-windows-x64-setup.exe.sha256`이 통과하는지 본다.
- [ ] Windows 진단하기 적용·Undo 실기(미리보기는 2026-11-25 확인). `get_checks` 경로·id의 `\\?\` 앞머리가 화면에 보이는지도 함께 본다. 2026-11-27 `17ff298`: IPC에는 여전히 남고(`A2-claude-rules-paths:\\?\C:\…`, `file`·`folder`), 대시보드·팝오버 본문에는 안 보였다. 숨긴 카드는 펼칠 수 없어 안을 못 봤다.
- [ ] **claude.ai 커넥터 `! Needs authentication`(2026-11-27, Windows, Claude Code 2.1.332).** 로컬 `plugin:` 줄에서 `⚠` 대신 `!`가 나왔다(확인됨). `coach/connectors.rs` `parse`가 `!`도 인증 필요로 읽게 고쳤다(2026-11-27, 단위 테스트). 다른 모르는 상태(예: `- Not configured`, claude.ai 줄에서 나오는지는 모름)는 지금처럼 결과 전체를 버린다(0031 "엄격히"). 남은 확인: 커넥터 하나를 인증 해제한 뒤 `claude mcp list`에서 claude.ai 줄도 `!`인지, kcal에 커넥터 행이 남는지.
- [ ] **팝오버가 화면 아래로 잘린다(2026-11-27, Windows, 사용자 보고 + dev 앱 재현, 1920×1080·125%).** 열 때 개요 높이로 top이 잡히고(top 338, bottom 809, 쓸 수 있는 높이 816, 논리 px) 열린 채 Claude 탭(668)·세션 펼침(740)으로 자라면 bottom이 1007·1078로 나갔다(확인됨, MCP 측정). 2026-11-27 고침: `tray.rs` `resize`가 Windows에서 높이를 바꾼 뒤 다시 놓는다(작업 영역 위에 붙어 있으면 top, 아니면 bottom 고정, 작업 영역 안으로 자름, `refit_top` 단위 테스트). macOS로는 Windows 경로를 컴파일하지 못해(교차 빌드에 `llvm-rc` 없음) CI가 첫 컴파일 확인이다. 2026-11-27 `8deb5e7` Windows 실제 앱 확인: 탭 바꾸기·세션 펼치고 접기에서 bottom이 작업 영역 끝(816)에 고정되고 접으면 줄어든다. **새로 남음: 높이가 바뀔 때마다 폭이 약 14px씩 커져 오른쪽 화면 밖으로 나간다**(`resize`가 outer 폭을 읽어 inner로 씀, 0.1.0부터, `docs/platform/windows-log.md` 2026-11-27). 2026-11-27 고침: 폭을 `inner_size()`로 읽는다. 남은 확인: Windows 실제 앱에서 높이를 여러 번 바꿔도 폭 310 유지. x는 작업 영역 안으로 자르지 않는다(폭이 안 자라면 넘칠 일 없음, 추정). 내용이 작업 영역보다 높으면 아래가 잘린다(`ponytail:`).
- [ ] **Windows 클론에서 `.claude/skills` 심볼릭 링크가 17바이트 텍스트 파일이 된다(2026-11-27, 확인됨).** `core.symlinks=false`(Git for Windows 기본)라 내용이 `../.agents/skills`인 일반 파일로 체크아웃된다. 그래서 이 기기의 Claude Code 세션에 `tauri-*` 프로젝트 스킬이 안 뜬다(`.kiro/skills`도 같은 구조라 같을 것, 추정). 고치는 방향: README에 `git clone -c core.symlinks=true`(개발자 모드·관리자 권한 필요)를 안내하거나 링크 대신 다른 방식을 쓴다. 2026-11-27 CONTRIBUTING "Setup"에 `git clone -c core.symlinks=true`를 적었다. 이미 받은 클론은 다시 받거나 `git config core.symlinks true` 뒤 `git checkout -- .claude/skills .kiro/skills`(추정).

## 나중에

- **4번째 에이전트 Antigravity(0027 ④) — 보류, 실제로 쓰는 사용자 기록이 생길 때.** ①설명표 ②대시보드 고르기 ③창 `group`은 끝났다(0027, 2026-11-19).
  - 읽기 조사(2026-11-21·27, 이 Mac): 기록은 `~/Library/Application Support/Antigravity/User/globalStorage/state.vscdb`(`antigravityUnifiedStateSync.*`, base64 protobuf, 스키마 모름)와 `~/.gemini/antigravity/conversations/*.pb`. 남은 쿼터 API·로컬 파일은 공식 문서에 없다(Settings 화면만). 이 Mac DB는 2026-05-21 뒤 안 바뀌어 검증 불가. 붙인다면 "세션 수만" 지원이 한계(추정). MCP·rules·skills 위치는 문서에 있음(`~/.gemini/config/{mcp_config.json,rules,skills}`, 작업 공간 `.agents/…`).
  - 붙일 때 다시 볼 것(2026-11-19 초안의 걱정): "전체"에서 한도 단위가 갈림(% 창 / 월 크레딧 / 한도 없음), 동심원 링 4개 이상·팝오버 탭 줄(폭 310)·메뉴바 막대가 좁아짐, 브랜드색 추가는 결정 기록(`docs/design.md`), 에이전트별 tuning 필드 묶기(0007), 공통 칸의 실측/추정 표시(0005), 한도 조회 타이머가 에이전트 수만큼 는다(`usage/schedule.rs`).
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
  - 겹치는 것: 위 "프로젝트 단위로 켜기"(계속 켜 둠)와 짝. 카탈로그는 "잠깐 켰다 치움"이다. 시작 전 한 결정 기록으로 묶을지 정한다.
- Claude Code `/insights` 심층 분석 → kcal "사용자 패턴 평가" 기능 검토. `/doctor`(설치·설정 점검)와 다른 기능이다. `/insights`는 최근 세션 로그로 사용 방식·마찰·CLAUDE.md 개선안을 HTML 리포트로 낸다.
  - 할 일: `/insights`를 돌려 리포트 항목을 정리하고, kcal이 하려는 사람 쪽 진단(무검토 승인, 확증편향·유도형 질문, 근거 요구 빈도)과 겹치는 것·빠진 것을 표로 비교한다.
  - 정할 것: kcal에 평가 기능을 넣을지. 넣는다면 "이미 있는 점검은 다시 만들지 않고 안내만" 원칙에 따라 `/insights`가 하는 부분은 안내로 두고 빠진 부분만 만든다. 0017 틀(유형·카드)에 맞는 칸이 있는지 먼저 본다.
  - 참고: 다중 에이전트판 [awesome-skills/insights](https://github.com/awesome-skills/insights).

- Linux 지원(요청 1명). OS별 경로·인증 읽기는 Rust 한 모듈에 모아 둔다.
- Claude Code statusLine 출력 모드(같은 실행 파일 인자) — 필요해지면 결정.
- 저장소 소셜 프리뷰 이미지(1280×640) 업로드와 GitHub topics 설정(macos, menu-bar, claude-code, codex, kiro, tauri). 저장소 설정에서 관리자가 한다.

- **Claude Code 2.1.307~332 신기능·11/27 vault 자료에서 나온 후보** (2026-11-27 정리, 추천 순서 1→2→3)
  1. 진단 카드 `CLAUDE_CODE_SUBAGENT_EFFORT` 미설정(2.1.316): 서브에이전트가 세션 effort를 물려받아 비싸다(추정, 확인 = 같은 작업 설정 전후 비교). `D-claude-effort-level` 옆.
  2. `claude auth status`의 `loginExpiresAt`(2.1.318)으로 로그인 만료 임박 표시, 한도 조회 401의 원인 안내(401 원인이 만료인지는 추정, 확인 = 만료 직전 401 로그 대조).
  3. 시작 컨텍스트에 MCP 서버 스킬(`server:skill`, 2.1.332)도 세기. 버전 고정 안 된 서버 표시. 실제 컨텍스트 크기는 모름(확인 = 새 세션 `/context` 비교).
  - 진단 카드 `CLAUDE_CODE_DISABLE_AGENT_PROGRESS_SUMMARIES`(2.1.323): 진행 요약 요청 비용. 규모는 모름.
  - 안내만: 고장 난 설정 파일·안 맞는 hook matcher는 `/status`(2.1.327), API 연결은 `claude doctor`(2.1.331)로 안내. 알려진 한계 "hook 점검은 최근 세션 폴더만"의 보완.
  - Code Mode(2.1.323) 켬 여부를 측정 조건으로 표시. 11/19 1회 A/B에서 켬 +19%(추정, 표본 1).
  - 카탈로그·프로젝트 단위: 플러그인 `engines`(2.1.323)·`optionalDependencies`(2.1.331)로 "이 Claude Code 버전에서 동작하나" 판정.
  - 사용자 패턴(`/insights` 검토 항목에 붙임): 확신도·근거 요구 빈도(Agent Harness 영상), 계속 손보는 프로젝트 수 = repo별 한 달 커밋한 날 5일 이상(Ronacher 영상, 1편 주장이라 기준은 측정 후 정함).
  - 판정 설계 참고: 카드 보고를 "파일·줄 → 출처 → 위험 → 경계 → 영향"으로, 파싱 실패는 통과가 아니라 unknown(AI-Infra-Guard). 0014 백업 대상을 비밀값·도구·신원으로 나누기(treg).
  - 확인만: easel(2.1.330) 차트가 대시보드와 겹치는지. 용도가 달라(세션 안 산출물 vs 상시 메뉴바) 둘 다 둔다(추정).
  - 뺀 것: `/tasks` 확장, `/tui classic`, `mcp_tool` hook onFailure, 이메일 전송 확인 — 사용량·비용 진단과 연결 없음(추정).
