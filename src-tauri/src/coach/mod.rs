//! 5층 코칭(docs/decisions/0014, ctxmeter `fix.js`): 켜고 끄는 스위치 제안 → 미리보기 → 적용(백업) → 앱 안 되돌리기.
//!
//! 쓰는 파일은 `switches::writable_files` 세 개뿐이고, 값 하나만 바꾼다. command는 대상 ID·기록 ID만 받는다.
//! TS 쪽 짝은 `src/lib/ipc.ts`의 같은 이름 타입.

pub(crate) mod checks;
pub(crate) mod contents;
pub(crate) mod edit;
pub(crate) mod log;
pub(crate) mod scope;
pub(crate) mod switches;
pub(crate) mod usage;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;
use time::macros::format_description;

use crate::context::{self, ContextSnapshot};
use crate::error::AppError;
use crate::i18n::pick;
use crate::settings::SettingsState;
use crate::tuning::Tuning;

use log::{AppliedSwitch, CoachAction};
use scope::ScopeMovePreview;
use switches::CoachSwitch;
#[cfg(test)]
use switches::SwitchFormat;
use usage::UsageIndex;

/// 미리보기. `before_hash`를 적용 요청에 그대로 보낸다.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchPreview {
    pub target: String,
    pub enabled: bool,
    pub file: String,
    /// 사람이 찾을 위치(section·키 경로와 줄 번호).
    pub location: String,
    /// 바꾸기 전 줄. 새로 넣는 줄이면 `None`.
    pub before: Option<String>,
    pub after: String,
    /// 지금 파일 내용의 해시(FNV-1a 64, 변경 감지용).
    pub before_hash: String,
    /// `false`면 이미 그 값이라 적용할 것이 없다.
    pub changed: bool,
}

/// 적용·되돌리기는 한 번에 하나만(같은 파일을 두 요청이 동시에 쓰지 않게).
static WRITE_LOCK: Mutex<()> = Mutex::new(());

const MAX_TARGET_LEN: usize = 512;
const HASH_LEN: usize = 16;
const MAX_ID_LEN: usize = 40;

