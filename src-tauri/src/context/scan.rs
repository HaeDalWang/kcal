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
        "실측값은 가장 최근 {agent} 세션(작업 폴더 {folder}) 기준이에요.",
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
        "hook {count}개: 출력 크기는 실행할 때 정해져 세지 않았어요.",
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
        "MCP 서버 {count}개: 도구 스키마는 서버를 실행해야 알 수 있어 세지 않았어요.",
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
        "플러그인 {id}: 캐시 폴더가 없어 세지 않았어요.",
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
        "이 작업 폴더({})의 Claude 세션 기록이 없어 {} 그 프로젝트의 MCP 도구가 포함돼요.",
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
                    "{} 그 프로젝트의 MCP 도구가 포함돼요.",
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
                        "아직 Claude 세션이 없어서 시스템 프롬프트와 내장 도구는 세지 않았어요. 세션을 한 번 시작한 뒤 새로고침해 주세요.",
                        "No Claude session found yet, so the system prompt and built-in tools aren't counted. Start a session, then press Refresh.",
                    )
                    .into(),
                );
            } else {
                warnings.push(
                    pick(
                        "최근 Claude 세션에서 시스템 프롬프트 크기가 보고되지 않아 시스템 프롬프트·도구는 세지 않았어요(Claude Code 2.1.300 이상 필요). 업데이트한 뒤 새 세션을 시작하고 새로고침해 주세요.",
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

#[path = "scan_codex.rs"]
mod codex_scan;
pub use codex_scan::codex;
#[cfg(test)]
use codex_scan::{codex_enabled_mcp_servers, codex_enabled_plugins};
pub(crate) use codex_scan::{codex_mcp_server_states, codex_plugin_states};

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
#[path = "scan_tests.rs"]
mod tests;
