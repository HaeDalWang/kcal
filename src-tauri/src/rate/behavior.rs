//! 3층 분해 행동 패턴(docs/decisions/0013): 보통 5시간 창이 무엇으로 찼나, 한도를 많이 쓴 Codex 세션.
//! `compute()`가 읽은 이벤트를 그대로 쓴다(rollout을 다시 읽지 않는다). 세션 이름만 읽기 루프에서 첫 줄을 더 본다.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;
use time::OffsetDateTime;

use super::codex::{self, Event};
use super::{CodexRate, CodexSession, RateStatus, TokenRate, WindowComposition};
use crate::tuning::Tuning;

const SESSION_META_MARK: &[u8] = br#""session_meta""#;
/// `session_meta` 접두 검사 길이(문자). `token_count`와 같은 400.
const PREFIX_CHARS: usize = 400;
/// cwd가 없을 때 이름으로 쓰는 rollout 파일 이름 부분: `rollout-` 뒤 `2026-11-02T23-34`.
const FILE_NAME_PREFIX: &str = "rollout-";
const FILE_NAME_CHARS: usize = 16;

/// 경로의 마지막 폴더 이름. `/`와 `\`를 모두 구분자로 본다(Windows 경로). 루트면 빈 문자열.
fn folder_name(cwd: &str) -> &str {
    cwd.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
}

/// 첫 줄이 `session_meta`면 `payload.cwd`(마지막 폴더 이름이 있는 경로만).
pub(super) fn session_cwd(line: &[u8]) -> Option<String> {
    if !codex::contains(codex::char_prefix(line, PREFIX_CHARS), SESSION_META_MARK) {
        return None;
    }
    let row: Value = serde_json::from_slice(line).ok()?;
    let cwd = row.get("payload")?.get("cwd")?.as_str()?;
    (!folder_name(cwd).is_empty()).then(|| cwd.to_owned())
}

/// cwd를 모를 때의 이름: rollout 파일 이름에서 시작 시각 부분.
fn file_label(file: &str) -> String {
    let rest = file.strip_prefix(FILE_NAME_PREFIX).unwrap_or(file);
    rest.chars().take(FILE_NAME_CHARS).collect()
}

/// 중복을 지운 이벤트와 rollout 파일 이름 → 작업 폴더 경로(`session_meta`에 cwd가 있는 파일만).
pub fn events_and_cwds(files: &[PathBuf]) -> (Vec<Event>, HashMap<String, String>) {
    codex::warm(files);
    let mut raw = Vec::new();
    let mut cwds = HashMap::new();
    for file in files {
        let rollout = codex::read_rollout(file);
        let (events, cwd) = &*rollout;
        if let (Some(first), Some(cwd)) = (events.first(), cwd) {
            cwds.insert(first.file.clone(), cwd.clone());
        }
        raw.extend(events.iter().cloned());
    }
    (codex::dedup(&raw), cwds)
}

/// 1%당 토큰으로 나눈 몫(%). 계수 0(`None`)인 항은 0.
fn per(tokens: f64, rate: Option<f64>) -> f64 {
    rate.map_or(0.0, |r| tokens / r)
}

fn is_miss(e: &Event, cache_miss_ratio: f64) -> bool {
    e.cached / e.input < cache_miss_ratio
}

/// 요청 하나의 몫(%): 캐시 입력 + 비캐시 입력 + 출력. 입력에 캐시가 들어 있다(`window_row`와 같은 정의).
fn request_share(e: &Event, rate: &TokenRate) -> f64 {
    per(e.cached, rate.cached_input)
        + per((e.input - e.cached).max(0.0), rate.uncached_input)
        + per(e.output, rate.output)
}

/// 기간 안 창 중 요청 수 중앙값 창(짝수 개면 아래쪽, 같으면 시간순 앞)의 구성. 창이 없으면 `None`.
pub fn composition(
    requests: &[Event],
    rate: &TokenRate,
    tuning: &Tuning,
) -> Option<WindowComposition> {
    let advanced = &tuning.advanced;
    let (mut windows, _) = codex::assign_windows(requests, advanced.reset_tol_secs as f64);
    if windows.is_empty() {
        return None;
    }
    let count = windows.len();
    // 정렬이 안정적이라 같은 요청 수면 시간순 앞 창이 먼저 온다.
    windows.sort_by_key(Vec::len);
    let window = &windows[(count - 1) / 2];
    let (sums, used_percent) = codex::window_row(window);
    let (uncached, cached, output) = (sums[0], sums[1], sums[2]);
    Some(WindowComposition {
        windows: count,
        requests: window.len(),
        misses: window
            .iter()
            .filter(|e| is_miss(e, advanced.cache_miss_ratio))
            .count(),
        input_share: per(cached, rate.cached_input) + per(uncached, rate.uncached_input),
        output_share: per(output, rate.output),
        used_percent,
    })
}

