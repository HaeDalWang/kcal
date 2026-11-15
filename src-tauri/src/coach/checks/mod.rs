//! 최적화 첫 점검(docs/decisions/0017 "첫 점검"): 효과 없는 설정(D)·커밋 안내 두 벌(C)·두 번 실림(B).
//!
//! 파일과 기록만 읽어 판정하고, 카드에는 구조화된 사실만 담는다(화면 문구는 `src/lib/check-copy.ts`).
//! 고치는 파일은 `~/.claude/settings.json` 하나이고 적용·되돌리기는 0014 방식 그대로다(같은 잠금·해시·백업·기록).
//! command는 점검 ID·옵션 ID·해시만 받는다. Rust가 판정을 다시 돌려 그 목록 안의 옵션만 적용한다.
//! TS 짝은 `src/lib/ipc.ts`의 같은 이름 타입.

pub(crate) mod diff;
pub(crate) mod dismissed;
mod duplicates;
mod git;
pub(crate) mod handoff;
mod ineffective;
pub(crate) mod long_instructions;
pub(crate) mod models;
pub(crate) mod rules_apply;
pub(crate) mod rules_handoff;
pub(crate) mod rules_paths;

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;

use super::edit::{self, JsonChange, JsonOp};
use super::log::{AppliedSwitch, CoachAction};
use super::{
    WRITE_LOCK, append_applied, backup_and_replace, copy_command, home, log_file, read_text,
    read_unchanged, record_id, refuse, switches, validate_hash,
};
use crate::cache::Cache;
use crate::context::{self, fsutil, paths, shape};
use crate::error::AppError;
use crate::i18n::pick;
use crate::metrics::DAY_SECS;
use crate::settings::SettingsState;
use crate::tuning::Tuning;
use crate::usage::Agent;
use models::{Effort, ModelCount, RecentRecords};

/// 처방 유형(0017 표). 화면 문구·묶음을 고르는 데 쓴다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CheckKind {
    A,
    D,
    C,
    B,
}

/// 최근 모델 하나의 판정. `is_ignored`: 이 카드의 설정을 이 모델이 무시한다.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelVerdict {
    pub model: String,
    pub requests: u64,
    /// 사용자 설정 `modelSettings.<model>.effortLevel`(effort 카드만).
    pub own_effort: Option<Effort>,
    /// 모델 기본 effort. 모르면 `None`(effort 카드만).
    pub default_effort: Option<Effort>,
    pub is_ignored: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMatch {
    pub file: String,
    /// 1부터.
    pub line: usize,
    /// 줄 글(앞 120자).
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsScope {
    User,
    Project,
    Local,
}

/// 카드 종류별 사실. `models`는 최근 `days`일 판정할 수 있는 모델(요청 많은 순), `ignored_by`·`moved_to`·`affected`는 그 안의 id.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CheckFacts {
    EffortLevel {
        value: Effort,
        days: u64,
        models: Vec<ModelVerdict>,
        ignored_by: Vec<String>,
        /// 지우면 값이 풀리는 모델. `remove`가 그 모델 자리에 같은 값을 적는다.
        moved_to: Vec<String>,
        /// 지금 모델(기간 안 요청이 가장 많은 모델).
        current: String,
        unknown_models: Vec<ModelCount>,
    },
    AlwaysThinking {
        days: u64,
        models: Vec<ModelVerdict>,
        ignored_by: Vec<String>,
        /// 이 설정이 실제로 먹는 모델(지우면 달라진다).
        affected: Vec<String>,
        unknown_models: Vec<ModelCount>,
    },
    MaxThinkingTokens {
        value: u64,
        days: u64,
        models: Vec<ModelVerdict>,
        ignored_by: Vec<String>,
        affected: Vec<String>,
        unknown_models: Vec<ModelCount>,
    },
    GitInstructions {
        matches: Vec<LineMatch>,
        /// 대상 글에 실제 서명 줄(`Co-Authored-By: 이름 <메일>`)이 있다.
        has_own_trailer: bool,
        /// 커밋 전에 돌리라는 내장 안내가 붙는 개인 스킬(`verify`·`simplify`).
        pre_commit_skills: Vec<String>,
    },
    AgentsMdHook {
        settings_file: String,
        scope: SettingsScope,
        /// hook command 앞 120자.
        command: String,
        agents_md: String,
        workspace: String,
    },
    SameSkill {
        name: String,
        personal_path: String,
        plugin_id: String,
        plugin_path: String,
        is_same_content: bool,
    },
    /// 0018 작업 A: 시작 때 실리는 Claude 안내문 하나가 `limit`줄을 넘는다. 파일 경로는 카드 `file`(실제 경로).
    LongInstructions {
        /// 파일 이름(`CLAUDE.md`·`CLAUDE.local.md`).
        name: String,
        lines: usize,
        limit: usize,
        /// 그 파일의 `@` 가져오기. 고쳐 쓴 글에서 빠지면 미리보기가 경고한다.
        imports: Vec<String>,
    },
    /// 0018 작업 B: 한 범위의 rules 폴더에 시작 때 늘 실리는(`paths` 없는) 규칙이 있다. 폴더 실제 경로는 카드 `file`.
    RulesPaths {
        scope: rules_paths::RulesScope,
        /// `.claude/rules`를 품은 폴더(사용자 범위는 홈, 프로젝트는 작업 폴더).
        folder: String,
        /// 규칙 본문 글자 합.
        chars: usize,
        /// 필요할 때만 싣게 바꿀 때 파일당 글자 한도(0017 행 A).
        limit: usize,
        /// 이름순.
        rules: Vec<rules_paths::RuleFact>,
    },
}

