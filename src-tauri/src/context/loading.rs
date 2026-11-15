//! MCP 도구 스키마가 세션 시작 때 들어가는가, 모델이 찾을 때까지 미뤄지는가(ctxmeter `tool-loading.js`).
//!
//! 출처(ctxmeter 2026-09-28·10-13 확인):
//!   Claude  https://code.claude.com/docs/en/agent-sdk/tool-search — `ENABLE_TOOL_SEARCH`
//!           unset|true|false|auto|auto:N, 서드파티 `ANTHROPIC_BASE_URL`이면 꺼짐,
//!           `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS`면 꺼짐, 서버의 `alwaysLoad: true`는 처음부터.
//!   Codex   tool_search가 늘 켜져 있고 MCP 도구는 늘 미뤄진다.
//!   Kiro    https://kiro.dev/docs/mcp/tool-search/ — IDE·CLI V3는 기본 지연, V2는
//!           `toolSearch.enabled`이고 임계값(창의 5% 또는 50,000 토큰)을 넘을 때만.
//!
//! 서버를 실행하지 않아 스키마 총량을 모르므로, 총량이 필요한 판정(`auto`, Kiro V2 임계값)은
//! ctxmeter가 총량을 모를 때처럼 "시작 때 들어감"으로 센다.

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::Loading;

const FIRST_PARTY_HOST: &str = "api.anthropic.com";

/// URL의 host. 형식이 아니면 `None`(ctxmeter: 파싱 실패 = 서드파티).
fn host(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = host_port.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

pub fn claude(env: &HashMap<String, String>, always_load: bool) -> Loading {
    let is_set = |key: &str| env.get(key).is_some_and(|v| !v.is_empty());
    if always_load || is_set("CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS") {
        return Loading::Startup;
    }
    let setting = env
        .get("ENABLE_TOOL_SEARCH")
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let is_auto = setting == "auto"
        || setting
            .strip_prefix("auto:")
            .is_some_and(|n| n.parse::<f64>().is_ok());
    match setting.as_str() {
        "false" => return Loading::Startup,
        "true" => return Loading::Deferred,
        // 창 대비 비율을 재려면 스키마 총량이 필요하다.
        _ if is_auto => return Loading::Startup,
        _ => {}
    }
    match env.get("ANTHROPIC_BASE_URL").filter(|v| !v.is_empty()) {
        Some(url) if host(url).as_deref() != Some(FIRST_PARTY_HOST) => Loading::Startup,
        _ => Loading::Deferred,
    }
}

/// Kiro `cli.json`의 평평한 점 키.
pub fn kiro(settings: &Map<String, Value>) -> Loading {
    let engine = settings
        .get("chat.agentEngine")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if engine != "v1" && engine != "v2" {
        return Loading::Deferred;
    }
    if settings.get("toolSearch.enabled") != Some(&Value::Bool(true)) {
        return Loading::Startup;
    }
    let threshold =
        |key: &str, default: f64| settings.get(key).and_then(Value::as_f64).unwrap_or(default);
    // 임계값이 둘 다 0이면 늘 켜진다. 아니면 총량을 몰라 넘는지 알 수 없다.
    if threshold("toolSearch.minPct", 5.0) == 0.0
        && threshold("toolSearch.minTokens", 50_000.0) == 0.0
    {
        Loading::Deferred
    } else {
        Loading::Startup
    }
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
    fn claude_defers_by_default_and_honours_switches() {
        assert_eq!(claude(&env(&[]), false), Loading::Deferred);
        assert_eq!(claude(&env(&[]), true), Loading::Startup);
        assert_eq!(
            claude(&env(&[("ENABLE_TOOL_SEARCH", "false")]), false),
            Loading::Startup
        );
        assert_eq!(
            claude(&env(&[("ENABLE_TOOL_SEARCH", " TRUE ")]), false),
            Loading::Deferred
        );
        assert_eq!(
            claude(&env(&[("ENABLE_TOOL_SEARCH", "auto:5")]), false),
            Loading::Startup
        );
        assert_eq!(
            claude(
                &env(&[("CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS", "1")]),
                false
            ),
            Loading::Startup
        );
    }

    #[test]
    fn claude_third_party_base_url_turns_tool_search_off() {
        let base = |url| claude(&env(&[("ANTHROPIC_BASE_URL", url)]), false);
        assert_eq!(base("https://api.anthropic.com"), Loading::Deferred);
        assert_eq!(
            base("https://user@API.anthropic.com:443/v1"),
            Loading::Deferred
        );
        assert_eq!(base("https://proxy.example.com"), Loading::Startup);
        assert_eq!(base("not a url"), Loading::Startup);
        // 명시적 true는 base URL보다 먼저다.
        let both = env(&[
            ("ANTHROPIC_BASE_URL", "https://proxy"),
            ("ENABLE_TOOL_SEARCH", "true"),
        ]);
        assert_eq!(claude(&both, false), Loading::Deferred);
    }

    #[test]
    fn kiro_v3_defers_and_v2_needs_tool_search() {
        let settings = |v: Value| v.as_object().unwrap().clone();
        assert_eq!(kiro(&settings(json!({}))), Loading::Deferred);
        assert_eq!(
            kiro(&settings(json!({"chat.agentEngine": "v3"}))),
            Loading::Deferred
        );
        assert_eq!(
            kiro(&settings(json!({"chat.agentEngine": "V2"}))),
            Loading::Startup
        );
        assert_eq!(
            kiro(&settings(
                json!({"chat.agentEngine": "v2", "toolSearch.enabled": true})
            )),
            Loading::Startup
        );
        assert_eq!(
            kiro(&settings(json!({
                "chat.agentEngine": "v2", "toolSearch.enabled": true,
                "toolSearch.minPct": 0, "toolSearch.minTokens": 0
            }))),
            Loading::Deferred
        );
    }
}
