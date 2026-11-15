//! 0018 작업 B: `paths` 없는 rules 카드 감지. 최근 `checkModelDays`일 Claude 작업 폴더마다 0005 측정
//! (`instructions::claude`)이 시작 때 싣는 규칙(`ContextKind::Rules`·`Loading::Startup`)의 실제 경로를 모아
//! 범위(사용자 `~/.claude/rules` / 작업 폴더 `<ws>/.claude/rules`)마다 카드 하나로 묶는다.
//!
//! 빼는 것: 플러그인 캐시, 실제 경로가 안내문 모양(`.claude/rules/` 아래 `.md`)이 아닌 것, 실제 경로가 그 범위
//! rules 폴더 밖인 링크. 같은 파일은 한 번만 본다. 규칙마다 무엇을 할지는 Claude에게 맡긴다(`rules_handoff.rs`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use super::handoff::is_instruction_file;
use super::long_instructions::HANDOFF;
use super::{CheckFacts, CheckKind, CheckOption, CoachCheck, Inputs};
use crate::context::fsutil::{front_matter, read_text, real_file, walk_markdown};
use crate::context::instructions::{self, imports_of, resolve_import};
use crate::context::{ContextKind, Loading, paths};
use crate::i18n::pick;
use crate::usage::Agent;

/// 0017 id 규칙(유형-에이전트-요지) 뒤에 범위 rules 폴더 실제 경로.
pub const ID_PREFIX: &str = "A2-claude-rules-paths:";
/// 0017 행 A: 필요할 때만 실리는 파일 하나의 글자 한도(200K 창). 넘으면 앞부분만 실린다.
pub const MAX_CHARS: usize = 40_000;
const DOCS: [&str; 2] = [
    "https://code.claude.com/docs/en/memory#path-specific-rules",
    "https://code.claude.com/docs/en/skills#frontmatter-reference",
];

/// 규칙 범위. TS 짝은 `ipc.ts` `RulesScope`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RulesScope {
    User,
    Project,
}

/// 카드 안 규칙 하나. TS 짝은 `ipc.ts` `RuleFact`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleFact {
    /// 범위 rules 폴더 안 상대 이름(`/`로 잇는다, 예: `common/testing.md`).
    pub name: String,
    /// 머리말을 뗀 본문 글자 수(Claude가 싣는 양).
    pub chars: usize,
    pub has_front_matter: bool,
    /// 같은 rules 폴더의 다른 파일 중 이 파일을 마크다운 링크·`@`로 가리키는 파일 수.
    pub referenced_by: usize,
    /// 실제 경로. 화면에 보내지 않는다(카드 `file`이 범위 폴더).
    #[serde(skip)]
    pub file: PathBuf,
}

/// 범위 하나: rules 폴더 실제 경로와 그 안에서 시작 때 실리는 규칙 실제 경로들.
pub struct Scope {
    pub scope: RulesScope,
    pub root: PathBuf,
    pub files: BTreeSet<PathBuf>,
}

fn startup_rules(home: &Path, workspace: &Path) -> Vec<PathBuf> {
    instructions::claude(home, Some(workspace))
        .items
        .into_iter()
        .filter(|i| i.kind == ContextKind::Rules && i.loading == Loading::Startup)
        .filter_map(|i| i.path.map(PathBuf::from))
        .collect()
}

/// 최근 작업 폴더 전부에서 범위별로 모은 규칙(rules 폴더 실제 경로 순).
pub fn collect_scopes(home: &Path, workspaces: &[PathBuf]) -> Vec<Scope> {
    let claude = paths::claude_root(home);
    // 플러그인 캐시는 사용자가 고칠 파일이 아니고 플러그인을 업데이트하면 덮어써진다(작업 A와 같다).
    let plugins = claude.join("plugins");
    let plugins = std::fs::canonicalize(&plugins).unwrap_or(plugins);
    let user_root = std::fs::canonicalize(claude.join("rules")).ok();
    let mut found: BTreeMap<PathBuf, Scope> = BTreeMap::new();
    for workspace in workspaces {
        let project_root = std::fs::canonicalize(workspace.join(".claude").join("rules")).ok();
        // 작업 폴더가 홈이면 두 rules 폴더가 같다. 사용자 범위를 먼저 본다.
        let roots = [
            (RulesScope::User, user_root.as_ref()),
            (RulesScope::Project, project_root.as_ref()),
        ];
        for file in startup_rules(home, workspace) {
            if file.starts_with(&plugins) || !is_instruction_file(&file) {
                continue;
            }
            let Some((scope, root)) = roots
                .iter()
                .find_map(|&(s, r)| r.filter(|r| file.starts_with(r)).map(|r| (s, r)))
            else {
                continue;
            };
            found
                .entry(root.clone())
                .or_insert_with(|| Scope {
                    scope,
                    root: root.clone(),
                    files: BTreeSet::new(),
                })
                .files
                .insert(file);
        }
    }
    found.into_values().collect()
}

