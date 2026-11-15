//! 시작 컨텍스트의 분모: 측정한 세션 모델의 컨텍스트 창(0005 "창 크기", 2026-11-11).
//!
//! Claude는 `/context`와 같은 값을 쓴다: 모델 창(tuning `CLAUDE_CONTEXT_WINDOWS`, 문서 출처)을 자동 압축 창이
//! 있으면 그만큼으로 줄인다. Codex는 rollout `token_count.info.model_context_window`(기록값)를 그대로 쓴다.

use std::collections::HashMap;

use serde_json::Value;

use crate::tuning::{
    CLAUDE_1M_WINDOW, CLAUDE_AUTO_COMPACT_WINDOW_RANGE, CLAUDE_CONTEXT_WINDOWS,
    CLAUDE_DISABLED_1M_WINDOW,
};

const ONE_M_SUFFIX: &str = "[1m]";
const DATE_SUFFIX_DIGITS: usize = 8;

/// 문서에 있는 모델 창. 날짜 접미사(`-YYYYMMDD`)는 뗀다. 모르는 모델은 `None`.
fn model_window(model: &str) -> Option<u64> {
    let (base, is_1m) = match model.strip_suffix(ONE_M_SUFFIX) {
        Some(base) => (base, true),
        None => (model, false),
    };
    let base = match base.rsplit_once('-') {
        Some((head, tail))
            if tail.len() == DATE_SUFFIX_DIGITS && tail.bytes().all(|b| b.is_ascii_digit()) =>
        {
            head
        }
        _ => base,
    };
    let window = CLAUDE_CONTEXT_WINDOWS
        .iter()
        .find(|(id, _)| *id == base)
        .map(|(_, w)| *w)?;
    Some(if is_1m { CLAUDE_1M_WINDOW } else { window })
}

/// `200000`·`200k`·`1M`. 접미사 없는 수는 토큰 수다. 범위 밖은 끝값, 읽을 수 없으면 `None`(무시).
fn parse_window(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    let (digits, scale) = match raw.char_indices().last()? {
        (i, 'k' | 'K') => (&raw[..i], 1_000),
        (i, 'm' | 'M') => (&raw[..i], 1_000_000),
        _ => (raw, 1),
    };
    let value = digits.parse::<u64>().ok()?.checked_mul(scale)?;
    Some(value.clamp(
        *CLAUDE_AUTO_COMPACT_WINDOW_RANGE.start(),
        *CLAUDE_AUTO_COMPACT_WINDOW_RANGE.end(),
    ))
}

fn setting_window(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(n) => parse_window(&n.as_u64()?.to_string()),
        Value::String(s) => parse_window(s),
        _ => None,
    }
}

/// 설정 파일 하나의 자동 압축 창: 모델별(`modelSettings.<model>.autoCompactWindow`)이 전체(`autoCompactWindow`)보다 앞선다.
fn file_window(settings: &Value, model: &str) -> Option<u64> {
    let per_model = settings
        .get("modelSettings")
        .and_then(|m| m.get(model))
        .and_then(|m| m.get("autoCompactWindow"));
    setting_window(per_model).or_else(|| setting_window(settings.get("autoCompactWindow")))
}

/// Claude Code `/context`의 분모. `settings`는 낮은 우선순위부터(user → project → local), `env`는 실행 env + settings `env`.
/// 모델을 모르면 `None`. 자동 압축 창은 모델 창을 넘지 않는다(문서 "caps it at the model's context window").
pub fn claude(model: &str, env: &HashMap<String, String>, settings: &[Value]) -> Option<u64> {
    let mut window = model_window(model)?;
    if env
        .get("CLAUDE_CODE_DISABLE_1M_CONTEXT")
        .map(String::as_str)
        == Some("1")
    {
        window = window.min(CLAUDE_DISABLED_1M_WINDOW);
    }
    let auto_compact = env
        .get("CLAUDE_CODE_AUTO_COMPACT_WINDOW")
        .and_then(|v| parse_window(v))
        .or_else(|| settings.iter().rev().find_map(|s| file_window(s, model)));
    Some(auto_compact.map_or(window, |a| a.min(window)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn claude_window_matches_context_command_denominator() {
        let none = HashMap::new();
        // 이 Mac(2026-11-11): /context "20k / 400k", settings modelSettings.claude-opus-5-5.autoCompactWindow 400000.
        let user = json!({"modelSettings": {"claude-opus-5-5": {"autoCompactWindow": 400000}}});
        assert_eq!(
            claude("claude-opus-5-5", &none, std::slice::from_ref(&user)),
            Some(400_000)
        );
        assert_eq!(claude("claude-opus-5-5", &none, &[]), Some(1_000_000));
        // 모델별 값은 다른 모델에 쓰지 않고, 전체 값은 모델 창을 넘지 않는다.
        assert_eq!(claude("claude-sonnet-5", &none, &[user]), Some(1_000_000));
        let all = json!({"autoCompactWindow": 500000});
        assert_eq!(
            claude("claude-opus-4-6", &none, std::slice::from_ref(&all)),
            Some(200_000)
        );
        assert_eq!(
            claude("claude-opus-4-6[1m]", &none, std::slice::from_ref(&all)),
            Some(500_000)
        );
        // env가 설정보다 앞서고, 뒤 설정 파일이 앞 파일보다 앞선다.
        let local = json!({"autoCompactWindow": "300k"});
        assert_eq!(
            claude("claude-opus-5", &none, &[all.clone(), local]),
            Some(300_000)
        );
        let override_env = env(&[("CLAUDE_CODE_AUTO_COMPACT_WINDOW", "200")]);
        assert_eq!(
            claude("claude-opus-5", &override_env, &[all]),
            Some(100_000)
        );
        let off = env(&[("CLAUDE_CODE_DISABLE_1M_CONTEXT", "1")]);
        assert_eq!(claude("claude-fable-5-2", &off, &[]), Some(200_000));
        // 날짜 접미사는 떼고, 모르는 모델은 창이 없다.
        assert_eq!(
            claude("claude-sonnet-4-6-20260101", &none, &[]),
            Some(200_000)
        );
        assert_eq!(claude("my-gateway-opus", &none, &[]), None);
        assert_eq!(claude("claude-haiku-4-5-20251001", &none, &[]), None);
        assert_eq!(parse_window("nope"), None);
        assert_eq!(parse_window("2M"), Some(1_000_000));
    }
}
