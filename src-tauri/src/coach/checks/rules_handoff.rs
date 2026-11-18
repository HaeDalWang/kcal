//! 0018 작업 B: 늘 실리는 규칙을 Claude에게 맡기기. 실행 경계(한 번에 하나, 한도 문, CLI 찾기, Opus 없음, 시간 초과,
//! stdin 프롬프트, 빈 작업 폴더, 오류 종류)는 작업 A의 `handoff::execute`·`classify`를 그대로 쓴다.
//!
//! Claude는 규칙마다 `paths` / 스킬로 옮기기 / 그대로 두기 중 하나를 고른다. kcal은 그 답을 0018 "조율자 기본값"
//! 검사로 거른 뒤 (카드 id) 열쇠로 규칙마다 원본 해시와 함께 하나만 보관한다. 적용은 이 보관본만 쓴다(`rules_apply.rs`).
//! TS 짝은 `src/lib/ipc.ts`의 같은 이름 타입(`RulesPreview`·`RuleProposal`·`Proposal`·`RuleProblem`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;

use super::handoff::{
    HandoffError, HandoffState, RunSpec, answer_language, check_limit, claude_remaining, clip,
    execute, find_cli, handoff_dir,
};
use super::models::RecentRecords;
use super::rules_paths::{ID_PREFIX, MAX_CHARS, RuleFact, RulesScope};
use super::{CheckFacts, MAX_ID_LEN, list};
use crate::coach::log::AppliedSwitch;
use crate::coach::{edit, read_text, refuse};
use crate::error::AppError;
use crate::i18n::pick;
use crate::tuning::Tuning;

/// 규칙마다 모든 칸이 필수다(안 쓰는 칸은 빈 값). `choice`는 세 값 중 하나.
pub const SCHEMA: &str = r#"{"type":"object","properties":{"rules":{"type":"array","items":{"type":"object","properties":{"key":{"type":"string"},"choice":{"type":"string","enum":["paths","skill","keep"]},"paths":{"type":"array","items":{"type":"string"}},"name":{"type":"string"},"description":{"type":"string"},"reason":{"type":"string"}},"required":["key","choice","paths","name","description","reason"],"additionalProperties":false}}},"required":["rules"],"additionalProperties":false}"#;
const MAX_PATHS: usize = 20;
const MAX_PATH_CHARS: usize = 200;
const MAX_NAME_CHARS: usize = 64;
/// 문서 `skills#frontmatter-reference`: 목록에서 `description`+`when_to_use`가 1,536자에서 잘린다.
const MAX_DESCRIPTION_CHARS: usize = 1_536;
const MAX_REASON_CHARS: usize = 300;
/// `r1`…: kcal이 규칙에 붙이는 짧은 열쇠의 최대 길이.
const MAX_KEY_LEN: usize = 8;

/// Claude가 `structured_output`에 담아 돌려주는 것.
#[derive(Clone, Debug, Deserialize)]
pub struct Answer {
    pub rules: Vec<RawRule>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RawRule {
    pub key: String,
    /// 스키마가 세 값으로 묶지만, 한 규칙이 어긋나도 나머지를 쓰려고 문자열로 받는다.
    pub choice: String,
    pub paths: Vec<String>,
    pub name: String,
    pub description: String,
    pub reason: String,
}

/// 검사에 떨어진 이유. 문구는 `check-copy.ts` `ruleProblemText`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleProblem {
    /// Claude가 이 규칙에 답하지 않았다.
    Missing,
    /// 같은 규칙에 답이 둘 이상이다.
    Duplicate,
    Choice,
    SkillName,
    Description,
    PathsCount,
    PathText,
    PathAbsolute,
}

/// 규칙 하나의 제안. `Unusable`은 적용 버튼이 없다.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "choice",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Proposal {
    Paths { paths: Vec<String> },
    Skill { name: String, description: String },
    Keep,
    Unusable { problem: RuleProblem },
}

