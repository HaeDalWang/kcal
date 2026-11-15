//! IPC 계약 fixture. Rust가 프론트엔드로 보내는 타입의 대표 값을 직렬화해 `src/test/fixtures/ipc/*.json`과 비교한다.
//! TS 쪽 짝은 `src/lib/ipc.test.ts`(같은 파일을 `ipc.ts` 타입의 키·enum 값과 맞춰 본다).
//! 필드나 enum 값을 바꾸면 여기서 실패한다. 의도한 변경이면 `UPDATE_IPC_FIXTURES=1 cargo test ipc_fixtures`로 다시 쓰고
//! (키 정렬), `pnpm exec prettier --write src/test/fixtures/ipc`로 줄바꿈을 맞춘 뒤 `ipc.ts`와 `pnpm test`를 같이 고친다. fixture 값은 모두 합성이다.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use time::macros::datetime;

use crate::coach::SwitchPreview;
use crate::coach::contents::{NameList, PluginContents};
use crate::coach::log::{AppliedSwitch, CoachAction, LoggedFile};
use crate::coach::scope::{
    ProjectRef, ProjectScope, ScopeFile, ScopeFileChange, ScopeMove, ScopeMovePreview, SwitchScope,
};
use crate::coach::switches::{CoachSwitch, Field, SwitchFormat, SwitchKind};
use crate::coach::usage::{ProjectUsage, SwitchUsage};
use crate::context::{
    AgentContext, ContextItem, ContextKind, ContextSnapshot, ContextSource, Loading,
};
use crate::error::AppError;
use crate::health::AgentSamples;
use crate::metrics::{AgentMetrics, BarUnit, HourBucket, MetricsSnapshot, UsageBar};
use crate::rate::{
    Bounds, CodexRate, CodexSession, KiroModelCredits, RateSnapshot, RateStatus, Spread,
    TokenRange, TokenRate, WindowComposition, WindowPattern,
};
use crate::settings::Settings;
use crate::usage::{
    Agent, AgentUsage, LimitWindow, ReasonKind, UsageSnapshot, UsageSource, WindowId,
};

fn check(name: &str, value: &impl Serialize) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../src/test/fixtures/ipc")
        .join(format!("{name}.json"));
    let actual = serde_json::to_value(value).unwrap();
    if std::env::var_os("UPDATE_IPC_FIXTURES").is_some_and(|v| v == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // preserve_order(0014)면 구조체 필드 순이 된다. fixture는 키 정렬로 두어 다시 써도 diff가 바뀐 값만 남게 한다.
        let mut sorted = actual;
        sorted.sort_all_objects();
        let text = serde_json::to_string_pretty(&sorted).unwrap() + "\n";
        std::fs::write(&path, text).unwrap();
        return;
    }
    let expected: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| {
            panic!(
                "{} 없음/깨짐. UPDATE_IPC_FIXTURES=1로 만든다",
                path.display()
            )
        });
    assert_eq!(
        actual, expected,
        "{name}: Rust IPC 타입이 fixture와 다르다. 의도한 변경이면 UPDATE_IPC_FIXTURES=1로 다시 쓰고 ipc.ts를 맞춘다"
    );
}

fn window(id: WindowId, used_percent: f64, reset_passed: bool) -> LimitWindow {
    LimitWindow {
        id,
        used_percent,
        resets_at: Some(datetime!(2026-11-04 15:00 UTC)),
        reset_passed,
        used: None,
        limit: None,
    }
}

#[test]
fn usage_snapshot_matches_fixture() {
    let at = Some(datetime!(2026-11-04 12:00 UTC));
    let claude = AgentUsage {
        agent: Agent::Claude,
        source: Some(UsageSource::Api),
        observed_at: at,
        windows: vec![
            window(WindowId::FiveHour, 12.5, false),
            window(WindowId::SevenDay, 0.0, true),
        ],
        reason: None,
        reason_kind: None,
        checked_at: at,
    };
    let codex = AgentUsage {
        agent: Agent::Codex,
        source: Some(UsageSource::Log),
        windows: vec![LimitWindow {
            resets_at: None,
            ..window(WindowId::FiveHour, 3.0, false)
        }],
        reason: Some("synthetic reason".into()),
        reason_kind: Some(ReasonKind::Failure),
        ..claude.clone()
    };
    let kiro = AgentUsage {
        agent: Agent::Kiro,
        source: Some(UsageSource::Cli),
        windows: vec![LimitWindow {
            used: Some(100.5),
            limit: Some(1000.0),
            ..window(WindowId::Month, 10.05, false)
        }],
        reason: Some("synthetic setup".into()),
        reason_kind: Some(ReasonKind::Setup),
        ..claude.clone()
    };
    let pending = AgentUsage::pending(Agent::Claude);
    check(
        "usage-snapshot",
        &UsageSnapshot {
            agents: vec![claude, codex, kiro, pending],
        },
    );
}

