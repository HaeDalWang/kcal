//! Opt-in numeric-only diagnostics; never writes to the real HOME.
use super::*;

fn date(ts: f64) -> String {
    OffsetDateTime::from_unix_timestamp(ts as i64)
        .unwrap()
        .date()
        .to_string()
}

fn summarize(label: &str, windows: &[Vec<Event>], tuning: &AdvancedTuning) {
    let (x, y): (Vec<_>, Vec<_>) = windows.iter().map(|w| window_row(w)).unzip();
    let groups: Vec<_> = (0..windows.len()).collect();
    let Some(fit) = fit::fit_eval(&x, &y, &groups, tuning.holdout_train) else {
        println!("SPLIT {label} windows={} fit=none", windows.len());
        return;
    };
    println!(
        "SPLIT {label} windows={} train={:.6} loo={:.6} holdout={:.6} val={:.6} coef={:?}",
        windows.len(),
        fit.train,
        fit.loo,
        fit.holdout,
        fit.validation(),
        fit.coef
    );
    let predictions: Vec<f64> = x
        .iter()
        .map(|row| {
            fit.used
                .iter()
                .zip(&fit.coef)
                .map(|(&j, b)| row[j] * b)
                .sum()
        })
        .collect();
    let mut order: Vec<_> = (0..windows.len()).collect();
    order.sort_by(|&a, &b| {
        (predictions[b] - y[b])
            .abs()
            .total_cmp(&(predictions[a] - y[a]).abs())
    });
    for i in order.into_iter().take(10) {
        let w = &windows[i];
        println!(
            "RESID {label} date={} reset={:.0} tokens={:?} pct={:.4} predicted={:.4} residual={:.4} events={} span_hours={:.2} first_pct={:?}",
            date(w[0].resets_at.unwrap()),
            w[0].resets_at.unwrap(),
            x[i],
            y[i],
            predictions[i],
            y[i] - predictions[i],
            w.len(),
            (w.last().unwrap().ts - w[0].ts) / 3600.0,
            w[0].used_percent
        );
    }
}

#[test]
#[ignore = "reads real HOME, prints only dates and numeric aggregates"]
fn codex_split_on_real_rollouts() {
    let home = std::env::home_dir().unwrap();
    let files = rollout_files(&home);
    let events = events(&files);
    let tuning = AdvancedTuning::default();
    let (windows, _) = assign_windows(&events, tuning.reset_tol_secs as f64);
    let cutoff = OffsetDateTime::parse("2026-11-22T00:00:00Z", &Rfc3339)
        .unwrap()
        .unix_timestamp() as f64;
    let (before, after): (Vec<_>, Vec<_>) = windows
        .iter()
        .cloned()
        .partition(|w| w[0].resets_at.unwrap() < cutoff);
    println!("FILES={} EVENTS={}", files.len(), events.len());
    summarize("all", &windows, &tuning);
    summarize("before", &before, &tuning);
    summarize("after", &after, &tuning);
    for month in ["2026-09", "2026-10", "2026-11"] {
        let selected: Vec<_> = windows
            .iter()
            .filter(|w| date(w[0].resets_at.unwrap()).starts_with(month))
            .cloned()
            .collect();
        if selected.len() >= tuning.min_samples_per_var * VARIABLES {
            summarize(month, &selected, &tuning);
        }
    }
}
