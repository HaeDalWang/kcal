# 0030. Claude 세션 현황과 오래 대기한 세션 끄기

## Summary (English)

- Decision: show running Claude Code sessions (busy / waiting / idle) in the popover, and offer one confirmed button that stops idle sessions older than a threshold. macOS only for stopping; Windows shows status only. Claude only at first.
- Why: many idle interactive sessions hold 220–550 MB each (measured 2026-11-24: 18 of 21 idle, about 6 GB). `claude agents` hides interactive sessions; `claude agents --json` lists them.
- Safety: no automatic stopping. The button lists what it will stop and asks once. Excluded: non-idle sessions, sessions whose child processes use CPU at or above a threshold, and sessions inside user-listed protected folders. The PID is re-checked against the session id right before signalling.
- Loss: the conversation stays and resumes with `claude --resume`; text typed but not sent in the input box is lost. There is no undo.

- 상태: 결정(구현 전)
- 날짜: 2026-11-25
- 앞선 결정: [0003](0003-product-scope.md) 범위, [0007](0007-tuning.md) tuning, [0027](0027-agent-descriptor-and-picker.md) 에이전트 설명표

## 문제

대화형 세션을 여러 개 띄우면 무엇이 일하고 무엇이 입력을 기다리는지 안 보인다. 대기 세션은 CPU를 거의 안 쓰지만 MCP 자식 포함 메모리를 잡는다(이 Mac 2026-11-24 실측: 12시간 넘은 12개를 끄자 빈 메모리 약 0.2GB → 3.7GB).

## 사용자 결정 (2026-11-24, 2026-11-25)

- 자동 종료는 넣지 않는다. "N시간 넘은 N개 끄기" 확인 버튼만 둔다.
- 끄기는 macOS만, Windows는 보기만. 처음엔 Claude 세션만.
- 제외 규칙: `idle` 아닌 세션, 자식 프로세스 CPU가 기준 이상인 세션, "보호 폴더" 안 세션.

## 결정

- **읽기**: `claude agents --json`을 실행한다(기존 Claude CLI 찾기 경로, 인자 배열 Command). 필드(`pid`·`sessionId`·`cwd`·`status`·`startedAt`·`kind`)는 모두 선택으로 읽고, 형식이 바뀌어도 죽지 않고 "읽지 못함"을 보인다(JSON 안정성 보장은 문서에서 못 봤다, 모름).
- **갱신**: polling 하지 않는다. 팝오버를 열 때와 사용자가 누를 때만 읽는다(AGENTS.md 성능 원칙).
- **IDLE 시간**: `~/.claude/projects/*/<sessionId>.jsonl`의 마지막 `timestamp`. 파일 수정 시각은 대기 중에도 바뀌어 쓰지 않는다(확인됨, 2026-11-24). 파일·timestamp가 없으면 IDLE 모름 → 끄기 대상에서 뺀다.
- **보일 것**: 팝오버 Claude 탭에 `입력 대기 · 작업 중 · 대기` 개수, 펼치면 세션별 상태·IDLE·폴더. 입력 대기(`waiting`)가 맨 위.
- **끄기**: 버튼 문구에 대상 개수, 누르면 대상 목록과 "보내지 않은 입력은 사라지고 대화는 `claude --resume`으로 이어진다"를 보이고 한 번 더 확인한다. 보내기 직전에 `claude agents --json`을 다시 읽어 같은 `pid`가 같은 `sessionId`이고 여전히 `idle`일 때만 `/bin/kill -TERM <pid>`를 보낸다(PID 재사용 방지). 새 의존성 없음.
- **제외**: 자식 프로세스 CPU 합은 `ps -A -o pid=,ppid=,%cpu=`로 계산한다. 기준과 보호 폴더 목록은 tuning에 둔다.
- **tuning(0007)**: `sessionIdleHours`(기본 24, 1~720), `sessionChildCpuPercent`(기본 2.0, 0.1~100), `sessionProtectedFolders`(기본 빈 목록, 절대 경로, 하위 폴더 포함). 경로 비교는 `fsutil::is_inside`.
- **IPC**: `list_claude_sessions`(읽기), `stop_idle_claude_sessions`(macOS에서만 실행, 결과 = 끈 개수·건너뛴 이유). Windows에서는 끄기 command가 오류를 돌려주고 UI는 버튼을 숨긴다.

## 위험

- 되돌리기가 없는 첫 동작이다. "확인 뒤에만 바꾼다"(README)에 맞춰 확인 창 없이 끄는 길은 만들지 않는다.
- `status` 판정은 Claude Code가 한다. `idle`이어도 사용자가 입력창에 글을 쓰는 중일 수 있다(모름). 그래서 IDLE 기준 기본값을 길게(24h) 둔다.
- 다른 앱이 띄운 봇 세션은 kcal이 가리지 못한다. 보호 폴더로 막는다.

## 남은 것

- Codex·Kiro 세션 보기. Windows 끄기.
