//! OS마다 다른 경로를 모은다(docs/platform/README.md). 다른 모듈은 경로를 직접 만들지 않는다.

use std::path::{Path, PathBuf};

/// Claude Code OAuth 토큰이 든 Keychain 항목(macOS).
#[cfg(target_os = "macos")]
pub const CLAUDE_KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// Windows·Linux의 Claude Code 평문 credentials 파일. macOS는 Keychain을 쓴다.
#[cfg(not(target_os = "macos"))]
pub fn claude_credentials_file(home: &Path) -> PathBuf {
    home.join(".claude").join(".credentials.json")
}

/// statusLine 기록기(agent-usage-audit)가 남기는 Claude rate limit 기록.
pub fn claude_statusline_log(home: &Path) -> PathBuf {
    home.join(".local/share/agent-usage-audit/data/claude-ratelimits.jsonl")
}

pub fn codex_auth_file(home: &Path) -> PathBuf {
    home.join(".codex").join("auth.json")
}

pub fn codex_sessions_dir(home: &Path) -> PathBuf {
    home.join(".codex").join("sessions")
}

/// kiro-cli 후보. 메뉴바 앱의 PATH는 짧아서 보통 설치 위치를 먼저 본다.
pub fn kiro_cli_candidates(home: &Path, path_env: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    #[cfg(windows)]
    let (installed, name) = {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"));
        (local.join("Kiro-Cli").join("kiro-cli.exe"), "kiro-cli.exe")
    };
    #[cfg(not(windows))]
    let (installed, name) = (home.join(".local/bin/kiro-cli"), "kiro-cli");

    let on_path: Vec<PathBuf> = path_env
        .map(|value| {
            std::env::split_paths(value)
                .map(|dir| dir.join(name))
                .collect()
        })
        .unwrap_or_default();
    std::iter::once(installed).chain(on_path).collect()
}

/// Claude Code CLI 후보(docs/decisions/0018 "실측 사실"). 설치 위치를 먼저, `PATH`를 다음에 본다.
/// Windows 설치 위치 `%USERPROFILE%\.local\bin\claude.exe`는 추정이다.
pub fn claude_cli_candidates(home: &Path, path_env: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    #[cfg(windows)]
    let name = "claude.exe";
    #[cfg(not(windows))]
    let name = "claude";

    let installed = home.join(".local").join("bin").join(name);
    let on_path: Vec<PathBuf> = path_env
        .map(|value| {
            std::env::split_paths(value)
                .map(|dir| dir.join(name))
                .collect()
        })
        .unwrap_or_default();
    std::iter::once(installed).chain(on_path).collect()
}
