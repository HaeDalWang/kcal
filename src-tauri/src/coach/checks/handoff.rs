//! Claude에게 맡기기(docs/decisions/0018 "실행 경계"·"안내문 쓰기 경계"·"작업 A").
//!
//! 사용자가 카드에서 버튼을 누를 때만 Claude Code를 한 번 실행한다(`claude -p --safe-mode`, 모델·effort 고정, 도구 없음).
//! 프롬프트는 stdin 파일로 넣고 작업 폴더는 앱 데이터 폴더 아래 빈 폴더다. 한 번에 하나만 돌고, 결과는
//! (카드 id, 원본 해시) 열쇠로 하나만 보관한다. 적용은 화면이 보낸 글이 아니라 이 보관본을 0014 방식으로 쓴다.
//! TS 짝은 `src/lib/ipc.ts`의 같은 이름 타입(`HandoffPreview`·`HandoffError`·`DiffLine`).

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::process::Output;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;

use super::diff::{self, DiffLine};
use super::long_instructions::{HANDOFF, ID_PREFIX, is_claude_md_name, line_count};
use super::models::RecentRecords;
use super::{CHECK_TARGET_PREFIX, CheckFacts, MAX_ID_LEN, list, target as check_target};
use crate::coach::log::{AppliedSwitch, CoachAction};
use crate::coach::{
    WRITE_LOCK, append_applied, backup_and_replace, copy_command, edit, read_text, record_id,
    refuse, validate_hash,
};
use crate::context::instructions::imports_of;
use crate::error::AppError;
use crate::i18n::pick;
use crate::tuning::Tuning;
use crate::usage::sources::{RunError, RunInput, is_executable, run};
use crate::usage::{Agent, UsageState, paths as usage_paths};

/// 고정 모델·effort(0017 "적용 방식"). 낮은 모델로 내려 실행하지 않는다.
pub const MODEL: &str = "claude-opus-5-5";
const EFFORT: &str = "medium";
const SCHEMA: &str = r#"{"type":"object","properties":{"text":{"type":"string"},"removed":{"type":"array","items":{"type":"string"}}},"required":["text","removed"],"additionalProperties":false}"#;
/// 앱 데이터 폴더 아래 이 실행 전용 폴더.
pub const DIR_NAME: &str = "claude-handoff";
const MAX_REMOVED: usize = 50;
const MAX_REMOVED_CHARS: usize = 300;
const MAX_REASON_CHARS: usize = 40;

/// 실패 이유. 문구는 `src/lib/check-copy.ts` `handoffErrorText`가 만든다. TS 짝은 `ipc.ts` `HandoffError`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HandoffError {
    /// 이미 맡긴 일이 돌고 있다.
    Busy,
    /// Claude 남은 한도를 모른다.
    LimitUnknown,
    /// 가장 적게 남은 창의 남은 %가 기준보다 적다.
    LimitTooLow {
        remaining: f64,
        min_remaining: u64,
    },
    CliMissing,
    /// 404 또는 `unrecognized_model`(0018 "실측 사실").
    OpusUnavailable,
    Timeout {
        secs: u64,
    },
    /// 그 밖의 실패. `reason`은 짧은 이유만(stderr 전체·경로·토큰은 넣지 않는다).
    Failed {
        reason: String,
    },
    /// 입력·상태 검증 실패(`AppError::InvalidInput`). 그대로 보여 줄 수 있는 문장.
    Refused {
        message: String,
    },
    Internal,
}

impl From<AppError> for HandoffError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Internal => Self::Internal,
            AppError::InvalidInput(message) => Self::Refused { message },
        }
    }
}

fn failed(reason: impl Into<String>) -> HandoffError {
    HandoffError::Failed {
        reason: reason.into(),
    }
}

/// Claude가 `structured_output`에 담아 돌려주는 것.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Rewrite {
    pub text: String,
    pub removed: Vec<String>,
}

/// 보관본 하나. `text`는 쓸 글 그대로(원본이 줄바꿈으로 끝나면 결과도 그렇게 맞춘다).
#[derive(Clone, Debug)]
struct Stored {
    id: String,
    before_hash: String,
    original: String,
    text: String,
    removed: Vec<String>,
}

