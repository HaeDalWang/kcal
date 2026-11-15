# 0014. 5층 코칭: 켜고 끄는 스위치 제안과 적용

- 상태: 채택(구현함, 2026-11-05)
- 날짜: 2026-11-05
- 앞선 결정: [0003](0003-product-scope.md) "5. 코칭", [0005](0005-context-measurement.md), [0007](0007-tuning.md), [0008](0008-breakdown.md), [0009](0009-ui-plain-language.md), [0010](0010-tray-and-dashboard.md). 근거: ctxmeter `develop/decisions/05-fix-mutation-model.md`, `src/fix.js`·`switches.js`·`config-edit.js`

## 문제와 목표

kcal은 시작 컨텍스트에서 무엇이 토큰을 쓰는지 보여 주지만, 사용자가 직접 행동할 방법이 없다. "진단하기" 버튼(0010)은 비활성 상태로 남아 있다.

완료 기준: 팝오버 "진단하기"에서 ① 꺼서 아낄 수 있는 스위치(크기를 측정한 것, 절약 큰 순)를 보고 ② 바꿀 줄을 미리 보고 ③ 적용하면 백업 뒤 값 하나만 바뀌고 ④ 앱 안에서 되돌릴 수 있다. 적용 기록은 6층 전후 비교에 쓴다.

바뀌지 않는 것: 기존 측정·환율·한도 화면. capability·의존성 추가 없음.

## 결정 (사용자 선택 2026-11-05)

| 결정                                                                                                                                                                                                                                                                          | 이유                                                                |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| 범위는 ctxmeter와 같다. Codex `~/.codex/config.toml`의 `[plugins."<id>"]`·`[mcp_servers.<name>]` `enabled`, Kiro `~/.kiro/settings/mcp.json`의 `mcpServers.<name>.disabled`, Claude `~/.claude/settings.json`의 `enabledPlugins.<id>`                                         | 모든 변경이 각 에이전트가 원래 제공하는 값 하나다                   |
| Claude MCP는 쓰지 않는다. "Claude Code에서 /mcp로 끄세요" 안내만 한다                                                                                                                                                                                                         | `~/.claude.json`은 Claude가 실행 중에 다시 쓴다(ctxmeter 05 정정)   |
| CLAUDE.md·AGENTS.md·rules·steering·스킬 본문은 건드리지 않는다                                                                                                                                                                                                                | 설정이 아니라 사용자가 쓴 글이다. 비용은 지금처럼 보여 준다         |
| 제안 = 켜져 있고 시작 때 실린 측정 토큰이 있는 것, 토큰 큰 순. 크기 모르는 MCP(실행하지 않음)·`deferred`는 제안하지 않고 "다른 스위치" 목록에 "크기 모름"으로만 둔다                                                                                                          | 약속할 절약량이 없다                                                |
| TOML은 줄 단위로만 고친다(섹션 안 `enabled` 교체, 없으면 헤더 바로 아래 삽입). 섹션이 없거나 두 번 나오면 거부. 여러 줄 배열·문자열 안의 `[`는 헤더로 보지 않는다                                                                                                             | 주석·다른 섹션을 그대로 둔다. TOML writer 의존성 없음               |
| JSON은 `serde_json`(`preserve_order`)으로 읽고 다시 쓴다. 파싱 실패(JSONC·끝 쉼표)는 거부, 다시 읽어 바꾼 키만 달라졌는지 확인, 끝 줄바꿈 유지                                                                                                                                | 주석이 없는 기계 생성 파일이다. 고치다 망가뜨리느니 거부한다        |
| 미리보기와 적용 사이에 파일이 바뀌면 거부(미리보기 때 내용 해시를 적용 요청에 같이 보낸다)                                                                                                                                                                                    | 에이전트가 실행 중에 쓴 변경을 덮지 않는다                          |
| 백업은 원본 옆 `<이름>.kcal-<UTC 시각>.bak`. 쓰기는 같은 폴더 임시 파일 → rename, 권한(mode) 유지, 심볼릭 링크면 가리키는 파일에 쓴다                                                                                                                                         | 사용자가 원본 옆에서 찾는다. 반쯤 쓴 파일이 보이지 않는다           |
| 적용 기록 `app_data_dir()/coach-log.jsonl`(한 줄: 시각·대상·파일·백업·켬/끔·토큰·적용 뒤 해시). 앱 안 "되돌리기"는 현재 파일 해시가 기록의 적용 뒤 해시와 같을 때만 백업을 복사한다. 다르면 거부하고 `cp` 명령을 보여 준다                                                    | 앱을 지워도 `cp`로 되돌릴 수 있다. 6층 전후 비교의 "처방한 날" 기록 |
| command는 대상 ID만 받는다. Rust가 스위치 목록을 다시 계산해 그 안에 있는 대상만 바꾼다. 되돌리기는 기록 ID만 받고, 백업 경로가 원본과 같은 폴더·`.kcal-*.bak` 이름인지 확인한다                                                                                              | 프론트가 임의 경로를 쓰게 하지 않는다                               |
| 화면: 팝오버 메뉴 "진단하기" → 코칭 화면(선택한 에이전트 탭 기준, 개요면 전체). 제안 줄 = 이름·"~N 토큰"·Codex면 "5시간 한도 약 X%"(0008 `windowShare`, 추론). 적용 버튼 → 바꿀 줄 미리보기 → 확인. 적용 뒤 백업 경로·되돌리기 버튼·`cp` 명령 복사. "다른 스위치"는 접어 둔다 | 0009·0011 틀(한눈 값 → 한 줄 설명 → 자세히)                         |
| 적용·되돌리기 뒤 컨텍스트 캐시를 다시 계산한다(기존 refresh 경로)                                                                                                                                                                                                             | 화면 합계가 바로 바뀐다                                             |
| 에이전트는 실행 중이면 다시 시작해야 반영된다 — 적용 뒤 한 줄로 알린다                                                                                                                                                                                                        | 설정은 시작 때 읽는다                                               |

