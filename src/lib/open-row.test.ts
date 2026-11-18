import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { checkRowKey, switchRowKey, useOpenRow } from "@/lib/open-row";

describe("useOpenRow", () => {
  it("keeps one row open: another row replaces it and the same row folds it", () => {
    const { result } = renderHook(() => useOpenRow());
    act(() => result.current.toggle(checkRowKey("a")));
    expect(result.current.key).toBe("check:a");
    act(() => result.current.toggle(switchRowKey("b")));
    expect(result.current.key).toBe("switch:b");
    act(() => result.current.toggle(switchRowKey("b")));
    expect(result.current.key).toBeNull();
  });

  it("uses the shared state when one is given", () => {
    const shared = { key: "check:x", toggle: () => {} };
    const { result } = renderHook(() => useOpenRow(shared));
    expect(result.current).toBe(shared);
  });
});
