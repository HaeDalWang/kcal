# 0031. Diagnose: claude.ai connectors and recent work folders

- 상태: 구현 (2026-11-25)
- 관련: 0007 tuning, 0017 E, 0022 사용 근거·범위, 0030 CLI 탐색

## Intent

Problem: Diagnose missed claude.ai connectors and project MCP/skills outside the latest session folder.
Outcome: one manual guide per discovered connector, and folder-labelled MCP/skill rows from the N most recent distinct session folders. A connector setting is never written by kcal.
Scope: existing Diagnose/settings UI and coach IPC. No dependency, polling, capability or CSP change. Existing plugin and usage-evidence policy stays in place.

## Rulings (2026-11-25)

- Ruling: one row per connector, in the existing manual-guide flow — each display name has its own `/mcp` action — cost if wrong: more rows under Keep as is.
- Ruling: `diagnoseRecentFolders` defaults to 5, accepts 1–20 — covers several active projects while bounding repeated project scans — cost if wrong: change the setting/default. Rust owns range/default; IPC, settings UI and both dictionaries expose it.
- Ruling: newest means transcript modification time, as in the existing shape scanner; inspect up to `diagnoseUsageFiles` transcripts, skipping duplicate/relative/missing cwd values, without requiring a request shape — reuses the existing session ordering and configurable scan bound — cost if wrong: a frequently updated session can rank ahead of a newer-created session; older folders beyond the scan bound are omitted.
- Ruling: JSON-encoded name/path pairs qualify project row IDs and appear in row text, title, evidence and manual panel — same-name skills/MCP in different folders must stay distinct and project skill writes must resolve the right file — cost if wrong: long paths truncate in compact rows (full title/panel remains).
- Ruling: keep user MCP once, include local/project overrides as folder rows, and remove their unscoped shape-only duplicates — preserve global behavior while exposing folder-specific configuration — cost if wrong: user/project overrides with the same name appear as separate scope rows.
- Ruling: project MCP cost uses only that folder's request shape; no absence-based off proposal is inferred for it — global usage counts cannot prove non-use in a folder — cost if wrong: fewer automatic proposals. Project skills retain their existing per-folder usage evidence.
- Ruling: discover connectors only on `get_switches` with Claude enabled, never on preview/apply; use existing `claude_cli_candidates` and asynchronous `usage::sources::run`, with `httpTimeoutSecs` — 0030 shares CLI discovery but its synchronous runner has no timeout; the existing async runner supplies cancellation, null stdin and windowless Windows execution — cost if wrong: API requests and connector health discovery share a timeout (default 15 seconds, range 1–300). Its settings label names both; the handoff-only timeout has a 60-second minimum and is too long for this health check.
- Ruling: run from HOME once, rather than executing project MCP commands in every recent folder — connectors are subscription-wide; local project data comes from existing file readers — cost if wrong: project-specific connector disables are not a live status inventory. The guide explicitly tells users to act in the intended project.
- Ruling: accept only the Claude Code 2.1.x text envelope and known status spellings on `claude.ai` lines (`✓`/`✔ Connected`, `✗`/`✘ Failed to connect`, `⚠ Needs authentication`); a changed connector line invalidates the result, while local server lines (`(HTTP)` suffix, `⏸ Pending approval`) are skipped without a status check — no wrong off advice from unknown output — cost if wrong: an innocuous CLI format change hides all connector rows until the parser fixture is updated. Missing CLI, nonzero exit, timeout and invalid UTF-8 log “could not read” without a card or crash. Endpoint/command output is never displayed or logged.
- Ruling: token estimate only from the matching normalized server name in the latest request shape; unknown stays unknown and connectors have no automatic off recommendation — do not infer a fixed 0.9k from one Figma example — cost if wrong: connectors absent from that request have no token estimate.
- Ruling: no local commit — the coordinator explicitly requested uncommitted changes — cost if wrong: coordinator commits the final diff.

## Off guide and source

[Official Claude Code MCP documentation](https://code.claude.com/docs/en/mcp#disable-claudeai-connectors), read 2026-11-25: subscription connectors fetched by Claude Code can be disabled per project in `/mcp` (display names in `disabledMcpServers`) or by name in `deniedMcpServers`. For all connectors, `disableClaudeAiConnectors: true` in any scope wins, or launch with `ENABLE_CLAUDEAI_MCP_SERVERS=false`. Desktop local/SSH sessions receive connectors from the app; disconnect at <https://claude.ai/customize/connectors>. The card gives both languages and these exceptions. kcal never edits these values.

The parser fixture uses the sample text supplied with task diag-4-6, with synthetic Slack/Drive/local rows to exercise failure and authentication states. It contains no real user configuration or credentials. A live run (Claude Code 2.1.332, 2026-11-25, about 6 seconds) printed `✔` and local `(HTTP)`/`⏸ Pending approval` lines; the fixture copies those formats with synthetic names and URLs.
