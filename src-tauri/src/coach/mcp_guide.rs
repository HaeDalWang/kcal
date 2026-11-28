//! Claude user MCP의 명령 안내만 만든다. 설정 파일은 쓰지 않는다(0022).
#[path = "mcp_guide_kiro.rs"]
mod kiro;
pub(crate) use kiro::attach as attach_kiro;

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

/// 이름에 이 낱말이 들어간 키·플래그의 값은 비밀값으로 본다(대소문자·`-`/`_` 무시).
const SECRET_WORDS: [&str; 7] = [
    "token",
    "secret",
    "password",
    "apikey",
    "authorization",
    "credential",
    "bearer",
];

fn is_secret_name(name: &str) -> bool {
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(name.as_str(), "env" | "headers" | "header" | "auth")
        || SECRET_WORDS.iter().any(|w| name.contains(w))
}

pub(super) fn masked(value: &Value, secret: bool) -> Value {
    let placeholder = pick("<직접 입력>", "<enter manually>");
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), masked(value, secret || is_secret_name(key))))
                .collect(),
        ),
        // `args`: `--api-key X`의 X, `--token=X`의 X, `Bearer …`·`Authorization: …`가 든 값을 가린다.
        Value::Array(values) => Value::Array(
            values
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let after_flag = i > 0
                        && values[i - 1]
                            .as_str()
                            .is_some_and(|f| f.starts_with('-') && is_secret_name(f));
                    match v.as_str() {
                        Some(text) if !secret && !after_flag => {
                            let lower = text.to_ascii_lowercase();
                            if lower.contains("bearer ") || lower.contains("authorization:") {
                                Value::String(placeholder.into())
                            } else if let Some((flag, _)) = text
                                .split_once('=')
                                .filter(|(f, _)| f.starts_with('-') && is_secret_name(f))
                            {
                                Value::String(format!("{flag}={placeholder}"))
                            } else {
                                masked(v, false)
                            }
                        }
                        _ => masked(v, secret || after_flag),
                    }
                })
                .collect(),
        ),
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
    // JSON은 명령줄이 아니라 stdin(`-`)으로 붙여 넣게 한다(2.1.327+): 채운 비밀값이 셸 기록·`ps`에 남지 않는다.
    // 명령 다음 줄은 `{`로 시작하는 JSON이고, 화면은 따로 복사하게 보인다.
    // Windows 콘솔 입력은 코드 페이지에 따라 ASCII 밖 글자가 깨질 수 있어 JSON `\uXXXX`로 바꾼다.
    let json = if windows { ascii_json(&json) } else { json };
    let add = |scope| {
        vec![
            format!("claude mcp add-json -s {scope} {name} -"),
            json.clone(),
        ]
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
    let local_add = |path: &str| {
        let mut steps = add("local");
        steps[0] = local(path, std::mem::take(&mut steps[0]));
        steps
    };
    let mut commands: Vec<_> = proposal
        .projects
        .iter()
        .flat_map(|p| local_add(&p.path))
        .collect();
    commands.push(remove("user"));
    let mut undo_commands = add("user");
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
    fn masks_secret_args_flags_and_bearer_values() {
        let input = json!({"command":"npx","args":["mcp-remote","https://h/sse","--header","Authorization: Bearer sk-1","--api-key","sk-2","--token=sk-3","Bearer sk-4","--port","8080"],"accessToken":"sk-5","clientSecret":"sk-6"});
        let out = masked(&input, false).to_string();
        for secret in ["sk-1", "sk-2", "sk-3", "sk-4", "sk-5", "sk-6"] {
            assert!(!out.contains(secret), "{secret} in {out}");
        }
        for kept in [
            "mcp-remote",
            "https://h/sse",
            "--api-key",
            "--token=",
            "8080",
            "npx",
        ] {
            assert!(out.contains(kept), "{kept} missing in {out}");
        }
    }

    #[test]
    fn adds_local_per_project_before_removing_user_and_undo_restores_user_first() {
        let guide =
            build_for_shell(Some(&json!({"command":"node"})), "docs", &proposal(), false).unwrap();
        // 프로젝트마다 [명령, stdin JSON] 두 줄 → remove user.
        assert_eq!(guide.commands.len(), 5);
        assert_eq!(
            guide.commands[0],
            "cd '/work/a'\"'\"'s app' && claude mcp add-json -s local 'docs' -"
        );
        assert_eq!(guide.commands[1], r#"{"command":"node"}"#);
        assert_eq!(
            guide.commands[2],
            "cd '/work/b' && claude mcp add-json -s local 'docs' -"
        );
        assert_eq!(guide.commands[3], r#"{"command":"node"}"#);
        assert_eq!(guide.commands[4], "claude mcp remove -s user 'docs'");
        assert_eq!(
            guide.undo_commands[..2],
            [
                "claude mcp add-json -s user 'docs' -".to_string(),
                r#"{"command":"node"}"#.to_string()
            ]
        );
        assert!(
            guide.undo_commands[2..]
                .iter()
                .all(|c| c.contains("remove -s local"))
        );
        assert_eq!(guide.undo_commands.len(), 4);
    }

    #[test]
    fn server_not_in_user_scope_gets_no_guide() {
        assert!(build(None, "connector", &proposal()).is_none());
    }

    #[test]
    fn posix_quotes_path_and_name_and_leaves_stdin_json_unquoted() {
        let guide =
            build_for_shell(Some(&json!({"args":["it's"]})), "it's", &proposal(), false).unwrap();
        assert!(guide.commands[0].ends_with("add-json -s local 'it'\"'\"'s' -"));
        assert_eq!(guide.commands[1], r#"{"args":["it's"]}"#);
        assert_eq!(quote("a'b", false), "'a'\"'\"'b'");
    }

    #[test]
    fn powershell_reads_ascii_json_from_stdin_and_stops_on_failed_location_change() {
        let guide = build_for_shell(
            Some(&json!({"args":["it's", "$env:TOKEN", "한"]})),
            "a'b",
            &proposal(),
            true,
        )
        .unwrap();
        assert_eq!(
            guide.commands[0],
            "Set-Location -LiteralPath '/work/a''s app' -ErrorAction Stop; claude mcp add-json -s local 'a''b' -"
        );
        assert_eq!(
            guide.commands[1],
            r#"{"args":["it's","$env:TOKEN","\ud55c"]}"#
        );
        assert!(
            guide
                .commands
                .iter()
                .all(|c| !c.contains(r#"{"args"#) || c.starts_with('{'))
        );
        let json: Value = serde_json::from_str(&ascii_json(r#"{"a":"한😀"}"#)).unwrap();
        assert_eq!(json["a"], "한😀");
    }
}
