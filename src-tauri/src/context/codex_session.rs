//! Codex가 실제로 보낸 요청의 크기(rollout `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`).
//!
//! 세 줄만 JSON으로 읽는다: `session_meta`(작업 폴더, `base_instructions` 바이트), 첫 `turn_context`(`model`),
//! 첫 `token_count`(`info.last_token_usage.input_tokens`, `info.model_context_window`). 메시지·도구 출력 줄은 바이트 substring 검사로 걸러
//! 파싱하지 않고, `base_instructions` 내용은 바이트 수만 남기고 버린다. `.jsonl.zst`는 건너뛴다.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use super::{fsutil, paths};

const META_MARKER: &[u8] = br#""type":"session_meta""#;
const TOKEN_COUNT_MARKER: &[u8] = br#""type":"token_count""#;
const TURN_CONTEXT_MARKER: &[u8] = br#""type":"turn_context""#;

#[derive(Debug, Default, PartialEq)]
pub struct CodexSession {
    /// 세션 작업 폴더(`session_meta.payload.cwd`).
    pub cwd: Option<String>,
    /// `base_instructions` 바이트(Codex 내장 시스템 지시문).
    pub system_bytes: Option<u64>,
    /// 첫 요청의 실제 입력 토큰. 첫 사용자 메시지가 들어 있다.
    pub first_request_tokens: Option<u64>,
    /// 첫 `turn_context.payload.model`.
    pub model: Option<String>,
    /// 같은 `token_count`의 `info.model_context_window`(Codex가 기록한 창 크기).
    pub context_window: Option<u64>,
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// `base_instructions`는 문자열이거나 `{text, …}` 객체다.
fn instructions_bytes(base: &Value) -> Option<u64> {
    let text = base.as_str().or_else(|| base.get("text")?.as_str())?;
    Some(text.len() as u64)
}

/// 첫 `info`가 있는 `token_count`: (입력 토큰, 창 크기).
fn first_request(row: &Value) -> Option<(u64, Option<u64>)> {
    if row.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = row.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let info = payload.get("info")?;
    let tokens = info
        .get("last_token_usage")?
        .get("input_tokens")?
        .as_u64()?;
    Some((
        tokens,
        info.get("model_context_window").and_then(Value::as_u64),
    ))
}

fn turn_model(line: &[u8]) -> Option<String> {
    let row: Value = serde_json::from_slice(line).ok()?;
    if row.get("type")?.as_str()? != "turn_context" {
        return None;
    }
    Some(row.get("payload")?.get("model")?.as_str()?.to_owned())
}

/// 한 rollout에서 값을 읽는다. 첫 `session_meta`의 cwd가 `workspace` 밖이면 더 읽지 않고 `None`.
fn read_session(file: &Path, workspace: Option<&Path>) -> Option<CodexSession> {
    let mut reader = BufReader::new(std::fs::File::open(file).ok()?);
    let mut line = Vec::new();
    let mut session: Option<CodexSession> = None;
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        // 기록 중인 세션의 잘린 줄은 파싱에 실패하고 건너뛴다.
        if session.is_none() {
            if !contains(&line, META_MARKER) {
                continue;
            }
            let Ok(row) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if row.get("type").and_then(Value::as_str) != Some("session_meta") {
                continue;
            }
            let payload = row.get("payload").unwrap_or(&Value::Null);
            let cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(workspace) = workspace
                && !cwd
                    .as_deref()
                    .is_some_and(|c| Path::new(c).starts_with(workspace))
            {
                return None;
            }
            session = Some(CodexSession {
                cwd,
                system_bytes: payload
                    .get("base_instructions")
                    .and_then(instructions_bytes),
                ..CodexSession::default()
            });
            continue;
        }
        let Some(s) = session.as_mut() else { continue };
        if s.model.is_none() && contains(&line, TURN_CONTEXT_MARKER) {
            s.model = turn_model(&line);
            continue;
        }
        if !contains(&line, TOKEN_COUNT_MARKER) {
            continue;
        }
        if let Ok(row) = serde_json::from_slice::<Value>(&line)
            && let Some((tokens, window)) = first_request(&row)
        {
            s.first_request_tokens = Some(tokens);
            s.context_window = window;
            break;
        }
    }
    session
}