fn refuse(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

fn validate_target(target: &str) -> Result<(), AppError> {
    if target.is_empty() || target.len() > MAX_TARGET_LEN {
        return Err(refuse(pick(
            "스위치 이름이 올바르지 않습니다.",
            "That switch name isn't valid.",
        )));
    }
    Ok(())
}

fn io_refusal(what: &str, error: &std::io::Error) -> AppError {
    eprintln!("kcal: 코칭 {what} 실패({:?})", error.kind());
    let kind = error.kind();
    refuse(crate::tr!(
        "{what}에 실패했습니다({kind:?}).",
        "Couldn't {what} ({kind:?})."
    ))
}

fn read_text(path: &Path) -> Result<(Vec<u8>, String), AppError> {
    let bytes = std::fs::read(path)
        .map_err(|e| io_refusal(pick("설정 파일 읽기", "read the settings file"), &e))?;
    let text = String::from_utf8(bytes.clone()).map_err(|_| {
        refuse(pick(
            "설정 파일이 UTF-8이 아니라 고치지 않습니다.",
            "The settings file isn't UTF-8, so kcal won't edit it.",
        ))
    })?;
    Ok((bytes, text))
}

fn switch_and_file(
    home: &Path,
    snapshot: Option<&ContextSnapshot>,
    usage: Option<&UsageIndex>,
    tuning: &Tuning,
    target: &str,
) -> Result<(CoachSwitch, PathBuf), AppError> {
    validate_target(target)?;
    let switch =
        switches::find(switches::list(home, snapshot, usage, tuning), target).map_err(refuse)?;
    let path = switch.path().ok_or_else(|| {
        refuse(switch.instruction.clone().unwrap_or_else(|| {
            pick(
                "kcal이 바꾸지 않는 스위치입니다.",
                "kcal doesn't change this switch.",
            )
            .into()
        }))
    })?;
    Ok((switch, path))
}

pub fn preview(
    home: &Path,
    snapshot: Option<&ContextSnapshot>,
    usage: Option<&UsageIndex>,
    tuning: &Tuning,
    target: &str,
    enabled: bool,
) -> Result<SwitchPreview, AppError> {
    let (switch, path) = switch_and_file(home, snapshot, usage, tuning, target)?;
    let (bytes, text) = read_text(&path)?;
    let edit = switch.plan(&text, enabled).map_err(refuse)?;
    Ok(SwitchPreview {
        target: switch.target,
        enabled,
        file: path.to_string_lossy().into_owned(),
        location: edit.location,
        before: edit.before,
        after: edit.after,
        before_hash: edit::content_hash(&bytes),
        changed: edit.changed,
    })
}

/// `<이름>.kcal-<UTC 시각>.bak`(원본 옆).
fn backup_path(file: &Path, now: OffsetDateTime) -> Result<PathBuf, AppError> {
    let stamp = now
        .format(format_description!(
            "[year][month][day]T[hour][minute][second][subsecond digits:3]Z"
        ))
        .map_err(|_| AppError::Internal)?;
    let name = file
        .file_name()
        .ok_or(AppError::Internal)?
        .to_string_lossy();
    Ok(file.with_file_name(format!("{name}.kcal-{stamp}.bak")))
}

/// 백업이 원본과 같은 폴더의 `<원본 이름>.kcal-*.bak`인지(기록 파일이 바뀌었어도 다른 파일을 덮지 않게).
fn is_backup_of(backup: &Path, file: &Path) -> bool {
    let (Some(name), Some(backup_name)) = (file.file_name(), backup.file_name()) else {
        return false;
    };
    let (name, backup_name) = (name.to_string_lossy(), backup_name.to_string_lossy());
    backup.parent() == file.parent()
        && backup_name.starts_with(&format!("{name}.kcal-"))
        && backup_name.ends_with(".bak")
        && !backup_name.contains(['/', '\\'])
}

/// 같은 폴더 임시 파일에 쓰고 rename한다(반쯤 쓴 파일이 보이지 않게). 권한을 유지하고, 링크면 가리키는 파일에 쓴다.
fn replace_file(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let target = std::fs::canonicalize(path)?;
    let permissions = std::fs::metadata(&target)?.permissions();
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?
        .to_string_lossy()
        .into_owned();
    let temp = target.with_file_name(format!(".{name}.kcal-{}.tmp", std::process::id()));
    let result = std::fs::write(&temp, content)
        .and_then(|()| std::fs::set_permissions(&temp, permissions))
        .and_then(|()| std::fs::rename(&temp, &target));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(windows)]
fn copy_command(from: &Path, to: &Path) -> String {
    format!("copy /Y \"{}\" \"{}\"", from.display(), to.display())
}

#[cfg(not(windows))]
fn copy_command(from: &Path, to: &Path) -> String {
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"));
    format!("cp {} {}", quote(from), quote(to))
}

fn record_id(now: OffsetDateTime) -> String {
    now.unix_timestamp_nanos().to_string()
}

fn validate_hash(before_hash: &str) -> Result<(), AppError> {
    if before_hash.len() != HASH_LEN || !before_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(refuse(pick(
            "미리보기 값이 올바르지 않습니다. 다시 미리 보세요.",
            "The preview is no longer valid. Preview again.",
        )));
    }
    Ok(())
}

/// 지금 내용이 미리보기 때와 같을 때만 읽은 글을 준다(에이전트가 실행 중에 쓴 변경을 덮지 않는다).
fn read_unchanged(path: &Path, before_hash: &str) -> Result<String, AppError> {
    let (bytes, text) = read_text(path)?;
    if edit::content_hash(&bytes) != before_hash {
        return Err(refuse(pick(
            "미리보기 뒤에 설정 파일이 바뀌었습니다. 아무것도 바꾸지 않았습니다. 다시 미리 보세요.",
            "The settings file changed after the preview. Nothing was changed. Preview again.",
        )));
    }
    Ok(text)
}

