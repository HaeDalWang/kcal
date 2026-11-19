# AGENTS.md — 공통 개발 규칙 (원본)

이 파일이 모든 에이전트(Claude Code, Codex, Kiro 등)의 규칙 원본이다. `CLAUDE.md`는 이 파일을 참조만 한다.
규칙을 바꿀 때는 이 파일만 고친다.

## 프로젝트

Tauri 2 + React + TypeScript 데스크톱 앱 템플릿 (macOS, Windows).
- 프론트엔드: `src/` (Vite, React, Tailwind CSS v4, shadcn/ui, Lucide)
- 백엔드: `src-tauri/` (Rust)
- 결정 기록: `docs/decisions/`
- 명령은 `README.md`의 "개발 명령"을 따른다. 패키지 매니저는 pnpm만 쓴다.

## 작업 시작 전

1. `gh run list --limit 5`로 CI 실패부터 본다. 실패가 있으면 그것을 먼저 고친다(macOS 게이트는 Windows 전용 코드를 컴파일하지 않고, main push CI는 Windows만 돈다).
2. 기존 구조와 관련 문서(`docs/backlog.md`, `README.md`, `docs/decisions/`)를 읽는다. 다음 할 일은 `docs/backlog.md`에 있다.
   - UI를 바꾸면 `docs/design.md`(디자인 규칙 원본)를 먼저 읽고 따른다.
3. 관련 Skill(`.agents/skills/`)을 확인한다.
   - IPC·경계 설계: `tauri-architecture`, `tauri-frontend`
   - Rust command·state: `tauri-rust`
   - capabilities·CSP·plugin 추가: `tauri-security`
   - 테스트: `tauri-testing`
   - React 성능: `react-best-practices`
   - UI 컴포넌트: `shadcn`
4. 새 의존성은 기존 의존성·플랫폼 기능으로 안 될 때만 추가하고, 이유를 결정 기록에 남긴다.

## 협업 규칙

- 하나의 파일을 여러 에이전트가 동시에 수정하지 않는다.
- 작업마다 담당 에이전트와 파일 소유 범위(수정 가능한 경로)를 지정한다. 범위 밖 파일은 고치지 않고 보고한다.
- 다른 에이전트·사람의 미커밋 변경을 되돌리거나 덮어쓰지 않는다.
- 작업 종료 시 보고: 변경 파일 목록, 실행한 검증 명령과 exit 코드, 남은 문제.
- 기술적 의사결정(의존성 추가, 구조 변경, 보안 경계 변경)은 `docs/decisions/NNNN-제목.md`로 기록한다.
- 커밋·푸시·배포는 사람이 요청했을 때만 한다.

## 성능 원칙

- 유휴 상태에서 CPU를 쓰지 않는다. polling, `setInterval`, 상시 animation loop를 금지한다. 갱신은 이벤트(Tauri event, 사용자 입력)로 한다.
  - 예외: 한도 조회·정기 검진 타이머(Rust 쪽 한정). 범위는 `docs/decisions/0003-product-scope.md` "주기 측정".
- 메인 UI 스레드를 막지 않는다. 무거운 연산은 Rust `async` command(필요하면 `tauri::async_runtime::spawn_blocking`)에서 한다.
- React 리렌더링을 최소화한다. 상태는 쓰는 곳 가까이 두고, 전역 store 라이브러리를 기본으로 넣지 않는다.
- 불필요한 라이브러리, 런타임, 백그라운드 프로세스를 추가하지 않는다.
- 주기·캐시 TTL·타임아웃·탐색 범위처럼 사용자가 조절할 만한 값은 하드코딩하지 않고 settings의 tuning 묶음(기본값 + Rust 쪽 범위 검증)에 둔다. 프로토콜 마커·URL·파일명·이벤트 이름·수치 허용오차는 제외. (설정 패널 계획, 2026-11-04)

## IPC 규칙

- 모든 command는 Rust 쪽에서 입력을 검증하고 `Result<T, E>`(직렬화 가능한 오류 타입)를 반환한다.
- 프론트엔드는 `invoke`를 직접 흩뿌리지 않고 `src/lib/ipc.ts`의 타입이 명시된 래퍼만 호출한다.
- Rust 타입과 TS 타입이 짝을 이루도록 같은 이름을 쓰고, 계약 변경 시 양쪽과 테스트를 같은 작업에서 고친다.

## 보안 원칙

- `src-tauri/capabilities/`는 최소 권한이다. 쓰지 않는 permission을 넣지 않는다.
- CSP를 끄지 않는다(`"csp": null` 금지).
- 개발용 MCP(`tauri-plugin-mcp-bridge`)는 Cargo feature `mcp`와 `debug_assertions`가 함께 켜질 때만 컴파일·등록된다. 프로덕션 빌드에 포함되지 않아야 한다. 이 경계를 바꾸려면 결정 기록이 필요하다.
- 비밀값을 코드·설정에 넣지 않는다.

## 완료 조건 (변경 종류에 맞게)

- 프론트엔드: `pnpm typecheck && pnpm lint && pnpm format:check && pnpm test`
- Rust: `pnpm rust:check` (fmt check + clippy -D warnings + test)
- UI 변경: `docs/design.md` 규칙과 대조하고, dev 앱을 실행해 라이트·다크 모드에서 확인(가능하면 MCP 스크린샷).
