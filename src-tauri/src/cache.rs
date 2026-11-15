//! 느린 계산(환율·컨텍스트)의 마지막 결과를 들고 있는 공용 캐시.
//!
//! 값이 있으면 즉시 돌려준다. 오래됐으면(호출이 넘긴 `ttl`, settings tuning) 옛 값을 먼저 주고 뒤에서 한 번만 다시 계산한다.
//! 값이 없을 때 이미 계산 중이면 그 결과를 같이 기다린다. 타이머는 없다: 다시 계산하는 시점은
//! 호출이 들어왔을 때뿐이다(AGENTS.md "성능 원칙").

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::error::AppError;

struct State<T> {
    value: Option<(T, Instant)>,
    is_computing: bool,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    finished: Condvar,
}

pub struct Cache<T>(Arc<Shared<T>>);

impl<T> Clone for Cache<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// 이번 호출이 할 일. 계산을 맡은 호출(`Refresh`, `Compute`)만 `compute`를 부른다.
enum Plan<T> {
    Fresh(T),
    /// 옛 값을 먼저 주고 뒤에서 다시 계산한다.
    Refresh(T),
    Compute,
    /// 다른 호출이 계산 중이다.
    Wait,
}

/// 계산을 맡은 쪽이 패닉해도 기다리는 호출이 영원히 잠들지 않게 한다.
struct Claim<'a, T>(&'a Shared<T>);

impl<T> Drop for Claim<'_, T> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.is_computing = false;
        }
        self.0.finished.notify_all();
    }
}

