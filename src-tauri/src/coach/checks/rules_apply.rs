//! 0018 작업 B 적용·되돌리기: 보관본의 규칙 하나를 `paths` 머리말 붙이기(0014 방식) 또는 스킬로 옮기기로 쓴다.
//!
//! 스킬로 옮기기(사용자 결정 2026-11-08)는 한 기록이다: 범위의 `skills/<name>/SKILL.md`를 새로 만들고(본문 = 규칙
//! 본문 그대로), 규칙 파일은 지우지 않고 백업 이름으로 옮긴다. 기록 `created`에 SKILL.md 경로를 남긴다.
//! 되돌리기는 SKILL.md가 kcal이 쓴 그대로이고 규칙 자리가 비어 있을 때만 한다(`undo_skill`).

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use time::OffsetDateTime;

use super::handoff::{HandoffState, is_undoable};
use super::models::RecentRecords;
use super::rules_handoff::{
    Proposal, RulesTarget, StoredRule, find_target, is_key, is_skill_name, skill_dir, stored,
};
use super::rules_paths::{MAX_CHARS, body_of};
use super::target as check_target;
use crate::coach::log::{AppliedSwitch, CoachAction};
use crate::coach::{
    WRITE_LOCK, append_applied, backup_and_replace, backup_path, copy_command, edit, io_refusal,
    is_backup_of, record_id, refuse, validate_hash,
};
use crate::context::fsutil::{front_matter, front_matter_key};
use crate::error::AppError;
use crate::i18n::pick;
use crate::tuning::Tuning;
use crate::usage::Agent;

/// 적용 기록 `target` 끝(`claude/check:<id>:paths|skill`).
pub const PATHS_OPTION: &str = "paths";
pub const SKILL_OPTION: &str = "skill";
fn changed() -> &'static str {
    pick(
        "Claude에게 맡긴 뒤 파일이 바뀌었어요. 다시 맡겨 주세요.",
        "The file changed after you asked Claude. Ask Claude again.",
    )
}

/// 화면이 보내는 것: 카드 id, 보관본 열쇠, 그 규칙의 원본 해시. 글·경로는 받지 않는다.
pub struct Request<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub before_hash: &'a str,
}

/// 보관본의 규칙 하나를 적용한다. 그 규칙 파일이 지금 카드에 있고 해시가 맡긴 때와 같아야 한다.
pub fn apply(
    state: &HandoffState,
    home: &Path,
    records: &RecentRecords,
    tuning: &Tuning,
    log_file: &Path,
    request: Request<'_>,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    validate_hash(request.before_hash)?;
    if !is_key(request.key) {
        return Err(refuse(pick(
            "규칙 이름이 올바르지 않아요.",
            "That rule name isn't valid.",
        )));
    }
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Internal)?;
    let target = find_target(home, records, tuning, request.id)?;
    let stored = stored(state, request.id)?.ok_or_else(|| {
        refuse(pick(
            "Claude에게 맡긴 결과가 없어요. 다시 맡겨 주세요.",
            "There's no result from Claude. Ask Claude again.",
        ))
    })?;
    let rule = stored
        .rules
        .iter()
        .find(|r| r.key == request.key)
        .ok_or_else(|| {
            refuse(pick(
                "Claude에게 맡긴 결과에 없는 규칙이에요. 다시 맡겨 주세요.",
                "That rule isn't in Claude's result. Ask Claude again.",
            ))
        })?;
    let current = target
        .rules
        .iter()
        .find(|r| r.fact.file == rule.file)
        .ok_or_else(|| {
            refuse(pick(
                "지금 점검 목록에 없는 규칙이에요. 화면을 다시 여세요.",
                "That rule isn't in the current check list. Reopen the screen.",
            ))
        })?;
    if rule.before_hash != request.before_hash || current.before_hash != request.before_hash {
        return Err(refuse(changed()));
    }
    let entry = match &rule.proposal {
        Proposal::Paths { paths } => apply_paths(&target, rule, &current.original, paths, now)?,
        Proposal::Skill { name, description } => {
            let skill = Skill { name, description };
            apply_skill(&target, rule, &current.original, skill, now)?
        }
        Proposal::Keep => {
            return Err(refuse(pick(
                "그대로 두기는 바꿀 것이 없어요.",
                "Keeping it as is has nothing to change.",
            )));
        }
        Proposal::Unusable { .. } => {
            return Err(refuse(pick(
                "Claude 제안을 쓸 수 없어 바꾸지 않아요.",
                "Claude's suggestion can't be used, so kcal won't apply it.",
            )));
        }
    };
    append_applied(log_file, &entry)?;
    Ok(entry)
}

