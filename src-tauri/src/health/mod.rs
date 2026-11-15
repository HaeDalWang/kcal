//! 6층 정기 검진 1차(docs/decisions/0016): 처방(코칭 적용 기록) 전후 비교와 주간 요약.
//!
//! 기준 시각 t마다 전 = [t − N일, t), 후 = [t, min(t + N일, 지금, 되돌린 시각)). 주간 요약은 t = 지금 − N일.
//! 지표는 요청당 입력 토큰(Kiro는 질문당 크레딧), 하루 평균 주간 한도 %p(Kiro는 크레딧), Codex 압축 횟수.
//! 로그는 숫자(토큰 수·크레딧·timestamp)만 읽는다(`read`). 마지막 결과를 `cache::Cache`에 두고,
//! 오래됐거나 적용 기록·설정이 바뀌었으면 뒤에서 다시 계산해 `health-updated` event로 보낸다.
//! TS 쪽 짝은 `src/lib/ipc.ts`의 같은 이름 타입.

mod read;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use time::OffsetDateTime;

use crate::cache::Cache;
use crate::coach::log::{self, AppliedSwitch, CoachAction};
use crate::error::AppError;
use crate::metrics::{BarUnit, DAY_SECS, at, unix_secs};
use crate::settings::{Settings, SettingsState};
use crate::usage::Agent;

pub use read::AgentSamples;

pub const HEALTH_UPDATED_EVENT: &str = "health-updated";

/// 구간 하나의 지표. 값을 모르면 `None`(0으로 채우지 않는다).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodStats {
    #[serde(with = "time::serde::rfc3339")]
    pub from: OffsetDateTime,
    /// 끝(포함하지 않음). 후 구간은 지금·되돌린 시각에서 잘린다.
    #[serde(with = "time::serde::rfc3339")]
    pub to: OffsetDateTime,
    /// 구간 길이(일, 소수). 하루 평균의 분모.
    pub elapsed_days: f64,
    /// 기록이 덮은 비율(0~1). 가장 이른 기록이 구간 시작보다 늦으면 1 미만. 기록이 없으면 `None`.
    pub coverage: Option<f64>,
    /// 요청 수(Kiro는 질문 수).
    pub requests: Option<u64>,
    /// 요청당 평균 입력 토큰(캐시 포함, Claude·Codex).
    pub avg_input_tokens: Option<f64>,
    /// 질문당 평균 크레딧(Kiro).
    pub avg_credits: Option<f64>,
    /// 하루 평균 쓴 양. 단위는 `used_unit`: percent = 주간 한도 %p, credits = 크레딧.
    pub daily_used: Option<f64>,
    pub used_unit: BarUnit,
    /// 압축 횟수(Codex만).
    pub compactions: Option<u64>,
}

/// 에이전트 하나의 전후. 주간 요약 한 줄.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentComparison {
    pub agent: Agent,
    pub before: PeriodStats,
    pub after: PeriodStats,
}

/// 적용 기록 하나의 전후(그 기록의 에이전트만).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrescriptionEffect {
    /// 적용 기록 ID(`AppliedSwitch.id`).
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub agent: Agent,
    pub name: String,
    /// `AppliedSwitch.target`. 점검 기록(`coach::checks::CHECK_TARGET_PREFIX`)이면 화면이 "켬/끔"을 붙이지 않는다.
    pub target: String,
    /// 적용 뒤 상태(켬 = true).
    pub enabled: bool,
    /// 되돌렸으면 그 시각. 후 구간은 여기서 잘린다.
    #[serde(with = "time::serde::rfc3339::option")]
    pub undone_at: Option<OffsetDateTime>,
    pub before: PeriodStats,
    pub after: PeriodStats,
}

/// 계산에 쓴 입력. 바뀌면 캐시가 새것이어도 다시 계산한다. IPC로 내보내지 않는다.
#[derive(Clone, Debug, Default, PartialEq)]
struct Basis {
    log_entries: usize,
    compare_days: u64,
    agents: Vec<Agent>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    #[serde(with = "time::serde::rfc3339")]
    pub computed_at: OffsetDateTime,
    pub compare_days: u64,
    /// 켠 에이전트마다 최근 N일 vs 그 전 N일. `Agent::ALL` 순서.
    pub weekly: Vec<AgentComparison>,
    /// 켠 에이전트의 적용 기록, 최근순.
    pub prescriptions: Vec<PrescriptionEffect>,
    #[serde(skip)]
    basis: Basis,
}

