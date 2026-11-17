use super::*;
use crate::context::fsutil::testing::TempDir;
use serde_json::json;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const TOL: f64 = 600.0;
const HOUR: f64 = 3600.0;

fn rfc(secs: f64) -> String {
    OffsetDateTime::from_unix_timestamp_nanos((secs * 1e9) as i128)
        .unwrap()
        .format(&Rfc3339)
        .unwrap()
}

fn assistant(ts: f64, id: &str, input: u64, read: u64, write: u64, output: u64) -> String {
    json!({"type": "assistant", "timestamp": rfc(ts), "message": {"id": id,
        "content": [{"type": "text", "text": "secret body"}],
        "usage": {"input_tokens": input, "cache_read_input_tokens": read,
                  "cache_creation_input_tokens": write, "output_tokens": output}}})
    .to_string()
}

fn record(at: f64, pct: f64, reset: f64) -> String {
    format!(
        r#"{{"at":"{}","fiveHour":{{"usedPercent":{pct},"resetsAt":"{}"}},"sevenDay":null}}"#,
        rfc(at),
        rfc(reset)
    )
}

fn req(ts: f64, input: f64, read: f64, write: f64, output: f64) -> Request {
    Request {
        ts,
        input,
        cache_read: read,
        cache_write: write,
        output,
    }
}

fn window(start: f64, reset: f64, until: f64, used_percent: f64) -> LimitWindow {
    LimitWindow {
        start,
        reset,
        until,
        used_percent,
    }
}

#[test]
fn keeps_the_line_with_the_largest_output_per_message_id() {
    let home = TempDir::new("rate-claude-requests");
    let lines = [
        assistant(10.0, "m1", 3, 100, 20, 1), // 스트리밍 중간 줄
        assistant(11.0, "m1", 3, 100, 20, 9),
        assistant(12.0, "m1", 3, 100, 20, 9), // 같은 출력이면 먼저 읽은 줄
        assistant(5.0, "m2", 1, 0, 50, 4),
        json!({"type": "user", "timestamp": rfc(6.0),
               "message": {"content": "\"type\":\"assistant\" \"usage\""}})
        .to_string(),
        r#"{"type":"assistant","usage" broken"#.to_owned(),
    ];
    home.write(".claude/projects/-w/s.jsonl", &lines.join("\n"));
    let got = requests(&files(home.path()));
    assert_eq!(
        got,
        [
            req(5.0, 1.0, 0.0, 50.0, 4.0),
            req(11.0, 3.0, 100.0, 20.0, 9.0)
        ]
    );
}

#[test]
fn windows_end_when_the_max_percent_was_first_recorded_and_never_overlap_the_previous_window() {
    let home = TempDir::new("rate-claude-windows");
    let first = 100.0 * HOUR;
    // 두 번째 창의 리셋이 첫 창 리셋 4시간 뒤(한도가 일찍 초기화된 경우) → 시작은 첫 창 리셋.
    let second = first + 4.0 * HOUR;
    let history = home.write(
        "data/limit-history.jsonl",
        &[
            record(first - 3.0 * HOUR, 10.0, first),
            record(first - 2.0 * HOUR, 30.0, first + 1.0), // 리셋 흔들림 1초 → 같은 창
            record(first - 1.0 * HOUR, 30.0, first),       // 같은 최대값: 먼저 기록한 시각이 끝
            record(second - 1.0 * HOUR, 5.0, second),
        ]
        .join("\n"),
    );
    let got = windows(&history, TOL);
    assert_eq!(
        got,
        [
            window(first - 5.0 * HOUR, first, first - 2.0 * HOUR, 30.0),
            window(first, second, second - 1.0 * HOUR, 5.0),
        ]
    );
    assert!(windows(&home.path().join("none.jsonl"), TOL).is_empty());
}