/// 실행 중 표시와 보관본. 앱을 끄면 사라진다. 실행 중 표시는 작업 A·B가 같이 쓴다(한 번에 하나).
#[derive(Default)]
pub struct HandoffState {
    is_running: AtomicBool,
    stored: Mutex<Option<Stored>>,
    /// 작업 B(`rules_handoff.rs`)의 보관본 하나.
    pub(super) rules: Mutex<Option<super::rules_handoff::RulesStored>>,
}

/// 돌고 있는 동안만 산다. 끝나면(실패·패닉 포함) 표시를 내린다.
pub(super) struct Running<'a>(&'a AtomicBool);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl HandoffState {
    pub(super) fn start(&self) -> Option<Running<'_>> {
        self.is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Running(&self.is_running))
    }

    fn stored(&self, id: &str, before_hash: &str) -> Result<Option<Stored>, AppError> {
        let guard = self.stored.lock().map_err(|_| AppError::Internal)?;
        Ok(guard
            .as_ref()
            .filter(|s| s.id == id && s.before_hash == before_hash)
            .cloned())
    }

    fn keep(&self, stored: Stored) -> Result<(), AppError> {
        *self.stored.lock().map_err(|_| AppError::Internal)? = Some(stored);
        Ok(())
    }
}

/// 미리보기. `beforeHash`를 `apply_claude_handoff`에 그대로 보낸다.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffPreview {
    pub id: String,
    /// 쓸 파일(링크면 따라간 실제 경로).
    pub file: String,
    pub before_hash: String,
    pub before_lines: usize,
    pub after_lines: usize,
    pub diff: Vec<DiffLine>,
    /// Claude가 합치거나 뺀 것(한국어 문장).
    pub removed: Vec<String>,
    /// 원본에 있던 `@` 가져오기 중 결과에 없는 것.
    pub missing_imports: Vec<String>,
    /// 결과가 비었거나 원본보다 길거나 원본과 같으면 `false`(적용 버튼을 끈다).
    pub can_apply: bool,
}

fn preview_of(file: &Path, stored: &Stored) -> HandoffPreview {
    let kept: BTreeSet<String> = imports_of(&stored.text).into_iter().collect();
    let missing: BTreeSet<String> = imports_of(&stored.original)
        .into_iter()
        .filter(|i| !kept.contains(i))
        .collect();
    let (before_lines, after_lines) = (line_count(&stored.original), line_count(&stored.text));
    HandoffPreview {
        id: stored.id.clone(),
        file: file.to_string_lossy().into_owned(),
        before_hash: stored.before_hash.clone(),
        before_lines,
        after_lines,
        diff: diff::lines(&stored.original, &stored.text),
        removed: stored.removed.clone(),
        missing_imports: missing.into_iter().collect(),
        can_apply: !stored.text.trim().is_empty()
            && after_lines <= before_lines
            && stored.text != stored.original,
    }
}

/// 맡길 카드 하나(지금 점검 목록 안). `file`은 실제 경로다.
#[derive(Clone, Debug)]
pub struct Target {
    pub id: String,
    pub file: PathBuf,
    pub name: String,
    pub original: String,
    pub before_hash: String,
}

/// 지금 목록에 있는 긴 안내문 카드만 찾고, 파일을 지금 읽는다.
pub fn find_target(
    home: &Path,
    records: &RecentRecords,
    tuning: &Tuning,
    id: &str,
) -> Result<Target, AppError> {
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
    let CheckFacts::LongInstructions { name, .. } = check.facts else {
        return Err(AppError::Internal);
    };
    let file = PathBuf::from(&check.file);
    let (bytes, original) = read_text(&file)?;
    Ok(Target {
        id: check.id,
        file,
        name,
        original,
        before_hash: edit::content_hash(&bytes),
    })
}

/// 남은 한도 문(0018 "남은 한도"). 값이 없으면 거절, 기준보다 적으면 거절.
pub fn check_limit(remaining: Option<f64>, min_remaining: u64) -> Result<(), HandoffError> {
    match remaining {
        None => Err(HandoffError::LimitUnknown),
        Some(r) if r < min_remaining as f64 => Err(HandoffError::LimitTooLow {
            remaining: r,
            min_remaining,
        }),
        Some(_) => Ok(()),
    }
}

