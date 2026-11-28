//! 켜고 끄는 스위치 목록(ctxmeter `switches.js`, 0014). 켜짐·꺼짐 둘 다 나온다.
//!
//! 설정 읽기는 `context::scan`의 함수를 그대로 쓴다. 토큰은 전역 컨텍스트 측정(캐시)의 항목에서 이름으로 맞추고,
//! Claude MCP는 실제로 보낸 요청(`shape`)에서 잰 값을 쓴다. 모르면 `None`(추측하지 않는다).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::contents::{self, PluginContents};
use super::edit::{self, Edit};
use super::mcp_guide::McpScopeGuide;
use super::scope::{self, ProjectSettings, ScopeMove, SwitchScope};
use super::skills::{self, SkillInfo};
use super::usage::{SwitchUsage, UsageIndex, codex_server_key, plugin_name};
use crate::context::{
    AgentContext, ContextKind, ContextSnapshot, Loading, fsutil, paths, scan, shape,
};
use crate::i18n::pick;
use crate::tuning::Tuning;
use crate::usage::Agent;

pub use super::mcp_guide::claude_mcp_instruction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SwitchKind {
    Plugin,
    Mcp,
    /// Claude 개인 스킬(`~/.claude/skills`). 설명을 숨기는 것이고 끄는 것이 아니다(0022 "스킬 단위").
    Skill,
    /// Claude 프로젝트 스킬(작업 폴더 `.claude/skills`).
    #[serde(rename = "projectSkill")]
    ProjectSkill,
}

/// 어떻게 바꾸나. `Manual`: kcal이 쓰지 않고 `instruction`만 보여 준다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SwitchFormat {
    Toml,
    Json,
    Manual,
}

/// 실제로 바꿀 값. 프론트로는 가지 않는다.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    /// Codex `config.toml` `[section]`의 `enabled`.
    TomlEnabled {
        section: String,
    },
    /// Kiro `mcp.json` `mcpServers.<name>.disabled`.
    JsonServerDisabled,
    /// Claude `settings.json` `enabledPlugins.<id>`.
    JsonPluginEnabled,
    None,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachSwitch {
    /// `codex/plugin:<id>`, `codex/mcp:<name>`, `kiro/mcp:<name>`, `claude/plugin:<id>`, `claude/mcp:<name>`.
    pub target: String,
    pub agent: Agent,
    pub kind: SwitchKind,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub enabled: bool,
    /// 시작 때 실리는 토큰(추정 또는 실측). 모르면 `None`.
    pub tokens: Option<u64>,
    /// 측정에서 본 로딩 시점. 측정에 없으면 `None`.
    pub loading: Option<Loading>,
    /// 끄기 제안 대상: 켜져 있고, 시작 때 실리고, 토큰을 알고, 최근 N일 0회(0014·0022).
    /// 목록은 제안 먼저 토큰 큰 순.
    pub is_proposed: bool,
    /// 최근 N일 사용 횟수(Claude·Codex·Kiro). 모르면 `None`.
    pub usage: Option<SwitchUsage>,
    /// 지금 켠 범위(Claude 플러그인만).
    pub scope: Option<SwitchScope>,
    /// "프로젝트에서만 켜기" 제안(Claude 플러그인·Codex MCP/플러그인, 0022).
    pub scope_move: Option<ScopeMove>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_guide: Option<McpScopeGuide>,
    /// 끄면 함께 꺼지는 것(플러그인 폴더의 스킬·에이전트·명령·MCP 서버 이름). MCP 스위치·폴더를 못 읽으면 `None`.
    pub contents: Option<PluginContents>,
    /// 스킬 스위치(개인·프로젝트)의 현재 값과 프로젝트 이름. 스킬이 아니면 `None`.
    pub skill: Option<SkillInfo>,
    pub format: SwitchFormat,
    /// 고칠 파일. `Manual`이면 `None`.
    pub file: Option<String>,
    /// `Manual`일 때 사용자에게 보여 줄 방법.
    pub instruction: Option<String>,
    #[serde(skip)]
    pub field: Field,
}

impl CoachSwitch {
    pub fn path(&self) -> Option<PathBuf> {
        self.file.as_ref().map(PathBuf::from)
    }

