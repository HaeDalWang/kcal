//! OS 등록 상태가 진실이다. settings.json에 복제하거나 첫 실행에 켜지 않는다(0024).
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::error::AppError;

fn report_error(error: impl std::fmt::Display) -> AppError {
    eprintln!("autostart: {error}");
    AppError::Internal
}

#[tauri::command]
pub async fn get_autostart(app: AppHandle) -> Result<bool, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.autolaunch().is_enabled().map_err(report_error)
    })
    .await
    .map_err(report_error)?
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.autolaunch();
        if enabled {
            #[cfg(target_os = "macos")]
            validate_launch_agent_path(
                &std::env::current_exe()
                    .and_then(std::fs::canonicalize)
                    .map_err(report_error)?,
            )?;
            manager.enable()
        } else {
            manager.disable()
        }
        .map_err(report_error)?;
        manager.is_enabled().map_err(report_error)
    })
    .await
    .map_err(report_error)?
}

/// 플러그인과 같은 canonical executable path로 판단하고 경로 자체는 프론트로 보내지 않는다.
#[tauri::command]
pub async fn get_autostart_path_warning() -> Result<bool, AppError> {
    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let path = std::env::current_exe()
                .and_then(std::fs::canonicalize)
                .map_err(report_error)?;
            Ok(needs_path_warning(&path))
        })
        .await
        .map_err(report_error)?
    }
    #[cfg(not(target_os = "macos"))]
    Ok(false)
}

// auto-launch 0.6은 실행 경로를 XML에 이스케이프 없이 삽입한다.
// 손상된 LaunchAgent를 쓰기 전에 거부한다. 해제는 이 검사 없이 허용한다.
#[cfg(any(target_os = "macos", test))]
fn validate_launch_agent_path(path: &std::path::Path) -> Result<(), AppError> {
    let path = path.to_string_lossy();
    if path.contains(['&', '<']) || path.contains("]]>") || path.chars().any(|c| c.is_control()) {
        return Err(AppError::InvalidInput(crate::i18n::pick(
            "앱 경로를 자동 시작 파일에 저장할 수 없어요. 앱을 /Applications로 옮긴 뒤 다시 켜세요.",
            "The app path cannot be saved in the start-at-login file. Move the app to /Applications and try again.",
        ).into()));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn needs_path_warning(path: &std::path::Path) -> bool {
    !path.starts_with("/Applications")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_for_downloads_and_dev_builds_but_not_applications() {
        for path in [
            "/Users/a/Downloads/kcal.app/Contents/MacOS/kcal",
            "/tmp/target/debug/kcal",
            "/Applications-other/kcal",
        ] {
            assert!(needs_path_warning(std::path::Path::new(path)));
        }
        assert!(!needs_path_warning(std::path::Path::new(
            "/Applications/kcal.app/Contents/MacOS/kcal"
        )));
    }

    #[test]
    fn rejects_xml_path_characters_before_launch_agent_registration() {
        for path in [
            "/Users/a&b/kcal",
            "/tmp/<kcal>",
            "/tmp/kcal]]>",
            "/tmp/kcal\n",
        ] {
            assert!(validate_launch_agent_path(std::path::Path::new(path)).is_err());
        }
        assert!(
            validate_launch_agent_path(std::path::Path::new(
                "/Users/a/Downloads/kcal.app/Contents/MacOS/kcal"
            ))
            .is_ok()
        );
        assert!(
            validate_launch_agent_path(std::path::Path::new("/tmp/space name/한글>/kcal")).is_ok()
        );
    }

    #[test]
    fn errors_use_the_existing_bilingual_serializable_contract() {
        for (lang, expected) in [
            (crate::i18n::Lang::Ko, "내부 작업이 실패했어요."),
            (crate::i18n::Lang::En, "Something went wrong inside kcal."),
        ] {
            crate::i18n::with_lang(lang, || {
                let error = serde_json::to_value(report_error("OS detail")).unwrap();
                assert_eq!(error["kind"], "internal");
                assert_eq!(error["message"], expected);
                assert!(!error.to_string().contains("OS detail"));
            });
        }
    }
}
