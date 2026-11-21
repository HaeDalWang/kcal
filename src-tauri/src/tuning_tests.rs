//! `tuning.rs` 테스트.

use super::*;
use serde_json::json;

#[test]
fn defaults_equal_the_former_constants() {
    let t = Tuning::default();
    assert_eq!(t.api_interval(), Duration::from_secs(300));
    assert_eq!(t.kiro_interval(), Duration::from_secs(600));
    assert_eq!(t.refresh_min_gap(), Duration::from_secs(30));
    assert_eq!(t.max_backoff(), Duration::from_secs(3600));
    assert_eq!(t.rates_ttl(), Duration::from_secs(1800));
    assert_eq!(t.context_ttl(), Duration::from_secs(600));
    assert_eq!(t.http_timeout(), Duration::from_secs(15));
    assert_eq!(t.keychain_timeout(), Duration::from_secs(10));
    assert_eq!(t.kiro_timeout(), Duration::from_secs(30));
    assert_eq!(
        (
            t.codex_sessions_to_search,
            t.kiro_sessions_to_search,
            t.shape_sessions_to_search,
            t.codex_rollouts_checked
        ),
        (50, 50, 5, 3)
    );
    assert_eq!((t.claude_log_tail_bytes, t.bytes_per_token), (65536, 4));
    assert_eq!((t.breakdown_days, t.low_percent), (14, 20));
    assert_eq!(t.metrics_ttl(), Duration::from_secs(300));
    assert_eq!((t.recent_bars, t.bar_days, t.claude_log_files), (8, 7, 50));
    assert_eq!(
        (t.top_sessions, t.marathon_requests, t.large_context_tokens),
        (3, 150, 130_000)
    );
    assert_eq!((t.daily_goal_percent, t.kiro_daily_credits), (14, 0));
    assert_eq!(
        (t.compare_days, t.check_model_days, t.check_alert_min),
        (7, 30, 1)
    );
    assert_eq!(t.claude_handoff_timeout(), Duration::from_secs(300));
    assert_eq!(t.claude_handoff_min_remaining, 20);
    assert_eq!(
        (
            t.diagnose_usage_days,
            t.diagnose_usage_files,
            t.diagnose_usage_max_mib,
            t.diagnose_min_sessions,
            t.scope_min_uses
        ),
        (30, 1000, 2048, 10, 3)
    );
    assert_eq!(
        t.advanced,
        AdvancedTuning {
            good_r2: 0.7,
            range_r2: 0.3,
            reset_tol_secs: 600,
            holdout_train: 0.7,
            min_samples_per_var: 3,
            claude_rate_days: 7,
            cache_miss_ratio: 0.5
        }
    );
    assert_eq!(t.validate(), Ok(()));
}

#[test]
fn json_round_trip_uses_camel_case_and_nests_advanced() {
    let value = serde_json::to_value(Tuning::default()).unwrap();
    assert_eq!(value["apiIntervalSecs"], 300);
    assert_eq!(value["advanced"]["goodR2"], 0.7);
    assert_eq!(Tuning::from_json(&value), Tuning::default());
}

#[test]
fn file_keeps_valid_fields_and_resets_only_the_bad_ones() {
    let tuning = Tuning::from_json(&json!({
        "apiIntervalSecs": 120,
        "kiroIntervalSecs": 5,          // 범위 밖
        "httpTimeoutSecs": "fast",      // 형식 틀림
        "sessionsToSearch": 9,          // 모르는 필드
        "advanced": { "goodR2": 0.9, "holdoutTrain": 1.0 },
    }));
    let expected = Tuning {
        api_interval_secs: 120,
        advanced: AdvancedTuning {
            good_r2: 0.9,
            ..AdvancedTuning::default()
        },
        ..Tuning::default()
    };
    assert_eq!(tuning, expected);
    assert_eq!(Tuning::from_json(&json!("x")), Tuning::default());
    assert_eq!(
        Tuning::from_json(&json!({"advanced": 5})),
        Tuning::default()
    );
}

#[test]
fn good_r2_below_range_r2_is_rejected() {
    let from_file = Tuning::from_json(&json!({"advanced": {"goodR2": 0.2}}));
    assert_eq!(from_file, Tuning::default());
    let mut tuning = Tuning::default();
    tuning.advanced.good_r2 = 0.2;
    let error = tuning.validate().unwrap_err();
    assert!(error.contains("advanced.goodR2") && error.contains("advanced.rangeR2"));
}