/// 설치 위치를 먼저, `PATH`를 다음에 본다(메뉴바 앱은 셸 PATH를 물려받지 않는다, 0005).
pub fn find_cli(home: &Path, path_env: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    usage_paths::claude_cli_candidates(home, path_env)
        .into_iter()
        .find(|p| is_executable(p))
}

/// Claude가 화면에 보일 설명(합치거나 뺀 것·고른 이유)을 쓸 언어. 화면 언어를 따른다(0021).
pub(super) fn answer_language() -> &'static str {
    pick("한국어", "English")
}

/// 프롬프트. 번역 금지·`@` 줄 유지는 반드시 넣는다(0018 실측: 없으면 한국어로 번역해 돌려줬다).
pub fn prompt(target: &Target) -> String {
    let lines = line_count(&target.original);
    format!(
        "아래 <file> 안의 글은 Claude Code가 세션마다 읽는 안내문 파일 {name}이다. 지금 {lines}줄이다.\n\
         이 파일을 다음 규칙대로 고쳐 써라.\n\
         - 규칙을 하나도 빼지 말고, 같은 뜻의 규칙과 겹치는 문장을 합쳐 200줄 아래로 줄여라.\n\
         - 원래 언어와 말투를 그대로 둬라. 번역하지 마라.\n\
         - `@`로 시작하는 가져오기 줄(예: @AGENTS.md)은 글자 그대로 남겨라.\n\
         - text에는 고쳐 쓴 파일 전체를, removed에는 합치거나 뺀 것을 {answer} 문장 목록으로 담아라.\n\
         <file>\n{text}\n</file>\n",
        name = target.name,
        text = target.original,
        answer = answer_language(),
    )
}

fn args(schema: &str) -> [&str; 13] {
    [
        "-p",
        "--safe-mode",
        "--model",
        MODEL,
        "--effort",
        EFFORT,
        "--tools",
        "",
        "--no-session-persistence",
        "--output-format",
        "json",
        "--json-schema",
        schema,
    ]
}

/// `terminal_reason` 같은 짧은 표식만 남긴다(영문 소문자·숫자·`_`, 40자).
fn short_mark(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?;
    let is_plain = text
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    (is_plain && !text.is_empty() && text.len() <= MAX_REASON_CHARS).then(|| text.to_owned())
}

fn exit_reason(out: &Output, json: &Value) -> String {
    let code = out
        .status
        .code()
        .map_or("signal".to_owned(), |c| c.to_string());
    match short_mark(json.get("terminal_reason")) {
        Some(mark) => format!("exit {code}, {mark}"),
        None => format!("exit {code}"),
    }
}

pub(super) fn clip(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

/// 실행 결과 판정(0018 "결과 판정"). 성공 = exit 0, `is_error: false`, 스키마(`T`)에 맞는 `structured_output`.
pub fn classify<T: DeserializeOwned>(
    result: Result<Output, RunError>,
    timeout: Duration,
) -> Result<T, HandoffError> {
    let out = match result {
        Ok(out) => out,
        Err(RunError::Timeout) => {
            return Err(HandoffError::Timeout {
                secs: timeout.as_secs(),
            });
        }
        Err(RunError::Spawn) => {
            return Err(failed(pick(
                "Claude를 실행하지 못했어요",
                "couldn't start Claude",
            )));
        }
    };
    let json: Option<Value> = serde_json::from_slice(&out.stdout).ok();
    let api_status = json
        .as_ref()
        .and_then(|j| j.get("api_error_status"))
        .and_then(Value::as_u64);
    let is_unrecognized = String::from_utf8_lossy(&out.stderr).contains("unrecognized_model");
    if api_status == Some(404) || is_unrecognized {
        return Err(HandoffError::OpusUnavailable);
    }
    let Some(json) = json else {
        return Err(failed(pick(
            "Claude 응답을 읽지 못했어요",
            "couldn't read Claude's answer",
        )));
    };
    let is_error = json.get("is_error").and_then(Value::as_bool) != Some(false);
    if !out.status.success() || is_error {
        return Err(failed(exit_reason(&out, &json)));
    }
    json.get("structured_output")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| {
            failed(pick(
                "Claude 응답이 약속한 모양이 아니에요",
                "Claude's answer wasn't in the agreed shape",
            ))
        })
}