/// 1%p ≈ 캐시 입력 2M · 비캐시 입력(캐시 쓰기 포함) 40K · 출력 5K인 합성 창.
#[test]
fn recovers_a_known_rate_and_excludes_windows_without_logged_requests() {
    let truth = [1.0 / 40_000.0, 1.0 / 2_000_000.0, 1.0 / 5_000.0];
    let mut seed: u64 = 11;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut requests = Vec::new();
    let mut wins = Vec::new();
    for i in 0..12 {
        let start = i as f64 * 5.0 * HOUR;
        let (write, read, output) = (1e4 + next() * 6e5, next() * 3e7, next() * 2e5);
        // 비캐시 입력 100 + 캐시 쓰기 → 같은 변수.
        requests.push(req(start + HOUR, 100.0, read, write - 100.0, output));
        let y = truth[0] * write + truth[1] * read + truth[2] * output;
        wins.push(window(start, start + 5.0 * HOUR, start + 2.0 * HOUR, y));
    }
    // 기록된 요청이 없는 창(다른 기기·웹 사용) 하나, 끝(최대 % 기록) 뒤 요청 하나.
    wins.push(window(60.0 * HOUR, 65.0 * HOUR, 64.0 * HOUR, 40.0));
    requests.push(req(3.0 * HOUR, 0.0, 9e9, 0.0, 9e9));
    requests.sort_by(|a, b| a.ts.total_cmp(&b.ts));

    let got = rate(&requests, &wins, &AdvancedTuning::default());
    assert_eq!(got.status, RateStatus::Estimated);
    assert_eq!((got.windows, got.excluded_windows), (12, 1));
    assert!(got.r2.unwrap() > 0.999);
    let per = got.per_percent.unwrap();
    for (value, want) in [
        (per.uncached_input, 40_000.0),
        (per.cached_input, 2_000_000.0),
        (per.output, 5_000.0),
    ] {
        assert!(
            (value.unwrap() / want - 1.0).abs() < 1e-6,
            "{value:?} vs {want}"
        );
    }

    // 창 9개 미만(변수 3 × 3)이면 회귀하지 않는다. 요청이 하나도 없으면 기록 없음.
    let few = rate(&requests, &wins[..8], &AdvancedTuning::default());
    assert_eq!((few.status, few.windows), (RateStatus::Insufficient, 8));
    let none = rate(&[], &wins, &AdvancedTuning::default());
    assert_eq!(
        (none.status, none.excluded_windows),
        (RateStatus::NoData, 13)
    );
}

#[test]
fn pattern_counts_recent_requests_and_cache_misses_per_window() {
    let day = 86_400.0;
    let now = 30.0 * day;
    let wins = [
        window(0.0, 5.0 * HOUR, 5.0 * HOUR, 10.0), // 기간 밖
        window(now - 10.0 * HOUR, now - 5.0 * HOUR, now - 6.0 * HOUR, 20.0),
        window(now - 5.0 * HOUR, now, now - HOUR, 30.0),
    ];
    let requests = [
        req(HOUR, 1.0, 90.0, 9.0, 1.0),
        req(now - 9.0 * HOUR, 1.0, 90.0, 9.0, 1.0),
        req(now - 8.0 * HOUR, 1.0, 10.0, 89.0, 1.0), // 미스
        req(now - 7.0 * HOUR, 0.0, 0.0, 0.0, 5.0),   // 입력 0 → 요청 아님
        req(now - 2.0 * HOUR, 1.0, 0.0, 99.0, 1.0),  // 미스
    ];
    let got = pattern(&requests, &wins, now, 14, &AdvancedTuning::default()).unwrap();
    assert_eq!(got.windows, 2);
    assert_eq!((got.requests.median, got.requests.p90), (1.5, 1.9));
    assert_eq!((got.misses.median, got.misses.p90), (1.0, 1.0));
    assert_eq!(
        pattern(
            &requests,
            &wins,
            now + 30.0 * day,
            14,
            &AdvancedTuning::default()
        ),
        None
    );
}

/// 이 머신의 실제 `~/.claude/projects`와 한도 기록으로 패리티 숫자를 낸다(숫자만).
/// `KCAL_PARITY_NOW`(유닉스 초)는 창 패턴 기준 시각. `cargo test -- --ignored --nocapture claude_parity`.
#[test]
#[ignore = "실제 ~/.claude/projects와 한도 기록을 읽는다(읽기만)"]
fn claude_parity_on_real_logs() {
    let home = std::env::home_dir().unwrap();
    let history = home
        .join("Library/Application Support/io.github.haedalwang.kcal")
        .join(crate::rate::history::FILE_NAME);
    let tuning = AdvancedTuning::default();
    let requests = requests(&files(&home));
    let wins = windows(&history, tuning.reset_tol_secs as f64);
    let shown = rate(&requests, &wins, &tuning);
    println!(
        "PARITY requests={} windows={} used={} excluded={}",
        requests.len(),
        wins.len(),
        shown.windows,
        shown.excluded_windows
    );
    let (rows, y): (Vec<Vec<f64>>, Vec<f64>) = wins
        .iter()
        .filter_map(|w| Some((window_row(&requests, w)?, w.used_percent)))
        .unzip();
    let groups: Vec<usize> = (0..rows.len()).collect();
    let fit = fit::fit_eval(&rows, &y, &groups, tuning.holdout_train).expect("fit");
    println!(
        "PARITY train={:.6} loo={:.6} holdout={:.6} val={:.6}",
        fit.train,
        fit.loo,
        fit.holdout,
        fit.validation()
    );
    let names = ["uncached_in", "cache_read", "output"];
    for (pos, &var) in fit.used.iter().enumerate() {
        let (lo, hi) = fit.fold_range[pos];
        println!(
            "PARITY coef {}={:e} fold_lo={:e} fold_hi={:e}",
            names[var], fit.coef[pos], lo, hi
        );
    }
    println!(
        "PARITY get_rates status={:?} r2={:?} per_percent={:?}",
        shown.status, shown.r2, shown.per_percent
    );
    if let Ok(now) = std::env::var("KCAL_PARITY_NOW") {
        let got = pattern(&requests, &wins, now.parse().unwrap(), 14, &tuning).expect("pattern");
        println!(
            "PATTERN windows={} n_median={} n_p90={} m_median={} m_p90={}",
            got.windows, got.requests.median, got.requests.p90, got.misses.median, got.misses.p90
        );
    }
}

