//! "그대로 두기"로 숨긴 카드(사용자 결정 2026-11-07). `app_data_dir()/check-dismissed.json`에 카드 id 배열 하나만 둔다.
//!
//! 카드는 id가 목록에 있고 **지금도 추천이 `keep`**일 때만 숨는다(`mark`). 추천이 바뀌면 다시 나온다.
//! 설정 파일은 건드리지 않는다. 쓰기는 0014와 같은 `WRITE_LOCK` 아래 temp→rename.

use std::path::{Path, PathBuf};

use super::{CheckFacts, CoachCheck, KEEP, MAX_ID_LEN, list};
use crate::coach::{WRITE_LOCK, refuse};
use crate::error::AppError;
use crate::i18n::pick;
use crate::tuning::Tuning;

use super::models::RecentRecords;

pub const FILE_NAME: &str = "check-dismissed.json";
/// 점검 id는 종류마다 몇 개뿐이다. 이보다 많으면 파일이 깨진 것으로 본다.
pub const MAX_IDS: usize = 64;

pub fn file(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(FILE_NAME)
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_LEN
}

/// 숨긴 id 목록. 파일이 없으면 빈 목록, 깨졌거나 모양·크기가 다르면 빈 목록(로그 남김).
pub fn read(path: &Path) -> Vec<String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            eprintln!(
                "kcal: 숨긴 카드 목록 읽기 실패({:?}), 빈 목록으로 봅니다",
                e.kind()
            );
            return Vec::new();
        }
    };
    match serde_json::from_str::<Vec<String>>(&text) {
        Ok(ids) if ids.len() <= MAX_IDS && ids.iter().all(|id| is_valid_id(id)) => ids,
        _ => {
            eprintln!("kcal: 숨긴 카드 목록 모양이 달라 빈 목록으로 봅니다");
            Vec::new()
        }
    }
}

/// 같은 폴더 임시 파일에 쓰고 rename한다(반쯤 쓴 파일이 보이지 않게).
fn write(path: &Path, ids: &[String]) -> Result<(), AppError> {
    let io = |e: std::io::Error| {
        eprintln!("kcal: 숨긴 카드 목록 쓰기 실패({:?})", e.kind());
        refuse(crate::tr!(
            "숨긴 카드 목록을 저장하지 못했습니다({:?}).",
            "Couldn't save the hidden card list ({:?}).",
            e.kind()
        ))
    };
    let dir = path.parent().ok_or(AppError::Internal)?;
    std::fs::create_dir_all(dir).map_err(io)?;
    let content = serde_json::to_vec(ids).map_err(|_| AppError::Internal)?;
    let temp = dir.join(format!(".{FILE_NAME}.kcal-{}.tmp", std::process::id()));
    let result = std::fs::write(&temp, content).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(io)
}

/// 숨길 수 있는 카드: 추천이 `keep`이거나, Claude에게 맡기는 카드(0018 작업 A 긴 안내문·작업 B 늘 실리는 규칙:
/// 버튼 "그대로 두기"가 있고 추천은 Claude에게 맡기기).
fn is_dismissable(check: &CoachCheck) -> bool {
    let is_keep_recommended = check
        .options
        .iter()
        .any(|o| o.id == KEEP && o.is_recommended);
    is_keep_recommended
        || matches!(
            check.facts,
            CheckFacts::LongInstructions { .. } | CheckFacts::RulesPaths { .. }
        )
}

/// `is_dismissed`를 채운 새 목록.
pub fn mark(checks: Vec<CoachCheck>, dismissed: &[String]) -> Vec<CoachCheck> {
    checks
        .into_iter()
        .map(|check| CoachCheck {
            is_dismissed: is_dismissable(&check) && dismissed.contains(&check.id),
            ..check
        })
        .collect()
}

/// 지금 판정 목록에 있고 추천이 `keep`인 카드만 숨긴다. 이미 숨겼으면 그대로 성공.
pub fn dismiss(
    home: &Path,
    records: &RecentRecords,
    tuning: &Tuning,
    path: &Path,
    id: &str,
) -> Result<(), AppError> {
    if !is_valid_id(id) {
        return Err(refuse(pick(
            "점검 이름이 올바르지 않습니다.",
            "That check name isn't valid.",
        )));
    }
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Internal)?;
    let checks = list(home, records, tuning);
    let check = checks.iter().find(|c| c.id == id).ok_or_else(|| {
        refuse(pick(
            "지금 점검 목록에 없는 항목입니다. 화면을 다시 여세요.",
            "This item isn't in the current check list. Reopen the screen.",
        ))
    })?;
    if !is_dismissable(check) {
        return Err(refuse(pick(
            "그대로 두기를 추천하는 카드만 숨길 수 있습니다.",
            "Only cards that recommend keeping things as they are can be hidden.",
        )));
    }
    let ids = read(path);
    if ids.iter().any(|d| d == id) {
        return Ok(());
    }
    if ids.len() >= MAX_IDS {
        return Err(refuse(pick(
            "숨긴 카드가 너무 많습니다. 다시 보기를 누른 뒤 하세요.",
            "Too many hidden cards. Press Show again, then try.",
        )));
    }
    let next: Vec<String> = ids.into_iter().chain([id.to_owned()]).collect();
    write(path, &next)
}