/// `removed` 정리: 앞뒤 공백을 떼고 빈 줄은 뺀다. 50개·한 줄 300자까지.
fn tidy(rewrite: Rewrite) -> Rewrite {
    Rewrite {
        removed: rewrite
            .removed
            .iter()
            .take(MAX_REMOVED)
            .map(|r| clip(r.trim(), MAX_REMOVED_CHARS))
            .filter(|r| !r.is_empty())
            .collect(),
        ..rewrite
    }
}

/// 원본이 줄바꿈으로 끝나면 결과도 줄바꿈으로 끝나게 맞춘다.
fn final_text(original: &str, text: &str) -> String {
    if original.ends_with('\n') && !text.is_empty() && !text.ends_with('\n') {
        format!("{text}\n")
    } else {
        text.to_owned()
    }
}

/// 실행에 필요한 것. `dir`은 앱 데이터 폴더 아래 `claude-handoff`.
pub struct RunSpec {
    pub cli: Option<PathBuf>,
    pub dir: PathBuf,
    pub timeout: Duration,
}

fn io_failure(error: &std::io::Error) -> HandoffError {
    eprintln!("kcal: Claude에게 맡기기 준비 실패({:?})", error.kind());
    failed(crate::tr!(
        "준비하지 못했어요({:?})",
        "couldn't prepare ({:?})",
        error.kind()
    ))
}

/// 프롬프트를 stdin 파일로 넣고, 빈 작업 폴더에서 `schema`로 한 번 실행한다. 프롬프트 파일은 끝나면 지운다.
pub(super) async fn execute<T: DeserializeOwned>(
    spec: &RunSpec,
    cli: &Path,
    prompt: &str,
    schema: &str,
) -> Result<T, HandoffError> {
    let work = spec.dir.join("work");
    std::fs::create_dir_all(&work).map_err(|e| io_failure(&e))?;
    let prompt_file = spec.dir.join(format!("prompt-{}.txt", std::process::id()));
    std::fs::write(&prompt_file, prompt).map_err(|e| io_failure(&e))?;
    let stdin = std::fs::File::open(&prompt_file);
    let result = match stdin {
        Ok(file) => {
            let input = RunInput {
                stdin: Some(file),
                cwd: Some(&work),
            };
            Ok(run(cli, &args(schema), spec.timeout, input).await)
        }
        Err(e) => Err(io_failure(&e)),
    };
    let _ = std::fs::remove_file(&prompt_file);
    classify(result?, spec.timeout)
}

/// 맡기기 한 번. 같은 열쇠의 보관본이 있으면 실행하지 않고 그것을 돌려준다(한도를 두 번 쓰지 않는다).
/// 그다음 순서: 돌고 있으면 거절 → 남은 한도 문 → CLI 찾기 → 실행 → 보관.
pub async fn ask(
    state: &HandoffState,
    target: Target,
    remaining: Option<f64>,
    min_remaining: u64,
    spec: RunSpec,
) -> Result<HandoffPreview, HandoffError> {
    if let Some(stored) = state.stored(&target.id, &target.before_hash)? {
        return Ok(preview_of(&target.file, &stored));
    }
    let Some(_running) = state.start() else {
        return Err(HandoffError::Busy);
    };
    check_limit(remaining, min_remaining)?;
    let cli = spec.cli.clone().ok_or(HandoffError::CliMissing)?;
    let rewrite = tidy(execute(&spec, &cli, &prompt(&target), SCHEMA).await?);
    let stored = Stored {
        text: final_text(&target.original, &rewrite.text),
        removed: rewrite.removed,
        id: target.id,
        before_hash: target.before_hash,
        original: target.original,
    };
    state.keep(stored.clone())?;
    Ok(preview_of(&target.file, &stored))
}

/// 적용 기록 이름(0018 "적용").
fn log_name(name: &str) -> String {
    crate::tr!("{name} 줄이기", "Shorten {name}")
}

