//! claude.ai connectors are discovered by the CLI; kcal never writes their settings.
use std::path::Path;

use super::switches::{CoachSwitch, SwitchKind, base, tool_server_name};
use crate::context::{Loading, fsutil, shape::ShapeSummary};
use crate::i18n::pick;
use crate::tuning::Tuning;
use crate::usage::{
    Agent,
    paths::claude_cli_candidates,
    sources::{RunInput, run},
};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Connector {
    name: String,
    status: &'static str,
}

/// Strict envelope and known statuses only. Changed output must not become off advice.
fn parse(text: &str) -> Option<Vec<Connector>> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    if !matches!(
        lines.next()?,
        "Checking MCP server health…" | "Checking MCP server health..."
    ) {
        return None;
    }
    let mut connectors = Vec::new();
    for line in lines {
        let (name, rest) = line.split_once(": ")?;
        let (endpoint, status) = rest.rsplit_once(" - ")?;
        if name.is_empty() || endpoint.is_empty() {
            return None;
        }
        // Local servers have more states ("⏸ Pending approval", "(HTTP)" suffix); only connectors are read.
        if let Some(display) = name.strip_prefix("claude.ai ") {
            let status = match status {
                "✓ Connected" | "✔ Connected" => "connected",
                "✗ Failed to connect" | "✘ Failed to connect" => "failed",
                // `!`: Claude Code 2.1.332 on Windows (seen on local lines, 2026-11-27).
                "⚠ Needs authentication" | "! Needs authentication" => "auth",
                _ => return None,
            };
            let url = reqwest::Url::parse(endpoint).ok()?;
            if display.trim().is_empty()
                || display != display.trim()
                || url.scheme() != "https"
                || url.host_str().is_none()
                || name.chars().any(char::is_control)
            {
                return None;
            }
            connectors.push(Connector {
                name: name.into(),
                status,
            });
        }
    }
    Some(connectors)
}

/// The CLI starts the user's MCP servers for health checks, so a popover focus must not rerun it.
/// Successful reads are reused for `contextTtlSecs` (connectors are part of the startup context).
// ponytail: two focuses inside the first ~6 s both run the CLI; add an in-flight guard if that shows up.
static CACHE: std::sync::Mutex<Option<(std::time::Instant, Vec<Connector>)>> =
    std::sync::Mutex::new(None);

pub(super) async fn read(home: &Path, tuning: &Tuning) -> Vec<Connector> {
    if let Some((at, cached)) = CACHE.lock().ok().and_then(|c| c.clone())
        && at.elapsed() < tuning.context_ttl()
    {
        return cached;
    }
    let connectors = read_cli(home, tuning).await;
    if let (Some(found), Ok(mut cache)) = (&connectors, CACHE.lock()) {
        *cache = Some((std::time::Instant::now(), found.clone()));
    }
    connectors.unwrap_or_default()
}

async fn read_cli(home: &Path, tuning: &Tuning) -> Option<Vec<Connector>> {
    let path_env = std::env::var_os("PATH");
    let Some(cli) = claude_cli_candidates(home, path_env.as_deref())
        .into_iter()
        .find(|p| p.is_file())
    else {
        eprintln!("kcal: could not read claude.ai connectors (CLI missing)");
        return None;
    };
    // Use the existing network health timeout and cancellable, windowless process runner.
    let output = run(
        &cli,
        &["mcp", "list"],
        tuning.http_timeout(),
        RunInput {
            cwd: Some(home),
            ..RunInput::default()
        },
    )
    .await;
    match output
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| parse(&s))
    {
        Some(connectors) => Some(connectors),
        None => {
            eprintln!(
                "kcal: could not read claude.ai connectors (CLI failed, timed out or output changed)"
            );
            None
        }
    }
}

fn instruction(connector: &Connector) -> String {
    let name = &connector.name;
    let denied = serde_json::json!({"serverName": name}).to_string();
    let status = match connector.status {
        "connected" => pick("연결됨", "Connected"),
        "failed" => pick("연결 실패", "Failed to connect"),
        _ => pick("인증 필요", "Needs authentication"),
    };
    crate::tr!(
        "{status} · claude.ai 구독 로그인에서 가져온 커넥터예요. kcal은 설정을 바꾸지 않아요.\n하나 끄기: 해당 프로젝트에서 Claude Code /mcp → {name} → 끄기. disabledMcpServers에 이 표시 이름으로 저장돼요. 또는 deniedMcpServers에 {denied}를 넣으세요.\n전부 끄기: Claude Code settings에 disableClaudeAiConnectors: true(어느 범위든 true가 이김), 또는 실행 환경에 ENABLE_CLAUDEAI_MCP_SERVERS=false를 두세요.\n데스크톱 앱 로컬·SSH 세션: 위 설정 대신 https://claude.ai/customize/connectors 에서 연결을 끊으세요.",
        "{status} · Fetched for a claude.ai subscription login. kcal does not change settings.\nTurn off one: in that project's Claude Code, open /mcp → {name} → disable. This saves the display name in disabledMcpServers. Or add {denied} to deniedMcpServers.\nTurn off all: set disableClaudeAiConnectors: true in Claude Code settings (true in any scope wins), or set ENABLE_CLAUDEAI_MCP_SERVERS=false in the launch environment.\nDesktop app local/SSH sessions: disconnect at https://claude.ai/customize/connectors instead."
    )
}