#[test]
fn context_snapshot_matches_fixture() {
    let item = |kind, source, loading, bytes: Option<u64>, path: Option<&str>| ContextItem {
        kind,
        name: "synthetic".into(),
        path: path.map(str::to_owned),
        bytes,
        tokens: bytes.map(|b| b / 4),
        source,
        loading,
    };
    use ContextKind::*;
    use ContextSource::*;
    use Loading::*;
    let items = vec![
        item(
            Instructions,
            Measured,
            Startup,
            Some(400),
            Some("/synthetic/AGENTS.md"),
        ),
        item(Rules, Estimated, Startup, Some(40), None),
        item(Skills, Estimated, Conditional, Some(80), None),
        item(Agents, Estimated, Conditional, None, None),
        item(Hooks, Estimated, Deferred, None, None),
        item(Mcp, Estimated, Deferred, None, None),
        item(McpInstructions, Measured, Startup, Some(40), None),
        item(Plugins, Estimated, Deferred, None, None),
        item(System, Measured, Startup, Some(4000), None),
        item(Tools, Measured, Startup, Some(400), None),
    ];
    let agent = |agent| AgentContext {
        agent,
        items: items.clone(),
        total_startup_tokens: 1110,
        first_request_tokens: Some(2048),
        first_request_chars: None,
        model: Some("synthetic-model".into()),
        context_window: Some(200_000),
        warnings: vec!["synthetic warning".into()],
    };
    check(
        "context-snapshot",
        &ContextSnapshot {
            computed_at: datetime!(2026-11-04 12:00 UTC),
            agents: vec![
                agent(Agent::Claude),
                AgentContext {
                    first_request_tokens: None,
                    first_request_chars: Some(9000),
                    model: None,
                    context_window: None,
                    warnings: vec![],
                    ..agent(Agent::Kiro)
                },
                AgentContext {
                    items: vec![],
                    ..agent(Agent::Codex)
                },
            ],
        },
    );
}

#[test]
fn rate_snapshot_matches_fixture() {
    let bounds = |min, max| Bounds { min, max };
    let rate = CodexRate {
        status: RateStatus::Estimated,
        r2: Some(0.8),
        windows: 12,
        excluded_windows: 1,
        per_percent: Some(TokenRate {
            cached_input: Some(1000.0),
            uncached_input: Some(200.0),
            output: None,
        }),
        fold_range: Some(TokenRange {
            cached_input: bounds(Some(900.0), Some(1100.0)),
            uncached_input: bounds(Some(100.0), None),
            output: bounds(None, None),
        }),
        pattern: Some(WindowPattern {
            days: 14,
            windows: 5,
            requests: Spread {
                median: 139.0,
                p90: 213.5,
            },
            misses: Spread {
                median: 2.0,
                p90: 5.0,
            },
        }),
        composition: Some(WindowComposition {
            windows: 5,
            requests: 139,
            misses: 2,
            input_share: 40.5,
            output_share: 8.25,
            used_percent: 52.0,
        }),
        sessions: vec![CodexSession {
            name: "synthetic-project".into(),
            cwd: Some("/synthetic/synthetic-project".into()),
            started_at: datetime!(2026-11-04 09:00 UTC),
            requests: 240,
            avg_input_tokens: 120_000.5,
            misses: 3,
            share: 61.5,
            is_marathon: true,
            is_large_context: false,
        }],
    };
    check(
        "rate-snapshot",
        &RateSnapshot {
            computed_at: datetime!(2026-11-04 12:00 UTC),
            claude: CodexRate {
                composition: None,
                sessions: vec![],
                ..rate.clone()
            },
            codex: rate,
            kiro: vec![KiroModelCredits {
                model: "synthetic-model".into(),
                turns: 7,
                credits_per_turn: 0.25,
            }],
        },
    );
    check(
        "rate-statuses",
        &[
            RateStatus::Estimated,
            RateStatus::RangeOnly,
            RateStatus::Unreliable,
            RateStatus::Insufficient,
            RateStatus::NoData,
        ],
    );
}