/// 보관본을 0014 방식(잠금·해시·백업·temp→rename·기록)으로 쓴다. 화면이 보낸 글은 받지 않는다.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    state: &HandoffState,
    home: &Path,
    records: &RecentRecords,
    tuning: &Tuning,
    log_file: &Path,
    id: &str,
    before_hash: &str,
    now: OffsetDateTime,
) -> Result<AppliedSwitch, AppError> {
    validate_hash(before_hash)?;
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Internal)?;
    let target = find_target(home, records, tuning, id)?;
    let stored = state.stored(id, before_hash)?.ok_or_else(|| {
        refuse(pick(
            "Claude에게 맡긴 결과가 없어요. 다시 맡겨 주세요.",
            "There's no result from Claude. Ask Claude again.",
        ))
    })?;
    if target.before_hash != before_hash {
        return Err(refuse(pick(
            "Claude에게 맡긴 뒤 파일이 바뀌었어요. 다시 맡겨 주세요.",
            "The file changed after you asked Claude. Ask Claude again.",
        )));
    }
    if !preview_of(&target.file, &stored).can_apply {
        return Err(refuse(pick(
            "Claude가 돌려준 글이 비었거나 원본보다 길어서 바꾸지 않아요.",
            "Claude's text is empty or longer than the original, so kcal won't apply it.",
        )));
    }
    let backup = backup_and_replace(&target.file, &stored.text, now)?;
    let entry = AppliedSwitch {
        id: record_id(now),
        at: now,
        action: CoachAction::Apply,
        target: check_target(id, HANDOFF),
        agent: Agent::Claude,
        name: log_name(&target.name),
        file: target.file.to_string_lossy().into_owned(),
        backup: backup.to_string_lossy().into_owned(),
        enabled: false,
        tokens: None,
        after_hash: edit::content_hash(stored.text.as_bytes()),
        undo_command: copy_command(&backup, &target.file),
        undo_of: None,
        created: None,
        files: Vec::new(),
    };
    append_applied(log_file, &entry)?;
    Ok(entry)
}

/// Claude 안내문 모양: 이름 `CLAUDE.md`·`CLAUDE.local.md`, 또는 `.claude/rules/` 아래 `.md`. 절대 경로만.
pub fn is_instruction_file(file: &Path) -> bool {
    if !file.is_absolute() || file.components().any(|c| c == Component::ParentDir) {
        return false;
    }
    if is_claude_md_name(file) {
        return true;
    }
    let parts: Vec<&std::ffi::OsStr> = file.components().map(|c| c.as_os_str()).collect();
    let is_md = file.extension().is_some_and(|e| e == "md");
    let in_rules = parts
        .windows(2)
        .any(|w| w[0] == ".claude" && w[1] == "rules");
    is_md && in_rules
}

/// 되돌리기 조건(0018 "되돌리기"): 점검 기록이고 파일이 Claude 안내문 모양이다. 백업 모양은 `undo`가 따로 본다.
pub fn is_undoable(target: &str, file: &Path) -> bool {
    target.starts_with(CHECK_TARGET_PREFIX) && is_instruction_file(file)
}

// ---------- commands ----------

pub(super) fn claude_remaining(app: &AppHandle) -> Result<Option<f64>, AppError> {
    let now = OffsetDateTime::now_utc();
    Ok(app
        .state::<UsageState>()
        .snapshot()?
        .agents
        .iter()
        .find(|a| a.agent == Agent::Claude)
        .and_then(|a| a.min_remaining(now)))
}

pub(super) fn handoff_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join(DIR_NAME))
        .map_err(|_| AppError::Internal)
}

/// "Claude에게 줄여 달라고 하기". 한도 검사는 화면만 믿지 않고 여기서 실행 직전에 다시 한다.
#[tauri::command]
pub async fn ask_claude_handoff(
    app: AppHandle,
    id: String,
) -> Result<HandoffPreview, HandoffError> {
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

/// 보관본을 적용한다. 기록은 `AppliedSwitch`라 `undo_switch`로 되돌린다.
#[tauri::command]
pub async fn apply_claude_handoff(
    app: AppHandle,
    id: String,
    before_hash: String,
) -> Result<AppliedSwitch, AppError> {
    let log = crate::coach::log_file(&app)?;
    let handle = app.clone();
    let result = super::with_records(&app, move |home, records, tuning| {
        let state = handle.state::<HandoffState>();
        let now = OffsetDateTime::now_utc();
        apply(
            &state,
            &home,
            &records,
            &tuning,
            &log,
            &id,
            &before_hash,
            now,
        )
    })
    .await?;
    crate::context::prewarm(app, true);
    Ok(result)
}

#[cfg(test)]
#[path = "handoff_tests.rs"]
mod tests;
