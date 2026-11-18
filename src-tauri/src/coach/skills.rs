//! 진단하기 스킬 단위(docs/decisions/0022 "갱신 2026-11-19"): Claude Code 개인 스킬(`~/.claude/skills/<이름>/SKILL.md`)과
//! 작업 폴더의 프로젝트 스킬(`<폴더>/.claude/skills/<이름>/SKILL.md`)의 설명을 숨긴다.
//!
//! 바꾸는 값은 설정 키 하나 `skillOverrides.<이름> = "user-invocable-only"`(Claude에게는 안 보이고 `/이름`으로는 부를 수 있다).
//! 개인 스킬은 사용자 `settings.json`, 프로젝트 스킬은 그 폴더 `.claude/settings.local.json`(커밋하지 않는 파일)에 쓴다.
//! 다시 보이기 = 그 키를 지움. 플러그인 스킬은 `skillOverrides`가 안 먹어서 목록에 넣지 않는다(공식 문서, 0022).
//! 쓰기·되돌리기는 0014처럼 백업 → 쓰기 → 기록 한 줄이고, 파일이 없었으면 새로 만들고 되돌릴 때 지운다(`scope`의 도구를 쓴다).

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;

use super::edit::{self, JsonOp, key};
use super::log::{AppliedSwitch, CoachAction, LoggedFile};
use super::scope::{self, Done, Planned, ScopeFileChange};
use super::switches::{CoachSwitch, Field, SwitchFormat, SwitchKind, base, file_switch};
use super::usage::{UsageIndex, folder_name};
use super::{
    SwitchPreview, append_applied, backup_and_replace, io_refusal, is_backup_of, read_text,
    record_id, refuse, replace_file, validate_hash,
};
use crate::context::{fsutil, paths};
use crate::error::AppError;
use crate::i18n::pick;
use crate::usage::Agent;

/// 개인 스킬 대상 이름의 앞부분(`claude/skill:<이름>`). TS 짝은 `ipc.ts` `SKILL_TARGET_PREFIX`.
pub const PERSONAL_PREFIX: &str = "claude/skill:";
/// 프로젝트 스킬 대상 이름의 앞부분(`claude/project-skill:<이름>`).
pub const PROJECT_PREFIX: &str = "claude/project-skill:";
/// 숨기는 값: 설명이 Claude에게 안 실리고 `/이름`으로는 부를 수 있다(손실 없음).
pub const HIDDEN: &str = "user-invocable-only";
const KEY: &str = "skillOverrides";

/// 스킬 스위치의 덧붙임. TS 짝은 `ipc.ts` `SkillInfo`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    /// `"on"`이 아닌 현재 `skillOverrides` 값(어느 범위든). 없으면 `None`.
    pub state: Option<String>,
    /// 프로젝트 스킬이면 그 폴더 이름.
    pub project: Option<String>,
}

pub fn is_skill_target(target: &str) -> bool {
    target.starts_with(PERSONAL_PREFIX) || target.starts_with(PROJECT_PREFIX)
}

pub fn is_skill(switch: &CoachSwitch) -> bool {
    matches!(switch.kind, SwitchKind::Skill | SwitchKind::ProjectSkill)
}

/// `<root>/<이름>/SKILL.md`가 있는 스킬 이름(이름순). 폴더 이름과 다른 front matter `name`이 있으면 건너뛴다
/// (`skillOverrides`·사용 기록이 어느 이름을 쓰는지 폴더만으로는 알 수 없다).
fn names_in(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            let file = dir.join("SKILL.md");
            let folder = entry.file_name().to_string_lossy().into_owned();
            let text = fsutil::read_text(&file);
            let declared = fsutil::front_matter_key(&text, "name");
            (file.is_file() && declared.is_none_or(|n| n == folder)).then_some(folder)
        })
        .collect();
    names.sort();
    names
}

fn override_in(settings: Option<&Value>, name: &str) -> Option<String> {
    settings?
        .get(KEY)?
        .get(name)?
        .as_str()
        .filter(|v| *v != "on")
        .map(str::to_owned)
}

/// 쓰는 파일: 개인 `~/.claude/settings.json`, 프로젝트 `<폴더>/.claude/settings.local.json`.
fn write_file(home: &Path, project: Option<&Path>) -> PathBuf {
    project.map_or_else(
        || paths::claude_root(home).join("settings.json"),
        |dir| dir.join(".claude").join("settings.local.json"),
    )
}

fn instruction(state: &str) -> String {
    crate::tr!(
        "이미 {state}(으)로 정해 둔 스킬이에요. 직접 정한 값이라 kcal이 바꾸지 않아요.",
        "This skill is already set to {state}. It's your own setting, so kcal won't change it."
    )
}

