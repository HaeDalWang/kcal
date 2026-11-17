//! Kiro가 세션 시작에 조립한 컨텍스트의 문자 수(`~/.kiro/sessions/<hash>/sess_*/messages.jsonl`).
//!
//! 첫 `session_start` 줄만 JSON으로 읽는다(바이트 substring으로 거른다). `payload.content`는
//! 시스템 조립문(작업 폴더 file tree 포함), `payload.steeringDocuments[].content`는 그 밖에 따로 실린
//! steering 문서다. 둘 다 문자 수만 남기고 내용은 버린다. 토큰 수 기록은 없다(0005).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use super::{fsutil, paths};

const START_MARKER: &[u8] = br#""session_start""#;

#[derive(Debug, PartialEq)]
pub struct KiroSession {
    /// `session.json`의 첫 `workspacePaths`.
    pub cwd: Option<String>,
    /// `contentChars` + steering 문서 문자 수. file tree(`fileTreeChars`)는 content 안에 있어 더하지 않는다.
    pub first_request_chars: u64,
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn chars(text: &Value) -> u64 {
    text.as_str().map_or(0, |s| s.chars().count() as u64)
}

fn start_chars(row: &Value) -> Option<u64> {
    let payload = row.get("payload")?;
    if payload.get("type")?.as_str()? != "session_start" {
        return None;
    }
    let content = payload
        .get("composition")
        .and_then(|c| c.get("contentChars")?.as_u64())
        .unwrap_or_else(|| payload.get("content").map_or(0, chars));
    let steering: u64 = payload
        .get("steeringDocuments")
        .and_then(Value::as_array)
        .map_or(0, |docs| {
            docs.iter().map(|d| d.get("content").map_or(0, chars)).sum()
        });
    Some(content + steering)
}

fn first_start_chars(file: &Path) -> Option<u64> {
    let mut reader = BufReader::new(std::fs::File::open(file).ok()?);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
        if contains(&line, START_MARKER)
            && let Ok(row) = serde_json::from_slice::<Value>(&line)
            && let Some(total) = start_chars(&row)
        {
            return Some(total);
        }
    }
}

/// `session.json`의 `workspacePaths`. 읽지 못하면 빈 목록.
fn workspace_paths(session_dir: &Path) -> Vec<String> {
    fsutil::read_json(&session_dir.join("session.json"))
        .and_then(|v| {
            v.get("workspacePaths")?.as_array().map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(str::to_owned))
                    .collect()
            })
        })
        .unwrap_or_default()
}

/// `sessions/<hash>/<id>/messages.jsonl`만, 최근에 바뀐 순. 세션 폴더 안(`snapshots` 등, 이 Mac에서 폴더의 2/3)은 내려가지 않는다.
pub fn message_files(home: &Path) -> Vec<PathBuf> {
    let subdirs = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|entries| entries.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    };
    let mut files: Vec<(PathBuf, SystemTime)> = subdirs(&paths::kiro_root(home).join("sessions"))
        .iter()
        .flat_map(|hash| subdirs(hash))
        .filter_map(|session| {
            let file = session.join("messages.jsonl");
            let meta = std::fs::metadata(&file).ok().filter(|m| m.is_file())?;
            Some((file, meta.modified().ok()?))
        })
        .collect();
    // 같은 시각이면 경로순(폴더 읽기 순서는 OS마다 다르다).
    files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    files.into_iter().map(|(p, _)| p).collect()
}

/// 가장 최근 세션의 시작 조립 문자 수. `workspace`가 `Some`이면 `workspacePaths`가 그 폴더 안인 세션만.
// ponytail: 최신순으로 `sessions_to_search`개만 연다. 더 오래된 작업 폴더까지 찾아야 하면 색인을 둔다.
pub fn latest(
    home: &Path,
    workspace: Option<&Path>,
    sessions_to_search: usize,
) -> Option<KiroSession> {
    message_files(home)
        .iter()
        .take(sessions_to_search)
        .find_map(|file| {
            let folders = workspace_paths(file.parent()?);
            if let Some(workspace) = workspace
                && !folders.iter().any(|f| Path::new(f).starts_with(workspace))
            {
                return None;
            }
            Some(KiroSession {
                cwd: folders.into_iter().next(),
                first_request_chars: first_start_chars(file)?,
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;

    fn start(content: &str, content_chars: u64, steering: &[&str]) -> String {
        let docs: Vec<Value> = steering.iter().map(|c| json!({"content": c})).collect();
        json!({"payload": {"type": "session_start", "content": content,
            "steeringDocuments": docs,
            "composition": {"schema": 1, "contentChars": content_chars, "fileTreeChars": 3}}})
        .to_string()
    }

    fn session(home: &TempDir, id: &str, folder: &str, messages: &str) {
        home.write(
            &format!(".kiro/sessions/h/{id}/session.json"),
            &json!({"workspacePaths": [folder]}).to_string(),
        );
        home.write(&format!(".kiro/sessions/h/{id}/messages.jsonl"), messages);
    }

    #[test]
    fn sums_content_and_steering_chars_from_first_session_start_only() {
        let home = TempDir::new("kiro-session");
        // 다른 줄은 깨진 JSON이라 파싱했다면 실패한다. 둘째 session_start는 무시한다.
        let text = [
            "{not json message".into(),
            r#"{"payload":{"type":"note","text":"session_start"}}"#.into(),
            start("가나다라", 10, &["가나", "abc"]),
            start("x", 99, &[]),
        ]
        .join("\n");
        session(&home, "sess_a", "/w/app", &text);
        let got = latest(home.path(), None, 50).unwrap();
        assert_eq!(got.cwd.as_deref(), Some("/w/app"));
        assert_eq!(got.first_request_chars, 15);
    }

    #[test]
    fn falls_back_to_content_length_without_composition() {
        let row = json!({"payload": {"type": "session_start", "content": "가나다"}});
        assert_eq!(start_chars(&row), Some(3));
    }

    #[test]
    fn filters_by_workspace_path_components() {
        let home = TempDir::new("kiro-session-filter");
        session(&home, "sess_app", "/w/app/sub", &start("a", 4, &[]));
        session(&home, "sess_other", "/w/other", &start("a", 1, &[]));
        let got = latest(home.path(), Some(Path::new("/w/app")), 50).unwrap();
        assert_eq!(got.first_request_chars, 4);
        assert!(latest(home.path(), Some(Path::new("/w/ap")), 50).is_none());
        assert!(latest(home.path(), Some(Path::new("/nowhere")), 50).is_none());
    }

    #[test]
    fn message_files_skip_files_nested_inside_a_session_folder() {
        let home = TempDir::new("kiro-message-files-depth");
        home.write(".kiro/sessions/h/sess_a/messages.jsonl", "{}");
        home.write(".kiro/sessions/h/sess_a/snapshots/x/messages.jsonl", "{}");
        home.write(".kiro/sessions/h/messages.jsonl", "{}");
        let got = message_files(home.path());
        assert_eq!(
            got,
            vec![home.path().join(".kiro/sessions/h/sess_a/messages.jsonl")]
        );
    }

    /// 이 Mac 실제 HOME에서 목록 시간. `cargo test --release --lib real_kiro_message_files_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "실제 HOME을 읽는다"]
    fn real_kiro_message_files_cost() {
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        let started = std::time::Instant::now();
        let files = message_files(&home);
        println!(
            "kiro message_files: {} files in {:?}",
            files.len(),
            started.elapsed()
        );
    }
}
