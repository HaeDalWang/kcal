//! 절편 없는 NNLS와 검증 R². `codex_rate.py`의 `solve`·`nnls`·`r2`·`fit_eval`을 연산 순서까지 그대로 옮겼다.
//! 계수는 %p / token 단위다.

const NNLS_TOL: f64 = 1e-12;
const NNLS_MAX_ITER: usize = 200;
/// 부분 문제의 정규방정식에 더하는 작은 ridge(특이 행렬 방지).
const RIDGE: f64 = 1e-10;
const SINGULAR: f64 = 1e-15;
/// holdout을 하려면 이만큼 그룹(창)이 있어야 한다.
const HOLDOUT_MIN_GROUPS: usize = 4;

/// Gauss-Jordan(부분 피벗). 특이 행렬이면 `None`.
fn solve(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = a.len();
    let mut m: Vec<Vec<f64>> = a
        .iter()
        .zip(b)
        .map(|(row, &bi)| row.iter().copied().chain([bi]).collect())
        .collect();
    for c in 0..n {
        // 같은 크기면 먼저 나온 행(파이썬 `max`와 같다).
        let mut piv = c;
        for r in c + 1..n {
            if m[r][c].abs() > m[piv][c].abs() {
                piv = r;
            }
        }
        if m[piv][c].abs() < SINGULAR {
            return None;
        }
        m.swap(c, piv);
        let pivot_row = m[c].clone();
        for (r, row) in m.iter_mut().enumerate() {
            if r != c && row[c] != 0.0 {
                let k = row[c] / pivot_row[c];
                for (value, &p) in row.iter_mut().zip(&pivot_row) {
                    *value -= k * p;
                }
            }
        }
    }
    Some((0..n).map(|i| m[i][n] / m[i][i]).collect())
}

/// Lawson–Hanson. 열을 노름으로 스케일한 뒤 정규방정식으로 부분 문제를 푼다. 특이 행렬이면 `None`.
pub fn nnls(x: &[Vec<f64>], y: &[f64]) -> Option<Vec<f64>> {
    let p = x.first().map_or(0, Vec::len);
    let norms: Vec<f64> = (0..p)
        .map(|j| {
            let norm = x.iter().map(|r| r[j] * r[j]).sum::<f64>().sqrt();
            if norm == 0.0 { 1.0 } else { norm }
        })
        .collect();
    let z: Vec<Vec<f64>> = x
        .iter()
        .map(|r| (0..p).map(|j| r[j] / norms[j]).collect())
        .collect();
    let g_mat: Vec<Vec<f64>> = (0..p)
        .map(|i| {
            (0..p)
                .map(|j| z.iter().map(|r| r[i] * r[j]).sum())
                .collect()
        })
        .collect();
    let g: Vec<f64> = (0..p)
        .map(|i| z.iter().zip(y).map(|(r, &yy)| r[i] * yy).sum())
        .collect();
    let grad = |x: &[f64]| -> Vec<f64> {
        (0..p)
            .map(|i| g[i] - (0..p).map(|j| g_mat[i][j] * x[j]).sum::<f64>())
            .collect()
    };
    let sub = |set: &[usize]| -> Option<Vec<f64>> {
        let a: Vec<Vec<f64>> = set
            .iter()
            .map(|&i| {
                set.iter()
                    .map(|&j| g_mat[i][j] + if i == j { RIDGE } else { 0.0 })
                    .collect()
            })
            .collect();
        let b: Vec<f64> = set.iter().map(|&i| g[i]).collect();
        let mut s = vec![0.0; p];
        for (&i, v) in set.iter().zip(solve(&a, &b)?) {
            s[i] = v;
        }
        Some(s)
    };

    let mut x_cur = vec![0.0; p];
    let mut set: Vec<usize> = Vec::new();
    for _ in 0..NNLS_MAX_ITER {
        let w = grad(&x_cur);
        let mut best: Option<usize> = None;
        for j in (0..p).filter(|j| !set.contains(j) && w[*j] > NNLS_TOL) {
            if best.is_none_or(|b| w[j] > w[b]) {
                best = Some(j);
            }
        }
        let Some(next) = best else { break };
        set.push(next);
        for _ in 0..NNLS_MAX_ITER {
            let s = sub(&set)?;
            if set.iter().all(|&i| s[i] > NNLS_TOL) {
                x_cur = s;
                break;
            }
            let alpha = set
                .iter()
                .filter(|&&i| s[i] <= NNLS_TOL && x_cur[i] - s[i] > 0.0)
                .map(|&i| x_cur[i] / (x_cur[i] - s[i]))
                .reduce(f64::min)
                .unwrap_or(0.0);
            x_cur = x_cur
                .iter()
                .zip(&s)
                .map(|(xi, si)| xi + alpha * (si - xi))
                .collect();
            set.retain(|&i| x_cur[i] > NNLS_TOL);
            if set.is_empty() {
                break;
            }
        }
    }
    Some(x_cur.iter().zip(&norms).map(|(xi, n)| xi / n).collect())
}

