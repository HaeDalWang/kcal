//! 진단하기 사용 기록(docs/decisions/0022): 최근 `diagnoseUsageDays`일 Claude 세션 기록에서 플러그인·MCP를 부른 횟수를
//! 세션을 시작한 폴더(그 기록 파일의 첫 `cwd`)별로 센다. 줄의 `cwd`는 세션 안 `cd`를 따라 바뀌어 쓰지 않는다.
//!
//! 세는 것: `assistant` 줄의 `tool_use` 이름 `mcp__<서버>__*`(플러그인 MCP는 `mcp__plugin_<이름>_<서버>__*`),
//! `Skill` `input.skill`·`Agent`/`Task` `input.subagent_type`의 `<플러그인 이름>:`, `user` 줄
//! `<command-name>/<플러그인 이름>:…</command-name>`. 이름과 횟수만 들고 있고 내용 문장은 남기지 않는다.
//! 접두사 없는 `/<이름>`·`Skill` 이름은 개인·그 프로젝트의 `skills/<이름>/SKILL.md`가 있으면 그 스킬 사용이다. 없으면(`/<이름>`만) 설치된 플러그인(`installed_plugins.json`)의 `skills/`·`commands/`에서 그 이름이 정확히 한 플러그인에만 있고
//! 사용자·그 프로젝트의 `.claude/skills`·`commands`에 같은 이름이 없을 때만 그 플러그인 사용으로 센다.
//! `api-request-blob` 줄은 보낸 요청을 다시 적은 것이라 세지 않는다. 같은 호출은 `tool_use.id`(명령은 줄 `uuid`)로 한 번만.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::contents::Parts;
use crate::cache::Cache;
use crate::context::{fsutil, paths};
use crate::error::AppError;
use crate::metrics::{DAY_SECS, claude, unix_secs};
use crate::tuning::Tuning;

/// 화면에 이름을 보일 프로젝트 수. 나머지는 `projectCount`로만 센다.
pub const TOP_PROJECTS: usize = 3;
const MIB: u64 = 1024 * 1024;

#[path = "usage_codex.rs"]
mod codex;
pub(super) use codex::server_key as codex_server_key;

static USE_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""tool_use"|<command-name>"#).unwrap());
/// `api-request`·`api-request-blob` 줄의 시작(요청 본문의 다시 적기라 건너뛴다).
const REQUEST_LINE: &[u8] = br#"{"type":"api-request"#;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    /// 도구 이름의 `mcp__<서버>__`.
    Server(String),
    /// 스킬·명령·에이전트의 `<플러그인 이름>:`.
    Plugin(String),
    /// 접두사 없는 `/<이름>`·`Skill` 입력. 읽는 동안에만 있고(`BareNames`가 스킬·플러그인으로 바꾸거나 버린다) 집계에는 안 남는다.
    /// `plugins`: 플러그인 사용으로도 풀어도 되는가(`<command-name>`만, `Skill` 입력은 스킬로만).
    Bare { name: String, plugins: bool },
    /// 개인(`~/.claude/skills/<이름>/`)·프로젝트(`<폴더>/.claude/skills/<이름>/`) 스킬. 개인 스킬은 모든 폴더 합계,
    /// 프로젝트 스킬은 그 폴더 값만 쓴다.
    Skill(String),
}

/// 프로젝트 하나에서의 한 키 사용: 횟수와 마지막 사용 시각(초).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Stat {
    count: u32,
    last: f64,
}

/// 최근 기록 집계. 프로젝트 = 세션을 시작한 폴더(실제 경로).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageIndex {
    pub days: u64,
    /// 파일 수·크기 상한 때문에 기간 안 기록을 다 읽지 못했다.
    pub is_partial: bool,
    /// 기간 안에 읽은 세션 기록 수(하위 에이전트 기록 `subagents/`는 뺀다). 0017: 근거가 되는 양.
    pub sessions: usize,
    /// 이 수 미만이면 0회여도 "안 쓴다"는 근거가 아니다(tuning `diagnoseMinSessions`).
    pub min_sessions: usize,
    /// tuning `scopeMinUses`.
    pub scope_min_uses: usize,
    counts: HashMap<Key, HashMap<PathBuf, Stat>>,
    /// 시작 폴더별 기간 안 세션 수(하위 에이전트 기록 제외). 프로젝트 스킬의 근거 양이다.
    project_sessions: HashMap<PathBuf, usize>,
    /// 기간 안 세션을 시작한 폴더 전부(쓴 것이 없어도).
    pub projects: BTreeSet<PathBuf>,
    /// Codex rollout에서 센 MCP·플러그인 사용(0022 Codex 갱신). 같은 구조를 따로 쓴다(세션 수·프로젝트가 에이전트마다 다르다).
    pub codex: Option<Box<UsageIndex>>,
}

