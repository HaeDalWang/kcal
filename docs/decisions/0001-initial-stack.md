# 0001. 초기 스택, Skills, MCP 선정

- 상태: 채택
- 날짜: 2026-10-28

## 버전 (2026-10-28 npm/crates 기준 확인)

| 항목 | 버전 | 비고 |
|---|---|---|
| Tauri CLI / @tauri-apps/api | 2.12.x | |
| React | 19.x | |
| Vite | 8.x | `@vitejs/plugin-react` 6.x (vite ^8 peer) |
| Tailwind CSS | 4.x | `@tailwindcss/vite` 플러그인 |
| shadcn CLI | 4.x | |
| TypeScript | **6.0.x** | 7.0이 최신이지만 typescript-eslint 8.72의 peer가 `typescript <6.1.0`. typescript-eslint가 7을 지원하면 올린다 |
| ESLint | 10.x | flat config |
| Vitest | 5.x | jsdom + Testing Library |
| Rust | 1.99.1 stable | rustup, profile minimal + clippy + rustfmt |
| pnpm | 12.15.0 | `packageManager` 필드로 고정 |

## Skills

`.agents/skills/SOURCES.md` 참고. 필수 범위 6개를 7개 Skill로 덮는다.
"데스크톱 UI 테스트"는 `tauri-testing`(단위·IPC 계약)과 MCP(실제 앱 조작)로 나눠 맡는다.

## MCP: hypothesi/mcp-server-tauri

후보 비교 (2026-10-28 GitHub 기준):

| 후보 | 라이선스 | 최근 push | 비고 |
|---|---|---|---|
| hypothesi/mcp-server-tauri (`@hypothesi/tauri-mcp-server` + `tauri-plugin-mcp-bridge`) | MIT | 2026-10-22 | 스크린샷, DOM 조작, IPC 실행·모니터링, 로그. Claude Code·Codex 지원 명시. **채택** |
| P3GLEG/tauri-plugin-mcp | 없음(라이선스 미표기) | 2026-10-14 | 라이선스가 없어 제외 |
| dirvine/tauri-mcp | MIT | 2025-07 | 1년 넘게 갱신 없음 |

보안 조치:
- bridge plugin은 기본 bind가 `0.0.0.0`(모든 인터페이스)이다 → `127.0.0.1`로 고정.
- Cargo optional feature `mcp` + `#[cfg(all(debug_assertions, feature = "mcp"))]`. release 빌드에는 crate 자체가 컴파일되지 않는다.
- bridge가 요구하는 `app.withGlobalTauri: true`와 bridge capability는 `src-tauri/tauri.mcp.conf.json` overlay에만 둔다. 기본 config에서는 꺼져 있다.
- MCP 서버 패키지 버전을 고정한다(`@hypothesi/tauri-mcp-server@0.13.2`). `npx` 미고정 실행은 공급망 위험이다.
- 설정 위치: Claude Code `.mcp.json`, Codex `.codex/config.toml`(레포 경로 자체가 trusted일 때만 읽음. 상위 디렉터리 trust는 상속되지 않음 — 2026-10-28 실측, README MCP 절 참고).
