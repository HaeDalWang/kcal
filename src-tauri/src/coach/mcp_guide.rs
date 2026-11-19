//! Claude user MCP의 명령 안내만 만든다. 설정 파일은 쓰지 않는다(0022).
use serde::Serialize;
use serde_json::Value;

use super::scope::ScopeMove;
use crate::i18n::pick;

pub fn claude_mcp_instruction() -> &'static str {
    pick(
        "Claude Code에서 /mcp를 열고 이 서버를 끄세요. kcal은 ~/.claude.json을 고치지 않아요(Claude가 실행 중에 다시 씁니다).",
        "Open /mcp in Claude Code and turn this server off. kcal doesn't edit ~/.claude.json (Claude rewrites it while running).",
    )
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpScopeGuide {
    pub commands: Vec<String>,
    pub undo_commands: Vec<String>,
}

fn quote(value: &str, windows: bool) -> String {
    let escaped = if windows {
        value.replace('\'', "''")
    } else {
        value.replace('\'', "'\"'\"'")
    };
    format!("'{escaped}'")
}

fn masked(value: &Value, secret: bool) -> Value {
    let placeholder = pick("<직접 입력>", "<enter manually>");
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| {
                    let is_secret = secret
                        || matches!(
                            key.to_ascii_lowercase().as_str(),
                            "env"
                                | "headers"
                                | "token"
                                | "apikey"
                                | "api_key"
                                | "secret"
                                | "client_secret"
                                | "password"
                                | "authorization"
                        );
                    (key.clone(), masked(value, is_secret))
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(|v| masked(v, secret)).collect()),
        _ if secret => Value::String(placeholder.into()),
        Value::String(text) => Value::String(match text.split_once('?') {
            Some((base, _)) => format!("{base}?{placeholder}"),
            None => text.clone(),
        }),
        _ => value.clone(),
    }
}

pub fn attach(
    mut switch: super::switches::CoachSwitch,
    home: &std::path::Path,
) -> super::switches::CoachSwitch {
    if switch.kind != super::switches::SwitchKind::Mcp {
        return switch;
    }
    let Some(proposal) = switch
        .usage
        .as_ref()
        .and_then(|used| super::scope::proposal(home, switch.enabled, used))
    else {
        return switch;
    };
    let state = crate::context::fsutil::read_json(&crate::context::paths::claude_state_file(home))
        .unwrap_or(Value::Null);
    switch.scope_guide = build(
        state.get("mcpServers").and_then(|s| s.get(&switch.name)),
        &switch.name,
        &proposal,
    );
    if switch.scope_guide.is_some() {
        switch.scope_move = Some(proposal);
    }
    switch
}

pub fn build(server: Option<&Value>, name: &str, proposal: &ScopeMove) -> Option<McpScopeGuide> {
    build_for_shell(server, name, proposal, cfg!(windows))
}

fn build_for_shell(
    server: Option<&Value>,
    name: &str,
    proposal: &ScopeMove,
    windows: bool,
) -> Option<McpScopeGuide> {
    let server = server.filter(|s| s.is_object())?;
    let name = quote(name, windows);
    let json = masked(server, false).to_string();
    // Windows PowerShell 5.1은 native 인자 속 `"`를 지우므로 JSON은 stdin(`-`)으로 넘긴다.
    // 5.1의 파이프는 ASCII로 보내므로 ASCII 밖 글자는 JSON `\uXXXX`로 바꾼다.
    let add = |scope| {
        if windows {
            format!(
                "{} | claude mcp add-json -s {scope} {name} -",
                quote(&ascii_json(&json), true)
            )
        } else {
            format!(
                "claude mcp add-json -s {scope} {name} {}",
                quote(&json, false)
            )
        }
    };
    let remove = |scope| format!("claude mcp remove -s {scope} {name}");
    let local = |path: &str, command: String| {
        let path = quote(path, windows);
        if windows {
            format!("Set-Location -LiteralPath {path} -ErrorAction Stop; {command}")
        } else {
            format!("cd {path} && {command}")
        }
    };
    let mut commands: Vec<_> = proposal
        .projects
        .iter()
        .map(|p| local(&p.path, add("local")))
        .collect();
    commands.push(remove("user"));
    let mut undo_commands = vec![add("user")];
    undo_commands.extend(
        proposal
            .projects
            .iter()
            .map(|p| local(&p.path, remove("local"))),
    );
    Some(McpScopeGuide {
        commands,
        undo_commands,
    })
}

