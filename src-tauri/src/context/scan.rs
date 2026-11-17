//! 에이전트별 항목 모으기(ctxmeter `scanner.js` `scanClaude`·`scanCodex`, `kiro-scan.js` `scanKiro`,
//! MCP 서버 목록은 `claude-mcp.js`·`mcp-cost.js` `configuredMcpServers`의 설정 읽기 부분만).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use super::fsutil::{self, AssetStatus, count_hooks, plugin_cache_directory, read_json};
use super::{
    AgentContext, ContextItem, ContextKind, Loading, codex_session, instructions, kiro_session,
    loading, paths, shape, window,
};
use crate::coach::contents;
use crate::i18n::{pick, plural};
use crate::tr;
use crate::tuning::Tuning;
use crate::usage::Agent;

const ACTIVE: &[AssetStatus] = &[AssetStatus::ActiveCandidate];
const PLUGIN_CACHE: &[AssetStatus] = &[AssetStatus::PluginCache];

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// skill 묶음 하나 → 항목 하나(skill이 없으면 없음). 목록에 front matter가 시작 때 들어간다.
fn skill_item(
    kind: ContextKind,
    id: &str,
    directory: &Path,
    allowed: &[AssetStatus],
) -> Option<ContextItem> {
    let group = fsutil::skill_group(directory, allowed);
    skill_entry(kind, id, directory, group.skills, group.metadata_bytes)
}

/// Claude 플러그인의 skill 묶음: 기본 `skills/` 아래 `SKILL.md`와 매니페스트(plugin.json·마켓플레이스 항목)가 더한 스킬.
/// 매니페스트를 못 읽으면 기본 `skills/`만 본다.
fn plugin_skill_item(root: &Path, id: &str, directory: &Path) -> Option<ContextItem> {
    let skills_dir = directory.join("skills");
    let (reads_default, extra) =
        contents::extra_skills(root, id).map_or((true, Vec::new()), |e| (e.reads_default, e.found));
    let mut files = if reads_default {
        fsutil::skill_files(&skills_dir, PLUGIN_CACHE)
    } else {
        Vec::new()
    };
    files.extend(
        extra
            .into_iter()
            .map(|(_, file)| file)
            .filter(|file| PLUGIN_CACHE.contains(&fsutil::classify(file))),
    );
    // 매니페스트가 기본 `skills/` 안을 다시 가리켜도 한 번만 센다.
    let mut real: Vec<PathBuf> = files.iter().filter_map(|f| f.canonicalize().ok()).collect();
    real.sort();
    real.dedup();
    let metadata_bytes = real
        .iter()
        .map(|f| std::fs::read(f).map_or(0, |c| fsutil::skill_metadata_bytes(&c)))
        .sum();
    skill_entry(
        ContextKind::Plugins,
        id,
        &skills_dir,
        real.len(),
        metadata_bytes,
    )
}

fn skill_entry(
    kind: ContextKind,
    id: &str,
    directory: &Path,
    skills: usize,
    metadata_bytes: u64,
) -> Option<ContextItem> {
    (skills > 0).then(|| {
        ContextItem::estimated(
            kind,
            skills_name(id, skills),
            Some(directory),
            metadata_bytes,
            Loading::Startup,
        )
    })
}

/// "user-skills (스킬 3개)".
fn skills_name(id: &str, skills: usize) -> String {
    let noun = plural(skills, "skill", "skills");
    tr!("{id} (스킬 {skills}개)", "{id} ({skills} {noun})")
}