/// 스위치 하나의 사용 횟수. TS 짝은 `ipc.ts` `SwitchUsage`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchUsage {
    pub days: u64,
    pub total: u32,
    /// 많이 쓴 순 위 `TOP_PROJECTS`곳.
    pub projects: Vec<ProjectUsage>,
    /// 쓴 프로젝트 수 전체.
    pub project_count: usize,
    /// 기록을 일부만 봤다(0회여도 근거가 아니다).
    pub is_partial: bool,
    /// 기간 안에 읽은 Claude 세션 수. 적으면 0회여도 "안 쓴다"는 근거가 아니다.
    pub sessions: usize,
    /// 0회를 근거로 보려면 필요한 최소 세션 수(tuning).
    pub min_sessions: usize,
    /// "프로젝트에서만 켜기"를 제안하는 데 필요한 최소 사용 횟수(tuning `scopeMinUses`).
    pub scope_min_uses: usize,
    /// 가장 최근 사용 시각(유닉스 초). 한 번도 안 썼으면 없음.
    pub last_used: Option<u64>,
    /// 가장 최근에 쓴 프로젝트의 폴더 이름.
    pub last_project: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUsage {
    /// 폴더 이름(화면용).
    pub name: String,
    pub path: String,
    pub count: u32,
    /// 폴더가 지금 있는가.
    pub exists: bool,
}

pub fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl UsageIndex {
    fn summary(&self, per_project: HashMap<PathBuf, Stat>) -> SwitchUsage {
        let mut projects: Vec<(PathBuf, Stat)> = per_project.into_iter().collect();
        projects.sort_by(|a, b| b.1.count.cmp(&a.1.count).then_with(|| a.0.cmp(&b.0)));
        let latest = projects
            .iter()
            .max_by(|a, b| a.1.last.total_cmp(&b.1.last).then_with(|| b.0.cmp(&a.0)));
        SwitchUsage {
            days: self.days,
            total: projects.iter().map(|p| p.1.count).sum(),
            last_used: latest.map(|(_, s)| s.last as u64),
            last_project: latest.map(|(path, _)| folder_name(path)),
            scope_min_uses: self.scope_min_uses,
            project_count: projects.len(),
            projects: projects
                .into_iter()
                .take(TOP_PROJECTS)
                .map(|(path, stat)| ProjectUsage {
                    name: folder_name(&path),
                    exists: path.is_dir(),
                    path: path.to_string_lossy().into_owned(),
                    count: stat.count,
                })
                .collect(),
            is_partial: self.is_partial,
            sessions: self.sessions,
            min_sessions: self.min_sessions,
        }
    }

    fn add(into: &mut HashMap<PathBuf, Stat>, from: Option<&HashMap<PathBuf, Stat>>) {
        for (path, stat) in from.into_iter().flatten() {
            let merged = into.entry(path.clone()).or_default();
            merged.count += stat.count;
            merged.last = merged.last.max(stat.last);
        }
    }

    /// MCP 서버 하나(도구 이름에 쓰는 서버 이름).
    pub fn for_server(&self, server: &str) -> SwitchUsage {
        let mut per_project = HashMap::new();
        Self::add(
            &mut per_project,
            self.counts.get(&Key::Server(server.to_owned())),
        );
        self.summary(per_project)
    }

    /// Codex plugin ID 전체로 집계한다(마켓이 다른 같은 이름을 합치지 않는다).
    pub fn for_codex_plugin(&self, id: &str) -> SwitchUsage {
        self.summary(
            self.counts
                .get(&Key::Plugin(id.to_owned()))
                .cloned()
                .unwrap_or_default(),
        )
    }

    /// 개인 스킬(`project` 없음, 모든 폴더 합계) 또는 그 폴더의 프로젝트 스킬. 프로젝트 스킬은 세션 수도 그 폴더 것만 센다
    /// (다른 폴더에서 안 쓴 것은 이 스킬을 안 쓴다는 근거가 아니다).
    pub fn for_skill(&self, name: &str, project: Option<&Path>) -> SwitchUsage {
        let mut per_project = HashMap::new();
        Self::add(
            &mut per_project,
            self.counts.get(&Key::Skill(name.to_owned())),
        );
        if let Some(only) = project {
            per_project.retain(|path, _| path == only);
        }
        let mut usage = self.summary(per_project);
        if let Some(only) = project {
            usage.sessions = self.project_sessions.get(only).copied().unwrap_or(0);
        }
        usage
    }

    /// 플러그인 `<이름>@<마켓>`: 스킬·명령·에이전트 + 그 플러그인의 MCP 도구. `names`는 목록의 플러그인 이름 전부로,
    /// `mcp__plugin_<이름>_`이 더 긴 이름의 것과 겹치면 긴 쪽에 붙인다.
    pub fn for_plugin(&self, id: &str, names: &[&str]) -> SwitchUsage {
        let name = plugin_name(id);
        let is_from = |server: &str, n: &str| server.starts_with(&format!("plugin_{n}_"));
        let mut per_project = HashMap::new();
        for (key, counts) in &self.counts {
            let is_mine = match key {
                Key::Plugin(p) => p == name,
                Key::Bare { .. } | Key::Skill(_) => false,
                Key::Server(s) => {
                    is_from(s, name) && !names.iter().any(|n| n.len() > name.len() && is_from(s, n))
                }
            };
            if is_mine {
                Self::add(&mut per_project, Some(counts));
            }
        }
        self.summary(per_project)
    }
}

