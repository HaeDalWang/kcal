import { render, screen, within } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";

import {
  afterText,
  changeOf,
  HealthCompare,
  healthStore,
  isPartial,
} from "@/components/health-compare";
import type { HealthReport, PeriodStats, PrescriptionEffect } from "@/lib/ipc";
import health from "@/test/fixtures/ipc/health-report.json";

const report = health as HealthReport;

afterEach(() => healthStore.reset());

function serve(value: HealthReport) {
  mockIPC((cmd) => (cmd === "get_health" ? value : null), { shouldMockEvents: true });
}

describe("health compare", () => {
  it("shows the change as an arrow and percent, and nothing when a side is unknown", () => {
    expect(changeOf(100, 80)).toEqual({ text: "↓20%", isUp: false });
    expect(changeOf(2, 3)).toEqual({ text: "↑50%", isUp: true });
    expect(changeOf(100, 100.5)).toEqual({ text: "비슷", isUp: null });
    expect(changeOf(null, 3)).toBeNull();
    expect(changeOf(0, 3)).toBeNull();
  });

  it("labels the after period as running, finished or undone", () => {
    const base = report.prescriptions[1] as PrescriptionEffect;
    const after = (elapsedDays: number): PeriodStats => ({ ...base.after, elapsedDays });
    expect(afterText({ ...base, undoneAt: null, after: after(2.4) }, 7)).toBe("3일째");
    expect(afterText({ ...base, undoneAt: null, after: after(0) }, 7)).toBe("1일째");
    expect(afterText({ ...base, undoneAt: null, after: after(7) }, 7)).toBe("7일 지남");
    expect(afterText(report.prescriptions[0], 7)).toMatch(/^되돌림 /);
    const [claude] = report.weekly;
    expect(isPartial(claude.before, claude.after)).toBe(true);
    expect(isPartial(report.weekly[2].before, report.weekly[2].after)).toBe(false);
  });

  it("renders weekly rows with no values as one no-data line and an empty prescription list", async () => {
    serve({ ...report, prescriptions: [] });
    render(<HealthCompare refreshKey="7" />);
    expect(await screen.findByText("아직 적용한 처방이 없어요.", { exact: false })).toBeTruthy();
    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(within(rows[0]).getByText("기록 일부만")).toBeTruthy();
    // Kiro는 기록이 없어 0이나 "— → —" 줄이 아니라 "기록 없음" 한 줄로 보인다.
    expect(within(rows[2]).getByText("기록 없음")).toBeTruthy();
    expect(within(rows[2]).queryByText("—")).toBeNull();
    expect(within(rows[2]).queryByText("0")).toBeNull();
  });

  it("names check records by their action and switches with on/off", async () => {
    const effort = report.prescriptions[1];
    const check: PrescriptionEffect = {
      ...effort,
      id: "9",
      name: "생각 깊이 옛 설정 정리",
      target: "claude/check:D-claude-effort-level:remove",
    };
    serve({ ...report, prescriptions: [check, effort] });
    render(<HealthCompare refreshKey="7" />);
    expect(await screen.findByText("생각 깊이 옛 설정 정리")).toBeTruthy();
    expect(screen.queryByText("생각 깊이 옛 설정 정리 끔")).toBeNull();
    expect(screen.getByText("synthetic@m 끔")).toBeTruthy();
  });
});
