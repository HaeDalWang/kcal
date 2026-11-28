//! Claude 모델 판정(0017 첫 점검 D). 모든 규칙은 공식 문서 원문(2026-11-06 받은 쪽, `code.claude.com/docs/en/`)과
//! 2026-11-07 Claude Code 2.1.319 실측을 따른다. 규칙 함수마다 근거 문장이 있는 쪽·절을 적었다.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::metrics::claude::ModelRequest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Opus,
    Sonnet,
    Haiku,
    Fable,
}

/// 정규화한 id에서 읽은 family·버전. 버전은 (major, minor), minor가 없으면 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Model {
    pub family: Family,
    pub version: (u32, u32),
}

static DATE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-\d{8}$").unwrap());

/// 끝의 `[1m]`과 `-YYYYMMDD`를 뗀다(`claude-haiku-4-5-20251001` → `claude-haiku-4-5`).
/// settings-reference `modelSettings`: "matches that model's alias, date-suffixed, `[1m]` … IDs to the same entry".
pub fn normalize(id: &str) -> String {
    let id = id.trim();
    let id = id.strip_suffix("[1m]").unwrap_or(id);
    DATE_SUFFIX.replace(id, "").into_owned()
}

fn family_of(token: &str) -> Option<Family> {
    match token {
        "opus" => Some(Family::Opus),
        "sonnet" => Some(Family::Sonnet),
        "haiku" => Some(Family::Haiku),
        "fable" => Some(Family::Fable),
        _ => None,
    }
}

/// `claude-opus-5-5`, `claude-fable-5-1`, 옛 형식 `claude-3-5-sonnet`. 그 밖의 id는 `None`(판정에서 뺀다).
pub fn parse(normalized: &str) -> Option<Model> {
    let rest = normalized.strip_prefix("claude-")?;
    let tokens: Vec<&str> = rest.split('-').collect();
    let (position, family) = tokens
        .iter()
        .enumerate()
        .find_map(|(i, t)| family_of(t).map(|f| (i, f)))?;
    let numbers: Vec<&str> = tokens
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != position)
        .map(|(_, t)| *t)
        .collect();
    let digits = |t: &str| (!t.is_empty() && t.len() <= 2).then(|| t.parse::<u32>().ok())?;
    let version = match numbers.as_slice() {
        [major] => (digits(major)?, 0),
        [major, minor] => (digits(major)?, digits(minor)?),
        _ => return None,
    };
    Some(Model { family, version })
}

impl Model {
    fn is_trio(self) -> bool {
        matches!(self.family, Family::Opus | Family::Sonnet | Family::Haiku)
    }

    /// 문서가 아는 가장 새 버전보다 새 모델(판정 근거가 없다).
    fn is_newer_than_docs(self) -> bool {
        let newest = if self.family == Family::Fable {
            (5, 1)
        } else {
            (5, 5)
        };
        self.version > newest
    }

    /// 사용자 설정 최상위 `effortLevel`이 이 모델에 먹나.
    /// settings-reference `effortLevel` 마지막 문단: "it keeps applying … on Opus 5, Fable 5.1, and earlier models.
    /// Opus 5.5 and models released after it ignore it". 실측 2026-11-07(`/effort status`, 2.1.319): Haiku 5.5 안 먹음,
    /// Haiku 4.5 먹음. Sonnet 5.5는 실측하지 않았다(같은 5.5 세대라 안 먹는다고 추정).
    pub fn applies_top_level_effort(self) -> bool {
        match self.family {
            Family::Fable => self.version <= (5, 1),
            _ => self.version < (5, 5),
        }
    }

    /// 모델 기본 effort. model-config "Adjust effort level" 5번: "`high` on every model that supports effort, except that
    /// Opus 5.5, Sonnet 5.5, and Haiku 5.5 default to `medium` and Opus 4.7 defaults to `xhigh`".
    /// Haiku 4.5는 문서의 effort 표에 없지만 실측(2026-11-07) `/effort status`가 high였다. 그래서 "그 밖"은 high로 둔다.
    /// 문서보다 새 모델은 모른다(`None`).
    pub fn default_effort(self) -> Option<Effort> {
        if self.is_newer_than_docs() {
            return None;
        }
        Some(match (self.family, self.version) {
            (_, (5, 5)) if self.is_trio() => Effort::Medium,
            (Family::Opus, (4, 7)) => Effort::Xhigh,
            _ => Effort::High,
        })
    }

    /// thinking을 끌 수 있나. model-config "Extended thinking": "You can't turn thinking off on Opus 5.5, Sonnet 5.5,
    /// Haiku 5.5, or the Fable models". env-vars `MAX_THINKING_TOKENS`·`CLAUDE_CODE_DISABLE_THINKING` 행도 같다.
    /// 5.5보다 새 모델도 끌 수 없다고 본다(추정, 문서에 아직 없음).
    pub fn can_disable_thinking(self) -> bool {
        !(self.family == Family::Fable || (self.is_trio() && self.version >= (5, 5)))
    }

