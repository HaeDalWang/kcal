use super::*;
use crate::context::fsutil::testing::TempDir;
use serde_json::json;

fn ev(ts: f64, file: &str, total: &str, used: Option<f64>, resets: Option<f64>) -> Event {
    Event {
        ts,
        tsr: format!("t{ts}"),
        file: file.into(),
        input: 10.0,
        cached: 4.0,
        output: 2.0,
        total_key: total.into(),
        used_percent: used,
        resets_at: resets,
        weekly_used_percent: None,
        weekly_resets_at: None,
    }
}

fn token_line(ts: &str, input: u64, total: u64, limit_id: &str, pct: f64, resets: u64) -> String {
    json!({"timestamp": ts, "type": "event_msg", "payload": {"type": "token_count",
        "info": {"last_token_usage": {"input_tokens": input, "cached_input_tokens": 1, "output_tokens": 2},
                 "total_token_usage": {"input_tokens": total}},
        "rate_limits": {"limit_id": limit_id,
            "primary": {"used_percent": pct, "window_minutes": 300, "resets_at": resets}}}})
    .to_string()
}

#[test]
fn reads_only_codex_token_counts_without_parsing_other_lines() {
    let home = TempDir::new("rate-codex-read");
    // 깨진 줄은 파싱했다면 실패한다. 400자 뒤에야 표시가 나오는 줄은 파싱하지 않는다.
    let late = format!("{{\"pad\":\"{}\",\"type\":\"token_count\"", "x".repeat(400));
    let text = [
        "{not json message".to_owned(),
        late,
        token_line("2026-11-04T01:00:00Z", 10, 10, "codex", 5.0, 1_800_000_000),
        token_line(
            "2026-11-04T01:01:00Z",
            20,
            30,
            "codex_other",
            6.0,
            1_800_000_000,
        ),
        json!({"type": "event_msg", "payload": {"type": "token_count", "info": null}}).to_string(),
    ]
    .join("\n");
    home.write(".codex/sessions/2026/11/04/rollout-a.jsonl", &text);
    let zst = home
        .path()
        .join(".codex/sessions/2026/11/04/rollout-b.jsonl.zst");
    std::fs::write(
        &zst,
        ruzstd::encoding::compress_to_vec(
            text.as_bytes(),
            ruzstd::encoding::CompressionLevel::Fastest,
        ),
    )
    .unwrap();
    home.write(
        ".codex/sessions/2026/11/04/rollout-broken.jsonl.zst",
        "not zstd",
    );
    home.write(".codex/archived_sessions/rollout-c.jsonl", &text);
    let files = rollout_files(home.path());
    assert_eq!(files.len(), 4);
    let tokens = |f: &Path| {
        read_events(f)
            .iter()
            .map(|e| (e.ts, e.input, e.output))
            .collect::<Vec<_>>()
    };
    assert_eq!(tokens(&files[2]), tokens(&files[1]));
    assert!(read_events(&files[3]).is_empty());
    let got = read_events(&files[1]);
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].input, got[0].cached, got[0].output),
        (10.0, 1.0, 2.0)
    );
    assert_eq!(
        (got[0].used_percent, got[0].resets_at),
        (Some(5.0), Some(1.8e9))
    );
    assert_eq!(got[0].ts, 1_793_754_000.0);
}

#[test]
fn prefix_counts_characters_not_bytes() {
    assert_eq!(char_prefix("가나다".as_bytes(), 2), "가나".as_bytes());
    assert_eq!(char_prefix(b"ab", 5), b"ab");
}

#[test]
fn dedup_drops_cross_file_copies_and_zeroes_repeated_totals() {
    let events = vec![
        ev(2.0, "b", "t=1", Some(1.0), Some(100.0)),
        ev(1.0, "a", "t=1", Some(1.0), Some(100.0)),
        // 다른 rollout에 같은 (timestamp, total)이 복사됨 → 삭제.
        Event {
            file: "z".into(),
            ..ev(1.0, "a", "t=1", Some(1.0), Some(100.0))
        },
        // 같은 파일에서 total이 직전과 같음 → 토큰 0, 라벨 유지.
        ev(3.0, "a", "t=1", Some(2.0), Some(100.0)),
    ];
    let got = dedup(&events);
    let summary: Vec<(f64, &str, f64)> = got
        .iter()
        .map(|e| (e.ts, e.file.as_str(), e.input))
        .collect();
    assert_eq!(
        summary,
        [(1.0, "a", 10.0), (2.0, "b", 10.0), (3.0, "a", 0.0)]
    );
    assert_eq!(got[2].used_percent, Some(2.0));
}

