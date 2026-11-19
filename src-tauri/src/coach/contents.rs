//! "끄면 함께 꺼지는 것": 설치된 플러그인 폴더(`fsutil::plugin_cache_directory`)에 든 스킬·에이전트·명령·MCP 서버 이름.
//! 기본 폴더(`skills/`·`agents/`·`commands/`·`.mcp.json`)에 매니페스트(`.claude-plugin/plugin.json`과 마켓플레이스 항목)가
//! 가리키는 경로를 Claude Code plugins-reference "Path rules"대로 더하거나 대체한다.
//! 파일 내용은 읽지 않고 이름만 센다(JSON은 서버 이름 키만). 폴더를 못 찾거나 비었으면 `None`(화면은 줄을 숨긴다).
//! TS 짝은 `ipc.ts` `PluginContents`·`NameList`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::context::fsutil;

/// 종류마다 화면에 보일 이름 수. 나머지는 `count`로만 센다.
pub const SHOWN_NAMES: usize = 3;

/// 한 종류의 이름(이름순 앞 `SHOWN_NAMES`개)과 전체 개수.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameList {
    pub names: Vec<String>,
    pub count: usize,
}

impl NameList {
    fn of(all: &[String]) -> Self {
        Self {
            names: all.iter().take(SHOWN_NAMES).cloned().collect(),
            count: all.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContents {
    pub skills: NameList,
    pub agents: NameList,
    pub commands: NameList,
    pub mcp_servers: NameList,
}

/// 플러그인 폴더의 이름 전부(이름순). `usage::BareNames`도 스킬·명령 이름을 여기서 얻는다.
#[derive(Debug, Default, PartialEq)]
pub struct Parts {
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub commands: Vec<String>,
    pub mcp_servers: Vec<String>,
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
}

fn entries(dir: &Path) -> impl Iterator<Item = std::fs::DirEntry> {
    std::fs::read_dir(dir).into_iter().flatten().flatten()
}

/// `dir` 안 `*.md` 파일의 이름(확장자 뺌).
fn md_stems(dir: &Path) -> Vec<String> {
    let names = entries(dir)
        .filter_map(|e| {
            let path = e.path();
            if path.extension()? != "md" || !path.is_file() {
                return None;
            }
            Some(path.file_stem()?.to_string_lossy().into_owned())
        })
        .collect();
    sorted(names)
}

/// `.mcp.json`의 서버 이름. `{"mcpServers": {...}}`와 서버를 바로 둔 `{...}` 두 모양을 받는다.
fn mcp_server_names(file: &Path) -> Vec<String> {
    match fsutil::read_json(file) {
        Some(Value::Object(root)) => server_names(root),
        _ => Vec::new(),
    }
}

fn server_names(root: Map<String, Value>) -> Vec<String> {
    let servers = match root.get("mcpServers") {
        Some(Value::Object(inner)) => inner.clone(),
        _ => root,
    };
    servers
        .into_iter()
        .filter(|(_, v)| v.is_object())
        .map(|(k, _)| k)
        .collect()
}

/// 매니페스트 키 값(경로 하나·배열·맵)을 항목 목록으로 편다. 키가 없으면 `None`.
fn items(manifest: Option<&Map<String, Value>>, key: &str) -> Option<Vec<Value>> {
    match manifest?.get(key)? {
        Value::Array(list) => Some(list.clone()),
        Value::Null => None,
        one => Some(vec![one.clone()]),
    }
}

/// 두 매니페스트의 같은 키를 이어 붙인다. 어느 쪽에든 있으면 `Some`.
fn joined(first: Option<Vec<Value>>, second: Option<Vec<Value>>) -> Option<Vec<Value>> {
    match (first, second) {
        (None, None) => None,
        (a, b) => Some(a.into_iter().chain(b).flatten().collect()),
    }
}

/// 매니페스트 경로 하나를 플러그인 폴더(`root`, 실제 경로) 안의 실제 경로로. `./`로 시작해야 하고
/// `..`·절대경로·폴더 밖을 가리키는 링크는 버린다. `allow_root`면 `"."`도 받는다(`skills`).
fn resolve(root: &Path, raw: &str, allow_root: bool) -> Option<PathBuf> {
    if allow_root && raw == "." {
        return Some(root.to_path_buf());
    }
    let rest = raw.strip_prefix("./")?;
    if rest.split(['/', '\\']).any(|part| part == "..") || Path::new(rest).is_absolute() {
        return None;
    }
    let real = root.join(rest).canonicalize().ok()?;
    real.starts_with(root).then_some(real)
}

/// `skills` 경로 하나: `SKILL.md`를 바로 담은 폴더면 스킬 하나(루트면 플러그인 이름), 아니면 `<name>/SKILL.md` 폴더들.
/// (이름, `SKILL.md` 경로).
fn skills_in(root: &Path, dir: &Path, plugin_name: &str) -> Vec<(String, PathBuf)> {
    let file = dir.join("SKILL.md");
    if file.is_file() {
        let name = if dir == root {
            plugin_name.to_owned()
        } else {
            dir.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        };
        return vec![(name, file)];
    }
    entries(dir)
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                e.path().join("SKILL.md"),
            )
        })
        .filter(|(_, file)| file.is_file())
        .collect()
}

