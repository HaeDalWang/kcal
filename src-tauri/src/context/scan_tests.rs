//! `scan.rs` 테스트.

use super::*;
use crate::context::ContextSource;
use crate::context::fsutil::testing::TempDir;

fn find<'a>(context: &'a AgentContext, name: &str) -> &'a ContextItem {
    context
        .items
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("{name} not in {:?}", context.items))
}

#[test]
fn claude_prefers_measured_shape_over_mcp_config() {
    let home = TempDir::new("scan-claude-shape");
    home.write(
        ".claude.json",
        r#"{"mcpServers": {"docs": {"command": "x"}}}"#,
    );
    home.write(".claude/settings.json", r#"{"hooks": {"Stop": [{}, {}]}}"#);
    let without = claude(home.path(), None, &HashMap::new(), &Tuning::default());
    let docs = find(&without, "docs");
    assert_eq!((docs.tokens, docs.loading), (None, Loading::Deferred));
    assert_eq!(find(&without, "hook 2개").loading, Loading::Conditional);

    let tools = r#"[{"name":"mcp__docs__read","input_schema":{}}]"#;
    let line = format!(
        r#"{{"type":"api-request-shape","shape":{{"system":[{{"text":"12345678"}}],"tools":{tools}}}}}"#
    );
    home.write(".claude/projects/-w/s.jsonl", &line);
    let with = claude(home.path(), None, &HashMap::new(), &Tuning::default());
    let system = find(&with, "시스템 프롬프트 (블록 1개)");
    assert_eq!(
        (system.tokens, system.source),
        (Some(2), ContextSource::Measured)
    );
    assert_eq!(
        find(&with, "docs (도구 1개)").source,
        ContextSource::Measured
    );
    assert!(with.items.iter().all(|i| i.name != "docs"));
}

#[test]
fn claude_plugin_skills_include_folders_a_marketplace_entry_lists() {
    let home = TempDir::new("scan-claude-market-skills");
    home.write(
        ".claude/settings.json",
        r#"{"enabledPlugins": {"kit@m": true}}"#,
    );
    home.write(
            ".claude/plugins/marketplaces/m/.claude-plugin/marketplace.json",
            r#"{"plugins": [{"name": "kit", "strict": false, "skills": ["./alpha", "./skills/beta"]}]}"#,
        );
    let skill = "---\nname: a\n---\nbody";
    for rel in ["alpha", "skills/beta", "skills/gamma"] {
        home.write(
            &format!(".claude/plugins/cache/m/kit/1.0.0/{rel}/SKILL.md"),
            skill,
        );
    }
    let got = claude(home.path(), None, &HashMap::new(), &Tuning::default());
    let kit = got
        .items
        .iter()
        .find(|i| i.kind == ContextKind::Plugins && i.name.starts_with("kit@m"))
        .expect("kit item");
    // alpha + skills/ 아래 beta·gamma, beta는 두 번 세지 않는다. front matter 16바이트 × 3.
    assert_eq!(kit.bytes, Some(48));
}

#[test]
fn claude_without_any_session_says_nothing_was_measured_not_that_it_is_too_old() {
    let home = TempDir::new("scan-claude-fresh");
    home.write(".claude/settings.json", "{}");
    let fresh = claude(home.path(), None, &HashMap::new(), &Tuning::default());
    let is_old =
        |w: &String| w.contains("needs Claude Code 2.1.300+") || w.contains("2.1.300 이상 필요");
    let is_unmeasured = |w: &String| {
        w.contains("No Claude session found yet") || w.contains("아직 Claude 세션이 없어서")
    };
    assert!(
        fresh.warnings.iter().any(is_unmeasured),
        "{:?}",
        fresh.warnings
    );
    assert!(!fresh.warnings.iter().any(is_old));
    // 세션이 있는데 shape 줄이 없으면(옛 Claude Code) 경고한다.
    home.write(".claude/projects/-w/s.jsonl", r#"{"type":"user"}"#);
    let old = claude(home.path(), None, &HashMap::new(), &Tuning::default());
    assert!(old.warnings.iter().any(is_old));
    assert!(!old.warnings.iter().any(is_unmeasured));
}

#[test]
fn claude_groups_startup_attachments_like_context_command() {
    let home = TempDir::new("scan-claude-attach");
    let work = TempDir::new("scan-claude-attach-work");
    home.write(".claude/CLAUDE.md", "file estimate only");
    home.write(
        ".claude/settings.json",
        r#"{"enabledPlugins": {"tools@market": true},
               "modelSettings": {"claude-opus-5-5": {"autoCompactWindow": 400000}}}"#,
    );
    let ws = work.path().to_string_lossy().into_owned();
    let line = |v: serde_json::Value| v.to_string();
    let attach =
        |a: serde_json::Value| line(serde_json::json!({"type": "attachment", "attachment": a}));
    let text = [
            line(serde_json::json!({"type": "user", "cwd": ws})),
            attach(serde_json::json!({"type": "instructions", "files": [
                {"path": "/h/.claude/CLAUDE.md", "type": "User", "content": "12345678"},
                {"path": "/h/.claude/rules/r.md", "type": "User", "content": "1234"}]})),
            attach(serde_json::json!({"type": "skill_listing", "skillCount": 3,
                "names": ["mine", "tools:a", "anthropic-skills:pdf"],
                "content": "- mine: x\n- tools:a: y\n- anthropic-skills:pdf: z"})),
            attach(serde_json::json!({"type": "agent_listing_delta", "addedTypes": ["Explore", "mine"],
                "addedLines": ["built in", "12345678"], "builtInTypes": ["Explore"], "builtInPluginTypes": []})),
            attach(serde_json::json!({"type": "mcp_instructions_delta", "addedNames": ["docs"], "addedBlocks": ["12345678"]})),
            // 두 번째 첨부(세션 중간)는 무시한다.
            attach(serde_json::json!({"type": "mcp_instructions_delta", "addedNames": ["late"], "addedBlocks": ["x"]})),
            line(serde_json::json!({"type": "api-request-shape", "shape": {"system": [{"text": "1234"}],
                "tools": [{"name": "Bash"}, {"name": "ToolSearch", "defer_loading": true}]}})),
            line(serde_json::json!({"type": "api-request", "params": {"model": "claude-opus-5-5"}})),
        ]
        .join("\n");
    let project = shape::claude_project_dir(home.path(), work.path());
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("s.jsonl"), &text).unwrap();

    let got = claude(
        home.path(),
        Some(work.path()),
        &HashMap::new(),
        &Tuning::default(),
    );
    let kind_of = |name: &str| (find(&got, name).kind, find(&got, name).source);
    assert_eq!(
        kind_of("CLAUDE.md"),
        (ContextKind::Instructions, ContextSource::Measured)
    );
    assert_eq!(find(&got, "CLAUDE.md").tokens, Some(2));
    assert_eq!(
        kind_of("r.md"),
        (ContextKind::Rules, ContextSource::Measured)
    );
    assert_eq!(find(&got, "tools@market (스킬 1개)").tokens, Some(3));
    assert_eq!(find(&got, "스킬 목록 (스킬 2개)").kind, ContextKind::Skills);
    assert_eq!(find(&got, "에이전트 목록 (1개)").tokens, Some(2));
    assert_eq!(
        kind_of("docs"),
        (ContextKind::McpInstructions, ContextSource::Measured)
    );
    assert!(got.items.iter().all(|i| i.name != "late"));
    assert_eq!(
        find(&got, "지연 로딩 도구 목록 (1개)").loading,
        Loading::Deferred
    );
    assert_eq!(find(&got, "내장 도구 (1개)").kind, ContextKind::Tools);
    assert_eq!(
        (got.model.as_deref(), got.context_window),
        (Some("claude-opus-5-5"), Some(400_000))
    );
    assert!(got.warnings.iter().all(|w| !w.contains("가장 최근")));

    // 이 폴더 세션이 없으면 가장 최근 세션으로 대신하고 경고한다. 첨부는 다른 프로젝트 것이라 쓰지 않는다.
    let other = TempDir::new("scan-claude-attach-other");
    let fallback = claude(
        home.path(),
        Some(other.path()),
        &HashMap::new(),
        &Tuning::default(),
    );
    assert!(
        fallback
            .warnings
            .iter()
            .any(|w| w.contains("세션 기록이 없어"))
    );
    assert_eq!(
        find(&fallback, "CLAUDE.md").source,
        ContextSource::Estimated
    );
    assert!(
        fallback
            .items
            .iter()
            .all(|i| i.kind != ContextKind::McpInstructions)
    );
    assert_eq!(
        find(&fallback, "내장 도구 (1개)").source,
        ContextSource::Measured
    );
}

