//! Codex rollout 기록에서 한도 읽기(`sources.rs`의 대체 경로).

use super::*;

/// 가장 최근에 쓰인 rollout 몇 개에서 가장 새 `rate_limits`. rollout이 하나도 없으면 `Ok(None)`.
// ponytail: `.jsonl.zst` 압축 rollout은 읽지 않는다(0004 열린 것). 필요하면 zstd crate.
pub(super) fn codex_from_log(
    home: &Path,
    now: OffsetDateTime,
    rollouts_checked: usize,
) -> Result<Option<Reading>, Fail> {
    let mut rollouts = Vec::new();
    walk_rollouts(&paths::codex_sessions_dir(home), &mut rollouts);
    if rollouts.is_empty() {
        return Ok(None);
    }
    rollouts.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    let latest = rollouts
        .iter()
        .take(rollouts_checked)
        .filter_map(|(_, file)| latest_rate_limits(file))
        .max_by_key(|(at, _)| *at);
    let Some((observed_at, limits)) = latest else {
        return fail(pick(
            "rate_limits가 있는 rollout 없음",
            "no rollout with rate_limits",
        ));
    };
    Ok(Some(Reading {
        observed_at: Some(observed_at),
        windows: parse::codex_rollout_windows(&limits, now)?,
    }))
}

fn walk_rollouts(dir: &Path, found: &mut Vec<(SystemTime, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            walk_rollouts(&path, found);
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("rollout-") && name.ends_with(".jsonl") {
            found.push((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), path));
        }
    }
}

/// 줄 단위로 읽는다(세션 로그는 수백 MB가 될 수 있다). 표시 문자열이 없는 줄은 JSON 파싱하지 않는다.
fn latest_rate_limits(file: &Path) -> Option<(OffsetDateTime, Value)> {
    let mut reader = BufReader::new(File::open(file).ok()?);
    let mut line = Vec::new();
    let mut latest: Option<(OffsetDateTime, Value)> = None;
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let Ok(text) = std::str::from_utf8(&line) else {
            continue;
        };
        if !text.contains(CODEX_RATE_LIMITS_MARK) {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        if let Some((at, limits)) = parse::codex_rollout_line(&row)
            && latest.as_ref().is_none_or(|(prev, _)| at > *prev)
        {
            latest = Some((at, limits.clone()));
        }
    }
    latest
}
