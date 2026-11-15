//! B. 두 번 실림: AGENTS.md를 다시 넣는 SessionStart hook, 플러그인 스킬과 같은 이름의 개인 스킬.
//! 근거: memory "Remove an earlier AGENTS.md workaround", skills "Resolve skills that share a name"
//! ("A plugin skill and a skill at any of the locations above | Both load").

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{CheckFacts, CheckKind, CheckOption, CoachCheck, Inputs, SettingsScope};
use crate::coach::edit::{JsonOp, Seg, key};
use crate::context::fsutil::{self, AssetStatus, front_matter_key};
use crate::context::{ContextItem, instructions, paths, scan};
use crate::i18n::pick;
use crate::usage::Agent;

const HOOK_DOCS: [&str; 1] =
    ["https://code.claude.com/docs/en/memory#remove-an-earlier-agents-md-workaround"];
const SKILL_DOCS: [&str; 2] = [
    "https://code.claude.com/docs/en/skills#resolve-skills-that-share-a-name",
    "https://code.claude.com/docs/en/settings-reference#skilloverrides",
];
const AGENTS_MD: &str = "AGENTS.md";
const MAX_COMMAND_CHARS: usize = 120;
const HIDE_PERSONAL: &str = "user-invocable-only";

/// 개인 스킬 `~/.claude/skills/<dir>/SKILL.md` 하나. 이름은 front matter `name`, 없으면 폴더 이름.
pub struct PersonalSkill {
    pub name: String,
    pub file: PathBuf,
    pub text: String,
}

fn skill_name(file: &Path, text: &str) -> Option<String> {
    front_matter_key(text, "name").or_else(|| {
        file.parent()?
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    })
}

pub fn personal_skills(home: &Path) -> Vec<PersonalSkill> {
    let Ok(entries) = std::fs::read_dir(paths::claude_root(home).join("skills")) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path().join("SKILL.md"))
        .filter(|f| f.is_file())
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|file| {
            let text = fsutil::read_text(&file);
            Some(PersonalSkill {
                name: skill_name(&file, &text)?,
                file,
                text,
            })
        })
        .collect()
}

/// 켜진 플러그인 스킬 이름 → (플러그인 id, SKILL.md). `context::scan`과 같은 캐시 폴더·분류를 쓴다. 이름이 겹치면 첫 것.
fn plugin_skills(home: &Path, settings: &Value) -> HashMap<String, (String, PathBuf)> {
    let root = paths::claude_root(home);
    let mut found = HashMap::new();
    for id in scan::enabled_plugin_ids(settings) {
        let Some(directory) = fsutil::plugin_cache_directory(&root, &id) else {
            continue;
        };
        let files = fsutil::walk_files(&directory.join("skills"), &|p| {
            p.file_name() == Some("SKILL.md".as_ref())
        });
        for file in files
            .into_iter()
            .filter(|f| fsutil::classify(f) == AssetStatus::PluginCache)
        {
            if let Some(name) = skill_name(&file, &fsutil::read_text(&file)) {
                found.entry(name).or_insert_with(|| (id.clone(), file));
            }
        }
    }
    found
}

