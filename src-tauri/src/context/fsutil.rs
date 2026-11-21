//! 에이전트별 스캔이 같이 쓰는 파일 도구(ctxmeter `scan-common.js`).
//!
//! 파일 내용은 크기·front matter 계산에만 쓰고 돌려주지 않는다.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// 바이트 ÷ `bytes_per_token`, 반올림(기본 4: ctxmeter `Math.round(bytes / 4)`). 1 이상이어야 한다(tuning이 보장).
pub fn estimate(bytes: u64, bytes_per_token: u64) -> u64 {
    (bytes + bytes_per_token / 2) / bytes_per_token
}

pub fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
}

/// 실제 파일(심볼릭 링크를 따라간 경로). 파일이 아니면 `None`.
pub fn real_file(path: &Path) -> Option<PathBuf> {
    let real = std::fs::canonicalize(path).ok()?;
    real.is_file().then_some(real)
}

/// 읽지 못하거나 JSON이 아니면 `None`.
pub fn read_json(path: &Path) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn read_text(path: &Path) -> String {
    std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

// 에이전트가 불러오는 것이 들어 있지 않고, .git은 매우 클 수 있다.
const NEVER_WALKED: [&str; 2] = ["node_modules", ".git"];

/// `directory` 아래에서 `predicate`에 맞는 파일. 심볼릭 링크를 따라가되 같은 폴더는 한 번만 본다.
/// 읽지 못하는 폴더는 건너뛴다. 결과는 이름순.
pub fn walk_files(directory: &Path, predicate: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(real) = std::fs::canonicalize(directory)
        && real.is_dir()
    {
        walk(directory, real, predicate, &mut files, &mut HashSet::new());
    }
    files
}

/// `real`은 `directory`의 실제 경로다.
fn walk(
    directory: &Path,
    real: PathBuf,
    predicate: &dyn Fn(&Path) -> bool,
    files: &mut Vec<PathBuf>,
    visited: &mut HashSet<PathBuf>,
) {
    // ctxmeter는 inode로 중복을 막는다. 폴더는 hard link가 없으므로 실제 경로로 같다(Windows 포함).
    if !visited.insert(real.clone()) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        if NEVER_WALKED.iter().any(|n| name == *n) {
            continue;
        }
        let item = entry.path();
        // 항목 종류는 stat 없이 나오고, 링크가 아닌 폴더의 실제 경로는 부모 실제 경로 + 이름이다
        // (Kiro 세션 폴더는 폴더 약 3만 개). 링크만 stat·canonicalize로 따라가고, 깨진 링크는 건너뛴다.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_symlink() {
            if kind.is_dir() {
                walk(&item, real.join(&name), predicate, files, visited);
            } else if kind.is_file() && predicate(&item) {
                files.push(item);
            }
            continue;
        }
        let Ok(meta) = std::fs::metadata(&item) else {
            continue;
        };
        if meta.is_dir() {
            if let Ok(target) = std::fs::canonicalize(&item) {
                walk(&item, target, predicate, files, visited);
            }
        } else if meta.is_file() && predicate(&item) {
            files.push(item);
        }
    }
}

/// 링크된 폴더는 따라가지 않는 `.md` 목록(ctxmeter `instructions.js` `walkMarkdown`). 이름순.
pub fn walk_markdown(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    let mut files = Vec::new();
    for entry in entries {
        let item = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            files.extend(walk_markdown(&item));
        } else if entry.file_name().to_string_lossy().ends_with(".md") {
            files.push(item);
        }
    }
    files.sort();
    files
}

/// 경로로 본 skill 파일의 상태(ctxmeter `classifyAssetPath`). 백업·원본·임시는 불러오지 않는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetStatus {
    Backup,
    MarketplaceSource,
    Staging,
    PluginCache,
    SyncedPlugin,
    ActiveCandidate,
}

static BACKUP_DIR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/(?:skills|agents|rules)\.bak\.[^/]+/").unwrap());

pub fn classify(path: &Path) -> AssetStatus {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if BACKUP_DIR.is_match(&normalized)
        || normalized.contains("/.backup/")
        || normalized.contains("/backups/")
    {
        AssetStatus::Backup
    } else if normalized.contains("/plugins/marketplaces/") {
        AssetStatus::MarketplaceSource
    } else if normalized.contains("/.tmp/") {
        AssetStatus::Staging
    } else if normalized.contains("/plugins/cache/") {
        AssetStatus::PluginCache
    } else if normalized.contains("/plugins/synced/") {
        AssetStatus::SyncedPlugin
    } else {
        AssetStatus::ActiveCandidate
    }
}