#[test]
fn metrics_snapshot_matches_fixture() {
    let bar = |value, is_current| UsageBar {
        starts_at: datetime!(2026-11-04 10:00 UTC),
        value,
        is_current,
    };
    let hour = |requests, used| HourBucket {
        starts_at: datetime!(2026-11-04 11:00 UTC),
        requests,
        used,
    };
    check(
        "metrics-snapshot",
        &MetricsSnapshot {
            computed_at: datetime!(2026-11-04 12:00 UTC),
            agents: vec![
                AgentMetrics {
                    agent: Agent::Claude,
                    recent_requests: None,
                    cache_hit_ratio: None,
                    bar_unit: BarUnit::Percent,
                    bars: vec![],
                    hourly: vec![],
                },
                AgentMetrics {
                    agent: Agent::Codex,
                    recent_requests: Some(42),
                    cache_hit_ratio: Some(0.92),
                    bar_unit: BarUnit::Percent,
                    bars: vec![bar(31.0, false), bar(12.0, true)],
                    hourly: vec![hour(0, 0.0), hour(17, 1.5)],
                },
                AgentMetrics {
                    agent: Agent::Kiro,
                    recent_requests: Some(3),
                    cache_hit_ratio: None,
                    bar_unit: BarUnit::Credits,
                    bars: vec![bar(1.25, true)],
                    hourly: vec![hour(2, 0.25)],
                },
            ],
        },
    );
}

#[test]
fn health_report_matches_fixture() {
    let now = datetime!(2026-11-06 12:00 UTC);
    let day = 86_400.0;
    let t = crate::metrics::unix_secs(now) - 7.0 * day;
    let both = |ts: f64, v: f64| vec![(ts - day, v), (ts + day, v / 2.0)];
    let samples = [
        // Claude: 한도 기록이 전 구간 중간부터 → coverage < 1.
        AgentSamples {
            agent: Agent::Claude,
            requests: both(t, 120_000.0),
            requests_from: Some(f64::NEG_INFINITY),
            used: both(t, 7.0),
            used_from: Some(t - 3.5 * day),
            compactions: None,
        },
        AgentSamples {
            agent: Agent::Codex,
            requests: both(t, 80_000.0),
            requests_from: Some(f64::NEG_INFINITY),
            used: both(t, 14.0),
            used_from: Some(f64::NEG_INFINITY),
            compactions: Some(vec![t - day, t - day, t + day]),
        },
        // Kiro: 기록 없음 → 값은 모두 null.
        AgentSamples {
            agent: Agent::Kiro,
            requests: vec![],
            requests_from: None,
            used: vec![],
            used_from: None,
            compactions: None,
        },
    ];
    let apply = |id: &str, at| AppliedSwitch {
        id: id.into(),
        at,
        action: CoachAction::Apply,
        target: "codex/plugin:synthetic@m".into(),
        agent: Agent::Codex,
        name: "synthetic@m".into(),
        file: "/synthetic/.codex/config.toml".into(),
        backup: "/synthetic/.codex/config.toml.kcal-x.bak".into(),
        enabled: false,
        tokens: Some(900),
        after_hash: "af63dc4c8601ec8c".into(),
        undo_command: "cp 'a' 'b'".into(),
        undo_of: None,
        created: None,
        files: Vec::new(),
    };
    let undo = AppliedSwitch {
        id: "3".into(),
        at: datetime!(2026-11-05 00:00 UTC),
        action: CoachAction::Undo,
        undo_of: Some("2".into()),
        ..apply("3", now)
    };
    let log = [
        undo,
        apply("2", datetime!(2026-11-04 00:00 UTC)),
        apply("1", datetime!(2026-10-30 12:00 UTC)),
    ];
    check(
        "health-report",
        &crate::health::report(&samples, &log, now, 7),
    );
}

#[test]
fn settings_matches_fixture() {
    check(
        "settings",
        &Settings {
            enabled_agents: vec![Agent::Claude, Agent::Kiro],
            reason: Some("synthetic reason".into()),
            ..Settings::default()
        },
    );
}

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
        contents: None,
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
    check("coach-switches", &[switch, manual, json, claude_plugin]);
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
    ];
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

