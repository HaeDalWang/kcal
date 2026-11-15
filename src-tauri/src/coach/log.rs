//! 적용 기록(`app_data_dir()/coach-log.jsonl`, 0014). 적용·되돌리기마다 한 줄. 6층 전후 비교의 "처방한 날" 기록이다.
//!
//! 시각·대상·파일 경로·백업 경로·켬/끔·토큰·적용 뒤 해시만 쓴다. 설정 파일 내용은 넣지 않는다.
// ponytail: 크기 상한 없음. 사용자가 누를 때만 한 줄(약 400바이트)이라 회전은 필요해지면 넣는다.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;

use crate::usage::Agent;

pub const FILE_NAME: &str = "coach-log.jsonl";

pub fn file(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(FILE_NAME)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoachAction {
    Apply,
    Undo,
}

fn agent_from_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Agent, D::Error> {
    let id = String::deserialize(deserializer)?;
    Agent::from_id(&id).ok_or_else(|| serde::de::Error::custom("unknown agent"))
}

/// 기록 한 줄이자 `apply_switch`·`undo_switch`·`get_coach_log`의 응답. TS 짝은 `ipc.ts` `AppliedSwitch`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedSwitch {
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub action: CoachAction,
    pub target: String,
    #[serde(deserialize_with = "agent_from_id")]
    pub agent: Agent,
    pub name: String,
    pub file: String,
    /// 적용 전 원본 복사본. 되돌리기 기록은 되돌린 적용의 백업.
    pub backup: String,
    /// 이 기록 뒤의 상태(켬 = true).
    pub enabled: bool,
    /// 적용할 때 측정한 시작 토큰. 모르면 `None`.
    pub tokens: Option<u64>,
    /// 이 기록이 쓴 뒤 파일 내용의 해시(`edit::content_hash`).
    pub after_hash: String,
    /// 앱 없이 되돌리는 명령(백업을 원본 위에 복사).
    pub undo_command: String,
    /// 되돌리기 기록이면 되돌린 적용 기록 ID.
    pub undo_of: Option<String>,
    /// 이 적용이 새로 만든 파일(규칙을 스킬로 옮기면 그 `SKILL.md` 실제 경로, 0018 작업 B). 없으면 줄에 넣지 않는다
    /// (예전 기록 줄이 그대로 읽힌다).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// 같은 적용이 함께 바꾼 다른 파일("프로젝트에서만 켜기", 0022). `file`·`backup`은 사용자 설정이다.
    /// 비면 줄에 넣지 않는다(예전 기록 줄이 그대로 읽힌다).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<LoggedFile>,
}

/// 함께 바꾼 파일 하나. TS 짝은 `ipc.ts` `LoggedFile`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggedFile {
    pub file: String,
    /// 적용 전 원본 복사본. `None`이면 이 적용이 새로 만든 파일이다(되돌리면 지운다).
    pub backup: Option<String>,
    pub after_hash: String,
    /// 이 적용이 파일을 담을 `.claude` 폴더도 새로 만들었다(되돌릴 때 비어 있으면 지운다).
    #[serde(default)]
    pub created_dir: bool,
}

pub fn append(path: &Path, entry: &AppliedSwitch) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    line.push('\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())
}

/// 최근순. 파일이 없거나 깨진 줄은 건너뛴다.
pub fn read(path: &Path) -> Vec<AppliedSwitch> {
    let mut entries: Vec<AppliedSwitch> = std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    entries.reverse();
    entries
}
