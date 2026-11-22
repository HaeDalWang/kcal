//! Agent-specific saved workspace metadata; never substitute another agent's workspace.
use super::Inputs;
use crate::context::fsutil;
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct Meta {
    #[serde(rename = "type")]
    kind: String,
    payload: Cwd,
}
#[derive(Deserialize)]
struct Cwd {
    cwd: Option<PathBuf>,
}

pub(super) fn codex(input: &Inputs) -> Option<PathBuf> {
    if cfg!(test) {
        return input.records.workspace.clone();
    }
    let root = input.home.join(".codex/sessions");
    session_files(&root, input.tuning.shape_sessions_to_search)
        .into_iter()
        .find_map(|file| {
            // Only the first metadata line; do not parse messages or auth.
            let mut reader = BufReader::new(std::fs::File::open(file).ok()?);
            let mut first = String::new();
            reader.read_line(&mut first).ok()?;
            let meta: Meta = serde_json::from_str(&first).ok()?;
            (meta.kind == "session_meta")
                .then_some(meta.payload.cwd)
                .flatten()
                .filter(|p| p.is_absolute() && p.is_dir())
        })
}

pub(super) fn session_files(root: &Path, limit: usize) -> Vec<PathBuf> {
    let mut files = vec![];
    if dated_files(root, 0, limit, &mut files).is_some() {
        return files;
    }
    let mut files = fsutil::walk_files(root, &|p| p.extension().is_some_and(|e| e == "jsonl"));
    sort_by_modified(&mut files);
    files.truncate(limit);
    files
}

fn sort_by_modified(files: &mut [PathBuf]) {
    files.sort_by_cached_key(|p| {
        std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok())
    });
}

fn dated_files(dir: &Path, depth: usize, limit: usize, files: &mut Vec<PathBuf>) -> Option<()> {
    if files.len() >= limit {
        return Some(());
    }
    let mut entries = std::fs::read_dir(dir)
        .ok()?
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
    if depth == 3 {
        let mut day = vec![];
        for entry in entries {
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                day.push(path);
            }
        }
        sort_by_modified(&mut day);
        files.extend(day.into_iter().take(limit - files.len()));
        return Some(());
    }
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_str()?;
        let width = if depth == 0 { 4 } else { 2 };
        if name.len() != width
            || !name.bytes().all(|b| b.is_ascii_digit())
            || !entry.file_type().ok()?.is_dir()
        {
            return None;
        }
        dated_files(&entry.path(), depth + 1, limit, files)?;
        if files.len() >= limit {
            break;
        }
    }
    Some(())
}

pub(super) fn kiro(input: &Inputs) -> Option<PathBuf> {
    if cfg!(test) {
        return input.records.workspace.clone();
    }
    #[derive(Deserialize)]
    struct Workspace {
        #[serde(rename = "workspacePaths")]
        paths: Vec<PathBuf>,
    }
    crate::context::kiro_session::message_files(input.home)
        .into_iter()
        .take(input.tuning.shape_sessions_to_search)
        .find_map(|messages| {
            let bytes = std::fs::read(messages.parent()?.join("session.json")).ok()?;
            let workspace: Workspace = serde_json::from_slice(&bytes).ok()?;
            workspace
                .paths
                .into_iter()
                .find(|p| p.is_absolute() && p.is_dir())
        })
}