#[test]
fn claude_handoff_types_match_fixture() {
    use crate::coach::checks::handoff::{HandoffError, HandoffPreview};
    let preview = HandoffPreview {
        id: "B-claude-long-instructions:/synthetic/w/CLAUDE.md".into(),
        file: "/synthetic/w/CLAUDE.md".into(),
        before_hash: "cbf29ce484222325".into(),
        before_lines: 340,
        after_lines: 180,
        diff: crate::coach::checks::diff::lines("same\nold\n", "same\nnew\n"),
        removed: vec!["겹치는 커밋 규칙 두 줄을 합쳤어요".into()],
        missing_imports: vec!["docs/style.md".into()],
        can_apply: true,
    };
    check("handoff-preview", &preview);
    check(
        "handoff-errors",
        &[
            HandoffError::Busy,
            HandoffError::LimitUnknown,
            HandoffError::LimitTooLow {
                remaining: 12.0,
                min_remaining: 20,
            },
            HandoffError::CliMissing,
            HandoffError::OpusUnavailable,
            HandoffError::Timeout { secs: 300 },
            HandoffError::Failed {
                reason: "exit 1, api_error".into(),
            },
            HandoffError::Refused {
                message: "synthetic message".into(),
            },
            HandoffError::Internal,
        ],
    );
}

fn rule_proposal(
    key: &str,
    name: &str,
    proposal: crate::coach::checks::rules_handoff::Proposal,
) -> crate::coach::checks::rules_handoff::RuleProposal {
    crate::coach::checks::rules_handoff::RuleProposal {
        key: key.into(),
        name: name.into(),
        chars: 900,
        has_front_matter: false,
        referenced_by: 0,
        before_hash: "cbf29ce484222325".into(),
        proposal,
        reason: "합성 이유예요.".into(),
        skill_exists: false,
        can_apply: true,
    }
}

#[test]
fn claude_rules_preview_matches_fixture() {
    use crate::coach::checks::rules_handoff::{Proposal, RuleProblem, RuleProposal, RulesPreview};
    use crate::coach::checks::rules_paths::RulesScope;
    let paths = Proposal::Paths {
        paths: vec!["src/**/*.ts".into()],
    };
    let skill = Proposal::Skill {
        name: "synthetic-skill".into(),
        description: "Use when synthetic.".into(),
    };
    let unusable = Proposal::Unusable {
        problem: RuleProblem::SkillName,
    };
    let preview = RulesPreview {
        id: "A2-claude-rules-paths:/synthetic/.claude/rules".into(),
        scope: RulesScope::User,
        folder: "/synthetic".into(),
        rules: vec![
            rule_proposal("r1", "a.md", paths),
            rule_proposal("r2", "b.md", skill),
            RuleProposal {
                can_apply: false,
                referenced_by: 2,
                ..rule_proposal("r3", "c.md", Proposal::Keep)
            },
            RuleProposal {
                can_apply: false,
                reason: String::new(),
                ..rule_proposal("r4", "d.md", unusable)
            },
        ],
        unknown_keys: 1,
    };
    check("rules-preview", &preview);
    use RuleProblem::*;
    let problems = [
        Missing,
        Duplicate,
        Choice,
        SkillName,
        Description,
        PathsCount,
        PathText,
        PathAbsolute,
    ];
    check("rule-problems", &problems);
}

#[test]
fn rules_skill_move_record_matches_fixture() {
    let moved = AppliedSwitch {
        id: "1762344000000000000".into(),
        at: datetime!(2026-11-05 12:00 UTC),
        action: CoachAction::Apply,
        target: "claude/check:A2-claude-rules-paths:/synthetic/.claude/rules:skill".into(),
        agent: Agent::Claude,
        name: "b.md 스킬로 옮기기".into(),
        file: "/synthetic/.claude/rules/b.md".into(),
        backup: "/synthetic/.claude/rules/b.md.kcal-20261105T120000000Z.bak".into(),
        enabled: false,
        tokens: None,
        after_hash: "af63dc4c8601ec8c".into(),
        undo_command: "mv 'a' 'b' && rm 'c' && rmdir 'd'".into(),
        undo_of: None,
        created: Some("/synthetic/.claude/skills/synthetic-skill/SKILL.md".into()),
        files: Vec::new(),
    };
    check("rules-applied", &moved);
}

#[test]
fn app_errors_match_fixture() {
    check(
        "app-errors",
        &[
            AppError::Internal,
            AppError::InvalidInput("synthetic message".into()),
        ],
    );
}
