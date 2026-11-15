# 0018. Claude에게 맡기기: 실행 경계와 안내문 쓰기 경계

- 상태: 결정(구현 전). 작업 A(실행 경계 + CLAUDE.md 200줄 카드)부터 넣고, 작업 B(`paths` 없는 rules 카드)는 A 뒤에 넣는다.
- 날짜: 2026-11-07
- 앞선 결정: [0014](0014-coaching.md) 미리보기·백업·되돌리기, [0017](0017-optimization-framework.md) "적용 방식"·"첫 점검"

## 문제

0017은 글을 고쳐 쓰는 처방(CLAUDE.md 줄이기 등)을 kcal이 직접 쓰지 않고 Claude에게 맡기기로 정했다. 이 결정에는 두 가지 새 경계가 따라온다.

1. kcal이 처음으로 다른 에이전트를 **실행해 한도를 쓴다**(지금까지는 `/usage` 조회처럼 한도를 안 쓰는 실행만 했다).
2. kcal이 설정 파일 3개(0014 `writable_files`) 밖, 사용자의 **안내문 파일(CLAUDE.md·rules)** 을 고쳐 쓴다.

AGENTS.md "보안 원칙"에 따라 경계를 바꾸기 전에 이 기록을 쓴다.

## 사용자 결정 (2026-11-07)

- **실행·적용 방식**: 제안대로 한다. Claude는 도구 없이(읽기 전용) 고쳐 쓴 글만 돌려준다. kcal이 차이를 보여 주고, 사용자가 고르면 0014 방식으로 백업한 뒤 바꾼다(되돌리기 가능). 쓰기 경계는 설정 파일에서 CLAUDE.md·rules 파일로 넓어지고, kcal은 **점검 목록에 뜬 파일만** 쓴다.
- **Opus 5.5를 쓸 수 없을 때**: 막고 이유를 알린다. 낮은 모델로 내려 실행하지 않는다.
- **200줄 카드 범위**: 최근 30일 Claude 세션 작업 폴더 전부. 기간은 모델 기록과 같은 tuning `checkModelDays`를 쓴다.
- **남은 한도 기준**: Claude 창(5h·7d) 중 가장 적게 남은 창의 남은 %가 20% 아래면 버튼을 끈다. 기준은 tuning이다. 한도를 모르면 버튼을 끈다.
- **`paths` 없는 rules의 감지**: Claude에게 후보를 받는다. 규칙마다 Claude가 `paths` 값, 스킬로 옮기기, 그대로 두기 중 하나를 제안하고 사용자가 고른다. 실행 경계는 위와 같다.

## 실측 사실 (2026-11-07, Claude Code 2.1.319, 이 Mac)

모두 확인됨. 실험 폴더는 빈 임시 폴더였다.

- `claude -p --safe-mode --model claude-opus-5-5 --effort medium --tools "" --no-session-persistence --output-format json`에 프롬프트를 stdin으로 넣으면 exit 0이다. JSON에 `result`·`is_error`·`usage`·`total_cost_usd`·`modelUsage`가 있다.
- `--safe-mode`는 CLAUDE.md·스킬·플러그인·hook·MCP·auto memory를 끈다(문서 `debug-your-config`, 인증과 모델 선택은 그대로). 같은 프롬프트에서 cache 생성 토큰이 10,919 → 1,746, 비용 추정이 0.088 → 0.014 USD로 줄었다. safe mode에서는 `~/.claude/projects/` 아래 폴더가 생기지 않았다(safe mode가 없으면 빈 `memory` 폴더가 생겼다). 두 경우 모두 세션 jsonl은 생기지 않았다.
- `--bare`는 쓸 수 없다. OAuth·키체인을 읽지 않아 구독 로그인 사용자가 실행할 수 없다.
- `--json-schema '<schema>'`를 주면 결과가 `structured_output`(객체)에 들어가고 `result`에는 같은 JSON 문자열이 들어간다. `--tools ""`와 함께 써도 되고, `num_turns`는 2다.
- 없는 모델(`claude-opus-9-9`)이면 exit 1, `is_error: true`, `terminal_reason: "api_error"`, `api_error_status: 404`다. `result`는 "There's an issue with the selected model (…). It may not exist or you may not have access to it. …"이고, stderr에 `[claude-code:unrecognized_model]`이 찍힌다. Opus를 쓸 수 없는 플랜에서도 같은 모양인지는 **모름**이다(그런 계정이 없다). 그래서 404와 `unrecognized_model`을 둘 다 "쓸 수 없음"으로 본다.
- 사용자 설정 `"language": "korean"`은 safe mode에서도 먹는다. 영어 규칙 3줄을 줄여 달라고 하자 한국어로 번역해 돌려줬다. 프롬프트에 "원래 언어와 말투를 그대로 두고 번역하지 말라"를 넣자 영어 그대로 왔고, 뺀 것 설명(`removed`)만 한국어로 왔다. → 프롬프트에 이 줄을 **반드시** 넣는다.
- CLI 위치: 이 Mac은 `~/.local/bin/claude`(→ `~/.local/share/claude/versions/2.1.319` 링크)다. 메뉴바 앱은 셸 `PATH`를 물려받지 않으므로(0005) 설치 위치를 먼저 보고 `PATH`를 다음에 본다. Windows 설치 위치 `%USERPROFILE%\.local\bin\claude.exe`는 **추정**이다(Windows에서 `where claude`로 확인).

