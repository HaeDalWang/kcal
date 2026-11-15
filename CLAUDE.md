# CLAUDE.md

공통 규칙의 원본은 [AGENTS.md](AGENTS.md)다. 작업 전에 반드시 읽고 따른다.

@AGENTS.md

- Skills: `.claude/skills`, `.kiro/skills` → `.agents/skills` (symlink, 프로젝트 로컬). 출처는 `.agents/skills/SOURCES.md`.
- MCP: `.mcp.json`(Claude)·`.kiro/settings/mcp.json`(Kiro, 같은 내용)의 `tauri` 서버는 `pnpm tauri:dev:mcp`로 띄운 dev 앱에만 연결된다.
