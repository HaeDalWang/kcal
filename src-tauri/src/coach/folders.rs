//! Folder-specific Diagnose rows. Global rows stay single; project IDs include full paths.
use std::path::Path;

use serde_json::Value;

use super::{skills, switches, usage::UsageIndex};
use crate::context::{Loading, fsutil, scan, shape};
use crate::tuning::Tuning;
use crate::usage::Agent;
use switches::{CoachSwitch, SwitchKind, base, claude_mcp_instruction, tool_server_name};

pub(super) fn project_switches(
    home: &Path,
    settings: &Value,
    usage: Option<&UsageIndex>,
    workspace: &Path,
    tuning: &Tuning,
) -> Vec<CoachSwitch> {
    let sent = shape::latest(home, Some(workspace), tuning.shape_sessions_to_search);
    let local_names = local_mcp_names(home, workspace);
    let mut all: Vec<_> = scan::claude_mcp_servers(home, Some(workspace))
        .into_iter()
        .filter(|(name, _)| local_names.contains(name))
        .map(|(name, _)| {
            let mut switch = base(Agent::Claude, SwitchKind::Mcp, &name, true);
            switch.instruction = Some(claude_mcp_instruction().into());
            if let Some(server) = sent
                .as_ref()
                .and_then(|s| s.mcp.iter().find(|m| m.server == tool_server_name(&name)))
            {
                switch.tokens = Some(fsutil::estimate(server.bytes, tuning.bytes_per_token));
                switch.loading = Some(Loading::Startup);
            }
            // Folder-specific absence is not evidence for all folders.
            switch
        })
        .collect();
    all.extend(
        skills::list(home, Some(settings), usage, Some(workspace))
            .into_iter()
            .filter(|s| s.kind == SwitchKind::ProjectSkill),
    );
    for switch in &mut all {
        let prefix = if switch.kind == SwitchKind::Mcp {
            "claude/mcp:"
        } else {
            skills::PROJECT_PREFIX
        };
        switch.target = format!("{prefix}{}", serde_json::json!([switch.name, workspace]));
        switch.project_path = Some(workspace.to_string_lossy().into_owned());
    }
    all
}

fn local_mcp_names(home: &Path, workspace: &Path) -> std::collections::HashSet<String> {
    let state = fsutil::read_json(&crate::context::paths::claude_state_file(home));
    let local = state
        .as_ref()
        .and_then(|s| {
            s.get("projects")?
                .get(workspace.to_string_lossy().as_ref())?
                .get("mcpServers")
        })
        .cloned();
    let project_configs: Vec<_> = workspace
        .ancestors()
        .filter_map(|p| fsutil::read_json(&p.join(".mcp.json")))
        .collect();
    local
        .iter()
        .chain(project_configs.iter().filter_map(|v| v.get("mcpServers")))
        .filter_map(Value::as_object)
        .flat_map(|m| m.keys().cloned())
        .collect()
}

pub(super) fn global_mcp(
    home: &Path,
    context: Option<&crate::context::AgentContext>,
    sent: Option<&shape::ShapeSummary>,
    project_rows: &[CoachSwitch],
    tuning: &Tuning,
) -> Vec<CoachSwitch> {
    let mut tokens: std::collections::HashMap<_, _> = sent
        .into_iter()
        .flat_map(|s| &s.mcp)
        .map(|m| {
            (
                m.server.clone(),
                fsutil::estimate(m.bytes, tuning.bytes_per_token),
            )
        })
        .collect();
    let mut all = configured_mcp(home, context, sent.is_some(), &mut tokens);
    for row in project_rows.iter().filter(|s| s.kind == SwitchKind::Mcp) {
        tokens.remove(&tool_server_name(&row.name));
    }
    let mut rest: Vec<_> = tokens
        .into_iter()
        .filter(|(name, _)| !name.starts_with("claude.ai ") && !all.iter().any(|s| s.name == *name))
        .collect();
    rest.sort();
    all.extend(rest.into_iter().map(|(name, tokens)| CoachSwitch {
        tokens: Some(tokens),
        loading: Some(Loading::Startup),
        ..manual(&name)
    }));
    all
}

