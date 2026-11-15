//! D. 효과 없는 설정: 사용자 설정(`~/.claude/settings.json`)의 값 중 최근 쓴 모델이 무시하는 것.
//! 판정은 `models`의 규칙 함수만 쓴다. project·local 설정은 보지 않는다(같은 키가 모든 모델에 먹어 판정이 다르다).

use serde_json::Value;

use super::models::{Effort, Model};
use super::{CheckFacts, CheckKind, CheckOption, CoachCheck, Inputs, ModelVerdict};
use crate::coach::edit::{JsonOp, key};
use crate::i18n::pick;
use crate::usage::Agent;

const EFFORT_DOCS: [&str; 2] = [
    "https://code.claude.com/docs/en/settings-reference#effortlevel",
    "https://code.claude.com/docs/en/model-config#adjust-effort-level",
];
const THINKING_DOCS: [&str; 2] = [
    "https://code.claude.com/docs/en/model-config#extended-thinking",
    "https://code.claude.com/docs/en/env-vars",
];

fn own_effort(input: &Inputs, model: &str) -> Option<Effort> {
    let value = input
        .settings
        .get("modelSettings")?
        .get(model)?
        .get("effortLevel")?;
    Effort::parse(value.as_str()?)
}

fn verdicts(
    input: &Inputs,
    is_ignored: impl Fn(Model) -> bool,
    with_effort: bool,
) -> Vec<ModelVerdict> {
    input
        .records
        .known
        .iter()
        .map(|(count, model)| ModelVerdict {
            model: count.model.clone(),
            requests: count.requests,
            own_effort: with_effort
                .then(|| own_effort(input, &count.model))
                .flatten(),
            default_effort: with_effort.then(|| model.default_effort()).flatten(),
            is_ignored: is_ignored(*model),
        })
        .collect()
}

fn names(models: &[ModelVerdict], keep: impl Fn(&ModelVerdict) -> bool) -> Vec<String> {
    models
        .iter()
        .filter(|m| keep(m))
        .map(|m| m.model.clone())
        .collect()
}

fn check(
    id: &str,
    input: &Inputs,
    facts: CheckFacts,
    options: Vec<CheckOption>,
    docs: &[&str],
) -> CoachCheck {
    CoachCheck {
        id: id.into(),
        kind: CheckKind::D,
        agent: Agent::Claude,
        facts,
        options,
        file: input.file.to_string_lossy().into_owned(),
        docs: docs.iter().map(|d| (*d).to_owned()).collect(),
        tokens: None,
        is_dismissed: false,
    }
}

fn save_effort(model: &str, value: Effort) -> JsonOp {
    let value = serde_json::to_value(value).unwrap_or(Value::Null);
    JsonOp::Set(
        vec![key("modelSettings"), key(model), key("effortLevel")],
        value,
    )
}

/// `remove`(추천): 최상위 키를 지우고 값이 풀리는 모델 자리에 같은 값을 적는다.
/// `remove-and-save-current`: 지금 모델이 무시하고 자기 값이 없을 때만, 지금 모델 자리에도 적는다.
fn effort_options(
    value: Effort,
    moved_to: &[String],
    models: &[ModelVerdict],
    current: &str,
) -> Option<Vec<CheckOption>> {
    let remove: Vec<JsonOp> = std::iter::once(JsonOp::Remove(vec![key("effortLevel")]))
        .chain(moved_to.iter().map(|m| save_effort(m, value)))
        .collect();
    let mut options = vec![CheckOption::write(
        "remove",
        pick(
            "생각 깊이 옛 설정 정리",
            "Clean up the old thinking-depth setting",
        ),
        true,
        remove.clone(),
    )];
    let verdict = models.iter().find(|m| m.model == current)?;
    if verdict.is_ignored && verdict.own_effort.is_none() {
        let ops = remove
            .into_iter()
            .chain([save_effort(current, value)])
            .collect();
        options.push(CheckOption::write(
            "remove-and-save-current",
            pick(
                "생각 깊이 옛 설정 정리(지금 모델에 저장)",
                "Clean up the old thinking-depth setting (save it for the current model)",
            ),
            false,
            ops,
        ));
    }
    Some(options)
}