#[derive(Default)]
struct Totals {
    first_ts: f64,
    requests: usize,
    input: f64,
    misses: usize,
    share: f64,
}

/// 세션(rollout 파일)마다 집계해 몫이 큰 순으로 `tuning.top_sessions`개. 같으면 요청 수, 이름 순.
pub fn sessions(
    requests: &[Event],
    cwds: &HashMap<String, String>,
    rate: &TokenRate,
    tuning: &Tuning,
) -> Vec<CodexSession> {
    let mut totals: HashMap<&str, Totals> = HashMap::new();
    for e in requests {
        let t = totals.entry(e.file.as_str()).or_insert_with(|| Totals {
            first_ts: e.ts,
            ..Totals::default()
        });
        t.first_ts = t.first_ts.min(e.ts);
        t.requests += 1;
        t.input += e.input;
        t.misses += usize::from(is_miss(e, tuning.advanced.cache_miss_ratio));
        t.share += request_share(e, rate);
    }
    let mut out: Vec<CodexSession> = totals
        .into_iter()
        .map(|(file, t)| {
            let avg_input_tokens = t.input / t.requests as f64;
            let cwd = cwds.get(file).cloned();
            CodexSession {
                name: cwd
                    .as_deref()
                    .map_or_else(|| file_label(file), |c| folder_name(c).to_owned()),
                cwd,
                started_at: OffsetDateTime::from_unix_timestamp(t.first_ts as i64)
                    .unwrap_or(OffsetDateTime::UNIX_EPOCH),
                requests: t.requests,
                avg_input_tokens,
                misses: t.misses,
                share: t.share,
                is_marathon: t.requests >= tuning.marathon_requests,
                is_large_context: avg_input_tokens >= tuning.large_context_tokens as f64,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.share
            .total_cmp(&a.share)
            .then(b.requests.cmp(&a.requests))
            .then_with(|| a.name.cmp(&b.name))
    });
    out.truncate(tuning.top_sessions);
    out
}

pub struct Behavior {
    pub composition: Option<WindowComposition>,
    pub sessions: Vec<CodexSession>,
}

/// 환율이 `Estimated`일 때만 몫을 낸다. 기간은 창 패턴과 같은 `tuning.breakdown_days`.
pub fn behavior(
    events: &[Event],
    cwds: &HashMap<String, String>,
    rate: &CodexRate,
    now: f64,
    tuning: &Tuning,
) -> Behavior {
    let per_percent = rate
        .per_percent
        .filter(|_| rate.status == RateStatus::Estimated);
    let Some(per_percent) = per_percent else {
        return Behavior {
            composition: None,
            sessions: Vec::new(),
        };
    };
    let requests = codex::recent_requests(events, now, tuning.breakdown_days);
    Behavior {
        composition: composition(&requests, &per_percent, tuning),
        sessions: sessions(&requests, cwds, &per_percent, tuning),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;

    const DAY: f64 = 86_400.0;
    const NOW: f64 = 100.0 * DAY;
    /// 1%당 캐시 입력 1,000 · 비캐시 입력 100 · 출력 10 토큰.
    const RATE: TokenRate = TokenRate {
        cached_input: Some(1_000.0),
        uncached_input: Some(100.0),
        output: Some(10.0),
    };

    /// 요청 하나: 입력 `input`(그중 캐시 `cached`), 출력 `output`, 창 `resets`, 사용 %.
    fn req(file: &str, ts: f64, input: f64, cached: f64, output: f64, resets: f64) -> Event {
        Event {
            ts,
            tsr: format!("t{ts}"),
            file: file.into(),
            input,
            cached,
            output,
            total_key: format!("{file}{ts}"),
            used_percent: Some(ts / DAY),
            resets_at: Some(resets),
            weekly_used_percent: None,
            weekly_resets_at: None,
        }
    }

    fn estimated() -> CodexRate {
        CodexRate {
            per_percent: Some(RATE),
            ..CodexRate::status(RateStatus::Estimated, 9, 0)
        }
    }

    #[test]
    fn composition_splits_the_median_window_into_input_and_output_shares() {
        let at = NOW - DAY;
        let requests = vec![
            // 창 A(요청 1개), 창 B(요청 2개), 창 C(요청 3개) → 중앙값 창은 B.
            req("a", at, 100.0, 100.0, 1.0, 1e9),
            req("a", at + 1.0, 1_000.0, 900.0, 10.0, 2e9),
            req("b", at + 2.0, 1_000.0, 200.0, 0.0, 2e9),
            req("a", at + 3.0, 10.0, 0.0, 0.0, 3e9),
            req("a", at + 4.0, 10.0, 0.0, 0.0, 3e9),
            req("a", at + 5.0, 10.0, 0.0, 0.0, 3e9),
        ];
        let got = composition(&requests, &RATE, &Tuning::default()).unwrap();
        assert_eq!((got.windows, got.requests, got.misses), (3, 2, 1));
        // 캐시 (900 + 200) / 1,000 + 비캐시 (100 + 800) / 100 = 1.1 + 9 = 10.1
        assert!(
            (got.input_share - 10.1).abs() < 1e-12,
            "{}",
            got.input_share
        );
        assert!((got.output_share - 1.0).abs() < 1e-12);
        assert_eq!(got.used_percent, (at + 2.0) / DAY);
        assert_eq!(composition(&[], &RATE, &Tuning::default()), None);
    }

    #[test]
    fn a_zero_coefficient_drops_its_term() {
        let rate = TokenRate {
            output: None,
            ..RATE
        };
        let e = req("a", NOW, 1_000.0, 500.0, 100.0, 1e9);
        assert!((request_share(&e, &rate) - 5.5).abs() < 1e-12);
        assert!((request_share(&e, &RATE) - 15.5).abs() < 1e-12);
    }

    #[test]
    fn sessions_are_ranked_by_share_and_cut_to_top_sessions() {
        let at = NOW - DAY;
        let mut requests = vec![req("big", at, 100_000.0, 99_000.0, 0.0, 1e9)];
        requests.extend((0..4).map(|i| req("many", at + f64::from(i), 1_000.0, 0.0, 0.0, 1e9)));
        requests.push(req(
            "rollout-2026-11-02T23-34-07-uuid.jsonl",
            at,
            10.0,
            0.0,
            0.0,
            1e9,
        ));
        let cwds = HashMap::from([("big".to_owned(), "/w/proj".to_owned())]);
        let tuning = Tuning {
            top_sessions: 2,
            ..Tuning::default()
        };
        let got = sessions(&requests, &cwds, &RATE, &tuning);
        let summary: Vec<(&str, usize, usize)> = got
            .iter()
            .map(|s| (s.name.as_str(), s.requests, s.misses))
            .collect();
        // big: 99 + 10 = 109%, many: 4 × 10 = 40%, 이름 없는 세션: 0.1% → 잘림.
        assert_eq!(summary, [("proj", 1, 0), ("many", 4, 4)]);
        assert!((got[0].share - 99.0 - 10.0).abs() < 1e-9);
        assert_eq!(got[1].avg_input_tokens, 1_000.0);
        assert_eq!(got[1].started_at.unix_timestamp(), at as i64);
        assert_eq!(got[0].cwd.as_deref(), Some("/w/proj"));
        assert_eq!(got[1].cwd, None);

        let all = sessions(&requests, &cwds, &RATE, &Tuning::default());
        assert_eq!(all[2].name, "2026-11-02T23-34");
    }

    #[test]
    fn marks_marathon_and_large_context_sessions_from_tuning() {
        let at = NOW - DAY;
        let requests: Vec<Event> = (0..3)
            .map(|i| req("a", at + f64::from(i), 2_000.0, 0.0, 0.0, 1e9))
            .chain([req("b", at, 500.0, 0.0, 0.0, 1e9)])
            .collect();
        let tuning = Tuning {
            marathon_requests: 3,
            large_context_tokens: 2_000,
            ..Tuning::default()
        };
        let got = sessions(&requests, &HashMap::new(), &RATE, &tuning);
        let flags: Vec<(&str, bool, bool)> = got
            .iter()
            .map(|s| (s.name.as_str(), s.is_marathon, s.is_large_context))
            .collect();
        assert_eq!(flags, [("a", true, true), ("b", false, false)]);
    }

    #[test]
    fn behavior_is_empty_unless_the_rate_is_estimated() {
        let events = vec![req("a", NOW - DAY, 1_000.0, 0.0, 0.0, 1e9)];
        let cwds = HashMap::new();
        let tuning = Tuning::default();
        let shown = behavior(&events, &cwds, &estimated(), NOW, &tuning);
        assert!(shown.composition.is_some() && shown.sessions.len() == 1);
        for status in [RateStatus::RangeOnly, RateStatus::Insufficient] {
            let rate = CodexRate {
                status,
                ..estimated()
            };
            let hidden = behavior(&events, &cwds, &rate, NOW, &tuning);
            assert!(hidden.composition.is_none() && hidden.sessions.is_empty());
        }
        // 기간 밖 요청만 있으면 창도 세션도 없다.
        let old = behavior(&events, &cwds, &estimated(), NOW + 30.0 * DAY, &tuning);
        assert!(old.composition.is_none() && old.sessions.is_empty());
    }

    #[test]
    fn session_cwd_comes_from_the_first_line_and_name_is_its_last_folder() {
        let meta = |cwd: &str| {
            json!({"type": "session_meta", "payload": {"cwd": cwd}})
                .to_string()
                .into_bytes()
        };
        assert_eq!(
            session_cwd(&meta("/Users/me/work/kcal/")),
            Some("/Users/me/work/kcal/".into())
        );
        assert_eq!(folder_name("/Users/me/work/kcal/"), "kcal");
        assert_eq!(folder_name(r"C:\work\proj"), "proj");
        assert_eq!(
            session_cwd(&meta(r"C:\work\proj")),
            Some(r"C:\work\proj".into())
        );
        assert_eq!(session_cwd(&meta("/")), None);
        assert_eq!(session_cwd(br#"{"type":"event_msg"}"#), None);

        let home = TempDir::new("rate-behavior-cwds");
        let token = json!({"timestamp": "2026-11-04T01:00:00Z", "type": "event_msg",
            "payload": {"type": "token_count",
                "info": {"last_token_usage": {"input_tokens": 5}, "total_token_usage": {"input_tokens": 5}},
                "rate_limits": {"limit_id": "codex", "primary": {"used_percent": 1.0, "resets_at": 1}}}})
        .to_string();
        let text = [String::from_utf8(meta("/w/proj")).unwrap(), token.clone()].join("\n");
        home.write(".codex/sessions/rollout-a.jsonl", &text);
        // cwd 줄이 첫 줄이 아니면 쓰지 않는다.
        home.write(
            ".codex/sessions/rollout-b.jsonl",
            &[token, String::from_utf8(meta("/w/late")).unwrap()].join("\n"),
        );
        let (events, cwds) = events_and_cwds(&codex::rollout_files(home.path()));
        assert_eq!(events.len(), 1, "같은 (timestamp, total)은 하나만 남는다");
        assert_eq!(
            cwds,
            HashMap::from([("rollout-a.jsonl".to_owned(), "/w/proj".to_owned())])
        );
    }

    /// 이 머신의 실제 `~/.codex`로 창 구성·상위 세션을 낸다(이름·숫자만).
    /// `cargo test -- --ignored --nocapture codex_behavior_on_real_rollouts`.
    #[test]
    #[ignore = "실제 ~/.codex를 읽는다"]
    fn codex_behavior_on_real_rollouts() {
        let home = std::env::home_dir().unwrap();
        let tuning = Tuning {
            top_sessions: 10,
            ..Tuning::default()
        };
        let now = OffsetDateTime::now_utc().unix_timestamp() as f64;
        let (events, cwds) = events_and_cwds(&codex::rollout_files(&home));
        let rate = codex::rate(&events, &tuning.advanced);
        println!(
            "BEHAVIOR status={:?} per={:?}",
            rate.status, rate.per_percent
        );
        let got = behavior(&events, &cwds, &rate, now, &tuning);
        println!("BEHAVIOR composition={:?}", got.composition);
        for s in got.sessions {
            println!(
                "BEHAVIOR session {} {} req={} avg_in={:.0} miss={} share={:.1} marathon={} large={}",
                s.name,
                s.started_at.date(),
                s.requests,
                s.avg_input_tokens,
                s.misses,
                s.share,
                s.is_marathon,
                s.is_large_context
            );
        }
    }
}