/// 결과 한 줄. `beforeHash`를 `apply_claude_rule`에 그대로 보낸다.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleProposal {
    pub key: String,
    pub name: String,
    pub chars: usize,
    pub has_front_matter: bool,
    pub referenced_by: usize,
    pub before_hash: String,
    pub proposal: Proposal,
    /// Claude가 고른 이유(한국어 한 문장, 300자까지). 답이 없으면 빈 값.
    pub reason: String,
    /// 스킬로 옮길 자리(`skills/<이름>`)가 이미 있다.
    pub skill_exists: bool,
    /// `paths`·스킬 제안이고 지금 바꿀 수 있다(글자 한도·머리말·이름 겹침).
    pub can_apply: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesPreview {
    pub id: String,
    pub scope: RulesScope,
    pub folder: String,
    /// 지금 카드에 있는 규칙만(이미 바꾼 규칙은 빠진다), 이름순.
    pub rules: Vec<RuleProposal>,
    /// Claude가 돌려준 답 중 kcal이 주지 않은 열쇠의 수.
    pub unknown_keys: usize,
}

/// 맡길 카드의 규칙 하나(지금 읽은 파일).
#[derive(Clone, Debug)]
pub struct TargetRule {
    pub key: String,
    pub fact: RuleFact,
    pub original: String,
    pub before_hash: String,
}

/// 맡길 카드 하나(지금 점검 목록 안). `root`는 범위 rules 폴더 실제 경로.
#[derive(Clone, Debug)]
pub struct RulesTarget {
    pub id: String,
    pub scope: RulesScope,
    pub folder: String,
    pub root: PathBuf,
    pub rules: Vec<TargetRule>,
}