/// 같은 Claude 기록을 읽는 네 곳(환율·지표·검진 전후·점검)을 이 Mac 실제 HOME에서 두 번씩 잰다(읽기만, 숫자만).
/// `cargo test --release -- --ignored real_claude_read_cost --nocapture`.
#[test]
#[ignore = "실제 ~/.claude/projects를 읽는다"]
fn real_claude_read_cost() {
    use std::time::Instant;
    let home = std::env::home_dir().unwrap();
    let history = crate::rate::history::file(
        &home.join("Library/Application Support/io.github.haedalwang.kcal"),
    );
    let now = unix_secs(OffsetDateTime::now_utc());
    let tuning = crate::tuning::Tuning::default();
    let rss_kb = || -> u64 {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    };
    println!("MEMO before rss={}KB", rss_kb());
    for pass in ["cold", "warm"] {
        let total = Instant::now();
        let t = Instant::now();
        let rate = requests(&files(&home));
        let rate_ms = t.elapsed().as_millis();
        let t = Instant::now();
        let m = crate::metrics::claude::metrics(&home, &history, now, &tuning);
        let metrics_ms = t.elapsed().as_millis();
        let t = Instant::now();
        let (inputs, _) =
            crate::metrics::claude::request_inputs(&home, now - 14.0 * crate::metrics::DAY_SECS);
        let health_ms = t.elapsed().as_millis();
        let t = Instant::now();
        let (models, cwds) =
            crate::metrics::claude::model_requests(&home, now - 30.0 * crate::metrics::DAY_SECS);
        let checks_ms = t.elapsed().as_millis();
        println!(
            "BENCH {pass} rate={rate_ms}ms({}) metrics={metrics_ms}ms({:?}) health={health_ms}ms({}) checks={checks_ms}ms({}/{}) total={}ms",
            rate.len(),
            m.recent_requests,
            inputs.len(),
            models.len(),
            cwds.len(),
            total.elapsed().as_millis()
        );
        let (files, n, bytes) = crate::metrics::claude::memo_stats();
        println!(
            "MEMO {pass} rss={}KB files={files} entries={n} est={bytes}B per_entry={}B",
            rss_kb(),
            bytes / n.max(1)
        );
    }
}

#[test]
fn log_files_drops_memo_of_deleted_files() {
    let home = TempDir::new("rate-claude-prune");
    let file = home.write(
        ".claude/projects/-w/gone.jsonl",
        &assistant(1_000.0, "m1", 3, 0, 0, 4),
    );
    let path = file.parent().unwrap().join("gone.jsonl");
    let memoed = |p: &Path| crate::metrics::claude::memo_has(p);
    assert_eq!(requests(&files(home.path())).len(), 1);
    assert!(memoed(&path));
    std::fs::remove_file(&path).unwrap();
    files(home.path());
    assert!(!memoed(&path));
}

#[test]
fn rate_requests_reuse_the_parse_other_readers_made() {
    let home = TempDir::new("rate-claude-once");
    let line = assistant(1_000.0, "m1", 3, 0, 0, 4);
    let file = home.write(".claude/projects/-w/s.jsonl", &line);
    let path = file.parent().unwrap().join("s.jsonl");
    let (inputs, _) = crate::metrics::claude::request_inputs(home.path(), 0.0);
    let got = requests(&files(home.path()));
    assert_eq!((inputs.len(), got.len(), got[0].output), (1, 1, 4.0));
    let parses = |f: &Path| {
        crate::metrics::claude::PARSE_COUNTS
            .lock()
            .unwrap()
            .get(f)
            .copied()
    };
    assert_eq!(parses(&path).or(parses(&file)), Some(1));
}