/// `context7@claude-plugins-official` → `context7`.
pub fn plugin_name(id: &str) -> &str {
    id.split_once('@').map_or(id, |(name, _)| name)
}

#[derive(Deserialize)]
struct Row {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    uuid: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<Content>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<Input>,
    text: Option<Value>,
}

/// 다른 도구 입력은 이 두 키만 읽는다(값이 문자열이 아니어도 줄을 버리지 않게 `Value`).
#[derive(Deserialize)]
struct Input {
    skill: Option<Value>,
    subagent_type: Option<Value>,
}

#[derive(Deserialize)]
struct CwdRow {
    cwd: Option<String>,
}

/// `plugin:skill`, `/plugin:cmd`, `/mcp__server__prompt`.
fn command_key(text: &str) -> Option<Key> {
    let text = text.trim().trim_start_matches('/');
    if let Some(rest) = text.strip_prefix("mcp__") {
        let server = rest.split("__").next().filter(|s| !s.is_empty())?;
        return Some(Key::Server(server.to_owned()));
    }
    let (plugin, rest) = text.split_once(':')?;
    (!plugin.is_empty() && !rest.is_empty()).then(|| Key::Plugin(plugin.to_owned()))
}

/// 접두사 없는 이름. 공백·`:`·`/`가 든 것은 이름이 아니다.
fn bare_name(name: &str) -> Option<&str> {
    let is_name = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    is_name.then_some(name)
}

/// 접두사 없는 `/<이름>`(`<command-name>`에서만).
fn bare_key(text: &str) -> Option<Key> {
    let name = bare_name(text.trim().strip_prefix('/')?)?;
    Some(Key::Bare {
        name: name.to_owned(),
        plugins: true,
    })
}

/// 설치된 플러그인의 `skills/<이름>/`·`commands/<이름>.md` 이름 → 가진 플러그인 id들.
struct BareNames {
    claude_root: PathBuf,
    owners: HashMap<String, BTreeSet<String>>,
}

