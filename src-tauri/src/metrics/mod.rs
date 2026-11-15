//! 트레이 지표(docs/decisions/0010): 최근 5시간 요청 수, 캐시 적중률, 최근 구간별 사용량 막대.
//!
//! - Codex: rollout `token_count`(rate::codex와 같은 수집·중복 제거).
//! - Claude: 요청 수·캐시는 `~/.claude/projects/**/*.jsonl`의 assistant `usage` 숫자, 막대는 한도 기록.
//! - Kiro: 5시간 한도가 없어 최근 5시간 질문 수와 하루(24시간) 단위 크레딧.
//!
//! 로그에서는 숫자(토큰 수, timestamp)와 `message.id`만 꺼낸다. 메시지 본문은 저장·로그·IPC로 내보내지 않는다.
//! 마지막 결과를 `cache::Cache`에 두고 즉시 돌려주며, 오래되면 뒤에서 다시 계산해 `metrics-updated` event로 보낸다.
//! TS 쪽 짝은 `src/lib/ipc.ts`의 같은 이름 타입.

pub(crate) mod claude;
mod codex;
mod kiro;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use time::OffsetDateTime;

use crate::cache::Cache;
use crate::error::AppError;
use crate::settings::SettingsState;
use crate::tuning::Tuning;
use crate::usage::Agent;

const FIVE_HOURS_SECS: f64 = 5.0 * 3600.0;
pub(crate) const DAY_SECS: f64 = 86_400.0;
const HOUR_SECS: f64 = 3600.0;

/// 막대 값의 단위.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BarUnit {
    /// 그 5시간 구간 안에서 가장 큰 사용 %(Claude·Codex).
    Percent,
    /// 그 하루(24시간) 동안 쓴 크레딧(Kiro).
    Credits,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageBar {
    /// 구간 시작(5시간 구간은 `resetsAt − 5h`).
    #[serde(with = "time::serde::rfc3339")]
    pub starts_at: OffsetDateTime,
    pub value: f64,
    /// 지금 진행 중인 구간.
    pub is_current: bool,
}

/// 한 시간(UTC 정시부터) 동안의 활동. 대시보드가 지역 날짜로 묶는다(0015).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HourBucket {
    #[serde(with = "time::serde::rfc3339")]
    pub starts_at: OffsetDateTime,
    /// 요청 수(Kiro는 질문 수).
    pub requests: u64,
    /// 쓴 양. 단위는 `AgentMetrics.bar_unit`: percent = 주간(7d) 한도 %p 증가분, credits = 크레딧.
    pub used: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMetrics {
    pub agent: Agent,
    /// 지금 5시간 한도 구간(Claude·Codex) 또는 최근 5시간(Kiro, 질문 수)의 요청 수. 모르면 `None`.
    pub recent_requests: Option<u64>,
    /// 같은 구간의 캐시에서 읽은 입력 ÷ 전체 입력(0~1, 토큰 기준). 모르면 `None`.
    pub cache_hit_ratio: Option<f64>,
    pub bar_unit: BarUnit,
    /// 오래된 순. 비어 있으면 기록 없음.
    pub bars: Vec<UsageBar>,
    /// 최근 `bar_days + 1`일의 시간 버킷. 오래된 순, 마지막이 지금 시간. 빈 시간도 0으로 들어 있다.
    pub hourly: Vec<HourBucket>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSnapshot {
    /// 계산한 시각(RFC 3339).
    #[serde(with = "time::serde::rfc3339")]
    pub computed_at: OffsetDateTime,
    pub agents: Vec<AgentMetrics>,
}

pub const METRICS_UPDATED_EVENT: &str = "metrics-updated";

/// 유닉스 초 → 시각. 범위 밖이면 1970-01-01.
pub(crate) fn at(secs: f64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos((secs * 1e9) as i128)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

pub(crate) fn unix_secs(time: OffsetDateTime) -> f64 {
    time.unix_timestamp_nanos() as f64 / 1e9
}

/// 파일이 마지막으로 바뀐 시각(유닉스 초).
pub(crate) fn modified_secs(path: &Path) -> Option<f64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let since = modified.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    Some(since.as_secs_f64())
}

/// 캐시 읽기 ÷ 전체 입력. 입력이 0이면 `None`.
fn ratio(cached: f64, total: f64) -> Option<f64> {
    (total > 0.0).then(|| (cached / total).clamp(0.0, 1.0))
}

/// `resets_at`을 `reset_tol_secs`로 묶은 5시간 구간 하나.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Window {
    first_reset: f64,
    /// 구간의 리셋 시각(묶음 안 가장 늦은 값).
    reset: f64,
    peak_percent: f64,
}

impl Window {
    fn contains(&self, resets_at: f64) -> bool {
        (self.first_reset..=self.reset).contains(&resets_at)
    }

