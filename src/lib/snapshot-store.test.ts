import { describe, expect, it } from "vitest";

import { createSnapshotStore, formatAge } from "@/lib/snapshot-store";

describe("formatAge", () => {
  const now = Date.parse("2026-11-04T12:00:00Z");
  const ago = (ms: number) => new Date(now - ms).toISOString();

  it("says 방금 under one minute, including a clock slightly ahead", () => {
    expect(formatAge(ago(59_000), now)).toBe("방금");
    expect(formatAge(ago(-5_000), now)).toBe("방금");
  });

  it("counts whole minutes, then hours", () => {
    expect(formatAge(ago(60_000), now)).toBe("1분 전 기준");
    expect(formatAge(ago(59 * 60_000), now)).toBe("59분 전 기준");
    expect(formatAge(ago(125 * 60_000), now)).toBe("2시간 전 기준");
  });
});

describe("createSnapshotStore", () => {
  type Snap = { computedAt: string; n: number };
  const snap = (n: number, at: string): Snap => ({ computedAt: at, n });

  function setup(fetch: () => Promise<Snap>) {
    let push: (value: Snap) => void = () => {};
    const store = createSnapshotStore<Snap>({
      fetch,
      listen: (handler) => {
        push = handler;
        return Promise.resolve(() => {});
      },
      computedAt: (s) => s.computedAt,
    });
    return { store, push: (value: Snap) => push(value) };
  }

  it("ignores a late older response so it cannot overwrite a newer value", () => {
    const { store, push } = setup(() => Promise.resolve(snap(1, "2026-11-04T12:00:00Z")));
    store.refresh();
    push(snap(2, "2026-11-04T12:05:00Z"));
    push(snap(1, "2026-11-04T12:01:00Z"));
    expect(store.getSnapshot().value?.n).toBe(2);
    store.reset();
  });

  it("keeps the last value and records the message when a refresh fails", async () => {
    const { store, push } = setup(() => Promise.reject(new Error("boom")));
    store.refresh();
    push(snap(1, "2026-11-04T12:00:00Z"));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(store.getSnapshot()).toEqual({ value: snap(1, "2026-11-04T12:00:00Z"), error: "boom" });
    store.reset();
  });
});
