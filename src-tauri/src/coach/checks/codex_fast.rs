//! D. Codex 빠른 모드(Fast): 저장된 설정이 `service_tier = "fast"`인 ChatGPT 로그인 사용자에게 한도를 더 쓴다고 알린다.
//! 안내만 한다(kcal은 Codex 설정 파일을 쓰지 않는다). Standard로 돌아가는 법은 Codex의 `/fast` 반복이다.
//!
//! 읽는 파일: `<home>/.codex/config.toml` 하나, `auth.json`은 로그인 종류(bool)만 꺼낸다.
//! ponytail: 사용자 층 `config.toml`만 본다. 프로젝트 `.codex/config.toml`(신뢰한 폴더)·profile 층·CLI override·
//! 관리자 정책은 안 본다(저장 설정 안내에 한정). 늘리려면 `scope_codex.rs`의 신뢰 프로젝트 판정을 얹어 층마다 같은 키를 본다.
//! Ultrafast는 `service_tier` 값이 문서에 없어 다루지 않는다.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::{CheckFacts, CheckKind, CheckOption, CoachCheck, Inputs};
use crate::context::fsutil;
use crate::context::paths;
use crate::i18n::pick;
use crate::usage::Agent;

pub const ID: &str = "D-codex-fast-mode";
pub const STANDARD: &str = "standard";

/// 근거 문서(2026-11-21 확인): 지원 모델·속도, 모델별 배수.
const DOCS: [&str; 2] = [
    "https://learn.chatgpt.com/docs/agent-configuration/speed#fast-mode",
    "https://learn.chatgpt.com/docs/pricing#speed-multipliers",
];

/// 모델 → 포함 사용량 배수. 문서 수치라 tuning이 아니다(AGENTS.md "수치 허용오차 제외"와 같은 이유).
/// 확인일 2026-11-21, 근거 facts 줄(2026-11-21-codex): 158 GPT-6.2 Astra/Sol 2x, 161 GPT-6.1 Sol/Terra 2.5x,
/// 246 GPT-6 Astra/Sol/Luna 2.5x(같은 표 `pricing#speed-multipliers`).
const MULTIPLIERS: [(&str, f64); 7] = [
    ("gpt-6.2-astra", 2.0),
    ("gpt-6.2-sol", 2.0),
    ("gpt-6.1-sol", 2.5),
    ("gpt-6.1-terra", 2.5),
    ("gpt-6-astra", 2.5),
    ("gpt-6-sol", 2.5),
    ("gpt-6-luna", 2.5),
];

fn multiplier(model: &str) -> Option<f64> {
    MULTIPLIERS
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, m)| *m)
}

/// TOML 값 한 줄: `key = "값"` 또는 `key = true|false`(뒤 주석 허용). 값 글자만 돌려준다.
fn value_of(line: &str, key: &str) -> Option<String> {
    static LINE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"^\s*([A-Za-z0-9_.]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|(true|false))\s*(?:#.*)?$"#)
            .unwrap()
    });
    let c = LINE.captures(line.trim_end_matches('\r'))?;
    if &c[1] != key {
        return None;
    }
    c.get(2)
        .or(c.get(3))
        .or(c.get(4))
        .map(|m| m.as_str().to_owned())
}

/// 저장 설정에서 읽은 값. 헤더(`[...]`) 앞이 최상위, `[features]` 안은 기능 키다. 같은 키가 또 나오면 뒤가 이긴다.
#[derive(Default)]
struct Saved {
    service_tier: Option<String>,
    model: Option<String>,
    fast_mode: Option<String>,
    forced_login: Option<String>,
}

fn read_saved(config: &str) -> Saved {
    let lines: Vec<&str> = config.split('\n').collect();
    let headers = crate::coach::edit::header_indices(&lines);
    let mut saved = Saved::default();
    let mut section: Option<String> = None;
    for (i, line) in lines.iter().enumerate() {
        if headers.contains(&i) {
            section = line
                .trim()
                .trim_start_matches('[')
                .split(']')
                .next()
                .map(|s| s.trim().to_owned());
            continue;
        }
        match section.as_deref() {
            None => {
                let get = |key: &str| value_of(line, key);
                saved.service_tier = get("service_tier").or(saved.service_tier);
                saved.model = get("model").or(saved.model);
                saved.forced_login = get("forced_login_method").or(saved.forced_login);
                saved.fast_mode = get("features.fast_mode").or(saved.fast_mode);
            }
            Some("features") => {
                saved.fast_mode = value_of(line, "fast_mode").or(saved.fast_mode);
            }
            Some(_) => {}
        }
    }
    saved
}

/// `auth.json`이 ChatGPT 로그인인가. 값은 bool만 남기고 토큰은 어디에도 두지 않는다.
fn is_chatgpt_login(home: &Path) -> bool {
    let Some(auth) = fsutil::read_json(&crate::usage::paths::codex_auth_file(home)) else {
        return false;
    };
    match auth.get("auth_mode").and_then(Value::as_str) {
        Some(mode) => mode.to_ascii_lowercase().starts_with("chatgpt"),
        None => auth
            .pointer("/tokens/access_token")
            .and_then(Value::as_str)
            .is_some_and(|t| !t.is_empty()),
    }
}

/// `D-codex-fast-mode`.
pub fn fast_mode(input: &Inputs) -> Option<CoachCheck> {
    let file = paths::codex_root(input.home).join("config.toml");
    let saved = read_saved(&std::fs::read_to_string(&file).ok()?);
    if saved.service_tier.as_deref() != Some("fast")
        || saved.fast_mode.as_deref() == Some("false")
        || saved.forced_login.as_deref() == Some("api")
        || !is_chatgpt_login(input.home)
    {
        return None;
    }
    let multiplier = saved.model.as_deref().and_then(multiplier);
    Some(CoachCheck {
        id: ID.into(),
        kind: CheckKind::D,
        agent: Agent::Codex,
        facts: CheckFacts::CodexFastMode {
            model: saved.model,
            multiplier,
        },
        options: vec![
            CheckOption {
                id: STANDARD.into(),
                is_recommended: true,
                is_writable: false,
                instruction: Some(
                    pick(
                        "Codex에서 `/fast`를 한 번 더 입력하세요. Standard로 돌아가고, 그 선택은 Codex가 저장해요.",
                        "Enter `/fast` once more in Codex. It returns to Standard, and Codex saves that choice.",
                    )
                    .into(),
                ),
                changes: Vec::new(),
                log_name: String::new(),
                ops: Vec::new(),
            },
            CheckOption::keep(false),
        ],
        file: file.to_string_lossy().into_owned(),
        docs: DOCS.iter().map(|d| (*d).to_owned()).collect(),
        tokens: None,
        is_dismissed: false,
    })
}