/// 원본 옆에 백업을 만들고 `content`로 바꾼다. 백업 경로를 돌려준다.
fn backup_and_replace(
    path: &Path,
    content: &str,
    now: OffsetDateTime,
) -> Result<PathBuf, AppError> {
    let backup = backup_path(path, now)?;
    if backup.exists() {
        return Err(refuse(pick(
            "같은 이름의 백업이 이미 있습니다. 잠시 뒤 다시 하세요.",
            "A backup with the same name already exists. Try again in a moment.",
        )));
    }
    std::fs::copy(path, &backup)
        .map_err(|e| io_refusal(pick("백업 만들기", "create the backup"), &e))?;
    replace_file(path, content.as_bytes())
        .map_err(|e| io_refusal(pick("설정 파일 쓰기", "write the settings file"), &e))?;
    Ok(backup)
}

fn append_applied(log_file: &Path, entry: &AppliedSwitch) -> Result<(), AppError> {
    log::append(log_file, entry).map_err(|e| {
        eprintln!("kcal: 코칭 기록 쓰기 실패({:?})", e.kind());
        refuse(crate::tr!(
            "설정은 바꿨지만 기록을 남기지 못해 앱에서 되돌릴 수 없습니다. 되돌리려면: {}",
            "The setting changed, but kcal couldn't log it, so it can't be undone here. To undo: {}",
            entry.undo_command
        ))
    })
}

#[allow(clippy::too_many_arguments)]
pub fn apply(
    home: &Path,
    snapshot: Option<&ContextSnapshot>,
    usage: Option<&UsageIndex>,
    tuning: &Tuning,
    log_file: &Path,
    target: &str,
    enabled: bool,
    before_hash: &str,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    validate_hash(before_hash)?;
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Internal)?;
    let (switch, path) = switch_and_file(home, snapshot, usage, tuning, target)?;
    let text = read_unchanged(&path, before_hash)?;
    let edit = switch.plan(&text, enabled).map_err(refuse)?;
    if !edit.changed {
        return Err(refuse(if enabled {
            pick("이미 켜져 있습니다.", "It's already on.")
        } else {
            pick("이미 꺼져 있습니다.", "It's already off.")
        }));
    }
    let backup = backup_and_replace(&path, &edit.content, now)?;
    let entry = AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Apply,
        target: switch.target,
        agent: switch.agent,
        name: switch.name,
        file: path.to_string_lossy().into_owned(),
        backup: backup.to_string_lossy().into_owned(),
        enabled,
        tokens: switch.tokens,
        after_hash: edit::content_hash(edit.content.as_bytes()),
        undo_command: copy_command(&backup, &path),
        undo_of: None,
        created: None,
        files: Vec::new(),
    };
    append_applied(log_file, &entry)?;
    Ok(entry)
}

/// "프로젝트에서만 켜기" 미리보기(0022). 대상은 스위치 목록 안의 Claude 플러그인이고, 제안이 있어야 한다.
pub fn preview_move(
    home: &Path,
    snapshot: Option<&ContextSnapshot>,
    usage: Option<&UsageIndex>,
    tuning: &Tuning,
    target: &str,
) -> Result<ScopeMovePreview, AppError> {
    let (switch, _) = switch_and_file(home, snapshot, usage, tuning, target)?;
    scope::preview(&switch)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_move(
    home: &Path,
    snapshot: Option<&ContextSnapshot>,
    usage: Option<&UsageIndex>,
    tuning: &Tuning,
    log_file: &Path,
    target: &str,
    before_hash: &str,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Internal)?;
    let (switch, _) = switch_and_file(home, snapshot, usage, tuning, target)?;
    scope::apply(&switch, log_file, before_hash, now)
}