    fn start(&self) -> f64 {
        self.reset - FIVE_HOURS_SECS
    }
}

/// (resets_at, used_percent) 표본을 구간으로 묶는다. 리셋 순.
fn windows(mut samples: Vec<(f64, f64)>, reset_tol_secs: f64) -> Vec<Window> {
    samples.retain(|(reset, pct)| reset.is_finite() && pct.is_finite());
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let resets: Vec<f64> = samples.iter().map(|s| s.0).collect();
    let ids = crate::rate::codex::cluster_ids(&resets, reset_tol_secs);
    let mut out: Vec<Window> = Vec::new();
    for (i, (reset, pct)) in samples.into_iter().enumerate() {
        match out.last_mut() {
            Some(w) if i > 0 && ids[i] == ids[i - 1] => {
                w.reset = reset;
                w.peak_percent = w.peak_percent.max(pct);
            }
            _ => out.push(Window {
                first_reset: reset,
                reset,
                peak_percent: pct,
            }),
        }
    }
    out
}

/// 지금 진행 중인 구간(리셋이 아직 안 지난 마지막 구간).
fn current(windows: &[Window], now: f64) -> Option<Window> {
    windows.last().copied().filter(|w| w.reset > now)
}

/// 최근 `count`개 구간의 막대. 지금 구간은 마지막 하나뿐이다: 한도가 일찍 초기화되면 리셋이 안 지난 옛 구간 뒤에
/// 새 구간이 생긴다(이 Mac Codex 2026-11-05: 09:14 리셋 구간이 04:23에 끝나고 04:44에 09:44 리셋 구간 시작).
fn percent_bars(windows: &[Window], now: f64, count: usize) -> Vec<UsageBar> {
    let running = current(windows, now).map(|w| w.reset);
    windows[windows.len().saturating_sub(count)..]
        .iter()
        .map(|w| UsageBar {
            starts_at: at(w.start()),
            value: w.peak_percent,
            is_current: Some(w.reset) == running,
        })
        .collect()
}

/// 시간 버킷이 덮는 기간의 시작(유닉스 초, UTC 정시). 지금 시간을 포함해 `(days + 1) × 24`개.
fn hourly_start(now: f64, days: u64) -> f64 {
    let hours = (days + 1) * 24;
    (now / HOUR_SECS).floor() * HOUR_SECS - (hours - 1) as f64 * HOUR_SECS
}

/// 요청 시각과 (시각, 쓴 양)을 시간 버킷에 넣는다. 기간 밖(`hourly_start` 전, `now` 뒤)은 버린다.
fn hourly(now: f64, days: u64, requests: &[f64], used: &[(f64, f64)]) -> Vec<HourBucket> {
    let start = hourly_start(now, days);
    let hours = ((days + 1) * 24) as usize;
    let slot = |ts: f64| {
        (ts >= start && ts <= now).then(|| (((ts - start) / HOUR_SECS) as usize).min(hours - 1))
    };
    let mut buckets: Vec<HourBucket> = (0..hours)
        .map(|i| HourBucket {
            starts_at: at(start + i as f64 * HOUR_SECS),
            requests: 0,
            used: 0.0,
        })
        .collect();
    for i in requests.iter().filter_map(|&ts| slot(ts)) {
        buckets[i].requests += 1;
    }
    for &(ts, amount) in used {
        if let Some(i) = slot(ts) {
            buckets[i].used += amount;
        }
    }
    buckets
}

/// 주간 한도 표본 (시각, resets_at, used_percent) → (시각, 증가분 %p). 시각순으로 본다.
/// - 같은 창(직전 리셋과 `reset_tol_secs` 이내): 그 창에서 지금까지 본 가장 큰 값보다 오른 만큼. 내려가면 0.
/// - 더 늦은 리셋의 새 창: 첫 표본은 0에서 오른 것으로 본다.
/// - 이미 지난 창의 늦게 온 표본은 0. 맨 첫 표본은 기준점(앞을 몰라 0).
pub(crate) fn weekly_increments(
    mut samples: Vec<(f64, f64, f64)>,
    reset_tol_secs: f64,
) -> Vec<(f64, f64)> {
    samples.retain(|(ts, reset, pct)| ts.is_finite() && reset.is_finite() && pct.is_finite());
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut window: Option<(f64, f64)> = None; // (리셋, 지금까지 최대 %)
    samples
        .into_iter()
        .map(|(ts, reset, pct)| {
            let delta = match window {
                None => 0.0,
                Some((last, _)) if reset > last + reset_tol_secs => pct,
                Some((last, peak)) if reset >= last - reset_tol_secs => (pct - peak).max(0.0),
                Some(_) => return (ts, 0.0),
            };
            window = match window {
                Some((last, peak)) if (reset - last).abs() <= reset_tol_secs => {
                    Some((last.max(reset), peak.max(pct)))
                }
                _ => Some((reset, pct)),
            };
            (ts, delta)
        })
        .collect()
}

