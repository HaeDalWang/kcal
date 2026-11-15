//! Kiro 지표. 5시간 한도가 없어 최근 5시간(지금 기준)의 질문(턴) 수와, 지금부터 거꾸로 자른 하루(24시간)
//! 막대 `bar_days`개의 크레딧 합을 낸다. 턴은 rate::kiro와 같이 `usage_summary`의 `unit == "credit"`만 센다.
//! 기록에 캐시 정보가 없어 캐시 적중률은 늘 `None`.

use std::path::Path;

use super::{
    AgentMetrics, BarUnit, DAY_SECS, FIVE_HOURS_SECS, UsageBar, at, hourly, hourly_start,
    modified_secs,
};
use crate::context::kiro_session;
use crate::rate::kiro::{TimedCredit, timed_credits};
use crate::tuning::Tuning;
use crate::usage::Agent;

pub fn metrics(home: &Path, now: f64, tuning: &Tuning) -> AgentMetrics {
    // 시간 버킷이 막대보다 하루 더 본다.
    let cut = hourly_start(now, tuning.bar_days);
    // 최근에 바뀐 순이라 기간 전에 바뀐 파일이 나오면 그 뒤는 볼 필요가 없다.
    let turns: Vec<TimedCredit> = kiro_session::message_files(home)
        .into_iter()
        .take_while(|f| modified_secs(f).is_some_and(|m| m >= cut))
        .flat_map(|f| timed_credits(&f))
        .collect();
    from_turns(&turns, now, tuning.bar_days)
}

fn from_turns(turns: &[TimedCredit], now: f64, days: u64) -> AgentMetrics {
    let timed: Vec<(f64, f64)> = turns
        .iter()
        .filter_map(|&(ts, credit)| Some((ts?, credit)))
        .filter(|(ts, _)| *ts <= now)
        .collect();
    let recent = timed
        .iter()
        .filter(|(ts, _)| *ts >= now - FIVE_HOURS_SECS)
        .count();
    let bars = (0..days)
        .map(|i| {
            let start = now - (days - i) as f64 * DAY_SECS;
            let end = start + DAY_SECS;
            let is_current = i + 1 == days;
            // 빈 합은 -0.0이라 화면에 "-0"이 보인다. +0.0을 더해 0으로 맞춘다.
            let value = timed
                .iter()
                .filter(|(ts, _)| *ts >= start && (*ts < end || is_current))
                .map(|(_, credit)| credit)
                .sum::<f64>()
                + 0.0;
            UsageBar {
                starts_at: at(start),
                value,
                is_current,
            }
        })
        .collect();
    AgentMetrics {
        agent: Agent::Kiro,
        recent_requests: Some(recent as u64),
        cache_hit_ratio: None,
        bar_unit: BarUnit::Credits,
        bars,
        hourly: hourly(
            now,
            days,
            &timed.iter().map(|(ts, _)| *ts).collect::<Vec<_>>(),
            &timed,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = 3600.0;

    #[test]
    fn counts_turns_in_the_last_five_hours_and_sums_credits_per_rolling_day() {
        let now = 100.0 * DAY_SECS;
        let turns = [
            (Some(now - H), 0.5),
            (Some(now - 4.9 * H), 0.25),
            (Some(now - 6.0 * H), 1.0),      // 5시간 밖, 오늘 막대 안
            (Some(now - DAY_SECS - H), 2.0), // 어제 막대
            (Some(now - 3.0 * DAY_SECS - H), 4.0), // 기간(3일) 밖
            (Some(now + H), 8.0),            // 미래 → 무시
            (None, 16.0),                    // 시각 없음 → 무시
        ];
        let got = from_turns(&turns, now, 3);
        assert_eq!(got.recent_requests, Some(2));
        assert_eq!(got.cache_hit_ratio, None);
        let bars: Vec<(f64, bool)> = got.bars.iter().map(|b| (b.value, b.is_current)).collect();
        assert_eq!(bars, [(0.0, false), (2.0, false), (1.75, true)]);
        assert_eq!(got.bars[0].starts_at, at(now - 3.0 * DAY_SECS));
        // 시간 버킷(3 + 1일): 질문 수와 크레딧 합. 4일 넘은 턴·미래·시각 없음은 빠진다.
        assert_eq!(got.hourly.len(), 4 * 24);
        let questions: u64 = got.hourly.iter().map(|b| b.requests).sum();
        let credits: f64 = got.hourly.iter().map(|b| b.used).sum();
        assert_eq!((questions, credits), (5, 7.75));
        // 지금이 정시라 마지막 버킷(지금 시간)은 비고, 한 시간 전 턴은 그 앞 버킷에 있다.
        let hours: Vec<(u64, f64)> = got.hourly[94..]
            .iter()
            .map(|b| (b.requests, b.used))
            .collect();
        assert_eq!(hours, [(1, 0.5), (0, 0.0)]);
    }

    #[test]
    fn days_without_credits_are_plus_zero_not_minus_zero() {
        let got = from_turns(&[], 100.0 * DAY_SECS, 3);
        assert!(
            got.bars
                .iter()
                .all(|b| b.value == 0.0 && b.value.is_sign_positive())
        );
    }
}