/// 스킬 스위치 한 줄. `project`가 있으면 프로젝트 스킬. `files`: 0은 쓰는 파일, 1~은 그 밖에서 값을 찾을 파일.
fn skill_switch(
    home: &Path,
    name: &str,
    project: Option<&Path>,
    user: Option<&Value>,
    usage: Option<&UsageIndex>,
) -> CoachSwitch {
    let kind = if project.is_some() {
        SwitchKind::ProjectSkill
    } else {
        SwitchKind::Skill
    };
    let write = write_file(home, project);
    // 쓰는 파일과 그 밖의 설정 파일(프로젝트 스킬: 사용자·프로젝트 공유; 개인 스킬: 이 작업 폴더의 두 파일은 보지 않는다).
    let own = project.and_then(|_| fsutil::read_json(&write));
    let others: Vec<Option<Value>> = match project {
        Some(dir) => vec![
            user.cloned(),
            fsutil::read_json(&dir.join(".claude").join("settings.json")),
        ],
        None => Vec::new(),
    };
    let write_is_user = project.is_none();
    let own_state = override_in(if write_is_user { user } else { own.as_ref() }, name);
    let other_states: Vec<String> = others
        .iter()
        .filter_map(|v| override_in(v.as_ref(), name))
        .collect();
    let state = own_state.clone().or_else(|| other_states.first().cloned());
    let is_restorable = own_state.as_deref() == Some(HIDDEN) && other_states.is_empty();
    let mut switch = base(Agent::Claude, kind, name, state.is_none());
    switch.skill = Some(SkillInfo {
        state: state.clone(),
        project: project.map(folder_name),
    });
    switch.usage = usage.map(|index| index.for_skill(name, project));
    // 0022과 같은 근거: 기록을 다 읽었고, 세션이 기준 이상이고, 0회. 값이 이미 정해져 있으면(`enabled`가 false) 사용자의 선택이다.
    switch.is_proposed = switch.enabled
        && switch
            .usage
            .as_ref()
            .is_some_and(|u| u.total == 0 && !u.is_partial && u.sessions >= u.min_sessions);
    match &state {
        None => {
            switch = file_switch(switch, SwitchFormat::Json, &write, Field::None);
        }
        Some(_) if is_restorable => {
            switch = file_switch(switch, SwitchFormat::Json, &write, Field::None);
        }
        Some(value) => switch.instruction = Some(instruction(value)),
    }
    switch
}

/// 개인 스킬 + 작업 폴더의 프로젝트 스킬. `user`: 사용자 `settings.json`. 플러그인 스킬은 넣지 않는다.
pub fn list(
    home: &Path,
    user: Option<&Value>,
    usage: Option<&UsageIndex>,
    workspace: Option<&Path>,
) -> Vec<CoachSwitch> {
    let mut all: Vec<CoachSwitch> = names_in(&paths::claude_root(home).join("skills"))
        .iter()
        .map(|name| skill_switch(home, name, None, user, usage))
        .collect();
    if let Some(dir) = workspace.filter(|d| scope::is_project_dir(home, d)) {
        all.extend(
            names_in(&dir.join(".claude").join("skills"))
                .iter()
                .map(|name| skill_switch(home, name, Some(dir), user, usage)),
        );
    }
    all
}

/// 새 값이 들어갈 자리가 이미 그 값인지·kcal이 바꿀 수 있는 값인지 보고 쓸 내용을 만든다.
fn plan(switch: &CoachSwitch, enabled: bool) -> Result<Planned, AppError> {
    let path = switch.path().ok_or(AppError::Internal)?;
    let is_project = switch.kind == SwitchKind::ProjectSkill;
    if is_project && scope::has_project_link(&path) {
        return Err(refuse(crate::tr!(
            "{}의 설정 폴더나 파일이 링크라 프로젝트 밖을 고칠 수 있어 고치지 않아요.",
            "The settings folder or file in {} is a link that could point outside the project, so kcal won't edit it.",
            path.display()
        )));
    }
    if enabled == switch.enabled {
        return Err(refuse(if enabled {
            pick(
                "이미 보이는 스킬이에요.",
                "The description is already shown.",
            )
        } else {
            pick("이미 숨겼어요.", "It's already hidden.")
        }));
    }
    let path_key = vec![key(KEY), key(&switch.name)];
    let op = if enabled {
        JsonOp::Remove(path_key)
    } else {
        JsonOp::Set(path_key, Value::String(HIDDEN.into()))
    };
    let exists = std::fs::symlink_metadata(&path).is_ok();
    let (original, text) = if exists {
        let (bytes, text) = read_text(&path)?;
        (Some(bytes), text)
    } else {
        (None, "{}\n".to_owned())
    };
    let json = edit::apply_json_ops(&text, &[op]).map_err(refuse)?;
    let change = json
        .changes
        .into_iter()
        .next()
        .ok_or_else(|| refuse(pick("바꿀 것이 없어요.", "There's nothing to change.")))?;
    let dir = path.parent().ok_or(AppError::Internal)?;
    Ok(Planned {
        change: ScopeFileChange {
            file: path.to_string_lossy().into_owned(),
            location: change.location,
            before: change.before.map(|v| format!("\"{}\": {v}", switch.name)),
            after: change.after.map_or_else(
                || pick("(이 줄을 지움)", "(line removed)").into(),
                |v| format!("\"{}\": {v}", switch.name),
            ),
            is_new_file: original.is_none(),
        },
        creates_dir: original.is_none() && !dir.exists(),
        path,
        original,
        content: json.content,
    })
}

