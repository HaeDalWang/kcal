//! Kiro 범위 이동은 JSON 안내만 만든다. 파일 쓰기는 하지 않는다.
use super::{McpScopeGuide, masked, quote};
use crate::coach::{scope, switches::CoachSwitch};
use serde_json::{Value, json};
use std::path::Path;

pub(crate) fn attach(mut switch: CoachSwitch, home: &Path, config: &Value) -> CoachSwitch {
    let proposal = switch
        .usage
        .as_ref()
        .and_then(|u| scope::proposal(home, switch.enabled, u));
    if let Some(proposal) = proposal {
        switch.scope_guide = build(config, &switch.name, &proposal, home, cfg!(windows));
        if switch.scope_guide.is_some() {
            switch.scope_move = Some(proposal);
        }
    }
    switch
}

/// 읽지 못한 기존 파일도 덮어쓸 근거가 없으므로 안내를 생략한다.
fn available(path: &Path, name: &str) -> Option<()> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let value: Value = serde_json::from_slice(&bytes).ok()?;
            let servers = value.as_object()?.get("mcpServers");
            (!servers.is_some_and(|s| !s.is_object() || s.get(name).is_some())).then_some(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(()),
        Err(_) => None,
    }
}

fn build(
    config: &Value,
    name: &str,
    proposal: &scope::ScopeMove,
    home: &Path,
    windows: bool,
) -> Option<McpScopeGuide> {
    let server = config.get("mcpServers")?.get(name)?.as_object()?;
    let mut server = masked(&Value::Object(server.clone()), false);
    server
        .as_object_mut()?
        .insert("disabled".into(), Value::Bool(false));
    let json = serde_json::to_string_pretty(&json!({"mcpServers": {name: server}})).ok()?;
    let mut commands = Vec::new();
    let user = quote(
        &home.join(".kiro/settings/mcp.json").to_string_lossy(),
        windows,
    );
    let name_json = serde_json::to_string(name).ok()?;
    let mut undo_commands = vec![crate::tr!(
        "{user}의 mcpServers[{name_json}]에서 disabled를 지우거나 false로 바꿔요.",
        "In {user}, remove disabled or set it to false in mcpServers[{name_json}]."
    )];
    for project in &proposal.projects {
        let path = Path::new(&project.path).join(".kiro/settings/mcp.json");
        available(&path, name)?;
        let file = quote(&path.to_string_lossy(), windows);
        commands.push(crate::tr!(
            "이 JSON을 {file}에 붙여 넣어요. 기존 mcpServers의 다른 항목은 유지하고 이 서버만 추가해요.",
            "Paste this JSON into {file}. Keep other mcpServers entries and add only this server."
        ));
        commands.push(json.clone());
        undo_commands.push(crate::tr!(
            "{file}의 mcpServers에서 {name_json} 항목만 삭제해요.",
            "In {file}, delete only the {name_json} entry from mcpServers."
        ));
    }
    commands.push(crate::tr!(
        "프로젝트 설정을 모두 확인한 다음 {user}의 mcpServers[{name_json}]에 \"disabled\": true를 넣어요.",
        "After checking every project setting, set \"disabled\": true in mcpServers[{name_json}] in {user}."
    ));
    Some(McpScopeGuide {
        commands,
        undo_commands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::scope::{ProjectRef, ScopeMove};
    use crate::context::fsutil::testing::TempDir;

    #[test]
    fn kiro_guide_masks_secrets_preserves_full_entry_and_quotes_both_shells() {
        let home = TempDir::new("kiro-guide");
        let config = json!({"mcpServers":{"docs'한":{"command":"node","args":["tool.js"],"env":{"KEY":"env-secret"},"headers":{"Auth":"header-secret"},"token":"token-secret","url":"https://example.test?key=query-secret","disabled":true,"autoApprove":["read"]}}});
        for count in 1..=3 {
            let proposal = ScopeMove {
                projects: (0..count)
                    .map(|i| ProjectRef {
                        name: format!("p{i}"),
                        path: home
                            .path()
                            .join(format!("app's {i}"))
                            .to_string_lossy()
                            .into(),
                    })
                    .collect(),
                skipped: vec![],
            };
            for windows in [false, true] {
                let guide = build(&config, "docs'한", &proposal, home.path(), windows).unwrap();
                let text = format!("{:?}{:?}", guide.commands, guide.undo_commands);
                for secret in [
                    "env-secret",
                    "header-secret",
                    "token-secret",
                    "query-secret",
                ] {
                    assert!(!text.contains(secret));
                }
                assert!(guide.commands[1].contains("\"disabled\": false"));
                assert!(guide.commands[1].contains("autoApprove"));
                let copied: Value = serde_json::from_str(&guide.commands[1]).unwrap();
                assert_eq!(copied["mcpServers"]["docs'한"]["disabled"], false);
                assert!(guide.commands[0].contains(if windows { "app''s" } else { "app'\"'\"'s" }));
                assert!(
                    guide.commands[0].contains(&quote(
                        &Path::new(&proposal.projects[0].path)
                            .join(".kiro/settings/mcp.json")
                            .to_string_lossy(),
                        windows
                    ))
                );
                assert_eq!(guide.commands.len(), count * 2 + 1);
                assert!(
                    guide
                        .commands
                        .last()
                        .unwrap()
                        .contains("\"disabled\": true")
                );
            }
        }
    }

    #[test]
    fn kiro_guide_skips_existing_server_and_unreadable_json() {
        let home = TempDir::new("kiro-guide-collision");
        let config = json!({"mcpServers":{"docs'한":{"command":"node"}}});
        let proposal = ScopeMove {
            projects: vec![ProjectRef {
                name: "app".into(),
                path: home.path().join("app").to_string_lossy().into(),
            }],
            skipped: vec![],
        };
        home.write(
            "app/.kiro/settings/mcp.json",
            "{\"mcpServers\":{\"docs'한\":{\"command\":\"other\"}}}",
        );
        assert!(build(&config, "docs'한", &proposal, home.path(), false).is_none());
        home.write("app/.kiro/settings/mcp.json", "broken");
        assert!(build(&config, "docs'한", &proposal, home.path(), false).is_none());
    }
}
