//! 2층 환율(docs/decisions/0006). 토큰·턴이 한도 몇 %p인가.
//!
//! - Codex: 5시간 창 단위 NNLS 추정(`codex`, `fit`).
//! - Kiro: 모델별 크레딧/턴 실측(`kiro`).
//! - Claude: 한도 기록(`history`)의 5시간 창 × Claude Code 기록 토큰으로 Codex와 같은 NNLS 추정(`claude`).
//!
//! 로그는 숫자 필드·모델 이름만 읽는다. 마지막 결과를 `cache::Cache`에 두고 즉시 돌려주며, 오래되면
//! 뒤에서 다시 계산해 `rates-updated` event로 보낸다. TS 쪽 짝은 `src/lib/ipc.ts`의 같은 이름 타입.

mod behavior;
mod claude;
pub(crate) mod codex;
mod fit;
pub(crate) mod history;
pub(crate) mod kiro;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use time::OffsetDateTime;

use crate::cache::Cache;
use crate::error::AppError;
use crate::settings::SettingsState;
use crate::tuning::Tuning;
use crate::usage::{Agent, AgentUsage, UsageSource};

/// 숫자 대신 보여 줄 상태. `Estimated`만 환율 숫자가 있다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RateStatus {
    /// 검증 R² ≥ `good_r2`(기본 0.7).
    Estimated,
    /// `range_r2`(기본 0.3) ≤ 검증 R² < `good_r2`: LOO fold 범위만.
    RangeOnly,
    /// 검증 R² < `range_r2`이거나 계산 불가.
    Unreliable,
    /// 창 수 < 변수 수 × `min_samples_per_var`(기본 3).
    Insufficient,
    /// 이벤트 없음.
    NoData,
}

/// 1%p당 토큰. 계수가 0이면(그 토큰은 한도를 설명하지 못함) `None`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRate {
    pub cached_input: Option<f64>,
    pub uncached_input: Option<f64>,
    pub output: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Bounds {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// LOO fold별 1%p당 토큰의 범위. 어떤 fold에서 계수가 0이면 `max`가 `None`(상한 없음).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRange {
    pub cached_input: Bounds,
    pub uncached_input: Bounds,
    pub output: Bounds,
}

/// 창 값들의 중앙값과 p90(파이썬 `statistics.median`, `quantiles(n=100, method="inclusive")[89]`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Spread {
    pub median: f64,
    pub p90: f64,
}

/// 최근 `days`일 5시간 창의 사용 패턴(실측). 3층 분해(docs/decisions/0008)의 N·M.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPattern {
    /// 기준 기간(일). settings `tuning.breakdownDays`.
    pub days: u64,
    /// 기간 안의 창 수.
    pub windows: usize,
    /// 창당 요청 수(반복 보고 제외, 입력 > 0).
    pub requests: Spread,
    /// 창당 캐시 미스 수(`cached / input < tuning.advanced.cacheMissRatio`).
    pub misses: Spread,
}

/// 보통 5시간 창 하나가 무엇으로 찼나(docs/decisions/0013). 몫은 창 토큰 합 ÷ 1%당 토큰(추론).
/// 기간 안 창 중 요청 수 중앙값 창(짝수 개면 아래쪽) 하나의 값이다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowComposition {
    /// 기간 안의 창 수.
    pub windows: usize,
    /// 고른 창의 요청 수 N·캐시 미스 수 M(실측). 검증 줄에 보인다.
    pub requests: usize,
    pub misses: usize,
    /// 캐시 입력 + 비캐시 입력의 몫(%).
    pub input_share: f64,
    /// 출력의 몫(%).
    pub output_share: f64,
    /// 고른 창의 실제 사용 %(창 안 최대 `used_percent`, 실측). 예상 합과 맞춰 보는 검증 값.
    pub used_percent: f64,
}