## 실행 경계

- **언제**: 사용자가 카드에서 버튼을 누를 때만 한 번 실행한다. 자동 실행·재시도·예약은 없다. 한 번에 하나만 돌린다. 돌고 있으면 "이미 맡긴 일이 끝나기를 기다리는 중이에요"로 거절한다.
- **무엇을**: 위 "실측 사실"의 인자에 `--json-schema`를 더한다. 모델 `claude-opus-5-5`·effort `medium`은 고정이다(0017). 프롬프트는 stdin으로 넣는다. 인자 길이 한도를 피하고, 파일 내용이 프로세스 목록에 보이지 않게 한다. 작업 폴더는 kcal 앱 데이터 폴더 아래 빈 폴더다(프로젝트 설정을 읽지 않게).
- **프로세스**: `usage/sources.rs`의 `run`(타임아웃·`kill_on_drop`·Windows 창 숨김)을 같이 쓴다. stdin 입력과 작업 폴더를 받도록 넓히고, 기존 호출(`/usage`)의 동작은 그대로 둔다.
- **시간 초과**: tuning `claudeHandoffTimeoutSecs`(기본 300, 60~1800, 정수). 넘으면 프로세스를 죽이고 "Claude가 N분 안에 끝내지 못했어요"로 알린다.
- **남은 한도**: tuning `claudeHandoffMinRemaining`(기본 20, 0~90, 정수, %). Claude `AgentUsage::min_remaining`이 기준 아래이거나 값이 없으면 버튼을 끈다. 화면만 믿지 않고 Rust command에서 실행 직전에 다시 검사한다. 버튼 바로 아래에 "한도를 씁니다(남은 한도가 있어야 실행)"와 지금 남은 %를 적는다(0017).
- **결과 판정**: exit 0, `is_error: false`, `structured_output`이 스키마에 맞아야 성공이다. 404·`unrecognized_model`이면 "이 계정에서는 Opus 5.5를 쓸 수 없어 맡길 수 없어요"로 막는다. 그 밖의 오류는 짧은 이유만 보인다(stderr 전체·토큰·경로 밖 정보는 화면에 내지 않는다). 실패해도 파일은 건드리지 않는다.
- **결과 보관**: 고쳐 쓴 글은 Rust 상태에 (카드 id, 원본 `before_hash`)를 열쇠로 하나만 둔다. 적용 command는 화면이 보낸 글이 아니라 이 보관본을 쓴다(화면에서 임의 글을 쓰게 하지 않는다). 같은 열쇠로 다시 누르면 다시 실행하지 않고 보관본을 돌려준다(한도를 두 번 쓰지 않는다). 앱을 끄면 사라진다.
- **보내는 것**: 고칠 파일의 내용과 고치는 방법 설명만 보낸다. 그 파일은 Claude Code가 세션마다 이미 보내는 내용이다. `USD` 비용은 구독 사용자에게 뜻이 없어 화면에 보이지 않는다.

## 안내문 쓰기 경계