impl BareNames {
    fn load(home: &Path) -> Self {
        let claude_root = paths::claude_root(home);
        let installed =
            fsutil::read_json(&claude_root.join("plugins").join("installed_plugins.json"));
        let mut owners: HashMap<String, BTreeSet<String>> = HashMap::new();
        let ids = installed
            .as_ref()
            .and_then(|v| v.get("plugins")?.as_object())
            .into_iter()
            .flat_map(|plugins| plugins.keys());
        for id in ids {
            let Some(parts) = Parts::read(&claude_root, id) else {
                continue;
            };
            for name in parts.skills.into_iter().chain(parts.commands) {
                owners.entry(name).or_default().insert(id.clone());
            }
        }
        Self {
            claude_root,
            owners,
        }
    }

    /// 이 이름의 개인 스킬이 있거나 그 프로젝트의 스킬이 있으면 그 스킬 사용이다.
    ///
    /// Ruling: 같은 이름이 플러그인 스킬에도 있으면 스킬이 이긴다 — 접두사 없는 이름은 스킬 자신의 이름이고 플러그인 스킬은
    /// `<플러그인>:` 이름공간이라(0017 사실표), 예전에는 겹치면 아무것도 세지 않았다. 스킬이 있는 이름은 센 쪽이 숨기기 제안을 덜
    /// 하는 안전한 쪽이다 — 틀리면 플러그인 사용이 그 스킬 사용으로 잡혀 제안이 하나 줄 뿐이다.
    /// 스킬이 없으면(`plugins`일 때만) 이름이 정확히 한 플러그인에만 있고 사용자·프로젝트의 같은 이름 명령이 없을 때 그 플러그인.
    fn resolve(&self, name: &str, project: &Path, plugins: bool) -> Option<Key> {
        let roots = [&self.claude_root, &project.join(".claude")];
        if roots
            .iter()
            .any(|root| root.join("skills").join(name).join("SKILL.md").is_file())
        {
            return Some(Key::Skill(name.to_owned()));
        }
        if !plugins {
            return None;
        }
        let mut ids = self.owners.get(name)?.iter();
        let (id, None) = (ids.next()?, ids.next()) else {
            return None;
        };
        let is_shadowed = roots.iter().any(|root| {
            root.join("skills").join(name).exists()
                || root.join("commands").join(format!("{name}.md")).exists()
        });
        (!is_shadowed).then(|| Key::Plugin(plugin_name(id).to_owned()))
    }
}

fn tool_key(block: &Block) -> Option<Key> {
    let name = block.name.as_deref()?;
    if name.starts_with("mcp__") {
        return command_key(name);
    }
    let input = block.input.as_ref()?;
    let value = match name {
        "Skill" => input.skill.as_ref(),
        "Agent" | "Task" => input.subagent_type.as_ref(),
        _ => None,
    }?;
    let text = value.as_str()?;
    match command_key(text) {
        Some(key @ Key::Plugin(_)) => Some(key),
        Some(_) => None,
        // `Skill` 입력의 접두사 없는 이름은 개인·프로젝트 스킬로만 푼다(플러그인 스킬은 늘 접두사가 붙는다).
        None if name == "Skill" => Some(Key::Bare {
            name: bare_name(text.trim().trim_start_matches('/'))?.to_owned(),
            plugins: false,
        }),
        None => None,
    }
}

fn command_keys(content: &Content) -> Vec<Key> {
    let texts: Vec<&str> = match content {
        Content::Text(text) => vec![text.as_str()],
        Content::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| b.text.as_ref()?.as_str())
            .collect(),
    };
    texts
        .iter()
        .flat_map(|t| t.split("<command-name>").skip(1))
        .filter_map(|s| {
            let text = s.split_once("</command-name>")?.0;
            command_key(text).or_else(|| bare_key(text))
        })
        .collect()
}

/// 한 줄의 사용: 키, 중복 확인용 ID, 줄 시각(초).
type Use = (Key, Option<String>, f64);