/// `D-claude-effort-level`: 최상위 `effortLevel`을 최근 모델 중 하나 이상이 무시한다.
/// user `env.CLAUDE_CODE_EFFORT_LEVEL`이 있으면 그 값이 모든 설정보다 우선해(model-config "Adjust effort level" 1번)
/// 판정이 달라지므로 내지 않는다(범위 밖).
pub fn effort_level(input: &Inputs) -> Option<CoachCheck> {
    let value = Effort::parse(input.settings.get("effortLevel")?.as_str()?)?;
    if input.env("CLAUDE_CODE_EFFORT_LEVEL").is_some() {
        return None;
    }
    let (current, _) = input.records.current()?;
    let models = verdicts(input, |m| !m.applies_top_level_effort(), true);
    let ignored_by = names(&models, |m| m.is_ignored);
    if ignored_by.is_empty() {
        return None;
    }
    // 지우면 값이 풀리는 모델: 값이 먹고, 자기 값이 없고, 기본값이 다르다(모르면 다르다고 본다).
    let moved_to = names(&models, |m| {
        !m.is_ignored && m.own_effort.is_none() && m.default_effort != Some(value)
    });
    let options = effort_options(value, &moved_to, &models, &current.model)?;
    let facts = CheckFacts::EffortLevel {
        value,
        days: input.records.days,
        models,
        ignored_by,
        moved_to,
        current: current.model.clone(),
        unknown_models: input.records.unknown.clone(),
    };
    Some(check(
        "D-claude-effort-level",
        input,
        facts,
        options,
        &EFFORT_DOCS,
    ))
}

/// 지우기 / 그대로. 이 설정이 먹는 모델이 없으면 지우기 추천, 있으면 그대로 추천(지우면 동작이 바뀐다).
fn thinking_options(
    log_name: &str,
    affected: &[String],
    path: Vec<crate::coach::edit::Seg>,
) -> Vec<CheckOption> {
    vec![
        CheckOption::write(
            "remove",
            log_name,
            affected.is_empty(),
            vec![JsonOp::Remove(path)],
        ),
        CheckOption::keep(!affected.is_empty()),
    ]
}

/// `D-claude-always-thinking`: `alwaysThinkingEnabled: false`인데 최근 모델 중 thinking을 못 끄는 모델이 있다.
pub fn always_thinking(input: &Inputs) -> Option<CoachCheck> {
    if input.settings.get("alwaysThinkingEnabled") != Some(&Value::Bool(false)) {
        return None;
    }
    let models = verdicts(input, |m| !m.can_disable_thinking(), false);
    let ignored_by = names(&models, |m| m.is_ignored);
    if ignored_by.is_empty() {
        return None;
    }
    let affected = names(&models, |m| !m.is_ignored);
    let options = thinking_options(
        pick("생각 끄기 설정 정리", "Clean up the thinking-off setting"),
        &affected,
        vec![key("alwaysThinkingEnabled")],
    );
    let facts = CheckFacts::AlwaysThinking {
        days: input.records.days,
        models,
        ignored_by,
        affected,
        unknown_models: input.records.unknown.clone(),
    };
    let docs = [
        THINKING_DOCS[0],
        "https://code.claude.com/docs/en/settings-reference#alwaysthinkingenabled",
    ];
    Some(check(
        "D-claude-always-thinking",
        input,
        facts,
        options,
        &docs,
    ))
}

/// `D-claude-max-thinking-tokens`: user `env.MAX_THINKING_TOKENS`가 숫자. `0`은 thinking 끄기와 같은 판정,
/// 양수는 adaptive 모델이 무시한다(`CLAUDE_CODE_DISABLE_ADAPTIVE_THINKING=1`이면 Opus·Sonnet 4.6은 먹는다).
pub fn max_thinking_tokens(input: &Inputs) -> Option<CoachCheck> {
    let value: u64 = input.env("MAX_THINKING_TOKENS")?.trim().parse().ok()?;
    let is_adaptive_disabled = input.is_env_on("CLAUDE_CODE_DISABLE_ADAPTIVE_THINKING");
    let models = verdicts(
        input,
        |m| {
            if value == 0 {
                !m.can_disable_thinking()
            } else {
                m.ignores_positive_thinking_budget(is_adaptive_disabled)
            }
        },
        false,
    );
    let ignored_by = names(&models, |m| m.is_ignored);
    if ignored_by.is_empty() {
        return None;
    }
    let affected = names(&models, |m| !m.is_ignored);
    let path = vec![key("env"), key("MAX_THINKING_TOKENS")];
    let options = thinking_options(
        pick(
            "생각 토큰 한도 설정 정리",
            "Clean up the thinking-token limit setting",
        ),
        &affected,
        path,
    );
    let facts = CheckFacts::MaxThinkingTokens {
        value,
        days: input.records.days,
        models,
        ignored_by,
        affected,
        unknown_models: input.records.unknown.clone(),
    };
    Some(check(
        "D-claude-max-thinking-tokens",
        input,
        facts,
        options,
        &THINKING_DOCS,
    ))
}
