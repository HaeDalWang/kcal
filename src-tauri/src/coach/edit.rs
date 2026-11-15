//! 설정 파일 내용 바꾸기(ctxmeter `config-edit.js`, 0014). 모두 문자열 → 문자열이라 쓰기 전에 미리 보여 줄 수 있다.
//!
//! 바꾸는 값은 언제나 하나다. section을 만들거나 지우거나, 순서를 바꾸지 않는다. 애매하면 고치지 않고 거부한다.
//! 오류 문자열은 사용자에게 그대로 보여 주는 거부 이유다.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::i18n::pick;
use crate::tr;

/// 바꾼 결과와 화면에 보여 줄 바꿀 줄 전/후.
#[derive(Debug, PartialEq)]
pub struct Edit {
    pub content: String,
    pub changed: bool,
    /// 사람이 찾을 위치(`[plugins."x"] 아래 3번째 줄`, `mcpServers."x".disabled`).
    pub location: String,
    /// 바꾸기 전 줄. 새로 넣는 줄이면 `None`.
    pub before: Option<String>,
    pub after: String,
}

static SECTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\[([^\]]+)\]").unwrap());
static TRIPLE_QUOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""""|'''"#).unwrap());
static QUOTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""(?:[^"\\]|\\.)*"|'[^']*'"#).unwrap());
static ENABLED_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*enabled\s*=").unwrap());
static ENABLED_BOOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*enabled\s*=\s*)(true|false)(\s*(?:#.*)?)$").unwrap());

/// 진짜 table 헤더인 줄 번호. 여러 줄 배열(`  ["a", "b"],`)·여러 줄 문자열 안에서 `[`로 시작하는 줄은
/// 값의 일부라 헤더로 보지 않는다(그렇게 보면 section이 일찍 끝나 키가 두 번 들어간다).
fn header_indices(lines: &[&str]) -> Vec<usize> {
    let mut headers = Vec::new();
    let mut depth: usize = 0;
    let mut open_string: Option<&str> = None;
    for (index, line) in lines.iter().enumerate() {
        if let Some(quote) = open_string {
            if line.contains(quote) {
                open_string = None;
            }
            continue;
        }
        if depth == 0 && SECTION.is_match(line) {
            headers.push(index);
            continue;
        }
        let triples: Vec<&str> = TRIPLE_QUOTE.find_iter(line).map(|m| m.as_str()).collect();
        if triples.len() % 2 == 1 {
            open_string = triples.last().copied();
            continue;
        }
        let code = QUOTED.replace_all(line, "");
        let code = code.split('#').next().unwrap_or("");
        let opened = code.matches('[').count();
        let closed = code.matches(']').count();
        depth = (depth + opened).saturating_sub(closed);
    }
    headers
}

/// `[section]` 안의 `enabled`를 `enabled`로 둔다. 있으면 값만 바꾸고(주석·들여쓰기 유지), 없으면 헤더 바로 아래에 넣는다.
/// section이 없거나 두 번 나오면, `enabled` 값이 true/false가 아니면 거부한다. 다른 바이트는 그대로다.
pub fn set_toml_enabled(content: &str, section: &str, enabled: bool) -> Result<Edit, String> {
    let lines: Vec<&str> = content.split('\n').collect();
    let headers = header_indices(&lines);
    let matching: Vec<usize> = headers
        .iter()
        .copied()
        .filter(|&i| SECTION.captures(lines[i]).is_some_and(|c| &c[1] == section))
        .collect();
    let start = match matching.as_slice() {
        [] => {
            return Err(tr!(
                "[{section}] section이 파일에 없어 바꿀 것이 없습니다.",
                "There's no [{section}] section in the file, so there's nothing to change."
            ));
        }
        [one] => *one,
        _ => {
            return Err(tr!(
                "[{section}] section이 두 번 이상 나와 어느 쪽을 바꿀지 정할 수 없습니다.",
                "The [{section}] section appears more than once, so kcal can't tell which to change."
            ));
        }
    };
    // section은 다음 헤더(하위 `.env` 포함)에서 끝난다. 그래서 하위 section의 키는 건드리지 않는다.
    let end = headers
        .iter()
        .copied()
        .find(|&i| i > start)
        .unwrap_or(lines.len());
    let value = if enabled { "true" } else { "false" };
    let existing = (start + 1..end).find(|&i| ENABLED_KEY.is_match(lines[i]));
    let mut next: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
    let Some(target) = existing else {
        // 헤더 줄이 CRLF면 넣는 줄도 CRLF.
        let ending = if lines[start].ends_with('\r') {
            "\r"
        } else {
            ""
        };
        let inserted = format!("enabled = {value}");
        next.insert(start + 1, format!("{inserted}{ending}"));
        return Ok(Edit {
            content: next.join("\n"),
            changed: true,
            location: tr!(
                "[{section}] 바로 아래 (새 줄, {}번째 줄)",
                "right below [{section}] (new line, line {})",
                start + 2
            ),
            before: None,
            after: inserted,
        });
    };
    let line = lines[target];
    let caps = ENABLED_BOOL.captures(line).ok_or_else(|| {
        tr!(
            "[{section}]의 enabled 값이 true/false가 아니라 고치지 않습니다.",
            "enabled in [{section}] isn't true/false, so kcal won't edit it."
        )
    })?;
    let replaced = format!("{}{value}{}", &caps[1], &caps[3]);
    let changed = replaced != line;
    next[target] = replaced.clone();
    Ok(Edit {
        content: if changed {
            next.join("\n")
        } else {
            content.to_owned()
        },
        changed,
        location: tr!(
            "[{section}] 안 {}번째 줄",
            "[{section}], line {}",
            target + 1
        ),
        before: Some(line.trim_end_matches('\r').to_owned()),
        after: replaced.trim_end_matches('\r').to_owned(),
    })
}

/// JSON 파일을 읽는다. 주석·끝 쉼표(JSONC)·객체가 아닌 최상위는 거부한다.
fn parse_object(content: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(content) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(pick("JSON 최상위가 객체가 아니라 고치지 않습니다.", "The JSON top level isn't an object, so kcal won't edit it.").into()),
        Err(_) => Err(pick(
            "올바른 JSON이 아니라(주석·끝 쉼표 등) 고치지 않습니다. 아무것도 바뀌지 않았습니다.",
            "It isn't valid JSON (comments, trailing commas, ...), so kcal won't edit it. Nothing was changed.",
        )
        .into()),
    }
}

/// 2칸 들여쓰기로 다시 쓰고 끝 줄바꿈을 원본과 맞춘다. 다시 읽어 `expected`와 같은지(바꾼 키만 다른지) 확인한다.
fn serialize_checked(
    original: &str,
    expected: &Map<String, Value>,
    keys_in_order: &[String],
) -> Result<String, String> {
    let mut text = serde_json::to_string_pretty(expected).map_err(|_| {
        pick(
            "JSON을 다시 쓰지 못했습니다.",
            "Couldn't write the JSON back.",
        )
        .to_owned()
    })?;
    if original.ends_with('\n') {
        text.push('\n');
    }
    let reparsed = parse_object(&text)?;
    let reparsed_keys: Vec<&String> = reparsed.keys().collect();
    let original_keys: Vec<&String> = keys_in_order.iter().collect();
    if &reparsed != expected || reparsed_keys != original_keys {
        return Err(pick("다시 읽은 결과가 예상과 달라(키 순서 등) 고치지 않습니다.", "Reading it back didn't match what was expected (key order etc.), so kcal won't edit it.").into());
    }
    Ok(text)
}

/// Kiro `mcp.json`: `mcpServers.<name>.disabled`를 둔다. 파일에 있는 서버만 바꾼다.
pub fn set_json_server_disabled(content: &str, name: &str, disabled: bool) -> Result<Edit, String> {
    let original = parse_object(content)?;
    let servers = original
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or(pick(
            "mcpServers 객체가 없어 고치지 않습니다.",
            "There's no mcpServers object, so kcal won't edit it.",
        ))?;
    let server = servers
        .get(name)
        .and_then(Value::as_object)
        .ok_or_else(|| {
            tr!(
                "{name} 서버가 mcpServers에 없어 고치지 않습니다.",
                "Server {name} isn't in mcpServers, so kcal won't edit it."
            )
        })?;
    let current = server.get("disabled").cloned();
    let location = format!("mcpServers.\"{name}\".disabled");
    let before = Some(format!(
        "\"disabled\": {}",
        current
            .as_ref()
            .map_or(pick("(없음)", "(none)").into(), Value::to_string)
    ));
    let after = format!("\"disabled\": {disabled}");
    if current == Some(Value::Bool(disabled)) || (!disabled && current.is_none()) {
        return Ok(Edit {
            content: content.to_owned(),
            changed: false,
            location,
            before,
            after,
        });
    }
    let mut next_server = server.clone();
    next_server.insert("disabled".into(), Value::Bool(disabled));
    let mut next_servers = servers.clone();
    next_servers.insert(name.to_owned(), Value::Object(next_server));
    let mut next = original.clone();
    next.insert("mcpServers".into(), Value::Object(next_servers));
    let keys: Vec<String> = original.keys().cloned().collect();
    Ok(Edit {
        content: serialize_checked(content, &next, &keys)?,
        changed: true,
        location,
        before,
        after,
    })
}

/// Claude `settings.json`: `enabledPlugins.<id>`를 둔다. 이미 적힌 플러그인만 바꾼다(새로 넣으면 설치가 된다).
pub fn set_json_plugin_enabled(content: &str, id: &str, enabled: bool) -> Result<Edit, String> {
    let original = parse_object(content)?;
    let plugins = original
        .get("enabledPlugins")
        .and_then(Value::as_object)
        .ok_or(pick(
            "enabledPlugins 객체가 없어 고치지 않습니다.",
            "There's no enabledPlugins object, so kcal won't edit it.",
        ))?;
    let current = plugins.get(id).ok_or_else(|| {
        tr!(
            "{id} 플러그인이 enabledPlugins에 없어 고치지 않습니다.",
            "Plugin {id} isn't in enabledPlugins, so kcal won't edit it."
        )
    })?;
    let location = format!("enabledPlugins.\"{id}\"");
    let before = Some(format!("\"{id}\": {current}"));
    let after = format!("\"{id}\": {enabled}");
    if current == &Value::Bool(enabled) {
        return Ok(Edit {
            content: content.to_owned(),
            changed: false,
            location,
            before,
            after,
        });
    }
    let mut next_plugins = plugins.clone();
    next_plugins.insert(id.to_owned(), Value::Bool(enabled));
    let mut next = original.clone();
    next.insert("enabledPlugins".into(), Value::Object(next_plugins));
    let keys: Vec<String> = original.keys().cloned().collect();
    Ok(Edit {
        content: serialize_checked(content, &next, &keys)?,
        changed: true,
        location,
        before,
        after,
    })
}

// ---------- 여러 키를 한 번에 바꾸는 JSON 편집(최적화 점검, 0017) ----------

/// JSON 경로 한 칸.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seg {
    Key(String),
    Index(usize),
}

pub fn key(name: &str) -> Seg {
    Seg::Key(name.to_owned())
}

/// 바꿀 것 하나. `Set`은 없는 중간 객체를 만든다(객체가 아닌 값이 막으면 거부). `Remove`는 있는 값만 지운다.
#[derive(Clone, Debug, PartialEq)]
pub enum JsonOp {
    Set(Vec<Seg>, Value),
    Remove(Vec<Seg>),
}

impl JsonOp {
    fn path(&self) -> &[Seg] {
        match self {
            Self::Set(path, _) | Self::Remove(path) => path,
        }
    }
}

/// 화면에 보일 바뀌는 것 하나. 새 키면 `before`가, 지우면 `after`가 `None`(값은 한 줄 JSON).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonChange {
    pub location: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct JsonEdit {
    pub content: String,
    pub changes: Vec<JsonChange>,
}

static PLAIN_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap());

/// `modelSettings."claude-opus-5".effortLevel`, `hooks.SessionStart[0].hooks[1]`.
pub fn location(path: &[Seg]) -> String {
    let mut text = String::new();
    for seg in path {
        match seg {
            Seg::Key(k) if PLAIN_KEY.is_match(k) => {
                if !text.is_empty() {
                    text.push('.');
                }
                text.push_str(k);
            }
            Seg::Key(k) => {
                if !text.is_empty() {
                    text.push('.');
                }
                text.push_str(&Value::String(k.clone()).to_string());
            }
            Seg::Index(i) => text.push_str(&format!("[{i}]")),
        }
    }
    text
}

fn get<'a>(root: &'a Value, path: &[Seg]) -> Option<&'a Value> {
    path.iter().try_fold(root, |value, seg| match seg {
        Seg::Key(k) => value.as_object()?.get(k),
        Seg::Index(i) => value.as_array()?.get(*i),
    })
}

/// `path`의 부모(없으면 만든다)를 고친다.
fn parent_mut<'a>(
    root: &'a mut Value,
    path: &[Seg],
    create: bool,
) -> Result<&'a mut Value, String> {
    let blocked = || {
        tr!(
            "{}이(가) 객체가 아니라 고치지 않습니다.",
            "{} isn't an object, so kcal won't edit it.",
            location(path)
        )
    };
    let mut value = root;
    for seg in &path[..path.len() - 1] {
        value = match seg {
            Seg::Key(k) => {
                let map = value.as_object_mut().ok_or_else(blocked)?;
                if create && !map.contains_key(k) {
                    map.insert(k.clone(), Value::Object(Map::new()));
                }
                map.get_mut(k).ok_or_else(blocked)?
            }
            Seg::Index(i) => value
                .as_array_mut()
                .and_then(|a| a.get_mut(*i))
                .ok_or_else(blocked)?,
        };
    }
    Ok(value)
}

fn apply_op(root: &mut Value, op: &JsonOp) -> Result<(), String> {
    let path = op.path();
    let Some(last) = path.last() else {
        return Err(pick("고칠 위치가 비어 있습니다.", "The place to edit is empty.").into());
    };
    let missing = || {
        tr!(
            "{}이(가) 파일에 없어 고치지 않습니다.",
            "{} isn't in the file, so kcal won't edit it.",
            location(path)
        )
    };
    let parent = parent_mut(root, path, matches!(op, JsonOp::Set(..)))?;
    match (op, last) {
        (JsonOp::Set(_, v), Seg::Key(k)) => {
            parent
                .as_object_mut()
                .ok_or_else(missing)?
                .insert(k.clone(), v.clone());
        }
        (JsonOp::Remove(_), Seg::Key(k)) => {
            // preserve_order에서 `remove`는 뒤 키를 앞으로 옮긴다. 순서를 지키려고 shift_remove.
            parent
                .as_object_mut()
                .and_then(|m| m.shift_remove(k))
                .ok_or_else(missing)?;
        }
        (JsonOp::Remove(_), Seg::Index(i)) => {
            let array = parent
                .as_array_mut()
                .filter(|a| *i < a.len())
                .ok_or_else(missing)?;
            array.remove(*i);
        }
        (JsonOp::Set(..), Seg::Index(_)) => {
            return Err(pick(
                "배열 칸에는 값을 넣지 않습니다.",
                "kcal doesn't put values into array slots.",
            )
            .into());
        }
    }
    Ok(())
}

/// `a` → `b`에서 달라진 경로. 같은 키들의 순서가 바뀌면 오류(순서는 지켜야 한다).
fn diff_paths(
    a: &Value,
    b: &Value,
    at: &mut Vec<Seg>,
    out: &mut Vec<Vec<Seg>>,
) -> Result<(), String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let common_x: Vec<&String> = x.keys().filter(|k| y.contains_key(*k)).collect();
            let common_y: Vec<&String> = y.keys().filter(|k| x.contains_key(*k)).collect();
            if common_x != common_y {
                return Err(pick(
                    "다시 읽은 결과의 키 순서가 달라 고치지 않습니다.",
                    "Reading it back changed the key order, so kcal won't edit it.",
                )
                .into());
            }
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                at.push(Seg::Key(k.clone()));
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => diff_paths(p, q, at, out)?,
                    _ => out.push(at.clone()),
                }
                at.pop();
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                at.push(Seg::Index(i));
                diff_paths(p, q, at, out)?;
                at.pop();
            }
        }
        _ if a != b => out.push(at.clone()),
        _ => {}
    }
    Ok(())
}

/// 달라진 경로가 모두 편집 대상인가: 그 경로 아래, `Set`이 새로 만든 중간 객체, 배열 칸을 지운 그 배열.
fn only_intended(original: &Value, diff: &[Vec<Seg>], ops: &[JsonOp]) -> bool {
    diff.iter().all(|d| {
        ops.iter().any(|op| {
            let path = op.path();
            let is_created_parent =
                matches!(op, JsonOp::Set(..)) && path.starts_with(d) && get(original, d).is_none();
            let is_shrunk_array =
                matches!((op, path.last()), (JsonOp::Remove(_), Some(Seg::Index(_))))
                    && d.as_slice() == &path[..path.len() - 1];
            d.starts_with(path) || is_created_parent || is_shrunk_array
        })
    })
}

/// `ops`를 차례로 적용한다. 이미 그 값인 `Set`은 건너뛴다. 다시 읽어 의도한 경로만 달라졌는지 확인하고,
/// 2칸 들여쓰기로 쓰며 끝 줄바꿈을 원본과 맞춘다. 바뀐 것이 없으면 `changes`가 비고 내용은 원본 그대로다.
pub fn apply_json_ops(content: &str, ops: &[JsonOp]) -> Result<JsonEdit, String> {
    let original = Value::Object(parse_object(content)?);
    let mut next = original.clone();
    let mut changes = Vec::new();
    let mut applied = Vec::new();
    for op in ops {
        let before = get(&next, op.path()).cloned();
        if let JsonOp::Set(_, v) = op
            && before.as_ref() == Some(v)
        {
            continue;
        }
        apply_op(&mut next, op)?;
        changes.push(JsonChange {
            location: location(op.path()),
            before: before.map(|v| v.to_string()),
            after: match op {
                JsonOp::Set(_, v) => Some(v.to_string()),
                JsonOp::Remove(_) => None,
            },
        });
        applied.push(op.clone());
    }
    if changes.is_empty() {
        return Ok(JsonEdit {
            content: content.to_owned(),
            changes,
        });
    }
    let mut text = serde_json::to_string_pretty(&next).map_err(|_| {
        pick(
            "JSON을 다시 쓰지 못했습니다.",
            "Couldn't write the JSON back.",
        )
        .to_owned()
    })?;
    if content.ends_with('\n') {
        text.push('\n');
    }
    let reparsed = Value::Object(parse_object(&text)?);
    let mut diff = Vec::new();
    diff_paths(&original, &reparsed, &mut Vec::new(), &mut diff)?;
    // 직렬화 비교는 키 순서까지 본다(Map의 ==는 순서를 보지 않는다).
    let is_same_order = serde_json::to_vec(&reparsed).ok() == serde_json::to_vec(&next).ok();
    if !is_same_order || !only_intended(&original, &diff, &applied) {
        return Err(pick(
            "다시 읽은 결과가 예상과 달라 고치지 않습니다.",
            "Reading it back didn't match what was expected, so kcal won't edit it.",
        )
        .into());
    }
    Ok(JsonEdit {
        content: text,
        changes,
    })
}

/// FNV-1a 64비트(16자리 hex). 미리보기 뒤 파일이 바뀌었는지 보는 변경 감지용이다. 보안용 해시가 아니다.
pub fn content_hash(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let hash = bytes
        .iter()
        .fold(OFFSET, |h, b| (h ^ u64::from(*b)).wrapping_mul(PRIME));
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "# top comment\nmodel = \"x\"\n\n[plugins.\"a@m\"]\nenabled = true # keep\n\n[plugins.\"b@m\"]\n\n[mcp_servers.one]\ncommand = \"x\"\nargs = [\n  [\"not\", \"a header\"],\n  \"y\",\n]\n[mcp_servers.one.env]\nenabled = \"leave me\"\n";

    /// 바뀐 줄 번호 하나만 다르고 나머지 줄이 같은지.
    fn only_line_differs(before: &str, after: &str, line: usize) {
        let (a, b): (Vec<&str>, Vec<&str>) =
            (before.split('\n').collect(), after.split('\n').collect());
        assert_eq!(a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            if i != line {
                assert_eq!(x, y, "line {i} changed");
            }
        }
    }

    #[test]
    fn toml_replaces_existing_enabled_value_and_keeps_comment() {
        let edit = set_toml_enabled(CONFIG, "plugins.\"a@m\"", false).unwrap();
        assert!(edit.changed);
        assert_eq!(edit.before.as_deref(), Some("enabled = true # keep"));
        assert_eq!(edit.after, "enabled = false # keep");
        only_line_differs(CONFIG, &edit.content, 4);
    }

    #[test]
    fn toml_inserts_enabled_right_below_header_when_missing() {
        let edit = set_toml_enabled(CONFIG, "plugins.\"b@m\"", true).unwrap();
        assert_eq!(edit.before, None);
        let expected = CONFIG.replace("[plugins.\"b@m\"]\n", "[plugins.\"b@m\"]\nenabled = true\n");
        assert_eq!(edit.content, expected);
    }

    #[test]
    fn toml_rejects_missing_or_duplicate_section() {
        assert!(
            set_toml_enabled(CONFIG, "plugins.\"zzz@m\"", false)
                .unwrap_err()
                .contains("없어")
        );
        let twice = format!("{CONFIG}[plugins.\"a@m\"]\nenabled = true\n");
        assert!(
            set_toml_enabled(&twice, "plugins.\"a@m\"", false)
                .unwrap_err()
                .contains("두 번")
        );
    }

    #[test]
    fn toml_ignores_brackets_inside_multiline_array() {
        // 배열 안 `[\"not\", ...]` 줄을 헤더로 보면 section이 거기서 끝나 `.env` 앞에 넣을 자리가 틀어진다.
        let edit = set_toml_enabled(CONFIG, "mcp_servers.one", false).unwrap();
        let expected = CONFIG.replace(
            "[mcp_servers.one]\n",
            "[mcp_servers.one]\nenabled = false\n",
        );
        assert_eq!(edit.content, expected);
        let lines: Vec<&str> = CONFIG.split('\n').collect();
        assert_eq!(header_indices(&lines).len(), 4);
    }

    #[test]
    fn toml_does_not_touch_keys_of_env_subsection() {
        let edit = set_toml_enabled(CONFIG, "mcp_servers.one", false).unwrap();
        assert!(
            edit.content
                .contains("[mcp_servers.one.env]\nenabled = \"leave me\"\n")
        );
        // 하위 section 자체를 고치려 하면 값이 bool이 아니라 거부한다.
        assert!(set_toml_enabled(CONFIG, "mcp_servers.one.env", false).is_err());
    }

    #[test]
    fn toml_keeps_every_other_byte_and_reports_no_change_when_already_set() {
        let edit = set_toml_enabled(CONFIG, "plugins.\"a@m\"", true).unwrap();
        assert!(!edit.changed);
        assert_eq!(edit.content, CONFIG);
        let crlf = CONFIG.replace('\n', "\r\n");
        let edit = set_toml_enabled(&crlf, "plugins.\"b@m\"", true).unwrap();
        assert_eq!(
            edit.content,
            crlf.replace(
                "[plugins.\"b@m\"]\r\n",
                "[plugins.\"b@m\"]\r\nenabled = true\r\n"
            )
        );
    }

    const KIRO: &str = "{\n  \"mcpServers\": {\n    \"zeta\": {\n      \"command\": \"z\"\n    },\n    \"alpha\": {\n      \"command\": \"a\",\n      \"disabled\": false\n    }\n  },\n  \"other\": 1\n}\n";

    #[test]
    fn json_sets_kiro_disabled_keeping_key_order_and_trailing_newline() {
        let edit = set_json_server_disabled(KIRO, "zeta", true).unwrap();
        assert!(edit.changed);
        assert_eq!(
            edit.content,
            "{\n  \"mcpServers\": {\n    \"zeta\": {\n      \"command\": \"z\",\n      \"disabled\": true\n    },\n    \"alpha\": {\n      \"command\": \"a\",\n      \"disabled\": false\n    }\n  },\n  \"other\": 1\n}\n"
        );
        let no_newline = KIRO.trim_end();
        assert!(
            !set_json_server_disabled(no_newline, "alpha", true)
                .unwrap()
                .content
                .ends_with('\n')
        );
        assert!(
            !set_json_server_disabled(KIRO, "alpha", false)
                .unwrap()
                .changed
        );
        assert!(
            !set_json_server_disabled(KIRO, "zeta", false)
                .unwrap()
                .changed
        );
    }

    #[test]
    fn json_sets_claude_enabled_plugins_entry() {
        let settings = "{\n  \"model\": \"x\",\n  \"enabledPlugins\": {\n    \"b@m\": true,\n    \"a@m\": false\n  }\n}\n";
        let edit = set_json_plugin_enabled(settings, "b@m", false).unwrap();
        assert_eq!(
            edit.content,
            settings.replace("\"b@m\": true", "\"b@m\": false")
        );
        assert_eq!(edit.before.as_deref(), Some("\"b@m\": true"));
        assert!(set_json_plugin_enabled(settings, "new@m", true).is_err());
    }

    #[test]
    fn json_rejects_jsonc_and_invalid_json() {
        let jsonc = "{\n  // comment\n  \"mcpServers\": {}\n}\n";
        assert!(
            set_json_server_disabled(jsonc, "x", true)
                .unwrap_err()
                .contains("JSON")
        );
        let trailing = "{\"enabledPlugins\": {\"a@m\": true,}}";
        assert!(set_json_plugin_enabled(trailing, "a@m", false).is_err());
        assert!(set_json_plugin_enabled("[]", "a@m", false).is_err());
    }

    #[test]
    fn json_rejects_names_not_in_the_file() {
        assert!(
            set_json_server_disabled(KIRO, "missing", true)
                .unwrap_err()
                .contains("missing")
        );
    }

    const SETTINGS: &str = "{\n  \"env\": {\n    \"A\": \"1\"\n  },\n  \"effortLevel\": \"high\",\n  \"modelSettings\": {\n    \"claude-opus-5-5\": {\n      \"effortLevel\": \"xhigh\"\n    }\n  },\n  \"hooks\": {\n    \"SessionStart\": [\n      {\n        \"hooks\": [\n          {\n            \"command\": \"a\"\n          },\n          {\n            \"command\": \"b\"\n          }\n        ]\n      }\n    ]\n  }\n}\n";

    fn path(segs: &[&str]) -> Vec<Seg> {
        segs.iter()
            .map(|s| s.parse().map_or_else(|_| key(s), Seg::Index))
            .collect()
    }

    #[test]
    fn json_ops_remove_and_set_nested_keys_keeping_order_and_newline() {
        let ops = [
            JsonOp::Remove(path(&["effortLevel"])),
            JsonOp::Set(
                path(&["modelSettings", "claude-opus-5", "effortLevel"]),
                Value::from("high"),
            ),
        ];
        let edit = apply_json_ops(SETTINGS, &ops).unwrap();
        let expected = SETTINGS
            .replace("  \"effortLevel\": \"high\",\n", "")
            .replace(
                "      \"effortLevel\": \"xhigh\"\n    }\n  },",
                "      \"effortLevel\": \"xhigh\"\n    },\n    \"claude-opus-5\": {\n      \"effortLevel\": \"high\"\n    }\n  },",
            );
        assert_eq!(edit.content, expected);
        assert_eq!(
            edit.changes,
            [
                JsonChange {
                    location: "effortLevel".into(),
                    before: Some("\"high\"".into()),
                    after: None
                },
                JsonChange {
                    location: "modelSettings.\"claude-opus-5\".effortLevel".into(),
                    before: None,
                    after: Some("\"high\"".into())
                },
            ]
        );
    }

    #[test]
    fn json_ops_remove_array_item_and_skip_set_to_same_value() {
        let ops = [JsonOp::Remove(path(&[
            "hooks",
            "SessionStart",
            "0",
            "hooks",
            "0",
        ]))];
        let edit = apply_json_ops(SETTINGS, &ops).unwrap();
        assert!(!edit.content.contains("\"a\"") && edit.content.contains("\"b\""));
        assert_eq!(edit.changes[0].location, "hooks.SessionStart[0].hooks[0]");
        let same = [JsonOp::Set(path(&["env", "A"]), Value::from("1"))];
        let edit = apply_json_ops(SETTINGS, &same).unwrap();
        assert!(edit.changes.is_empty());
        assert_eq!(edit.content, SETTINGS);
    }

    #[test]
    fn json_ops_refuse_missing_keys_blocked_paths_and_jsonc() {
        assert!(apply_json_ops(SETTINGS, &[JsonOp::Remove(path(&["nope"]))]).is_err());
        let blocked = [JsonOp::Set(path(&["effortLevel", "x"]), Value::from(1))];
        assert!(apply_json_ops(SETTINGS, &blocked).is_err());
        assert!(apply_json_ops("{\n  // c\n}\n", &[JsonOp::Remove(path(&["a"]))]).is_err());
    }

    #[test]
    fn only_intended_rejects_changes_outside_the_edited_paths() {
        let original: Value = serde_json::from_str(SETTINGS).unwrap();
        let ops = [JsonOp::Remove(path(&["hooks", "SessionStart", "0"]))];
        assert!(only_intended(
            &original,
            &[path(&["hooks", "SessionStart"])],
            &ops
        ));
        assert!(!only_intended(&original, &[path(&["hooks"])], &ops));
        assert!(!only_intended(&original, &[path(&["env", "A"])], &ops));
        // Set은 없던 중간 객체만 새로 만들 수 있다. 있던 객체 전체가 바뀌면 거부.
        let set = [JsonOp::Set(
            path(&["skillOverrides", "x"]),
            Value::from("off"),
        )];
        assert!(only_intended(&original, &[path(&["skillOverrides"])], &set));
        let set = [JsonOp::Set(path(&["env", "B"]), Value::from("2"))];
        assert!(!only_intended(&original, &[path(&["env"])], &set));
    }

    #[test]
    fn content_hash_is_fnv1a_64_hex() {
        // FNV-1a 64 공개 테스트 값.
        assert_eq!(content_hash(b""), "cbf29ce484222325");
        assert_eq!(content_hash(b"a"), "af63dc4c8601ec8c");
    }
}