- **쓸 수 있는 파일**: 적용 때는 **지금 점검 목록에 뜬 카드의 파일과 같은 경로**만 쓴다. 카드가 없으면 거절한다.
- **되돌리기**: 0014 `undo`는 `writable_files` 고정 목록만 받는다. 고쳐 쓴 뒤에는 카드가 사라지므로(200줄 아래가 됨) 목록으로는 되돌릴 수 없다. 그래서 기록 `target`이 `claude/check:`로 시작하고, 파일이 Claude 안내문 모양(이름 `CLAUDE.md`·`CLAUDE.local.md`, 또는 `.claude/rules/` 아래 `.md`)이고, 백업이 kcal 것(`is_backup_of`)일 때만 되돌린다. 나머지 조건(적용 뒤 파일이 바뀌면 거절, 잠금, 기록)은 0014와 같다.
- **링크**: 안내문 파일이 링크(예: dotfiles 저장소)일 수 있다. temp→rename으로 링크 자리를 바꾸면 링크가 일반 파일로 바뀐다. 그래서 링크를 따라간 실제 파일을 쓰고, 백업도 실제 파일 옆에 둔다. 기록에는 실제 경로를 남긴다.
- **확인**: 적용 직전에 원본 해시를 다시 비교한다(0014 `read_unchanged`). 다르면 "Claude에게 맡긴 뒤 파일이 바뀌었어요. 다시 맡겨 주세요"로 거절한다.

## 작업 A: CLAUDE.md 200줄 카드

- **감지**: 최근 `checkModelDays`일 Claude 세션 작업 폴더 전부(세션 기록 줄의 `cwd`, 모델 기록과 같은 walker에서 같이 읽는다. 지금 없는 폴더는 뺀다)마다 0005 Claude 안내문 측정(`instructions::claude`)이 시작 때 싣는 파일 중 이름이 `CLAUDE.md`·`CLAUDE.local.md`인 것을 모은다. 같은 파일은 한 번만 본다. `@`로 가져오는 파일(예: AGENTS.md)은 넣지 않는다. Codex·Kiro도 읽는 파일일 수 있고, 되돌리기 조건이 넓어진다. 파일당 줄 수가 200을 넘으면 카드 하나다. 200은 문서 값(`memory#write-effective-instructions`)이라 tuning이 아니라 상수다.
- **카드**: 제목 "안내문이 N줄이라 Claude가 다 지키기 어려워요"(0017). 버튼 "Claude에게 줄여 달라고 하기"(추천)와 "그대로 두기".
- **프롬프트**: 규칙을 하나도 빼지 말고 겹치는 것을 합쳐 200줄 아래로 줄인다. 원래 언어와 말투를 그대로 둔다(번역 금지). `@` 가져오기 줄은 그대로 둔다. 파일 전체를 `text`에, 합치거나 뺀 것을 `removed`(한국어 문장 목록)에 담는다.
- **미리보기**: 줄 단위 차이(LCS, 새 의존성 없음)를 0014 미리보기 패널로 보인다. Claude의 `removed` 목록을 "Claude가 합치거나 뺀 것"으로 함께 보인다. 원본의 `@` 가져오기 줄이 결과에 없으면 "빠진 가져오기"로 경고한다. 결과가 비었거나 원본보다 길면 적용 버튼을 끈다.
- **적용**: 보관본을 0014 방식(잠금·해시·백업·temp→rename·기록)으로 쓴다. 기록 이름 "<파일 이름> 줄이기".

## 작업 B: `paths` 없는 rules 카드 (A 뒤)

- **감지**: 같은 작업 폴더들에서 시작 때 실리는(`paths` 없는) rules 파일. 카드 하나에 모은다.
- **Claude 후보**: 한 번 실행으로 규칙마다 `paths`(glob 목록) · 스킬로 옮기기(이름·설명) · 그대로 두기(이유) 중 하나를 받는다. 사용자가 규칙마다 고른다.
- **적용**: `paths`는 kcal이 front matter에 기계적으로 붙인다. 붙이기 전에 파일당 글자 한도(0017 행 A, 200K 창 40,000자)를 검사한다. 스킬로 옮기기는 `~/.claude/skills/<이름>/SKILL.md`를 만들고 규칙 파일을 지운다(둘 다 백업·되돌리기). 이름이 겹치면 거절한다.
- **잃는 것을 적는다**: `paths` 규칙은 압축 뒤 빠졌다가 그 파일을 다시 열면 돌아온다. 스킬은 Claude가 관련 있다고 볼 때나 사용자가 부를 때만 실린다.

### 사용자 결정 (작업 B, 2026-11-08)

- **스킬로 옮기기 = 한 기록으로 묶는다.** 규칙 파일은 지우지 않고 같은 폴더의 백업 이름(`<이름>.kcal-<시각>.bak`, 0014 백업 모양)으로 옮기고, SKILL.md를 새로 만든다. 기록 한 줄에 새 파일 경로를 더한다. 되돌리기는 SKILL.md가 kcal이 쓴 그대로이고 규칙 자리가 비어 있을 때만, 백업을 규칙 자리로 되살리고 SKILL.md(와 비었으면 그 폴더)를 지운다. 위 "적용"의 "규칙 파일을 지운다"를 이것으로 바꾼다.
- **SKILL.md 본문 = 규칙 본문 그대로.** Claude는 `name`·`description`만 제안하고, kcal이 머리말을 붙인다. 본문은 한 글자도 바꾸지 않는다.