    /// 이 스위치를 `enabled`로 두는 편집. `Manual`이면 안내 문장으로 거부한다.
    pub fn plan(&self, content: &str, enabled: bool) -> Result<Edit, String> {
        match &self.field {
            Field::TomlEnabled { section } => edit::set_toml_enabled(content, section, enabled),
            Field::JsonServerDisabled => {
                edit::set_json_server_disabled(content, &self.name, !enabled)
            }
            Field::JsonPluginEnabled => edit::set_json_plugin_enabled(content, &self.name, enabled),
            Field::None => Err(self.instruction.clone().unwrap_or_else(|| {
                pick(
                    "kcal이 바꾸지 않는 스위치예요.",
                    "kcal doesn't change this switch.",
                )
                .into()
            })),
        }
    }
}

/// kcal이 쓸 수 있는 설정 파일 전부(되돌리기 기록 검증에 쓴다).
pub fn writable_files(home: &Path) -> [PathBuf; 3] {
    [
        paths::codex_root(home).join("config.toml"),
        paths::kiro_root(home).join("settings").join("mcp.json"),
        paths::claude_root(home).join("settings.json"),
    ]
}

/// 한 에이전트의 측정 항목에서 이름으로 토큰·로딩 시점을 찾는다.
/// 플러그인 항목 이름은 `<id> (스킬 N개)`, MCP는 `<name>`(크기 모름)이다. 시작 항목의 토큰만 더한다.
pub(super) fn measured(
    context: Option<&AgentContext>,
    kind: ContextKind,
    name: &str,
) -> (Option<u64>, Option<Loading>) {
    let Some(context) = context else {
        return (None, None);
    };
    let prefix = format!("{name} (");
    let found: Vec<_> = context
        .items
        .iter()
        .filter(|i| i.kind == kind && (i.name == name || i.name.starts_with(&prefix)))
        .collect();
    let loading = found.iter().map(|i| i.loading).min();
    let tokens = found
        .iter()
        .filter(|i| i.loading == Loading::Startup)
        .filter_map(|i| i.tokens)
        .reduce(|a, b| a + b);
    (tokens, loading)
}

/// Claude가 도구 이름에 쓰는 서버 이름(영숫자·`_`·`-` 밖은 `_`).
pub(super) fn tool_server_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub(super) fn base(agent: Agent, kind: SwitchKind, name: &str, enabled: bool) -> CoachSwitch {
    let kind_id = match kind {
        SwitchKind::Plugin => "plugin",
        SwitchKind::Mcp => "mcp",
        SwitchKind::Skill => "skill",
        SwitchKind::ProjectSkill => "project-skill",
    };
    CoachSwitch {
        target: format!("{}/{kind_id}:{name}", agent.id()),
        agent,
        kind,
        name: name.to_owned(),
        project_path: None,
        enabled,
        tokens: None,
        loading: None,
        is_proposed: false,
        usage: None,
        scope: None,
        scope_move: None,
        scope_guide: None,
        contents: None,
        skill: None,
        format: SwitchFormat::Manual,
        file: None,
        instruction: None,
        field: Field::None,
    }
}

pub(super) fn file_switch(
    mut switch: CoachSwitch,
    format: SwitchFormat,
    file: &Path,
    field: Field,
) -> CoachSwitch {
    switch.format = format;
    switch.file = Some(file.to_string_lossy().into_owned());
    switch.field = field;
    switch
}

/// 켠 것만 토큰을 붙인다(꺼진 것은 측정에 없다).
fn with_cost(mut switch: CoachSwitch, cost: (Option<u64>, Option<Loading>)) -> CoachSwitch {
    if switch.enabled {
        (switch.tokens, switch.loading) = cost;
    }
    switch
}