fn used_unit(agent: Agent) -> BarUnit {
    match agent {
        Agent::Kiro => BarUnit::Credits,
        Agent::Claude | Agent::Codex => BarUnit::Percent,
    }
}

fn in_period(ts: f64, from: f64, to: f64) -> bool {
    ts >= from && ts < to
}

/// 기록이 `[from, to)`를 덮은 비율. 기록이 없으면 `None`.
fn covered(data_from: Option<f64>, from: f64, to: f64) -> Option<f64> {
    data_from.map(|start| ((to - start.max(from)) / (to - from)).clamp(0.0, 1.0))
}

fn sum_in(items: &[(f64, f64)], from: f64, to: f64) -> (u64, f64) {
    items
        .iter()
        .filter(|(ts, _)| in_period(*ts, from, to))
        .fold((0, 0.0), |(n, total), (_, v)| (n + 1, total + v))
}

/// `[from, to)`의 지표. 기록마다(요청·쓴 양) 덮은 비율이 0이면 그 값은 `None`이고,
/// `coverage`는 있는 기록 중 가장 작은 비율이다.
pub fn period(samples: &AgentSamples, from: f64, to: f64) -> PeriodStats {
    let to = to.max(from);
    let unit = used_unit(samples.agent);
    let elapsed_days = (to - from) / DAY_SECS;
    let empty = PeriodStats {
        from: at(from),
        to: at(to),
        elapsed_days,
        coverage: None,
        requests: None,
        avg_input_tokens: None,
        avg_credits: None,
        daily_used: None,
        used_unit: unit,
        compactions: None,
    };
    if to <= from {
        return empty;
    }
    let request_cover = covered(samples.requests_from, from, to);
    let used_cover = covered(samples.used_from, from, to);
    let has_requests = request_cover.is_some_and(|c| c > 0.0);
    let (count, total) = sum_in(&samples.requests, from, to);
    let avg = (has_requests && count > 0).then(|| total / count as f64);
    PeriodStats {
        coverage: match (request_cover, used_cover) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        },
        requests: has_requests.then_some(count),
        avg_input_tokens: avg.filter(|_| unit == BarUnit::Percent),
        avg_credits: avg.filter(|_| unit == BarUnit::Credits),
        // 기록이 덮은 일수로 나눈다: 구간 전체로 나누면 기록이 일부뿐인 쪽이 작게 보여 전후 비교가 틀어진다.
        daily_used: used_cover
            .filter(|c| *c > 0.0)
            .map(|c| sum_in(&samples.used, from, to).1 / (elapsed_days * c)),
        compactions: samples
            .compactions
            .as_ref()
            .filter(|_| has_requests)
            .map(|times| times.iter().filter(|ts| in_period(**ts, from, to)).count() as u64),
        ..empty
    }
}

/// 기준 시각 `t`의 전 `[t − span, t)`와 후 `[t, end)`.
fn compare(samples: &AgentSamples, t: f64, span: f64, end: f64) -> (PeriodStats, PeriodStats) {
    (period(samples, t - span, t), period(samples, t, end))
}

/// 읽어야 하는 가장 이른 시각: 주간 요약의 전 구간 시작과 가장 오래된 적용 기록 − N일 중 이른 쪽.
pub fn read_start(log: &[AppliedSwitch], now: f64, days: u64) -> f64 {
    let span = days as f64 * DAY_SECS;
    log.iter()
        .filter(|e| e.action == CoachAction::Apply)
        .map(|e| unix_secs(e.at) - span)
        .fold(now - 2.0 * span, f64::min)
}