### 조율자 기본값 (작업 B, 2026-11-08, 사용자 확인 "이대로" 2026-11-08)

- **카드는 범위마다 하나**: 사용자 rules(`~/.claude/rules/**`) 하나, 프로젝트마다(`<작업 폴더>/.claude/rules/**`) 하나. 위 "감지"의 "카드 하나에 모은다"를 좁힌다. 이유: 스킬이 갈 곳이 범위마다 다르고(`~/.claude/skills/` · `<작업 폴더>/.claude/skills/`), 한 번에 보내는 글 양이 범위 하나로 묶인다.
- **보내는 이름**: 절대 경로 대신 범위 안 상대 이름(예: `common/git-workflow.md`)만 보낸다(홈 폴더 이름을 보내지 않는다).
- **다른 규칙이 이 파일을 가리킬 때**(이 Mac: `coding-style.md` → `simplicity.md`, `development-workflow.md` → `git-workflow.md`): 링크는 고치지 않고 "다른 규칙 N곳이 이 파일을 가리켜요"로 경고만 한다. 문서에 있는 가져오기는 `@` 줄뿐이라(`memory#import-additional-files`) 마크다운 링크가 끊겨도 시작 때 실리는 것은 같다(추정: Claude가 그 링크를 Read하려다 없는 파일을 만날 수는 있다).
- **이미 머리말이 있는 규칙**: `paths`는 기존 머리말 안에 넣는다. 스킬로 옮기기는 거절한다("머리말이 있는 규칙은 아직 옮기지 않아요"). 머리말 다른 키를 SKILL.md에 어떻게 옮길지 정하지 않았기 때문이다.
- **스킬 이름·설명 검사**: 이름은 kcal 기준으로 영문 소문자·숫자·`-`, 1~64자, `-`로 시작·끝나지 않음(폴더 이름이 되므로 macOS·Windows 둘 다 안전한 쪽). 설명은 한 줄, 1~1,536자(문서 `skills#frontmatter-reference`: 목록에서 `description`+`when_to_use`가 1,536자에서 잘린다). 설명은 규칙 본문과 같은 언어로 받는다.
- **`paths` 검사**: 1~20개, 하나에 1~200자, 줄바꿈·제어 문자 없음, `/`·`~`로 시작하지 않음. 큰따옴표 YAML 목록으로 쓰고, 쓴 뒤 0005 `front_matter_key(.., "paths")`로 다시 읽혀야 한다(문서 `memory#frontmatter-reference`: YAML이 깨지면 `paths` 없는 것처럼 실린다). 본문이 40,000자를 넘으면 `paths` 적용을 끈다(0017 행 A).

## 남은 것

- 취소 버튼은 넣지 않는다. 실제 실행이 자주 1분을 넘으면 넣는다.
- `--max-budget-usd`는 넣지 않는다. 도구가 없어 한 턴(+구조화 출력)으로 끝나 비용이 늘 수 없다.
- 오래된 한도 값(조회가 오래 실패한 경우)도 "아는 값"으로 본다. 문제가 되면 `observed_at` 기준을 더한다.
- Opus를 쓸 수 없는 플랜의 실제 오류 모양, Windows 설치 위치는 확인해야 한다.

## 구현하며 정한 것 (작업 A, 2026-11-07)

- **모듈**: 감지 `src-tauri/src/coach/checks/long_instructions.rs`, 실행·판정·보관·적용·되돌리기 조건 `handoff.rs`(테스트 `handoff_tests.rs`), 줄 차이 `diff.rs`(LCS 표, 표가 4,000,000칸을 넘으면 원본 전체 빼기 + 결과 전체 넣기로 보인다, `ponytail:` 주석). `checks/mod.rs`는 카드 종류 하나·모듈 선언만 늘었다.
- **IPC**:

| command                | 입력               | 출력                                                                                                                                             |
| ---------------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `ask_claude_handoff`   | `id`               | `HandoffPreview`(file·beforeHash·beforeLines·afterLines·diff `{kind, text}`·removed·missingImports·canApply). 실패는 `HandoffError`(`kind` 태그) |
| `apply_claude_handoff` | `id`, `beforeHash` | `AppliedSwitch`(`target` = `claude/check:<id>:handoff`, 이름 "<파일 이름> 줄이기"). 되돌리기는 `undo_switch`                                     |