fn file_name(file: &Path) -> String {
    file.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn entry(
    target: &RulesTarget,
    option: &str,
    name: String,
    file: &Path,
    backup: &Path,
    now: OffsetDateTime,
) -> AppliedSwitch {
    AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Apply,
        target: check_target(&target.id, option),
        agent: Agent::Claude,
        name,
        file: file.to_string_lossy().into_owned(),
        backup: backup.to_string_lossy().into_owned(),
        enabled: false,
        tokens: None,
        after_hash: String::new(),
        undo_command: String::new(),
        undo_of: None,
        created: None,
        files: Vec::new(),
    }
}

// ---------- paths ----------

/// YAML 큰따옴표 문자열(JSON 문자열은 YAML 큰따옴표 문자열이다. 제어 문자는 검사에서 이미 뺐다).
fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}

fn paths_block(paths: &[String], newline: &str) -> String {
    let items: String = paths
        .iter()
        .map(|p| format!("  - {}{newline}", quoted(p)))
        .collect();
    format!("paths:{newline}{items}")
}

/// 머리말이 없으면 맨 앞에 새 머리말을, 있으면 여는 `---` 다음 줄에 `paths:` 목록을 넣는다.
pub fn with_paths(original: &str, paths: &[String]) -> Result<String, AppError> {
    let Some((block, _)) = front_matter(original) else {
        return Ok(format!("---\n{}---\n{original}", paths_block(paths, "\n")));
    };
    if block.lines().any(|l| l.trim_start().starts_with("paths:")) {
        return Err(refuse(pick(
            "머리말에 이미 paths 줄이 있어 바꾸지 않아요.",
            "The front matter already has a paths line, so kcal won't change it.",
        )));
    }
    let start = original
        .find('\n')
        .map(|i| i + 1)
        .ok_or(AppError::Internal)?;
    let newline = if original[..start].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    Ok(format!(
        "{}{}{}",
        &original[..start],
        paths_block(paths, newline),
        &original[start..]
    ))
}

fn apply_paths(
    target: &RulesTarget,
    rule: &StoredRule,
    original: &str,
    paths: &[String],
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    if body_of(original).chars().count() > MAX_CHARS {
        return Err(refuse(pick(
            "규칙이 40,000자를 넘어 필요할 때만 실리면 뒷부분이 잘려요. 바꾸지 않아요.",
            "The rule is over 40,000 characters, so loading it only when needed would cut the end. kcal won't change it.",
        )));
    }
    let text = with_paths(original, paths)?;
    // 0005 측정이 `paths`를 읽어야 하고(YAML이 깨지면 늘 실린다), 머리말 뒤 본문은 바이트 단위로 같아야 한다.
    if front_matter_key(&text, "paths").is_none() || body_of(&text) != body_of(original) {
        return Err(refuse(pick(
            "머리말을 붙인 글을 다시 읽지 못해 바꾸지 않아요.",
            "kcal couldn't read the text back after adding the front matter, so it won't change it.",
        )));
    }
    let backup = backup_and_replace(&rule.file, &text, now)?;
    let name = crate::tr!(
        "{} 필요할 때만 싣기",
        "Load {} only when needed",
        file_name(&rule.file)
    );
    Ok(AppliedSwitch {
        after_hash: edit::content_hash(text.as_bytes()),
        undo_command: copy_command(&backup, &rule.file),
        ..entry(target, PATHS_OPTION, name, &rule.file, &backup, now)
    })
}

// ---------- 스킬로 옮기기 ----------

struct Skill<'a> {
    name: &'a str,
    description: &'a str,
}

