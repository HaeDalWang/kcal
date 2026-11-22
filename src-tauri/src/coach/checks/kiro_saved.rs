//! Saved local default profile only. Unknown engine, custom agents and external settings are deferred.
use super::{CheckKind, CoachCheck, Inputs, saved_advice::card};
use crate::usage::{
    Agent, paths,
    sources::{RunInput, is_executable, run},
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::SystemTime,
};

// Official version floors, confirmed 2026-11-21: Kiro facts 145–168, 496–498.
const V3_SEARCH_MIN: (u32, u32, u32) = (2, 31, 1);
const V3_COMPACTION_MIN: (u32, u32, u32) = (3, 3, 0);
// Documented defaults, confirmed 2026-11-21 (facts 158–168, 102); not kcal tuning.
const DEFAULT_SEARCH_MIN_PCT: f64 = 5.0;
const DEFAULT_SEARCH_MIN_TOKENS: f64 = 50_000.0;
const DEFAULT_COMPACTION_THRESHOLD: f64 = 80.0;
type Version = (u32, u32, u32);

fn object(path: &Path) -> Option<Map<String, Value>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .ok()?
            .as_object()
            .cloned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Map::new()),
        Err(_) => None,
    }
}

// Serde skips env, headers and other server values without deserializing or retaining their contents.
#[derive(Deserialize)]
struct Server {
    #[serde(default)]
    disabled: Option<bool>,
}
#[derive(Deserialize)]
struct Servers {
    #[serde(default, rename = "mcpServers")]
    servers: BTreeMap<String, Server>,
}
fn servers(path: &Path) -> Option<BTreeMap<String, Server>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(serde_json::from_slice::<Servers>(&bytes).ok()?.servers),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(BTreeMap::new()),
        Err(_) => None,
    }
}

pub(super) fn list(input: &Inputs) -> Vec<CoachCheck> {
    if !cfg!(test)
        && ["KIRO_HOME", "KIRO_DEFAULT_SETTINGS"]
            .iter()
            .any(|k| std::env::var_os(k).is_some())
    {
        return vec![];
    }
    evaluate_with_probe(
        input.home,
        super::saved_workspace::kiro(input).as_deref(),
        || {
            // Tests inject versions or fake executables, never the real PATH CLI.
            if cfg!(test) {
                None
            } else {
                installed_version(
                    input.home,
                    std::env::var_os("PATH").as_deref(),
                    input.tuning.kiro_timeout(),
                )
            }
        },
    )
    .unwrap_or_default()
}

pub(super) fn installed_version(
    home: &Path,
    path_env: Option<&std::ffi::OsStr>,
    timeout: std::time::Duration,
) -> Option<Version> {
    let cli = paths::kiro_cli_candidates(home, path_env)
        .into_iter()
        .find(|p| is_executable(p))?;
    type Cache = BTreeMap<PathBuf, (SystemTime, Option<Version>)>;
    static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
    let modified = std::fs::metadata(&cli).ok()?.modified().ok()?;
    // ponytail: serialize version probes; per-path locks if multiple CLI installations contend.
    let mut cache = CACHE.lock().ok()?;
    if let Some((mtime, version)) = cache.get(&cli)
        && *mtime == modified
    {
        return *version;
    }
    let version = probe_version(&cli, timeout);
    cache.insert(cli, (modified, version));
    version
}

fn probe_version(cli: &Path, timeout: std::time::Duration) -> Option<Version> {
    // list() runs on the existing blocking worker, outside the UI/runtime thread.
    let output =
        tauri::async_runtime::block_on(run(cli, &["--version"], timeout, RunInput::default()))
            .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_version(std::str::from_utf8(&output.stdout).ok()?)
}

pub(super) fn parse_version(stdout: &str) -> Option<Version> {
    let pattern =
        regex::Regex::new(r"(?:^|[^\w.])v?([0-9]+)\.([0-9]+)\.([0-9]+)(?:$|[^\w.\-])").ok()?;
    stdout.lines().find_map(|line| {
        let parts = pattern.captures(line)?;
        Some((
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
            parts[3].parse().ok()?,
        ))
    })
}

#[cfg(test)]
pub(super) fn evaluate(
    home: &Path,
    workspace: Option<&Path>,
    version: Option<Version>,
) -> Option<Vec<CoachCheck>> {
    evaluate_with_probe(home, workspace, || version)
}