#[test]
fn breakdown_days_and_cache_miss_ratio_are_range_checked() {
    let from_file = Tuning::from_json(&json!({
        "breakdownDays": 91,
        "advanced": { "cacheMissRatio": 0.01 },
    }));
    assert_eq!(from_file, Tuning::default());
    let kept = Tuning::from_json(&json!({
        "breakdownDays": 90,
        "advanced": { "cacheMissRatio": 0.95 },
    }));
    assert_eq!(
        (kept.breakdown_days, kept.advanced.cache_miss_ratio),
        (90, 0.95)
    );
    let mut tuning = Tuning {
        breakdown_days: 0,
        ..Tuning::default()
    };
    tuning.advanced.cache_miss_ratio = 0.96;
    let error = tuning.validate().unwrap_err();
    assert!(error.contains("breakdownDays") && error.contains("advanced.cacheMissRatio"));
}

#[test]
fn validate_names_every_out_of_range_field() {
    let tuning = Tuning {
        bytes_per_token: 0,
        codex_sessions_to_search: 501,
        claude_log_tail_bytes: 1,
        ..Tuning::default()
    };
    let error = tuning.validate().unwrap_err();
    for name in [
        "bytesPerToken",
        "codexSessionsToSearch",
        "claudeLogTailBytes",
    ] {
        assert!(error.contains(name), "{error}");
    }
    assert!(!error.contains("apiIntervalSecs"));
}

/// 필드 하나를 JSON 이름으로 바꾼 tuning. R² 쌍은 짝 조건에 걸리지 않게 둘 다 같은 값으로 둔다.
fn with_field(key: &str, value: f64, is_integer: bool) -> Tuning {
    let mut json = serde_json::to_value(Tuning::default()).unwrap();
    let keys: &[&str] = match key {
        "advanced.goodR2" | "advanced.rangeR2" => &["advanced.goodR2", "advanced.rangeR2"],
        _ => &[key],
    };
    for k in keys {
        let pointer = format!("/{}", k.replace('.', "/"));
        *json.pointer_mut(&pointer).unwrap() = if is_integer {
            json!(value as u64)
        } else {
            json!(value)
        };
    }
    serde_json::from_value(json).unwrap()
}

#[test]
fn spec_covers_every_field_with_its_default_and_validated_bounds() {
    let spec = Tuning::spec();
    let default_json = serde_json::to_value(Tuning::default()).unwrap();
    let Value::Object(top) = &default_json else {
        unreachable!()
    };
    let mut json_keys: Vec<String> = leaves(top)
        .into_iter()
        .map(|(pointer, _)| pointer.trim_start_matches('/').replace('/', "."))
        .collect();
    let mut spec_keys: Vec<String> = spec.iter().map(|f| f.key.to_owned()).collect();
    json_keys.sort();
    spec_keys.sort();
    assert_eq!(spec_keys, json_keys);

    for f in &spec {
        let pointer = format!("/{}", f.key.replace('.', "/"));
        assert_eq!(
            default_json.pointer(&pointer).and_then(Value::as_f64),
            Some(f.default),
            "{}",
            f.key
        );
        let step = if f.is_integer { 1.0 } else { 0.001 };
        for inside in [f.min, f.max] {
            let t = with_field(f.key, inside, f.is_integer);
            assert!(!t.invalid_fields().contains(&f.key), "{} = {inside}", f.key);
        }
        // 음수는 u64 형식에서 이미 거부된다(범위 검사까지 오지 않는다).
        for outside in [f.min - step, f.max + step] {
            if f.is_integer && outside < 0.0 {
                continue;
            }
            let t = with_field(f.key, outside, f.is_integer);
            assert!(t.invalid_fields().contains(&f.key), "{} = {outside}", f.key);
        }
    }
}

#[test]
fn spec_uses_the_named_ranges() {
    let find = |key: &str| *Tuning::spec().iter().find(|f| f.key == key).unwrap();
    let api = find("apiIntervalSecs");
    assert_eq!(
        (api.min, api.max, api.default, api.is_integer),
        (
            *API_INTERVAL_SECS_RANGE.start() as f64,
            *API_INTERVAL_SECS_RANGE.end() as f64,
            300.0,
            true
        )
    );
    let tail = find("claudeLogTailBytes");
    assert_eq!(
        (tail.min, tail.max),
        (
            *CLAUDE_LOG_TAIL_BYTES_RANGE.start() as f64,
            *CLAUDE_LOG_TAIL_BYTES_RANGE.end() as f64
        )
    );
    let holdout = find("advanced.holdoutTrain");
    assert_eq!(
        (
            holdout.min,
            holdout.max,
            holdout.default,
            holdout.is_integer
        ),
        (
            *HOLDOUT_TRAIN_RANGE.start(),
            *HOLDOUT_TRAIN_RANGE.end(),
            0.7,
            false
        )
    );
    let low = find("lowPercent");
    assert_eq!(
        (low.min, low.max, low.default, low.is_integer),
        (1.0, 99.0, 20.0, true)
    );
}

