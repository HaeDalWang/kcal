//! 에이전트가 시작할 때 읽는 지시 파일(ctxmeter `instructions.js`). 각 벤더 문서의 탐색 규칙을 따른다.
//!
//!   Claude  https://code.claude.com/docs/en/memory — `~/.claude/CLAUDE.md`, 작업 폴더와 그 위
//!           모든 폴더의 CLAUDE.md / .claude/CLAUDE.md / CLAUDE.local.md, @import 4단계,
//!           `paths` 없는 rule. CLAUDE.md가 하나도 없을 때만 AGENTS.md.
//!   Codex   ~/.codex AGENTS.override.md 또는 AGENTS.md, 그리고 git root부터 작업 폴더까지 폴더마다
//!           하나, 합계 `project_doc_max_bytes`(기본 32 KiB)에서 자른다.
//!   Kiro    https://kiro.dev/docs/steering/ — `inclusion`이 없거나 `always`인 steering(전역·작업 폴더),
//!           AGENTS.md는 항상.
//!
//! 내용은 @import·front matter를 찾는 데만 읽고 보관하지 않는다.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::fsutil::{self, front_matter, front_matter_key, real_file};
use super::{ContextItem, ContextKind, Loading, paths};

const MAX_IMPORT_HOPS: usize = 4;
pub const CODEX_DEFAULT_MAX_BYTES: u64 = 32 * 1024;