/// 한 줄의 사용. `since`보다 오래된 줄은 빈 목록.
fn line_uses(line: &[u8], since: f64) -> Vec<Use> {
    if line.starts_with(REQUEST_LINE) || !USE_MARK.is_match(line) {
        return Vec::new();
    }
    let Ok(row) = serde_json::from_slice::<Row>(line) else {
        return Vec::new();
    };
    let at = row
        .timestamp
        .as_deref()
        .and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok())
        .map(unix_secs)
        .filter(|&at| at >= since);
    let Some((content, at)) = row.message.and_then(|m| m.content).zip(at) else {
        return Vec::new();
    };
    match (row.kind.as_deref(), content) {
        (Some("assistant"), Content::Blocks(blocks)) => blocks
            .into_iter()
            .filter(|b| b.kind.as_deref() == Some("tool_use"))
            .filter_map(|b| Some((tool_key(&b)?, b.id, at)))
            .collect(),
        (Some("user"), content) => command_keys(&content)
            .into_iter()
            .enumerate()
            .map(|(i, k)| (k, row.uuid.as_ref().map(|u| format!("{u}#{i}")), at))
            .collect(),
        _ => Vec::new(),
    }
}

/// 읽은 기록의 시작 폴더(첫 `cwd`). 없으면 `None`.
fn read_file(
    file: &Path,
    since: f64,
    bare: &BareNames,
    seen: &mut HashSet<String>,
    index: &mut UsageIndex,
) -> Option<PathBuf> {
    let first_cwd = RefCell::new(None::<String>);
    let uses: Vec<Use> = claude::read_lines(file, |line| {
        if first_cwd.borrow().is_none() {
            let cwd = serde_json::from_slice::<CwdRow>(line)
                .ok()
                .and_then(|r| r.cwd);
            *first_cwd.borrow_mut() = cwd.filter(|c| !c.is_empty());
        }
        Some(line_uses(line, since))
    })
    .into_iter()
    .flatten()
    .collect();
    let project = first_cwd.into_inner().map(PathBuf::from)?;
    for (key, id, at) in uses {
        if id.is_some_and(|id| !seen.insert(id)) {
            continue;
        }
        let key = match key {
            Key::Bare { name, plugins } => match bare.resolve(&name, &project, plugins) {
                Some(key) => key,
                None => continue,
            },
            key => key,
        };
        let stat = index
            .counts
            .entry(key)
            .or_default()
            .entry(project.clone())
            .or_default();
        stat.count += 1;
        stat.last = stat.last.max(at);
    }
    index.projects.insert(project.clone());
    Some(project)
}

/// 최근 `diagnoseUsageDays`일에 바뀐 기록을 최근 순으로 `diagnoseUsageFiles`개, 합계 `diagnoseUsageMaxMib`까지 읽는다.
pub fn read(home: &Path, tuning: &Tuning, now: f64) -> UsageIndex {
    let since = now - tuning.diagnose_usage_days as f64 * DAY_SECS;
    let limit = tuning.diagnose_usage_files;
    let files = claude::log_files(home, since, limit.saturating_add(1));
    let budget = tuning.diagnose_usage_max_mib.saturating_mul(MIB);
    let mut index = UsageIndex {
        days: tuning.diagnose_usage_days,
        min_sessions: tuning.diagnose_min_sessions,
        scope_min_uses: tuning.scope_min_uses,
        is_partial: files.len() > limit,
        codex: Some(Box::new(codex::read(home, tuning, now))),
        ..UsageIndex::default()
    };
    let bare = BareNames::load(home);
    let mut seen = HashSet::new();
    let mut read_bytes = 0u64;
    for file in files.iter().take(limit) {
        let size = std::fs::metadata(file).map_or(0, |m| m.len());
        if read_bytes.saturating_add(size) > budget {
            index.is_partial = true;
            break;
        }
        read_bytes += size;
        let project = read_file(file, since, &bare, &mut seen, &mut index);
        if !file.components().any(|c| c.as_os_str() == "subagents") {
            index.sessions += 1;
            if let Some(project) = project {
                *index.project_sessions.entry(project).or_default() += 1;
            }
        }
    }
    index
}

/// `read`의 마지막 결과. 점검 탭이 부를 때만 계산한다(타이머 없음). TTL은 tuning `metricsTtlSecs`.
pub struct UsageState(Cache<UsageIndex>);

impl Default for UsageState {
    fn default() -> Self {
        Self(Cache::new())
    }
}

fn now_secs() -> f64 {
    unix_secs(OffsetDateTime::now_utc())
}

