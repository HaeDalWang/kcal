//! File-only saved Codex configuration. No auth.json, command execution or automatic edits.
use super::{
    CheckKind, CoachCheck, Inputs,
    saved_advice::card,
    saved_toml::{self, SavedToml},
};
use crate::{
    context::scan::{codex_trusted_projects, trust_key},
    usage::Agent,
};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

// Documentation constants, confirmed 2026-11-21 (facts 11–13, 134–140).
const DEFAULT_DOC_BYTES: u64 = 32768;
const HIGH_EFFORTS: [&str; 2] = ["max", "ultra"];
const DOCS: &str = "https://learn.chatgpt.com/docs/customization/agents-md";
const HOOK_DOCS: &str = "https://learn.chatgpt.com/docs/hooks";

fn system_dir(home: &Path) -> PathBuf {
    if cfg!(test) {
        home.join("etc/codex")
    } else {
        PathBuf::from("/etc/codex")
    }
}

fn codex_home(home: &Path) -> Option<PathBuf> {
    if !cfg!(test) && std::env::var_os("CODEX_HOME").is_some() {
        return None;
    }
    Some(home.join(".codex"))
}

pub(super) fn list(input: &Inputs) -> Vec<CoachCheck> {
    let Some(root) = codex_home(input.home) else {
        return vec![];
    };
    let file = root.join("config.toml");
    let Some(user) = saved_toml::read(&file) else {
        return vec![];
    };
    let mut cards = vec![];
    let Some(system) = saved_toml::read(&system_dir(input.home).join("config.toml")) else {
        return cards;
    };
    let Some(requirements) = saved_toml::read(&system_dir(input.home).join("requirements.toml"))
    else {
        return cards;
    };
    let managed_only = requirements.values.get("allow_managed_hooks_only");
    let has_policy = managed_only.is_some_and(|v| v != &Value::Bool(false));
    let mut effective = system.values;
    effective.extend(user.values.clone());
    // Selected profile/remote policy cannot establish a document budget from these files.
    let has_profile = effective.contains_key("profile");
    let saved_workspace = super::saved_workspace::codex(input);
    let workspace = saved_workspace.as_deref();
    let dirs = workspace.and_then(|w| project_dirs(w, &effective));
    let trusted = codex_trusted_projects(&std::fs::read_to_string(&file).unwrap_or_default());
    let user_values = user.values.clone();
    let mut layers = vec![(root.clone(), user)];
    if let Some(dirs) = &dirs {
        for dir in dirs {
            if !project_trusted(dir, &user_values, &trusted) {
                continue;
            }
            let path = dir.join(".codex");
            let Some(layer) = saved_toml::read(&path.join("config.toml")) else {
                return cards;
            };
            effective.extend(layer.values.clone());
            layers.push((path, layer));
        }
    }
    if !has_profile && !effective.contains_key("profile") {
        cards.extend(effort_card(&layers));
    }
    if !has_profile
        && !effective.contains_key("profile")
        && let Some(dirs) = &dirs
        && let Some(c) = instructions(&root, dirs, &effective)
    {
        cards.push(c);
    }
    if effective.get("features.hooks") == Some(&Value::Bool(true)) {
        for (dir, layer) in layers.into_iter().filter(|_| !has_policy) {
            if dir.ancestors().any(|p| p.join(".git").is_file()) {
                continue;
            }
            cards.extend(duplicate_hooks(&dir, &layer));
        }
        if let Some(layer) = saved_toml::read(&system_dir(input.home).join("config.toml")) {
            cards.extend(duplicate_hooks(&system_dir(input.home), &layer));
        }
    }
    cards
}

// Ruling: nearest explicit trust decision wins; otherwise inherit ancestor trust.
fn project_trusted(
    dir: &Path,
    user: &Map<String, Value>,
    trusted: &std::collections::HashSet<String>,
) -> bool {
    for ancestor in dir.ancestors() {
        let name = ancestor.to_string_lossy();
        let quoted = serde_json::to_string(name.as_ref()).unwrap_or_default();
        for key in [
            format!("projects.{quoted}.trust_level"),
            format!("projects.'{name}'.trust_level"),
        ] {
            if let Some(value) = user.get(&key) {
                return value.as_str() == Some("trusted");
            }
        }
        if trusted.contains(&trust_key(&name)) {
            return true;
        }
    }
    false
}

fn effort_card(layers: &[(PathBuf, SavedToml)]) -> Option<CoachCheck> {
    let keys = ["model_reasoning_effort", "plan_mode_reasoning_effort"];
    // Any project effort override replaces the user-card source as a whole.
    let (dir, layer) = layers
        .iter()
        .rev()
        .find(|(_, layer)| keys.iter().any(|key| layer.values.contains_key(*key)))?;
    let efforts = keys
        .into_iter()
        .filter_map(|key| {
            let value = layer.values.get(key)?.as_str()?;
            HIGH_EFFORTS
                .contains(&value)
                .then(|| format!("{key} = \"{value}\""))
        })
        .collect::<Vec<_>>();
    if efforts.is_empty() {
        return None;
    }
    Some(card(
        "reasoning-effort",
        CheckKind::D,
        Agent::Codex,
        &dir.join("config.toml"),
        efforts,
        &["https://learn.chatgpt.com/docs/models"],
    ))
}