- 2026-11-07: "진단하기"는 팝오버에서 대시보드 "점검" 탭(에이전트 칸 오른쪽)으로 옮겼다. 동작은 그대로다(0017 "화면 자리").

## IPC

| command          | 입력                              | 출력                                                                                   |
| ---------------- | --------------------------------- | -------------------------------------------------------------------------------------- |
| `get_switches`   | 없음                              | `CoachSwitch[]`(target·agent·kind·name·enabled·tokens·loading·format·file·instruction) |
| `preview_switch` | `target`, `enabled`               | `SwitchPreview`(파일·바꿀 줄 전/후·`beforeHash`·changed)                               |
| `apply_switch`   | `target`, `enabled`, `beforeHash` | `AppliedSwitch`(기록 ID·백업·되돌리기 `cp` 명령)                                       |
| `undo_switch`    | 기록 ID                           | `AppliedSwitch`                                                                        |
| `get_coach_log`  | 없음                              | `AppliedSwitch[]`(최근순)                                                              |

구현하며 정한 것(2026-11-05):

- `CoachSwitch.isProposed`: 제안 판정(켜짐·`Startup`·토큰 > 0)은 Rust가 한다. 목록은 제안 먼저 토큰 큰 순.
- `SwitchPreview`는 `location`(section·키 경로와 줄 번호)·`before`(새 줄이면 null)·`after`를 준다. TOML은 `enabled` 값만 바꾸고 들여쓰기·줄 끝 주석·CRLF를 유지한다. 값이 true/false가 아니면 거부.
- `AppliedSwitch`: `id`·`at`·`action`(apply/undo)·`target`·`agent`·`name`·`file`·`backup`·`enabled`·`tokens`·`afterHash`·`undoCommand`·`undoOf`. 되돌리기도 한 줄을 남기고(`undoOf` = 적용 기록 ID), 같은 기록은 두 번 되돌리지 않는다. 되돌리기는 기록의 `file`이 kcal이 쓰는 세 파일 중 하나일 때만 한다.
- 백업 이름의 시각은 `YYYYMMDDTHHMMSSmmmZ`. Windows의 되돌리기 명령은 `copy /Y`.
- Claude MCP 목록 = `~/.claude.json` user scope 서버 + 마지막 `api-request-shape`에 실린 서버(플러그인·커넥터 포함). 토큰은 shape 실측. shape가 있는데 거기 없는 서버는 `deferred`로 본다.
- 해시는 FNV-1a 64(hex 16자리). 변경 감지용이고 보안용이 아니다.
- `serde_json`에 `preserve_order` feature를 켰다: JSON을 다시 쓸 때 키 순서를 지키려고. `indexmap`(2.14.2)·`foldhash`는 이미 Cargo.lock에 있어 새 crate가 아니다.
- 스위치 목록은 `context::scan`의 Codex section 읽기를 그대로 쓴다. 그 읽기는 여러 줄 배열 안의 `[`로 시작하는 줄을 헤더로 본다(편집 쪽은 무시). 그런 파일에서는 목록의 켜짐 표시가 틀릴 수 있고, 편집은 실제 section 기준으로 한다.

## 열린 것

- 크기 모르는 MCP 측정(서버 실행)은 하지 않는다. 필요해지면 별도 결정.
- Windows 경로(`%USERPROFILE%\.codex` 등)·권한 유지는 실기 확인 전.
- 6층 전후 비교는 이 기록을 읽어 다음 작업에서 만든다.
