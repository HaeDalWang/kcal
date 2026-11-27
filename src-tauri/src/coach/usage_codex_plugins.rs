//! Codex plugin 사용 근거: 도구 입력에서 설치 cache 아래 SKILL.md를 읽은 호출만 센다.
use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::super::Key;
use crate::context::paths;
use crate::metrics::unix_secs;

static SKILL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([^/\s\"'`]+)/([^/\s\"'`]+)/([^/\s\"'`]+)/skills/(?:[^\s\"'`]+/)?SKILL\.md(?:$|[\s\"'`),;])"#).unwrap()
});
static READ: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\b(?:cat|head|tail|sed|awk|rg|grep|more|less|Get-Content)\s|\b(?:readFileSync|readFile|read_text|read_to_string|read_file|open)\s*\()").unwrap()
});

fn commands(value: &Value) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    object
        .iter()
        .flat_map(|(key, value)| {
            if matches!(key.as_str(), "cmd" | "command" | "code") {
                value.as_str().map(str::to_owned).into_iter().collect()
            } else {
                Vec::new()
            }
        })
        .collect()
}

pub(super) fn line_uses(line: &[u8], home: &Path, since: f64) -> Vec<(Key, Option<String>, f64)> {
    if !crate::rate::codex::contains(line, b"SKILL.md") {
        return Vec::new();
    }
    let Ok(row) = serde_json::from_slice::<Value>(line) else {
        return Vec::new();
    };
    let Some(at) = row["timestamp"]
        .as_str()
        .and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok())
        .map(unix_secs)
        .filter(|&at| at >= since)
    else {
        return Vec::new();
    };
    if row["type"] != "response_item" {
        return Vec::new();
    }
    let payload = &row["payload"];
    let code = match payload["type"].as_str() {
        Some("custom_tool_call") if payload["name"] == "exec" => payload["input"]
            .as_str()
            .map(str::to_owned)
            .into_iter()
            .collect(),
        Some("function_call") => {
            let args = payload["arguments"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok());
            args.as_ref().map(commands).unwrap_or_default()
        }
        _ => Vec::new(),
    };
    plugin_ids(code, home)
        .into_iter()
        .map(|plugin| {
            let id = payload["call_id"]
                .as_str()
                .map(|id| format!("plugin:{plugin}:{id}"));
            (Key::Plugin(plugin), id, at)
        })
        .collect()
}

fn plugin_ids(code: Vec<String>, home: &Path) -> HashSet<String> {
    let root = paths::codex_root(home)
        .join("plugins/cache")
        .to_string_lossy()
        .replace('\\', "/")
        + "/";
    let mut plugins = HashSet::new();
    for code in code {
        if !READ.is_match(&code) {
            continue;
        }
        let normalized = code.replace("\\\\", "/").replace('\\', "/");
        for rest in normalized.split(&root).skip(1) {
            if let Some(c) = SKILL
                .captures(rest)
                .filter(|c| c.get(0).is_some_and(|m| m.start() == 0))
            {
                plugins.insert(format!("{}@{}", &c[2], &c[1]));
            }
        }
    }
    plugins
}