/// 지시 파일 탐색 결과.
pub struct Instructions {
    pub items: Vec<ContextItem>,
    /// Codex: `project_doc_max_bytes`에서 잘렸다.
    pub truncated: bool,
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// 실제 경로 기준으로 한 번만 담는다. 크기 0인 파일은 담지 않는다(ctxmeter `collector`).
#[derive(Default)]
struct Collector {
    seen: HashSet<PathBuf>,
    items: Vec<ContextItem>,
}

impl Collector {
    /// 새로 담았으면 실제 경로를 돌려준다.
    fn add(&mut self, file: &Path, kind: ContextKind, bytes: Option<u64>) -> Option<PathBuf> {
        let real = real_file(file)?;
        if !self.seen.insert(real.clone()) {
            return None;
        }
        let size = bytes.or_else(|| fsutil::file_size(&real))?;
        if size == 0 {
            return None;
        }
        self.items.push(ContextItem::estimated(
            kind,
            display_name(file),
            Some(&real),
            size,
            Loading::Startup,
        ));
        Some(real)
    }
}

/// 루트부터 `workspace`까지의 폴더.
fn lineage(workspace: &Path) -> Vec<PathBuf> {
    let mut chain: Vec<PathBuf> = workspace.ancestors().map(Path::to_path_buf).collect();
    chain.reverse();
    chain
}

static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(```|~~~)").unwrap());
static CODE_SPAN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^`]*`").unwrap());
static IMPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:^|\s)@((?:\\ |[^\s"'])+)"#).unwrap());
static TRAILING_MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.,;:!?)]+$").unwrap());

/// 코드 블록·코드 span 밖의 `@path`. `\ `는 공백, 끝의 문장 부호는 경로가 아니다.
pub fn imports_of(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut fenced = false;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if FENCE.is_match(line) {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let outside = CODE_SPAN.replace_all(line, " ");
        for caps in IMPORT.captures_iter(&outside) {
            let reference = caps[1].replace("\\ ", " ");
            let reference = TRAILING_MARK.replace(&reference, "");
            if !reference.is_empty() {
                found.push(reference.into_owned());
            }
        }
    }
    found
}

pub(crate) fn resolve_import(reference: &str, from: &Path, home: &Path) -> PathBuf {
    if let Some(rest) = reference.strip_prefix("~/") {
        return home.join(rest);
    }
    let path = Path::new(reference);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    from.parent().unwrap_or(from).join(path)
}

pub fn claude(home: &Path, workspace: Option<&Path>) -> Instructions {
    let directories = workspace.map(lineage).unwrap_or_default();
    claude_in(home, workspace, &directories)
}

/// `directories`: 루트부터 작업 폴더까지(테스트는 실제 홈이 끼지 않게 임시 폴더 안으로 줄인다).
fn claude_in(home: &Path, workspace: Option<&Path>, directories: &[PathBuf]) -> Instructions {
    let mut found = Collector::default();
    let user_file = paths::claude_root(home).join("CLAUDE.md");
    add_with_imports(&mut found, &user_file, home, 0);
    // 경로 문자열은 링크·`\\?\`·대소문자로 달라질 수 있어 실제 경로로 비교한다.
    let user_real = real_file(&user_file);

    if workspace.is_some() {
        let project_names = [
            PathBuf::from("CLAUDE.md"),
            Path::new(".claude").join("CLAUDE.md"),
            PathBuf::from("CLAUDE.local.md"),
        ];
        let has_claude_md = directories.iter().any(|d| {
            project_names.iter().any(|n| {
                real_file(&d.join(n)).is_some_and(|real| Some(&real) != user_real.as_ref())
            })
        });
        let agents_names = [
            PathBuf::from("AGENTS.md"),
            Path::new(".claude").join("AGENTS.md"),
        ];
        let names = if has_claude_md {
            &project_names[..]
        } else {
            &agents_names[..]
        };
        for directory in directories {
            for name in names {
                add_with_imports(&mut found, &directory.join(name), home, 0);
            }
        }
    }

    let mut rule_dirs = vec![paths::claude_root(home).join("rules")];
    rule_dirs.extend(workspace.map(|w| w.join(".claude").join("rules")));
    let mut conditional = Vec::new();
    for directory in rule_dirs {
        for file in fsutil::walk_markdown(&directory) {
            let text = fsutil::read_text(&file);
            // Claude는 rule을 불러올 때 front matter를 뗀다.
            let bytes = (text.len() - front_matter(&text).map_or(0, |(_, len)| len)) as u64;
            if front_matter_key(&text, "paths").is_some() {
                let real = real_file(&file).unwrap_or_else(|| file.clone());
                conditional.push(ContextItem::estimated(
                    ContextKind::Rules,
                    display_name(&file),
                    Some(&real),
                    bytes,
                    Loading::Conditional,
                ));
                continue;
            }
            found.add(&file, ContextKind::Rules, Some(bytes));
        }
    }
    let mut items = found.items;
    items.extend(conditional);
    Instructions {
        items,
        truncated: false,
    }
}

fn add_with_imports(found: &mut Collector, file: &Path, home: &Path, hops: usize) {
    let Some(real) = found.add(file, ContextKind::Instructions, None) else {
        return;
    };
    if hops >= MAX_IMPORT_HOPS {
        return;
    }
    for reference in imports_of(&fsutil::read_text(&real)) {
        add_with_imports(
            found,
            &resolve_import(&reference, &real, home),
            home,
            hops + 1,
        );
    }
}

/// `~/.codex/config.toml` 첫 section 앞의 최상위 키.
fn codex_doc_settings(home: &Path) -> (u64, Vec<String>) {
    static MAX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*project_doc_max_bytes\s*=\s*(\d+)").unwrap());
    static FALLBACKS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*project_doc_fallback_filenames\s*=\s*\[(.*)\]").unwrap());
    static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());
    let mut max_bytes = CODEX_DEFAULT_MAX_BYTES;
    let mut fallbacks = Vec::new();
    let text = fsutil::read_text(&paths::codex_root(home).join("config.toml"));
    for line in text.split('\n') {
        if line.trim_start().starts_with('[') {
            break;
        }
        if let Some(value) = MAX.captures(line).and_then(|c| c[1].parse().ok()) {
            max_bytes = value;
        }
        if let Some(caps) = FALLBACKS.captures(line) {
            fallbacks = QUOTED
                .captures_iter(&caps[1])
                .map(|c| c[1].to_owned())
                .collect();
        }
    }
    (max_bytes, fallbacks)
}

fn first_non_empty(directory: &Path, names: &[String]) -> Option<(PathBuf, u64)> {
    names.iter().find_map(|name| {
        let file = directory.join(name);
        let real = real_file(&file)?;
        let size = fsutil::file_size(&real).filter(|&s| s > 0)?;
        Some((file, size))
    })
}

/// Codex가 프로젝트 파일(AGENTS.md, `.agents/skills`)을 찾는 폴더: git 루트부터 `workspace`까지.
/// `.git`이 있는 가장 가까운 위 폴더가 루트이고, 없으면 `workspace`만.
pub fn codex_project_directories(workspace: &Path) -> Vec<PathBuf> {
    let chain = lineage(workspace);
    match chain.iter().rev().find(|d| d.join(".git").exists()) {
        Some(root) => chain
            .iter()
            .filter(|d| d.starts_with(root))
            .cloned()
            .collect(),
        None => vec![workspace.to_path_buf()],
    }
}

pub fn codex(home: &Path, workspace: Option<&Path>) -> Instructions {
    let mut found = Collector::default();
    let global_names = ["AGENTS.override.md".to_owned(), "AGENTS.md".to_owned()];
    if let Some((file, _)) = first_non_empty(&paths::codex_root(home), &global_names) {
        found.add(&file, ContextKind::Instructions, None);
    }
    let mut truncated = false;
    if let Some(workspace) = workspace {
        let (max_bytes, fallbacks) = codex_doc_settings(home);
        let names: Vec<String> = global_names.into_iter().chain(fallbacks).collect();
        let mut remaining = max_bytes;
        for directory in codex_project_directories(workspace) {
            let Some((file, size)) = first_non_empty(&directory, &names) else {
                continue;
            };
            if remaining == 0 {
                truncated = true;
                break;
            }
            let taken = size.min(remaining);
            truncated |= taken < size;
            found.add(&file, ContextKind::Instructions, Some(taken));
            remaining -= taken;
        }
    }
    Instructions {
        items: found.items,
        truncated,
    }
}

pub fn kiro(home: &Path, workspace: Option<&Path>) -> Instructions {
    let mut found = Collector::default();
    let mut on_demand = Vec::new();
    let mut directories = vec![paths::kiro_root(home).join("steering")];
    directories.extend(workspace.map(|w| w.join(".kiro").join("steering")));
    for directory in directories {
        for file in fsutil::walk_markdown(&directory) {
            let text = fsutil::read_text(&file);
            let inclusion = front_matter_key(&text, "inclusion")
                .unwrap_or_else(|| "always".into())
                .to_lowercase();
            // AGENTS.md는 inclusion 방식이 없다: 어디 있든 항상 들어간다.
            let is_agents_md = file.file_name() == Some("AGENTS.md".as_ref());
            if is_agents_md || inclusion == "always" {
                found.add(&file, ContextKind::Rules, None);
                continue;
            }
            let loading = if inclusion == "filematch" {
                Loading::Conditional
            } else {
                Loading::Deferred
            };
            let real = real_file(&file).unwrap_or_else(|| file.clone());
            on_demand.push(ContextItem::estimated(
                ContextKind::Rules,
                display_name(&file),
                Some(&real),
                text.len() as u64,
                loading,
            ));
        }
    }
    if let Some(workspace) = workspace {
        found.add(
            &workspace.join("AGENTS.md"),
            ContextKind::Instructions,
            None,
        );
    }
    let mut items = found.items;
    items.extend(on_demand);
    Instructions {
        items,
        truncated: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;

    fn startup(result: &Instructions) -> Vec<(String, u64)> {
        result
            .items
            .iter()
            .filter(|i| i.loading == Loading::Startup)
            .map(|i| (i.name.clone(), i.bytes.unwrap()))
            .collect()
    }

    fn by_loading(result: &Instructions, loading: Loading) -> Vec<String> {
        result
            .items
            .iter()
            .filter(|i| i.loading == loading)
            .map(|i| i.name.clone())
            .collect()
    }

    #[test]
    fn imports_skip_code_and_trailing_punctuation() {
        let text = "See @a.md and @b\\ c.md.\n`@span.md` mail x@y.z\n```\n@fenced.md\n```\n@~/d.md";
        assert_eq!(imports_of(text), ["a.md", "b c.md", "~/d.md"]);
    }

    #[test]
    fn claude_follows_imports_and_loads_rules_without_paths() {
        let home = TempDir::new("claude-imports");
        home.write(".claude/CLAUDE.md", "use @extra.md\n");
        home.write(".claude/extra.md", "more @~/deep.md");
        home.write("deep.md", "deep");
        home.write(".claude/rules/always.md", "---\nname: x\n---\nrule");
        home.write(
            ".claude/rules/scoped.md",
            "---\npaths:\n  - \"src/**\"\n---\nscoped",
        );
        let result = claude(home.path(), None);
        assert_eq!(
            startup(&result),
            [
                ("CLAUDE.md".to_owned(), 14),
                ("extra.md".to_owned(), 15),
                ("deep.md".to_owned(), 4),
                // front matter("---\nname: x\n---\n", 16바이트)를 뺀 크기
                ("always.md".to_owned(), 4),
            ]
        );
        assert_eq!(by_loading(&result, Loading::Conditional), ["scoped.md"]);
    }

    #[test]
    fn claude_import_chain_stops_after_four_hops() {
        let home = TempDir::new("claude-hops");
        home.write(".claude/CLAUDE.md", "@1.md");
        for i in 1..=5 {
            home.write(&format!(".claude/{i}.md"), &format!("@{}.md", i + 1));
        }
        let names: Vec<String> = startup(&claude(home.path(), None))
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names, ["CLAUDE.md", "1.md", "2.md", "3.md", "4.md"]);
    }

    #[test]
    fn claude_reads_agents_md_only_when_no_claude_md_in_lineage() {
        let home = TempDir::new("claude-agents-home");
        let work = TempDir::new("claude-agents-work");
        work.write("AGENTS.md", "agents");
        // Windows 임시 폴더는 실제 홈 아래라 전체 lineage를 쓰면 실제 `~\.claude\CLAUDE.md`를 만난다.
        let only_work = [work.path().to_path_buf()];
        let measure = || names(&claude_in(home.path(), Some(work.path()), &only_work));
        assert_eq!(measure(), ["AGENTS.md"]);

        work.write(".claude/CLAUDE.md", "claude");
        work.write("CLAUDE.local.md", "local");
        assert_eq!(measure(), ["CLAUDE.md", "CLAUDE.local.md"]);
    }

    fn names(result: &Instructions) -> Vec<String> {
        startup(result).into_iter().map(|(n, _)| n).collect()
    }

    /// 작업 폴더가 홈 아래일 때 전역 CLAUDE.md는 프로젝트 CLAUDE.md로 세지 않는다.
    /// home을 링크 경로로 넘겨 lineage 경로와 문자열이 달라지게 한다(Windows `\\?\`·대소문자와 같은 경우).
    #[cfg(unix)]
    #[test]
    fn claude_user_file_reached_through_a_link_is_not_a_project_claude_md() {
        let base = TempDir::new("claude-agents-link");
        let home = base.path().join("home");
        base.write("home/.claude/CLAUDE.md", "user");
        base.write("home/work/AGENTS.md", "agents");
        let link = base.path().join("home-link");
        std::os::unix::fs::symlink(&home, &link).unwrap();
        let work = home.join("work");
        let directories = [home.clone(), work.clone()];
        assert_eq!(
            names(&claude_in(&link, Some(&work), &directories)),
            ["CLAUDE.md", "AGENTS.md"]
        );
    }

    #[test]
    fn codex_chain_runs_from_git_root_and_caps_at_max_bytes() {
        let home = TempDir::new("codex-home");
        home.write(".codex/AGENTS.md", "global");
        home.write(".codex/config.toml", "project_doc_max_bytes = 10\n[x]\n");
        let repo = TempDir::new("codex-repo");
        std::fs::create_dir(repo.path().join(".git")).unwrap();
        repo.write("AGENTS.md", "12345678");
        repo.write("sub/AGENTS.override.md", "override");
        repo.write("sub/AGENTS.md", "ignored");
        let result = codex(home.path(), Some(&repo.path().join("sub")));
        assert_eq!(
            startup(&result),
            [
                ("AGENTS.md".to_owned(), 6),
                ("AGENTS.md".to_owned(), 8),
                ("AGENTS.override.md".to_owned(), 2),
            ]
        );
        assert!(result.truncated);
    }

    #[test]
    fn codex_default_cap_is_32_kib() {
        let home = TempDir::new("codex-cap-home");
        let repo = TempDir::new("codex-cap-repo");
        std::fs::create_dir(repo.path().join(".git")).unwrap();
        repo.write("AGENTS.md", &"x".repeat(40 * 1024));
        let result = codex(home.path(), Some(repo.path()));
        assert_eq!(startup(&result), [("AGENTS.md".to_owned(), 32 * 1024)]);
        assert!(result.truncated);
    }

    #[test]
    fn codex_global_prefers_non_empty_override() {
        let home = TempDir::new("codex-override");
        home.write(".codex/AGENTS.override.md", "");
        home.write(".codex/AGENTS.md", "global");
        assert_eq!(
            startup(&codex(home.path(), None)),
            [("AGENTS.md".to_owned(), 6)]
        );
        home.write(".codex/AGENTS.override.md", "over");
        assert_eq!(
            startup(&codex(home.path(), None)),
            [("AGENTS.override.md".to_owned(), 4)]
        );
    }

    #[test]
    fn kiro_steering_inclusion_decides_loading() {
        let home = TempDir::new("kiro-steering");
        home.write(".kiro/steering/plain.md", "plain");
        home.write(".kiro/steering/always.md", "---\ninclusion: always\n---\nx");
        home.write(
            ".kiro/steering/match.md",
            "---\ninclusion: fileMatch\n---\nx",
        );
        home.write(".kiro/steering/manual.md", "---\ninclusion: manual\n---\nx");
        home.write(".kiro/steering/auto.md", "---\ninclusion: auto\n---\nx");
        home.write(".kiro/steering/AGENTS.md", "---\ninclusion: manual\n---\nx");
        let work = TempDir::new("kiro-work");
        work.write("AGENTS.md", "project");
        let result = kiro(home.path(), Some(work.path()));
        let mut names: Vec<String> = startup(&result).into_iter().map(|(n, _)| n).collect();
        names.sort();
        assert_eq!(names, ["AGENTS.md", "AGENTS.md", "always.md", "plain.md"]);
        assert_eq!(by_loading(&result, Loading::Conditional), ["match.md"]);
        assert_eq!(
            by_loading(&result, Loading::Deferred),
            ["auto.md", "manual.md"]
        );
    }
}
