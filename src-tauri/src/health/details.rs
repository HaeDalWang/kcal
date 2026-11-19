//! 주간 요약 전용 분해. 기존 파일 캐시·중복 제거와 환율 판정을 재사용한다.
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use super::{AgentComparison, ModelInputShare, PeriodRate, PeriodStats, in_period};
use crate::metrics::{claude, modified_secs, unix_secs};
use crate::rate::{RateStatus, codex};
use crate::tuning::{AdvancedTuning, Tuning};
use crate::usage::Agent;

struct Input {
    ts: f64,
    tokens: f64,
    automated: bool,
    model: Option<Arc<str>>,
}

fn shares(period: &mut PeriodStats, inputs: &[Input]) {
    let (from, to) = (unix_secs(period.from), unix_secs(period.to));
    let mut total = 0.0;
    let mut automated = 0.0;
    let mut models: HashMap<&str, f64> = HashMap::new();
    for input in inputs.iter().filter(|i| in_period(i.ts, from, to)) {
        if !input.tokens.is_finite() || input.tokens <= 0.0 {
            continue;
        }
        total += input.tokens;
        if input.automated {
            automated += input.tokens;
        }
        *models
            .entry(input.model.as_deref().unwrap_or("unknown"))
            .or_default() += input.tokens;
    }
    if total <= 0.0 {
        return;
    }
    period.automated_input_share = Some(automated / total * 100.0);
    let mut models: Vec<_> = models.into_iter().collect();
    models.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    const TOP_MODELS: usize = 3;
    let mut top: Vec<_> = models
        .iter()
        .take(TOP_MODELS)
        .map(|(model, tokens)| ModelInputShare {
            model: Some((*model).to_owned()),
            share: tokens / total * 100.0,
        })
        .collect();
    if models.len() > TOP_MODELS {
        top.push(ModelInputShare {
            model: None,
            share: models[TOP_MODELS..].iter().map(|m| m.1).sum::<f64>() / total * 100.0,
        });
    }
    period.model_input_shares = Some(top);
}

fn claude_inputs(home: &Path, since: f64) -> Vec<Input> {
    let mut seen = HashSet::new();
    claude::log_files(home, since, usize::MAX)
        .iter()
        .flat_map(|file| {
            claude::entries(file)
                .iter()
                .filter_map(|e| {
                    let tokens = e.usage?.total_input();
                    if tokens <= 0.0 || e.id.as_ref().is_some_and(|id| !seen.insert(id.clone())) {
                        return None;
                    }
                    Some(Input {
                        ts: e.ts,
                        tokens,
                        model: e.model.clone(),
                        // ponytail: 새 entrypoint·누락은 대화형. 자동 실행 방식이 늘면 검증 뒤 확장한다.
                        automated: e.entrypoint.as_deref() == Some("sdk-cli"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn period_rate(
    period: &PeriodStats,
    events: &[codex::Event],
    tuning: &AdvancedTuning,
) -> PeriodRate {
    let (from, to) = (unix_secs(period.from), unix_secs(period.to));
    let events: Vec<_> = events
        .iter()
        .filter(|e| in_period(e.ts, from, to))
        .cloned()
        .collect();
    let rate = codex::rate(&events, tuning);
    let usable = matches!(rate.status, RateStatus::Estimated | RateStatus::RangeOnly);
    PeriodRate {
        status: rate.status,
        windows: rate.windows,
        per_percent: rate.per_percent.filter(|_| usable),
        fold_range: rate.fold_range.filter(|_| usable),
    }
}

pub(super) fn enrich(weekly: &mut [AgentComparison], home: &Path, tuning: &Tuning) {
    for comparison in weekly {
        let since = unix_secs(comparison.before.from);
        let (inputs, events) = match comparison.agent {
            Agent::Kiro => continue,
            Agent::Claude => (claude_inputs(home, since), Vec::new()),
            Agent::Codex => {
                let files: Vec<_> = codex::rollout_files(home)
                    .into_iter()
                    .filter(|f| modified_secs(f).is_some_and(|m| m >= since))
                    .collect();
                let attributed = codex::attributed_events(&files);
                let inputs = attributed
                    .iter()
                    .map(|(e, a)| Input {
                        ts: e.ts,
                        tokens: e.input,
                        automated: a.automated,
                        model: a.model.clone(),
                    })
                    .collect();
                (inputs, attributed.into_iter().map(|(e, _)| e).collect())
            }
        };
        for period in [&mut comparison.before, &mut comparison.after] {
            shares(period, &inputs);
            if comparison.agent == Agent::Codex {
                period.codex_rate = Some(period_rate(period, &events, &tuning.advanced));
            }
        }
    }
}

#[cfg(test)]
mod tests;