/// SKILL.md 내용: kcal이 붙인 머리말 + 규칙 본문 그대로.
pub fn skill_text(name: &str, description: &str, original: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: {}\n---\n{original}",
        quoted(description)
    )
}

#[cfg(windows)]
fn skill_undo_command(backup: &Path, file: &Path, skill_md: &Path, dir: &Path) -> String {
    let q = |p: &Path| format!("\"{}\"", p.display());
    format!(
        "move /Y {} {} && del {} && rmdir {}",
        q(backup),
        q(file),
        q(skill_md),
        q(dir)
    )
}

#[cfg(not(windows))]
fn skill_undo_command(backup: &Path, file: &Path, skill_md: &Path, dir: &Path) -> String {
    let q = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"));
    format!(
        "mv {} {} && rm {} && rmdir {}",
        q(backup),
        q(file),
        q(skill_md),
        q(dir)
    )
}

fn skill_exists() -> &'static str {
    pick(
        "같은 이름의 스킬이 이미 있어요. 옮기지 않아요.",
        "A skill with the same name already exists. kcal won't move it.",
    )
}

/// `skills/<name>` 폴더를 새로 만들고(있으면 실패) SKILL.md를 새로 쓴다. 실패하면 만든 것을 지운다.
fn create_skill(dir: &Path, skill_md: &Path, content: &str) -> Result<(), AppError> {
    let skills = dir.parent().ok_or(AppError::Internal)?;
    std::fs::create_dir_all(skills)
        .map_err(|e| io_refusal(pick("스킬 폴더 만들기", "create the skill folder"), &e))?;
    std::fs::create_dir(dir).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => refuse(skill_exists()),
        _ => io_refusal(pick("스킬 폴더 만들기", "create the skill folder"), &e),
    })?;
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(skill_md)
        .and_then(|mut f| f.write_all(content.as_bytes()));
    if let Err(e) = written {
        let _ = std::fs::remove_file(skill_md);
        let _ = std::fs::remove_dir(dir);
        return Err(io_refusal(
            pick("스킬 파일 쓰기", "write the skill file"),
            &e,
        ));
    }
    Ok(())
}

fn apply_skill(
    target: &RulesTarget,
    rule: &StoredRule,
    original: &str,
    skill: Skill<'_>,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    if front_matter(original).is_some() {
        return Err(refuse(pick(
            "머리말이 있는 규칙은 아직 옮기지 않아요.",
            "Rules with front matter aren't moved yet.",
        )));
    }
    let dir = skill_dir(&target.root, skill.name)
        .filter(|_| is_skill_name(skill.name))
        .ok_or(AppError::Internal)?;
    if std::fs::symlink_metadata(&dir).is_ok() {
        return Err(refuse(skill_exists()));
    }
    let backup = backup_path(&rule.file, now)?;
    if std::fs::symlink_metadata(&backup).is_ok() {
        return Err(refuse(pick(
            "같은 이름의 백업이 이미 있어요. 잠시 뒤 다시 하세요.",
            "A backup with the same name already exists. Try again in a moment.",
        )));
    }
    let skill_md = dir.join("SKILL.md");
    let content = skill_text(skill.name, skill.description, original);
    create_skill(&dir, &skill_md, &content)?;
    if let Err(e) = std::fs::rename(&rule.file, &backup) {
        let _ = std::fs::remove_file(&skill_md);
        let _ = std::fs::remove_dir(&dir);
        return Err(io_refusal(
            pick("규칙 파일 옮기기", "move the rule file"),
            &e,
        ));
    }
    let name = crate::tr!(
        "{} 스킬로 옮기기",
        "Move {} to a skill",
        file_name(&rule.file)
    );
    Ok(AppliedSwitch {
        after_hash: edit::content_hash(content.as_bytes()),
        undo_command: skill_undo_command(&backup, &rule.file, &skill_md, &dir),
        created: Some(skill_md.to_string_lossy().into_owned()),
        ..entry(target, SKILL_OPTION, name, &rule.file, &backup, now)
    })
}

// ---------- 되돌리기 ----------