/// skill 위치 묶음(id, 폴더들). 앞쪽이 이긴다: 같은 이름은 먼저 나온 묶음에서 한 번만 센다(Codex 실측, 0005).
/// 이름은 front matter `name`, 없으면 SKILL.md가 든 폴더 이름.
fn named_skill_items(places: &[(&str, Vec<PathBuf>)]) -> Vec<ContextItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for (id, directories) in places {
        let (mut skills, mut bytes, mut first) = (0, 0, None);
        for directory in directories {
            for file in fsutil::skill_files(directory, ACTIVE) {
                let content = std::fs::read(&file).unwrap_or_default();
                let name = fsutil::front_matter_key(&String::from_utf8_lossy(&content), "name")
                    .unwrap_or_else(|| file.parent().map_or_else(String::new, file_name));
                if seen.insert(name) {
                    skills += 1;
                    bytes += fsutil::skill_metadata_bytes(&content);
                    first.get_or_insert(directory);
                }
            }
        }
        if let Some(directory) = first {
            items.extend(skill_entry(
                ContextKind::Skills,
                id,
                directory,
                skills,
                bytes,
            ));
        }
    }
    items
}

/// 폴더 아래 `suffix`로 끝나는 파일마다 항목 하나(파일 크기).
fn file_items(
    kind: ContextKind,
    directory: &Path,
    suffix: &str,
    prefix: &str,
    loading: Loading,
) -> Vec<ContextItem> {
    fsutil::walk_files(directory, &|p| p.to_string_lossy().ends_with(suffix))
        .into_iter()
        .filter_map(|file| {
            let bytes = fsutil::file_size(&file)?;
            Some(ContextItem::estimated(
                kind,
                format!("{prefix}{}", file_name(&file)),
                Some(&file),
                bytes,
                loading,
            ))
        })
        .collect()
}

/// 전역 모드의 실측값은 어느 프로젝트든 가장 최근 세션에서 온다. 그 출처를 밝힌다.
fn global_session_note(agent: &str, cwd: Option<&str>) -> String {
    let folder = cwd
        .map(|c| c.trim_end_matches(['/', '\\']))
        .and_then(|c| c.rsplit(['/', '\\']).next())
        .filter(|name| !name.is_empty())
        .unwrap_or(pick("알 수 없음", "unknown"));
    tr!(
        "실측값은 가장 최근 {agent} 세션(작업 폴더 {folder}) 기준입니다.",
        "Measured values come from the latest {agent} session (folder {folder})."
    )
}

fn hook_item(count: usize, items: &mut Vec<ContextItem>, warnings: &mut Vec<String>) {
    if count == 0 {
        return;
    }
    let noun = plural(count, "hook", "hooks");
    items.push(ContextItem::unknown_size(
        ContextKind::Hooks,
        tr!("hook {count}개", "{count} {noun}"),
        Loading::Conditional,
    ));
    warnings.push(tr!(
        "hook {count}개: 출력 크기는 실행할 때 정해져 세지 않았습니다.",
        "{count} {noun}: output size is decided when they run, so it isn't counted."
    ));
}

/// 설정에서 읽은 MCP 서버(실행하지 않음) → 크기 모르는 항목.
fn mcp_items(
    servers: Vec<(String, Loading)>,
    items: &mut Vec<ContextItem>,
    warnings: &mut Vec<String>,
) {
    if servers.is_empty() {
        return;
    }
    let count = servers.len();
    let noun = plural(count, "server", "servers");
    warnings.push(tr!(
        "MCP 서버 {count}개: 도구 스키마는 서버를 실행해야 알 수 있어 세지 않았습니다.",
        "{count} MCP {noun}: tool schemas are only known once a server runs, so they aren't counted."
    ));
    items.extend(
        servers
            .into_iter()
            .map(|(name, loading)| ContextItem::unknown_size(ContextKind::Mcp, name, loading)),
    );
}

fn plugin_missing(id: &str) -> String {
    tr!(
        "플러그인 {id}: 캐시 폴더가 없어 세지 않았습니다.",
        "Plugin {id}: no cache folder, so it isn't counted."
    )
}

fn object(value: Option<&Value>) -> Map<String, Value> {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

// ---------- Claude ----------

/// `settings.json` `enabledPlugins` 전부와 켜짐 여부(`true`만 켜짐), 이름순.
pub(crate) fn claude_plugin_states(settings: &Value) -> Vec<(String, bool)> {
    let mut states: Vec<(String, bool)> = object(settings.get("enabledPlugins"))
        .into_iter()
        .map(|(id, on)| (id, on == Value::Bool(true)))
        .collect();
    states.sort();
    states
}

pub(crate) fn enabled_plugin_ids(settings: &Value) -> Vec<String> {
    claude_plugin_states(settings)
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(id, _)| id)
        .collect()
}