#[test]
fn claude_mcp_scopes_honour_approval_and_switches() {
    let home = TempDir::new("scan-claude-mcp");
    let work = TempDir::new("scan-claude-mcp-work");
    let key = work.path().to_string_lossy().replace('\\', "\\\\");
    home.write(
            ".claude.json",
            &format!(
                r#"{{"mcpServers": {{"user": {{}}, "off": {{}}}},
                "projects": {{"{key}": {{"enabledMcpjsonServers": ["approved"], "disabledMcpServers": ["off"],
                "mcpServers": {{"local": {{"alwaysLoad": true}}}}}}}}}}"#
            ),
        );
    work.write(
        ".mcp.json",
        r#"{"mcpServers": {"approved": {}, "pending": {}}}"#,
    );
    let mut servers = claude_mcp_servers(home.path(), Some(work.path()));
    servers.sort();
    assert_eq!(
        servers,
        [
            ("approved".to_owned(), false),
            ("local".to_owned(), true),
            ("user".to_owned(), false)
        ]
    );
}

#[test]
fn claude_mcp_reads_parent_folders_and_bypass_prompt_skip() {
    let home = TempDir::new("scan-claude-mcp-parent");
    let work = home.path().join("work/app");
    home.write(
        "work/.mcp.json",
        r#"{"mcpServers": {"near": {}, "far": {}, "blocked": {}}}"#,
    );
    home.write(
        ".mcp.json",
        r#"{"mcpServers": {"far": {"alwaysLoad": true}}}"#,
    );
    home.write("work/app/.mcp.json", r#"{"mcpServers": {"here": {}}}"#);
    let key = work.to_string_lossy().replace('\\', "\\\\");
    home.write(
        ".claude.json",
        &format!(
            r#"{{"projects": {{"{key}": {{"enabledMcpjsonServers": ["near", "far", "here"]}}}}}}"#
        ),
    );
    let mut servers = claude_mcp_servers(home.path(), Some(&work));
    servers.sort();
    // 가까운 폴더(work)의 far가 먼 폴더(홈)의 far를 덮는다. 승인 안 된 blocked는 빠진다.
    assert_eq!(
        servers,
        [
            ("far".to_owned(), false),
            ("here".to_owned(), false),
            ("near".to_owned(), false)
        ]
    );

    // 승인 창을 건너뛰는 설정이면 승인 기록이 없어도 실린다. disabledMcpjsonServers는 그래도 막는다.
    home.write(
        ".claude/settings.json",
        r#"{"skipDangerousModePermissionPrompt": true, "disabledMcpjsonServers": ["near"]}"#,
    );
    home.write(".claude.json", "{}");
    let mut servers: Vec<String> = claude_mcp_servers(home.path(), Some(&work))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    servers.sort();
    assert_eq!(servers, ["blocked", "far", "here"]);
}

#[test]
fn codex_toml_readers_follow_opposite_defaults() {
    let config = r#"
[plugins."a@m"]
enabled = true # on
[plugins."b@m"]
[mcp_servers.one]
command = "x"
[mcp_servers.one.env]
enabled = false
[mcp_servers."two.dot"]
enabled = false
[mcp_servers.three]
"#;
    assert_eq!(codex_enabled_plugins(config), ["a@m"]);
    assert_eq!(codex_enabled_mcp_servers(config), ["one", "three"]);
}

#[test]
fn codex_toml_readers_ignore_multi_line_array_rows_that_look_like_headers() {
    let config = r#"
[mcp_servers.x]
args = [
  ["a", "b"],
  ["c"],
]
enabled = false
[plugins."y@m"]
list = [
  [1],
]
enabled = true
"#;
    assert!(codex_enabled_mcp_servers(config).is_empty());
    assert_eq!(codex_enabled_plugins(config), ["y@m"]);
    let states = codex_mcp_server_states(config);
    assert_eq!(states.len(), 1);
    assert!(!states[0].enabled);
}

#[test]
fn codex_measured_system_item_and_first_request_tokens() {
    let home = TempDir::new("scan-codex-session");
    home.write(".codex/AGENTS.md", "global");
    let meta = r#"{"type":"session_meta","payload":{"cwd":"/w/app","base_instructions":{"text":"12345678"}}}"#;
    let count = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":123}}}}"#;
    home.write(
        ".codex/sessions/2026/11/04/rollout-a.jsonl",
        &format!("{meta}\n{count}"),
    );
    let got = codex(home.path(), None, &Tuning::default());
    let system = find(&got, "시스템 지시문");
    assert_eq!(
        (system.tokens, system.source, system.loading),
        (Some(2), ContextSource::Measured, Loading::Startup)
    );
    assert_eq!(got.first_request_tokens, Some(123));
    assert!(
        got.warnings
            .iter()
            .any(|w| w.contains("가장 최근 Codex 세션(작업 폴더 app)"))
    );
    // 작업 폴더를 주면 출처 경고는 없다.
    let scoped = codex(home.path(), Some(Path::new("/w/app")), &Tuning::default());
    assert!(scoped.warnings.iter().all(|w| !w.contains("가장 최근")));
    assert_eq!(scoped.first_request_tokens, Some(123));
}

#[test]
fn kiro_first_request_chars_and_session_note() {
    let home = TempDir::new("scan-kiro-session");
    let start = r#"{"payload":{"type":"session_start","content":"abc","steeringDocuments":[{"content":"de"}],"composition":{"contentChars":3}}}"#;
    home.write(
        ".kiro/sessions/h/sess_a/session.json",
        r#"{"workspacePaths":["/w/app"]}"#,
    );
    home.write(".kiro/sessions/h/sess_a/messages.jsonl", start);
    let got = kiro(home.path(), None, &Tuning::default());
    assert_eq!(got.first_request_chars, Some(5));
    assert!(
        got.warnings
            .iter()
            .any(|w| w.contains("가장 최근 Kiro 세션(작업 폴더 app)"))
    );
    let scoped = kiro(home.path(), Some(Path::new("/w/app")), &Tuning::default());
    assert_eq!(scoped.first_request_chars, Some(5));
    assert!(scoped.warnings.iter().all(|w| !w.contains("가장 최근")));
    assert_eq!(
        kiro(home.path(), Some(Path::new("/w/x")), &Tuning::default()).first_request_chars,
        None
    );
}

fn skill(dir: &TempDir, path: &str, name: Option<&str>) {
    let front = name.map_or_else(String::new, |n| format!("---\nname: {n}\n---\n"));
    dir.write(&format!("{path}/SKILL.md"), &format!("{front}body"));
}

/// 항목은 토큰 순으로 정렬되므로 이름순으로 돌려준다.
fn names_of(context: &AgentContext) -> Vec<&str> {
    let mut names: Vec<&str> = context
        .items
        .iter()
        .filter(|i| i.kind == ContextKind::Skills)
        .map(|i| i.name.as_str())
        .collect();
    names.sort_unstable();
    names
}

#[test]
fn codex_counts_skills_from_every_location_once_per_name() {
    let home = TempDir::new("scan-codex-skills");
    let repo = home.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    skill(&home, "repo/.agents/skills/root-skill", None);
    skill(&home, "repo/.agents/skills/shadcn", None);
    skill(&home, "repo/sub/.agents/skills/sub-skill", None);
    skill(&home, ".agents/skills/web-browse", None);
    skill(&home, ".agents/skills/shadcn", None);
    skill(
        &home,
        ".agents/skills/orca-computer-use",
        Some("orca-computer-use"),
    );
    skill(&home, ".codex/skills/shadcn", None);
    skill(&home, ".codex/skills/.system/imagegen", None);
    #[cfg(unix)]
    {
        let skills = home.path().join(".agents/skills");
        std::os::unix::fs::symlink("orca-computer-use", skills.join("computer-use")).unwrap();
        std::os::unix::fs::symlink("nowhere", skills.join("broken")).unwrap();
    }
    let got = codex(home.path(), Some(&repo.join("sub")), &Tuning::default());
    assert_eq!(
        names_of(&got),
        [
            "home-agent-skills (스킬 2개)",
            "repo-skills (스킬 3개)",
            "user-and-system-skills (스킬 1개)"
        ]
    );
    // 작업 폴더가 없으면 저장소 위치는 빠진다.
    let global = codex(home.path(), None, &Tuning::default());
    assert_eq!(
        names_of(&global),
        [
            "home-agent-skills (스킬 3개)",
            "user-and-system-skills (스킬 1개)"
        ]
    );
}

#[test]
fn kiro_counts_a_skill_in_both_workspace_and_global_once_in_workspace() {
    let home = TempDir::new("scan-kiro-skills");
    let work = TempDir::new("scan-kiro-skills-work");
    skill(&work, ".kiro/skills/shared", None);
    skill(&home, ".kiro/skills/shared", Some("shared"));
    skill(&home, ".kiro/skills/mine", None);
    let got = kiro(home.path(), Some(work.path()), &Tuning::default());
    assert_eq!(
        names_of(&got),
        ["user-skills (스킬 1개)", "workspace-skills (스킬 1개)"]
    );
}

#[test]
fn kiro_mcp_loading_follows_cli_engine() {
    let home = TempDir::new("scan-kiro");
    home.write(
        ".kiro/settings/mcp.json",
        r#"{"mcpServers": {"on": {}, "off": {"disabled": true}}}"#,
    );
    home.write(".kiro/hooks/a.json", r#"{"hooks": [{}, {}, {}]}"#);
    let v3 = kiro(home.path(), None, &Tuning::default());
    assert_eq!(find(&v3, "on").loading, Loading::Deferred);
    assert!(v3.items.iter().all(|i| i.name != "off"));
    assert_eq!(find(&v3, "hook 3개").tokens, None);

    home.write(".kiro/settings/cli.json", r#"{"chat.agentEngine": "v2"}"#);
    assert_eq!(
        find(&kiro(home.path(), None, &Tuning::default()), "on").loading,
        Loading::Startup
    );
}

#[test]
fn global_session_note_names_session_folder_on_any_os() {
    assert!(
        global_session_note("Claude", Some("/Users/me/work/kcal/")).contains("작업 폴더 kcal)")
    );
    assert!(global_session_note("Claude", Some(r"C:\Users\me\app")).contains("작업 폴더 app)"));
    assert!(global_session_note("Claude", None).contains("작업 폴더 알 수 없음)"));
}