#[test]
fn windows_group_resets_within_tolerance_and_count_excluded() {
    let events = vec![
        ev(1.0, "a", "1", Some(3.0), Some(1000.0)),
        ev(2.0, "a", "2", Some(4.0), Some(1018.0)), // 흔들림 18초 → 같은 창
        ev(3.0, "a", "3", Some(1.0), Some(1000.0 + 18.0 + 601.0)), // 601초 → 새 창
        ev(4.0, "a", "4", None, Some(50_000.0)),    // used_percent 없음 → 제외된 창
        ev(5.0, "a", "5", Some(1.0), None),
    ];
    let (windows, clusters) = assign_windows(&events, 600.0);
    assert_eq!(clusters, 3);
    let sizes: Vec<usize> = windows.iter().map(Vec::len).collect();
    assert_eq!(sizes, [2, 1]);
    assert_eq!(window_row(&windows[0]), (vec![12.0, 8.0, 4.0], 4.0));
}

fn synthetic_windows(count: usize) -> Vec<Event> {
    // 창마다 이벤트 하나: 1%p ≈ 비캐시 66K · 캐시 742K · 출력 5.5K.
    (0..count)
        .map(|i| {
            let f = i as f64;
            let (unc, cached, out) = (
                20_000.0 + 37_000.0 * ((f * 1.7) % 5.0),
                1e6 + 3e6 * ((f * 2.3) % 7.0),
                1_000.0 + 2_000.0 * ((f * 3.1) % 4.0),
            );
            let pct = unc / 66_000.0 + cached / 742_000.0 + out / 5_500.0;
            Event {
                input: unc + cached,
                cached,
                output: out,
                ..ev(f, "a", &i.to_string(), Some(pct), Some(f * 18_000.0))
            }
        })
        .collect()
}

#[test]
fn estimates_known_rates_from_synthetic_windows() {
    let got = rate(&synthetic_windows(20), &AdvancedTuning::default());
    assert_eq!(got.status, RateStatus::Estimated);
    assert_eq!((got.windows, got.excluded_windows), (20, 0));
    assert!(got.r2.unwrap() > 0.999);
    let per = got.per_percent.unwrap();
    for (value, want) in [
        (per.uncached_input, 66_000.0),
        (per.cached_input, 742_000.0),
        (per.output, 5_500.0),
    ] {
        assert!(
            (value.unwrap() / want - 1.0).abs() < 1e-6,
            "{value:?} vs {want}"
        );
    }
    let range = got.fold_range.unwrap().cached_input;
    assert!(range.min.unwrap() <= 742_000.1 && range.max.unwrap() >= 741_999.9);
}

#[test]
fn r2_thresholds_decide_the_status() {
    // 퍼센트에 ±10% 흔들림을 줘서 검증 R²가 1 아래로 내려오게 한다.
    let mut events = synthetic_windows(20);
    for (i, e) in events.iter_mut().enumerate() {
        let jitter = 1.0 + 0.1 * ((i % 3) as f64 - 1.0);
        e.used_percent = e.used_percent.map(|p| p * jitter);
    }
    let judge = |good_r2, range_r2| {
        let tuning = AdvancedTuning {
            good_r2,
            range_r2,
            ..AdvancedTuning::default()
        };
        rate(&events, &tuning)
    };
    let r2 = judge(0.7, 0.3).r2.unwrap();
    assert!(0.3 < r2 && r2 < 0.99, "{r2}");
    assert_eq!(judge(r2 - 0.01, 0.3).status, RateStatus::Estimated);
    let range_only = judge(r2 + 0.01, 0.3);
    assert_eq!(range_only.status, RateStatus::RangeOnly);
    assert!(range_only.per_percent.is_none() && range_only.fold_range.is_some());
    assert_eq!(judge(r2 + 0.02, r2 + 0.01).status, RateStatus::Unreliable);
}