fn compute(home: &Path, history_file: &Path, tuning: &Tuning) -> MetricsSnapshot {
    let now = OffsetDateTime::now_utc();
    let secs = unix_secs(now);
    MetricsSnapshot {
        computed_at: now,
        agents: Agent::ALL
            .into_iter()
            .map(|agent| match agent {
                Agent::Claude => claude::metrics(home, history_file, secs, tuning),
                Agent::Codex => codex::metrics(home, secs, tuning),
                Agent::Kiro => kiro::metrics(home, secs, tuning),
            })
            .collect(),
    }
}

pub struct MetricsState(Cache<MetricsSnapshot>);

impl Default for MetricsState {
    fn default() -> Self {
        Self(Cache::new())
    }
}

/// `is_forced`(새로고침)면 TTL을 0으로 보고 바로 다시 계산한다.
async fn metrics(app: AppHandle, is_forced: bool) -> Result<MetricsSnapshot, AppError> {
    let home = std::env::home_dir().ok_or(AppError::Internal)?;
    let history: PathBuf = crate::rate::history_file(&app).ok_or(AppError::Internal)?;
    let cache = app.state::<MetricsState>().0.clone();
    let tuning = app.state::<SettingsState>().tuning();
    let ttl = if is_forced {
        Duration::ZERO
    } else {
        tuning.metrics_ttl()
    };
    cache
        .get(
            ttl,
            move || compute(&home, &history, &tuning),
            move |snapshot| {
                if app.emit(METRICS_UPDATED_EVENT, snapshot).is_err() {
                    eprintln!("kcal: {METRICS_UPDATED_EVENT} event 전송 실패");
                }
            },
        )
        .await
}

/// 에이전트별 트레이 지표. 마지막 결과를 바로 돌려주고, 오래됐으면 뒤에서 다시 계산해 `metrics-updated`로 보낸다.
#[tauri::command]
pub async fn get_metrics(app: AppHandle) -> Result<MetricsSnapshot, AppError> {
    metrics(app, false).await
}

