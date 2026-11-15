//! 컨텍스트 측정이 읽는 에이전트 설정 폴더. 다른 context 모듈은 홈에서 경로를 직접 만들지 않는다.
//!
//! 세 에이전트 모두 macOS·Windows에서 홈(`%USERPROFILE%`) 아래 같은 이름을 쓴다(docs/platform/README.md).
//! 폴더 안 구조(`skills/`, `rules/` 등)는 OS와 상관없이 같아서 각 스캔 모듈이 붙인다.
//! `CLAUDE_CONFIG_DIR`·`CODEX_HOME` 재지정은 ctxmeter처럼 보지 않는다(0005 "옮기지 않은 것").

use std::path::{Path, PathBuf};

pub fn claude_root(home: &Path) -> PathBuf {
    home.join(".claude")
}

/// Claude Code 상태 파일: user·local scope MCP 서버, 프로젝트별 승인.
pub fn claude_state_file(home: &Path) -> PathBuf {
    home.join(".claude.json")
}

pub fn codex_root(home: &Path) -> PathBuf {
    home.join(".codex")
}

pub fn kiro_root(home: &Path) -> PathBuf {
    home.join(".kiro")
}