pub fn undo(
    home: &Path,
    log_file: &Path,
    id: &str,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    if id.is_empty()
        || id.len() > MAX_ID_LEN
        || !id.bytes().all(|b| b.is_ascii_digit() || b == b'-')
    {
        return Err(refuse(pick(
            "기록 ID가 올바르지 않습니다.",
            "That record ID isn't valid.",
        )));
    }
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Internal)?;
    let entries = log::read(log_file);
    let applied = entries
        .iter()
        .find(|e| e.id == id && e.action == CoachAction::Apply)
        .ok_or_else(|| {
            refuse(pick(
                "되돌릴 적용 기록이 없습니다.",
                "There's no applied change to undo.",
            ))
        })?;
    if entries.iter().any(|e| e.undo_of.as_deref() == Some(id)) {
        return Err(refuse(pick(
            "이미 되돌린 기록입니다.",
            "This change was already undone.",
        )));
    }
    // 프로젝트에서만 켜기(0022): 여러 파일을 한 기록으로 함께 되돌린다.
    if !applied.files.is_empty() || applied.target.starts_with(scope::SCOPE_TARGET_PREFIX) {
        let entry = scope::undo(home, applied, now)?;
        log::append(log_file, &entry)
            .map_err(|e| io_refusal(pick("기록 쓰기", "write the change log"), &e))?;
        return Ok(entry);
    }
    // 규칙을 스킬로 옮긴 기록(0018 작업 B): 새로 만든 파일이 있어 따로 되돌린다.
    if let Some(created) = applied.created.as_deref() {
        let entry = checks::rules_apply::undo_skill(applied, created, now)?;
        log::append(log_file, &entry)
            .map_err(|e| io_refusal(pick("기록 쓰기", "write the change log"), &e))?;
        return Ok(entry);
    }
    let file = PathBuf::from(&applied.file);
    let backup = PathBuf::from(&applied.backup);
    // 설정 파일 3개, 또는 Claude에게 맡겨 고친 안내문(0018 "되돌리기": 적용 뒤 카드가 사라져도 되돌린다).
    let is_known = switches::writable_files(home).contains(&file)
        || checks::handoff::is_undoable(&applied.target, &file);
    if !is_known || !is_backup_of(&backup, &file) {
        return Err(refuse(pick(
            "기록의 파일·백업 경로가 kcal이 만든 것이 아니라 되돌리지 않습니다.",
            "The file or backup path in the record wasn't made by kcal, so it won't be undone.",
        )));
    }
    let (current, _) = read_text(&file)?;
    if edit::content_hash(&current) != applied.after_hash {
        return Err(refuse(crate::tr!(
            "적용 뒤에 설정 파일이 바뀌어 앱에서 되돌리지 않습니다. 그 변경을 버려도 되면 직접 되돌리세요: {}",
            "The settings file changed after the change was applied, so kcal won't undo it. If you can drop that edit, undo it yourself: {}",
            applied.undo_command
        )));
    }
    let original =
        std::fs::read(&backup).map_err(|e| io_refusal(pick("백업 읽기", "read the backup"), &e))?;
    replace_file(&file, &original)
        .map_err(|e| io_refusal(pick("설정 파일 되돌리기", "restore the settings file"), &e))?;
    let entry = AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Undo,
        enabled: !applied.enabled,
        after_hash: edit::content_hash(&original),
        undo_of: Some(applied.id.clone()),
        ..applied.clone()
    };
    log::append(log_file, &entry)
        .map_err(|e| io_refusal(pick("기록 쓰기", "write the change log"), &e))?;
    Ok(entry)
}

// ---------- commands ----------

fn home() -> Result<PathBuf, AppError> {
    std::env::home_dir().ok_or(AppError::Internal)
}

fn log_file(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map(|dir| log::file(&dir))
        .map_err(|_| AppError::Internal)
}