/// 기간 안 Codex 세션(rollout 파일 하나)의 사용량. 한도를 많이 쓴 순.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexSession {
    /// `session_meta`의 `cwd` 마지막 폴더. 없으면 rollout 파일 이름의 시작 시각 부분.
    pub name: String,
    /// `session_meta`의 `cwd` 전체 경로. 같은 `name`을 구분할 때 쓴다. 모르면 null.
    pub cwd: Option<String>,
    /// 기간 안 첫 요청 시각(RFC 3339).
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    /// 기간 안 요청 수(반복 보고 제외, 입력 > 0).
    pub requests: usize,
    /// 요청당 평균 입력 토큰(캐시 포함).
    pub avg_input_tokens: f64,
    pub misses: usize,
    /// 요청마다의 몫(%)을 더한 값(추론). 여러 5시간 창에 걸치면 100을 넘을 수 있다.
    pub share: f64,
    /// 요청 수 ≥ `tuning.marathonRequests`.
    pub is_marathon: bool,
    /// 평균 입력 ≥ `tuning.largeContextTokens`.
    pub is_large_context: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRate {
    pub status: RateStatus,
    /// 검증 R² = min(LOO, 시간순 holdout). 회귀를 못 했으면 `None`.
    pub r2: Option<f64>,
    /// 회귀에 쓴 5시간 창 수.
    pub windows: usize,
    /// `used_percent`가 없어 뺀 창 수.
    pub excluded_windows: usize,
    pub per_percent: Option<TokenRate>,
    pub fold_range: Option<TokenRange>,
    /// 최근 창 사용 패턴. 기간 안에 창이 없으면 `None`. 환율 판정과 따로 계산한다.
    pub pattern: Option<WindowPattern>,
    /// 보통 창의 구성. 환율이 `Estimated`가 아니거나 기간 안에 창이 없으면 `None`.
    pub composition: Option<WindowComposition>,
    /// 한도를 많이 쓴 세션 상위 `tuning.topSessions`개. 환율이 `Estimated`가 아니면 빈 목록.
    pub sessions: Vec<CodexSession>,
}

impl CodexRate {
    fn status(status: RateStatus, windows: usize, excluded_windows: usize) -> Self {
        Self {
            status,
            r2: None,
            windows,
            excluded_windows,
            per_percent: None,
            fold_range: None,
            pattern: None,
            composition: None,
            sessions: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroModelCredits {
    /// `session.json`의 `modelId`. 없으면 `unknown`.
    pub model: String,
    pub turns: u64,
    pub credits_per_turn: f64,
}

/// Claude 환율. Codex와 같은 모양이다(`composition`은 `None`, `sessions`는 빈 목록).
pub type ClaudeRate = CodexRate;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateSnapshot {
    /// 이 값을 계산한 시각(RFC 3339).
    #[serde(with = "time::serde::rfc3339")]
    pub computed_at: OffsetDateTime,
    pub codex: CodexRate,
    /// 턴이 많은 순. 비어 있으면 기록 없음.
    pub kiro: Vec<KiroModelCredits>,
    pub claude: ClaudeRate,
}

pub const RATES_UPDATED_EVENT: &str = "rates-updated";

/// 첫 계산에서 동시에 파싱할 최대 스레드 수. 나머지 코어는 앱·다른 작업에 남긴다.
const WARM_THREADS: usize = 4;

/// 파일마다 `read`(메모를 채우는 읽기)를 최대 `WARM_THREADS`개 스레드로 나눠 부른다.
pub(crate) fn warm_each(files: &[PathBuf], read: impl Fn(&Path) + Sync) {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(WARM_THREADS)
        .min(files.len());
    if threads < 2 {
        return;
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while let Some(file) =
                    files.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                {
                    read(file);
                }
            });
        }
    });
}

// Codex rollout·Kiro 세션·Claude 기록은 파일별 (mtime, 크기) 메모로 바뀐 파일만 다시 읽는다.
// 첫 계산은 메모에 없는 Codex·Kiro 파일을 `warm_each`로 나눠 읽는다(release, 같은 디스크 캐시).
fn compute(home: &Path, history_file: &Path, tuning: &Tuning) -> RateSnapshot {
    let now = OffsetDateTime::now_utc();
    let advanced = &tuning.advanced;
    // 이벤트는 한 번만 읽어 환율·창 패턴·행동 패턴에 같이 쓴다.
    let (events, cwds) = behavior::events_and_cwds(&codex::rollout_files(home));
    let now_secs = now.unix_timestamp_nanos() as f64 / 1e9;
    let rate = codex::rate(&events, advanced);
    let behavior = behavior::behavior(&events, &cwds, &rate, now_secs, tuning);
    let claude_requests = claude::requests(&claude::files(home));
    let claude_windows = claude::windows(history_file, advanced.reset_tol_secs as f64);
    RateSnapshot {
        computed_at: now,
        codex: CodexRate {
            pattern: codex::pattern(&events, now_secs, tuning.breakdown_days, advanced),
            composition: behavior.composition,
            sessions: behavior.sessions,
            ..rate
        },
        kiro: kiro::credits(home),
        claude: ClaudeRate {
            pattern: claude::pattern(
                &claude_requests,
                &claude_windows,
                now_secs,
                tuning.breakdown_days,
                advanced,
            ),
            ..claude::rate(&claude_requests, &claude_windows, advanced)
        },
    }
}