fn skill_folders(dir: &Path) -> Vec<String> {
    entries(dir)
        .filter(|e| e.path().join("SKILL.md").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

/// `.md` 파일 하나의 이름(확장자 뺌).
fn md_stem(path: &Path) -> Option<String> {
    (path.is_file() && path.extension()? == "md")
        .then(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))?
}

/// `commands` 항목: `.md` 파일·그 폴더 경로, 또는 이름 → `source`/`content` 맵(키가 이름).
fn commands_in(root: &Path, item: &Value) -> Vec<String> {
    match item {
        Value::String(raw) => match resolve(root, raw, false) {
            Some(path) if path.is_dir() => md_stems(&path),
            Some(path) => md_stem(&path).into_iter().collect(),
            None => Vec::new(),
        },
        Value::Object(map) => map
            .iter()
            .filter(|(_, v)| v.is_object())
            .map(|(k, _)| k.clone())
            .collect(),
        _ => Vec::new(),
    }
}

/// `mcpServers` 항목: `.json` 파일 경로 또는 인라인 맵. 번들(`.mcpb`·`.dxt`·URL)은 풀어 봐야 알아 세지 않는다.
fn servers_in(root: &Path, item: &Value) -> Vec<String> {
    match item {
        Value::String(raw) if raw.ends_with(".json") => {
            resolve(root, raw, false).map_or_else(Vec::new, |file| mcp_server_names(&file))
        }
        Value::Object(map) => server_names(map.clone()),
        _ => Vec::new(),
    }
}

/// Codex 플러그인 폴더의 MCP 서버 이름. 호환 매니페스트(`.codex-plugin/plugin.json`)에 `mcpServers`가 있으면 그것만
/// (이 Mac 설치본은 `"./.mcp.json"`), 없으면 이식형 고정 경로 루트 `mcp.json`(`plugins/build`). 매니페스트가 가리키지 않는
/// `.mcp.json`을 Codex가 읽는지는 문서에 없어 세지 않는다.
pub(crate) fn codex_plugin_mcp_servers(plugin: &Path) -> Vec<String> {
    let Ok(dir) = plugin.canonicalize() else {
        return Vec::new();
    };
    let manifest = fsutil::read_json(&dir.join(".codex-plugin").join("plugin.json"));
    let declared = manifest.as_ref().and_then(Value::as_object);
    let names = match items(declared, "mcpServers") {
        Some(list) => list.iter().flat_map(|v| servers_in(&dir, v)).collect(),
        None => mcp_server_names(&dir.join("mcp.json")),
    };
    sorted(names)
}

/// 마켓플레이스 파일(`<agent_root>/plugins/marketplaces/<mkt>/.claude-plugin/marketplace.json`)의 `plugins[]` 중 이름이 같은 항목.
/// 실제 경로가 `plugins/marketplaces` 밖이면 읽지 않는다.
fn marketplace_entry(
    agent_root: &Path,
    name: &str,
    marketplace: &str,
) -> Option<Map<String, Value>> {
    let markets = agent_root.join("plugins").join("marketplaces");
    let file = markets
        .join(marketplace)
        .join(".claude-plugin")
        .join("marketplace.json");
    let real = file.canonicalize().ok()?;
    if !real.starts_with(markets.canonicalize().ok()?) {
        return None;
    }
    let Value::Object(mut root) = fsutil::read_json(&real)? else {
        return None;
    };
    let Value::Array(plugins) = root.remove("plugins")? else {
        return None;
    };
    plugins.into_iter().find_map(|p| match p {
        Value::Object(entry) if entry.get("name").and_then(Value::as_str) == Some(name) => {
            Some(entry)
        }
        _ => None,
    })
}

/// 마켓플레이스 항목이 매니페스트에 더하는 구성요소 키(Claude Code plugins-reference "Strict mode").
const ENTRY_COMPONENT_KEYS: [&str; 6] = [
    "commands",
    "agents",
    "skills",
    "hooks",
    "outputStyles",
    "themes",
];

/// 플러그인 폴더의 `plugin.json`과 마켓플레이스 항목을 합친 구성요소 키.
/// `plugin.json`이 없으면 항목이 매니페스트다. 있으면 항목의 `commands`·`agents`·`skills`를 뒤에 붙이고(`strict` 기본),
/// `strict: false`인데 항목이 구성요소를 적었으면 Claude Code가 플러그인을 못 불러온다(`None`).
#[derive(Default)]
struct Manifest {
    skills: Option<Vec<Value>>,
    commands: Option<Vec<Value>>,
    agents: Option<Vec<Value>>,
    mcp_servers: Vec<Value>,
    /// 마켓플레이스 루트(`source` `"./"`)를 가리키는 항목이 `skills`를 적으면 기본 `skills/`를 읽지 않는다.
    skips_default_skills: bool,
}

impl Manifest {
    fn read(dir: &Path, entry: Option<&Map<String, Value>>) -> Option<Self> {
        let own = match fsutil::read_json(&dir.join(".claude-plugin").join("plugin.json")) {
            Some(Value::Object(map)) => Some(map),
            _ => None,
        };
        let is_strict = entry
            .and_then(|e| e.get("strict"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let declares =
            |e: &Map<String, Value>| ENTRY_COMPONENT_KEYS.iter().any(|k| e.contains_key(*k));
        if own.is_some() && !is_strict && entry.is_some_and(declares) {
            return None;
        }
        let (base, extra) = match &own {
            Some(own) => (Some(own), entry.filter(|_| is_strict)),
            None => (entry, None),
        };
        let at_market_root = entry
            .and_then(|e| e.get("source"))
            .and_then(Value::as_str)
            .is_some_and(|s| s == "." || s == "./");
        Some(Self {
            skills: joined(items(base, "skills"), items(extra, "skills")),
            commands: joined(items(base, "commands"), items(extra, "commands")),
            agents: joined(items(base, "agents"), items(extra, "agents")),
            mcp_servers: items(base, "mcpServers").unwrap_or_default(),
            skips_default_skills: at_market_root && entry.is_some_and(|e| e.contains_key("skills")),
        })
    }
}

/// 기본 `skills/` 밖에서 온 스킬: 매니페스트 `skills` 경로와 루트 `SKILL.md` 하나짜리 플러그인.
/// 컨텍스트 측정(`context::scan`)도 이것으로 그 스킬의 크기를 더한다.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ExtraSkills {
    /// 기본 `skills/`도 읽는가(마켓플레이스 루트 항목이 `skills`를 적으면 아니다).
    pub reads_default: bool,
    /// (이름, `SKILL.md` 실제 경로).
    pub found: Vec<(String, PathBuf)>,
}

impl ExtraSkills {
    fn read(dir: &Path, plugin_name: &str, manifest: &Manifest) -> Self {
        let mut found: Vec<(String, PathBuf)> = manifest
            .skills
            .iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|raw| resolve(dir, raw, true))
            .flat_map(|path| skills_in(dir, &path, plugin_name))
            .collect();
        let has_skill_source = manifest.skills.is_some() || dir.join("skills").is_dir();
        let root_skill = dir.join("SKILL.md");
        if !has_skill_source && root_skill.is_file() {
            found.push((plugin_name.to_owned(), root_skill));
        }
        Self {
            reads_default: !manifest.skips_default_skills,
            found,
        }
    }
}

/// 플러그인의 실제 폴더, 이름, 합친 매니페스트. 폴더를 못 찾거나 매니페스트가 충돌하면 `None`.
fn load(agent_root: &Path, plugin_id: &str) -> Option<(PathBuf, String, Manifest)> {
    let dir = plugin_directory(agent_root, plugin_id)?;
    let (name, marketplace) = plugin_id.rsplit_once('@')?;
    let entry = marketplace_entry(agent_root, name, marketplace);
    let manifest = Manifest::read(&dir, entry.as_ref())?;
    Some((dir, name.to_owned(), manifest))
}

/// `<name>@<marketplace>` 플러그인의 기본 `skills/` 밖 스킬. `None`은 `load`와 같다.
pub(crate) fn extra_skills(agent_root: &Path, plugin_id: &str) -> Option<ExtraSkills> {
    let (dir, name, manifest) = load(agent_root, plugin_id)?;
    Some(ExtraSkills::read(&dir, &name, &manifest))
}

impl Parts {
    /// `<name>@<marketplace>` 플러그인의 이름 전부. 폴더를 못 찾거나 매니페스트가 충돌하면 `None`.
    pub fn read(agent_root: &Path, plugin_id: &str) -> Option<Self> {
        let (dir, name, manifest) = load(agent_root, plugin_id)?;
        Some(Self::from_manifest(&dir, &name, &manifest))
    }

    /// 기본 폴더(skills/ agents/ commands/ .mcp.json) + 매니페스트 키(plugins-reference "Path rules").
    fn from_manifest(dir: &Path, plugin_name: &str, manifest: &Manifest) -> Self {
        let strings = |list: &Option<Vec<Value>>| -> Vec<String> {
            list.iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        };
        let extra = ExtraSkills::read(dir, plugin_name, manifest);
        let mut skills = if extra.reads_default {
            skill_folders(&dir.join("skills"))
        } else {
            Vec::new()
        };
        skills.extend(extra.found.into_iter().map(|(name, _)| name));
        let commands = match &manifest.commands {
            Some(list) => list.iter().flat_map(|v| commands_in(dir, v)).collect(),
            None => md_stems(&dir.join("commands")),
        };
        // agents는 .md 파일만 받는다(폴더는 Claude Code가 받지 않는다).
        let agents = match &manifest.agents {
            Some(_) => strings(&manifest.agents)
                .iter()
                .filter_map(|raw| md_stem(&resolve(dir, raw, false)?))
                .collect(),
            None => md_stems(&dir.join("agents")),
        };
        let mut mcp_servers = mcp_server_names(&dir.join(".mcp.json"));
        mcp_servers.extend(manifest.mcp_servers.iter().flat_map(|v| servers_in(dir, v)));
        Self {
            skills: sorted(skills),
            agents: sorted(agents),
            commands: sorted(commands),
            mcp_servers: sorted(mcp_servers),
        }
    }

    fn is_empty(&self) -> bool {
        self.skills.is_empty()
            && self.agents.is_empty()
            && self.commands.is_empty()
            && self.mcp_servers.is_empty()
    }
}

/// 이름 조각 하나가 폴더 한 단계로만 읽히는가(`..`·구분자·빈 값이면 아니다).
fn is_plain_segment(part: &str) -> bool {
    !part.is_empty() && part != "." && part != ".." && !part.contains(['/', '\\', '\0'])
}

/// `<name>@<marketplace>`의 캐시 폴더. id는 설정 파일에서 온 값이라 경로 조각을 검사하고,
/// 찾은 폴더가 실제 경로로도 `<agent_root>/plugins/cache` 안에 있을 때만 쓴다(심볼릭 링크로 밖을 가리키면 버린다).
fn plugin_directory(agent_root: &Path, plugin_id: &str) -> Option<PathBuf> {
    let (name, marketplace) = plugin_id.rsplit_once('@')?;
    if !is_plain_segment(name) || !is_plain_segment(marketplace) {
        return None;
    }
    let dir = fsutil::plugin_cache_directory(agent_root, plugin_id)?;
    let cache = agent_root
        .join("plugins")
        .join("cache")
        .canonicalize()
        .ok()?;
    let real = dir.canonicalize().ok()?;
    real.starts_with(&cache).then_some(real)
}

/// 플러그인 스위치의 "함께 꺼지는 것". 폴더가 없거나 이름이 하나도 없으면 `None`.
pub fn read(agent_root: &Path, plugin_id: &str) -> Option<PluginContents> {
    let parts = Parts::read(agent_root, plugin_id)?;
    if parts.is_empty() {
        return None;
    }
    Some(PluginContents {
        skills: NameList::of(&parts.skills),
        agents: NameList::of(&parts.agents),
        commands: NameList::of(&parts.commands),
        mcp_servers: NameList::of(&parts.mcp_servers),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;

    const VERSION_DIR: &str = ".claude/plugins/cache/m/kit/1.0.0";

    fn fake_plugin(home: &TempDir) {
        for rel in [
            "skills/review/SKILL.md",
            "skills/label/SKILL.md",
            "skills/summarize/SKILL.md",
            "skills/triage/SKILL.md",
            "agents/checker.md",
            "commands/ship.md",
        ] {
            home.write(&format!("{VERSION_DIR}/{rel}"), "x");
        }
        // SKILL.md 없는 폴더, md가 아닌 파일은 세지 않는다.
        home.write(&format!("{VERSION_DIR}/skills/notes/README.txt"), "x");
        home.write(&format!("{VERSION_DIR}/agents/notes.txt"), "x");
        home.write(
            &format!("{VERSION_DIR}/.mcp.json"),
            r#"{"mcpServers": {"tracker": {"command": "x"}, "bad": 1}}"#,
        );
    }

    #[test]
    fn lists_names_by_kind_with_a_cap_and_the_full_count() {
        let home = TempDir::new("coach-contents");
        fake_plugin(&home);
        let root = home.path().join(".claude");
        let got = read(&root, "kit@m").expect("contents");
        assert_eq!(got.skills.names, ["label", "review", "summarize"]);
        assert_eq!(got.skills.count, 4);
        assert_eq!(
            (got.agents.names, got.agents.count),
            (vec!["checker".to_owned()], 1)
        );
        assert_eq!(got.commands.names, ["ship"]);
        assert_eq!(
            (got.mcp_servers.names, got.mcp_servers.count),
            (vec!["tracker".to_owned()], 1)
        );
    }

    #[test]
    fn reads_server_names_from_a_bare_mcp_json() {
        let home = TempDir::new("coach-contents-bare-mcp");
        home.write(
            &format!("{VERSION_DIR}/.mcp.json"),
            r#"{"one": {}, "two": {}}"#,
        );
        let got = read(&home.path().join(".claude"), "kit@m").expect("contents");
        assert_eq!(got.mcp_servers.names, ["one", "two"]);
        assert_eq!(got.skills.count, 0);
    }

    #[test]
    fn missing_or_empty_plugin_folders_give_none() {
        let home = TempDir::new("coach-contents-none");
        let root = home.path().join(".claude");
        assert_eq!(read(&root, "kit@m"), None);
        home.write(&format!("{VERSION_DIR}/README.md"), "x");
        assert_eq!(read(&root, "kit@m"), None);
    }

    #[test]
    fn rejects_ids_that_would_leave_the_plugin_cache() {
        let home = TempDir::new("coach-contents-escape");
        home.write(".claude/plugins/cache/m/skills/x/SKILL.md", "x");
        home.write("outside/1.0.0/skills/x/SKILL.md", "x");
        let root = home.path().join(".claude");
        for id in [
            "..@m",
            "kit@..",
            "a/b@m",
            "kit@",
            "@m",
            "kit",
            "../../../outside@m",
        ] {
            assert_eq!(read(&root, id), None, "{id}");
        }
    }

    const MARKET_FILE: &str = ".claude/plugins/marketplaces/m/.claude-plugin/marketplace.json";

    fn parts(home: &TempDir) -> Option<Parts> {
        Parts::read(&home.path().join(".claude"), "kit@m")
    }

    fn write_manifest(home: &TempDir, manifest: Value) {
        home.write(
            &format!("{VERSION_DIR}/.claude-plugin/plugin.json"),
            &manifest.to_string(),
        );
    }

    #[test]
    fn reads_skill_folders_listed_by_a_marketplace_entry_without_plugin_json() {
        let home = TempDir::new("coach-contents-market");
        home.write(
            MARKET_FILE,
            &json!({"plugins": [
                {"name": "other", "skills": ["./nope"]},
                {"name": "kit", "source": {"source": "url", "url": "https://example.com/kit.git"},
                 "strict": false, "skills": ["./alpha-skill", "./beta-skill"]}
            ]})
            .to_string(),
        );
        for rel in [
            "alpha-skill/SKILL.md",
            "beta-skill/SKILL.md",
            "gamma/SKILL.md",
        ] {
            home.write(&format!("{VERSION_DIR}/{rel}"), "x");
        }
        let got = parts(&home).expect("parts");
        assert_eq!(got.skills, ["alpha-skill", "beta-skill"]);
    }

    #[test]
    fn plugin_json_skills_add_to_the_default_folder_and_entry_skills_append() {
        let home = TempDir::new("coach-contents-skills-add");
        write_manifest(&home, json!({"name": "kit", "skills": "./extra/"}));
        home.write(
            MARKET_FILE,
            &json!({"plugins": [{"name": "kit", "skills": ["./more"]}]}).to_string(),
        );
        for rel in [
            "skills/a/SKILL.md",
            "extra/b/SKILL.md",
            "extra/c/SKILL.md",
            "more/SKILL.md",
        ] {
            home.write(&format!("{VERSION_DIR}/{rel}"), "x");
        }
        assert_eq!(parts(&home).expect("parts").skills, ["a", "b", "c", "more"]);
    }

    #[test]
    fn strict_false_entry_with_components_next_to_plugin_json_does_not_load() {
        let home = TempDir::new("coach-contents-conflict");
        write_manifest(&home, json!({"name": "kit"}));
        home.write(&format!("{VERSION_DIR}/skills/a/SKILL.md"), "x");
        home.write(
            MARKET_FILE,
            &json!({"plugins": [{"name": "kit", "strict": false, "skills": ["./a"]}]}).to_string(),
        );
        assert_eq!(parts(&home), None);
    }

    #[test]
    fn manifest_commands_and_agents_replace_the_default_folders() {
        let home = TempDir::new("coach-contents-replace");
        write_manifest(
            &home,
            json!({"commands": ["./cmds/", "./single.md"], "agents": ["./team/reviewer.md", "./team/"]}),
        );
        for rel in [
            "commands/old.md",
            "agents/old-agent.md",
            "cmds/deploy.md",
            "single.md",
            "team/reviewer.md",
            "team/tester.md",
        ] {
            home.write(&format!("{VERSION_DIR}/{rel}"), "x");
        }
        let got = parts(&home).expect("parts");
        assert_eq!(got.commands, ["deploy", "single"]);
        // agents는 파일만: 폴더 "./team/"은 세지 않는다.
        assert_eq!(got.agents, ["reviewer"]);

        write_manifest(
            &home,
            json!({"commands": {"about": {"content": "x"}, "bad": 1}}),
        );
        assert_eq!(parts(&home).expect("parts").commands, ["about"]);
    }

    #[test]
    fn manifest_mcp_servers_merge_with_mcp_json() {
        let home = TempDir::new("coach-contents-mcp");
        home.write(
            &format!("{VERSION_DIR}/.mcp.json"),
            r#"{"mcpServers": {"tracker": {}}}"#,
        );
        home.write(
            &format!("{VERSION_DIR}/mcp/more.json"),
            r#"{"mcpServers": {"more-one": {}}}"#,
        );
        write_manifest(
            &home,
            json!({"mcpServers": [{"inline-one": {"command": "x"}, "tracker": {}}, "./mcp/more.json", "./b.mcpb"]}),
        );
        let got = parts(&home).expect("parts");
        assert_eq!(got.mcp_servers, ["inline-one", "more-one", "tracker"]);
    }

    #[test]
    fn a_root_skill_md_alone_is_one_skill_named_after_the_plugin() {
        let home = TempDir::new("coach-contents-root-skill");
        home.write(&format!("{VERSION_DIR}/SKILL.md"), "x");
        assert_eq!(parts(&home).expect("parts").skills, ["kit"]);
        // skills/ 폴더가 있으면 루트 SKILL.md는 따로 세지 않는다.
        home.write(&format!("{VERSION_DIR}/skills/a/SKILL.md"), "x");
        assert_eq!(parts(&home).expect("parts").skills, ["a"]);
    }

    #[test]
    fn drops_manifest_paths_that_leave_the_plugin_folder() {
        let home = TempDir::new("coach-contents-path-escape");
        home.write("outside/x/SKILL.md", "x");
        home.write("outside/evil.md", "x");
        let abs = home.path().join("outside").to_string_lossy().into_owned();
        let mut skills = vec![
            "../../../../../outside".to_owned(),
            "./a/../../../../../../outside".to_owned(),
        ];
        skills.push(abs.clone());
        skills.push("outside".to_owned());
        #[cfg(unix)]
        {
            let dir = home.path().join(VERSION_DIR);
            std::fs::create_dir_all(&dir).unwrap();
            std::os::unix::fs::symlink(home.path().join("outside"), dir.join("link-out")).unwrap();
            skills.push("./link-out".to_owned());
        }
        write_manifest(
            &home,
            json!({"skills": skills, "commands": [format!("{abs}/evil.md"), "./../evil.md"]}),
        );
        home.write(&format!("{VERSION_DIR}/skills/a/SKILL.md"), "x");
        let got = parts(&home).expect("parts");
        assert_eq!(got.skills, ["a"]);
        assert!(got.commands.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn ignores_a_marketplace_folder_linked_outside() {
        let home = TempDir::new("coach-contents-market-link");
        home.write(
            "outside/.claude-plugin/marketplace.json",
            &json!({"plugins": [{"name": "kit", "skills": ["./alpha"]}]}).to_string(),
        );
        home.write(&format!("{VERSION_DIR}/alpha/SKILL.md"), "x");
        let markets = home.path().join(".claude/plugins/marketplaces");
        std::fs::create_dir_all(&markets).unwrap();
        std::os::unix::fs::symlink(home.path().join("outside"), markets.join("m")).unwrap();
        assert_eq!(parts(&home).expect("parts").skills, Vec::<String>::new());
    }

    #[cfg(unix)]
    #[test]
    fn ignores_a_cache_folder_linked_outside() {
        let home = TempDir::new("coach-contents-link");
        home.write("outside/1.0.0/skills/x/SKILL.md", "x");
        let market = home.path().join(".claude/plugins/cache/m");
        std::fs::create_dir_all(&market).unwrap();
        std::os::unix::fs::symlink(home.path().join("outside"), market.join("kit")).unwrap();
        assert_eq!(read(&home.path().join(".claude"), "kit@m"), None);
    }
}
