//! `ipc_fixtures.rs`의 코치 fixture 테스트(스위치·점검).

use super::*;

#[test]
fn coach_types_match_fixture() {
    let switch = CoachSwitch {
        target: "codex/plugin:synthetic@m".into(),
        agent: Agent::Codex,
        kind: SwitchKind::Plugin,
        name: "synthetic@m".into(),
        enabled: true,
        tokens: Some(900),
        loading: Some(Loading::Startup),
        is_proposed: true,
        usage: None,
        scope: None,
        scope_move: None,
        scope_guide: None,
        contents: None,
        skill: None,
        format: SwitchFormat::Toml,
        file: Some("/synthetic/.codex/config.toml".into()),
        instruction: None,
        field: Field::None,
    };
    let manual = CoachSwitch {
        target: "claude/mcp:synthetic".into(),
        agent: Agent::Claude,
        kind: SwitchKind::Mcp,
        name: "synthetic".into(),
        tokens: None,
        loading: None,
        is_proposed: false,
        format: SwitchFormat::Manual,
        file: None,
        instruction: Some("synthetic instruction".into()),
        ..switch.clone()
    };
    let json = CoachSwitch {
        target: "kiro/mcp:synthetic".into(),
        agent: Agent::Kiro,
        enabled: false,
        format: SwitchFormat::Json,
        ..manual.clone()
    };
    let usage = SwitchUsage {
        days: 30,
        total: 15,
        projects: vec![
            ProjectUsage {
                name: "alpha".into(),
                path: "/synthetic/alpha".into(),
                count: 12,
                exists: true,
            },
            ProjectUsage {
                name: "gone".into(),
                path: "/synthetic/gone".into(),
                count: 3,
                exists: false,
            },
        ],
        project_count: 2,
        is_partial: false,
        sessions: 24,
        min_sessions: 10,
        scope_min_uses: 3,
        last_used: Some(1_793_000_000),
        last_project: Some("alpha".into()),
    };
    let claude_plugin = CoachSwitch {
        target: "claude/plugin:synthetic@m".into(),
        agent: Agent::Claude,
        format: SwitchFormat::Json,
        file: Some("/synthetic/.claude/settings.json".into()),
        usage: Some(usage),
        scope: Some(SwitchScope {
            user: true,
            projects: vec![
                ProjectScope {
                    name: "beta".into(),
                    path: "/synthetic/beta".into(),
                    file: ScopeFile::Local,
                    enabled: false,
                },
                ProjectScope {
                    name: "gamma".into(),
                    path: "/synthetic/gamma".into(),
                    file: ScopeFile::Project,
                    enabled: true,
                },
            ],
        }),
        scope_move: Some(ScopeMove {
            projects: vec![ProjectRef {
                name: "alpha".into(),
                path: "/synthetic/alpha".into(),
            }],
            skipped: vec!["gone".into()],
        }),
        contents: Some(PluginContents {
            skills: NameList {
                names: vec![
                    "alpha-skill".into(),
                    "beta-skill".into(),
                    "gamma-skill".into(),
                ],
                count: 5,
            },
            agents: NameList {
                names: vec!["helper".into()],
                count: 1,
            },
            commands: NameList::default(),
            mcp_servers: NameList {
                names: vec!["synthetic-server".into()],
                count: 1,
            },
        }),
        is_proposed: false,
        ..switch.clone()
    };
    let skill = CoachSwitch {
        target: "claude/skill:synthetic-skill".into(),
        kind: SwitchKind::Skill,
        name: "synthetic-skill".into(),
        tokens: None,
        loading: None,
        is_proposed: true,
        skill: Some(SkillInfo {
            state: None,
            project: None,
        }),
        file: Some("/synthetic/.claude/settings.json".into()),
        scope: None,
        scope_move: None,
        scope_guide: None,
        contents: None,
        ..claude_plugin.clone()
    };
    let project_skill = CoachSwitch {
        target: "claude/project-skill:synthetic-local".into(),
        kind: SwitchKind::ProjectSkill,
        name: "synthetic-local".into(),
        enabled: false,
        is_proposed: false,
        skill: Some(SkillInfo {
            state: Some("name-only".into()),
            project: Some("alpha".into()),
        }),
        format: SwitchFormat::Manual,
        file: None,
        instruction: Some("synthetic instruction".into()),
        usage: None,
        scope: None,
        scope_move: None,
        scope_guide: None,
        contents: None,
        ..skill.clone()
    };
    check(
        "coach-switches",
        &[switch, manual, json, claude_plugin, skill, project_skill],
    );
    check(
        "scope-move-preview",
        &ScopeMovePreview {
            target: "claude/plugin:synthetic@m".into(),
            changes: vec![
                ScopeFileChange {
                    file: "/synthetic/alpha/.claude/settings.local.json".into(),
                    location: "enabledPlugins.\"synthetic@m\"".into(),
                    before: None,
                    after: "true".into(),
                    is_new_file: true,
                },
                ScopeFileChange {
                    file: "/synthetic/.claude/settings.json".into(),
                    location: "enabledPlugins.\"synthetic@m\"".into(),
                    before: Some("\"synthetic@m\": true".into()),
                    after: "\"synthetic@m\": false".into(),
                    is_new_file: false,
                },
            ],
            skipped: vec!["gone".into()],
            before_hash: "cbf29ce484222325".into(),
        },
    );
    check(
        "switch-preview",
        &SwitchPreview {
            target: "codex/plugin:synthetic@m".into(),
            enabled: false,
            file: "/synthetic/.codex/config.toml".into(),
            location: "[plugins.\"synthetic@m\"] 안 2번째 줄".into(),
            before: Some("enabled = true".into()),
            after: "enabled = false".into(),
            before_hash: "cbf29ce484222325".into(),
            changed: true,
        },
    );
    let applied = AppliedSwitch {
        id: "1762344000000000000".into(),
        at: datetime!(2026-11-05 12:00 UTC),
        action: CoachAction::Apply,
        target: "codex/plugin:synthetic@m".into(),
        agent: Agent::Codex,
        name: "synthetic@m".into(),
        file: "/synthetic/.codex/config.toml".into(),
        backup: "/synthetic/.codex/config.toml.kcal-20261105T120000000Z.bak".into(),
        enabled: false,
        tokens: Some(900),
        after_hash: "af63dc4c8601ec8c".into(),
        undo_command: "cp 'a' 'b'".into(),
        undo_of: None,
        created: None,
        files: Vec::new(),
    };
    let undone = AppliedSwitch {
        id: "1762344060000000000".into(),
        action: CoachAction::Undo,
        enabled: true,
        tokens: None,
        undo_of: Some(applied.id.clone()),
        ..applied.clone()
    };
    let moved = AppliedSwitch {
        id: "1762344120000000000".into(),
        target: "claude/plugin-scope:synthetic@m".into(),
        agent: Agent::Claude,
        name: "synthetic@m: on only in alpha".into(),
        file: "/synthetic/.claude/settings.json".into(),
        backup: "/synthetic/.claude/settings.json.kcal-20261105T120200000Z.bak".into(),
        undo_command: "cp 'a' 'b' && rm 'c' && rmdir 'd'".into(),
        files: vec![LoggedFile {
            file: "/synthetic/alpha/.claude/settings.local.json".into(),
            backup: None,
            after_hash: "af63dc4c8601ec8c".into(),
            created_dir: true,
        }],
        ..applied.clone()
    };
    check("coach-log", &[moved, undone, applied]);
}

