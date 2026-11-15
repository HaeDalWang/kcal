//! 한도 조회 타이머 하나(0003 "주기 측정"). 기본 Claude·Codex 5분, Kiro 10분(settings tuning).
//! 429면 `Retry-After`, 없으면 간격을 두 배씩 최대 60분(tuning `maxBackoffSecs`)까지 늘린다.
//! 간격은 조회 때마다 현재 settings에서 읽는다. 바꾼 값은 다음 조회의 예약부터 적용된다.
//! 설정에서 끈 에이전트는 조회하지 않고, 다시 켜면 바로 1회 조회한 뒤 기존 주기로 돌아간다.
//! 새로고침(`request_refresh`)은 타이머를 깨워 켠 에이전트를 바로 조회한다. 직전 조회가 429였던 에이전트는
//! 늘어난 간격을 그대로 지키고, 직전 조회 뒤 tuning `refreshMinGapSecs`(기본 30초)가 안 지났으면 그때까지 미룬다.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};
use time::OffsetDateTime;
use tokio::time::Instant;

use super::parse::Fail;
use super::sources::{self, Context, Outcome, RateLimit};
use super::{Agent, ReasonKind, USAGE_UPDATED_EVENT, UsageSnapshot, UsageState};
use crate::i18n::pick;
use crate::settings::SettingsState;
use crate::tuning::Tuning;

const AGENTS: usize = Agent::ALL.len();

/// 새로고침 요청 표시. 타이머가 다음에 깰 때 한 번 소비한다. 둘째 칸이 켜져 있으면 최소 간격을 건너뛴다.
#[derive(Default)]
pub struct RefreshRequest(AtomicBool, AtomicBool);

/// 사용량을 바로 다시 조회하게 한다(`refresh_all`). 결과는 기존 `usage-updated` event로 간다.
pub fn request_refresh<R: Runtime>(app: &AppHandle<R>) {
    wake(app, false);
}

/// 화면 언어가 바뀌어 이유 문구를 새 언어로 다시 만들어야 할 때. 최소 간격을 지키지 않는다:
/// 첫 실행에서 앞선 조회(Rust 기본 영어)와 프론트엔드의 언어 알림이 몇 초 안에 잇따라 오고, 간격을 지키면
/// 한국어 화면이 최대 30초 동안 영어 이유를 보인다. 429를 받은 에이전트는 여전히 건드리지 않는다.
pub fn request_language_refresh<R: Runtime>(app: &AppHandle<R>) {
    wake(app, true);
}

fn wake<R: Runtime>(app: &AppHandle<R>, is_gap_skipped: bool) {
    if let Some(request) = app.try_state::<RefreshRequest>() {
        if is_gap_skipped {
            request.1.store(true, Ordering::SeqCst);
        }
        request.0.store(true, Ordering::SeqCst);
    }
    app.state::<SettingsState>().changed.notify_one();
}

/// 새로고침 뒤 예정 시각. 429를 받은 에이전트는 그대로, 나머지는 지금(직전 조회 + 최소 간격 이후).
fn refreshed_due(
    due: [Instant; AGENTS],
    last_fetch: [Option<Instant>; AGENTS],
    is_rate_limited: [bool; AGENTS],
    now: Instant,
    min_gap: Duration,
) -> [Instant; AGENTS] {
    std::array::from_fn(|i| {
        if is_rate_limited[i] {
            return due[i];
        }
        let earliest = last_fetch[i].map_or(now, |at| (at + min_gap).max(now));
        due[i].min(earliest)
    })
}

fn base_interval(agent: Agent, tuning: &Tuning) -> Duration {
    match agent {
        Agent::Claude | Agent::Codex => tuning.api_interval(),
        Agent::Kiro => tuning.kiro_interval(),
    }
}

/// 다음 조회까지의 간격. `current`는 직전 간격. 429 뒤 늘어난 간격은 `max_backoff`까지이되
/// 기본 간격보다 짧아지지 않는다.
pub fn next_delay(
    base: Duration,
    current: Duration,
    rate_limit: Option<RateLimit>,
    max_backoff: Duration,
) -> Duration {
    match rate_limit {
        None => base,
        // 서버가 준 값을 따르되 기본 간격보다 자주 부르지 않는다.
        Some(RateLimit {
            retry_after_secs: Some(seconds),
        }) => Duration::from_secs(seconds).max(base),
        Some(RateLimit {
            retry_after_secs: None,
        }) => current.saturating_mul(2).min(max_backoff).max(base),
    }
}

/// 이번 회차에 조회할 에이전트 index와 갱신된 예정 시각.
/// 꺼진 에이전트는 제외하고, 직전 회차에 꺼져 있다가 켜진 에이전트는 지금 조회한다.
fn plan(
    enabled: [bool; AGENTS],
    was_enabled: [bool; AGENTS],
    due: [Instant; AGENTS],
    now: Instant,
) -> ([Instant; AGENTS], Vec<usize>) {
    let due: [Instant; AGENTS] = std::array::from_fn(|i| {
        if enabled[i] && !was_enabled[i] {
            now
        } else {
            due[i]
        }
    });
    let running = (0..AGENTS)
        .filter(|&i| enabled[i] && due[i] <= now)
        .collect();
    (due, running)
}

