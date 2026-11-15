//! 0018 작업 A: 최근 `checkModelDays`일 Claude 작업 폴더마다 시작 때 실리는 `CLAUDE.md`·`CLAUDE.local.md` 중
//! 200줄을 넘는 파일(파일 하나에 카드 하나). 고쳐 쓰기는 Claude에게 맡긴다(`handoff.rs`).
//!
//! 0005 측정(`instructions::claude`)이 싣는 파일 중, 사용자 파일과 작업 폴더·그 위 폴더의 이름 자리
//! (`CLAUDE.md`, `.claude/CLAUDE.md`, `CLAUDE.local.md`)에 있는 것만 본다. `@`로 가져온 파일(AGENTS.md 등)은 넣지 않는다.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{CheckFacts, CheckKind, CheckOption, CoachCheck, Inputs};
use crate::context::fsutil::real_file;
use crate::context::{ContextKind, Loading, instructions, paths};
use crate::i18n::pick;
use crate::usage::Agent;

/// 문서 값(`memory#write-effective-instructions`: 파일당 200줄 아래 권장). tuning이 아니다.
pub const MAX_LINES: usize = 200;
pub const ID_PREFIX: &str = "B-claude-long-instructions:";
/// Claude에게 맡기기 옵션 id. 쓰기는 `apply_check`가 아니라 `apply_claude_handoff`가 한다.
pub const HANDOFF: &str = "handoff";
const DOCS: &str = "https://code.claude.com/docs/en/memory#write-effective-instructions";

/// 줄 수(마지막 줄바꿈 뒤 빈 줄은 세지 않는다).
pub fn line_count(text: &str) -> usize {
    text.lines().count()
}

/// 이 작업 폴더에서 이름 자리로 실리는 파일의 실제 경로(가져온 파일·플러그인 캐시 제외).
fn loaded_claude_files(home: &Path, workspace: &Path) -> Vec<PathBuf> {
    let user = paths::claude_root(home).join("CLAUDE.md");
    // 플러그인 캐시는 사용자가 고칠 파일이 아니고 플러그인을 업데이트하면 덮어써진다.
    let plugins = paths::claude_root(home).join("plugins");
    let plugins = std::fs::canonicalize(&plugins).unwrap_or(plugins);
    let names = [
        PathBuf::from("CLAUDE.md"),
        Path::new(".claude").join("CLAUDE.md"),
        PathBuf::from("CLAUDE.local.md"),
    ];
    let slots: BTreeSet<PathBuf> = std::iter::once(user)
        .chain(
            workspace
                .ancestors()
                .flat_map(|dir| names.iter().map(move |n| dir.join(n))),
        )
        .filter_map(|p| real_file(&p))
        .collect();
    instructions::claude(home, Some(workspace))
        .items
        .into_iter()
        .filter(|i| i.kind == ContextKind::Instructions && i.loading == Loading::Startup)
        .filter_map(|i| i.path.map(PathBuf::from))
        .filter(|p| slots.contains(p) && is_claude_md_name(p) && !p.starts_with(&plugins))
        .collect()
}

/// 실제 파일 이름이 `CLAUDE.md`·`CLAUDE.local.md`인가. `CLAUDE.md -> AGENTS.md` 링크는 실제 파일이 AGENTS.md라 뺀다
/// (Codex·Kiro도 읽고, 기록의 실제 경로가 되돌리기 조건(안내문 모양)에 맞지 않는다).
pub fn is_claude_md_name(file: &Path) -> bool {
    file.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| NAMES.contains(&n))
}

pub const NAMES: [&str; 2] = ["CLAUDE.md", "CLAUDE.local.md"];

/// 최근 작업 폴더 전부에서 모은 안내문 파일(실제 경로, 한 번씩).
pub fn collect_files(home: &Path, workspaces: &[PathBuf]) -> BTreeSet<PathBuf> {
    workspaces
        .iter()
        .flat_map(|w| loaded_claude_files(home, w))
        .collect()
}

fn card(file: &Path, text: &str) -> CoachCheck {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let real = file.to_string_lossy().into_owned();
    let handoff = CheckOption {
        is_writable: false,
        instruction: Some(
            pick(
                "Claude에게 맡긴 뒤 미리보기에서 바꿔요.",
                "Ask Claude, then apply it from the preview.",
            )
            .into(),
        ),
        ..CheckOption::write(HANDOFF, "", true, Vec::new())
    };
    CoachCheck {
        id: format!("{ID_PREFIX}{real}"),
        kind: CheckKind::B,
        agent: Agent::Claude,
        facts: CheckFacts::LongInstructions {
            name,
            lines: line_count(text),
            limit: MAX_LINES,
            imports: instructions::imports_of(text),
        },
        options: vec![handoff, CheckOption::keep(false)],
        file: real,
        docs: vec![DOCS.into()],
        tokens: None,
        is_dismissed: false,
    }
}

pub(super) fn long_instructions(input: &Inputs) -> Vec<CoachCheck> {
    collect_files(input.home, &input.records.workspaces)
        .into_iter()
        .filter_map(|file| {
            let text = std::fs::read_to_string(&file).ok()?;
            (line_count(&text) > MAX_LINES).then(|| card(&file, &text))
        })
        .collect()
}