fn configured_mcp(
    home: &Path,
    context: Option<&crate::context::AgentContext>,
    has_sent: bool,
    tokens: &mut std::collections::HashMap<String, u64>,
) -> Vec<CoachSwitch> {
    let mut configured = scan::claude_mcp_servers(home, None);
    configured.sort();
    let mut all = Vec::new();
    for (name, _) in configured {
        let mut switch = manual(&name);
        switch.tokens = tokens.remove(&tool_server_name(&name));
        switch.loading = if switch.tokens.is_some() {
            Some(Loading::Startup)
        } else if has_sent {
            Some(Loading::Deferred)
        } else {
            switches::measured(context, crate::context::ContextKind::Mcp, &name).1
        };
        all.push(switch);
    }
    all
}

fn manual(name: &str) -> CoachSwitch {
    CoachSwitch {
        instruction: Some(claude_mcp_instruction().into()),
        ..base(Agent::Claude, SwitchKind::Mcp, name, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;
    use std::fs::{File, FileTimes};
    use std::time::{Duration, SystemTime};

    #[test]
    fn recent_folders_are_distinct_newest_first_and_limited_by_tuning() {
        let home = TempDir::new("diagnose-folders");
        for (i, cwd) in ["old", "middle", "new", "new"].iter().enumerate() {
            home.write(
                &format!("work/{cwd}/.claude/skills/review/SKILL.md"),
                "review",
            );
            let line = json!({"type":"user", "cwd":home.path().join("work").join(cwd)});
            let file = home.path().join(format!(".claude/projects/p/{i}.jsonl"));
            home.write(&format!(".claude/projects/p/{i}.jsonl"), &line.to_string());
            File::options()
                .write(true)
                .open(file)
                .unwrap()
                .set_times(
                    FileTimes::new()
                        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(i as u64 + 1)),
                )
                .unwrap();
        }
        let mut tuning = Tuning {
            diagnose_recent_folders: 3,
            ..Tuning::default()
        };
        let recent = |t: &Tuning| {
            shape::recent_workspaces(
                home.path(),
                t.diagnose_usage_files,
                t.diagnose_recent_folders,
            )
        };
        assert_eq!(
            recent(&tuning),
            ["new", "middle", "old"].map(|p| home.path().join("work").join(p))
        );
        tuning.diagnose_recent_folders = 2;
        assert_eq!(recent(&tuning).len(), 2);
        let all = switches::list(home.path(), None, None, &tuning);
        let project: Vec<_> = all
            .iter()
            .filter(|s| s.kind == SwitchKind::ProjectSkill)
            .collect();
        assert_eq!(project.len(), 2);
        assert_ne!(project[0].target, project[1].target);
        assert!(project.iter().all(|s| s.project_path.is_some()));
    }

    #[test]
    fn same_named_project_mcp_rows_keep_folder_cost_and_manual_guidance() {
        let home = TempDir::new("diagnose-project-mcp");
        home.write(".claude.json", &json!({"mcpServers":{"global":{"command":"node"}},"projects": {
            home.path().join("work/a").to_string_lossy(): {"mcpServers":{"docs":{"command":"node"}}},
            home.path().join("work/b").to_string_lossy(): {"mcpServers":{"docs":{"command":"other"}}}
        }}).to_string());
        for folder in ["a", "b"] {
            home.write(&format!("work/{folder}/exists"), "");
            let workspace = home.path().join("work").join(folder);
            let file = shape::claude_project_dir(home.path(), &workspace).join("s.jsonl");
            let size = if folder == "a" { 100 } else { 200 };
            let text = format!(
                "{}\n{}",
                json!({"cwd":workspace}),
                json!({"type":"api-request-shape", "shape":{"tools":[{"name":"mcp__docs__read", "description":"x".repeat(size)}]}})
            );
            home.write(
                file.strip_prefix(home.path()).unwrap().to_str().unwrap(),
                &text,
            );
        }
        let all = switches::list(home.path(), None, None, &Tuning::default());
        let docs: Vec<_> = all.iter().filter(|s| s.name == "docs").collect();
        assert_eq!(docs.len(), 2);
        assert_ne!(docs[0].target, docs[1].target);
        assert_ne!(docs[0].tokens, docs[1].tokens);
        assert!(docs.iter().all(|s| s.loading == Some(Loading::Startup)));
        assert!(
            docs.iter()
                .all(|s| s.project_path.is_some() && s.file.is_none())
        );
        assert_eq!(all.iter().filter(|s| s.name == "global").count(), 1);
    }
}