/// skill 목록에 들어가는 front matter 바이트(ctxmeter `frontmatterOf`): 여는 `---`부터
/// 닫는 `\n---` 다음 한 글자까지. BOM·CRLF 허용, front matter가 없으면 0.
pub fn skill_metadata_bytes(content: &[u8]) -> u64 {
    let body = content.strip_prefix(BOM).unwrap_or(content);
    let opening = if body.starts_with(b"---\n") {
        4
    } else if body.starts_with(b"---\r\n") {
        5
    } else {
        return 0;
    };
    let start = content.len() - body.len() + opening - 1;
    let end = content[start..]
        .windows(4)
        .position(|w| w == b"\n---")
        .map(|i| start + i);
    end.map_or(0, |end| (end + 5).min(content.len()) as u64)
}

/// skill 묶음 하나: 폴더 아래 `SKILL.md` 수와 front matter 바이트 합.
pub struct SkillGroup {
    pub skills: usize,
    pub metadata_bytes: u64,
}

/// 폴더 아래 `SKILL.md` 전부(이름순).
pub fn skill_files(directory: &Path, allowed: &[AssetStatus]) -> Vec<PathBuf> {
    walk_files(directory, &|p| p.file_name() == Some("SKILL.md".as_ref()))
        .into_iter()
        .filter(|p| allowed.contains(&classify(p)))
        .collect()
}

pub fn skill_group(directory: &Path, allowed: &[AssetStatus]) -> SkillGroup {
    let files = skill_files(directory, allowed);
    let metadata_bytes = files
        .iter()
        .map(|f| std::fs::read(f).map_or(0, |c| skill_metadata_bytes(&c)))
        .sum();
    SkillGroup {
        skills: files.len(),
        metadata_bytes,
    }
}

static FRONT_MATTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\A\x{FEFF}?---\r?\n(.*?)\r?\n---(?:\r?\n|\z)").unwrap());

/// 규칙·steering 파일의 front matter(ctxmeter `instructions.js` `frontMatter`): (본문, 전체 바이트 길이).
pub fn front_matter(text: &str) -> Option<(&str, usize)> {
    let caps = FRONT_MATTER.captures(text)?;
    Some((caps.get(1)?.as_str(), caps.get(0)?.len()))
}

/// front matter의 한 줄 값. 따옴표를 벗기고, 비었으면 `None`.
pub fn front_matter_key(text: &str, key: &str) -> Option<String> {
    let (block, _) = front_matter(text)?;
    let pattern = Regex::new(&format!(r"(?m)^{}:\s*(.*?)\s*$", regex::escape(key))).ok()?;
    let value = pattern.captures(block)?.get(1)?.as_str();
    let value = value.strip_prefix(['"', '\'']).unwrap_or(value);
    let value = value.strip_suffix(['"', '\'']).unwrap_or(value).trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// 이벤트별 handler 배열 길이의 합(ctxmeter `countHooks`).
pub fn count_hooks(hooks: Option<&Value>) -> usize {
    hooks.and_then(Value::as_object).map_or(0, |events| {
        events
            .values()
            .filter_map(Value::as_array)
            .map(Vec::len)
            .sum()
    })
}

/// 플러그인 캐시 폴더(ctxmeter `pluginCacheDirectory`). `name@marketplace`.
/// 설치 기록(`installed_plugins.json`)의 경로를 먼저 쓰고, 없으면 버전 폴더 이름순 마지막.
pub fn plugin_cache_directory(agent_root: &Path, plugin_id: &str) -> Option<PathBuf> {
    let separator = plugin_id.rfind('@').filter(|&i| i >= 1)?;
    let (name, marketplace) = (&plugin_id[..separator], &plugin_id[separator + 1..]);
    let root = agent_root
        .join("plugins")
        .join("cache")
        .join(marketplace)
        .join(name);
    if !root.is_dir() {
        return None;
    }
    let installs = read_json(&agent_root.join("plugins").join("installed_plugins.json"));
    let recorded = installs
        .as_ref()
        .and_then(|v| v.get("plugins")?.get(plugin_id)?.as_array())
        .into_iter()
        .flatten()
        .filter_map(|i| i.get("installPath")?.as_str().map(PathBuf::from))
        .find(|p| p.parent() == Some(root.as_path()) && p.is_dir());
    if recorded.is_some() {
        return recorded;
    }
    let mut versions: Vec<_> = std::fs::read_dir(&root)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name())
        .collect();
    versions.sort();
    versions.last().map(|v| root.join(v))
}

/// 세션 `cwd`가 작업 폴더 안인가. Windows 경로는 대소문자를 가리지 않아(`c:\\`와 `C:\\`) 소문자로 맞춰 비교한다.
pub fn is_inside(path: &Path, root: &Path) -> bool {
    if cfg!(windows) {
        starts_with_ignoring_case(path, root)
    } else {
        path.starts_with(root)
    }
}

fn starts_with_ignoring_case(path: &Path, root: &Path) -> bool {
    let lower = |p: &Path| PathBuf::from(without_verbatim(&p.to_string_lossy().to_lowercase()));
    lower(path).starts_with(lower(root))
}