fn names(value: Option<&Value>, default: &[&str]) -> Option<Vec<String>> {
    match value {
        None => Some(default.iter().map(|s| (*s).into()).collect()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                let name = v.as_str()?;
                (!name.is_empty()
                    && Path::new(name).components().count() == 1
                    && !matches!(name, "." | ".."))
                .then(|| name.into())
            })
            .collect(),
        _ => None,
    }
}

fn project_dirs(workspace: &Path, settings: &Map<String, Value>) -> Option<Vec<PathBuf>> {
    let markers = names(settings.get("project_root_markers"), &[".git"])?;
    if markers.is_empty() {
        return Some(vec![workspace.into()]);
    }
    let root = workspace
        .ancestors()
        .find(|dir| markers.iter().any(|m| dir.join(m).exists()));
    let Some(root) = root else {
        return Some(vec![workspace.into()]);
    };
    let mut dirs = workspace
        .ancestors()
        .take_while(|p| p.starts_with(root))
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    dirs.reverse();
    Some(dirs)
}

fn selected(dir: &Path, fallback: &[String]) -> Option<Option<(PathBuf, u64)>> {
    for name in ["AGENTS.override.md", "AGENTS.md"]
        .into_iter()
        .map(String::from)
        .chain(fallback.iter().cloned())
    {
        let file = dir.join(name);
        match std::fs::read(&file) {
            Ok(bytes) => {
                let text = std::str::from_utf8(&bytes).ok()?;
                if !text.trim().is_empty() {
                    return Some(Some((file, bytes.len() as u64)));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    Some(None)
}

fn instructions(
    home: &Path,
    dirs: &[PathBuf],
    settings: &Map<String, Value>,
) -> Option<CoachCheck> {
    let limit = match settings.get("project_doc_max_bytes") {
        None => DEFAULT_DOC_BYTES,
        Some(v) => v.as_u64()?,
    };
    let fallback = names(settings.get("project_doc_fallback_filenames"), &[])?;
    let _global = selected(home, &[])?;
    let files = dirs
        .iter()
        .map(|d| selected(d, &fallback))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let sum = files.iter().try_fold(0u64, |s, (_, n)| s.checked_add(*n))?;
    if sum <= limit {
        return None;
    }
    let individual = files.iter().any(|(_, n)| *n > limit);
    let mut evidence = vec![
        format!("project_doc_max_bytes = {limit}"),
        format!(
            "{}: {sum} bytes",
            if individual {
                "individual-over"
            } else {
                "combined-only"
            }
        ),
    ];
    evidence.extend(
        files
            .iter()
            .map(|(p, n)| format!("{}: {n} bytes", p.display())),
    );
    Some(card(
        "instructions-size",
        CheckKind::A,
        Agent::Codex,
        &files.first()?.0,
        evidence,
        &[DOCS],
    ))
}

fn known_group(group: &Map<String, Value>) -> bool {
    group
        .keys()
        .all(|k| matches!(k.as_str(), "matcher" | "hooks"))
        && group.get("matcher").is_none_or(Value::is_string)
}

fn known_handler(handler: &Value) -> bool {
    handler.as_object().is_some_and(|h| {
        h.keys().all(|k| {
            matches!(
                k.as_str(),
                "type"
                    | "command"
                    | "timeout"
                    | "async"
                    | "statusMessage"
                    | "commandWindows"
                    | "command_windows"
                    | "additionalContextLimit"
            )
        }) && h.get("type").and_then(Value::as_str) == Some("command")
            && h.get("command")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
            && h.get("timeout").is_none_or(Value::is_u64)
            && h.get("async").is_none_or(Value::is_boolean)
            && h.get("statusMessage").is_none_or(Value::is_string)
            && h.get("commandWindows").is_none_or(Value::is_string)
            && h.get("command_windows").is_none_or(Value::is_string)
            && h.get("additionalContextLimit").is_none_or(Value::is_u64)
    })
}

fn duplicate_hooks(dir: &Path, saved: &SavedToml) -> Option<CoachCheck> {
    let json = crate::context::fsutil::read_json(&dir.join("hooks.json"))?;
    if json
        .as_object()?
        .keys()
        .any(|k| !matches!(k.as_str(), "hooks" | "description"))
    {
        return None;
    }
    let hooks = json.get("hooks")?.as_object()?;
    let mut events = vec![];
    for (event, groups) in &saved.hooks {
        let Some(other) = hooks.get(event).and_then(Value::as_array) else {
            continue;
        };
        for group in groups.as_array()? {
            let g = group.as_object()?;
            if !known_group(g) {
                continue;
            }
            for candidate in other
                .iter()
                .filter_map(Value::as_object)
                .filter(|g| known_group(g))
            {
                if g.get("matcher") != candidate.get("matcher") {
                    continue;
                }
                let (Some(a), Some(b)) = (
                    g.get("hooks").and_then(Value::as_array),
                    candidate.get("hooks").and_then(Value::as_array),
                ) else {
                    continue;
                };
                if a.iter().filter(|h| known_handler(h)).any(|h| b.contains(h)) {
                    events.push(event.clone());
                }
            }
        }
    }
    events.sort();
    events.dedup();
    if events.is_empty() {
        return None;
    }
    events.push(dir.join("hooks.json").to_string_lossy().into_owned());
    let mut result = card(
        "duplicate-hooks",
        CheckKind::B,
        Agent::Codex,
        &dir.join("config.toml"),
        events,
        &[HOOK_DOCS],
    );
    // Multiple layers can produce separate cards, with stable path-qualified IDs.
    result.id = format!("{}:{}", result.id, dir.display());
    Some(result)
}