fn rollout_files(home: &Path) -> Vec<PathBuf> {
    let mut files: Vec<(PathBuf, SystemTime)> =
        fsutil::walk_files(&paths::codex_root(home).join("sessions"), &|p| {
            p.extension() == Some("jsonl".as_ref())
        })
        .into_iter()
        .filter_map(|p| {
            let modified = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((p, modified))
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    files.into_iter().map(|(p, _)| p).collect()
}

/// 가장 최근 rollout의 실측값. `workspace`가 `Some`이면 그 폴더 안에서 시작한 세션만.
/// 오래된 세션은 지금 설정을 말해 주지 않아 최신순 `sessions_to_search`개만 연다.
// ponytail: 더 오래된 폴더까지 찾아야 하면 색인을 둔다.
pub fn latest(
    home: &Path,
    workspace: Option<&Path>,
    sessions_to_search: usize,
) -> Option<CodexSession> {
    rollout_files(home)
        .iter()
        .take(sessions_to_search)
        .find_map(|f| read_session(f, workspace))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;

    fn meta(cwd: &str, base: Value) -> String {
        json!({"type": "session_meta", "payload": {"cwd": cwd, "base_instructions": base}})
            .to_string()
    }

    fn token_count(info: Value) -> String {
        json!({"type": "event_msg", "payload": {"type": "token_count", "info": info}}).to_string()
    }

    fn usage(input: u64) -> Value {
        json!({"last_token_usage": {"input_tokens": input}, "model_context_window": 475_000})
    }

    #[test]
    fn reads_system_bytes_and_first_token_count_without_parsing_message_lines() {
        let home = TempDir::new("codex-session");
        // 메시지 줄은 깨진 JSON이라 파싱했다면 실패한다. 표시 글자가 이스케이프돼 안 맞는 줄도 둔다.
        let quoted = json!({"type": "response_item", "text": r#"{"type":"token_count"}"#});
        let turn = json!({"type": "turn_context", "payload": {"model": "gpt-6.1-sol"}});
        let text = [
            meta("/w/app", json!({"text": "가나다"})),
            "{not json message".into(),
            turn.to_string(),
            token_count(Value::Null),
            quoted.to_string(),
            token_count(usage(29_861)),
            token_count(usage(34_119)),
        ]
        .join("\n");
        home.write(".codex/sessions/2026/11/04/rollout-a.jsonl", &text);
        let got = latest(home.path(), None, 50).unwrap();
        assert_eq!(got.cwd.as_deref(), Some("/w/app"));
        assert_eq!(got.system_bytes, Some(9));
        assert_eq!(got.first_request_tokens, Some(29_861));
        assert_eq!(got.model.as_deref(), Some("gpt-6.1-sol"));
        assert_eq!(got.context_window, Some(475_000));
    }

    #[test]
    fn accepts_string_base_instructions_and_missing_token_count() {
        let home = TempDir::new("codex-session-string");
        home.write(
            ".codex/sessions/2026/11/04/rollout-a.jsonl",
            &meta("/w/app", json!("abcd")),
        );
        let got = latest(home.path(), None, 50).unwrap();
        assert_eq!(
            (got.system_bytes, got.first_request_tokens),
            (Some(4), None)
        );
    }

    #[test]
    fn ignores_zst_and_filters_by_workspace() {
        let home = TempDir::new("codex-session-filter");
        home.write(
            ".codex/sessions/2026/11/04/rollout-other.jsonl",
            &meta("/w/other", json!("xx")),
        );
        home.write(
            ".codex/sessions/2026/11/04/rollout-app.jsonl",
            &format!(
                "{}\n{}",
                meta("/w/app/sub", json!("abcd")),
                token_count(usage(7))
            ),
        );
        home.write(
            ".codex/sessions/2026/11/04/rollout-zst.jsonl.zst",
            &meta("/w/app", json!("zzzzzzzz")),
        );
        let got = latest(home.path(), Some(Path::new("/w/app")), 50).unwrap();
        assert_eq!(
            (got.system_bytes, got.first_request_tokens),
            (Some(4), Some(7))
        );
        // `/w/ap`는 `/w/app`의 상위 이름이 아니다: 경로 단위로 비교한다.
        assert!(latest(home.path(), Some(Path::new("/w/ap")), 50).is_none());
        assert!(latest(home.path(), Some(Path::new("/nowhere")), 50).is_none());
    }

    #[test]
    fn searches_only_the_newest_rollouts_up_to_the_limit() {
        let home = TempDir::new("codex-session-limit");
        let old = home.write(
            ".codex/sessions/2026/11/03/rollout-app.jsonl",
            &meta("/w/app", json!("abcd")),
        );
        let new = home.write(
            ".codex/sessions/2026/11/04/rollout-other.jsonl",
            &meta("/w/other", json!("xx")),
        );
        let touch = |path: &Path, secs: u64| {
            let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs);
            let file = std::fs::File::options().write(true).open(path).unwrap();
            file.set_modified(at).unwrap();
        };
        touch(&old, 1_000);
        touch(&new, 2_000);
        let workspace = Some(Path::new("/w/app"));
        assert!(latest(home.path(), workspace, 1).is_none());
        assert!(latest(home.path(), workspace, 2).is_some());
    }
}