impl<T: Clone + Send + 'static> Cache<T> {
    pub fn new() -> Self {
        Self(Arc::new(Shared {
            state: Mutex::new(State {
                value: None,
                is_computing: false,
            }),
            finished: Condvar::new(),
        }))
    }

    fn plan(&self, ttl: Duration) -> Result<Plan<T>, AppError> {
        let mut state = self.0.state.lock().map_err(|_| AppError::Internal)?;
        let cached = state
            .value
            .as_ref()
            .map(|(v, at)| (v.clone(), at.elapsed()));
        Ok(match cached {
            Some((value, age)) if age < ttl => Plan::Fresh(value),
            _ if state.is_computing => match cached {
                Some((value, _)) => Plan::Fresh(value),
                None => Plan::Wait,
            },
            cached => {
                state.is_computing = true;
                cached.map_or(Plan::Compute, |(value, _)| Plan::Refresh(value))
            }
        })
    }

    /// 계산을 맡은 호출이 부른다. 결과를 저장하고 기다리는 호출을 깨운다.
    fn run(&self, compute: impl FnOnce() -> T) -> T {
        let _claim = Claim(&self.0);
        let value = compute();
        if let Ok(mut state) = self.0.state.lock() {
            state.value = Some((value.clone(), Instant::now()));
        }
        value
    }

    fn wait(&self) -> Result<T, AppError> {
        let state = self.0.state.lock().map_err(|_| AppError::Internal)?;
        let state = self
            .0
            .finished
            .wait_while(state, |s| s.is_computing)
            .map_err(|_| AppError::Internal)?;
        state
            .value
            .as_ref()
            .map(|(v, _)| v.clone())
            .ok_or(AppError::Internal)
    }

    /// 캐시된 값, 없으면 계산한 값. 다시 계산이 끝날 때마다 `on_computed`를 부른다(event 전송용).
    pub async fn get(
        &self,
        ttl: Duration,
        compute: impl FnOnce() -> T + Send + 'static,
        on_computed: impl FnOnce(&T) + Send + 'static,
    ) -> Result<T, AppError> {
        let this = self.clone();
        let run = move || {
            let value = this.run(compute);
            on_computed(&value);
            value
        };
        match self.plan(ttl)? {
            Plan::Fresh(value) => Ok(value),
            Plan::Refresh(value) => {
                tauri::async_runtime::spawn_blocking(run);
                Ok(value)
            }
            Plan::Compute => tauri::async_runtime::spawn_blocking(run)
                .await
                .map_err(|_| AppError::Internal),
            Plan::Wait => {
                let this = self.clone();
                tauri::async_runtime::spawn_blocking(move || this.wait())
                    .await
                    .map_err(|_| AppError::Internal)?
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    const LONG: Duration = Duration::from_secs(3600);

    #[test]
    fn serves_the_cached_value_without_recomputing_while_fresh() {
        let cache = Cache::new();
        assert!(matches!(cache.plan(LONG).unwrap(), Plan::Compute));
        assert_eq!(cache.run(|| 1), 1);
        assert!(matches!(cache.plan(LONG).unwrap(), Plan::Fresh(1)));
        assert!(matches!(cache.plan(LONG).unwrap(), Plan::Fresh(1)));
    }

    #[test]
    fn returns_the_old_value_once_when_stale_and_claims_a_single_refresh() {
        let cache = Cache::new();
        cache.plan(Duration::ZERO).unwrap();
        cache.run(|| 1);
        assert!(matches!(
            cache.plan(Duration::ZERO).unwrap(),
            Plan::Refresh(1)
        ));
        // 계산 중에는 다른 호출이 또 맡지 않고 옛 값을 받는다.
        assert!(matches!(
            cache.plan(Duration::ZERO).unwrap(),
            Plan::Fresh(1)
        ));
        cache.run(|| 2);
        assert!(matches!(
            cache.plan(Duration::ZERO).unwrap(),
            Plan::Refresh(2)
        ));
    }

    #[test]
    fn reads_the_ttl_from_each_call_so_a_changed_setting_applies_to_the_next_one() {
        let cache = Cache::new();
        cache.plan(LONG).unwrap();
        cache.run(|| 1);
        assert!(matches!(cache.plan(LONG).unwrap(), Plan::Fresh(1)));
        assert!(matches!(
            cache.plan(Duration::ZERO).unwrap(),
            Plan::Refresh(1)
        ));
    }

    #[test]
    fn computes_once_when_requests_arrive_together_on_an_empty_cache() {
        let cache = Cache::new();
        let calls = Arc::new(AtomicUsize::new(0));
        assert!(matches!(cache.plan(LONG).unwrap(), Plan::Compute));
        let waiters: Vec<_> = (0..4)
            .map(|_| {
                let cache = cache.clone();
                thread::spawn(move || {
                    assert!(matches!(cache.plan(LONG).unwrap(), Plan::Wait));
                    cache.wait().unwrap()
                })
            })
            .collect();
        thread::sleep(Duration::from_millis(50));
        let counter = Arc::clone(&calls);
        cache.run(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            7
        });
        for waiter in waiters {
            assert_eq!(waiter.join().unwrap(), 7);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn get_computes_once_for_concurrent_callers_and_reports_each_computation() {
        let cache: Cache<u32> = Cache::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let updates = Arc::new(AtomicUsize::new(0));
        let values = tauri::async_runtime::block_on(async {
            let tasks: Vec<_> = (0..4)
                .map(|_| {
                    let (cache, calls, updates) = (cache.clone(), calls.clone(), updates.clone());
                    tauri::async_runtime::spawn(async move {
                        cache
                            .get(
                                LONG,
                                move || {
                                    thread::sleep(Duration::from_millis(100));
                                    calls.fetch_add(1, Ordering::SeqCst);
                                    5
                                },
                                move |_| {
                                    updates.fetch_add(1, Ordering::SeqCst);
                                },
                            )
                            .await
                            .unwrap()
                    })
                })
                .collect();
            let mut values = Vec::new();
            for task in tasks {
                values.push(task.await.unwrap());
            }
            values
        });
        assert_eq!(values, [5; 4]);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(updates.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn releases_waiters_with_an_error_when_the_computation_panics() {
        let cache: Cache<u32> = Cache::new();
        cache.plan(LONG).unwrap();
        let waiter = {
            let cache = cache.clone();
            thread::spawn(move || cache.wait())
        };
        let panicking = cache.clone();
        let _ = thread::spawn(move || panicking.run(|| panic!("boom"))).join();
        assert!(waiter.join().unwrap().is_err());
        assert!(matches!(cache.plan(LONG).unwrap(), Plan::Compute));
    }
}