pub fn preview(switch: &CoachSwitch, enabled: bool) -> Result<SwitchPreview, AppError> {
    let planned = plan(switch, enabled)?;
    Ok(SwitchPreview {
        target: switch.target.clone(),
        enabled,
        file: planned.change.file.clone(),
        location: planned.change.location.clone(),
        before: planned.change.before.clone(),
        after: planned.change.after.clone(),
        before_hash: scope::plan_hash(std::slice::from_ref(&planned)),
        changed: true,
    })
}

/// 호출하는 쪽이 `WRITE_LOCK`을 잡는다.
pub fn apply(
    switch: &CoachSwitch,
    log_file: &Path,
    enabled: bool,
    before_hash: &str,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    validate_hash(before_hash)?;
    let planned = plan(switch, enabled)?;
    if scope::plan_hash(std::slice::from_ref(&planned)) != before_hash {
        return Err(refuse(pick(
            "미리보기 뒤에 설정 파일이 바뀌었어요. 아무것도 바꾸지 않았어요. 다시 미리 보세요.",
            "The settings file changed after the preview. Nothing was changed. Preview again.",
        )));
    }
    let done = match planned.original {
        Some(_) => backup_and_replace(&planned.path, &planned.content, now).map(|backup| {
            Done::Replaced {
                path: planned.path.clone(),
                backup,
            }
        })?,
        None => scope::create_file(&planned)?,
    };
    let after_hash = edit::content_hash(planned.content.as_bytes());
    let (backup, created_dir) = match &done {
        Done::Replaced { backup, .. } => (Some(backup.to_string_lossy().into_owned()), false),
        Done::Created { created_dir, .. } => (None, *created_dir),
    };
    let file = planned.path.to_string_lossy().into_owned();
    let entry = AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Apply,
        target: switch.target.clone(),
        agent: Agent::Claude,
        name: switch.name.clone(),
        file: file.clone(),
        backup: backup.clone().unwrap_or_default(),
        enabled,
        tokens: None,
        after_hash: after_hash.clone(),
        undo_command: scope::undo_command(&[done]),
        undo_of: None,
        created: None,
        files: vec![LoggedFile {
            file,
            backup,
            after_hash,
            created_dir,
        }],
    };
    append_applied(log_file, &entry)?;
    Ok(entry)
}

/// 기록의 파일이 kcal이 쓰는 모양인가: 개인은 사용자 `settings.json`, 프로젝트는 `<폴더>/.claude/settings.local.json`.
fn is_known(home: &Path, applied: &AppliedSwitch) -> bool {
    let [logged] = applied.files.as_slice() else {
        return false;
    };
    let file = Path::new(&logged.file);
    let is_shape = if applied.target.starts_with(PROJECT_PREFIX) {
        scope::is_known_project_file(logged)
    } else {
        file == paths::claude_root(home).join("settings.json")
            && logged
                .backup
                .as_deref()
                .is_none_or(|b| is_backup_of(Path::new(b), file))
    };
    is_skill_target(&applied.target) && logged.file == applied.file && is_shape
}

/// 적용 뒤 그대로일 때만 되돌린다: 백업이 있으면 복사하고, 없으면(새로 만든 파일) 지운다.
/// 호출하는 쪽이 `WRITE_LOCK`을 잡고, 이미 되돌린 기록인지 본다.
pub fn undo(
    home: &Path,
    applied: &AppliedSwitch,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    if !is_known(home, applied) {
        return Err(refuse(pick(
            "기록의 파일·백업 경로가 kcal이 만든 것이 아니라 되돌리지 않아요.",
            "The file or backup path in the record wasn't made by kcal, so it won't be undone.",
        )));
    }
    let logged = &applied.files[0];
    let path = Path::new(&logged.file);
    if scope::current_hash(path).as_deref() != Some(logged.after_hash.as_str()) {
        return Err(refuse(crate::tr!(
            "적용 뒤에 설정 파일이 바뀌어 앱에서 되돌리지 않아요. 그 변경을 버려도 되면 직접 되돌리세요: {}",
            "The settings file changed after the change was applied, so kcal won't undo it. If you can drop that edit, undo it yourself: {}",
            applied.undo_command
        )));
    }
    match &logged.backup {
        Some(backup) => {
            let original = std::fs::read(backup)
                .map_err(|e| io_refusal(pick("백업 읽기", "read the backup"), &e))?;
            replace_file(path, &original)
        }
        None => std::fs::remove_file(path).map(|()| {
            if logged.created_dir
                && let Some(dir) = path.parent()
            {
                // 비어 있을 때만 지워진다(그 사이 다른 파일이 생겼으면 둔다).
                let _ = std::fs::remove_dir(dir);
            }
        }),
    }
    .map_err(|e| io_refusal(pick("설정 파일 되돌리기", "restore the settings file"), &e))?;
    Ok(AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Undo,
        enabled: !applied.enabled,
        undo_of: Some(applied.id.clone()),
        files: Vec::new(),
        ..applied.clone()
    })
}

#[cfg(test)]
#[path = "skills_tests.rs"]
mod tests;
