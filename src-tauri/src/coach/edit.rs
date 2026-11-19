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
pub(crate) fn header_indices(lines: &[&str]) -> Vec<usize> {
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
                "[{section}] section이 파일에 없어 바꿀 것이 없어요.",
                "There's no [{section}] section in the file, so there's nothing to change."
            ));
        }
        [one] => *one,
        _ => {
            return Err(tr!(
                "[{section}] section이 두 번 이상 나와 어느 쪽을 바꿀지 정할 수 없어요.",
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
            "[{section}]의 enabled 값이 true/false가 아니라 고치지 않아요.",
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

static MCP_ROOT_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:\[\s*mcp_servers\s*\]|mcp_servers\s*[.=])").unwrap());

/// 프로젝트 `.codex/config.toml`의 `[section]`에 `enabled = true`를 둔다(Codex "프로젝트에서만 켜기", 0022).
/// section이 있으면 `set_toml_enabled`와 같고, 없으면 파일 끝에 새 section을 덧붙인다. 파일이 `mcp_servers`를 표 헤더 없이
/// 인라인·점 키로 적었으면(같은 표를 두 번 정의해 Codex가 설정을 못 읽게 될 수 있어) 거부한다.
pub fn enable_toml_section(content: &str, section: &str) -> Result<Edit, String> {
    let lines: Vec<&str> = content.split('\n').collect();
    let has_section = header_indices(&lines)
        .iter()
        .any(|&i| SECTION.captures(lines[i]).is_some_and(|c| &c[1] == section));
    if has_section {
        return set_toml_enabled(content, section, true);
    }
    if lines.iter().any(|l| MCP_ROOT_KEY.is_match(l)) {
        return Err(pick(
            "이 파일이 mcp_servers를 [mcp_servers.이름] 꼴이 아닌 방식으로 적고 있어 고치지 않아요.",
            "This file writes mcp_servers in a form other than [mcp_servers.name], so kcal won't edit it.",
        )
        .into());
    }
    let ending = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut next = content.to_owned();
    if !next.is_empty() && !next.ends_with('\n') {
        next.push_str(ending);
    }
    if !next.trim().is_empty() {
        next.push_str(ending);
    }
    next.push_str(&format!("[{section}]{ending}enabled = true{ending}"));
    Ok(Edit {
        content: next,
        changed: true,
        location: tr!(
            "파일 끝에 새 [{section}] section",
            "a new [{section}] section at the end of the file"
        ),
        before: None,
        after: "enabled = true".to_owned(),
    })
}

/// JSON 파일을 읽는다. 주석·끝 쉼표(JSONC)·객체가 아닌 최상위는 거부한다.
fn parse_object(content: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(content) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(pick("JSON 최상위가 객체가 아니라 고치지 않아요.", "The JSON top level isn't an object, so kcal won't edit it.").into()),
        Err(_) => Err(pick(
            "올바른 JSON이 아니라(주석·끝 쉼표 등) 고치지 않아요. 아무것도 바뀌지 않았어요.",
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
            "JSON을 다시 쓰지 못했어요.",
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
        return Err(pick("다시 읽은 결과가 예상과 달라(키 순서 등) 고치지 않아요.", "Reading it back didn't match what was expected (key order etc.), so kcal won't edit it.").into());
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
            "mcpServers 객체가 없어 고치지 않아요.",
            "There's no mcpServers object, so kcal won't edit it.",
        ))?;
    let server = servers
        .get(name)
        .and_then(Value::as_object)
        .ok_or_else(|| {
            tr!(
                "{name} 서버가 mcpServers에 없어 고치지 않아요.",
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
            "enabledPlugins 객체가 없어 고치지 않아요.",
            "There's no enabledPlugins object, so kcal won't edit it.",
        ))?;
    let current = plugins.get(id).ok_or_else(|| {
        tr!(
            "{id} 플러그인이 enabledPlugins에 없어 고치지 않아요.",
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
            "{}이(가) 객체가 아니라 고치지 않아요.",
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
        return Err(pick("고칠 위치가 비어 있어요.", "The place to edit is empty.").into());
    };
    let missing = || {
        tr!(
            "{}이(가) 파일에 없어 고치지 않아요.",
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
                "배열 칸에는 값을 넣지 않아요.",
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
                    "다시 읽은 결과의 키 순서가 달라 고치지 않아요.",
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
            "JSON을 다시 쓰지 못했어요.",
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
            "다시 읽은 결과가 예상과 달라 고치지 않아요.",
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
#[path = "edit_tests.rs"]
mod tests;