pub async fn index(app: &AppHandle, home: &Path, tuning: Tuning) -> Result<UsageIndex, AppError> {
    let cache = app.state::<UsageState>().0.clone();
    let owned = home.to_path_buf();
    let read_tuning = tuning.clone();
    let cached = cache
        .get(
            tuning.metrics_ttl(),
            move || read(&owned, &read_tuning, now_secs()),
            |_| {},
        )
        .await?;
    if cached.days == tuning.diagnose_usage_days {
        return Ok(cached);
    }
    // 기간 설정을 바꾼 직후: 이번 응답은 바로 새 기간으로 읽는다(캐시는 뒤에서 다시 계산 중).
    let owned = home.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || read(&owned, &tuning, now_secs()))
        .await
        .map_err(|_| AppError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;

    const NOW: &str = "2026-11-13T00:00:00Z";

    fn now() -> f64 {
        unix_secs(OffsetDateTime::parse(NOW, &Rfc3339).unwrap())
    }

    fn assistant(cwd: &str, ts: &str, tools: &[(&str, &str, Value)]) -> String {
        let content: Vec<Value> = tools
            .iter()
            .map(|(id, name, input)| json!({"type": "tool_use", "id": id, "name": name, "input": input}))
            .chain([json!({"type": "text", "text": "secret body"})])
            .collect();
        json!({"type": "assistant", "cwd": cwd, "timestamp": ts, "message": {"content": content}})
            .to_string()
    }

    fn user_command(cwd: &str, ts: &str, uuid: &str, command: &str) -> String {
        json!({"type": "user", "cwd": cwd, "timestamp": ts, "uuid": uuid,
            "message": {"content": format!("<command-name>{command}</command-name>\n<command-args>secret</command-args>")}})
        .to_string()
    }

    fn fixture(home: &TempDir) {
        let a = home.path().join("work/a");
        let a = a.to_str().unwrap();
        let b = home.path().join("work/b");
        let b = b.to_str().unwrap();
        let recent = "2026-11-12T10:00:00Z";
        let session_a = [
            json!({"type": "permission-mode", "permissionMode": "default"}).to_string(),
            user_command(a, recent, "u1", "/kit:deploy"),
            user_command(a, recent, "u2", "/clear"),
            assistant(
                a,
                recent,
                &[
                    ("t1", "mcp__plugin_kit_srv__go", json!({})),
                    ("t2", "Skill", json!({"skill": "kit:review"})),
                    ("t3", "Skill", json!({"skill": "plain-skill"})),
                    ("t4", "mcp__claude_ai_Slack__post", json!({"text": "x"})),
                    ("t5", "Agent", json!({"subagent_type": "kit:helper"})),
                    ("t6", "Bash", json!({"command": "ls"})),
                ],
            ),
            // `cd` 뒤 줄의 cwd는 바뀌어도 시작 폴더(a)에 붙인다.
            assistant(
                "/tmp/elsewhere",
                recent,
                &[("t7", "mcp__plugin_kit-pro_srv__go", json!({}))],
            ),
            // 요청 다시 적기 줄은 세지 않는다.
            format!(
                r#"{{"type":"api-request-blob","cwd":"{a}","timestamp":"{recent}","message":{{"content":[{{"type":"tool_use","id":"t9","name":"mcp__plugin_kit_srv__go"}}]}}}}"#
            ),
            // 기간 밖.
            assistant(
                a,
                "2026-09-01T00:00:00Z",
                &[("t8", "mcp__plugin_kit_srv__go", json!({}))],
            ),
            // 이어 하기로 다시 적힌 같은 호출(t1)은 한 번만.
            assistant(a, recent, &[("t1", "mcp__plugin_kit_srv__go", json!({}))]),
            "{broken".into(),
        ];
        home.write(
            ".claude/projects/-work-a/s1.jsonl",
            &(session_a.join("\n") + "\n"),
        );
        home.write(
            ".claude/projects/-work-b/s2.jsonl",
            &assistant(b, recent, &[("t10", "mcp__plugin_kit_srv__go", json!({}))]),
        );
    }

    #[test]
    fn counts_plugin_and_mcp_uses_per_session_start_folder() {
        let home = TempDir::new("coach-usage-count");
        fixture(&home);
        std::fs::create_dir_all(home.path().join("work/a")).unwrap();
        let index = read(home.path(), &Tuning::default(), now());
        assert!(!index.is_partial);
        assert_eq!(index.sessions, 2);
        let names = ["kit", "kit-pro"];
        let kit = index.for_plugin("kit@m", &names);
        // a: 명령 1 + MCP 1(t1, 다시 적힌 줄은 빼고) + 스킬 1 + 에이전트 1, b: MCP 1.
        assert_eq!((kit.total, kit.project_count), (5, 2));
        assert_eq!(
            kit.projects
                .iter()
                .map(|p| (p.name.as_str(), p.count, p.exists))
                .collect::<Vec<_>>(),
            [("a", 4, true), ("b", 1, false)]
        );
        // 더 긴 이름의 플러그인 MCP는 그쪽에 붙고, 시작 폴더 a에 센다.
        let pro = index.for_plugin("kit-pro@m", &names);
        assert_eq!((pro.total, pro.projects[0].name.as_str()), (1, "a"));
        assert_eq!(index.for_server("claude_ai_Slack").total, 1);
        assert_eq!(index.for_server("nothing").total, 0);
        assert_eq!(index.for_plugin("plain-skill@m", &names).total, 0);
        assert_eq!(index.projects.len(), 2);
    }

    #[test]
    fn reports_the_last_use_time_and_project() {
        let home = TempDir::new("coach-usage-last");
        let (a, b) = (home.path().join("work/a"), home.path().join("work/b"));
        let line = |cwd: &Path, ts: &str, id: &str| {
            assistant(
                cwd.to_str().unwrap(),
                ts,
                &[(id, "Skill", json!({"skill": "kit:review"}))],
            )
        };
        let early = line(&a, "2026-11-05T01:00:00Z", "t1");
        let late = line(&b, "2026-11-10T02:00:00Z", "t2");
        let older_in_a = line(&a, "2026-11-02T00:00:00Z", "t3");
        home.write(
            ".claude/projects/-a/s1.jsonl",
            &format!("{early}\n{older_in_a}\n"),
        );
        home.write(".claude/projects/-b/s2.jsonl", &late);
        let index = read(home.path(), &Tuning::default(), now());
        let kit = index.for_plugin("kit@m", &["kit"]);
        let at = |t: &str| unix_secs(OffsetDateTime::parse(t, &Rfc3339).unwrap()) as u64;
        assert_eq!(kit.total, 3);
        assert_eq!(kit.last_used, Some(at("2026-11-10T02:00:00Z")));
        assert_eq!(kit.last_project.as_deref(), Some("b"));
        assert_eq!(kit.scope_min_uses, 3);
        let none = index.for_plugin("nope@m", &["nope"]);
        assert_eq!((none.last_used, none.last_project), (None, None));
    }

    /// 설치된 플러그인 `one`(skills/solo, commands/only-cmd, skills/both)과 `two`(skills/both).
    fn bare_fixture(home: &TempDir) {
        home.write(
            ".claude/plugins/installed_plugins.json",
            &json!({"version": 2, "plugins": {"one@m": [], "two@m": []}}).to_string(),
        );
        for (plugin, rel) in [
            ("one", "skills/solo/SKILL.md"),
            ("one", "commands/only-cmd.md"),
            ("one", "skills/both/SKILL.md"),
            ("one", "skills/mine/SKILL.md"),
            ("one", "skills/local/SKILL.md"),
            ("two", "skills/both/SKILL.md"),
        ] {
            home.write(
                &format!(".claude/plugins/cache/m/{plugin}/1.0.0/{rel}"),
                "x",
            );
        }
        // 사용자·프로젝트가 같은 이름을 가졌다.
        home.write(".claude/skills/mine/SKILL.md", "x");
        home.write("work/a/.claude/skills/local/SKILL.md", "x");
        let a = home.path().join("work/a");
        let a = a.to_str().unwrap();
        let recent = "2026-11-12T10:00:00Z";
        let commands = [
            "/solo",
            "/only-cmd",
            "/both",
            "/mine",
            "/local",
            "/clear",
            "/solo",
        ];
        let lines: Vec<String> = commands
            .iter()
            .enumerate()
            .map(|(i, c)| user_command(a, recent, &format!("u{i}"), c))
            .collect();
        home.write(
            ".claude/projects/-work-a/s.jsonl",
            &(lines.join("\n") + "\n"),
        );
    }

    #[test]
    fn counts_unprefixed_commands_only_for_the_single_owning_plugin() {
        let home = TempDir::new("coach-usage-bare");
        bare_fixture(&home);
        let index = read(home.path(), &Tuning::default(), now());
        let names = ["one", "two"];
        // /solo 2번 + /only-cmd 1번만. 두 플러그인에 있는 /both, 사용자·프로젝트 스킬과 겹치는 /mine·/local, 아무도 안 가진 /clear는 안 센다.
        assert_eq!(index.for_plugin("one@m", &names).total, 3);
        assert_eq!(index.for_plugin("two@m", &names).total, 0);
    }

    #[test]
    fn unprefixed_commands_count_skills_a_marketplace_entry_points_to() {
        let home = TempDir::new("coach-usage-bare-market");
        home.write(
            ".claude/plugins/installed_plugins.json",
            &json!({"version": 2, "plugins": {"kit@m": []}}).to_string(),
        );
        home.write(
            ".claude/plugins/marketplaces/m/.claude-plugin/marketplace.json",
            &json!({"plugins": [{"name": "kit", "strict": false, "skills": ["./alpha-skill"]}]})
                .to_string(),
        );
        home.write(
            ".claude/plugins/cache/m/kit/1.0.0/alpha-skill/SKILL.md",
            "x",
        );
        let a = home.path().join("work/a");
        home.write(
            ".claude/projects/-work-a/s.jsonl",
            &user_command(
                a.to_str().unwrap(),
                "2026-11-12T10:00:00Z",
                "u1",
                "/alpha-skill",
            ),
        );
        let index = read(home.path(), &Tuning::default(), now());
        assert_eq!(index.for_plugin("kit@m", &["kit"]).total, 1);
    }

    #[test]
    fn unprefixed_commands_count_nothing_without_installed_plugins() {
        let home = TempDir::new("coach-usage-bare-none");
        let a = home.path().join("work/a");
        home.write(
            ".claude/projects/-work-a/s.jsonl",
            &user_command(a.to_str().unwrap(), "2026-11-12T10:00:00Z", "u1", "/solo"),
        );
        let index = read(home.path(), &Tuning::default(), now());
        assert_eq!(index.for_plugin("one@m", &["one"]).total, 0);
    }

    #[test]
    fn file_and_size_caps_mark_the_index_partial() {
        let home = TempDir::new("coach-usage-caps");
        fixture(&home);
        let one_file = Tuning {
            diagnose_usage_files: 1,
            ..Tuning::default()
        };
        let index = read(home.path(), &one_file, now());
        assert!(index.is_partial);
        assert!(index.for_plugin("kit@m", &["kit"]).is_partial);
        let roomy = Tuning {
            diagnose_usage_max_mib: 16,
            ..Tuning::default()
        };
        assert!(!read(home.path(), &roomy, now()).is_partial);
    }

    #[test]
    fn counts_sessions_but_not_subagent_logs() {
        let home = TempDir::new("coach-usage-sessions");
        let line = r#"{"type":"user","cwd":"/w"}"#;
        home.write(".claude/projects/-w/a.jsonl", line);
        home.write(".claude/projects/-w/b.jsonl", line);
        home.write(".claude/projects/-w/a/subagents/agent-1.jsonl", line);
        let index = read(home.path(), &Tuning::default(), now_secs());
        assert_eq!(index.sessions, 2);
    }

    #[test]
    fn reads_nothing_without_session_logs() {
        let home = TempDir::new("coach-usage-empty");
        let index = read(home.path(), &Tuning::default(), now());
        assert_eq!(index.for_plugin("kit@m", &["kit"]).total, 0);
        assert!(index.projects.is_empty() && !index.is_partial);
        assert_eq!(index.sessions, 0);
    }
}