/// 고를 것 하나. `keep`과 `is_writable == false`는 적용할 수 없다.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOption {
    pub id: String,
    pub is_recommended: bool,
    pub is_writable: bool,
    /// kcal이 쓰지 않을 때 사용자가 할 일.
    pub instruction: Option<String>,
    /// 지금 파일 기준으로 바뀔 것(자세히용). 쓸 수 없으면 비어 있다.
    pub changes: Vec<JsonChange>,
    /// 적용 기록에 남길 짧은 이름.
    #[serde(skip)]
    pub log_name: String,
    #[serde(skip)]
    pub ops: Vec<JsonOp>,
}

impl CheckOption {
    fn write(id: &str, log_name: &str, is_recommended: bool, ops: Vec<JsonOp>) -> Self {
        Self {
            id: id.into(),
            is_recommended,
            is_writable: true,
            instruction: None,
            changes: Vec::new(),
            log_name: log_name.into(),
            ops,
        }
    }

    fn keep(is_recommended: bool) -> Self {
        Self {
            id: KEEP.into(),
            is_writable: false,
            ..Self::write(KEEP, "", is_recommended, Vec::new())
        }
    }
}

pub const KEEP: &str = "keep";

/// 점검 카드 하나. TS 짝은 `ipc.ts` `CoachCheck`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachCheck {
    /// `D-claude-effort-level`, `B-claude-same-skill:<name>` 등.
    pub id: String,
    pub kind: CheckKind,
    pub agent: Agent,
    pub facts: CheckFacts,
    pub options: Vec<CheckOption>,
    /// 카드가 말하는 설정 파일(B project hook이면 그 프로젝트 파일).
    pub file: String,
    /// 근거 공식 문서.
    pub docs: Vec<String>,
    /// 세션마다 아끼는 토큰(B만, 추정). 모르면 `None`.
    pub tokens: Option<u64>,
    /// "그대로 두기"로 숨겼고 지금도 추천이 `keep`이다(`dismissed::mark`). 화면은 숨긴다.
    pub is_dismissed: bool,
}

/// 미리보기. `before_hash`를 `apply_check`에 그대로 보낸다.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckPreview {
    pub id: String,
    pub option: String,
    pub file: String,
    pub changes: Vec<JsonChange>,
    pub before_hash: String,
}