/// 켜진 Claude MCP 서버와 `alwaysLoad`. scope: user(`~/.claude.json`) < project(작업 폴더와 그 위 폴더의 `.mcp.json`, 승인된 것) < local.
/// `/mcp`로 끈 서버(`disabledMcpServers`)는 뺀다. `~/.claude/mcp.json`은 Claude가 읽지 않아 보지 않는다.
pub(crate) fn claude_mcp_servers(home: &Path, workspace: Option<&Path>) -> Vec<(String, bool)> {
    let state = read_json(&paths::claude_state_file(home)).unwrap_or(Value::Null);
    let mut by_name: HashMap<String, Value> = object(state.get("mcpServers")).into_iter().collect();
    let mut switched_off = HashSet::new();
    if let Some(workspace) = workspace {
        let key = workspace.to_string_lossy();
        let project = object(state.get("projects").and_then(|p| p.get(key.as_ref())));
        let is_approved = project_approval(home, workspace, &project);
        // 작업 폴더 위 폴더의 `.mcp.json`도 project 서버로 실린다(Claude Code 2.1.326 실측, 공식 문서에는 없음).
        // 먼 폴더부터 읽어 가까운 폴더의 같은 이름이 덮는다(추정).
        let dirs: Vec<&Path> = workspace.ancestors().collect();
        for dir in dirs.into_iter().rev() {
            let project_file = read_json(&dir.join(".mcp.json")).unwrap_or(Value::Null);
            for (name, config) in object(project_file.get("mcpServers")) {
                if is_approved(&name) {
                    by_name.insert(name, config);
                }
            }
        }
        by_name.extend(object(project.get("mcpServers")));
        switched_off.extend(strings(project.get("disabledMcpServers")));
    }
    by_name
        .into_iter()
        .filter(|(name, config)| config.is_object() && !switched_off.contains(name))
        .map(|(name, config)| {
            let always = config.get("alwaysLoad") == Some(&Value::Bool(true));
            (name, always)
        })
        .collect()
}

/// project scope 서버 승인. 레포의 `.claude/settings.json` 승인은 trust를 수락한 뒤에만 센다.
fn project_approval(
    home: &Path,
    workspace: &Path,
    project: &Map<String, Value>,
) -> impl Fn(&str) -> bool {
    let user_settings = paths::claude_root(home).join("settings.json");
    // 공식 문서(MCP "Project scope"): bypassPermissions 모드 + 사용자 설정 `skipDangerousModePermissionPrompt`면
    // 승인을 묻지 않고 싣는다. 세션 모드는 kcal이 모르므로 이 설정이 켜져 있으면 실리는 쪽으로 센다.
    // ponytail: 같은 Mac의 bypass 아닌 대화형 세션에서는 승인 전 서버를 더 센다. 세션 기록에서 모드를 읽으면 나눈다.
    let skips_prompt = read_json(&user_settings)
        .is_some_and(|s| s.get("skipDangerousModePermissionPrompt") == Some(&Value::Bool(true)));
    let mut files = vec![
        user_settings,
        workspace.join(".claude").join("settings.local.json"),
    ];
    if project.get("hasTrustDialogAccepted") == Some(&Value::Bool(true)) {
        files.push(workspace.join(".claude").join("settings.json"));
    }
    let settings: Vec<Value> = files.iter().filter_map(|f| read_json(f)).collect();
    let approve_all = skips_prompt
        || settings
            .iter()
            .any(|s| s.get("enableAllProjectMcpServers") == Some(&Value::Bool(true)));
    let collect = |key: &str| -> HashSet<String> {
        strings(project.get(key))
            .into_iter()
            .chain(settings.iter().flat_map(|s| strings(s.get(key))))
            .collect()
    };
    let approved = collect("enabledMcpjsonServers");
    let rejected = collect("disabledMcpjsonServers");
    move |name| !rejected.contains(name) && (approve_all || approved.contains(name))
}