fn codex(
    home: &Path,
    context: Option<&AgentContext>,
    usage: Option<&UsageIndex>,
) -> Vec<CoachSwitch> {
    let file = paths::codex_root(home).join("config.toml");
    if !file.is_file() {
        return Vec::new();
    }
    let config = fsutil::read_text(&file);
    let trusted = scan::codex_trusted_projects(&config);
    let codex_usage = usage.and_then(|u| u.codex.as_deref());
    let plugins = scan::codex_plugin_states(&config).into_iter().map(|s| {
        let mut switch = base(Agent::Codex, SwitchKind::Plugin, &s.name, s.enabled);
        switch.contents = contents::read(&paths::codex_root(home), &s.name);
        if let Some(index) = codex_usage {
            let used = index.for_codex_plugin(&s.name);
            let explicit = edit::set_toml_enabled(&config, &s.section, false)
                .is_ok_and(|e| e.before.is_some() && e.changed);
            switch.scope_move =
                scope::codex::proposal(home, s.enabled && explicit, &used, &trusted);
            switch.usage = Some(used);
        }
        let switch = file_switch(
            switch,
            SwitchFormat::Toml,
            &file,
            Field::TomlEnabled { section: s.section },
        );
        with_cost(switch, measured(context, ContextKind::Plugins, &s.name))
    });
    // MCP 사용 기록은 0회도 붙인다. 끄기와 프로젝트 범위 제안은 같은 기록·세션 기준을 쓴다(0022, 2026-11-21).
    let servers = scan::codex_mcp_server_states(&config).into_iter().map(|s| {
        let mut switch = base(Agent::Codex, SwitchKind::Mcp, &s.name, s.enabled);
        if let Some(index) = codex_usage {
            let used = index.for_server(&codex_server_key(&s.name));
            switch.scope_move = scope::codex::proposal(home, s.enabled, &used, &trusted);
            switch.usage = Some(used);
        }
        let switch = file_switch(
            switch,
            SwitchFormat::Toml,
            &file,
            Field::TomlEnabled { section: s.section },
        );
        with_cost(switch, measured(context, ContextKind::Mcp, &s.name))
    });
    plugins.chain(servers).collect()
}

fn kiro(
    home: &Path,
    context: Option<&AgentContext>,
    usage: Option<&UsageIndex>,
) -> Vec<CoachSwitch> {
    let file = paths::kiro_root(home).join("settings").join("mcp.json");
    let Some(mcp) = fsutil::read_json(&file) else {
        return Vec::new();
    };
    scan::kiro_mcp_states(&mcp)
        .into_iter()
        .map(|(name, on)| {
            let mut switch = base(Agent::Kiro, SwitchKind::Mcp, &name, on);
            if let Some(index) = usage.and_then(|u| u.kiro.as_deref()) {
                switch.usage = Some(index.for_server(&name));
                switch = super::mcp_guide::attach_kiro(switch, home, &mcp);
            }
            let switch = file_switch(switch, SwitchFormat::Json, &file, Field::JsonServerDisabled);
            with_cost(switch, measured(context, ContextKind::Mcp, &name))
        })
        .collect()
}

/// 사용 횟수·지금 범위·옮기기 제안을 붙인다(사용 기록이 있을 때만).
fn with_usage(
    mut switch: CoachSwitch,
    home: &Path,
    usage: Option<&UsageIndex>,
    projects: &ProjectSettings,
    names: &[&str],
) -> CoachSwitch {
    let Some(index) = usage else {
        return switch;
    };
    let used = match switch.kind {
        SwitchKind::Plugin => index.for_plugin(&switch.name, names),
        SwitchKind::Mcp => index.for_server(&tool_server_name(&switch.name)),
        SwitchKind::Skill | SwitchKind::ProjectSkill => return switch,
    };
    if switch.kind == SwitchKind::Plugin {
        switch.scope = Some(projects.scope(&switch.name, switch.enabled));
        switch.scope_move = scope::proposal(home, switch.enabled, &used);
    }
    switch.usage = Some(used);
    super::mcp_guide::attach(switch, home)
}

fn claude(
    home: &Path,
    context: Option<&AgentContext>,
    usage: Option<&UsageIndex>,
    tuning: &Tuning,
) -> Vec<CoachSwitch> {
    let file = paths::claude_root(home).join("settings.json");
    let settings = fsutil::read_json(&file).unwrap_or(Value::Null);
    let states = scan::claude_plugin_states(&settings);
    let names: Vec<&str> = states.iter().map(|(id, _)| plugin_name(id)).collect();
    let projects = match usage {
        Some(index) => ProjectSettings::read(home, &index.projects),
        None => ProjectSettings::read(home, std::iter::empty()),
    };
    let mut switches: Vec<CoachSwitch> = states
        .iter()
        .map(|(id, on)| {
            let mut switch = base(Agent::Claude, SwitchKind::Plugin, id, *on);
            switch.contents = contents::read(&paths::claude_root(home), id);
            let switch = file_switch(switch, SwitchFormat::Json, &file, Field::JsonPluginEnabled);
            with_cost(switch, measured(context, ContextKind::Plugins, id))
        })
        .collect();

    let sent = shape::latest(home, None, tuning.shape_sessions_to_search);
    let mut project_rows = Vec::new();
    for workspace in shape::recent_workspaces(
        home,
        tuning.diagnose_usage_files,
        tuning.diagnose_recent_folders,
    ) {
        if scope::is_project_dir(home, &workspace) {
            project_rows.extend(super::folders::project_switches(
                home, &settings, usage, &workspace, tuning,
            ));
        }
    }
    switches.extend(super::folders::global_mcp(
        home,
        context,
        sent.as_ref(),
        &project_rows,
        tuning,
    ));
    let mut switches: Vec<CoachSwitch> = switches
        .into_iter()
        .map(|s| with_usage(s, home, usage, &projects, &names))
        .collect();
    switches.extend(skills::list(home, Some(&settings), usage, None));
    switches.extend(project_rows);
    switches
}