fn same_content(a: &Path, b: &Path) -> bool {
    let read = |p: &Path| {
        std::fs::read(p)
            .ok()
            .map(|bytes| String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
    };
    matches!((read(a), read(b)), (Some(x), Some(y)) if x == y)
}

/// `B-claude-same-skill:<name>`. user `skillOverrides.<name>`이 이미 `"on"` 밖의 값이면 사용자가 정한 것이라 내지 않는다.
pub fn same_skills(input: &Inputs) -> Vec<CoachCheck> {
    if !input.file.is_file() {
        return Vec::new();
    }
    let plugins = plugin_skills(input.home, &input.settings);
    let overrides = input.object("skillOverrides");
    personal_skills(input.home)
        .into_iter()
        .filter(|s| {
            overrides
                .get(&s.name)
                .is_none_or(|v| v.as_str() == Some("on"))
        })
        .filter_map(|skill| {
            let (plugin_id, plugin_file) = plugins.get(&skill.name)?;
            let is_same_content = same_content(&skill.file, plugin_file);
            let tokens = fsutil::estimate(
                fsutil::skill_metadata_bytes(skill.text.as_bytes()),
                input.tuning.bytes_per_token,
            );
            let hide = JsonOp::Set(
                vec![key("skillOverrides"), key(&skill.name)],
                Value::from(HIDE_PERSONAL),
            );
            let name = &skill.name;
            let log_name = crate::tr!("{name} 내 스킬 숨기기", "Hide my {name} skill");
            Some(CoachCheck {
                id: format!("B-claude-same-skill:{}", skill.name),
                kind: CheckKind::B,
                agent: Agent::Claude,
                facts: CheckFacts::SameSkill {
                    name: skill.name.clone(),
                    personal_path: skill.file.to_string_lossy().into_owned(),
                    plugin_id: plugin_id.clone(),
                    plugin_path: plugin_file.to_string_lossy().into_owned(),
                    is_same_content,
                },
                options: vec![
                    CheckOption::write("hide-personal", &log_name, is_same_content, vec![hide]),
                    CheckOption::keep(!is_same_content),
                ],
                file: input.file.to_string_lossy().into_owned(),
                docs: SKILL_DOCS.iter().map(|d| (*d).to_owned()).collect(),
                tokens: Some(tokens),
                is_dismissed: false,
            })
        })
        .collect()
}

/// 이 작업 폴더에서 Claude Code가 스스로 싣는 AGENTS.md(직접 읽기든 `@AGENTS.md` import든). 작업 폴더 것을 먼저 고른다.
fn loaded_agents_md(home: &Path, workspace: &Path) -> Option<ContextItem> {
    let items: Vec<ContextItem> = instructions::claude(home, Some(workspace))
        .items
        .into_iter()
        .filter(|i| {
            i.path
                .as_deref()
                .is_some_and(|p| Path::new(p).file_name() == Some(AGENTS_MD.as_ref()))
        })
        .collect();
    let in_workspace = |i: &&ContextItem| {
        let real = std::fs::canonicalize(workspace).ok();
        i.path
            .as_deref()
            .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
            == real
    };
    items.iter().find(in_workspace).or(items.last()).cloned()
}

/// hook 하나를 지우는 편집. 그 때문에 빈 matcher 묶음·빈 `SessionStart` 배열이 되면 그것을 지운다.
fn remove_hook(groups: &[Value], group: usize, hook: usize) -> JsonOp {
    let base = vec![key("hooks"), key("SessionStart")];
    let hooks_in_group = groups[group]
        .get("hooks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let path = match (hooks_in_group, groups.len()) {
        (1, 1) => base,
        (1, _) => [base, vec![Seg::Index(group)]].concat(),
        _ => [
            base,
            vec![Seg::Index(group), key("hooks"), Seg::Index(hook)],
        ]
        .concat(),
    };
    JsonOp::Remove(path)
}

/// (설정 파일, 범위, 그 파일의 SessionStart 묶음). project·local이 user와 같은 파일(작업 폴더 = 홈)이면 뺀다.
fn hook_files(input: &Inputs, workspace: &Path) -> Vec<(PathBuf, SettingsScope, Vec<Value>)> {
    let dir = workspace.join(".claude");
    let user_real = std::fs::canonicalize(&input.file).ok();
    [
        (input.file.clone(), SettingsScope::User),
        (dir.join("settings.json"), SettingsScope::Project),
        (dir.join("settings.local.json"), SettingsScope::Local),
    ]
    .into_iter()
    .filter(|(file, scope)| {
        *scope == SettingsScope::User || std::fs::canonicalize(file).ok() != user_real
    })
    .filter_map(|(file, scope)| {
        let settings = fsutil::read_json(&file)?;
        let groups = settings
            .get("hooks")?
            .get("SessionStart")?
            .as_array()?
            .clone();
        Some((file, scope, groups))
    })
    .collect()
}

fn hook_instruction(file: &Path, command: &str) -> String {
    crate::tr!(
        "{}의 hooks.SessionStart에서 command가 \"{command}\"인 hook을 지우세요.",
        "In {}, delete the hooks.SessionStart hook whose command is \"{command}\".",
        file.display()
    )
}

/// hook 하나의 카드. user 설정만 kcal이 쓰고, project·local은 안내만 한다.
struct HookAt<'a> {
    file: &'a Path,
    scope: SettingsScope,
    groups: &'a [Value],
    group: usize,
    hook: usize,
}

fn hook_check(
    at: HookAt,
    command: &str,
    agents_md: &ContextItem,
    workspace: &Path,
    n: usize,
    tokens: Option<u64>,
) -> CoachCheck {
    let short: String = command.chars().take(MAX_COMMAND_CHARS).collect();
    let option = if at.scope == SettingsScope::User {
        let ops = vec![remove_hook(at.groups, at.group, at.hook)];
        CheckOption::write(
            "remove-hook",
            pick("AGENTS.md hook 빼기", "Remove the AGENTS.md hook"),
            true,
            ops,
        )
    } else {
        CheckOption {
            is_writable: false,
            instruction: Some(hook_instruction(at.file, &short)),
            ..CheckOption::write("remove-hook", "", true, Vec::new())
        }
    };
    CoachCheck {
        id: format!("B-claude-agents-md-hook:{n}"),
        kind: CheckKind::B,
        agent: Agent::Claude,
        facts: CheckFacts::AgentsMdHook {
            settings_file: at.file.to_string_lossy().into_owned(),
            scope: at.scope,
            command: short,
            agents_md: agents_md.path.clone().unwrap_or_default(),
            workspace: workspace.to_string_lossy().into_owned(),
        },
        options: vec![option],
        file: at.file.to_string_lossy().into_owned(),
        docs: HOOK_DOCS.iter().map(|d| (*d).to_owned()).collect(),
        tokens,
        is_dismissed: false,
    }
}

/// `B-claude-agents-md-hook:<n>`: SessionStart hook(모든 matcher)의 command에 AGENTS.md가 있고,
/// 최근 세션 작업 폴더에서 Claude Code가 AGENTS.md를 이미 싣는다. user 설정만 kcal이 고친다.
pub fn agents_md_hooks(input: &Inputs) -> Vec<CoachCheck> {
    let Some(workspace) = input.records.workspace.as_deref() else {
        return Vec::new();
    };
    let Some(agents_md) = loaded_agents_md(input.home, workspace) else {
        return Vec::new();
    };
    let tokens = agents_md
        .bytes
        .map(|b| fsutil::estimate(b, input.tuning.bytes_per_token));
    let mut checks = Vec::new();
    for (file, scope, groups) in hook_files(input, workspace) {
        for (group, entry) in groups.iter().enumerate() {
            let hooks = entry
                .get("hooks")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for (hook, value) in hooks.iter().enumerate() {
                let Some(command) = value
                    .get("command")
                    .and_then(Value::as_str)
                    .filter(|c| c.contains(AGENTS_MD))
                else {
                    continue;
                };
                let at = HookAt {
                    file: &file,
                    scope,
                    groups: &groups,
                    group,
                    hook,
                };
                checks.push(hook_check(
                    at,
                    command,
                    &agents_md,
                    workspace,
                    checks.len(),
                    tokens,
                ));
            }
        }
    }
    checks
}
