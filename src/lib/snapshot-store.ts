// 환율·컨텍스트·지표의 마지막 값을 컴포넌트 밖(메모리)에 둔다. 팝오버를 닫았다 열어도, 탭을 바꿔도 남는다.
// 값은 command 응답과 Rust의 갱신 event로 바뀐다. 타이머·polling 없음.
import { useEffect, useSyncExternalStore } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { t } from "@/lib/i18n";
import {
  errorMessage,
  getContext,
  getMetrics,
  getRates,
  onContextUpdated,
  onMetricsUpdated,
  onRatesUpdated,
  type ContextSnapshot,
  type MetricsSnapshot,
  type RateSnapshot,
} from "@/lib/ipc";

export type StoreState<T> = { value: T | undefined; error: string | null };

export type SnapshotStore<T> = {
  subscribe: (listener: () => void) => () => void;
  getSnapshot: () => StoreState<T>;
  /** command를 한 번 부른다. 처음 부를 때 event 구독도 시작한다. */
  refresh: () => void;
  /** 테스트용: 값·구독을 비운다. */
  reset: () => void;
};

type Source<T> = {
  fetch: () => Promise<T>;
  listen: (handler: (value: T) => void) => Promise<UnlistenFn>;
  computedAt: (value: T) => string;
};

export function createSnapshotStore<T>(source: Source<T>): SnapshotStore<T> {
  let state: StoreState<T> = { value: undefined, error: null };
  let unlisten: Promise<UnlistenFn> | null = null;
  const listeners = new Set<() => void>();

  const set = (next: StoreState<T>) => {
    state = next;
    for (const listener of listeners) listener();
  };
  // 늦게 도착한 옛 응답이 새 값을 덮지 않게 한다.
  const accept = (value: T) => {
    const current = state.value;
    if (current && Date.parse(source.computedAt(value)) < Date.parse(source.computedAt(current))) {
      return;
    }
    set({ value, error: null });
  };

  const onFocus = () => {
    // 팝오버를 다시 열면 한 번 부른다(Rust가 캐시를 바로 주고, 오래됐으면 뒤에서 갱신한다).
    if (listeners.size > 0) store.refresh();
  };

  const store: SnapshotStore<T> = {
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    getSnapshot: () => state,
    refresh() {
      if (!unlisten) {
        unlisten = source.listen(accept);
        unlisten.catch(() => {});
        window.addEventListener("focus", onFocus);
      }
      source
        .fetch()
        .then(accept, (reason: unknown) =>
          set({ value: state.value, error: errorMessage(reason) }),
        );
    },
    reset() {
      window.removeEventListener("focus", onFocus);
      void unlisten?.then((f) => f()).catch(() => {});
      unlisten = null;
      state = { value: undefined, error: null };
    },
  };
  return store;
}

export const ratesStore = createSnapshotStore<RateSnapshot>({
  fetch: getRates,
  listen: onRatesUpdated,
  computedAt: (snapshot) => snapshot.computedAt,
});

export const contextStore = createSnapshotStore<ContextSnapshot>({
  fetch: () => getContext(),
  listen: onContextUpdated,
  computedAt: (snapshot) => snapshot.computedAt,
});

export const metricsStore = createSnapshotStore<MetricsSnapshot>({
  fetch: getMetrics,
  listen: onMetricsUpdated,
  computedAt: (snapshot) => snapshot.computedAt,
});

/** 저장소 값을 읽는다. 마운트(또는 `refreshKey` 변경)할 때 command를 한 번 부른다. */
export function useSnapshot<T>(store: SnapshotStore<T>, refreshKey: unknown): StoreState<T> {
  useEffect(() => {
    store.refresh();
  }, [store, refreshKey]);
  return useSyncExternalStore(store.subscribe, store.getSnapshot);
}

const MINUTE_MS = 60_000;

/** "방금"(1분 미만), "N분 전 기준", 60분 이상은 "N시간 전 기준". */
export function formatAge(computedAt: string, now: number = Date.now()): string {
  const minutes = Math.floor((now - Date.parse(computedAt)) / MINUTE_MS);
  if (!(minutes >= 1)) return t.age.justNow;
  if (minutes < 60) return t.age.minutes(minutes);
  return t.age.hours(Math.floor(minutes / 60));
}