/// 전체 스위치 목록. `snapshot`: 전역 컨텍스트 측정(없으면 토큰을 모른다). `usage`: Claude·Codex 사용 기록.
/// 사용 기록이 없으면 제안하지 않는다. 제안 먼저 토큰 큰 순, 나머지는 원래 순서.
pub fn list(
    home: &Path,
    snapshot: Option<&ContextSnapshot>,
    usage: Option<&UsageIndex>,
    tuning: &Tuning,
) -> Vec<CoachSwitch> {
    let context = |agent| snapshot.and_then(|s| s.agents.iter().find(|a| a.agent == agent));
    let mut all: Vec<CoachSwitch> = [
        claude(home, context(Agent::Claude), usage, tuning),
        codex(home, context(Agent::Codex), usage),
        kiro(home, context(Agent::Kiro), usage),
    ]
    .into_iter()
    .flatten()
    .collect();
    // 같은 section이 두 번 나오면 첫 것만 둔다(고칠 때 두 번 나온다는 이유로 거부된다).
    let mut targets = HashSet::new();
    all.retain(|s| targets.insert(s.target.clone()));
    for switch in &mut all {
        // 사용 기록이 있으면 최근 N일 0회이고, 기록을 다 봤고, 세션이 `min_sessions`(tuning `diagnoseMinSessions`)개 이상일 때만
        // (0017 "E는 사용 기록 근거가 있을 때만"). 기록이 없거나 적으면 "안 쓴다"가 아니라 "모른다".
        // Codex 플러그인은 읽기 사용만 알아 끄기를 제안하지 않는다. Kiro MCP도 같은 끄기 기준을 쓴다(0022).
        let is_unused = match &switch.usage {
            Some(u) => u.total == 0 && !u.is_partial && u.sessions >= u.min_sessions,
            None => false,
        };
        if skills::is_skill(switch)
            || (switch.agent == Agent::Codex && switch.kind == SwitchKind::Plugin)
        {
            continue;
        }
        switch.is_proposed = switch.enabled
            && switch.loading == Some(Loading::Startup)
            && switch.tokens.is_some_and(|t| t > 0)
            && is_unused;
    }
    all.sort_by_key(|s| {
        (
            !s.is_proposed,
            std::cmp::Reverse(s.is_proposed.then_some(s.tokens).flatten()),
        )
    });
    all
}