/// 표시·비교 경로에서 `\\?\c:\x` → `c:\x`, `\\?\unc\srv\x` → `\\srv\x`. 한쪽만 이 앞머리가 붙어도 같은 폴더로 본다.
pub(super) fn without_verbatim(path: &str) -> String {
    if let Some(rest) = path
        .strip_prefix(r"\\?\UNC\")
        .or_else(|| path.strip_prefix(r"\\?\unc\"))
    {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    }
}

#[cfg(test)]
pub mod testing {
    use std::path::{Path, PathBuf};

    /// 테스트마다 새 임시 폴더. 끝나면 `Drop`에서 지운다.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("kcal-ctx-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            // macOS의 /var → /private/var 처럼 링크가 낀 경로를 실제 경로로 맞춘다.
            Self(std::fs::canonicalize(&dir).unwrap())
        }

        pub fn write(&self, relative: &str, content: &str) -> PathBuf {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
            path
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn starts_with_ignoring_case_matches_folders_that_differ_only_in_case() {
        assert!(super::starts_with_ignoring_case(
            Path::new("/Work/App/src"),
            Path::new("/work/app")
        ));
        assert!(!super::starts_with_ignoring_case(
            Path::new("/work/apple"),
            Path::new("/work/app")
        ));
    }

    #[test]
    fn verbatim_prefix_is_dropped_before_comparing() {
        assert_eq!(super::without_verbatim(r"\\?\c:\work\app"), r"c:\work\app");
        assert_eq!(
            super::without_verbatim(r"\\?\unc\srv\share"),
            r"\\srv\share"
        );
        assert_eq!(super::without_verbatim(r"c:\work"), r"c:\work");
    }

    use super::*;

    #[test]
    fn estimate_rounds_bytes_over_bytes_per_token() {
        assert_eq!(estimate(0, 4), 0);
        assert_eq!(estimate(1, 4), 0);
        assert_eq!(estimate(2, 4), 1);
        assert_eq!(estimate(18812, 4), 4703);
        // 다른 비율도 같은 반올림 규칙을 따른다.
        assert_eq!(estimate(18812, 3), 6271);
        assert_eq!(estimate(7, 1), 7);
    }

    #[test]
    fn skill_metadata_counts_front_matter_through_closing_line() {
        assert_eq!(skill_metadata_bytes(b"---\nname: a\n---\nbody"), 16);
        assert_eq!(
            skill_metadata_bytes(b"\xEF\xBB\xBF---\r\nname: a\r\n---\r\n"),
            21
        );
        assert_eq!(skill_metadata_bytes(b"---\n---\n"), 8);
        assert_eq!(skill_metadata_bytes(b"no front matter"), 0);
        assert_eq!(skill_metadata_bytes(b"---\nunclosed"), 0);
    }

    #[test]
    fn front_matter_key_reads_scalars_and_lists() {
        let text = "---\ninclusion: \"fileMatch\"\npaths:\n  - \"src/**\"\n---\nbody";
        assert_eq!(
            front_matter_key(text, "inclusion").as_deref(),
            Some("fileMatch")
        );
        assert!(front_matter_key(text, "paths").is_some());
        assert_eq!(front_matter_key(text, "name"), None);
        assert_eq!(front_matter_key("no front matter", "paths"), None);
    }

    #[test]
    fn classify_skips_backups_and_marketplace_sources() {
        let p = |s: &str| classify(Path::new(s));
        assert_eq!(p("/h/.claude/skills.bak.1/a/SKILL.md"), AssetStatus::Backup);
        assert_eq!(
            p("/h/.claude/plugins/marketplaces/m/SKILL.md"),
            AssetStatus::MarketplaceSource
        );
        assert_eq!(
            p("/h/.claude/plugins/cache/m/p/1/skills/a/SKILL.md"),
            AssetStatus::PluginCache
        );
        assert_eq!(
            p("/h/.claude/skills/a/SKILL.md"),
            AssetStatus::ActiveCandidate
        );
    }

    #[cfg(unix)]
    #[test]
    fn walk_follows_links_once_and_skips_broken_links() {
        use std::os::unix::fs::symlink;
        let dir = testing::TempDir::new("walk-links");
        dir.write("root/a/x.md", "");
        dir.write("other/y.md", "");
        let root = dir.path().join("root");
        symlink(dir.path().join("other"), root.join("b")).unwrap();
        symlink(&root, root.join("a/loop")).unwrap();
        symlink(root.join("nowhere"), root.join("broken")).unwrap();
        symlink(root.join("a/x.md"), root.join("z.md")).unwrap();
        let got = walk_files(&root, &|p| p.extension().is_some_and(|e| e == "md"));
        let names: Vec<_> = got.iter().map(|p| p.strip_prefix(&root).unwrap()).collect();
        // 폴더 링크는 따라가고(b), 이미 본 폴더로 가는 링크(a/loop)는 다시 들어가지 않는다. 파일 링크(z.md)는 센다.
        assert_eq!(names, ["a/x.md", "b/y.md", "z.md"].map(Path::new).to_vec());
    }
}