#[test]
fn low_percent_is_range_checked_like_other_fields() {
    for bad in [0, 100] {
        let from_file = Tuning::from_json(&json!({ "lowPercent": bad }));
        assert_eq!(from_file.low_percent, 20, "{bad}");
        let error = Tuning {
            low_percent: bad,
            ..Tuning::default()
        }
        .validate()
        .unwrap_err();
        assert!(error.contains("lowPercent"), "{error}");
    }
    for good in [1, 99] {
        assert_eq!(
            Tuning::from_json(&json!({ "lowPercent": good })).low_percent,
            good
        );
    }
    assert_eq!(
        Tuning::from_json(&json!({ "lowPercent": 12.5 })).low_percent,
        20
    );
}

#[test]
fn daily_goals_are_range_checked_and_kiro_zero_means_auto() {
    for (key, bad) in [
        ("dailyGoalPercent", 0),
        ("dailyGoalPercent", 101),
        ("kiroDailyCredits", 100_001),
    ] {
        assert_eq!(
            Tuning::from_json(&json!({ key: bad })),
            Tuning::default(),
            "{key} = {bad}"
        );
    }
    let kept = Tuning::from_json(&json!({ "dailyGoalPercent": 100, "kiroDailyCredits": 0 }));
    assert_eq!((kept.daily_goal_percent, kept.kiro_daily_credits), (100, 0));
    assert_eq!(
        Tuning::from_json(&json!({ "kiroDailyCredits": -1 })),
        Tuning::default()
    );
    let error = Tuning {
        daily_goal_percent: 0,
        ..Tuning::default()
    }
    .validate()
    .unwrap_err();
    assert!(error.contains("dailyGoalPercent"), "{error}");
}

#[test]
fn check_alert_min_is_an_integer_from_1_to_20() {
    for bad in [json!(0), json!(21), json!(1.5), json!("2")] {
        assert_eq!(
            Tuning::from_json(&json!({ "checkAlertMin": bad.clone() })).check_alert_min,
            1,
            "{bad}"
        );
    }
    for good in [1, 20] {
        assert_eq!(
            Tuning::from_json(&json!({ "checkAlertMin": good })).check_alert_min,
            good
        );
    }
    let error = Tuning {
        check_alert_min: 0,
        ..Tuning::default()
    }
    .validate()
    .unwrap_err();
    assert!(error.contains("checkAlertMin"), "{error}");
}

#[test]
fn claude_handoff_timeout_and_min_remaining_are_integers_in_range() {
    for (key, bad) in [
        ("claudeHandoffTimeoutSecs", json!(59)),
        ("claudeHandoffTimeoutSecs", json!(1801)),
        ("claudeHandoffTimeoutSecs", json!(90.5)),
        ("claudeHandoffMinRemaining", json!(91)),
        ("claudeHandoffMinRemaining", json!(-1)),
        ("claudeHandoffMinRemaining", json!(2.5)),
    ] {
        assert_eq!(
            Tuning::from_json(&json!({ key: bad.clone() })),
            Tuning::default(),
            "{key} = {bad}"
        );
    }
    let kept = Tuning::from_json(&json!({
        "claudeHandoffTimeoutSecs": 1800,
        "claudeHandoffMinRemaining": 0,
    }));
    assert_eq!(
        (
            kept.claude_handoff_timeout_secs,
            kept.claude_handoff_min_remaining
        ),
        (1800, 0)
    );
    let error = Tuning {
        claude_handoff_timeout_secs: 30,
        claude_handoff_min_remaining: 95,
        ..Tuning::default()
    }
    .validate()
    .unwrap_err();
    assert!(
        error.contains("claudeHandoffTimeoutSecs") && error.contains("claudeHandoffMinRemaining"),
        "{error}"
    );
}

#[test]
fn claude_rate_days_accepts_3_to_90_and_rejects_out_of_range_values() {
    for bad in [2, 91] {
        let tuning = Tuning {
            advanced: AdvancedTuning {
                claude_rate_days: bad,
                ..AdvancedTuning::default()
            },
            ..Tuning::default()
        };
        assert!(
            tuning
                .validate()
                .unwrap_err()
                .contains("advanced.claudeRateDays")
        );
        assert_eq!(
            Tuning::from_json(&json!({"advanced": {"claudeRateDays": bad}})),
            Tuning::default()
        );
    }
    for good in [3, 90] {
        let tuning = Tuning::from_json(&json!({"advanced": {"claudeRateDays": good}}));
        assert_eq!(tuning.advanced.claude_rate_days, good);
        assert_eq!(tuning.validate(), Ok(()));
    }
}