/// 읽은 기록(`samples`, 켠 에이전트만)과 적용 기록(최근순)으로 보고서를 만든다.
pub fn report(
    samples: &[AgentSamples],
    log: &[AppliedSwitch],
    now: OffsetDateTime,
    days: u64,
) -> HealthReport {
    let secs = unix_secs(now);
    let span = days as f64 * DAY_SECS;
    let weekly = samples
        .iter()
        .map(|s| {
            let (before, after) = compare(s, secs - span, span, secs);
            AgentComparison {
                agent: s.agent,
                before,
                after,
            }
        })
        .collect();
    let prescriptions = log
        .iter()
        .filter(|e| e.action == CoachAction::Apply)
        .filter_map(|e| {
            let s = samples.iter().find(|s| s.agent == e.agent)?;
            let undone_at = log
                .iter()
                .find(|u| u.action == CoachAction::Undo && u.undo_of.as_deref() == Some(&e.id))
                .map(|u| u.at);
            let t = unix_secs(e.at);
            let end = (t + span)
                .min(secs)
                .min(undone_at.map_or(f64::INFINITY, unix_secs));
            let (before, after) = compare(s, t, span, end);
            Some(PrescriptionEffect {
                id: e.id.clone(),
                at: e.at,
                agent: e.agent,
                name: e.name.clone(),
                target: e.target.clone(),
                enabled: e.enabled,
                undone_at,
                before,
                after,
            })
        })
        .collect();
    HealthReport {
        computed_at: now,
        compare_days: days,
        weekly,
        prescriptions,
        basis: Basis::default(),
    }
}

fn basis(log: &[AppliedSwitch], settings: &Settings) -> Basis {
    Basis {
        log_entries: log.len(),
        compare_days: settings.tuning.compare_days,
        agents: settings.enabled_agents.clone(),
    }
}

fn compute(
    home: &Path,
    history: &Path,
    log: &[AppliedSwitch],
    settings: &Settings,
) -> HealthReport {
    let now = OffsetDateTime::now_utc();
    let days = settings.tuning.compare_days;
    let since = read_start(log, unix_secs(now), days);
    let samples: Vec<AgentSamples> = settings
        .enabled_agents
        .iter()
        .map(|&agent| read::samples(home, history, agent, since, &settings.tuning))
        .collect();
    HealthReport {
        basis: basis(log, settings),
        ..report(&samples, log, now, days)
    }
}

pub struct HealthState(Cache<HealthReport>);

impl Default for HealthState {
    fn default() -> Self {
        Self(Cache::new())
    }
}

fn log_file(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map(|dir| log::file(&dir))
        .map_err(|_| AppError::Internal)
}

/// `is_forced`(새로고침)면 TTL을 0으로 보고 바로 다시 계산한다. TTL은 지표와 같은 `metricsTtlSecs`.
async fn health(app: AppHandle, is_forced: bool) -> Result<HealthReport, AppError> {
    let home = std::env::home_dir().ok_or(AppError::Internal)?;
    let history = crate::rate::history_file(&app).ok_or(AppError::Internal)?;
    let log_path = log_file(&app)?;
    let log = tauri::async_runtime::spawn_blocking(move || log::read(&log_path))
        .await
        .map_err(|_| AppError::Internal)?;
    let settings = app.state::<SettingsState>().get()?;
    let current = basis(&log, &settings);
    let ttl = if is_forced {
        Duration::ZERO
    } else {
        settings.tuning.metrics_ttl()
    };
    let cache = app.state::<HealthState>().0.clone();
    let emitter = app.clone();
    let report = cache
        .get(
            ttl,
            move || compute(&home, &history, &log, &settings),
            move |report| {
                if emitter.emit(HEALTH_UPDATED_EVENT, report).is_err() {
                    eprintln!("kcal: {HEALTH_UPDATED_EVENT} event 전송 실패");
                }
            },
        )
        .await?;
    // 적용·되돌리기·설정 변경 뒤 첫 호출: 옛 값을 주고 뒤에서 다시 계산한다(결과는 event).
    if !is_forced && report.basis != current {
        prewarm(app, true);
    }
    Ok(report)
}

/// 처방 전후 비교와 주간 요약. 마지막 결과를 바로 돌려주고, 오래됐으면 뒤에서 다시 계산해 `health-updated`로 보낸다.
#[tauri::command]
pub async fn get_health(app: AppHandle) -> Result<HealthReport, AppError> {
    health(app, false).await
}

/// `is_forced`면 캐시가 새것이어도 다시 계산한다(새로고침).
pub fn prewarm(app: AppHandle, is_forced: bool) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = health(app, is_forced).await {
            eprintln!("kcal: 처방 효과 계산 실패({e:?})");
        }
    });
}

#[cfg(test)]
mod tests;
