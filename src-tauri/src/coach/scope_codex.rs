//! 진단하기 "프로젝트에서만 켜기"의 Codex MCP 서버판(docs/decisions/0022 Codex 갱신, 2026-11-19).
//!
//! 사용자 `~/.codex/config.toml`의 `[mcp_servers.<이름>]` `enabled`를 false로, 그 프로젝트 `.codex/config.toml`의 같은
//! section `enabled`를 true로 둔다. 확인(2026-11-19, openai/codex `config/src/merge.rs`·`loader/mod.rs`): 신뢰한 프로젝트의
//! `.codex/config.toml`은 사용자 설정 위에 키 단위로 겹치고(표는 깊이 합침), 신뢰하지 않은 프로젝트의 파일은 읽지 않는다.
//! 프로젝트 쪽에 `command`가 없어도 합친 결과에는 사용자 파일의 `command`가 있다. 프로젝트 전용 "커밋하지 않는" 변형은 문서에 없다.
//! 쓰기·되돌리기는 Claude 판(`scope.rs`)의 `plan`·`apply`·`undo`를 그대로 쓴다.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{Planned, ScopeFileChange, ScopeMove, has_project_link, is_plain_absolute};
use crate::coach::log::LoggedFile;
use crate::coach::switches::{CoachSwitch, Field};
use crate::coach::usage::SwitchUsage;
use crate::coach::{edit, is_backup_of, read_text, refuse};
use crate::context::scan::trust_key;
use crate::error::AppError;
use crate::i18n::pick;

/// 적용 기록 `target`의 앞부분. TS 짝은 `ipc-coach.ts` `CODEX_SCOPE_TARGET_PREFIX`.
pub const TARGET_PREFIX: &str = "codex/mcp-scope:";
/// TS 짝은 `ipc-coach.ts` `CODEX_PLUGIN_SCOPE_TARGET_PREFIX`.
pub const PLUGIN_TARGET_PREFIX: &str = "codex/plugin-scope:";
const DIR: &str = ".codex";
const FILE: &str = "config.toml";

/// 쓴 프로젝트가 1~`MAX_PROJECTS`곳이고 Claude 판과 같은 횟수·세션 기준을 넘고, 그 폴더가 전부 신뢰한 프로젝트일 때.
/// 신뢰하지 않은 폴더가 하나라도 있으면 제안하지 않는다(그 폴더에서는 프로젝트 파일을 읽지 않아 서버가 꺼진 채로 남는다).
/// `trusted`: `scan::codex_trusted_projects`의 경로(`scan::trust_key`로 맞춘 것).
pub fn proposal(
    home: &Path,
    user: bool,
    usage: &SwitchUsage,
    trusted: &HashSet<String>,
) -> Option<ScopeMove> {
    let moving = super::proposal(home, user, usage)?;
    moving
        .projects
        .iter()
        .all(|p| trusted.contains(&trust_key(&p.path)))
        .then_some(moving)
}

fn config_file(project: &Path) -> PathBuf {
    project.join(DIR).join(FILE)
}

/// 프로젝트 `.codex/config.toml`에 이 서버를 켠다. 이미 켜져 있으면 `None`.
pub(super) fn plan_project(project: &Path, section: &str) -> Result<Option<Planned>, AppError> {
    let dir = project.join(DIR);
    let path = config_file(project);
    if has_project_link(&path) {
        return Err(refuse(crate::tr!(
            "{}의 설정 폴더나 파일이 링크라 프로젝트 밖을 고칠 수 있어 고치지 않아요.",
            "The settings folder or file in {} is a link that could point outside the project, so kcal won't edit it.",
            project.display()
        )));
    }
    if dir.exists() && !dir.is_dir() {
        return Err(refuse(crate::tr!(
            "{}이(가) 폴더가 아니라 고치지 않아요.",
            "{} isn't a folder, so kcal won't edit it.",
            dir.display()
        )));
    }
    let is_new = std::fs::symlink_metadata(&path).is_err();
    let (original, text) = if is_new {
        (None, String::new())
    } else {
        let (bytes, text) = read_text(&path)?;
        (Some(bytes), text)
    };
    let edit = edit::enable_toml_section(&text, section).map_err(refuse)?;
    if !edit.changed {
        return Ok(None);
    }
    Ok(Some(Planned {
        change: ScopeFileChange {
            file: path.to_string_lossy().into_owned(),
            location: edit.location,
            before: edit.before,
            after: edit.after,
            is_new_file: is_new,
        },
        creates_dir: is_new && !dir.exists(),
        path,
        original,
        content: edit.content,
    }))
}

/// 이 스위치의 `[mcp_servers.<이름>]` section(고칠 때 헤더를 찾는 그 글자).
pub(super) fn section(switch: &CoachSwitch) -> Result<&str, AppError> {
    match &switch.field {
        Field::TomlEnabled { section } => Ok(section),
        _ => Err(AppError::Internal),
    }
}

/// 기록의 프로젝트 파일이 kcal이 쓰는 모양인가: `<절대 경로>/.codex/config.toml`, 백업은 그 옆.
pub(super) fn is_known_project_file(logged: &LoggedFile) -> bool {
    let file = Path::new(&logged.file);
    is_plain_absolute(file)
        && file.file_name() == Some(FILE.as_ref())
        && file.parent().and_then(Path::file_name) == Some(DIR.as_ref())
        && !has_project_link(file)
        && logged
            .backup
            .as_deref()
            .is_none_or(|b| is_backup_of(Path::new(b), file))
}

/// 사용자 설정이 이미 꺼져 있을 때의 거부 문구(Claude 판과 같다).
pub(super) fn already_off() -> AppError {
    refuse(pick("이미 꺼져 있어요.", "It's already off."))
}

#[cfg(test)]
#[path = "scope_codex_tests.rs"]
mod tests;