/// Claude settings 파일, 낮은 우선순위부터(user → project → local).
fn claude_settings(home: &Path, workspace: Option<&Path>) -> Vec<Value> {
    let mut files = vec![paths::claude_root(home).join("settings.json")];
    if let Some(w) = workspace {
        files.push(w.join(".claude").join("settings.json"));
        files.push(w.join(".claude").join("settings.local.json"));
    }
    files.iter().filter_map(|f| read_json(f)).collect()
}

/// Claude가 쓰는 env: 실행 환경 위에 settings의 `env`를 겹친다.
fn claude_env(settings: &[Value], env: &HashMap<String, String>) -> HashMap<String, String> {
    let mut merged = env.clone();
    for value in settings {
        for (key, v) in object(value.get("env")) {
            if let Some(s) = v.as_str() {
                merged.insert(key, s.to_owned());
            }
        }
    }
    merged
}

fn is_rule_path(path: &str) -> bool {
    path.contains("/rules/") || path.contains("\\rules\\")
}

/// 세션 시작 첨부(실측)를 `/context` 묶음 항목으로. 설정 파일로 어림한 시작 항목 중 같은 묶음은 지운다.
/// 조건부·지연 항목(`paths` rule, agent 본문)은 첨부에 없어 그대로 둔다.
fn apply_startup_attachments(
    startup: shape::StartupAttachments,
    plugin_ids: &[String],
    items: &mut Vec<ContextItem>,
) {
    let drop_startup = |items: &mut Vec<ContextItem>, kinds: &[ContextKind]| {
        items.retain(|i| i.loading != Loading::Startup || !kinds.contains(&i.kind));
    };
    if let Some(files) = startup.memory_files {
        drop_startup(items, &[ContextKind::Instructions, ContextKind::Rules]);
        items.extend(files.into_iter().map(|f| {
            let kind = if is_rule_path(&f.name) {
                ContextKind::Rules
            } else {
                ContextKind::Instructions
            };
            let path = PathBuf::from(&f.name);
            ContextItem {
                path: Some(f.name),
                ..ContextItem::measured(kind, file_name(&path), f.bytes)
            }
        }));
    }
    if let Some(listing) = startup.skills {
        drop_startup(items, &[ContextKind::Skills, ContextKind::Plugins]);
        let (mut rest_skills, mut rest_bytes) = (listing.skills, listing.bytes);
        // 켠 플러그인의 스킬은 플러그인 id로 묶는다(코칭이 `<id> (스킬 N개)` 이름으로 찾는다, 0014).
        for (prefix, skills, bytes) in listing.by_prefix {
            let Some(id) = plugin_ids
                .iter()
                .find(|id| id.split('@').next() == Some(prefix.as_str()))
            else {
                continue;
            };
            rest_skills = rest_skills.saturating_sub(skills);
            rest_bytes = rest_bytes.saturating_sub(bytes);
            items.push(ContextItem::measured(
                ContextKind::Plugins,
                skills_name(id, skills),
                bytes,
            ));
        }
        let noun = plural(rest_skills, "skill", "skills");
        items.push(ContextItem::measured(
            ContextKind::Skills,
            tr!(
                "스킬 목록 (스킬 {rest_skills}개)",
                "Skill list ({rest_skills} {noun})"
            ),
            rest_bytes,
        ));
    }
    if let Some(agents) = startup.custom_agents.filter(|a| !a.is_empty()) {
        items.push(ContextItem::measured(
            ContextKind::Agents,
            tr!("에이전트 목록 ({}개)", "Agent list ({})", agents.len()),
            agents.iter().map(|a| a.bytes).sum(),
        ));
    }
    for server in startup.mcp_instructions.unwrap_or_default() {
        items.push(ContextItem::measured(
            ContextKind::McpInstructions,
            server.name,
            server.bytes,
        ));
    }
}