pub struct RatesState(Cache<RateSnapshot>);

impl Default for RatesState {
    fn default() -> Self {
        Self(Cache::new())
    }
}

pub(crate) fn history_file<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| history::file(&dir))
}

/// `is_forced`(새로고침)면 TTL을 0으로 보고 바로 다시 계산한다. 이미 계산 중이면 그 결과를 기다리지 않는다.
async fn rates(app: AppHandle, is_forced: bool) -> Result<RateSnapshot, AppError> {
    let home = std::env::home_dir().ok_or(AppError::Internal)?;
    let history = history_file(&app).ok_or(AppError::Internal)?;
    let cache = app.state::<RatesState>().0.clone();
    // 다음 계산부터 바뀐 값을 쓴다. 이미 캐시된 값은 다음 갱신 때 바뀐다.
    let tuning = app.state::<SettingsState>().tuning();
    let ttl = if is_forced {
        Duration::ZERO
    } else {
        tuning.rates_ttl()
    };
    cache
        .get(
            ttl,
            move || compute(&home, &history, &tuning),
            move |snapshot| {
                if app.emit(RATES_UPDATED_EVENT, snapshot).is_err() {
                    eprintln!("kcal: rates-updated event 전송 실패");
                }
            },
        )
        .await
}

/// 에이전트별 환율. 마지막 결과를 바로 돌려주고, 오래됐으면 뒤에서 다시 계산해 `rates-updated`로 보낸다.
#[tauri::command]
pub async fn get_rates(app: AppHandle) -> Result<RateSnapshot, AppError> {
    rates(app, false).await
}

/// 앱 시작 때 한 번 미리 계산한다. `is_forced`면 캐시가 새것이어도 다시 계산한다(새로고침).
pub fn prewarm(app: AppHandle, is_forced: bool) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = rates(app, is_forced).await {
            eprintln!("kcal: 환율 계산 실패({e:?})");
        }
    });
}

/// Claude 조회가 API(주 경로)로 성공했으면 한도 기록에 한 줄 더한다. 실패는 로그만 남긴다.
pub fn record_claude<R: Runtime>(app: &AppHandle<R>, usage: &AgentUsage) {
    if usage.agent != Agent::Claude || usage.source != Some(UsageSource::Api) {
        return;
    }
    let at = usage.observed_at.unwrap_or_else(OffsetDateTime::now_utc);
    let Some(entry) = history::HistoryEntry::from_windows(at, &usage.windows) else {
        return;
    };
    let Some(path) = history_file(app) else {
        eprintln!("kcal: 한도 기록 경로를 찾지 못함");
        return;
    };
    let reset_tol_secs = app
        .state::<SettingsState>()
        .tuning()
        .advanced
        .reset_tol_secs as f64;
    if let Err(e) = history::record(&path, &entry, reset_tol_secs) {
        eprintln!("kcal: 한도 기록 쓰기 실패({:?})", e.kind());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(windows: usize) -> RateSnapshot {
        RateSnapshot {
            computed_at: OffsetDateTime::UNIX_EPOCH,
            codex: CodexRate::status(RateStatus::NoData, 0, 0),
            kiro: Vec::new(),
            claude: CodexRate::status(RateStatus::Insufficient, windows, 2),
        }
    }

    #[test]
    fn serializes_status_in_snake_case_and_fields_in_camel_case() {
        let json = serde_json::to_value(snapshot(4)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "computedAt": "1970-01-01T00:00:00Z",
                "codex": {"status": "no_data", "r2": null, "windows": 0, "excludedWindows": 0,
                          "perPercent": null, "foldRange": null, "pattern": null,
                          "composition": null, "sessions": []},
                "kiro": [],
                "claude": {"status": "insufficient", "r2": null, "windows": 4, "excludedWindows": 2,
                           "perPercent": null, "foldRange": null, "pattern": null,
                           "composition": null, "sessions": []}
            })
        );
    }
}