    /// 양수 `MAX_THINKING_TOKENS`를 무시하나. env-vars `MAX_THINKING_TOKENS`: "For a positive value, Claude Code ignores the
    /// number itself on adaptive reasoning models, except when `CLAUDE_CODE_DISABLE_ADAPTIVE_THINKING` turns adaptive
    /// reasoning off". model-config "Adaptive reasoning and fixed thinking budgets": "Fable models, Sonnet 5 and later,
    /// Haiku 5.5, and Opus 4.7 and later always use adaptive reasoning", Opus 4.6·Sonnet 4.6은 env로 끌 수 있다.
    /// effort 표에 없는 옛 모델(Haiku 4.5 등)은 adaptive가 아니라 숫자가 먹는다.
    pub fn ignores_positive_thinking_budget(self, is_adaptive_disabled: bool) -> bool {
        let is_always_adaptive = match self.family {
            Family::Fable => true,
            Family::Sonnet => self.version >= (5, 0),
            Family::Haiku => self.version >= (5, 5),
            Family::Opus => self.version >= (4, 7),
        };
        let is_switchable =
            matches!(self.family, Family::Opus | Family::Sonnet) && self.version == (4, 6);
        is_always_adaptive || (is_switchable && !is_adaptive_disabled)
    }
}

/// effort 단계. 순서가 깊이 순이다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" => Some(Self::Xhigh),
            "max" => Some(Self::Max),
            _ => None,
        }
    }
}

/// 모델 하나의 요청 수(같은 `message.id`는 한 번).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCount {
    /// 정규화한 id(`claude-opus-5-5`).
    pub model: String,
    pub requests: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortCount {
    pub effort: Effort,
    pub requests: u64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct SubagentEffort {
    pub heavy_requests: u64,
    pub subagent_requests: u64,
    pub by_effort: Vec<EffortCount>,
    pub agents: Vec<String>,
}

impl SubagentEffort {
    pub(crate) fn from_requests(requests: Vec<crate::metrics::claude::Entry>) -> Self {
        let mut summary = Self::default();
        let mut seen = std::collections::HashSet::new();
        let mut counts = std::collections::BTreeMap::new();
        let mut agents = std::collections::BTreeSet::new();
        for entry in requests {
            if entry.model.as_deref() == Some(SYNTHETIC)
                || entry.id.is_some_and(|id| !seen.insert(id))
            {
                continue;
            }
            summary.subagent_requests += 1;
            let meta = entry.subagent.as_deref();
            let Some(effort) = meta
                .and_then(|m| m.effort.as_deref())
                .and_then(Effort::parse)
            else {
                continue;
            };
            *counts.entry(effort).or_insert(0) += 1;
            if effort >= Effort::Xhigh {
                summary.heavy_requests += 1;
                agents.extend(meta.and_then(|m| m.attribution_agent.as_deref().map(String::from)));
            }
        }
        summary.by_effort = counts
            .into_iter()
            .map(|(effort, requests)| EffortCount { effort, requests })
            .collect();
        summary.agents = agents.into_iter().collect();
        summary
    }
}

/// 최근 Claude 기록 요약(캐시 값). 요청이 많은 순, 같으면 이름순.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RecentRecords {
    pub days: u64,
    pub subagent_effort: SubagentEffort,
    /// 판정할 수 있는 모델.
    pub known: Vec<(ModelCount, Model)>,
    /// 읽지 못한 id(판정에서 뺀다).
    pub unknown: Vec<ModelCount>,
    /// 가장 최근 `api-request-shape` 세션의 작업 폴더(전역 측정과 같은 출처). 모르면 `None`.
    pub workspace: Option<std::path::PathBuf>,
    /// 최근 `days`일 Claude 세션 작업 폴더 전부(기록 줄의 `cwd`, 지금 있는 폴더만, 이름순). 0018 작업 A.
    pub workspaces: Vec<std::path::PathBuf>,
}

const SYNTHETIC: &str = "<synthetic>";