#[test]
fn coach_checks_match_fixture() {
    use crate::coach::checks::models::{Effort, ModelCount};
    use crate::coach::checks::rules_paths::{RuleFact, RulesScope};
    use crate::coach::checks::{
        CheckFacts, CheckKind, CheckOption, CheckPreview, CoachCheck, LineMatch, ModelVerdict,
        SettingsScope,
    };
    use crate::coach::edit::JsonChange;
    let change = JsonChange {
        location: "effortLevel".into(),
        before: Some("\"high\"".into()),
        after: None,
    };
    let option = |id: &str, is_writable: bool| CheckOption {
        id: id.into(),
        is_recommended: is_writable,
        is_writable,
        instruction: (!is_writable).then(|| "synthetic instruction".into()),
        changes: if is_writable {
            vec![change.clone()]
        } else {
            vec![]
        },
        log_name: "synthetic".into(),
        ops: vec![],
    };
    let verdict = ModelVerdict {
        model: "claude-opus-5-5".into(),
        requests: 9,
        own_effort: Some(Effort::Xhigh),
        default_effort: Some(Effort::Medium),
        is_ignored: true,
    };
    let unknown = vec![ModelCount {
        model: "synthetic-model".into(),
        requests: 1,
    }];
    let card = |id: &str, kind, facts, tokens| CoachCheck {
        id: id.into(),
        kind,
        agent: Agent::Claude,
        facts,
        options: vec![option("remove", true), option("keep", false)],
        file: "/synthetic/.claude/settings.json".into(),
        docs: vec!["https://code.claude.com/docs/en/settings-reference#effortlevel".into()],
        tokens,
        is_dismissed: false,
    };
    let ids = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let checks = [
        card(
            "D-claude-effort-level",
            CheckKind::D,
            CheckFacts::EffortLevel {
                value: Effort::High,
                days: 30,
                models: vec![verdict.clone()],
                ignored_by: ids(&["claude-opus-5-5"]),
                moved_to: vec![],
                current: "claude-opus-5-5".into(),
                unknown_models: unknown.clone(),
            },
            None,
        ),
        card(
            "D-claude-always-thinking",
            CheckKind::D,
            CheckFacts::AlwaysThinking {
                days: 30,
                models: vec![verdict.clone()],
                ignored_by: ids(&["claude-opus-5-5"]),
                affected: vec![],
                unknown_models: vec![],
            },
            None,
        ),
        card(
            "D-claude-max-thinking-tokens",
            CheckKind::D,
            CheckFacts::MaxThinkingTokens {
                value: 8000,
                days: 30,
                models: vec![verdict],
                ignored_by: ids(&["claude-opus-5-5"]),
                affected: vec![],
                unknown_models: vec![],
            },
            None,
        ),
        card(
            "C-claude-git-instructions",
            CheckKind::C,
            CheckFacts::GitInstructions {
                matches: vec![LineMatch {
                    file: "/synthetic/.claude/rules/git.md".into(),
                    line: 3,
                    text: "## Commit Message Format".into(),
                }],
                has_own_trailer: false,
                pre_commit_skills: ids(&["verify"]),
            },
            None,
        ),
        card(
            "B-claude-agents-md-hook:0",
            CheckKind::B,
            CheckFacts::AgentsMdHook {
                settings_file: "/synthetic/w/.claude/settings.json".into(),
                scope: SettingsScope::Project,
                command: "cat AGENTS.md".into(),
                agents_md: "/synthetic/w/AGENTS.md".into(),
                workspace: "/synthetic/w".into(),
            },
            Some(1200),
        ),
        card(
            "B-claude-same-skill:synthetic",
            CheckKind::B,
            CheckFacts::SameSkill {
                name: "synthetic".into(),
                personal_path: "/synthetic/.claude/skills/synthetic/SKILL.md".into(),
                plugin_id: "synthetic@m".into(),
                plugin_path:
                    "/synthetic/.claude/plugins/cache/m/synthetic/1/skills/synthetic/SKILL.md"
                        .into(),
                is_same_content: true,
            },
            Some(40),
        ),
        card(
            "B-claude-long-instructions:/synthetic/w/CLAUDE.md",
            CheckKind::B,
            CheckFacts::LongInstructions {
                name: "CLAUDE.md".into(),
                scope: RulesScope::Project,
                folder: "/synthetic/w".into(),
                lines: 340,
                limit: 200,
                imports: ids(&["AGENTS.md"]),
            },
            None,
        ),
        card(
            "A2-claude-rules-paths:/synthetic/.claude/rules",
            CheckKind::A,
            CheckFacts::RulesPaths {
                scope: RulesScope::User,
                folder: "/synthetic".into(),
                chars: 3100,
                limit: 40_000,
                rules: vec![RuleFact {
                    name: "common/testing.md".into(),
                    chars: 3100,
                    has_front_matter: false,
                    referenced_by: 1,
                    file: "/synthetic/.claude/rules/common/testing.md".into(),
                }],
            },
            None,
        ),
        CoachCheck {
            agent: Agent::Codex,
            file: "/synthetic/.codex/config.toml".into(),
            ..card(
                "D-codex-fast-mode",
                CheckKind::D,
                CheckFacts::CodexFastMode {
                    model: Some("gpt-6.2-astra".into()),
                    multiplier: Some(2.0),
                },
                None,
            )
        },
    ];
    let mut checks = checks.to_vec();
    for (topic, kind, agent) in [
        ("reasoning-effort", CheckKind::D, Agent::Codex),
        ("instructions-size", CheckKind::A, Agent::Codex),
        ("duplicate-hooks", CheckKind::B, Agent::Codex),
        ("tool-search-v3", CheckKind::A, Agent::Kiro),
        ("tool-search-v2", CheckKind::A, Agent::Kiro),
        ("auto-compaction", CheckKind::D, Agent::Kiro),
    ] {
        checks.push(CoachCheck {
            id: format!("synthetic-{topic}"),
            kind,
            agent,
            facts: CheckFacts::SavedAdvice {
                topic: topic.into(),
                evidence: vec!["synthetic setting".into()],
            },
            options: vec![option("guide", false), option("keep", false)],
            file: "/synthetic/settings".into(),
            docs: vec![],
            tokens: None,
            is_dismissed: false,
        });
    }
    check("coach-checks", &checks);
    check(
        "check-preview",
        &CheckPreview {
            id: "D-claude-effort-level".into(),
            option: "remove".into(),
            file: "/synthetic/.claude/settings.json".into(),
            changes: vec![
                change,
                JsonChange {
                    location: "modelSettings.\"claude-opus-5\".effortLevel".into(),
                    before: None,
                    after: Some("\"high\"".into()),
                },
            ],
            before_hash: "cbf29ce484222325".into(),
        },
    );
}
