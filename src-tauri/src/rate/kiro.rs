//! Kiro 모델별 크레딧/턴(실측). `~/.kiro/sessions/<hash>/sess_*/messages.jsonl`의
//! `usage_summary` 줄에서 `promptTurnSummaries[]`의 `unit == "credit"`인 `usage`만 더한다.
//!
//! 본문 줄은 JSON 파싱하지 않는다: 앞 300자 안에 `"payload":{"type":"usage_summary"`가 있는 줄만 읽는다.
//! 턴 요약에는 모델이 없어 같은 폴더 `session.json`의 `modelId`를 쓴다(세션 중 모델을 바꾸면 마지막 값으로 묶인다).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::KiroModelCredits;
use crate::context::{fsutil, kiro_session};

const SUMMARY_MARK: &[u8] = br#""payload":{"type":"usage_summary""#;
const PREFIX_BYTES: usize = 300;
const CREDIT_UNIT: &str = "credit";
/// `modelId`가 없는 세션.
pub const UNKNOWN_MODEL: &str = "unknown";

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// 한 줄의 턴별 크레딧. `usage_summary`가 아니면 빈 목록.
fn turn_credits(row: &Value) -> Vec<f64> {
    let Some(payload) = row.get("payload") else {
        return Vec::new();
    };
    if payload.get("type").and_then(Value::as_str) != Some("usage_summary") {
        return Vec::new();
    }
    payload
        .get("promptTurnSummaries")
        .and_then(Value::as_array)
        .map(|turns| {
            turns
                .iter()
                .filter(|t| t.get("unit").and_then(Value::as_str) == Some(CREDIT_UNIT))
                .filter_map(|t| t.get("usage")?.as_f64())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .collect()
        })
        .unwrap_or_default()
}

/// 턴 하나: (그 `usage_summary` 줄의 `timestamp`(유닉스 초, 없거나 틀리면 `None`), 크레딧).
pub type TimedCredit = (Option<f64>, f64);

/// 파일 하나의 턴별 크레딧과 시각. 접두 검사를 통과한 줄만 JSON으로 읽는다.
pub fn timed_credits(file: &Path) -> Vec<TimedCredit> {
    let Ok(handle) = std::fs::File::open(file) else {
        return Vec::new();
    };
    let mut reader = BufReader::new(handle);
    let mut line = Vec::new();
    let mut credits = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if !contains(&line[..line.len().min(PREFIX_BYTES)], SUMMARY_MARK) {
            continue;
        }
        if let Ok(row) = serde_json::from_slice::<Value>(&line) {
            let ts = row
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok())
                .map(|t| t.unix_timestamp_nanos() as f64 / 1e9);
            credits.extend(turn_credits(&row).into_iter().map(|c| (ts, c)));
        }
    }
    credits
}

fn file_credits(file: &Path) -> Vec<f64> {
    timed_credits(file).into_iter().map(|(_, c)| c).collect()
}

fn session_model(session_dir: &Path) -> String {
    fsutil::read_json(&session_dir.join("session.json"))
        .and_then(|v| v.get("modelId")?.as_str().map(str::to_owned))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| UNKNOWN_MODEL.to_owned())
}

/// 모델별 턴 수와 평균 크레딧. 턴이 많은 순(같으면 이름순). 기록이 없으면 빈 목록.
pub fn credits(home: &Path) -> Vec<KiroModelCredits> {
    let mut totals: BTreeMap<String, (u64, f64)> = BTreeMap::new();
    for file in kiro_session::message_files(home) {
        let turns = file_credits(&file);
        if turns.is_empty() {
            continue;
        }
        let model = file
            .parent()
            .map_or_else(|| UNKNOWN_MODEL.to_owned(), session_model);
        let entry = totals.entry(model).or_default();
        entry.0 += turns.len() as u64;
        entry.1 += turns.iter().sum::<f64>();
    }
    let mut models: Vec<KiroModelCredits> = totals
        .into_iter()
        .map(|(model, (turns, sum))| KiroModelCredits {
            model,
            turns,
            credits_per_turn: sum / turns as f64,
        })
        .collect();
    models.sort_by_key(|m| std::cmp::Reverse(m.turns));
    models
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;

    /// 실제 파일처럼 `payload.type`이 맨 앞이다(`json!`은 키를 정렬해서 직접 쓴다).
    fn summary(turns: Value) -> String {
        format!(
            r#"{{"id":"x","timestamp":"2026-11-04T01:00:00Z","payload":{{"type":"usage_summary","promptTurnSummaries":{turns},"status":"success"}}}}"#
        )
    }

    fn session(home: &TempDir, id: &str, model: Option<&str>, lines: &[String]) {
        let meta = model.map_or(json!({}), |m| json!({"modelId": m}));
        home.write(
            &format!(".kiro/sessions/h/{id}/session.json"),
            &meta.to_string(),
        );
        home.write(
            &format!(".kiro/sessions/h/{id}/messages.jsonl"),
            &lines.join("\n"),
        );
    }

    #[test]
    fn averages_credit_turns_per_session_model_without_parsing_message_lines() {
        let home = TempDir::new("rate-kiro");
        let credit = |v: f64| json!({"unit": "credit", "unitPlural": "credits", "usage": v});
        session(
            &home,
            "sess_a",
            Some("claude-sonnet-5.5"),
            &[
                "{not json message".into(),
                // 본문 안의 비슷한 문자열(이스케이프됨)은 표시와 맞지 않아 파싱하지 않는다.
                json!({"payload": {"type": "user", "content": "\"payload\":{\"type\":\"usage_summary\""}})
                    .to_string(),
                summary(json!([credit(0.4)])),
                summary(json!([credit(0.6), {"unit": "token", "usage": 99.0}])),
                summary(json!([])),
            ],
        );
        session(
            &home,
            "sess_b",
            Some("claude-sonnet-5.5"),
            &[summary(json!([credit(0.5)]))],
        );
        session(&home, "sess_c", None, &[summary(json!([credit(2.0)]))]);
        session(
            &home,
            "sess_d",
            Some("claude-opus-5.5"),
            &["{\"payload\":{}}".into()],
        );

        let got = credits(home.path());
        assert_eq!(got.len(), 2);
        assert_eq!(
            (got[0].model.as_str(), got[0].turns),
            ("claude-sonnet-5.5", 3)
        );
        assert!((got[0].credits_per_turn - 0.5).abs() < 1e-12);
        assert_eq!((got[1].model.as_str(), got[1].turns), (UNKNOWN_MODEL, 1));
        assert_eq!(got[1].credits_per_turn, 2.0);
    }

    #[test]
    fn returns_no_models_without_sessions() {
        let home = TempDir::new("rate-kiro-none");
        assert!(credits(home.path()).is_empty());
    }
}
