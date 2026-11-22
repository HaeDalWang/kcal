//! Conservative saved-settings reader. Unsupported TOML is unknown, never a default.
//! ponytail: single-line scalar/array values and tables only; use a full TOML parser when the dependency scope permits it.
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Default)]
pub(super) struct SavedToml {
    pub values: Map<String, Value>,
    pub hooks: Map<String, Value>,
}

fn value(raw: &str) -> Option<Value> {
    if raw.len() >= 2
        && raw.starts_with('\'')
        && raw.ends_with('\'')
        && !raw[1..raw.len() - 1].contains('\'')
    {
        return Some(Value::String(raw[1..raw.len() - 1].into()));
    }
    let value: Value = serde_json::from_str(raw).ok()?;
    if !supported_value(&value) {
        return None;
    }
    Some(value)
}

fn uncomment(line: &str) -> Option<&str> {
    let mut quote = None;
    let mut escape = false;
    for (i, c) in line.char_indices() {
        if escape {
            escape = false;
        } else if c == '\\' && quote == Some('"') {
            escape = true;
        } else if quote == Some(c) {
            quote = None;
        } else if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c);
        } else if quote.is_none() && c == '#' {
            return Some(line[..i].trim());
        }
    }
    quote.is_none().then_some(line.trim())
}

pub(super) fn read(path: &Path) -> Option<SavedToml> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(SavedToml::default()),
        Err(_) => None,
    }
}

pub(super) fn parse(text: &str) -> Option<SavedToml> {
    let mut saved = SavedToml::default();
    let mut section = String::new();
    let mut group: Option<Map<String, Value>> = None;
    let mut handler: Option<Map<String, Value>> = None;
    let mut event = String::new();
    let mut ignored_array = false;
    let mut array_keys = std::collections::HashSet::new();
    let mut seen = std::collections::HashSet::new();
    for raw in text.lines() {
        let line = uncomment(raw)?;
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            ignored_array = false;
            array_keys.clear();
            if line.starts_with("[[hooks.") && line.ends_with("]]") {
                let name = &line[8..line.len() - 2];
                if name.ends_with(".hooks") {
                    if name.strip_suffix(".hooks")? != event || group.is_none() {
                        return None;
                    }
                    flush_handler(&mut group, &mut handler);
                    handler = Some(Map::new());
                } else {
                    flush_group(&mut saved, &event, &mut group, &mut handler);
                    if name.contains('.') || name.is_empty() {
                        return None;
                    }
                    event = name.into();
                    group = Some(Map::new());
                }
                section = "hooks".into();
                continue;
            }
            flush_group(&mut saved, &event, &mut group, &mut handler);
            if line.starts_with("[[") && line.ends_with("]]") {
                section = line[2..line.len() - 2].trim().into();
                if !valid_key(&section) {
                    return None;
                }
                ignored_array = true;
                continue;
            }
            if !line.ends_with(']') || line.starts_with("[[") {
                return None;
            }
            section = line[1..line.len() - 1].trim().into();
            if !valid_key(&section) || !seen.insert(format!("[{section}]")) {
                return None;
            }
            continue;
        }
        let (key, raw_value) = line.split_once('=')?;
        let key = key.trim();
        if !valid_key(key) {
            return None;
        }
        // Do not deserialize MCP environment values or unrelated configuration values.
        if section.starts_with("mcp_servers.")
            || (section.is_empty() && key.starts_with("mcp_servers."))
        {
            continue;
        }
        let parsed = value(raw_value.trim())?;
        if ignored_array {
            if !array_keys.insert(key.to_owned()) {
                return None;
            }
            continue;
        }
        if section == "hooks" && group.is_some() {
            let target = handler.as_mut().or(group.as_mut())?;
            if target.insert(key.into(), parsed).is_some() {
                return None;
            }
        } else {
            let full = if section.is_empty() {
                key.into()
            } else {
                format!("{section}.{key}")
            };
            if saved.values.insert(full, parsed).is_some() {
                return None;
            }
        }
    }
    flush_group(&mut saved, &event, &mut group, &mut handler);
    Some(saved)
}

fn flush_handler(group: &mut Option<Map<String, Value>>, handler: &mut Option<Map<String, Value>>) {
    if let (Some(group), Some(handler)) = (group.as_mut(), handler.take()) {
        group
            .entry("hooks")
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .expect("only arrays inserted")
            .push(Value::Object(handler));
    }
}

fn flush_group(
    saved: &mut SavedToml,
    event: &str,
    group: &mut Option<Map<String, Value>>,
    handler: &mut Option<Map<String, Value>>,
) {
    flush_handler(group, handler);
    if let Some(group) = group.take() {
        saved
            .hooks
            .entry(event)
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .expect("only arrays inserted")
            .push(Value::Object(group));
    }
}

fn valid_key(key: &str) -> bool {
    static KEY: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"^(?:[A-Za-z0-9_-]+|"(?:[^"\\]|\\.)*"|'[^']*')(?:\s*\.\s*(?:[A-Za-z0-9_-]+|"(?:[^"\\]|\\.)*"|'[^']*'))*$"#).unwrap()
    });
    KEY.is_match(key)
}

fn supported_value(value: &Value) -> bool {
    match value {
        Value::Null | Value::Object(_) => false,
        Value::Array(items) => items.iter().all(supported_value),
        _ => true,
    }
}