/// 다음에 깰 시각. 켜진 에이전트의 가장 이른 예정 시각.
/// 켜진 에이전트가 없으면 `idle`만큼 뒤(설정이 바뀌면 그 전에 깬다).
fn next_wake(
    enabled: [bool; AGENTS],
    due: [Instant; AGENTS],
    now: Instant,
    idle: Duration,
) -> Instant {
    (0..AGENTS)
        .filter(|&i| enabled[i])
        .map(|i| due[i])
        .min()
        .unwrap_or(now + idle)
}

/// 켜진 에이전트와 현재 tuning. 설정 잠금이 깨졌으면 모두 켜고 기본값.
fn current_settings<R: Runtime>(app: &AppHandle<R>) -> ([bool; AGENTS], Tuning) {
    match app.state::<SettingsState>().get() {
        Ok(settings) => (Agent::ALL.map(|a| settings.is_enabled(a)), settings.tuning),
        Err(_) => ([true; AGENTS], Tuning::default()),
    }
}

/// 타이머를 시작한다. 결과마다 state를 갱신하고 `usage-updated` event와 `on_update`(트레이)를 부른다.
/// `SettingsState`가 먼저 manage돼 있어야 한다.
pub fn start<R: Runtime>(app: AppHandle<R>, on_update: fn(&AppHandle<R>, &UsageSnapshot)) {
    let home = std::env::home_dir().unwrap_or_default();
    let ctx = Context::new(home);
    let changed = app.state::<SettingsState>().changed.clone();
    app.manage(RefreshRequest::default());
    tauri::async_runtime::spawn(async move {
        let (_, first) = current_settings(&app);
        let mut delay = Agent::ALL.map(|agent| base_interval(agent, &first));
        let mut due = [Instant::now(); AGENTS];
        let mut was_enabled = [false; AGENTS];
        let mut last_fetch = [None; AGENTS];
        let mut is_rate_limited = [false; AGENTS];
        loop {
            let (enabled, tuning) = current_settings(&app);
            let request = app.state::<RefreshRequest>();
            if request.0.swap(false, Ordering::SeqCst) {
                let gap = if request.1.swap(false, Ordering::SeqCst) {
                    Duration::ZERO
                } else {
                    tuning.refresh_min_gap()
                };
                due = refreshed_due(due, last_fetch, is_rate_limited, Instant::now(), gap);
            }
            let (next_due, due_now) = plan(enabled, was_enabled, due, Instant::now());
            (due, was_enabled) = (next_due, enabled);
            let running: Vec<_> = due_now
                .into_iter()
                .map(|i| {
                    last_fetch[i] = Some(Instant::now());
                    let (agent, ctx, tuning) = (Agent::ALL[i], ctx.clone(), tuning);
                    (
                        i,
                        tauri::async_runtime::spawn(async move {
                            sources::fetch(agent, &ctx, &tuning).await
                        }),
                    )
                })
                .collect();
            for (i, handle) in running {
                let agent = Agent::ALL[i];
                let outcome = handle.await.unwrap_or_else(|_| {
                    Outcome::failed(
                        agent,
                        Fail(
                            pick("조회 중 내부 오류", "internal error while checking").to_owned(),
                            ReasonKind::Failure,
                        ),
                    )
                });
                is_rate_limited[i] = outcome.rate_limit.is_some();
                // ponytail: 간격을 줄여도 이미 잡힌 예정 시각은 그대로다(다음 조회부터 새 간격). 바로 당기려면 `due`를 새 간격으로 자른다.
                delay[i] = next_delay(
                    base_interval(agent, &tuning),
                    delay[i],
                    outcome.rate_limit,
                    tuning.max_backoff(),
                );
                due[i] = Instant::now() + delay[i];
                crate::rate::record_claude(&app, &outcome.usage);
                let state = app.state::<UsageState>();
                match state.update(outcome.usage, OffsetDateTime::now_utc()) {
                    Ok(snapshot) => {
                        on_update(&app, &snapshot);
                        if app.emit(USAGE_UPDATED_EVENT, &snapshot).is_err() {
                            eprintln!("kcal: {USAGE_UPDATED_EVENT} event 전송 실패");
                        }
                    }
                    Err(_) => eprintln!("kcal: 한도 state 잠금 실패"),
                }
            }
            // 예정 시각까지 자되, 설정이 바뀌면 바로 깬다.
            let wake = next_wake(enabled, due, Instant::now(), tuning.api_interval());
            let _ = tokio::time::timeout_at(wake, changed.notified()).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);
    const API: Duration = Duration::from_secs(5 * 60);
    const HOUR: Duration = Duration::from_secs(60 * 60);
    const GAP: Duration = Duration::from_secs(30);

    fn limited(seconds: Option<u64>) -> Option<RateLimit> {
        Some(RateLimit {
            retry_after_secs: seconds,
        })
    }

    #[test]
    fn skips_disabled_agents() {
        let now = Instant::now();
        let all = [true; AGENTS];
        let (_, running) = plan([true, true, false], all, [now; AGENTS], now);
        assert_eq!(running, [0, 1]);
        // 꺼진 에이전트는 다음 깰 시각에도 영향을 주지 않는다.
        let due = [now + 5 * MIN, now + 5 * MIN, now + MIN];
        assert_eq!(
            next_wake([true, true, false], due, now, 5 * MIN),
            now + 5 * MIN
        );
    }

    #[test]
    fn fetches_re_enabled_agent_immediately() {
        let now = Instant::now();
        let due = [now + 3 * MIN, now + 3 * MIN, now + 9 * MIN];
        let (due, running) = plan([true; AGENTS], [true, true, false], due, now);
        assert_eq!(running, [2]);
        assert_eq!(due[2], now);
        // 계속 켜져 있던 에이전트는 기존 주기를 지킨다.
        let (_, running) = plan([true; AGENTS], [true; AGENTS], [now + MIN; AGENTS], now);
        assert!(running.is_empty());
    }

    #[test]
    fn uses_base_interval_when_not_rate_limited() {
        assert_eq!(next_delay(API, 40 * MIN, None, HOUR), API);
        let defaults = Tuning::default();
        assert_eq!(base_interval(Agent::Claude, &defaults), 5 * MIN);
        assert_eq!(base_interval(Agent::Kiro, &defaults), 10 * MIN);
    }

    #[test]
    fn base_interval_and_backoff_cap_follow_tuning() {
        let tuning = Tuning {
            api_interval_secs: 2 * 60,
            kiro_interval_secs: 20 * 60,
            max_backoff_secs: 15 * 60,
            ..Tuning::default()
        };
        assert_eq!(base_interval(Agent::Codex, &tuning), 2 * MIN);
        assert_eq!(base_interval(Agent::Kiro, &tuning), 20 * MIN);
        let mut delay = tuning.api_interval();
        let mut seen = Vec::new();
        for _ in 0..5 {
            delay = next_delay(
                tuning.api_interval(),
                delay,
                limited(None),
                tuning.max_backoff(),
            );
            seen.push(delay.as_secs() / 60);
        }
        assert_eq!(seen, [4, 8, 15, 15, 15]);
        // 상한이 기본 간격보다 작아도 기본 간격 아래로 내려가지 않는다.
        assert_eq!(next_delay(API, API, limited(None), MIN), API);
    }

    #[test]
    fn doubles_interval_up_to_sixty_minutes_without_retry_after() {
        let mut delay = API;
        let mut seen = Vec::new();
        for _ in 0..6 {
            delay = next_delay(API, delay, limited(None), HOUR);
            seen.push(delay.as_secs() / 60);
        }
        assert_eq!(seen, [10, 20, 40, 60, 60, 60]);
    }

    #[test]
    fn refresh_fetches_now_except_rate_limited_or_just_fetched_agents() {
        let now = Instant::now();
        let due = [now + 5 * MIN; AGENTS];
        let last = [Some(now - MIN), Some(now - Duration::from_secs(10)), None];
        let got = refreshed_due(due, last, [false; AGENTS], now, GAP);
        // 1분 전 조회 → 지금. 10초 전 조회 → 20초 뒤. 조회한 적 없음 → 지금.
        assert_eq!(got, [now, now + Duration::from_secs(20), now]);
        // 429 뒤 늘어난 간격은 새로고침해도 그대로다.
        let got = refreshed_due(due, [None; AGENTS], [true, false, false], now, GAP);
        assert_eq!(got, [now + 5 * MIN, now, now]);
        // 이미 더 이른 예정 시각은 늦추지 않는다.
        let got = refreshed_due([now; AGENTS], last, [false; AGENTS], now, GAP);
        assert_eq!(got, [now; AGENTS]);
        // 최소 간격은 tuning 값을 따른다.
        let got = refreshed_due(due, last, [false; AGENTS], now, 2 * MIN);
        assert_eq!(got, [now + MIN, now + Duration::from_secs(110), now]);
    }

    #[test]
    fn language_refresh_with_zero_gap_refetches_just_fetched_agents_but_not_rate_limited() {
        let now = Instant::now();
        let due = [now + 5 * MIN; AGENTS];
        let last = [Some(now - Duration::from_secs(1)); AGENTS];
        let got = refreshed_due(due, last, [false, true, false], now, Duration::ZERO);
        assert_eq!(got, [now, now + 5 * MIN, now]);
    }

    #[test]
    fn follows_retry_after_but_not_below_base_interval() {
        assert_eq!(next_delay(API, API, limited(Some(7200)), HOUR), 120 * MIN);
        assert_eq!(next_delay(API, API, limited(Some(30)), HOUR), API);
    }
}