/// 측정·사용 기록(캐시)을 읽고 blocking 스레드에서 `work`를 돌린다.
async fn with_snapshot<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(
        PathBuf,
        Option<ContextSnapshot>,
        Option<UsageIndex>,
        Tuning,
    ) -> Result<T, AppError>
    + Send
    + 'static,
) -> Result<T, AppError> {
    let home = home()?;
    let snapshot = context::global_snapshot(app).await.ok();
    let tuning = app.state::<SettingsState>().tuning();
    let usage = usage::index(app, &home, tuning).await.ok();
    tauri::async_runtime::spawn_blocking(move || work(home, snapshot, usage, tuning))
        .await
        .map_err(|_| AppError::Internal)?
}

#[tauri::command]
pub async fn get_switches(app: AppHandle) -> Result<Vec<CoachSwitch>, AppError> {
    with_snapshot(&app, |home, snapshot, usage, tuning| {
        Ok(switches::list(
            &home,
            snapshot.as_ref(),
            usage.as_ref(),
            &tuning,
        ))
    })
    .await
}

#[tauri::command]
pub async fn preview_switch(
    app: AppHandle,
    target: String,
    enabled: bool,
) -> Result<SwitchPreview, AppError> {
    with_snapshot(&app, move |home, snapshot, usage, tuning| {
        preview(
            &home,
            snapshot.as_ref(),
            usage.as_ref(),
            &tuning,
            &target,
            enabled,
        )
    })
    .await
}

#[tauri::command]
pub async fn apply_switch(
    app: AppHandle,
    target: String,
    enabled: bool,
    before_hash: String,
) -> Result<AppliedSwitch, AppError> {
    let log = log_file(&app)?;
    let result = with_snapshot(&app, move |home, snapshot, usage, tuning| {
        let now = OffsetDateTime::now_utc();
        apply(
            &home,
            snapshot.as_ref(),
            usage.as_ref(),
            &tuning,
            &log,
            &target,
            enabled,
            &before_hash,
            now,
        )
    })
    .await?;
    // 화면 합계가 바로 바뀌게 기존 새로고침 경로로 다시 잰다.
    context::prewarm(app, true);
    Ok(result)
}

#[tauri::command]
pub async fn preview_scope_move(
    app: AppHandle,
    target: String,
) -> Result<ScopeMovePreview, AppError> {
    with_snapshot(&app, move |home, snapshot, usage, tuning| {
        preview_move(&home, snapshot.as_ref(), usage.as_ref(), &tuning, &target)
    })
    .await
}

#[tauri::command]
pub async fn apply_scope_move(
    app: AppHandle,
    target: String,
    before_hash: String,
) -> Result<AppliedSwitch, AppError> {
    let log = log_file(&app)?;
    let result = with_snapshot(&app, move |home, snapshot, usage, tuning| {
        apply_move(
            &home,
            snapshot.as_ref(),
            usage.as_ref(),
            &tuning,
            &log,
            &target,
            &before_hash,
            OffsetDateTime::now_utc(),
        )
    })
    .await?;
    context::prewarm(app, true);
    Ok(result)
}

#[tauri::command]
pub async fn undo_switch(app: AppHandle, id: String) -> Result<AppliedSwitch, AppError> {
    let log = log_file(&app)?;
    let home = home()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        undo(&home, &log, &id, OffsetDateTime::now_utc())
    })
    .await
    .map_err(|_| AppError::Internal)??;
    context::prewarm(app, true);
    Ok(result)
}

#[tauri::command]
pub async fn get_coach_log(app: AppHandle) -> Result<Vec<AppliedSwitch>, AppError> {
    let log = log_file(&app)?;
    tauri::async_runtime::spawn_blocking(move || log::read(&log))
        .await
        .map_err(|_| AppError::Internal)
}

#[cfg(test)]
mod tests;