fn predict(beta: &[f64], row: &[f64]) -> f64 {
    beta.iter().zip(row).map(|(b, v)| b * v).sum()
}

/// 결정계수. 분산이 0이면 NaN.
pub fn r2(y: &[f64], yhat: &[f64]) -> f64 {
    let m = y.iter().sum::<f64>() / y.len() as f64;
    let sst: f64 = y.iter().map(|v| (v - m).powi(2)).sum();
    let sse: f64 = y.iter().zip(yhat).map(|(a, b)| (a - b).powi(2)).sum();
    if sst > 0.0 { 1.0 - sse / sst } else { f64::NAN }
}

#[derive(Debug)]
pub struct Fit {
    /// 쓰인 변수의 index(입력 열 기준). 모든 행이 0인 열은 빠진다.
    pub used: Vec<usize>,
    pub coef: Vec<f64>,
    /// 학습 R². 패리티 확인(`codex_parity_on_real_rollouts`)에만 쓴다.
    #[cfg_attr(not(test), allow(dead_code))]
    pub train: f64,
    pub loo: f64,
    pub holdout: f64,
    /// 변수별 LOO fold 계수의 (최소, 최대). fold가 없으면 비어 있다.
    pub fold_range: Vec<(f64, f64)>,
}

impl Fit {
    /// 판정에 쓰는 검증 R² = min(LOO, holdout). holdout이 없으면(NaN) LOO.
    pub fn validation(&self) -> f64 {
        if self.holdout.is_nan() {
            self.loo
        } else if self.loo.is_nan() {
            f64::NAN
        } else {
            self.loo.min(self.holdout)
        }
    }
}

