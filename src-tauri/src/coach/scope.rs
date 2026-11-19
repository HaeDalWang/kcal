//! 진단하기 "프로젝트에서만 켜기"(docs/decisions/0022).
//!
//! 사용자 전체로 켠 Claude 플러그인을 최근 쓴 프로젝트(1~`MAX_PROJECTS`곳)에서만 켠다: 사용자 `settings.json`
//! `enabledPlugins.<id>`를 false로, 그 프로젝트 `.claude/settings.local.json` `enabledPlugins.<id>`를 true로.
//! 확인(2026-11-13, Claude Code 2.1.323): local 값이 사용자 값을 양쪽 방향으로 덮고, `installed_plugins.json`의
//! local 항목은 세션이 시작하며 스스로 넣는다. 그래서 kcal은 이 두 키만 쓴다.
//! 0014처럼 백업 → 쓰기 → 기록 한 줄이고, 되돌리기는 그 한 줄로 모든 파일을 함께 되돌린다.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};
use time::OffsetDateTime;

use super::edit::{self, JsonOp, key};
use super::log::{AppliedSwitch, CoachAction, LoggedFile};
use super::switches::{self, CoachSwitch};
use super::usage::{SwitchUsage, folder_name};
use super::{
    append_applied, backup_and_replace, copy_command, io_refusal, is_backup_of, read_text,
    record_id, refuse, replace_file, validate_hash,
};
use crate::context::fsutil;
use crate::error::AppError;
use crate::i18n::pick;
use crate::usage::Agent;

#[path = "scope_codex.rs"]
pub mod codex;

/// 적용 기록 `target`의 앞부분. TS 짝은 `ipc.ts` `SCOPE_TARGET_PREFIX`(화면이 이름 뒤 "켬/끔"을 붙이지 않는다).
pub const SCOPE_TARGET_PREFIX: &str = "claude/plugin-scope:";

/// "프로젝트에서만 켜기" 적용 기록의 `target`인가(Claude 플러그인·Codex MCP).
pub fn is_scope_target(target: &str) -> bool {
    target.starts_with(SCOPE_TARGET_PREFIX) || target.starts_with(codex::TARGET_PREFIX)
}

fn target_prefix(agent: Agent) -> &'static str {
    if agent == Agent::Codex {
        codex::TARGET_PREFIX
    } else {
        SCOPE_TARGET_PREFIX
    }
}
/// 이 수 이하의 프로젝트에서만 썼으면 옮기기를 제안한다. 더 많으면 "그대로(자주 씀)".
pub const MAX_PROJECTS: usize = 3;
const CLAUDE_DIR: &str = ".claude";
const LOCAL_FILE: &str = "settings.local.json";
const PROJECT_FILE: &str = "settings.json";

/// 프로젝트 설정 파일 종류. `local`이 `project`보다 우선한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeFile {
    Project,
    Local,
}

/// 프로젝트 하나의 설정에 적힌 이 플러그인 값.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScope {
    pub name: String,
    pub path: String,
    pub file: ScopeFile,
    pub enabled: bool,
}

/// 지금 켠 범위. TS 짝은 `ipc.ts` `SwitchScope`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchScope {
    /// 사용자 settings 값(true = 모든 프로젝트에서 켬).
    pub user: bool,
    /// 최근 세션 프로젝트 중 그 프로젝트 설정에 이 플러그인 값이 적힌 곳(파일 둘 다 있으면 local 값).
    pub projects: Vec<ProjectScope>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRef {
    pub name: String,
    pub path: String,
}

/// "프로젝트에서만 켜기" 제안. TS 짝은 `ipc.ts` `ScopeMove`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeMove {
    /// 켤 프로젝트(지금 있는 폴더).
    pub projects: Vec<ProjectRef>,
    /// 썼지만 폴더가 없어져 건너뛸 프로젝트 이름.
    pub skipped: Vec<String>,
}

/// 바뀌는 곳 하나(파일마다 하나).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeFileChange {
    pub file: String,
    pub location: String,
    pub before: Option<String>,
    pub after: String,
    /// 파일을 새로 만든다.
    pub is_new_file: bool,
}

/// 미리보기. `beforeHash`를 `apply_scope_move`에 그대로 보낸다. TS 짝은 `ipc.ts` `ScopeMovePreview`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeMovePreview {
    pub target: String,
    /// 프로젝트 파일들, 마지막이 사용자 설정.
    pub changes: Vec<ScopeFileChange>,
    pub skipped: Vec<String>,
    pub before_hash: String,
}