/// shape 출처. 작업 폴더를 줬는데 그 폴더 세션이 없으면 가장 최근 세션으로 대신하고 경고한다.
fn claude_session(
    home: &Path,
    workspace: Option<&Path>,
    tuning: &Tuning,
    warnings: &mut Vec<String>,
) -> Option<(shape::ShapeSummary, bool)> {
    let search = tuning.shape_sessions_to_search;
    if let Some(sent) = shape::latest(home, workspace, search) {
        return Some((sent, true));
    }
    let workspace = workspace?;
    let sent = shape::latest(home, None, search)?;
    warnings.push(tr!(
        "이 작업 폴더({})의 Claude 세션 기록이 없어 {} 그 프로젝트의 MCP 도구가 포함됩니다.",
        "No Claude session in this folder ({}). {} Its MCP tools are included.",
        file_name(workspace),
        global_session_note("Claude", sent.cwd.as_deref())
    ));
    Some((sent, false))
}

pub fn claude(
    home: &Path,
    workspace: Option<&Path>,
    env: &HashMap<String, String>,
    tuning: &Tuning,
) -> AgentContext {
    let root = paths::claude_root(home);
    let settings = read_json(&root.join("settings.json")).unwrap_or(Value::Null);
    let all_settings = claude_settings(home, workspace);
    let env = claude_env(&all_settings, env);
    let plugin_ids = enabled_plugin_ids(&settings);
    let mut items = instructions::claude(home, workspace).items;
    let mut warnings = Vec::new();

    items.extend(skill_item(
        ContextKind::Skills,
        "user-skills",
        &root.join("skills"),
        ACTIVE,
    ));
    items.extend(skill_item(
        ContextKind::Skills,
        "agent-skills",
        &root.join(".agents").join("skills"),
        ACTIVE,
    ));
    items.extend(file_items(
        ContextKind::Agents,
        &root.join("agents"),
        ".md",
        "",
        Loading::Conditional,
    ));
    for id in &plugin_ids {
        let Some(directory) = plugin_cache_directory(&root, id) else {
            warnings.push(plugin_missing(id));
            continue;
        };
        items.extend(plugin_skill_item(&root, id, &directory));
        let prefix = format!("{}/", id.split('@').next().unwrap_or(id));
        items.extend(file_items(
            ContextKind::Agents,
            &directory.join("agents"),
            ".md",
            &prefix,
            Loading::Conditional,
        ));
    }
    hook_item(
        count_hooks(settings.get("hooks")),
        &mut items,
        &mut warnings,
    );

    let mut model = None;
    match claude_session(home, workspace, tuning, &mut warnings) {
        Some((sent, is_same_workspace)) => {
            if workspace.is_none() {
                warnings.push(tr!(
                    "{} 그 프로젝트의 MCP 도구가 포함됩니다.",
                    "{} Its MCP tools are included.",
                    global_session_note("Claude", sent.cwd.as_deref())
                ));
            }
            // 다른 프로젝트 세션의 지시 파일·스킬은 이 작업 폴더 것이 아니라 설정 파일 어림을 그대로 둔다.
            if is_same_workspace {
                apply_startup_attachments(sent.startup, &plugin_ids, &mut items);
            }
            model = sent.model;
            let blocks = sent.system_blocks;
            let noun = plural(blocks, "block", "blocks");
            items.push(ContextItem::measured(
                ContextKind::System,
                tr!(
                    "시스템 프롬프트 (블록 {blocks}개)",
                    "System prompt ({blocks} {noun})"
                ),
                sent.system_bytes,
            ));
            items.push(ContextItem::measured(
                ContextKind::Tools,
                tr!(
                    "내장 도구 ({}개)",
                    "Built-in tools ({})",
                    sent.builtin_tools
                ),
                sent.builtin_bytes,
            ));
            if sent.deferred_tools > 0 {
                items.push(ContextItem::measured_deferred(
                    ContextKind::Tools,
                    tr!(
                        "지연 로딩 도구 목록 ({}개)",
                        "Deferred tool list ({})",
                        sent.deferred_tools
                    ),
                    sent.deferred_bytes,
                ));
            }
            items.extend(sent.mcp.into_iter().map(|s| {
                let (server, tools) = (&s.server, s.tools);
                let noun = plural(tools, "tool", "tools");
                ContextItem::measured(
                    ContextKind::Mcp,
                    tr!("{server} (도구 {tools}개)", "{server} ({tools} {noun})"),
                    s.bytes,
                )
            }));
        }
        None => {
            if !shape::has_sessions(home) {
                warnings.push(
                    pick(
                        "아직 Claude 세션이 없어서 시스템 프롬프트와 내장 도구는 세지 않았습니다. 세션을 한 번 시작한 뒤 새로고침하세요.",
                        "No Claude session found yet, so the system prompt and built-in tools aren't counted. Start a session, then press Refresh.",
                    )
                    .into(),
                );
            } else {
                warnings.push(
                    pick(
                        "최근 Claude 세션에서 시스템 프롬프트 크기가 보고되지 않아 시스템 프롬프트·도구는 세지 않았습니다(Claude Code 2.1.300 이상 필요). 업데이트한 뒤 새 세션을 시작하고 새로고침하세요.",
                        "No recent Claude session reported the system prompt size (needs Claude Code 2.1.300+), so the system prompt and tools aren't counted. Start a new session after updating, then press Refresh.",
                    )
                    .into(),
                );
            }
            let servers = claude_mcp_servers(home, workspace)
                .into_iter()
                .map(|(name, always)| (name, loading::claude(&env, always)))
                .collect();
            mcp_items(servers, &mut items, &mut warnings);
        }
    }
    let mut context = AgentContext::new(Agent::Claude, items, warnings, tuning.bytes_per_token);
    context.context_window = model
        .as_deref()
        .and_then(|m| window::claude(m, &env, &all_settings));
    context.model = model;
    context
}

