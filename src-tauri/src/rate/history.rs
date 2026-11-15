//! Claude 한도 기록(`app_data_dir()/limit-history.jsonl`). 공식 경로(API) 조회가 성공할 때마다 한 줄.
//!
//! 한 줄: `{"at":RFC3339,"fiveHour":{"usedPercent":n,"resetsAt":RFC3339|null},"sevenDay":{..}}`.
//! 숫자와 시각만 쓴다. 토큰·인증 정보·계정 id는 이 모듈에 들어오지 않는다.
//! 직전 줄과 두 창의 사용률이 같고 리셋 시각이 같은 창(tuning `resetTolSecs` 이내)이면 쓰지 않는다.
// ponytail: 크기 상한 없음. 줄당 약 150바이트, 값이 바뀔 때만 써서 5분 조회 기준 하루 최대 약 43KB(연 16MB).
// 커지면 오래된 줄을 잘라 내는 회전을 넣는다.

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::usage::{LimitWindow, WindowId};

pub const FILE_NAME: &str = "limit-history.jsonl";
/// 직전 줄을 찾으려고 끝에서 읽는 양. 한 줄보다 충분히 크다.
const TAIL_BYTES: u64 = 4096;

pub fn file(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(FILE_NAME)
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowRecord {
    pub used_percent: f64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub resets_at: Option<OffsetDateTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub five_hour: Option<WindowRecord>,
    pub seven_day: Option<WindowRecord>,
}

impl HistoryEntry {
    /// 5시간·주간 창이 하나도 없으면 `None`.
    pub fn from_windows(at: OffsetDateTime, windows: &[LimitWindow]) -> Option<Self> {
        let pick = |id: WindowId| {
            windows.iter().find(|w| w.id == id).map(|w| WindowRecord {
                used_percent: w.used_percent,
                resets_at: w.resets_at,
            })
        };
        let entry = Self {
            at,
            five_hour: pick(WindowId::FiveHour),
            seven_day: pick(WindowId::SevenDay),
        };
        (entry.five_hour.is_some() || entry.seven_day.is_some()).then_some(entry)
    }
}

fn same_reset(a: Option<OffsetDateTime>, b: Option<OffsetDateTime>, reset_tol_secs: f64) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => ((a - b).as_seconds_f64()).abs() <= reset_tol_secs,
        (None, None) => true,
        _ => false,
    }
}

fn same_window(a: Option<WindowRecord>, b: Option<WindowRecord>, reset_tol_secs: f64) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            a.used_percent == b.used_percent && same_reset(a.resets_at, b.resets_at, reset_tol_secs)
        }
        (None, None) => true,
        _ => false,
    }
}

/// 같은 값·같은 창이면 기록하지 않는다(시각은 비교하지 않는다).
pub fn is_duplicate(previous: &HistoryEntry, next: &HistoryEntry, reset_tol_secs: f64) -> bool {
    same_window(previous.five_hour, next.five_hour, reset_tol_secs)
        && same_window(previous.seven_day, next.seven_day, reset_tol_secs)
}

fn last_entry(path: &Path) -> Option<HistoryEntry> {
    let mut handle = std::fs::File::open(path).ok()?;
    let size = handle.metadata().ok()?.len();
    handle
        .seek(SeekFrom::Start(size.saturating_sub(TAIL_BYTES)))
        .ok()?;
    let mut tail = Vec::new();
    handle.read_to_end(&mut tail).ok()?;
    String::from_utf8_lossy(&tail)
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line).ok())
}

/// 기록했으면 `true`, 직전 줄과 같아 건너뛰었으면 `false`.
pub fn record(path: &Path, entry: &HistoryEntry, reset_tol_secs: f64) -> std::io::Result<bool> {
    if last_entry(path).is_some_and(|last| is_duplicate(&last, entry, reset_tol_secs)) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    line.push('\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())?;
    Ok(true)
}

/// 파일 순서대로의 5시간 창 기록. 파일이 없거나 깨진 줄은 건너뛴다.
pub fn five_hour_records(path: &Path) -> Vec<WindowRecord> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<HistoryEntry>(line).ok())
        .filter_map(|e| e.five_hour)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use time::macros::datetime;

    const TOL: f64 = 600.0;

    fn window(id: WindowId, used: f64, resets_at: OffsetDateTime) -> LimitWindow {
        LimitWindow {
            id,
            used_percent: used,
            resets_at: Some(resets_at),
            reset_passed: false,
            used: None,
            limit: None,
        }
    }

    fn entry(at: OffsetDateTime, five: f64, five_reset: OffsetDateTime) -> HistoryEntry {
        HistoryEntry::from_windows(
            at,
            &[
                window(WindowId::FiveHour, five, five_reset),
                window(WindowId::SevenDay, 40.0, datetime!(2026-11-10 0:00 UTC)),
            ],
        )
        .unwrap()
    }

    #[test]
    fn skips_same_values_in_same_window_and_records_changes() {
        let dir = TempDir::new("rate-history");
        let path = file(&dir.path().join("app-data"));
        let reset = datetime!(2026-11-04 15:00 UTC);
        assert!(
            record(
                &path,
                &entry(datetime!(2026-11-04 12:00 UTC), 10.0, reset),
                TOL
            )
            .unwrap()
        );
        // 값 같음, 리셋 시각 흔들림 1초 → 같은 창 → 쓰지 않는다.
        let jitter = reset + time::Duration::seconds(1);
        assert!(
            !record(
                &path,
                &entry(datetime!(2026-11-04 12:05 UTC), 10.0, jitter),
                TOL
            )
            .unwrap()
        );
        // 값이 바뀜 → 쓴다.
        assert!(
            record(
                &path,
                &entry(datetime!(2026-11-04 12:10 UTC), 12.0, reset),
                TOL
            )
            .unwrap()
        );
        // 같은 값이라도 새 창이면 쓴다.
        let next = datetime!(2026-11-04 20:00 UTC);
        assert!(
            record(
                &path,
                &entry(datetime!(2026-11-04 15:10 UTC), 12.0, next),
                TOL
            )
            .unwrap()
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 3);
        let windows = crate::rate::claude::windows(&path, TOL);
        assert_eq!(windows.len(), 2);
    }

    #[test]
    fn reset_tolerance_decides_whether_a_shifted_reset_is_the_same_window() {
        let reset = datetime!(2026-11-04 15:00 UTC);
        let at = datetime!(2026-11-04 12:00 UTC);
        let shifted = reset + time::Duration::seconds(30);
        let (a, b) = (entry(at, 10.0, reset), entry(at, 10.0, shifted));
        assert!(is_duplicate(&a, &b, TOL));
        assert!(!is_duplicate(&a, &b, 5.0));
    }

    #[test]
    fn line_holds_only_times_and_percents() {
        let e = entry(
            datetime!(2026-11-04 12:00 UTC),
            10.0,
            datetime!(2026-11-04 15:00 UTC),
        );
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            serde_json::json!({
                "at": "2026-11-04T12:00:00Z",
                "fiveHour": {"usedPercent": 10.0, "resetsAt": "2026-11-04T15:00:00Z"},
                "sevenDay": {"usedPercent": 40.0, "resetsAt": "2026-11-10T00:00:00Z"}
            })
        );
        assert!(HistoryEntry::from_windows(datetime!(2026-11-04 12:00 UTC), &[]).is_none());
    }
}