- `HandoffError` 종류: `busy`·`limitUnknown`·`limitTooLow {remaining, minRemaining}`·`cliMissing`·`opusUnavailable`·`timeout {secs}`·`failed {reason}`·`refused {message}`(=`AppError::InvalidInput`)·`internal`. 문구는 `check-copy.ts` `handoffErrorText`가 만든다. `failed`의 이유는 `exit <코드>`와 `terminal_reason`(영문 소문자·숫자·`_` 40자까지)만, 또는 "Claude 응답을 읽지 못했어요"·"Claude 응답이 약속한 모양이 아니에요"·"Claude를 실행하지 못했어요". stdout `result`·stderr 본문은 넣지 않는다.
- **카드**: id `B-claude-long-instructions:<실제 경로>`, 유형 B(겹치는 것을 합친다), `facts.type = "longInstructions"`(`name`·`lines`·`limit`·`imports`). 옵션 `handoff`(추천, `isWritable: false` — `preview_check`·`apply_check`로는 쓸 수 없다)와 `keep`. 알림 개수에 들어간다(추천이 `keep`이 아니다).
- **감지 세부**: 작업 폴더는 같은 walker가 이미 읽는 assistant 줄의 `cwd`다(JSON을 한 번 더 읽지 않는다). 대상 = 0005 측정이 시작 때 싣는 파일 ∩ 이름 자리(사용자 `~/.claude/CLAUDE.md`, 작업 폴더와 그 위 폴더의 `CLAUDE.md`·`.claude/CLAUDE.md`·`CLAUDE.local.md`)의 실제 경로. 그래서 `@`로만 가져온 파일은 빠진다. 줄 수 = `str::lines()` 개수(끝 줄바꿈 뒤 빈 줄은 세지 않는다).
- **링크 대상 이름**: 실제 파일 이름이 `CLAUDE.md`·`CLAUDE.local.md`인 것만 본다. 실데이터에 `CLAUDE.md -> AGENTS.md` 링크(280줄)가 있었다. 그 실제 파일은 AGENTS.md라 Codex·Kiro도 읽고, 기록의 실제 경로가 되돌리기 조건(안내문 모양)에 맞지 않아 적용 뒤 앱에서 되돌릴 수 없게 된다. "이름이 CLAUDE.md"와 "기록에는 실제 경로"를 함께 지키려고 실제 경로 이름으로 판정했다.
- **"그대로 두기"**: 이 카드는 추천이 `keep`이 아니어도 숨길 수 있다(`dismissed.rs` `is_dismissable`). 0017 "그대로 두기" 규칙(추천이 `keep`인 카드만)에 이 카드 종류 하나를 더했다. 숨긴 카드는 "다시 보기"로만 돌아온다(파일이 더 길어져도 숨긴 채). 2026-11-07 사용자 승인("숨기게 둠").
- **실행 세부**: 인자는 "실측 사실"의 줄 + `--json-schema {"text": string, "removed": string[]}`(`additionalProperties: false`). 프롬프트는 `<앱 데이터>/claude-handoff/prompt-<pid>.txt`에 써서 그 파일을 stdin으로 주고(인자·프로세스 목록에 내용이 없다), 끝나면 지운다. 작업 폴더는 `<앱 데이터>/claude-handoff/work`(kcal은 그 안에 아무것도 쓰지 않는다). `usage/sources.rs` `run`은 `RunInput { stdin, cwd }`를 받고, 기존 `/usage`·Keychain 호출은 `RunInput::default()`(stdin null, 작업 폴더 그대로)라 동작이 같다. 이를 위해 `usage/mod.rs`의 `paths`·`sources` 선언을 `pub(crate)`로 바꿨다(범위 밖 파일 두 줄, `pub(crate)`가 crate 안에서 보이려면 필요).
- **순서**: 같은 열쇠의 보관본이 있으면 바로 돌려준다(한도 문도 거치지 않는다, 실행이 없으니까) → 돌고 있으면 `busy` → 한도 문 → CLI 찾기 → 실행 → 보관. 보관본은 하나뿐이라 다른 카드를 맡기면 앞 것은 사라진다.
- **removed 정리**: 앞뒤 공백을 떼고 빈 줄은 뺀다. 50개·한 줄 300자까지.
- **적용할 수 없는 결과**: 비었거나(공백만), 줄 수가 원본보다 많거나, 원본과 똑같으면 `canApply: false`. 원본이 줄바꿈으로 끝나면 결과 끝에 줄바꿈 하나를 맞춘다.
- **화면**: 버튼 아래 "한도를 씁니다(남은 한도가 있어야 실행)", "지금 남은 한도 N%"(모르면 "지금 남은 한도: 모름"), 꺼졌으면 빨간 이유("남은 한도가 N%보다 적어요." / "한도를 아직 몰라요."). 남은 %는 점검 탭이 `useUsageSnapshot`(get_usage + `usage-updated`)에서, 기준은 `useSettings`에서 읽는다(새 polling 없음). 실행 중에는 그 카드 자리에 "Claude가 고쳐 쓰는 중이에요. 길면 N분까지 걸려요."(버튼 없음, 취소는 "남은 것" 그대로 미룸)와 다른 카드 버튼을 끈다. 미리보기는 그 카드 자리의 "Claude가 고쳐 쓴 글" 패널(줄 수 변화, 파일, 차이 상자 `max-h-64` 스크롤, "Claude가 합치거나 뺀 것", 빨간 "빠진 가져오기", 확인하고 바꾸기). 0014 `ChangePanel`은 한 줄 바꾸기용이라 쓰지 않고 완료 패널(`DonePanel`)만 같이 쓴다. 실패하면 카드로 돌아가 칸 맨 위에 문구를 보인다.
- **tuning**: `claudeHandoffTimeoutSecs`(기본 300, 60~1800, 정수) 설정 패널 "시간 초과"의 "Claude에게 맡기기"(분 단위로 보인다), `claudeHandoffMinRemaining`(기본 20, 0~90, 정수) "한도 조회"의 "Claude에게 맡길 때 필요한 남은 한도"(%). 기준과 같으면 실행한다(기준 **아래**만 거절).
- **실제 실행 테스트(돌리지 않았다)**: `cd src-tauri && cargo test --lib real_claude_rewrites_overlapping_claude_md -- --ignored --nocapture`. TempDir에 겹치는 규칙 10묶음·`@` 가져오기 두 줄인 영어 CLAUDE.md(약 234줄)를 만들고, 실제 HOME의 `claude`를 찾아 한 번 실행한다. 한도 문은 거치지 않으니 남은 한도를 먼저 직접 본다. 출력: `before_lines`·`after_lines`·`can_apply`·`changed_lines`·`missing_imports`·`removed:` 줄.
- **실데이터(읽기 전용, 2026-11-07 이 Mac)**: 최근 30일 작업 폴더 308개 중 지금 있는 것 198개, 모은 안내문 27개, 카드 1개(`~/.claude/plugins/cache/<marketplace>/<plugin>/<버전>/CLAUDE.md` 214줄). 기록 읽기 1.4초(release), 안내문 모으기 0.13초. 아래 "플러그인 캐시"로 그 카드는 빠져 지금은 0개다.
- **플러그인 캐시(조율자 검토에서 더함)**: `~/.claude/plugins/` 아래 실제 경로는 대상에서 뺀다(`long_instructions.rs` `loaded_claude_files`, 테스트 `claude_md_in_plugin_cache_is_not_a_target`). 그 안에서 세션을 열면 그 CLAUDE.md도 시작 때 실리지만, 사용자가 쓴 파일이 아니고 플러그인을 업데이트하면 덮여 고쳐 써도 남지 않는다. 고칠 곳은 플러그인 원본 저장소다.