/// 판정 함수들이 같이 읽는 입력.
pub(super) struct Inputs<'a> {
    pub home: &'a Path,
    pub file: PathBuf,
    /// 사용자 설정(읽지 못하면 `Null`).
    pub settings: Value,
    pub records: &'a RecentRecords,
    pub tuning: &'a Tuning,
}

impl Inputs<'_> {
    /// 사용자 설정 `env.<name>`(문자열·숫자).
    pub fn env(&self, name: &str) -> Option<String> {
        match self.settings.get("env")?.get(name)? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    pub fn is_env_on(&self, name: &str) -> bool {
        self.env(name).is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
    }

    pub fn object(&self, key: &str) -> Map<String, Value> {
        self.settings
            .get(key)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }
}

pub(super) fn user_settings_file(home: &Path) -> PathBuf {
    paths::claude_root(home).join("settings.json")
}

const MAX_ID_LEN: usize = 512;

fn validate(id: &str, option: &str) -> Result<(), AppError> {
    if id.is_empty() || id.len() > MAX_ID_LEN || option.is_empty() || option.len() > MAX_ID_LEN {
        return Err(refuse(pick(
            "점검 이름이 올바르지 않습니다.",
            "That check name isn't valid.",
        )));
    }
    Ok(())
}

/// 전체 점검 목록. 쓸 수 있는 옵션에는 지금 파일 기준 바뀔 것을 채운다.
pub fn list(home: &Path, records: &RecentRecords, tuning: &Tuning) -> Vec<CoachCheck> {
    let file = user_settings_file(home);
    let input = Inputs {
        home,
        settings: fsutil::read_json(&file).unwrap_or(Value::Null),
        file: file.clone(),
        records,
        tuning,
    };
    let mut checks: Vec<CoachCheck> = [
        ineffective::effort_level(&input),
        ineffective::always_thinking(&input),
        ineffective::max_thinking_tokens(&input),
        git::git_instructions(&input),
    ]
    .into_iter()
    .flatten()
    .chain(duplicates::agents_md_hooks(&input))
    .chain(duplicates::same_skills(&input))
    .chain(long_instructions::long_instructions(&input))
    .chain(rules_paths::rules_paths(&input))
    .collect();
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    for option in checks.iter_mut().flat_map(|c| c.options.iter_mut()) {
        if option.is_writable {
            option.changes = edit::apply_json_ops(&text, &option.ops)
                .map(|e| e.changes)
                .unwrap_or_default();
        }
    }
    checks
}

/// 목록 안의 점검·옵션만 찾는다. 적용할 수 없는 옵션(`keep`, 안내만)은 이유와 함께 거부한다.
fn find(
    checks: Vec<CoachCheck>,
    id: &str,
    option: &str,
) -> Result<(CoachCheck, CheckOption), AppError> {
    let check = checks.into_iter().find(|c| c.id == id).ok_or_else(|| {
        refuse(pick(
            "지금 점검 목록에 없는 항목입니다. 화면을 다시 여세요.",
            "This item isn't in the current check list. Reopen the screen.",
        ))
    })?;
    let picked = check
        .options
        .iter()
        .find(|o| o.id == option)
        .cloned()
        .ok_or_else(|| {
            refuse(pick(
                "이 점검에 없는 선택입니다.",
                "That option isn't part of this check.",
            ))
        })?;
    if picked.id == KEEP {
        return Err(refuse(pick(
            "그대로 두기는 바꿀 것이 없습니다.",
            "Keeping it as is has nothing to change.",
        )));
    }
    if !picked.is_writable {
        return Err(refuse(picked.instruction.clone().unwrap_or_else(|| {
            pick(
                "kcal이 고치지 않는 설정입니다.",
                "kcal doesn't edit this setting.",
            )
            .into()
        })));
    }
    Ok((check, picked))
}