impl RecentRecords {
    /// 요청 목록을 모델별로 센다. `<synthetic>`(Claude Code가 만든 가짜 응답)은 뺀다.
    pub fn from_requests(days: u64, requests: Vec<ModelRequest>) -> Self {
        let mut seen = std::collections::HashSet::new();
        let mut counts: HashMap<String, u64> = HashMap::new();
        for (_, id, model) in requests {
            if model == SYNTHETIC || id.is_some_and(|id| !seen.insert(id)) {
                continue;
            }
            *counts.entry(normalize(&model)).or_default() += 1;
        }
        let mut all: Vec<ModelCount> = counts
            .into_iter()
            .map(|(model, requests)| ModelCount { model, requests })
            .collect();
        all.sort_by(|a, b| {
            b.requests
                .cmp(&a.requests)
                .then_with(|| a.model.cmp(&b.model))
        });
        let (mut known, mut unknown) = (Vec::new(), Vec::new());
        for count in all {
            match parse(&count.model) {
                Some(model) => known.push((count, model)),
                None => unknown.push(count),
            }
        }
        Self {
            days,
            subagent_effort: SubagentEffort::default(),
            known,
            unknown,
            workspace: None,
            workspaces: Vec::new(),
        }
    }

    /// 지금 모델 = 기간 안 요청이 가장 많은 판정 가능한 모델.
    pub fn current(&self) -> Option<&(ModelCount, Model)> {
        self.known.first()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> Model {
        parse(&normalize(id)).unwrap_or_else(|| panic!("{id}"))
    }

    #[test]
    fn normalize_strips_date_and_1m_suffix() {
        assert_eq!(normalize("claude-haiku-4-5-20251001"), "claude-haiku-4-5");
        assert_eq!(normalize("claude-sonnet-4-6[1m]"), "claude-sonnet-4-6");
        assert_eq!(normalize("claude-opus-5-5"), "claude-opus-5-5");
    }

    #[test]
    fn parse_reads_new_and_old_id_shapes_and_rejects_others() {
        assert_eq!(
            model("claude-opus-5-5"),
            Model {
                family: Family::Opus,
                version: (5, 5)
            }
        );
        assert_eq!(model("claude-opus-5").version, (5, 0));
        assert_eq!(model("claude-3-5-sonnet-20241022").family, Family::Sonnet);
        assert_eq!(parse("gpt-5"), None);
        assert_eq!(parse("claude-opus"), None);
        assert_eq!(parse("<synthetic>"), None);
    }

    #[test]
    fn top_level_effort_is_ignored_from_5_5_and_after_fable_5_1() {
        for id in [
            "claude-opus-5-5",
            "claude-sonnet-5-5",
            "claude-haiku-5-5",
            "claude-fable-5-2",
        ] {
            assert!(!model(id).applies_top_level_effort(), "{id}");
        }
        for id in [
            "claude-opus-5",
            "claude-sonnet-5",
            "claude-fable-5-1",
            "claude-haiku-4-5",
            "claude-opus-4-7",
        ] {
            assert!(model(id).applies_top_level_effort(), "{id}");
        }
    }

    #[test]
    fn default_effort_follows_the_model_config_sentence() {
        let effort = |id| model(id).default_effort();
        assert_eq!(effort("claude-opus-5-5"), Some(Effort::Medium));
        assert_eq!(effort("claude-haiku-5-5"), Some(Effort::Medium));
        assert_eq!(effort("claude-opus-4-7"), Some(Effort::Xhigh));
        assert_eq!(effort("claude-opus-5"), Some(Effort::High));
        assert_eq!(effort("claude-haiku-4-5"), Some(Effort::High));
        assert_eq!(effort("claude-opus-6"), None);
    }

    #[test]
    fn thinking_rules_match_env_vars_and_model_config() {
        assert!(!model("claude-opus-5-5").can_disable_thinking());
        assert!(!model("claude-fable-5").can_disable_thinking());
        assert!(model("claude-opus-5").can_disable_thinking());
        assert!(model("claude-haiku-4-5").can_disable_thinking());
        let ignores = |id, off| model(id).ignores_positive_thinking_budget(off);
        assert!(ignores("claude-sonnet-5", false) && ignores("claude-opus-4-7", true));
        assert!(ignores("claude-opus-4-6", false) && !ignores("claude-opus-4-6", true));
        assert!(!ignores("claude-haiku-4-5", false));
    }

    #[test]
    fn counts_unique_message_ids_per_model_without_synthetic() {
        let r = |id: &str, m: &str| (0.0, Some(id.to_owned()), m.to_owned());
        let records = RecentRecords::from_requests(
            30,
            vec![
                r("a", "claude-opus-5-5"),
                r("a", "claude-opus-5-5"),
                r("b", "claude-opus-5-5[1m]"),
                r("c", "claude-haiku-4-5-20251001"),
                r("d", SYNTHETIC),
                r("e", "gpt-5"),
            ],
        );
        let known: Vec<(&str, u64)> = records
            .known
            .iter()
            .map(|(c, _)| (c.model.as_str(), c.requests))
            .collect();
        assert_eq!(known, [("claude-opus-5-5", 2), ("claude-haiku-4-5", 1)]);
        assert_eq!(records.unknown[0].model, "gpt-5");
        assert_eq!(records.current().unwrap().0.model, "claude-opus-5-5");
    }
}