pub(super) fn evaluate_with_probe(
    home: &Path,
    workspace: Option<&Path>,
    probe: impl FnOnce() -> Option<Version>,
) -> Option<Vec<CoachCheck>> {
    let global_file = home.join(".kiro/settings/cli.json");
    let global = object(&global_file)?;
    let engine = global.get("chat.agentEngine")?.as_str()?;
    if !matches!(engine, "v2" | "v3") {
        return None;
    }
    let workspace = workspace?;
    let local_file = workspace.join(".kiro/settings/cli.json");
    let local = object(&local_file)?;
    // Selected custom agent / registry / cloud settings cannot be resolved from cli.json alone.
    if global.keys().chain(local.keys()).any(|k| {
        k.to_ascii_lowercase().contains("agent")
            && !matches!(
                k.as_str(),
                "chat.agentEngine" | "chat.enableAutoAgentUpgrade"
            )
            || k.to_ascii_lowercase().contains("cloud")
            || k.to_ascii_lowercase().contains("registry")
    }) {
        return None;
    }
    if has_agents(&home.join(".kiro/agents"))? || has_agents(&workspace.join(".kiro/agents"))? {
        return None;
    }
    let effective = |key: &str| local.get(key).or_else(|| global.get(key));
    let file_for = |key: &str| -> PathBuf {
        if local.contains_key(key) || !global.contains_key(key) {
            local_file.clone()
        } else {
            global_file.clone()
        }
    };
    let search = boolean(effective("toolSearch.enabled"))?;
    let compaction = boolean(effective("chat.disableAutoCompaction"))?;
    let min_pct = numeric(effective("toolSearch.minPct"))?;
    let min_tokens = numeric(effective("toolSearch.minTokens"))?;
    let threshold = numeric(effective("compaction.autoCompactionThreshold"))?;
    let mut cards = vec![];
    let search_candidate =
        (engine == "v2" && search != Some(true)) || (engine == "v3" && search == Some(false));
    let search_has_mcp = if search_candidate {
        let mut mcp = servers(&home.join(".kiro/settings/mcp.json"))?;
        mcp.extend(servers(&workspace.join(".kiro/settings/mcp.json"))?);
        mcp.values().any(|s| s.disabled != Some(true))
    } else {
        false
    };
    let version = if engine == "v3" && (search_has_mcp || compaction == Some(true)) {
        probe()
    } else {
        None
    };
    if search_has_mcp && (engine == "v2" || version.is_some_and(|v| v >= V3_SEARCH_MIN)) {
        let topic = if engine == "v3" {
            "tool-search-v3"
        } else {
            "tool-search-v2"
        };
        let evidence = vec![
            "\"toolSearch.enabled\": true".into(),
            format!(
                "toolSearch.minPct = {}",
                min_pct.unwrap_or(DEFAULT_SEARCH_MIN_PCT)
            ),
            format!(
                "toolSearch.minTokens = {}",
                min_tokens.unwrap_or(DEFAULT_SEARCH_MIN_TOKENS)
            ),
        ];
        cards.push(card(
            topic,
            CheckKind::A,
            Agent::Kiro,
            &file_for("toolSearch.enabled"),
            evidence,
            &["https://kiro.dev/docs/mcp/tool-search/"],
        ));
    }
    if engine == "v3" && compaction == Some(true) && version.is_some_and(|v| v >= V3_COMPACTION_MIN)
    {
        cards.push(card(
            "auto-compaction",
            CheckKind::D,
            Agent::Kiro,
            &file_for("chat.disableAutoCompaction"),
            vec![
                "\"chat.disableAutoCompaction\": false".into(),
                format!(
                    "compaction.autoCompactionThreshold = {}%",
                    threshold.unwrap_or(DEFAULT_COMPACTION_THRESHOLD)
                ),
            ],
            &[
                "https://kiro.dev/docs/compaction",
                "https://kiro.dev/docs/cli/chat/settings#compaction",
            ],
        ));
    }
    Some(cards)
}

fn has_agents(dir: &Path) -> Option<bool> {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => Some(entries.next().transpose().ok()?.is_some()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

fn numeric(value: Option<&Value>) -> Option<Option<f64>> {
    match value {
        None => Some(None),
        Some(v) => v.as_f64().filter(|n| n.is_finite() && *n >= 0.0).map(Some),
    }
}

fn boolean(value: Option<&Value>) -> Option<Option<bool>> {
    match value {
        None => Some(None),
        Some(v) => v.as_bool().map(Some),
    }
}