/// 목록 안의 대상만 찾는다(프론트가 임의 대상을 고치게 하지 않는다).
pub fn find(switches: Vec<CoachSwitch>, target: &str) -> Result<CoachSwitch, String> {
    switches
        .into_iter()
        .find(|s| s.target == target)
        .ok_or_else(|| {
            crate::tr!(
                "{target}은(는) 바꿀 수 있는 스위치 목록에 없어요.",
                "{target} isn't in the list of switches kcal can change."
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ContextItem, ContextSource, fsutil::testing::TempDir};
    use time::OffsetDateTime;

    fn item(kind: ContextKind, name: &str, tokens: Option<u64>, loading: Loading) -> ContextItem {
        ContextItem {
            kind,
            name: name.into(),
            path: None,
            bytes: tokens.map(|t| t * 4),
            tokens,
            source: ContextSource::Estimated,
            loading,
        }
    }

    fn snapshot(agents: Vec<(Agent, Vec<ContextItem>)>) -> ContextSnapshot {
        ContextSnapshot {
            computed_at: OffsetDateTime::UNIX_EPOCH,
            agents: agents
                .into_iter()
                .map(|(agent, items)| AgentContext {
                    agent,
                    items,
                    total_startup_tokens: 0,
                    first_request_tokens: None,
                    first_request_chars: None,
                    model: None,
                    context_window: None,
                    warnings: vec![],
                })
                .collect(),
        }
    }

    fn home(name: &str) -> TempDir {
        let home = TempDir::new(&format!("coach-switches-{name}"));
        home.write(
            ".codex/config.toml",
            "[plugins.\"big@m\"]\nenabled = true\n[plugins.\"off@m\"]\n[plugins.\"small@m\"]\nenabled = true\n[mcp_servers.docs]\ncommand = \"x\"\n",
        );
        home.write(
            ".kiro/settings/mcp.json",
            r#"{"mcpServers": {"k": {}, "kd": {"disabled": true}}}"#,
        );
        home.write(
            ".claude/settings.json",
            r#"{"enabledPlugins": {"cp@m": true, "co@m": false}}"#,
        );
        home
    }

    fn measured_snapshot() -> ContextSnapshot {
        snapshot(vec![
            (
                Agent::Codex,
                vec![
                    item(
                        ContextKind::Plugins,
                        "big@m (스킬 3개)",
                        Some(900),
                        Loading::Startup,
                    ),
                    item(
                        ContextKind::Plugins,
                        "small@m (스킬 1개)",
                        Some(50),
                        Loading::Startup,
                    ),
                    item(ContextKind::Mcp, "docs", None, Loading::Deferred),
                ],
            ),
            (
                Agent::Kiro,
                vec![item(ContextKind::Mcp, "k", None, Loading::Startup)],
            ),
            (
                Agent::Claude,
                vec![item(
                    ContextKind::Plugins,
                    "cp@m (스킬 2개)",
                    Some(300),
                    Loading::Startup,
                )],
            ),
        ])
    }

    #[test]
    fn lists_switched_off_ones_too_and_proposes_only_claude_by_size() {
        let home = home("t1");
        let (index, tuning) = no_use_evidence();
        let all = list(
            home.path(),
            Some(&measured_snapshot()),
            Some(&index),
            &tuning,
        );
        let proposed: Vec<&str> = all
            .iter()
            .filter(|s| s.is_proposed)
            .map(|s| s.target.as_str())
            .collect();
        // 이 fixture의 Codex·Kiro는 사용 근거가 없어 제안하지 않는다(0022).
        assert_eq!(proposed, ["claude/plugin:cp@m"]);
        let off = all
            .iter()
            .find(|s| s.target == "codex/plugin:off@m")
            .unwrap();
        assert!(!off.enabled && off.tokens.is_none() && !off.is_proposed);
        assert!(all.iter().any(|s| s.target == "kiro/mcp:kd" && !s.enabled));
        assert!(
            all.iter()
                .any(|s| s.target == "claude/plugin:co@m" && !s.enabled)
        );
    }

    #[test]
    fn plugin_switches_carry_what_turns_off_with_them_and_mcp_switches_do_not() {
        let home = home("contents");
        home.write(
            ".claude/plugins/cache/m/cp/1.0.0/skills/review/SKILL.md",
            "x",
        );
        home.write(".claude/plugins/cache/m/cp/1.0.0/agents/checker.md", "x");
        home.write(".codex/plugins/cache/m/big/1.0.0/skills/plan/SKILL.md", "x");
        let all = list(home.path(), None, None, &Tuning::default());
        let get = |target: &str| find(all.clone(), target).unwrap().contents;
        let cp = get("claude/plugin:cp@m").expect("claude plugin contents");
        assert_eq!(
            (cp.skills.names, cp.agents.names),
            (vec!["review".to_owned()], vec!["checker".to_owned()])
        );
        assert_eq!(get("codex/plugin:big@m").map(|c| c.skills.count), Some(1));
        // 폴더가 없는 플러그인, MCP 스위치는 없음.
        assert_eq!(get("claude/plugin:co@m"), None);
        assert_eq!(get("codex/mcp:docs"), None);
        assert_eq!(get("kiro/mcp:k"), None);
    }

    #[test]
    fn unknown_size_and_deferred_are_not_proposed() {
        let home = home("t2");
        let all = list(
            home.path(),
            Some(&measured_snapshot()),
            None,
            &Tuning::default(),
        );
        let docs = all.iter().find(|s| s.target == "codex/mcp:docs").unwrap();
        assert_eq!(
            (docs.tokens, docs.loading, docs.is_proposed),
            (None, Some(Loading::Deferred), false)
        );
        let kiro = all.iter().find(|s| s.target == "kiro/mcp:k").unwrap();
        assert_eq!((kiro.tokens, kiro.is_proposed), (None, false));
        // 측정이 없으면 아무것도 제안하지 않는다.
        assert!(
            list(home.path(), None, None, &Tuning::default())
                .iter()
                .all(|s| !s.is_proposed)
        );
    }

    /// `cp@m`(300 토큰)을 최근에 쓴 기록과 `.claude.json` MCP `docs`를 쓴 기록.
    /// 기록을 다 봤고 아무것도 안 썼다(세션 수 기준은 끈다): 크기만으로 제안이 갈리는 근거.
    fn no_use_evidence() -> (UsageIndex, Tuning) {
        let tuning = Tuning {
            diagnose_min_sessions: 0,
            ..Tuning::default()
        };
        (UsageIndex::default(), tuning)
    }

    fn used_index(home: &TempDir, files: usize) -> UsageIndex {
        let now = OffsetDateTime::now_utc();
        let stamp = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        for (i, tool) in ["Skill", "mcp__docs__read"].iter().enumerate() {
            let line = serde_json::json!({"type": "assistant", "cwd": home.path().join("w"),
                "timestamp": stamp, "message": {"content": [{"type": "tool_use", "id": format!("t{i}"),
                "name": tool, "input": {"skill": "cp:review"}}]}});
            home.write(
                &format!(".claude/projects/-w/s{i}.jsonl"),
                &line.to_string(),
            );
        }
        let tuning = Tuning {
            diagnose_usage_files: files,
            ..Tuning::default()
        };
        super::super::usage::read(home.path(), &tuning, crate::metrics::unix_secs(now))
    }

    #[test]
    fn claude_switches_used_in_the_recent_days_are_not_proposed_for_turning_off() {
        let home = home("t6");
        home.write(".claude.json", r#"{"mcpServers": {"docs": {}}}"#);
        let tools = r#"[{"name":"mcp__docs__read","input_schema":{"a":"12345678"}}]"#;
        home.write(
            ".claude/projects/-shape/s.jsonl",
            &format!(r#"{{"type":"api-request-shape","shape":{{"system":[],"tools":{tools}}}}}"#),
        );
        let snapshot = measured_snapshot();
        let proposed = |index: Option<&UsageIndex>| -> Vec<String> {
            list(home.path(), Some(&snapshot), index, &Tuning::default())
                .into_iter()
                .filter(|s| s.is_proposed && s.agent == Agent::Claude)
                .map(|s| s.target)
                .collect()
        };
        // 사용 기록을 못 읽었으면 근거가 없어 제안하지 않는다.
        assert!(proposed(None).is_empty());
        let index = used_index(&home, 1000);
        assert!(proposed(Some(&index)).is_empty());
        let all = list(
            home.path(),
            Some(&snapshot),
            Some(&index),
            &Tuning::default(),
        );
        let cp = find(all.clone(), "claude/plugin:cp@m").unwrap();
        assert_eq!(cp.usage.map(|u| (u.total, u.project_count)), Some((1, 1)));
        assert_eq!(
            find(all, "claude/mcp:docs").unwrap().usage.unwrap().total,
            1
        );
        // Codex·Kiro는 사용 기록이 없어 토큰이 커도 제안하지 않는다.
        let all = list(
            home.path(),
            Some(&snapshot),
            Some(&index),
            &Tuning::default(),
        );
        let big = find(all, "codex/plugin:big@m").unwrap();
        assert!(big.tokens.is_some() && big.usage.is_some() && !big.is_proposed);
    }

    #[test]
    fn zero_uses_in_a_partial_read_are_not_evidence_for_turning_off() {
        let home = home("t7");
        // 파일 하나만 읽으면 cp 기록을 놓칠 수 있다: 0회여도 제안하지 않는다.
        let index = used_index(&home, 1);
        assert!(index.is_partial);
        let all = list(
            home.path(),
            Some(&measured_snapshot()),
            Some(&index),
            &Tuning::default(),
        );
        assert!(!find(all, "claude/plugin:cp@m").unwrap().is_proposed);
    }

    /// 사용 없는 세션 기록 `recent`개(방금) + `old`개(60일 전)를 쓰고 읽는다. 기간은 30일.
    fn quiet_history(home: &TempDir, recent: usize, old: usize) -> UsageIndex {
        let stale = std::time::SystemTime::now() - std::time::Duration::from_secs(60 * 86_400);
        for i in 0..recent + old {
            let rel = format!(".claude/projects/-w/q{i}.jsonl");
            home.write(
                &rel,
                &serde_json::json!({"type": "user", "cwd": home.path().join("w")}).to_string(),
            );
            if i >= recent {
                std::fs::File::options()
                    .write(true)
                    .open(home.path().join(&rel))
                    .unwrap()
                    .set_modified(stale)
                    .unwrap();
            }
        }
        let now = crate::metrics::unix_secs(OffsetDateTime::now_utc());
        super::super::usage::read(home.path(), &Tuning::default(), now)
    }

    #[test]
    fn plugin_is_not_proposed_without_enough_claude_sessions_to_judge() {
        let is_proposed = |name: &str, recent: usize, old: usize| {
            let home = home(name);
            let index = quiet_history(&home, recent, old);
            let all = list(
                home.path(),
                Some(&measured_snapshot()),
                Some(&index),
                &Tuning::default(),
            );
            let cp = find(all, "claude/plugin:cp@m").unwrap();
            (cp.is_proposed, cp.usage.map(|u| u.sessions))
        };
        // 기록 없음, 30일 넘은 기록만, 세션 3개: 0회여도 근거가 아니다.
        assert_eq!(is_proposed("q-none", 0, 0), (false, Some(0)));
        assert_eq!(is_proposed("q-old", 0, 12), (false, Some(0)));
        assert_eq!(is_proposed("q-few", 3, 0), (false, Some(3)));
        // 세션이 충분하고 0회면 제안한다.
        assert_eq!(is_proposed("q-enough", 10, 0), (true, Some(10)));
    }

    #[test]
    fn claude_mcp_has_instruction_only_and_cannot_be_planned() {
        let home = home("t3");
        home.write(
            ".claude.json",
            r#"{"mcpServers": {"my.docs": {"command": "x"}, "lazy": {}}}"#,
        );
        let tools = r#"[{"name":"mcp__my_docs__read","input_schema":{"a":"12345678"}},{"name":"mcp__plugin_x__go","input_schema":{}}]"#;
        home.write(
            ".claude/projects/-w/s.jsonl",
            &format!(r#"{{"type":"api-request-shape","shape":{{"system":[],"tools":{tools}}}}}"#),
        );
        let (index, tuning) = no_use_evidence();
        let all = list(
            home.path(),
            Some(&measured_snapshot()),
            Some(&index),
            &tuning,
        );
        let docs = find(all.clone(), "claude/mcp:my.docs").unwrap();
        assert_eq!(docs.format, SwitchFormat::Manual);
        assert_eq!(docs.file, None);
        assert_eq!(docs.instruction.as_deref(), Some(claude_mcp_instruction()));
        assert!(docs.is_proposed && docs.tokens.is_some());
        assert_eq!(
            docs.plan("{}", false).unwrap_err(),
            claude_mcp_instruction()
        );
        let lazy = find(all.clone(), "claude/mcp:lazy").unwrap();
        assert_eq!(
            (lazy.loading, lazy.is_proposed),
            (Some(Loading::Deferred), false)
        );
        assert!(find(all, "claude/mcp:plugin_x").unwrap().loading == Some(Loading::Startup));
    }

    #[test]
    fn rejects_targets_outside_the_switch_list() {
        let home = home("t4");
        let all = list(home.path(), None, None, &Tuning::default());
        assert!(find(all.clone(), "codex/plugin:../../etc/passwd").is_err());
        assert!(find(all, "codex/plugin:new@m").is_err());
    }

    #[test]
    fn codex_plugin_without_section_is_not_listed_and_missing_files_list_nothing() {
        let empty = TempDir::new("coach-switches-empty");
        assert!(list(empty.path(), None, None, &Tuning::default()).is_empty());
        let home = home("t5");
        let all = list(home.path(), None, None, &Tuning::default());
        assert_eq!(
            all.iter()
                .filter(|s| s.agent == Agent::Codex && s.kind == SwitchKind::Plugin)
                .count(),
            3
        );
    }
    include!("switches_codex_tests.rs");
    include!("switches_kiro_tests.rs");
}