/// `created`가 그 규칙 범위의 `skills/<이름>/SKILL.md` 모양인가(`<폴더>/.claude/rules/…` → `<폴더>/.claude/skills/<이름>/SKILL.md`).
pub fn is_skill_of(rule: &Path, created: &Path) -> bool {
    if !created.is_absolute() || created.components().any(|c| c == Component::ParentDir) {
        return false;
    }
    let parts: Vec<&std::ffi::OsStr> = rule.components().map(|c| c.as_os_str()).collect();
    let Some(at) = parts
        .windows(2)
        .position(|w| w[0] == ".claude" && w[1] == "rules")
    else {
        return false;
    };
    let skills: PathBuf = rule
        .components()
        .take(at + 1)
        .collect::<PathBuf>()
        .join("skills");
    let name = created
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str());
    created.file_name() == Some("SKILL.md".as_ref())
        && name.is_some_and(is_skill_name)
        && created.parent().and_then(Path::parent) == Some(skills.as_path())
}

fn refuse_with(message: &str, command: &str) -> AppError {
    refuse(crate::tr!(
        "{message} 직접 되돌리려면: {command}",
        "{message} To undo it yourself: {command}"
    ))
}

/// 스킬로 옮긴 기록 되돌리기: 백업을 규칙 자리로 되살리고 SKILL.md(와 비었으면 그 폴더)를 지운다.
/// 부르는 쪽(`coach::undo`)이 잠금·기록 찾기·이미 되돌림 검사를 한다. 기록 쓰기도 부르는 쪽이 한다.
pub fn undo_skill(
    applied: &AppliedSwitch,
    created: &str,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    let (file, backup, skill_md) = (
        PathBuf::from(&applied.file),
        PathBuf::from(&applied.backup),
        PathBuf::from(created),
    );
    let command = applied.undo_command.as_str();
    let is_ours = is_undoable(&applied.target, &file)
        && is_backup_of(&backup, &file)
        && is_skill_of(&file, &skill_md);
    if !is_ours {
        return Err(refuse_with(
            pick(
                "기록의 파일·백업 경로가 kcal이 만든 것이 아니라 되돌리지 않아요.",
                "The file or backup path in the record wasn't made by kcal, so it won't be undone.",
            ),
            command,
        ));
    }
    let current = std::fs::read(&skill_md).map_err(|_| {
        refuse_with(
            pick(
                "스킬 파일을 읽지 못해 앱에서 되돌리지 않아요.",
                "kcal couldn't read the skill file, so it won't undo here.",
            ),
            command,
        )
    })?;
    if edit::content_hash(&current) != applied.after_hash {
        return Err(refuse(crate::tr!(
            "옮긴 뒤에 스킬 파일이 바뀌어 앱에서 되돌리지 않아요. 그 변경을 버려도 되면 직접 되돌리세요: {command}",
            "The skill file changed after the move, so kcal won't undo here. If you can drop that edit, undo it yourself: {command}"
        )));
    }
    if std::fs::symlink_metadata(&file).is_ok() {
        return Err(refuse_with(
            pick(
                "규칙 자리에 파일이 다시 생겨 앱에서 되돌리지 않아요.",
                "A file is back where the rule was, so kcal won't undo here.",
            ),
            command,
        ));
    }
    let original =
        std::fs::read(&backup).map_err(|e| io_refusal(pick("백업 읽기", "read the backup"), &e))?;
    std::fs::rename(&backup, &file)
        .map_err(|e| io_refusal(pick("규칙 되살리기", "restore the rule"), &e))?;
    std::fs::remove_file(&skill_md)
        .map_err(|e| io_refusal(pick("스킬 파일 지우기", "delete the skill file"), &e))?;
    if let Some(dir) = skill_md.parent() {
        // 비었을 때만 지워진다(사용자가 더한 파일이 있으면 그대로 둔다).
        let _ = std::fs::remove_dir(dir);
    }
    Ok(AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Undo,
        enabled: !applied.enabled,
        after_hash: edit::content_hash(&original),
        undo_of: Some(applied.id.clone()),
        ..applied.clone()
    })
}

#[cfg(test)]
#[path = "rules_apply_tests.rs"]
mod tests;