/// `rows`: 변수 값, `y`: 목표, `groups`: 시간순 그룹 번호(LOO 단위). 특이 행렬이면 `None`.
/// `holdout_train`: 시간순 holdout에서 학습에 쓰는 앞쪽 그룹 비율(기본 0.7), 나머지로 평가한다.
pub fn fit_eval(rows: &[Vec<f64>], y: &[f64], groups: &[usize], holdout_train: f64) -> Option<Fit> {
    let cols = rows.first().map_or(0, Vec::len);
    let used: Vec<usize> = (0..cols)
        .filter(|&j| rows.iter().any(|r| r[j] != 0.0))
        .collect();
    let x: Vec<Vec<f64>> = rows
        .iter()
        .map(|r| used.iter().map(|&j| r[j]).collect())
        .collect();
    let pick = |idx: &[usize]| -> (Vec<Vec<f64>>, Vec<f64>) {
        (
            idx.iter().map(|&i| x[i].clone()).collect(),
            idx.iter().map(|&i| y[i]).collect(),
        )
    };
    let beta = nnls(&x, y)?;
    let train = r2(y, &x.iter().map(|r| predict(&beta, r)).collect::<Vec<_>>());

    let mut sorted_groups: Vec<usize> = groups.to_vec();
    sorted_groups.sort_unstable();
    sorted_groups.dedup();
    let mut loo: Vec<Option<f64>> = vec![None; y.len()];
    let mut folds: Vec<Vec<f64>> = Vec::new();
    for &g in &sorted_groups {
        let train_idx: Vec<usize> = (0..y.len()).filter(|&i| groups[i] != g).collect();
        if train_idx.is_empty() {
            continue;
        }
        let (tx, ty) = pick(&train_idx);
        let b = nnls(&tx, &ty)?;
        for i in (0..y.len()).filter(|&i| groups[i] == g) {
            loo[i] = Some(predict(&b, &x[i]));
        }
        folds.push(b);
    }
    let ok: Vec<usize> = (0..y.len()).filter(|&i| loo[i].is_some()).collect();
    let loo_r2 = r2(
        &ok.iter().map(|&i| y[i]).collect::<Vec<_>>(),
        &ok.iter().filter_map(|&i| loo[i]).collect::<Vec<_>>(),
    );

    let mut holdout = f64::NAN;
    if sorted_groups.len() >= HOLDOUT_MIN_GROUPS {
        let last = sorted_groups.len() - 1;
        let cut = sorted_groups[((sorted_groups.len() as f64 * holdout_train) as usize).min(last)];
        let train_idx: Vec<usize> = (0..y.len()).filter(|&i| groups[i] < cut).collect();
        let test_idx: Vec<usize> = (0..y.len()).filter(|&i| groups[i] >= cut).collect();
        let (tx, ty) = pick(&train_idx);
        let b = nnls(&tx, &ty)?;
        holdout = r2(
            &test_idx.iter().map(|&i| y[i]).collect::<Vec<_>>(),
            &test_idx
                .iter()
                .map(|&i| predict(&b, &x[i]))
                .collect::<Vec<_>>(),
        );
    }

    let fold_range = if folds.is_empty() {
        Vec::new()
    } else {
        (0..used.len())
            .map(|j| {
                let values = folds.iter().map(|f| f[j]);
                (
                    values.clone().fold(f64::INFINITY, f64::min),
                    values.fold(f64::NEG_INFINITY, f64::max),
                )
            })
            .collect()
    };
    Some(Fit {
        used,
        coef: beta,
        train,
        loo: loo_r2,
        holdout,
        fold_range,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f64], b: &[f64], tol: f64) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(u, v)| (u - v).abs() < tol)
    }

    #[test]
    fn nnls_recovers_exact_coefficients() {
        // codex_rate.py selfcheck와 같은 데이터.
        let x = vec![
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![1.0, 1.0],
            vec![2.0, 1.0],
        ];
        let y: Vec<f64> = x.iter().map(|r| 2.0 * r[0] + 3.0 * r[1]).collect();
        assert!(close(&nnls(&x, &y).unwrap(), &[2.0, 3.0], 1e-6));
    }

    #[test]
    fn nnls_pins_negative_coefficient_to_zero() {
        let x = vec![
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![1.0, 1.0],
            vec![2.0, 1.0],
        ];
        let y: Vec<f64> = x.iter().map(|r| 2.0 * r[0] - 3.0 * r[1]).collect();
        let b = nnls(&x, &y).unwrap();
        assert_eq!(b[1], 0.0);
        assert!(b[0] > 0.0);
    }

    /// 토큰 규모의 합성 창: 1%p ≈ 캐시 입력 742K · 비캐시 66K · 출력 5.5K.
    #[test]
    fn fit_recovers_known_per_percent_rates_with_high_validation_r2() {
        let truth = [1.0 / 66_000.0, 1.0 / 742_000.0, 1.0 / 5_500.0];
        let mut seed: u64 = 7;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let rows: Vec<Vec<f64>> = (0..30)
            .map(|_| vec![next() * 2e6, next() * 3e7, next() * 2e5])
            .collect();
        let y: Vec<f64> = rows.iter().map(|r| predict(&truth, r)).collect();
        let groups: Vec<usize> = (0..rows.len()).collect();
        let fit = fit_eval(&rows, &y, &groups, 0.7).unwrap();
        for (got, want) in fit.coef.iter().zip(truth) {
            assert!((got / want - 1.0).abs() < 1e-6, "{got} vs {want}");
        }
        assert!(fit.validation() > 0.999);
        assert_eq!(fit.fold_range.len(), 3);
    }

    #[test]
    fn fit_drops_all_zero_columns_and_r2_is_nan_without_variance() {
        let rows = vec![
            vec![1.0, 0.0],
            vec![2.0, 0.0],
            vec![3.0, 0.0],
            vec![4.0, 0.0],
        ];
        let fit = fit_eval(&rows, &[5.0; 4], &[0, 1, 2, 3], 0.7).unwrap();
        assert_eq!(fit.used, [0]);
        assert!(fit.train.is_nan() && fit.validation().is_nan());
    }
}