static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\]\(\s*<?([^)\s>]+)").unwrap());

/// 글이 가리키는 파일(마크다운 링크·`@` 가져오기)의 실제 경로. 웹 주소·`#` 앵커만 있는 링크는 뺀다.
fn pointed_files(text: &str, from: &Path, home: &Path) -> BTreeSet<PathBuf> {
    let links = LINK.captures_iter(text).filter_map(|c| {
        let target = c[1].split('#').next().unwrap_or_default().to_owned();
        let is_web = target.contains("://") || target.starts_with("mailto:");
        (!target.is_empty() && !is_web).then_some(target)
    });
    links
        .chain(imports_of(text))
        .filter_map(|r| real_file(&resolve_import(&r, from, home)))
        .collect()
}

/// 파일마다 그 파일을 가리키는 다른 파일 수(같은 rules 폴더 안의 모든 `.md`가 가리키는 쪽).
fn referrers(root: &Path, home: &Path) -> BTreeMap<PathBuf, usize> {
    let mut counts = BTreeMap::new();
    for file in walk_markdown(root) {
        let Some(real) = real_file(&file) else {
            continue;
        };
        for target in pointed_files(&read_text(&real), &real, home) {
            if target != real {
                *counts.entry(target).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// 머리말을 뗀 본문(Claude는 규칙을 불러올 때 머리말을 뗀다).
pub fn body_of(text: &str) -> &str {
    &text[front_matter(text).map_or(0, |(_, len)| len)..]
}

fn relative_name(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn rule_facts(scope: &Scope, home: &Path) -> Vec<RuleFact> {
    let pointed = referrers(&scope.root, home);
    scope
        .files
        .iter()
        .map(|file| {
            let text = read_text(file);
            RuleFact {
                name: relative_name(&scope.root, file),
                chars: body_of(&text).chars().count(),
                has_front_matter: front_matter(&text).is_some(),
                referenced_by: pointed.get(file).copied().unwrap_or(0),
                file: file.clone(),
            }
        })
        .collect()
}

fn card(scope: &Scope, home: &Path) -> CoachCheck {
    let mut rules = rule_facts(scope, home);
    rules.sort_by(|a, b| a.name.cmp(&b.name));
    // `<폴더>/.claude/rules`의 `<폴더>`(사용자 범위는 홈).
    let folder = scope
        .root
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&scope.root);
    let root = scope.root.to_string_lossy().into_owned();
    let handoff = CheckOption {
        is_writable: false,
        instruction: Some(
            pick(
                "Claude에게 맡긴 뒤 규칙마다 골라 바꿔요.",
                "Ask Claude, then pick a change for each rule.",
            )
            .into(),
        ),
        ..CheckOption::write(HANDOFF, "", true, Vec::new())
    };
    CoachCheck {
        id: format!("{ID_PREFIX}{root}"),
        kind: CheckKind::A,
        agent: Agent::Claude,
        facts: CheckFacts::RulesPaths {
            scope: scope.scope,
            folder: folder.to_string_lossy().into_owned(),
            chars: rules.iter().map(|r| r.chars).sum(),
            limit: MAX_CHARS,
            rules,
        },
        options: vec![handoff, CheckOption::keep(false)],
        file: root,
        docs: DOCS.iter().map(|d| (*d).to_owned()).collect(),
        tokens: None,
        is_dismissed: false,
    }
}

pub(super) fn rules_paths(input: &Inputs) -> Vec<CoachCheck> {
    collect_scopes(input.home, &input.records.workspaces)
        .iter()
        .map(|scope| card(scope, input.home))
        .collect()
}