pub(super) fn append(
    all: &mut Vec<CoachSwitch>,
    connectors: Vec<Connector>,
    sent: Option<&ShapeSummary>,
    tuning: &Tuning,
) {
    for connector in connectors {
        // Replaces the shape-only row of the same server (kept as the fallback when the CLI fails).
        let server = tool_server_name(&connector.name);
        all.retain(|s| !(s.kind == SwitchKind::Mcp && s.file.is_none() && s.name == server));
        if all
            .iter()
            .any(|s| s.target == format!("claude/connector:{}", connector.name))
        {
            continue;
        }
        let mut switch = base(Agent::Claude, SwitchKind::Mcp, &connector.name, true);
        switch.target = format!("claude/connector:{}", connector.name);
        switch.instruction = Some(instruction(&connector));
        if let Some(server) = sent.and_then(|s| {
            s.mcp
                .iter()
                .find(|m| m.server == tool_server_name(&connector.name))
        }) {
            switch.tokens = Some(fsutil::estimate(server.bytes, tuning.bytes_per_token));
            switch.loading = Some(Loading::Startup);
        }
        all.push(switch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_connector_statuses_and_ignores_local_servers() {
        let connectors = parse(include_str!("fixtures/claude-mcp-list.txt")).unwrap();
        assert_eq!(
            connectors,
            vec![
                Connector {
                    name: "claude.ai Figma".into(),
                    status: "connected"
                },
                Connector {
                    name: "claude.ai Slack".into(),
                    status: "failed"
                },
                Connector {
                    name: "claude.ai Drive".into(),
                    status: "auth"
                },
            ]
        );
        let mut all = Vec::new();
        append(&mut all, connectors, None, &Tuning::default());
        assert!(
            all.iter()
                .all(|s| s.file.is_none() && s.plan("{}", false).is_err() && s.tokens.is_none())
        );
    }

    #[test]
    fn reads_bang_needs_authentication() {
        let text = "Checking MCP server health…\nclaude.ai Drive: https://example.com - ! Needs authentication\nplugin:x:y: npx y - ! Needs authentication";
        assert_eq!(
            parse(text).unwrap(),
            vec![Connector {
                name: "claude.ai Drive".into(),
                status: "auth"
            }]
        );
    }

    #[test]
    fn unknown_or_changed_output_produces_no_advice() {
        for input in [
            "garbage",
            "",
            "claude.ai Figma: https://example.com - ✓ Connected",
            "Checking MCP server health…\nclaude.ai Figma: https://example.com - new status",
            "Checking MCP server health…\nclaude.ai : https://example.com - ✓ Connected",
            "Checking MCP server health…\nclaude.ai Figma: command - ✓ Connected",
            "Checking MCP server health…\nclaude.ai Figma: https:// - ✓ Connected",
        ] {
            assert!(parse(input).is_none(), "{input}");
        }
    }
    #[test]
    fn connector_card_replaces_the_shape_only_fallback_row() {
        let connectors = parse(include_str!("fixtures/claude-mcp-list.txt")).unwrap();
        let mut all = vec![base(
            Agent::Claude,
            SwitchKind::Mcp,
            "claude_ai_Figma",
            true,
        )];
        append(&mut all, connectors, None, &Tuning::default());
        assert_eq!(all.len(), 3);
        assert!(
            all.iter()
                .all(|s| s.target.starts_with("claude/connector:"))
        );
    }

    #[test]
    fn connector_estimate_and_english_guide_use_the_verified_display_name() {
        crate::i18n::with_lang(crate::i18n::Lang::En, || {
            let connectors = parse(include_str!("fixtures/claude-mcp-list.txt")).unwrap();
            let sent = crate::context::shape::summarise(&serde_json::json!({"shape":{"tools":[
                {"name":"mcp__claude_ai_Figma__read", "description":"A tool"}
            ]}}));
            let mut all = Vec::new();
            append(&mut all, connectors, Some(&sent), &Tuning::default());
            assert!(all[0].tokens.is_some_and(|t| t > 0));
            assert!(all[1].tokens.is_none());
            for card in all {
                let text = card.instruction.unwrap();
                assert!(!text.chars().any(|c| ('\u{ac00}'..='\u{d7a3}').contains(&c)));
                for step in [
                    "/mcp",
                    "disabledMcpServers",
                    "deniedMcpServers",
                    "disableClaudeAiConnectors: true",
                    "ENABLE_CLAUDEAI_MCP_SERVERS=false",
                    "local/SSH",
                    "https://claude.ai/customize/connectors",
                ] {
                    assert!(text.contains(step), "{step}");
                }
            }
        });
    }
    #[cfg(unix)]
    #[test]
    fn cli_discovery_uses_temp_home_and_returns_no_cards_on_failure_or_timeout() {
        use crate::context::fsutil::testing::TempDir;
        use std::os::unix::fs::PermissionsExt;
        let home = TempDir::new("connector-cli");
        let cli = home.path().join(".local/bin/claude");
        let fixture = include_str!("fixtures/claude-mcp-list.txt");
        let script = format!(
            "#!/bin/sh\n[ \"$1 $2\" = \"mcp list\" ] || exit 9\n/bin/cat <<'SAMPLE'\n{fixture}SAMPLE\n"
        );
        home.write(".local/bin/claude", &script);
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let tuning = Tuning {
            http_timeout_secs: 1,
            ..Tuning::default()
        };
        let read = |home| tauri::async_runtime::block_on(read_cli(home, &tuning));
        assert_eq!(read(home.path()).map(|c| c.len()), Some(3));
        home.write(".local/bin/claude", "#!/bin/sh\nexit 1\n");
        assert!(read(home.path()).is_none());
        home.write(".local/bin/claude", "#!/bin/sh\nexec /bin/sleep 5\n");
        let start = std::time::Instant::now();
        assert!(read(home.path()).is_none());
        assert!(start.elapsed() < std::time::Duration::from_secs(4));
    }
}