### 알려진 한계 (작업 A)

- 링크의 실제 파일 이름이 `CLAUDE.md`·`CLAUDE.local.md`가 아니면(예: dotfiles의 `claude-global.md`) 카드가 뜨지 않는다(위 "링크 대상 이름").
- Windows 경로·실행은 이 Mac에서 확인하지 않았다(가짜 CLI 테스트는 unix만). Windows CI 컴파일은 아직이다.

## 구현하며 정한 것 (작업 B, 2026-11-08)

- **모듈**: 감지 `src-tauri/src/coach/checks/rules_paths.rs`, 프롬프트·스키마·검사·보관·command `rules_handoff.rs`(테스트 `rules_tests.rs`), 적용·스킬 되돌리기 `rules_apply.rs`(테스트 `rules_apply_tests.rs`). `handoff.rs`는 `execute`·`classify`가 스키마 문자열과 결과 타입(`T: DeserializeOwned`)을 받게 넓혔다. 작업 A는 같은 스키마·같은 `Rewrite`이고, `removed` 정리(앞뒤 공백·빈 줄·50개·300자)는 A의 `ask` 안 `tidy`로 옮겼다. 실행 중 표시(`HandoffState.is_running`)는 A·B가 같이 써서 한 번에 하나만 돈다. B 보관본은 같은 `HandoffState`의 칸 하나(`rules`)다.
- **IPC**:

| command             | 입력                      | 출력                                                                                                                                                                                                                                                         |
| ------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `ask_claude_rules`  | `id`                      | `RulesPreview`(id·scope·folder·rules `RuleProposal[]`·unknownKeys). 규칙 줄 = key·name·chars·hasFrontMatter·referencedBy·beforeHash·proposal(`choice` 태그: paths·skill·keep·unusable{problem})·reason·skillExists·canApply. 실패는 `HandoffError`(A와 같다) |
| `apply_claude_rule` | `id`, `key`, `beforeHash` | `AppliedSwitch`(`target` = `claude/check:<id>:paths` 또는 `:skill`). 되돌리기는 `undo_switch`                                                                                                                                                                |

- **카드**: id `A2-claude-rules-paths:<범위 rules 폴더 실제 경로>`, 유형 A(`CheckKind::A`를 더했다), `facts.type = "rulesPaths"`(scope `user|project`·folder·chars 합·limit 40,000·rules[name·chars·hasFrontMatter·referencedBy]). 카드 `file`은 rules 폴더. 옵션 `handoff`(추천, `isWritable: false`)와 `keep`. 숨길 수 있고(`dismissed.rs` `is_dismissable`에 이 종류를 더함) 알림 개수에 들어간다. 글자 수는 머리말을 뗀 본문의 `chars()` 개수(Claude가 싣는 양).
- **감지 세부**: 작업 폴더마다 `instructions::claude`의 `Rules`·`Startup` 항목 실제 경로 중, 플러그인 캐시(`~/.claude/plugins` 실제 경로 아래)가 아니고 `is_instruction_file`에 맞고, 사용자 rules 폴더 또는 그 작업 폴더 rules 폴더의 실제 경로 아래인 것만 본다(밖으로 가는 링크는 뺀다). 작업 폴더가 홈이면 두 폴더가 같아 사용자 범위로 센다. 같은 파일은 범위 안에서 한 번. 작업 폴더가 하나도 없으면(기록 없음) 카드도 없다.
- **"다른 규칙이 가리킴"**: 같은 rules 폴더의 모든 `.md`(머리말 있는 규칙 포함)에서 마크다운 링크 `](…)`(`#앵커` 떼고 웹 주소 빼고)와 `@` 가져오기를 그 파일 기준으로 풀어 실제 경로가 같으면 센다(자기 자신 제외). 이를 위해 `context/instructions.rs` `resolve_import`를 `pub(crate)`로 열었다.
- **프롬프트·검사**: 규칙마다 `<rule key="rN" name="상대 이름">본문</rule>`(키는 이름순 `r1`…). 번역 금지·설명은 본문과 같은 언어·`reason`은 한국어 한 문장·쓰지 않는 칸은 빈 값을 넣는다. `choice`는 스키마가 세 값으로 묶지만 Rust는 문자열로 받아 한 규칙이 어긋나도 나머지를 쓴다. 검사 탈락 이유는 `RuleProblem`(missing·duplicate·choice·skillName·description·pathsCount·pathText·pathAbsolute, 문구는 `check-copy.ts` `ruleProblemText`). `paths`는 `\`로 시작해도 절대 경로로 본다(Windows). 설명은 앞뒤 공백을 떼고 검사한다. 이유는 첫 줄만 300자까지. 모르는 키는 개수만(`unknownKeys`, 화면 한 줄).
- **보관·재사용**: 보관본 열쇠는 카드 id 하나, 규칙마다 실제 경로·원본 해시·판정을 둔다. 다시 누르면 지금 카드의 규칙이 **모두** 보관본에 같은 해시로 있을 때 실행하지 않는다(한 규칙을 적용해 카드가 줄어도 그대로 쓴다). 적용은 보관본의 키로 파일을 찾고, 그 파일이 지금 카드에 있고 맡긴 때와 지금 해시가 같아야 한다. 열쇠는 맡긴 때 붙인 것이라 카드가 줄어도 바뀌지 않는다.
- **`paths` 적용**: 머리말 블록에 이미 `paths:` 줄이 있으면(값이 비어 늘 실리는 경우) 키가 두 번 생기지 않게 거절한다. 여는 줄이 CRLF면 넣는 줄도 CRLF. glob은 JSON 문자열로 따옴표를 친다(YAML 큰따옴표 문자열과 같다). 쓴 뒤 `front_matter_key(.., "paths")`가 읽히고 `body_of`(머리말 뒤)가 원본과 바이트 단위로 같아야 쓴다.
- **스킬로 옮기기**: 0018 순서 그대로. `skills/` 폴더는 없으면 만들고 실패해도 남긴다(빈 폴더). 백업 이름이 이미 있으면 거절. SKILL.md 쓰기가 실패해도 만든 폴더를 지운다. 거절 문구에는 앱 밖 되돌리기 명령을 붙인다.
- **기록 `created`**: `AppliedSwitch.created: Option<String>`(`serde(default, skip_serializing_if)`, TS `created?: string`). 예전 기록 줄은 그대로 읽히고, 없으면 줄에 넣지 않아 기존 fixture(`coach-log.json`)는 그대로다. 구조체 칸이 늘어 모든 리터럴에 `created: None`을 더했다. 이 때문에 범위 밖 `src-tauri/src/health/tests.rs` 한 줄과 작업 A 테스트 `handoff_tests.rs` 한 줄(`created: None`)을 고쳤다(동작은 그대로, 컴파일에 필요).
- **스킬 되돌리기**: `coach::undo`가 잠금·기록 찾기·이미 되돌림 검사 뒤 `created`가 있으면 `rules_apply::undo_skill`로 간다. `created` 모양 = 규칙 경로의 `.claude/rules` 앞 `.claude` 아래 `skills/<스킬 이름 규칙>/SKILL.md`(절대 경로, `..` 없음).
- **화면**: 카드는 A의 `HandoffActions`(한도 문·남은 %·그대로 두기)를 그대로 쓴다. 버튼 "Claude에게 규칙마다 물어보기". 결과는 그 카드 자리의 "Claude가 규칙마다 제안한 것"(`src/components/rules-panel.tsx`): 규칙마다 이름·글자 수, 제안, Claude 이유, "잃는 것", 경고(빨간 글씨), "이대로 바꾸기"(`keep`·탈락은 버튼 없음). 적용한 줄은 "✓ 바꿨어요. 되돌리려면 진단하기의 '최근 바꾼 것'에서…"로 바뀌고 칸은 남는다. "최근 바꾼 것" 줄 title에 SKILL.md 경로를 더했다. 실행 중 문구 "Claude가 규칙을 읽는 중이에요. 길면 N분까지 걸려요."
- **파일 나누기(800줄 규칙)**: `src/lib/ipc.ts`가 이미 846줄이라 점검·맡기기 부분을 `src/lib/ipc-checks.ts`로 옮기고 `ipc.ts`에서 다시 내보낸다(컴포넌트는 그대로 `@/lib/ipc`에서 가져온다). B IPC 계약 테스트는 `src/lib/ipc-rules.test.ts`. `checks-view.tsx`의 `Inline`은 `rules-panel.tsx`로 옮겨 한 방향으로만 가져온다. 범위 목록에 없던 새 파일 셋이다.
- **미리보기**: `dashboard.html?tab=checks&rules=ok|opus&remaining=N`.
- **실제 실행 테스트(돌리지 않았다)**: `cd src-tauri && cargo test --lib real_claude_proposes_where_rules_belong -- --ignored --nocapture`. TempDir 홈에 규칙 3개(`frontend/react-components.md` 특정 파일용, `release.md` 특정 작업용, `communication.md` 늘 필요)를 만들고 실제 HOME의 `claude`를 한 번 실행한다. 한도 문은 거치지 않으니 남은 한도를 먼저 직접 본다. 출력: 규칙마다 `key name can_apply proposal reason`, `unknown_keys`.
- **실데이터(읽기 전용, 2026-11-08 이 Mac, release)**: 최근 30일 작업 폴더 200개, 카드 2개. 사용자 `~/.claude/rules` 9개(`common/*`, 20,631자, 다른 규칙이 가리킴 경고 4개: development-workflow·git-workflow·simplicity·testing), 프로젝트 하나 10개(16,104자, 경고 5개). 머리말 있는 규칙 0개. 기록 읽기 2.5초, 점검 목록 0.3초.

### 알려진 한계 (작업 B)

- Opus를 쓸 수 없는 플랜의 오류 모양, Windows 경로·실행·`move`/`del` 명령은 이 Mac에서 확인하지 않았다(A와 같다).
- Claude의 실제 제안 품질(특히 사용자 범위 규칙의 `paths` glob이 프로젝트마다 맞는지)은 실제 실행 전이라 모른다.
- 마크다운 링크가 가리키는 파일을 스킬로 옮기면 그 링크는 끊긴다(경고만, 0018 조율자 기본값).