/// 숨긴 카드를 모두 다시 보인다.
pub fn restore(path: &Path) -> Result<(), AppError> {
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Internal)?;
    write(path, &[])
}

#[cfg(test)]
mod tests {
    use super::super::tests::{GIT_RULE, MAC_SETTINGS, THIS_MAC, home_with, message, records};
    use super::*;
    use crate::context::fsutil::testing::TempDir;

    const GIT: &str = "C-claude-git-instructions";
    const EFFORT: &str = "D-claude-effort-level";

    fn marked(home: &TempDir, path: &Path) -> Vec<(String, bool)> {
        mark(
            list(home.path(), &records(&THIS_MAC), &Tuning::default()),
            &read(path),
        )
        .into_iter()
        .map(|c| (c.id, c.is_dismissed))
        .collect()
    }

    fn run(home: &TempDir, path: &Path, id: &str) -> Result<(), String> {
        dismiss(
            home.path(),
            &records(&THIS_MAC),
            &Tuning::default(),
            path,
            id,
        )
        .map_err(message)
    }

    #[test]
    fn dismissing_a_keep_card_hides_it_until_restored() {
        let home = home_with("dismiss", MAC_SETTINGS);
        home.write(".claude/rules/git.md", GIT_RULE);
        let path = home.path().join("app-data").join(FILE_NAME);
        run(&home, &path, GIT).unwrap();
        // 두 번 눌러도 그대로 성공, 목록은 한 줄.
        run(&home, &path, GIT).unwrap();
        assert_eq!(read(&path), [GIT]);
        assert_eq!(
            marked(&home, &path),
            [(EFFORT.into(), false), (GIT.into(), true)]
        );
        restore(&path).unwrap();
        assert!(read(&path).is_empty());
        assert!(marked(&home, &path).iter().all(|(_, d)| !d));
    }

    #[test]
    fn dismiss_refuses_cards_not_recommending_keep_and_unknown_ids() {
        let home = home_with("dismiss-refuse", MAC_SETTINGS);
        let path = home.path().join(FILE_NAME);
        assert!(
            run(&home, &path, EFFORT)
                .unwrap_err()
                .contains("그대로 두기")
        );
        assert!(run(&home, &path, GIT).unwrap_err().contains("목록"));
        assert!(run(&home, &path, "").is_err());
        assert!(run(&home, &path, &"x".repeat(MAX_ID_LEN + 1)).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn dismissed_card_shows_again_when_its_recommendation_changes() {
        let home = home_with("dismiss-change", MAC_SETTINGS);
        home.write(".claude/rules/git.md", GIT_RULE);
        let path = home.path().join(FILE_NAME);
        run(&home, &path, GIT).unwrap();
        // 규칙에 서명 줄이 생기면 C 추천이 "내 규칙만 남기기"로 바뀐다.
        home.write(
            ".claude/rules/git.md",
            &format!("{GIT_RULE}\nCo-Authored-By: Me <me@example.com>\n"),
        );
        assert_eq!(read(&path), [GIT]);
        assert_eq!(
            marked(&home, &path),
            [(EFFORT.into(), false), (GIT.into(), false)]
        );
    }

    #[test]
    fn broken_or_oversized_file_reads_as_empty_list() {
        let dir = TempDir::new("dismiss-broken");
        let path = dir.path().join(FILE_NAME);
        for content in [
            "not json".to_owned(),
            "{\"ids\": []}".to_owned(),
            "[1, 2]".to_owned(),
            "[\"\"]".to_owned(),
            serde_json::to_string(&vec!["a"; MAX_IDS + 1]).unwrap(),
        ] {
            std::fs::write(&path, &content).unwrap();
            assert!(read(&path).is_empty(), "{content}");
        }
        assert!(read(&dir.path().join("missing.json")).is_empty());
    }
}