/// 쓰는 파일은 0014 writable 목록의 Claude 설정 하나뿐이다.
fn writable_file(home: &Path) -> Result<PathBuf, AppError> {
    let file = user_settings_file(home);
    if !switches::writable_files(home).contains(&file) {
        return Err(AppError::Internal);
    }
    Ok(file)
}

pub fn preview(
    home: &Path,
    records: &RecentRecords,
    tuning: &Tuning,
    id: &str,
    option: &str,
) -> Result<CheckPreview, AppError> {
    validate(id, option)?;
    let (_, picked) = find(list(home, records, tuning), id, option)?;
    let file = writable_file(home)?;
    let (bytes, text) = read_text(&file)?;
    let edit = edit::apply_json_ops(&text, &picked.ops).map_err(refuse)?;
    Ok(CheckPreview {
        id: id.into(),
        option: option.into(),
        file: file.to_string_lossy().into_owned(),
        changes: edit.changes,
        before_hash: edit::content_hash(&bytes),
    })
}

/// 점검 적용 기록 `target`의 앞부분. TS 짝은 `ipc.ts` `CHECK_TARGET_PREFIX`(화면이 이름 뒤 "켬/끔"을 붙이지 않는다).
pub const CHECK_TARGET_PREFIX: &str = "claude/check:";

/// 적용 기록 `target`(`claude/check:<id>:<option>`).
pub fn target(id: &str, option: &str) -> String {
    format!("{CHECK_TARGET_PREFIX}{id}:{option}")
}

#[allow(clippy::too_many_arguments)]
pub fn apply(
    home: &Path,
    records: &RecentRecords,
    tuning: &Tuning,
    log_file: &Path,
    id: &str,
    option: &str,
    before_hash: &str,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    validate(id, option)?;
    validate_hash(before_hash)?;
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Internal)?;
    let (check, picked) = find(list(home, records, tuning), id, option)?;
    let file = writable_file(home)?;
    let text = read_unchanged(&file, before_hash)?;
    let edit = edit::apply_json_ops(&text, &picked.ops).map_err(refuse)?;
    if edit.changes.is_empty() {
        return Err(refuse(pick(
            "이미 그 값이라 바꿀 것이 없습니다.",
            "It already has that value, so there's nothing to change.",
        )));
    }
    let backup = backup_and_replace(&file, &edit.content, now)?;
    let entry = AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Apply,
        target: target(id, option),
        agent: Agent::Claude,
        name: picked.log_name,
        file: file.to_string_lossy().into_owned(),
        backup: backup.to_string_lossy().into_owned(),
        enabled: false,
        tokens: check.tokens,
        after_hash: edit::content_hash(edit.content.as_bytes()),
        undo_command: copy_command(&backup, &file),
        undo_of: None,
        created: None,
        files: Vec::new(),
    };
    append_applied(log_file, &entry)?;
    Ok(entry)
}

// ---------- 최근 기록(캐시) ----------

/// 최근 `days`일 Claude 기록의 모델 수와 최근 세션 작업 폴더. 이 Mac 30일치는 수백 MB라 캐시한다.
pub fn read_records(home: &Path, tuning: &Tuning, now: f64) -> RecentRecords {
    let days = tuning.check_model_days;
    let since = now - days as f64 * DAY_SECS;
    let (requests, workspaces) = crate::metrics::claude::model_requests(home, since);
    RecentRecords {
        workspace: shape::latest(home, None, tuning.shape_sessions_to_search)
            .and_then(|s| s.cwd)
            .map(PathBuf::from)
            .filter(|p| p.is_dir()),
        workspaces: workspaces.into_iter().filter(|p| p.is_dir()).collect(),
        ..RecentRecords::from_requests(days, requests)
    }
}

/// `read_records`의 마지막 결과. TTL은 tuning `metricsTtlSecs`(같은 기록 파일을 읽는 트레이 지표와 같다).
pub struct ChecksState(Cache<RecentRecords>);

impl Default for ChecksState {
    fn default() -> Self {
        Self(Cache::new())
    }
}

fn now_secs() -> f64 {
    crate::metrics::unix_secs(OffsetDateTime::now_utc())
}

