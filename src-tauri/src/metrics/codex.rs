//! Codex 지표. 최근 `bar_days`일 안에 바뀐 rollout만 읽는다(rate::codex와 같은 접두 검사·중복 제거).
//! 요청 = 반복 보고가 아니고 입력 > 0인 `token_count`(rate::codex::pattern과 같은 정의).

use std::path::Path;

use super::{
    AgentMetrics, BarUnit, DAY_SECS, HourBucket, current, hourly, hourly_start, modified_secs,
    percent_bars, ratio, weekly_increments, windows,
};
use crate::rate::codex::{self as rollout, Event};
use crate::tuning::Tuning;
use crate::usage::Agent;

pub fn metrics(home: &Path, now: f64, tuning: &Tuning) -> AgentMetrics {
    // 시간 버킷이 막대보다 하루 더 본다.
    let cut = hourly_start(now, tuning.bar_days);
    let files: Vec<_> = rollout::rollout_files(home)
        .into_iter()
        .filter(|f| modified_secs(f).is_some_and(|m| m >= cut))
        .collect();
    from_events(&rollout::events(&files), now, tuning)
}

/// 중복을 지운 이벤트(`rollout::events`)에서 지표를 낸다.
fn from_events(events: &[Event], now: f64, tuning: &Tuning) -> AgentMetrics {
    let cut = now - tuning.bar_days as f64 * DAY_SECS;
    let recent: Vec<&Event> = events
        .iter()
        .filter(|e| e.ts >= cut && e.ts <= now)
        .collect();
    let samples = recent
        .iter()
        .filter_map(|e| Some((e.resets_at?, e.used_percent?)))
        .collect();
    let windows = windows(samples, tuning.advanced.reset_tol_secs as f64);
    let current = current(&windows, now);
    let requests: Vec<&&Event> = current
        .iter()
        .flat_map(|w| {
            recent.iter().filter(move |e| {
                e.input > 0.0
                    && e.used_percent.is_some()
                    && e.resets_at.is_some_and(|r| w.contains(r))
                    && e.ts >= w.start()
            })
        })
        .collect();
    let (cached, input) = requests.iter().fold((0.0, 0.0), |(c, i), e| {
        (c + e.cached.min(e.input), i + e.input)
    });
    AgentMetrics {
        agent: Agent::Codex,
        recent_requests: current.map(|_| requests.len() as u64),
        cache_hit_ratio: current.and_then(|_| ratio(cached, input)),
        bar_unit: BarUnit::Percent,
        bars: percent_bars(&windows, now, tuning.recent_bars),
        hourly: hourly_activity(events, now, tuning),
    }
}

/// 시간 버킷: 요청(반복 보고 아님, 입력 > 0)과 주간(secondary) %p 증가분. 증가분은 기간 앞 표본까지 보고 잰다.
fn hourly_activity(events: &[Event], now: f64, tuning: &Tuning) -> Vec<HourBucket> {
    let requests: Vec<f64> = events
        .iter()
        .filter(|e| e.input > 0.0)
        .map(|e| e.ts)
        .collect();
    let samples = events
        .iter()
        .filter(|e| e.ts <= now)
        .filter_map(|e| Some((e.ts, e.weekly_resets_at?, e.weekly_used_percent?)))
        .collect();
    let used = weekly_increments(samples, tuning.advanced.reset_tol_secs as f64);
    hourly(now, tuning.bar_days, &requests, &used)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = 3600.0;

    fn ev(ts: f64, input: f64, cached: f64, pct: Option<f64>, reset: f64) -> Event {
        Event {
            ts,
            tsr: format!("t{ts}"),
            file: "a".into(),
            input,
            cached,
            output: 1.0,
            total_key: format!("k{ts}"),
            used_percent: pct,
            resets_at: Some(reset),
            weekly_used_percent: None,
            weekly_resets_at: None,
        }
    }

    #[test]
    fn hourly_counts_requests_and_weekly_increments_per_hour() {
        let now = 100.0 * H + 30.0 * 60.0;
        let week = 200.0 * H;
        let weekly = |e: Event, pct: f64| Event {
            weekly_used_percent: Some(pct),
            weekly_resets_at: Some(week),
            ..e
        };
        let events = vec![
            weekly(ev(98.2 * H, 100.0, 0.0, Some(1.0), 103.0 * H), 20.0), // 기준점
            weekly(ev(99.1 * H, 100.0, 0.0, Some(2.0), 103.0 * H), 21.0),
            weekly(ev(99.4 * H, 0.0, 0.0, Some(2.0), 103.0 * H), 21.0), // 반복 보고: 요청 아님
            weekly(ev(100.2 * H, 100.0, 0.0, Some(3.0), 103.0 * H), 23.5),
        ];
        let got = from_events(&events, now, &Tuning::default());
        assert_eq!(got.hourly.len(), 8 * 24);
        let last: Vec<(u64, f64)> = got.hourly[got.hourly.len() - 3..]
            .iter()
            .map(|b| (b.requests, b.used))
            .collect();
        assert_eq!(last, [(1, 0.0), (1, 1.0), (1, 2.5)]);
    }

    #[test]
    fn counts_requests_and_token_cache_hits_in_the_running_window_only() {
        let now = 100.0 * H;
        let (old, live) = (98.0 * H, 103.0 * H);
        let events = vec![
            ev(94.0 * H, 100.0, 50.0, Some(40.0), old), // 지난 구간
            ev(99.0 * H, 100.0, 90.0, Some(2.0), live), // 지금 구간
            ev(99.5 * H, 300.0, 0.0, Some(5.0), live + 10.0), // 흔들림 10초 → 같은 구간
            ev(99.6 * H, 0.0, 0.0, Some(5.0), live),    // 입력 0(반복 보고) → 요청 아님
            ev(99.7 * H, 100.0, 100.0, None, live),     // used_percent 없음 → 요청 아님
            ev(101.0 * H, 100.0, 100.0, Some(9.0), live), // 미래 → 무시
        ];
        let got = from_events(&events, now, &Tuning::default());
        assert_eq!(got.recent_requests, Some(2));
        assert_eq!(got.cache_hit_ratio, Some(90.0 / 400.0));
        let bars: Vec<(f64, bool)> = got.bars.iter().map(|b| (b.value, b.is_current)).collect();
        assert_eq!(bars, [(40.0, false), (5.0, true)]);
    }

    #[test]
    fn hides_requests_when_no_window_is_running() {
        let now = 100.0 * H;
        let events = vec![ev(94.0 * H, 100.0, 50.0, Some(40.0), 98.0 * H)];
        let got = from_events(&events, now, &Tuning::default());
        assert_eq!((got.recent_requests, got.cache_hit_ratio), (None, None));
        assert_eq!(got.bars.len(), 1);
        let few = Tuning {
            bar_days: 1,
            ..Tuning::default()
        };
        // 10일 전 이벤트는 막대 기간(1일) 밖이다.
        let stale = vec![ev(now - 240.0 * H, 1.0, 1.0, Some(1.0), now - 238.0 * H)];
        assert!(from_events(&stale, now, &few).bars.is_empty());
    }
}
