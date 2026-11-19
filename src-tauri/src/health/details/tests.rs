use super::*;
use crate::context::fsutil::testing::TempDir;
use crate::health::{read, report};
use serde_json::json;
use time::macros::datetime;

fn summary(home: &Path, agent: Agent) -> AgentComparison {
    let tuning = Tuning::default();
    let samples = read::samples(home, &home.join("none"), agent, 0.0, &tuning);
    let mut report = report(&[samples], &[], datetime!(2026-11-19 00:00 UTC), 1);
    enrich(&mut report.weekly, home, &tuning);
    report.weekly.remove(0)
}

#[test]
fn claude_automated_share_uses_cached_input_and_unknown_entrypoints_are_interactive() {
    let home = TempDir::new("health-entrypoints");
    let assistant = |id: &str, entrypoint: Option<&str>, ts: &str| {
        json!({
        "type":"assistant", "timestamp":ts, "entrypoint":entrypoint,
        "message":{"id":id,"model":"opus","usage":{"input_tokens":10,"cache_read_input_tokens":90}}
    }).to_string()
    };
    let lines = [
        assistant("old", Some("sdk-cli"), "2026-11-17T12:00:00Z"),
        assistant("a", Some("sdk-cli"), "2026-11-18T01:00:00Z"),
        assistant("b", Some("cli"), "2026-11-18T02:00:00Z"),
        assistant("c", Some("future"), "2026-11-18T03:00:00Z"),
        assistant("d", None, "2026-11-18T04:00:00Z"),
        assistant("a", Some("sdk-cli"), "2026-11-18T01:00:00Z"),
    ];
    home.write(".claude/projects/w/session.jsonl", &lines.join("\n"));
    let got = summary(home.path(), Agent::Claude);
    assert_eq!(got.before.automated_input_share, Some(100.0));
    assert_eq!(got.after.automated_input_share, Some(25.0));
    assert_eq!(got.after.model_input_shares.unwrap()[0].share, 100.0);
    assert!(got.after.codex_rate.is_none());
    let entries = claude::entries(&home.path().join(".claude/projects/w/session.jsonl"));
    assert!(Arc::ptr_eq(
        entries[1].entrypoint.as_ref().unwrap(),
        entries[5].entrypoint.as_ref().unwrap()
    ));
}

fn token(ts: &str, total: u64, input: u64) -> String {
    json!({"type":"event_msg","timestamp":ts,"payload":{
        "type":"token_count", "info":{"last_token_usage":{"input_tokens":input},"total_token_usage":{"input_tokens":total}},
        "rate_limits":{"limit_id":"codex","primary":{"used_percent":10,"resets_at":1795046400}}
    }}).to_string()
}

#[test]
fn codex_exec_split_tracks_each_turn_model_and_period_rate_is_null_when_insufficient() {
    let home = TempDir::new("health-codex-models");
    let session =
        |origin: &str| json!({"type":"session_meta","payload":{"originator":origin}}).to_string();
    let model = |name: &str| json!({"type":"turn_context","payload":{"model":name}}).to_string();
    home.write(
        ".codex/sessions/rollout-a.jsonl",
        &[
            session("codex_exec"),
            model("model-a"),
            token("2026-11-17T12:00:00Z", 50, 50),
            token("2026-11-18T01:00:00Z", 100, 50),
            model("model-b"),
            // 같은 이벤트의 재보고는 첫 모델을 보존한다.
            token("2026-11-18T01:00:00Z", 100, 50),
            token("2026-11-18T02:00:00Z", 250, 150),
            // 새 토큰 없는 보고는 기존 dedup대로 입력 0.
            token("2026-11-18T02:01:00Z", 250, 150),
        ]
        .join("\n"),
    );
    home.write(
        ".codex/sessions/rollout-b.jsonl",
        &[
            session("codex-tui"),
            model("model-c"),
            token("2026-11-18T03:00:00Z", 200, 200),
        ]
        .join("\n"),
    );
    home.write(
        ".codex/sessions/rollout-c.jsonl",
        &[session("future"), token("2026-11-18T04:00:00Z", 100, 100)].join("\n"),
    );
    let files = codex::rollout_files(home.path());
    let events = codex::events(&files);
    let attributed: Vec<_> = codex::attributed_events(&files)
        .into_iter()
        .map(|(e, _)| e)
        .collect();
    assert_eq!(
        events, attributed,
        "attribution must not change existing rate inputs"
    );
    let got = summary(home.path(), Agent::Codex);
    assert_eq!(got.before.automated_input_share, Some(100.0));
    assert_eq!(got.after.automated_input_share, Some(40.0));
    let models = got.after.model_input_shares.as_ref().unwrap();
    assert_eq!(
        models
            .iter()
            .map(|m| m.model.as_deref())
            .collect::<Vec<_>>(),
        [Some("model-c"), Some("model-b"), Some("unknown"), None]
    );
    assert_eq!(
        models.iter().map(|m| m.share).collect::<Vec<_>>(),
        [40.0, 30.0, 20.0, 10.0]
    );
    for period in [got.before, got.after] {
        let rate = period.codex_rate.unwrap();
        assert_eq!(rate.status, RateStatus::Insufficient);
        assert_eq!(rate.windows, 1);
        assert!(rate.per_percent.is_none() && rate.fold_range.is_none());
    }
}

#[test]
fn top_three_and_other_are_input_weighted_deterministic_and_exclude_period_end() {
    let home = TempDir::new("health-share-top");
    let mut period = summary(home.path(), Agent::Claude).after;
    let ts = unix_secs(period.from);
    let inputs: Vec<_> = [
        ("b", 30.0),
        ("a", 30.0),
        ("c", 20.0),
        ("d", 10.0),
        ("e", 10.0),
    ]
    .into_iter()
    .map(|(m, tokens)| Input {
        ts,
        tokens,
        model: Some(Arc::from(m)),
        automated: false,
    })
    .chain([Input {
        ts: unix_secs(period.to),
        tokens: 1000.0,
        model: None,
        automated: true,
    }])
    .collect();
    shares(&mut period, &inputs);
    let models = period.model_input_shares.unwrap();
    assert_eq!(
        models
            .iter()
            .map(|m| m.model.as_deref())
            .collect::<Vec<_>>(),
        [Some("a"), Some("b"), Some("c"), None]
    );
    assert_eq!(
        models.iter().map(|m| m.share).collect::<Vec<_>>(),
        [30.0, 30.0, 20.0, 20.0]
    );
    assert_eq!(period.automated_input_share, Some(0.0));
}

#[test]
fn no_input_is_null_and_kiro_is_unchanged() {
    let home = TempDir::new("health-share-empty");
    for agent in [Agent::Claude, Agent::Codex, Agent::Kiro] {
        let got = summary(home.path(), agent);
        for period in [got.before, got.after] {
            assert!(period.automated_input_share.is_none());
            assert!(period.model_input_shares.is_none());
            if agent == Agent::Codex {
                let rate = period.codex_rate.unwrap();
                assert_eq!(rate.status, RateStatus::NoData);
                assert_eq!(rate.windows, 0);
                assert!(rate.per_percent.is_none() && rate.fold_range.is_none());
            } else {
                assert!(period.codex_rate.is_none());
            }
        }
    }
}