/// 앱 시작 때 한 번 미리 계산한다. `is_forced`면 캐시가 새것이어도 다시 계산한다(새로고침).
pub fn prewarm(app: AppHandle, is_forced: bool) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = metrics(app, is_forced).await {
            eprintln!("kcal: 지표 계산 실패({e:?})");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = 3600.0;

    #[test]
    fn groups_resets_within_tolerance_and_keeps_the_peak() {
        let got = windows(
            vec![(10.0 * H + 18.0, 30.0), (10.0 * H, 12.0), (15.0 * H, 4.0)],
            600.0,
        );
        assert_eq!(got.len(), 2);
        assert_eq!(
            (got[0].first_reset, got[0].reset, got[0].peak_percent),
            (10.0 * H, 10.0 * H + 18.0, 30.0)
        );
        assert!(got[0].contains(10.0 * H + 5.0) && !got[0].contains(15.0 * H));
        assert_eq!(got[1].start(), 10.0 * H);
    }

    #[test]
    fn bars_keep_the_latest_windows_and_mark_the_running_one() {
        let ws = windows(
            (1..=5)
                .map(|i| (f64::from(i) * 5.0 * H, f64::from(i)))
                .collect(),
            600.0,
        );
        let now = 23.0 * H;
        let bars = percent_bars(&ws, now, 3);
        let values: Vec<f64> = bars.iter().map(|b| b.value).collect();
        assert_eq!(values, [3.0, 4.0, 5.0]);
        assert_eq!(
            bars.iter().map(|b| b.is_current).collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(bars[2].starts_at, at(20.0 * H));
        assert_eq!(current(&ws, now).map(|w| w.reset), Some(25.0 * H));
        assert_eq!(current(&ws, 26.0 * H), None);
        assert_eq!(ratio(1.0, 0.0), None);
        // 일찍 초기화돼 리셋이 안 지난 구간이 둘이면 마지막 것만 지금 구간이다.
        let early = windows(vec![(24.0 * H, 8.0), (24.5 * H, 58.0)], 600.0);
        let flags: Vec<bool> = percent_bars(&early, 20.0 * H, 8)
            .iter()
            .map(|b| b.is_current)
            .collect();
        assert_eq!(flags, [false, true]);
    }

    #[test]
    fn serializes_fields_in_camel_case_and_unit_in_snake_case() {
        let json = serde_json::to_value(AgentMetrics {
            agent: Agent::Kiro,
            recent_requests: Some(3),
            cache_hit_ratio: None,
            bar_unit: BarUnit::Credits,
            bars: vec![UsageBar {
                starts_at: OffsetDateTime::UNIX_EPOCH,
                value: 1.5,
                is_current: true,
            }],
            hourly: vec![HourBucket {
                starts_at: OffsetDateTime::UNIX_EPOCH,
                requests: 2,
                used: 0.5,
            }],
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "agent": "kiro", "recentRequests": 3, "cacheHitRatio": null, "barUnit": "credits",
                "bars": [{"startsAt": "1970-01-01T00:00:00Z", "value": 1.5, "isCurrent": true}],
                "hourly": [{"startsAt": "1970-01-01T00:00:00Z", "requests": 2, "used": 0.5}]
            })
        );
    }

    #[test]
    fn hourly_fills_every_hour_of_the_last_days_and_drops_out_of_range_items() {
        let now = 100.0 * DAY_SECS + 5.5 * H; // 지금 시간 = 5시
        let start = 98.0 * DAY_SECS + 6.0 * H; // (1 + 1)일 = 48칸, 지금 시간(5시)에서 47시간 전
        let got = hourly(
            now,
            1,
            &[now - 10.0, now - 20.0, start, start - 1.0, now + 1.0],
            &[
                (start + 1.5 * H, 2.0),
                (start + 1.9 * H, 0.5),
                (now + H, 9.0),
            ],
        );
        assert_eq!(got.len(), 48);
        assert_eq!(got[0].starts_at, at(start));
        assert_eq!(got[47].starts_at, at(100.0 * DAY_SECS + 5.0 * H));
        assert_eq!((got[47].requests, got[0].requests), (2, 1));
        assert_eq!(got[1].used, 2.5);
        let total: f64 = got.iter().map(|b| b.used).sum();
        assert_eq!(total, 2.5);
        assert_eq!(got.iter().map(|b| b.requests).sum::<u64>(), 3);
    }

    #[test]
    fn weekly_increments_rise_within_a_window_restart_after_reset_and_ignore_drops() {
        let (w1, w2) = (200.0 * H, 368.0 * H);
        let got = weekly_increments(
            vec![
                (2.0 * H, w1 + 5.0, 12.0), // 시각순으로 다시 놓인다
                (1.0 * H, w1, 10.0),       // 맨 첫 표본: 기준점
                (3.0 * H, w1, 11.0),       // 내려감 → 0, 최대값 12 유지
                (4.0 * H, w1, 13.0),       // 12에서 +1
                (5.0 * H, w2, 3.0),        // 새 창: 0에서 +3
                (6.0 * H, w1, 50.0),       // 지난 창의 늦은 표본 → 0
                (7.0 * H, w2, 4.5),        // +1.5
            ],
            600.0,
        );
        let deltas: Vec<f64> = got.iter().map(|d| d.1).collect();
        assert_eq!(deltas, [0.0, 2.0, 0.0, 1.0, 3.0, 0.0, 1.5]);
        assert_eq!(got[1].0, 2.0 * H);
    }

    /// 이 Mac의 실제 기록으로 지표를 낸다(숫자만, 토큰·본문 없음).
    /// `cargo test -- --ignored real_metrics_on_this_mac --nocapture`.
    #[test]
    #[ignore = "실제 ~/.codex·~/.claude·~/.kiro와 한도 기록을 읽는다"]
    fn real_metrics_on_this_mac() {
        let home = std::env::home_dir().unwrap();
        let history = crate::rate::history::file(
            &home.join("Library/Application Support/io.github.haedalwang.kcal"),
        );
        let snapshot = compute(&home, &history, &Tuning::default());
        println!("METRICS now={}", snapshot.computed_at);
        for m in &snapshot.agents {
            let bars: Vec<String> = m
                .bars
                .iter()
                .map(|b| {
                    let mark = if b.is_current { "*" } else { "" };
                    format!("{}={:.2}{mark}", b.starts_at, b.value)
                })
                .collect();
            println!(
                "METRICS {:?} recent_requests={:?} cache_hit_ratio={:?} bar_unit={:?} bars=[{}]",
                m.agent,
                m.recent_requests,
                m.cache_hit_ratio.map(|r| (r * 1000.0).round() / 10.0),
                m.bar_unit,
                bars.join(", ")
            );
            let busy: Vec<String> = m
                .hourly
                .iter()
                .filter(|b| b.requests > 0 || b.used > 0.0)
                .map(|b| format!("{}={}/{:.2}", b.starts_at, b.requests, b.used))
                .collect();
            println!(
                "HOURLY {:?} buckets={} requests={} used={:.2} active=[{}]",
                m.agent,
                m.hourly.len(),
                m.hourly.iter().map(|b| b.requests).sum::<u64>(),
                m.hourly.iter().map(|b| b.used).sum::<f64>(),
                busy.join(", ")
            );
        }
    }
}