// ---------- Codex ----------

static SECTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\[([^\]]+)\]").unwrap());

/// Codex `config.toml`의 켜고 끄는 section 하나(5층 코칭이 같은 결과로 스위치 목록을 만든다, 0014).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TomlSwitchState {
    pub name: String,
    /// 파일에 적힌 그대로의 section 이름(`plugins."a@m"`, `mcp_servers.one`). 고칠 때 이 헤더를 찾는다.
    pub section: String,
    pub enabled: bool,
}

/// `[plugins."<id>"]` 전부와 켜짐 여부. `enabled = true`가 없으면 꺼짐.
pub(crate) fn codex_plugin_states(config: &str) -> Vec<TomlSwitchState> {
    static PLUGIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^plugins\."(.+)"$"#).unwrap());
    static ENABLED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*enabled\s*=\s*true(?:\s*(?:#.*)?)?$").unwrap());
    let mut states: Vec<TomlSwitchState> = Vec::new();
    let mut current: Option<usize> = None;
    for line in config.split('\n') {
        if let Some(section) = SECTION.captures(line) {
            current = PLUGIN.captures(&section[1]).map(|c| {
                states.push(TomlSwitchState {
                    name: c[1].to_owned(),
                    section: section[1].to_owned(),
                    enabled: false,
                });
                states.len() - 1
            });
        } else if let Some(i) = current
            && ENABLED.is_match(line)
        {
            states[i].enabled = true;
        }
    }
    states
}

fn codex_enabled_plugins(config: &str) -> Vec<String> {
    codex_plugin_states(config)
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| s.name)
        .collect()
}

/// `[mcp_servers.<name>]` 전부와 켜짐 여부. `enabled = false`가 없으면 켜짐(플러그인과 반대 기본값).
pub(crate) fn codex_mcp_server_states(config: &str) -> Vec<TomlSwitchState> {
    static SERVER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^mcp_servers\.(?:"([^"]+)"|([^.]+))$"#).unwrap());
    static DISABLED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*enabled\s*=\s*false(?:\s*(?:#.*)?)?$").unwrap());
    let mut states = Vec::new();
    let mut current: Option<TomlSwitchState> = None;
    for line in config.split('\n') {
        if let Some(section) = SECTION.captures(line) {
            states.extend(current.take());
            // `.env` 같은 하위 section은 이름을 비워 그 안의 키를 서버 키로 읽지 않는다.
            current = SERVER
                .captures(&section[1])
                .and_then(|c| c.get(1).or(c.get(2)))
                .map(|m| TomlSwitchState {
                    name: m.as_str().to_owned(),
                    section: section[1].to_owned(),
                    enabled: true,
                });
        } else if let Some(state) = current.as_mut()
            && DISABLED.is_match(line)
        {
            state.enabled = false;
        }
    }
    states.extend(current);
    states
}

fn codex_enabled_mcp_servers(config: &str) -> Vec<String> {
    codex_mcp_server_states(config)
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| s.name)
        .collect()
}

/// Codex가 skill을 싣는 위치, 이긴 순서(0005). `/etc/codex/skills`(관리자)는 넣지 않았다.
fn codex_skill_places(home: &Path, workspace: Option<&Path>) -> Vec<(&'static str, Vec<PathBuf>)> {
    let root = paths::codex_root(home);
    let repo = workspace
        .map(|w| {
            instructions::codex_project_directories(w)
                .into_iter()
                .rev()
                .map(|d| d.join(".agents").join("skills"))
                .collect()
        })
        .unwrap_or_default();
    vec![
        ("repo-skills", repo),
        (
            "home-agent-skills",
            vec![home.join(".agents").join("skills")],
        ),
        ("agent-skills", vec![root.join(".agents").join("skills")]),
        ("user-and-system-skills", vec![root.join("skills")]),
    ]
}

pub fn codex(home: &Path, workspace: Option<&Path>, tuning: &Tuning) -> AgentContext {
    let root = paths::codex_root(home);
    let config = fsutil::read_text(&root.join("config.toml"));
    let found = instructions::codex(home, workspace);
    let mut items = found.items;
    let mut warnings = Vec::new();
    if found.truncated {
        warnings.push(
            pick(
                "AGENTS.md 파일이 Codex의 크기 제한(project_doc_max_bytes)에서 잘렸습니다.",
                "AGENTS.md files were cut off by Codex's size limit (project_doc_max_bytes).",
            )
            .into(),
        );
    }
    items.extend(file_items(
        ContextKind::Agents,
        &root.join("agents"),
        ".toml",
        "",
        Loading::Conditional,
    ));
    items.extend(named_skill_items(&codex_skill_places(home, workspace)));
    for id in codex_enabled_plugins(&config) {
        match plugin_cache_directory(&root, &id) {
            Some(directory) => items.extend(skill_item(
                ContextKind::Plugins,
                &id,
                &directory.join("skills"),
                PLUGIN_CACHE,
            )),
            None => warnings.push(plugin_missing(&id)),
        }
    }
    let hooks = read_json(&root.join("hooks.json")).unwrap_or(Value::Null);
    hook_item(count_hooks(hooks.get("hooks")), &mut items, &mut warnings);
    let servers = codex_enabled_mcp_servers(&config)
        .into_iter()
        .map(|name| (name, Loading::Deferred))
        .collect();
    mcp_items(servers, &mut items, &mut warnings);

    // base_instructions는 Codex 내장 프롬프트라 AGENTS.md·skill 추정 항목과 겹치지 않는다(0005).
    let sent = codex_session::latest(home, workspace, tuning.codex_sessions_to_search);
    if let Some(sent) = &sent {
        if workspace.is_none() {
            warnings.push(global_session_note("Codex", sent.cwd.as_deref()));
        }
        if let Some(bytes) = sent.system_bytes {
            items.push(ContextItem::measured(
                ContextKind::System,
                pick("시스템 지시문", "System instructions").into(),
                bytes,
            ));
        }
    }
    let mut context = AgentContext::new(Agent::Codex, items, warnings, tuning.bytes_per_token);
    if let Some(sent) = sent {
        context.first_request_tokens = sent.first_request_tokens;
        context.model = sent.model;
        context.context_window = sent.context_window;
    }
    context
}

// ---------- Kiro ----------

fn kiro_hook_count(home: &Path, workspace: Option<&Path>) -> usize {
    let mut directories = vec![paths::kiro_root(home).join("hooks")];
    directories.extend(workspace.map(|w| w.join(".kiro").join("hooks")));
    // 작업 폴더가 홈이면 두 경로가 같은 폴더다.
    let unique: HashSet<PathBuf> = directories
        .iter()
        .filter_map(|d| std::fs::canonicalize(d).ok())
        .collect();
    unique
        .iter()
        .flat_map(|d| fsutil::walk_files(d, &|p| p.extension() == Some("json".as_ref())))
        .filter_map(|f| read_json(&f)?.get("hooks")?.as_array().map(Vec::len))
        .sum()
}

/// Kiro `mcp.json` `mcpServers`의 서버(객체인 것) 전부와 켜짐 여부(`"disabled": true`면 꺼짐).
pub(crate) fn kiro_mcp_states(mcp: &Value) -> Vec<(String, bool)> {
    object(mcp.get("mcpServers"))
        .into_iter()
        .filter(|(_, v)| v.is_object())
        .map(|(name, v)| {
            let on = v.get("disabled") != Some(&Value::Bool(true));
            (name, on)
        })
        .collect()
}

pub fn kiro(home: &Path, workspace: Option<&Path>, tuning: &Tuning) -> AgentContext {
    let root = paths::kiro_root(home);
    let mut items = instructions::kiro(home, workspace).items;
    let mut warnings = Vec::new();
    // 같은 이름이 둘 다 실리는지는 문서에 없다. "workspace 우선"을 따라 한 번만 센다(추정, 0005).
    let places = [
        (
            "workspace-skills",
            workspace
                .map(|w| w.join(".kiro").join("skills"))
                .into_iter()
                .collect(),
        ),
        ("user-skills", vec![root.join("skills")]),
    ];
    items.extend(named_skill_items(&places));
    // custom agent는 고를 때만 들어간다.
    items.extend(file_items(
        ContextKind::Agents,
        &root.join("agents"),
        ".json",
        "",
        Loading::Conditional,
    ));
    hook_item(kiro_hook_count(home, workspace), &mut items, &mut warnings);

    let mut settings = object(read_json(&root.join("settings").join("cli.json")).as_ref());
    if let Some(w) = workspace {
        settings.extend(object(
            read_json(&w.join(".kiro").join("settings").join("cli.json")).as_ref(),
        ));
    }
    let mode = loading::kiro(&settings);
    let mcp = read_json(&root.join("settings").join("mcp.json")).unwrap_or(Value::Null);
    let servers = kiro_mcp_states(&mcp)
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(name, _)| (name, mode))
        .collect();
    mcp_items(servers, &mut items, &mut warnings);

    let powers = fsutil::walk_files(&root.join("powers"), &|p| {
        p.file_name() == Some("plugin.json".as_ref())
    })
    .len();
    if powers > 0 {
        let noun = plural(powers, "power", "powers");
        items.push(ContextItem::unknown_size(
            ContextKind::Plugins,
            tr!("power {powers}개", "{powers} {noun}"),
            Loading::Deferred,
        ));
    }
    let sent = kiro_session::latest(home, workspace, tuning.kiro_sessions_to_search);
    if let Some(sent) = &sent
        && workspace.is_none()
    {
        warnings.push(global_session_note("Kiro", sent.cwd.as_deref()));
    }
    let mut context = AgentContext::new(Agent::Kiro, items, warnings, tuning.bytes_per_token);
    context.first_request_chars = sent.map(|s| s.first_request_chars);
    context
}

#[cfg(test)]
mod tests {
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
        let is_old = |w: &String| {
            w.contains("needs Claude Code 2.1.300+") || w.contains("2.1.300 이상 필요")
        };
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
            &format!(r#"{{"projects": {{"{key}": {{"enabledMcpjsonServers": ["near", "far", "here"]}}}}}}"#),
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
}