/// JSON 문자열의 ASCII 밖 글자를 `\uXXXX`로 쓴다(JSON 문자열 안에서만 나오므로 뜻은 같다).
fn ascii_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::scope::ProjectRef;
    use super::*;
    use serde_json::json;

    fn proposal() -> ScopeMove {
        ScopeMove {
            projects: vec![
                ProjectRef {
                    name: "a".into(),
                    path: "/work/a's app".into(),
                },
                ProjectRef {
                    name: "b".into(),
                    path: "/work/b".into(),
                },
            ],
            skipped: vec![],
        }
    }

    #[test]
    fn only_user_scope_servers_with_complete_proposal_evidence_get_a_guide() {
        use super::super::{
            switches::{SwitchKind, base},
            usage::{ProjectUsage, SwitchUsage},
        };
        use crate::{context::fsutil::testing::TempDir, usage::Agent};
        let home = TempDir::new("mcp-guide-user-scope");
        home.write("work/app/exists", "");
        home.write(".claude.json", r#"{"mcpServers":{"docs":{"command":"node"}},"projects":{"elsewhere":{"mcpServers":{"connector":{}}}}}"#);
        let mut switch = base(Agent::Claude, SwitchKind::Mcp, "docs", true);
        switch.usage = Some(SwitchUsage {
            days: 30,
            total: 3,
            project_count: 1,
            is_partial: false,
            sessions: 10,
            min_sessions: 10,
            scope_min_uses: 3,
            last_used: None,
            last_project: None,
            projects: vec![ProjectUsage {
                name: "app".into(),
                path: home.path().join("work/app").to_string_lossy().into(),
                count: 3,
                exists: true,
            }],
        });
        let eligible = attach(switch.clone(), home.path());
        assert!(eligible.scope_guide.is_some() && eligible.scope_move.is_some());
        let mut absent = switch.clone();
        absent.name = "connector".into();
        let absent = attach(absent, home.path());
        assert!(absent.scope_guide.is_none() && absent.scope_move.is_none());
        for reason in [
            "partial",
            "few_sessions",
            "few_uses",
            "home",
            "many_projects",
            "disabled",
        ] {
            let mut denied = switch.clone();
            let used = denied.usage.as_mut().unwrap();
            match reason {
                "partial" => used.is_partial = true,
                "few_sessions" => used.sessions = 9,
                "few_uses" => used.total = 2,
                "home" => used.projects[0].path = home.path().to_string_lossy().into(),
                "many_projects" => used.project_count = 4,
                _ => denied.enabled = false,
            }
            let denied = attach(denied, home.path());
            assert!(
                denied.scope_guide.is_none() && denied.scope_move.is_none(),
                "{reason}"
            );
        }
    }

    #[test]
    fn masks_env_headers_and_url_queries_preserving_keys_and_other_fields() {
        let input = json!({"env":{"TOKEN":"env-secret", "PORT":42},"headers":{"Authorization":"header-secret"},"url":"https://example.test/mcp?token=query-secret", "command":"node", "args":["server.js"], "type":"http"});
        let output = masked(&input, false);
        let placeholder = pick("<직접 입력>", "<enter manually>");
        assert_eq!(output["env"]["TOKEN"], placeholder);
        assert_eq!(output["env"]["PORT"], placeholder);
        assert_eq!(output["headers"]["Authorization"], placeholder);
        assert_eq!(
            output["url"],
            format!("https://example.test/mcp?{placeholder}")
        );
        for key in ["command", "args", "type"] {
            assert_eq!(output[key], input[key]);
        }
        let guide = build(Some(&input), "docs", &proposal()).unwrap();
        for command in guide.commands.iter().chain(&guide.undo_commands) {
            for secret in ["env-secret", "header-secret", "query-secret"] {
                assert!(!command.contains(secret));
            }
        }
    }

    #[test]
    fn adds_local_per_project_before_removing_user_and_undo_restores_user_first() {
        let guide =
            build_for_shell(Some(&json!({"command":"node"})), "docs", &proposal(), false).unwrap();
        assert_eq!(guide.commands.len(), 3);
        assert!(guide.commands[0].starts_with("cd '/work/a"));
        assert!(guide.commands[1].starts_with("cd '/work/b'"));
        assert!(
            guide.commands[..2]
                .iter()
                .all(|c| c.contains("add-json -s local"))
        );
        assert_eq!(guide.commands[2], "claude mcp remove -s user 'docs'");
        assert!(guide.undo_commands[0].starts_with("claude mcp add-json -s user"));
        assert!(
            guide.undo_commands[1..]
                .iter()
                .all(|c| c.contains("remove -s local"))
        );
    }

    #[test]
    fn server_not_in_user_scope_gets_no_guide() {
        assert!(build(None, "connector", &proposal()).is_none());
    }

    #[test]
    fn posix_quoting_escapes_single_quote_in_json_path_and_name() {
        let guide =
            build_for_shell(Some(&json!({"args":["it's"]})), "it's", &proposal(), false).unwrap();
        assert!(guide.commands[0].contains("'it'\"'\"'s'"));
        assert!(guide.commands[0].contains("it'\"'\"'s\""));
        assert_eq!(quote("a'b", false), "'a'\"'\"'b'");
    }

    #[test]
    fn powershell_pipes_ascii_json_through_stdin_and_stops_on_failed_location_change() {
        let guide = build_for_shell(
            Some(&json!({"args":["it's", "$env:TOKEN", "한"]})),
            "a'b",
            &proposal(),
            true,
        )
        .unwrap();
        assert!(
            guide.commands[0]
                .starts_with("Set-Location -LiteralPath '/work/a''s app' -ErrorAction Stop; '")
        );
        assert!(guide.commands[0].ends_with("| claude mcp add-json -s local 'a''b' -"));
        assert!(guide.commands[0].contains("it''s") && guide.commands[0].contains("\\ud55c"));
        assert!(guide.commands[0].is_ascii());
        let json: Value = serde_json::from_str(&ascii_json(r#"{"a":"한😀"}"#)).unwrap();
        assert_eq!(json["a"], "한😀");
    }
}