#[test]
fn min_samples_per_var_and_reset_tolerance_are_read_from_tuning() {
    let few = AdvancedTuning {
        min_samples_per_var: 2,
        ..AdvancedTuning::default()
    };
    // 창 8개는 기본(3 × 3 = 9)이면 부족하지만 2 × 3 = 6이면 회귀한다.
    assert_ne!(
        rate(&synthetic_windows(8), &few).status,
        RateStatus::Insufficient
    );
    let events = vec![
        ev(1.0, "a", "1", Some(3.0), Some(1000.0)),
        ev(2.0, "a", "2", Some(4.0), Some(1018.0)),
    ];
    assert_eq!(assign_windows(&events, 600.0).0.len(), 1);
    assert_eq!(assign_windows(&events, 5.0).0.len(), 2);
}

#[test]
fn reports_insufficient_samples_instead_of_numbers() {
    let got = rate(&synthetic_windows(8), &AdvancedTuning::default());
    assert_eq!(got.status, RateStatus::Insufficient);
    assert_eq!(got.windows, 8);
    assert!(got.r2.is_none() && got.per_percent.is_none() && got.fold_range.is_none());
    assert_eq!(
        rate(&[], &AdvancedTuning::default()).status,
        RateStatus::NoData
    );
}

#[test]
fn reports_unreliable_when_tokens_do_not_explain_percent() {
    let mut events = synthetic_windows(12);
    for (i, e) in events.iter_mut().enumerate() {
        e.used_percent = Some(if i % 2 == 0 { 90.0 } else { 5.0 });
    }
    let got = rate(&events, &AdvancedTuning::default());
    assert_eq!(got.status, RateStatus::Unreliable);
    assert!(got.per_percent.is_none() && got.fold_range.is_none());
}

#[test]
fn median_and_p90_match_python_statistics() {
    let ten: Vec<f64> = (1..=10).map(f64::from).collect();
    // statistics.median → 5.5, quantiles(n=100, inclusive)[89] → 9.1
    assert_eq!((median(&ten), p90(&ten)), (5.5, 9.1));
    assert_eq!((median(&[7.0]), p90(&[7.0])), (7.0, 7.0));
    assert_eq!(
        spread(&[3, 1, 2]),
        Spread {
            median: 2.0,
            p90: 2.8
        }
    );
}

#[test]
fn pattern_counts_requests_and_cache_misses_per_window_in_the_period() {
    let day = DAY_SECS;
    let now = 100.0 * day;
    let req = |ts: f64, total: &str, input: f64, cached: f64, resets: f64| Event {
        input,
        cached,
        ..ev(ts, "a", total, Some(1.0), Some(resets))
    };
    let events = vec![
        req(now - 20.0 * day, "old", 10.0, 9.0, 1.0), // 기간 밖
        req(now - 2.0 * day, "1", 10.0, 9.0, 1e9),    // 창 1: 캐시 적중
        req(now - 2.0 * day + 1.0, "2", 10.0, 4.0, 1e9), // 창 1: 미스(0.4 < 0.5)
        req(now - 2.0 * day + 2.0, "3", 0.0, 0.0, 1e9), // 입력 0(반복 보고) → 요청 아님
        req(now - day, "4", 10.0, 5.0, 2e9),          // 창 2: 0.5는 미스 아님
        Event {
            used_percent: None,
            ..req(now - day, "5", 10.0, 0.0, 3e9) // used_percent 없음 → 창 아님
        },
    ];
    let tuning = AdvancedTuning::default();
    let got = pattern(&events, now, 14, &tuning).unwrap();
    assert_eq!((got.days, got.windows), (14, 2));
    assert_eq!(
        got.requests,
        Spread {
            median: 1.5,
            p90: 1.9
        }
    );
    assert_eq!(
        got.misses,
        Spread {
            median: 0.5,
            p90: 0.9
        }
    );

    let strict = AdvancedTuning {
        cache_miss_ratio: 0.95,
        ..tuning
    };
    assert_eq!(
        pattern(&events, now, 14, &strict).unwrap().misses.median,
        1.5
    );
    assert_eq!(pattern(&events, now + 30.0 * day, 14, &tuning), None);
}