pub(super) fn is_plain_absolute(path: &Path) -> bool {
    path.is_absolute() && !path.components().any(|c| c == Component::ParentDir)
}

/// 설정을 쓸 수 있는 프로젝트 폴더: 지금 있고, 홈 자체가 아니다(홈의 `.claude`는 사용자 설정 폴더다).
pub(super) fn is_project_dir(home: &Path, path: &Path) -> bool {
    is_plain_absolute(path) && path != home && path.parent().is_some() && path.is_dir()
}

/// 프로젝트 설정 하나의 `enabledPlugins`(id → (파일, 값)).
type PluginValues = HashMap<String, (ScopeFile, bool)>;

/// 최근 세션 프로젝트마다 그 프로젝트 설정의 `enabledPlugins`.
pub struct ProjectSettings(Vec<(PathBuf, PluginValues)>);

impl ProjectSettings {
    pub fn read<'a>(home: &Path, projects: impl IntoIterator<Item = &'a PathBuf>) -> Self {
        let read_file = |dir: &Path, name: &str| {
            fsutil::read_json(&dir.join(CLAUDE_DIR).join(name))
                .map(|v| crate::context::scan::claude_plugin_states(&v))
                .unwrap_or_default()
        };
        Self(
            projects
                .into_iter()
                .filter(|p| is_project_dir(home, p))
                .filter_map(|dir| {
                    let mut plugins = HashMap::new();
                    for (file, name) in [
                        (ScopeFile::Project, PROJECT_FILE),
                        (ScopeFile::Local, LOCAL_FILE),
                    ] {
                        for (id, on) in read_file(dir, name) {
                            plugins.insert(id, (file, on));
                        }
                    }
                    (!plugins.is_empty()).then(|| (dir.clone(), plugins))
                })
                .collect(),
        )
    }

    pub fn scope(&self, id: &str, user: bool) -> SwitchScope {
        SwitchScope {
            user,
            projects: self
                .0
                .iter()
                .filter_map(|(dir, plugins)| {
                    let (file, enabled) = *plugins.get(id)?;
                    Some(ProjectScope {
                        name: folder_name(dir),
                        path: dir.to_string_lossy().into_owned(),
                        file,
                        enabled,
                    })
                })
                .collect(),
        }
    }
}

/// 사용자 전체로 켜져 있고, 기록을 다 봤고, 기간 안 사용이 `scopeMinUses`회 이상이고 세션이 `diagnoseMinSessions`개 이상이고,
/// 쓴 프로젝트가 1~`MAX_PROJECTS`곳이면 옮기기를 제안한다. 1~2회는 자주 쓰는지 알 수 없어 제안하지 않는다.
/// 홈 폴더에서 시작한 세션에서 썼으면 제안하지 않는다(어느 프로젝트에도 속하지 않는 세션이다).
pub fn proposal(home: &Path, user: bool, usage: &SwitchUsage) -> Option<ScopeMove> {
    let has_enough_uses = usize::try_from(usage.total).map_or(true, |n| n >= usage.scope_min_uses);
    if !user
        || usage.is_partial
        || !has_enough_uses
        || usage.sessions < usage.min_sessions
        || !(1..=MAX_PROJECTS).contains(&usage.project_count)
    {
        return None;
    }
    let mut projects = Vec::new();
    let mut skipped = Vec::new();
    for used in &usage.projects {
        let path = Path::new(&used.path);
        if path == home {
            return None;
        }
        if is_project_dir(home, path) {
            projects.push(ProjectRef {
                name: used.name.clone(),
                path: used.path.clone(),
            });
        } else {
            skipped.push(used.name.clone());
        }
    }
    (!projects.is_empty()).then_some(ScopeMove { projects, skipped })
}

/// 쓸 파일 하나.
pub(super) struct Planned {
    pub(super) path: PathBuf,
    /// 지금 내용. `None`이면 새로 만든다.
    pub(super) original: Option<Vec<u8>>,
    pub(super) content: String,
    pub(super) change: ScopeFileChange,
    pub(super) creates_dir: bool,
}

fn local_file(project: &Path) -> PathBuf {
    project.join(CLAUDE_DIR).join(LOCAL_FILE)
}

fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// 프로젝트 쪽 `.claude` 폴더나 설정 파일이 링크면 쓰지 않는다(낯선 저장소가 프로젝트 밖 파일을 가리켜 덮게 하지 못하게).
/// 사용자 설정 파일의 링크(dotfiles)는 대상이 아니라 계속 따라간다.
pub(super) fn has_project_link(file: &Path) -> bool {
    is_link(file) || file.parent().is_some_and(is_link)
}

fn plan_project(project: &Path, id: &str) -> Result<Option<Planned>, AppError> {
    let dir = project.join(CLAUDE_DIR);
    if has_project_link(&local_file(project)) {
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
    let path = local_file(project);
    let path_text = path.to_string_lossy().into_owned();
    if std::fs::symlink_metadata(&path).is_err() {
        let mut plugins = Map::new();
        plugins.insert(id.to_owned(), Value::Bool(true));
        let mut root = Map::new();
        root.insert("enabledPlugins".into(), Value::Object(plugins));
        let content = serde_json::to_string_pretty(&Value::Object(root))
            .map_err(|_| AppError::Internal)?
            + "\n";
        return Ok(Some(Planned {
            change: ScopeFileChange {
                file: path_text,
                location: edit::location(&[key("enabledPlugins"), key(id)]),
                before: None,
                after: "true".into(),
                is_new_file: true,
            },
            path,
            original: None,
            content,
            creates_dir: !dir.exists(),
        }));
    }
    let (bytes, text) = read_text(&path)?;
    let ops = [JsonOp::Set(
        vec![key("enabledPlugins"), key(id)],
        Value::Bool(true),
    )];
    let json = edit::apply_json_ops(&text, &ops).map_err(refuse)?;
    let Some(change) = json.changes.into_iter().next() else {
        return Ok(None);
    };
    Ok(Some(Planned {
        change: ScopeFileChange {
            file: path_text,
            location: change.location,
            before: change.before,
            after: change.after.unwrap_or_default(),
            is_new_file: false,
        },
        path,
        original: Some(bytes),
        content: json.content,
        creates_dir: false,
    }))
}

/// 프로젝트 파일들(이미 켜져 있는 곳은 빼고) + 마지막에 사용자 설정.
fn plan(switch: &CoachSwitch) -> Result<(Vec<Planned>, &ScopeMove), AppError> {
    let moving = switch.scope_move.as_ref().ok_or_else(|| {
        refuse(pick(
            "이 항목은 프로젝트에서만 켜기 대상이 아닙니다.",
            "This item isn't a candidate for turning on only in projects.",
        ))
    })?;
    let mut planned = Vec::new();
    for project in &moving.projects {
        let dir = Path::new(&project.path);
        planned.extend(if switch.agent == Agent::Codex {
            codex::plan_project(dir, codex::section(switch)?)?
        } else {
            plan_project(dir, &switch.name)?
        });
    }
    let path = switch.path().ok_or(AppError::Internal)?;
    let (bytes, text) = read_text(&path)?;
    let user = switch.plan(&text, false).map_err(refuse)?;
    if !user.changed {
        return Err(codex::already_off());
    }
    planned.push(Planned {
        change: ScopeFileChange {
            file: path.to_string_lossy().into_owned(),
            location: user.location,
            before: user.before,
            after: user.after,
            is_new_file: false,
        },
        path,
        original: Some(bytes),
        content: user.content,
        creates_dir: false,
    });
    Ok((planned, moving))
}

/// 모든 대상 파일의 경로와 지금 내용(없으면 표시)으로 만든 해시.
pub(super) fn plan_hash(planned: &[Planned]) -> String {
    let mut all = Vec::new();
    for p in planned {
        all.extend(p.path.to_string_lossy().as_bytes());
        all.push(0);
        match &p.original {
            Some(bytes) => all.extend(bytes),
            None => all.extend(b"\x01missing"),
        }
        all.push(0);
    }
    edit::content_hash(&all)
}

pub fn preview(switch: &CoachSwitch) -> Result<ScopeMovePreview, AppError> {
    let (planned, moving) = plan(switch)?;
    Ok(ScopeMovePreview {
        target: switch.target.clone(),
        before_hash: plan_hash(&planned),
        skipped: moving.skipped.clone(),
        changes: planned.into_iter().map(|p| p.change).collect(),
    })
}

/// 쓴 것 하나(실패하면 거꾸로 되돌린다).
pub(super) enum Done {
    Replaced { path: PathBuf, backup: PathBuf },
    Created { path: PathBuf, created_dir: bool },
}

pub(super) fn create_file(planned: &Planned) -> Result<Done, AppError> {
    let dir = planned.path.parent().ok_or(AppError::Internal)?;
    if planned.creates_dir {
        std::fs::create_dir(dir)
            .map_err(|e| io_refusal(pick("폴더 만들기", "create the folder"), &e))?;
    }
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&planned.path)
        .and_then(|mut f| f.write_all(planned.content.as_bytes()));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&planned.path);
        if planned.creates_dir {
            let _ = std::fs::remove_dir(dir);
        }
        return Err(io_refusal(
            pick("설정 파일 만들기", "create the settings file"),
            &error,
        ));
    }
    Ok(Done::Created {
        path: planned.path.clone(),
        created_dir: planned.creates_dir,
    })
}

