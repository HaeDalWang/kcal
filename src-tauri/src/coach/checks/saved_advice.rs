//! Advice only. Evidence contains setting names, sizes and event names, never commands or secrets.
use super::{CheckFacts, CheckKind, CheckOption, CoachCheck, Inputs};
use crate::usage::Agent;
use std::path::Path;

pub(super) fn card(
    topic: &str,
    kind: CheckKind,
    agent: Agent,
    file: &Path,
    evidence: Vec<String>,
    docs: &[&str],
) -> CoachCheck {
    let prefix = match kind {
        CheckKind::A => "A",
        CheckKind::B => "B",
        _ => "D",
    };
    let agent_id = if agent == Agent::Codex {
        "codex"
    } else {
        "kiro"
    };
    CoachCheck {
        id: format!("{prefix}-{agent_id}-{topic}"), kind, agent,
        facts: CheckFacts::SavedAdvice { topic: topic.into(), evidence },
        options: vec![CheckOption {
            id: "guide".into(), is_recommended: true, is_writable: false,
            instruction: Some(crate::i18n::pick("안내만 해요. 표시된 파일과 설정을 직접 검토하세요. kcal은 파일을 고치지 않아요.", "Advice only. Review the displayed file and setting yourself. kcal does not edit files.").into()),
            changes: vec![], log_name: String::new(), ops: vec![],
        }, CheckOption::keep(false)],
        file: file.to_string_lossy().into_owned(), docs: docs.iter().map(|s| (*s).into()).collect(),
        tokens: None, is_dismissed: false,
    }
}

pub(super) fn list(input: &Inputs) -> Vec<CoachCheck> {
    let mut cards = super::codex_saved::list(input);
    cards.extend(super::kiro_saved::list(input));
    cards
}