async fn records(app: &AppHandle, home: &Path, tuning: Tuning) -> Result<RecentRecords, AppError> {
    let cache = app.state::<ChecksState>().0.clone();
    let owned = home.to_path_buf();
    let cached = cache
        .get(
            tuning.metrics_ttl(),
            move || read_records(&owned, &tuning, now_secs()),
            |_| {},
        )
        .await?;
    if cached.days == tuning.check_model_days {
        return Ok(cached);
    }
    // 기간 설정을 바꾼 직후: 캐시는 뒤에서 새 기간으로 다시 계산 중이다. 이번 응답은 바로 새 기간으로 읽는다.
    let owned = home.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || read_records(&owned, &tuning, now_secs()))
        .await
        .map_err(|_| AppError::Internal)
}

/// 기록을 읽고 blocking 스레드에서 `work`를 돌린다.
async fn with_records<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(PathBuf, RecentRecords, Tuning) -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    let home = home()?;
    let tuning = app.state::<SettingsState>().tuning();
    let records = records(app, &home, tuning).await?;
    tauri::async_runtime::spawn_blocking(move || work(home, records, tuning))
        .await
        .map_err(|_| AppError::Internal)?
}

/// 앱 시작 때 한 번, 뒤에서 최근 기록을 읽어 캐시를 데운다(타이머 아님). 팝오버 "점검" 알림 개수가
/// 처음 열 때부터 바로 나오게 한다(docs/decisions/0017 "화면 자리").
pub fn prewarm(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let result = match home() {
            Ok(home) => {
                let tuning = app.state::<SettingsState>().tuning();
                records(&app, &home, tuning).await.map(|_| ())
            }
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            eprintln!("kcal: 점검 기록 미리 읽기 실패({error:?})");
        }
    });
}

fn dismissed_file(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map(|dir| dismissed::file(&dir))
        .map_err(|_| AppError::Internal)
}

#[tauri::command]
pub async fn get_checks(app: AppHandle) -> Result<Vec<CoachCheck>, AppError> {
    let file = dismissed_file(&app)?;
    with_records(&app, move |home, records, tuning| {
        Ok(dismissed::mark(
            list(&home, &records, &tuning),
            &dismissed::read(&file),
        ))
    })
    .await
}

/// "그대로 두기": 추천이 `keep`인 카드를 숨긴다. 화면은 뒤에 `get_checks`를 다시 읽는다.
#[tauri::command]
pub async fn dismiss_check(app: AppHandle, id: String) -> Result<(), AppError> {
    let file = dismissed_file(&app)?;
    with_records(&app, move |home, records, tuning| {
        dismissed::dismiss(&home, &records, &tuning, &file, &id)
    })
    .await
}

/// 숨긴 카드를 모두 다시 보인다.
#[tauri::command]
pub async fn restore_dismissed_checks(app: AppHandle) -> Result<(), AppError> {
    let file = dismissed_file(&app)?;
    tauri::async_runtime::spawn_blocking(move || dismissed::restore(&file))
        .await
        .map_err(|_| AppError::Internal)?
}

#[tauri::command]
pub async fn preview_check(
    app: AppHandle,
    id: String,
    option: String,
) -> Result<CheckPreview, AppError> {
    with_records(&app, move |home, records, tuning| {
        preview(&home, &records, &tuning, &id, &option)
    })
    .await
}

#[tauri::command]
pub async fn apply_check(
    app: AppHandle,
    id: String,
    option: String,
    before_hash: String,
) -> Result<AppliedSwitch, AppError> {
    let log = log_file(&app)?;
    let result = with_records(&app, move |home, records, tuning| {
        let now = OffsetDateTime::now_utc();
        apply(
            &home,
            &records,
            &tuning,
            &log,
            &id,
            &option,
            &before_hash,
            now,
        )
    })
    .await?;
    context::prewarm(app, true);
    Ok(result)
}

#[cfg(test)]
mod tests;