fn roll_back(done: &[Done]) {
    for step in done.iter().rev() {
        let result = match step {
            Done::Replaced { path, backup } => {
                std::fs::read(backup).and_then(|original| replace_file(path, &original))
            }
            Done::Created { path, created_dir } => std::fs::remove_file(path).map(|()| {
                if *created_dir && let Some(dir) = path.parent() {
                    let _ = std::fs::remove_dir(dir);
                }
            }),
        };
        if let Err(error) = result {
            eprintln!("kcal: 프로젝트에서만 켜기 되돌림 실패({:?})", error.kind());
        }
    }
}

#[cfg(windows)]
fn remove_command(path: &Path, dir: Option<&Path>) -> String {
    let file = format!("del \"{}\"", path.display());
    dir.map_or(file.clone(), |d| {
        format!("{file} && rmdir \"{}\"", d.display())
    })
}

#[cfg(not(windows))]
fn remove_command(path: &Path, dir: Option<&Path>) -> String {
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"));
    let file = format!("rm {}", quote(path));
    dir.map_or(file.clone(), |d| format!("{file} && rmdir {}", quote(d)))
}

/// 사용자 설정을 먼저, 그다음 프로젝트 파일(만든 순의 반대)을 되돌리는 명령.
pub(super) fn undo_command(done: &[Done]) -> String {
    done.iter()
        .rev()
        .map(|step| match step {
            Done::Replaced { path, backup } => copy_command(backup, path),
            Done::Created { path, created_dir } => {
                remove_command(path, created_dir.then(|| path.parent()).flatten())
            }
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

fn log_name(id: &str, moving: &ScopeMove) -> String {
    let names = moving
        .projects
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    crate::tr!(
        "{id}: 프로젝트에서만 켬({names})",
        "{id}: on only in {names}"
    )
}

/// 미리보기 때와 파일들이 같을 때만, 프로젝트 파일 → 사용자 설정 순으로 쓴다. 중간에 실패하면 쓴 것을 되돌린다.
/// 호출하는 쪽이 `WRITE_LOCK`을 잡는다.
pub fn apply(
    switch: &CoachSwitch,
    log_file: &Path,
    before_hash: &str,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    validate_hash(before_hash)?;
    let (planned, moving) = plan(switch)?;
    if plan_hash(&planned) != before_hash {
        return Err(refuse(pick(
            "미리보기 뒤에 설정 파일이 바뀌었어요. 아무것도 바꾸지 않았어요. 다시 미리 보세요.",
            "The settings file changed after the preview. Nothing was changed. Preview again.",
        )));
    }
    let mut done = Vec::new();
    for p in &planned {
        let step = match p.original {
            Some(_) => backup_and_replace(&p.path, &p.content, now).map(|backup| Done::Replaced {
                path: p.path.clone(),
                backup,
            }),
            None => create_file(p),
        };
        match step {
            Ok(step) => done.push(step),
            Err(error) => {
                roll_back(&done);
                return Err(error);
            }
        }
    }
    let Some((Done::Replaced { path, backup }, projects)) = done.split_last() else {
        return Err(AppError::Internal);
    };
    let files = projects
        .iter()
        .zip(&planned)
        .map(|(step, p)| {
            let after_hash = edit::content_hash(p.content.as_bytes());
            match step {
                Done::Replaced { path, backup } => LoggedFile {
                    file: path.to_string_lossy().into_owned(),
                    backup: Some(backup.to_string_lossy().into_owned()),
                    after_hash,
                    created_dir: false,
                },
                Done::Created { path, created_dir } => LoggedFile {
                    file: path.to_string_lossy().into_owned(),
                    backup: None,
                    after_hash,
                    created_dir: *created_dir,
                },
            }
        })
        .collect();
    let user_content = &planned.last().ok_or(AppError::Internal)?.content;
    let entry = AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Apply,
        target: format!("{}{}", target_prefix(switch.agent), switch.name),
        agent: switch.agent,
        name: log_name(&switch.name, moving),
        file: path.to_string_lossy().into_owned(),
        backup: backup.to_string_lossy().into_owned(),
        enabled: false,
        tokens: switch.tokens,
        after_hash: edit::content_hash(user_content.as_bytes()),
        undo_command: undo_command(&done),
        undo_of: None,
        created: None,
        files,
    };
    append_applied(log_file, &entry)?;
    Ok(entry)
}

/// 기록의 프로젝트 파일이 kcal이 쓰는 모양인가: `<절대 경로>/.claude/settings.local.json`, 백업은 그 옆.
pub(super) fn is_known_project_file(logged: &LoggedFile) -> bool {
    let file = Path::new(&logged.file);
    let is_local_file = is_plain_absolute(file)
        && file.file_name() == Some(LOCAL_FILE.as_ref())
        && file.parent().and_then(Path::file_name) == Some(CLAUDE_DIR.as_ref());
    is_local_file
        && !has_project_link(file)
        && logged
            .backup
            .as_deref()
            .is_none_or(|b| is_backup_of(Path::new(b), file))
}

pub(super) fn current_hash(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| edit::content_hash(&b))
}

/// 한 기록의 모든 파일이 적용 뒤 그대로일 때만 함께 되돌린다. 새로 만든 파일은 지운다.
/// 호출하는 쪽이 `WRITE_LOCK`을 잡고, 이미 되돌린 기록인지 본다.
pub fn undo(
    home: &Path,
    applied: &AppliedSwitch,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    let file = PathBuf::from(&applied.file);
    let backup = PathBuf::from(&applied.backup);
    let is_known_file: fn(&LoggedFile) -> bool = if applied.target.starts_with(codex::TARGET_PREFIX)
    {
        codex::is_known_project_file
    } else {
        is_known_project_file
    };
    let is_known = is_scope_target(&applied.target)
        && switches::writable_files(home).contains(&file)
        && is_backup_of(&backup, &file)
        && applied.files.iter().all(is_known_file);
    if !is_known {
        return Err(refuse(pick(
            "기록의 파일·백업 경로가 kcal이 만든 것이 아니라 되돌리지 않아요.",
            "The file or backup path in the record wasn't made by kcal, so it won't be undone.",
        )));
    }
    let is_untouched = current_hash(&file).as_deref() == Some(applied.after_hash.as_str())
        && applied
            .files
            .iter()
            .all(|f| current_hash(Path::new(&f.file)).as_deref() == Some(f.after_hash.as_str()));
    if !is_untouched {
        return Err(refuse(crate::tr!(
            "적용 뒤에 설정 파일이 바뀌어 앱에서 되돌리지 않아요. 그 변경을 버려도 되면 직접 되돌리세요: {}",
            "The settings file changed after the change was applied, so kcal won't undo it. If you can drop that edit, undo it yourself: {}",
            applied.undo_command
        )));
    }
    let original =
        std::fs::read(&backup).map_err(|e| io_refusal(pick("백업 읽기", "read the backup"), &e))?;
    replace_file(&file, &original)
        .map_err(|e| io_refusal(pick("설정 파일 되돌리기", "restore the settings file"), &e))?;
    for logged in applied.files.iter().rev() {
        let path = Path::new(&logged.file);
        let result = match &logged.backup {
            Some(b) => std::fs::read(b).and_then(|bytes| replace_file(path, &bytes)),
            None => std::fs::remove_file(path).map(|()| {
                if logged.created_dir
                    && let Some(dir) = path.parent()
                {
                    // 비어 있을 때만 지워진다(그 사이 다른 파일이 생겼으면 둔다).
                    let _ = std::fs::remove_dir(dir);
                }
            }),
        };
        result.map_err(|e| {
            io_refusal(
                pick("프로젝트 설정 되돌리기", "restore the project settings"),
                &e,
            )
        })?;
    }
    Ok(AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Undo,
        enabled: true,
        after_hash: edit::content_hash(&original),
        undo_of: Some(applied.id.clone()),
        files: Vec::new(),
        ..applied.clone()
    })
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