/// 이 머신의 실제 `~/.codex`로 창 패턴을 낸다(숫자만). `now`는 `KCAL_PARITY_NOW`(유닉스 초).
/// `cargo test -- --ignored --nocapture codex_pattern_parity`.
#[test]
#[ignore = "실제 ~/.codex를 읽는다"]
fn codex_pattern_parity_on_real_rollouts() {
    let now: f64 = std::env::var("KCAL_PARITY_NOW")
        .expect("KCAL_PARITY_NOW")
        .parse()
        .unwrap();
    let home = std::env::home_dir().unwrap();
    let events = events(&rollout_files(&home));
    let got = pattern(&events, now, 14, &AdvancedTuning::default()).expect("pattern");
    println!(
        "PATTERN windows={} n_median={} n_p90={} m_median={} m_p90={}",
        got.windows, got.requests.median, got.requests.p90, got.misses.median, got.misses.p90
    );
}

/// 이 머신의 실제 `~/.codex`로 패리티 숫자를 낸다(숫자만). `cargo test -- --ignored --nocapture codex_parity`.
#[test]
#[ignore = "실제 ~/.codex를 읽는다"]
fn codex_parity_on_real_rollouts() {
    let home = std::env::home_dir().unwrap();
    let files = rollout_files(&home);
    let events = events(&files);
    let result = window_fit(&events, &AdvancedTuning::default());
    let fit = result.fit.flatten().expect("fit");
    let names = ["uncached_in", "cached_in", "output"];
    println!("PARITY files={} events={}", files.len(), events.len());
    println!(
        "PARITY windows={} excluded={} train={:.6} loo={:.6} holdout={:.6} val={:.6}",
        result.windows,
        result.excluded_windows,
        fit.train,
        fit.loo,
        fit.holdout,
        fit.validation()
    );
    // get_rates가 돌려주는 값(같은 이벤트).
    let shown = rate(&events, &AdvancedTuning::default());
    println!(
        "PARITY get_rates status={:?} r2={:?} windows={} per_percent={:?}",
        shown.status, shown.r2, shown.windows, shown.per_percent
    );
    for (pos, &var) in fit.used.iter().enumerate() {
        let (lo, hi) = fit.fold_range[pos];
        println!(
            "PARITY coef {}={:e} fold_lo={:e} fold_hi={:e}",
            names[var], fit.coef[pos], lo, hi
        );
    }
}

#[test]
fn for_each_line_stops_at_overlong_line() {
    let home = TempDir::new("overlong");
    let path = home.write("rollout-x.jsonl", "ok\n0123456789abcdef\nlater\n");
    let mut got = Vec::new();
    for_each_line_capped(&path, 8, |l| got.push(l.to_vec()));
    assert_eq!(got, [b"ok\n".to_vec()]);
    // 상한 이하이면 모두 읽는다(상한 길이 줄 + 개행 포함).
    let mut all = 0;
    for_each_line_capped(&path, 16, |_| all += 1);
    assert_eq!(all, 3);
}

#[test]
fn rereads_a_rollout_only_after_it_changes_and_forgets_deleted_ones() {
    let home = TempDir::new("rate-codex-memo");
    let line = token_line("2026-11-04T01:00:00Z", 10, 10, "codex", 5.0, 1_800_000_000);
    home.write(".codex/sessions/2026/11/04/rollout-m.jsonl", &line);
    let file = rollout_files(home.path()).remove(0);
    let reads = || {
        PARSE_COUNTS
            .lock()
            .unwrap()
            .get(&file)
            .copied()
            .unwrap_or(0)
    };
    assert_eq!(read_events(&file).len(), 1);
    assert_eq!(read_events(&file).len(), 1);
    assert_eq!(reads(), 1);

    let second = token_line("2026-11-04T01:01:00Z", 20, 30, "codex", 6.0, 1_800_000_000);
    std::fs::write(&file, format!("{line}\n{second}")).unwrap();
    assert_eq!(read_events(&file).len(), 2);
    assert_eq!(reads(), 2);

    std::fs::remove_file(&file).unwrap();
    assert!(rollout_files(home.path()).is_empty());
    assert!(!PARSED.lock().unwrap().contains_key(&file));
}