/// 보관본의 규칙 하나. 열쇠는 맡긴 때 붙인 것이다(적용 뒤 카드가 줄어도 바뀌지 않는다).
#[derive(Clone, Debug)]
pub struct StoredRule {
    pub key: String,
    pub file: PathBuf,
    pub before_hash: String,
    pub proposal: Proposal,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct RulesStored {
    pub id: String,
    pub rules: Vec<StoredRule>,
    pub unknown_keys: usize,
}

/// 지금 목록에 있는 규칙 카드만 찾고, 규칙 파일을 지금 읽는다. 열쇠는 이름순 `r1`….
pub fn find_target(
    home: &Path,
    records: &RecentRecords,
    tuning: &Tuning,
    id: &str,
) -> Result<RulesTarget, AppError> {
    if !id.starts_with(ID_PREFIX) || id.len() > MAX_ID_LEN {
        return Err(refuse(pick(
            "점검 이름이 올바르지 않아요.",
            "That check name isn't valid.",
        )));
    }
    let check = list(home, records, tuning)
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| {
            refuse(pick(
                "지금 점검 목록에 없는 항목이에요. 화면을 다시 여세요.",
                "This item isn't in the current check list. Reopen the screen.",
            ))
        })?;
    let CheckFacts::RulesPaths {
        scope,
        folder,
        rules,
        ..
    } = check.facts
    else {
        return Err(AppError::Internal);
    };
    let rules = rules
        .into_iter()
        .enumerate()
        .map(|(i, fact)| {
            let (bytes, original) = read_text(&fact.file)?;
            Ok(TargetRule {
                key: format!("r{}", i + 1),
                fact,
                original,
                before_hash: edit::content_hash(&bytes),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(RulesTarget {
        id: check.id,
        scope,
        folder,
        root: PathBuf::from(check.file),
        rules,
    })
}

/// 프롬프트. 상대 이름과 본문만 넣는다(절대 경로·홈 이름 없음). 번역 금지·설명 언어·이유 언어는 반드시 넣는다.
pub fn prompt(target: &RulesTarget) -> String {
    let scope = match target.scope {
        RulesScope::User => "사용자 rules(모든 프로젝트에 실린다)",
        RulesScope::Project => "프로젝트 rules(이 프로젝트에만 실린다)",
    };
    let rules: String = target
        .rules
        .iter()
        .map(|r| {
            format!(
                "<rule key=\"{}\" name=\"{}\">\n{}\n</rule>\n",
                r.key, r.fact.name, r.original
            )
        })
        .collect();
    format!(
        "아래 <rule> 안의 글은 Claude Code가 세션마다 늘 싣는 규칙 파일 {count}개다. 범위: {scope}.\n\
         규칙마다 어디에 둘지 하나를 골라라.\n\
         - 특정 파일을 다룰 때만 필요한 규칙이면 choice를 \"paths\"로 하고, paths에 그 파일을 고르는 glob 목록(프로젝트 폴더 기준 상대 경로, 예: \"src/**/*.ts\")을 담아라. `/`나 `~`로 시작하지 마라.\n\
         - 특정 작업을 할 때만 필요한 규칙이면 choice를 \"skill\"로 하고, name에 스킬 이름(영문 소문자·숫자·-, 64자 이하, -로 시작하거나 끝나지 않음)을, description에 이 스킬을 언제 쓰는지 한 줄로 담아라.\n\
         - 늘 필요한 규칙이면 choice를 \"keep\"으로 해라.\n\
         - 규칙 글은 고쳐 쓰지 마라. 원래 언어와 말투를 그대로 둬라. 번역하지 마라.\n\
         - description은 그 규칙 본문과 같은 언어로 써라.\n\
         - reason에는 그렇게 고른 이유를 {answer} 한 문장으로 써라.\n\
         - rules에 규칙마다 하나씩 담고, key는 받은 그대로 써라. 쓰지 않는 칸은 빈 문자열이나 빈 목록으로 둬라.\n\
         {rules}",
        count = target.rules.len(),
        answer = answer_language(),
    )
}

fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

/// 0018 "조율자 기본값" 스킬 이름: 영문 소문자·숫자·`-`, 1~64자, `-`로 시작·끝나지 않음(폴더 이름이 된다).
pub fn is_skill_name(name: &str) -> bool {
    let is_plain = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    is_plain
        && (1..=MAX_NAME_CHARS).contains(&name.len())
        && !name.starts_with('-')
        && !name.ends_with('-')
}

fn check_paths(paths: &[String]) -> Result<Vec<String>, RuleProblem> {
    if !(1..=MAX_PATHS).contains(&paths.len()) {
        return Err(RuleProblem::PathsCount);
    }
    for path in paths {
        let chars = path.chars().count();
        if !(1..=MAX_PATH_CHARS).contains(&chars) || has_control(path) || path.trim().is_empty() {
            return Err(RuleProblem::PathText);
        }
        if path.starts_with(['/', '~', '\\']) {
            return Err(RuleProblem::PathAbsolute);
        }
    }
    Ok(paths.to_vec())
}

fn check_skill(raw: &RawRule) -> Result<Proposal, RuleProblem> {
    if !is_skill_name(&raw.name) {
        return Err(RuleProblem::SkillName);
    }
    let description = raw.description.trim();
    let chars = description.chars().count();
    if !(1..=MAX_DESCRIPTION_CHARS).contains(&chars) || has_control(description) {
        return Err(RuleProblem::Description);
    }
    Ok(Proposal::Skill {
        name: raw.name.clone(),
        description: description.to_owned(),
    })
}

/// 답 하나를 검사한다. 떨어지면 `Unusable`.
pub fn check_answer(raw: &RawRule) -> Proposal {
    let checked = match raw.choice.as_str() {
        "paths" => check_paths(&raw.paths).map(|paths| Proposal::Paths { paths }),
        "skill" => check_skill(raw),
        "keep" => Ok(Proposal::Keep),
        _ => Err(RuleProblem::Choice),
    };
    checked.unwrap_or_else(|problem| Proposal::Unusable { problem })
}

/// 이유는 첫 줄만, 300자까지.
fn reason_of(raw: &RawRule) -> String {
    let first = raw.reason.trim().lines().next().unwrap_or_default();
    clip(first.trim(), MAX_REASON_CHARS)
}

/// Claude 답을 규칙마다 판정한다. 빠진 열쇠·두 번 나온 열쇠는 `Unusable`, 모르는 열쇠는 센다.
pub fn judge(target: &RulesTarget, answer: &Answer) -> RulesStored {
    let mut by_key: BTreeMap<&str, Vec<&RawRule>> = BTreeMap::new();
    for raw in &answer.rules {
        by_key.entry(raw.key.as_str()).or_default().push(raw);
    }
    let unknown_keys = answer
        .rules
        .iter()
        .filter(|raw| !target.rules.iter().any(|r| r.key == raw.key))
        .count();
    let rules = target
        .rules
        .iter()
        .map(|rule| {
            let answers = by_key.get(rule.key.as_str()).map_or(&[][..], Vec::as_slice);
            let (proposal, reason) = match answers {
                [] => (
                    Proposal::Unusable {
                        problem: RuleProblem::Missing,
                    },
                    String::new(),
                ),
                [raw] => (check_answer(raw), reason_of(raw)),
                _ => (
                    Proposal::Unusable {
                        problem: RuleProblem::Duplicate,
                    },
                    String::new(),
                ),
            };
            StoredRule {
                key: rule.key.clone(),
                file: rule.fact.file.clone(),
                before_hash: rule.before_hash.clone(),
                proposal,
                reason,
            }
        })
        .collect();
    RulesStored {
        id: target.id.clone(),
        rules,
        unknown_keys,
    }
}

/// 스킬이 갈 폴더: 범위의 `skills/<name>`(`~/.claude/rules` → `~/.claude/skills`).
pub fn skill_dir(root: &Path, name: &str) -> Option<PathBuf> {
    Some(root.parent()?.join("skills").join(name))
}

fn row(target: &RulesTarget, rule: &TargetRule, stored: &StoredRule) -> RuleProposal {
    let fact = &rule.fact;
    let skill_exists = match &stored.proposal {
        Proposal::Skill { name, .. } => {
            skill_dir(&target.root, name).is_none_or(|dir| std::fs::symlink_metadata(dir).is_ok())
        }
        _ => false,
    };
    let can_apply = match &stored.proposal {
        Proposal::Paths { .. } => fact.chars <= MAX_CHARS,
        Proposal::Skill { .. } => !fact.has_front_matter && !skill_exists,
        Proposal::Keep | Proposal::Unusable { .. } => false,
    };
    RuleProposal {
        key: stored.key.clone(),
        name: fact.name.clone(),
        chars: fact.chars,
        has_front_matter: fact.has_front_matter,
        referenced_by: fact.referenced_by,
        before_hash: stored.before_hash.clone(),
        proposal: stored.proposal.clone(),
        reason: stored.reason.clone(),
        skill_exists,
        can_apply,
    }
}

/// 지금 카드 규칙만, 보관본의 제안으로 보인다.
pub fn preview_of(target: &RulesTarget, stored: &RulesStored) -> RulesPreview {
    let rules = target
        .rules
        .iter()
        .filter_map(|rule| {
            let found = stored.rules.iter().find(|s| s.file == rule.fact.file)?;
            Some(row(target, rule, found))
        })
        .collect();
    RulesPreview {
        id: target.id.clone(),
        scope: target.scope,
        folder: target.folder.clone(),
        rules,
        unknown_keys: stored.unknown_keys,
    }
}

/// 같은 카드이고 지금 카드의 규칙이 모두 보관본에 같은 해시로 있으면 그 보관본(다시 실행하지 않는다).
pub fn reusable(
    state: &HandoffState,
    target: &RulesTarget,
) -> Result<Option<RulesStored>, AppError> {
    let guard = state.rules.lock().map_err(|_| AppError::Internal)?;
    Ok(guard
        .as_ref()
        .filter(|s| {
            s.id == target.id
                && target.rules.iter().all(|t| {
                    s.rules
                        .iter()
                        .any(|r| r.file == t.fact.file && r.before_hash == t.before_hash)
                })
        })
        .cloned())
}

/// 이 카드의 보관본(적용용).
pub fn stored(state: &HandoffState, id: &str) -> Result<Option<RulesStored>, AppError> {
    let guard = state.rules.lock().map_err(|_| AppError::Internal)?;
    Ok(guard.as_ref().filter(|s| s.id == id).cloned())
}

fn keep(state: &HandoffState, stored: RulesStored) -> Result<(), AppError> {
    *state.rules.lock().map_err(|_| AppError::Internal)? = Some(stored);
    Ok(())
}

/// 맡기기 한 번. 순서는 작업 A와 같다: 보관본 → 돌고 있으면 거절 → 남은 한도 문 → CLI 찾기 → 실행 → 판정·보관.
pub async fn ask(
    state: &HandoffState,
    target: RulesTarget,
    remaining: Option<f64>,
    min_remaining: u64,
    spec: RunSpec,
) -> Result<RulesPreview, HandoffError> {
    if let Some(stored) = reusable(state, &target)? {
        return Ok(preview_of(&target, &stored));
    }
    let Some(_running) = state.start() else {
        return Err(HandoffError::Busy);
    };
    check_limit(remaining, min_remaining)?;
    let cli = spec.cli.clone().ok_or(HandoffError::CliMissing)?;
    let answer: Answer = execute(&spec, &cli, &prompt(&target), SCHEMA).await?;
    let stored = judge(&target, &answer);
    keep(state, stored.clone())?;
    Ok(preview_of(&target, &stored))
}

// ---------- commands ----------

/// "Claude에게 맡기기"(규칙 카드). 한도 검사는 화면만 믿지 않고 여기서 실행 직전에 다시 한다.
#[tauri::command]
pub async fn ask_claude_rules(app: AppHandle, id: String) -> Result<RulesPreview, HandoffError> {
    let dir = handoff_dir(&app)?;
    let (target, home, tuning) = super::with_records(&app, move |home, records, tuning| {
        find_target(&home, &records, &tuning, &id).map(|t| (t, home, tuning))
    })
    .await?;
    let path_env = std::env::var_os("PATH");
    let spec = RunSpec {
        cli: find_cli(&home, path_env.as_deref()),
        dir,
        timeout: tuning.claude_handoff_timeout(),
    };
    let remaining = claude_remaining(&app)?;
    let state = app.state::<HandoffState>();
    ask(
        &state,
        target,
        remaining,
        tuning.claude_handoff_min_remaining,
        spec,
    )
    .await
}

/// 보관본의 규칙 하나를 적용한다(`paths` 붙이기 또는 스킬로 옮기기). 기록은 `AppliedSwitch`라 `undo_switch`로 되돌린다.
#[tauri::command]
pub async fn apply_claude_rule(
    app: AppHandle,
    id: String,
    key: String,
    before_hash: String,
) -> Result<AppliedSwitch, AppError> {
    let log = crate::coach::log_file(&app)?;
    let handle = app.clone();
    let result = super::with_records(&app, move |home, records, tuning| {
        let state = handle.state::<HandoffState>();
        let request = super::rules_apply::Request {
            id: &id,
            key: &key,
            before_hash: &before_hash,
        };
        let now = OffsetDateTime::now_utc();
        super::rules_apply::apply(&state, &home, &records, &tuning, &log, request, now)
    })
    .await?;
    crate::context::prewarm(app, true);
    Ok(result)
}

/// 키 모양: `r` + 숫자.
pub fn is_key(key: &str) -> bool {
    key.len() <= MAX_KEY_LEN
        && key
            .strip_prefix('r')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
#[path = "rules_tests.rs"]
pub(super) mod tests;
