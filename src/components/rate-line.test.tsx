import { act, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it } from "vitest";

import { codexDetailText, codexTokensText, RateLine } from "@/components/rate-line";
import type { CodexRate, RateSnapshot } from "@/lib/ipc";

const bounds = (min: number, max: number | null) => ({ min, max });
const estimated: CodexRate = {
  status: "estimated",
  r2: 0.92,
  windows: 40,
  excludedWindows: 1,
  perPercent: { cachedInput: 742_000, uncachedInput: 66_000, output: 5_500 },
  foldRange: {
    cachedInput: bounds(717_000, 810_000),
    uncachedInput: bounds(56_000, 76_000),
    output: bounds(4_900, 6_200),
  },
  pattern: null,
  composition: null,
  sessions: [],
};

const pattern = {
  days: 14,
  windows: 19,
  requests: { median: 139, p90: 213 },
  misses: { median: 2, p90: 5 },
};

/** Claude 환율이 판정을 통과하기 전(R² 낮음). */
const collecting: CodexRate = {
  ...estimated,
  status: "unreliable",
  r2: 0.12,
  windows: 3,
  perPercent: null,
  foldRange: null,
};

function snapshot(codex: CodexRate = estimated, claude: CodexRate = collecting): RateSnapshot {
  return {
    computedAt: new Date().toISOString(),
    codex,
    kiro: [
      { model: "claude-sonnet-5.5", turns: 30, creditsPerTurn: 0.52 },
      { model: "claude-opus-5.5", turns: 10, creditsPerTurn: 0.906 },
    ],
    claude,
  };
}

function renderWith(agent: "claude" | "codex" | "kiro", value: RateSnapshot) {
  let calls = 0;
  mockIPC((cmd) => {
    if (cmd !== "get_rates") return undefined;
    calls += 1;
    return value;
  });
  render(<RateLine agent={agent} breakdownDays={14} />);
  return () => calls;
}

describe("RateLine", () => {
  it("shows the estimated Codex tokens per 1% with a ~ prefix and an estimate tooltip", async () => {
    const calls = renderWith("codex", snapshot({ ...estimated, pattern }));
    // 139 / (137/742K + 2/66K) = 약 646.7K
    const value = await screen.findByText("~646.7K 토큰");
    expect(value.getAttribute("title")).toContain("예상: 기록으로 계산한 값");
    expect(calls()).toBe(1);
  });

  it("keeps statistics in the value tooltip and out of the visible text, never showing R²", async () => {
    renderWith("codex", snapshot({ ...estimated, pattern }));
    const value = await screen.findByText("~646.7K 토큰");
    const section = value.closest("section")!;
    const title = value.getAttribute("title")!.replace(/\u00a0/g, " ");
    expect(title).toContain("캐시 입력 742K · 입력 66K · 출력 5.5K");
    expect(title).toContain("5시간 구간 40개");
    expect(title).not.toContain("R²");
    expect(section.querySelector("[data-slot=details]")).toBeNull();
    expect(section.textContent).not.toMatch(/R²|환율|%p|p90|추정|실측|구간/);
  });

  it("shows the measured request pattern with the uncached request share", async () => {
    renderWith("codex", snapshot({ ...estimated, pattern }));
    const line = await screen.findByText("5시간마다 보통 139번 요청");
    expect(line.getAttribute("title")).toContain("정확: 측정한 값");
    expect(line.getAttribute("title")).toContain("바쁠 때(상위 10%): 요청 213번");
    expect(line.getAttribute("title")).toContain("중앙값끼리 나눈 근사값");
    const section = line.closest("section")!;
    expect(section.textContent).toContain("캐시되지 않은 요청 ~1.4% · 최근 14일");
    expect(section.textContent).not.toContain("바쁠 때");
  });

  it("says there was no recent use when the rate is known but the period has no 5-hour windows", async () => {
    renderWith("codex", snapshot());
    await screen.findByText("최근 14일 사용 없음");
    expect(screen.queryByText("아직 알 수 없어요")).toBeNull();
    expect(screen.queryByText(/번 요청/)).toBeNull();
  });

  it("shows a status sentence instead of numbers when samples are insufficient", async () => {
    renderWith(
      "codex",
      snapshot({
        ...estimated,
        status: "insufficient",
        r2: null,
        windows: 8,
        perPercent: null,
        foldRange: null,
      }),
    );
    expect(await screen.findByText("기록이 부족해요")).toBeTruthy();
    expect(screen.queryByText(/^~/)).toBeNull();
  });

  it("shows Kiro credits per question as a model table under a short headline", async () => {
    renderWith("kiro", snapshot());
    const value = await screen.findByText("모델마다 달라요");
    expect(value.getAttribute("title")).toBe("정확: 측정한 값");
    const rows = within(value.closest("section")!).getAllByRole("row");
    // 머리줄 + 질문 수 많은 모델 먼저.
    expect(rows.map((r) => r.textContent)).toEqual([
      "모델크레딧월 한도 몫",
      "sonnet-5.50.52—",
      "opus-5.50.91—",
    ]);
    expect(rows[1].getAttribute("title")).toBe("sonnet-5.5: 질문 30번 기준");
  });

  it("shows the single model's credits in the headline", async () => {
    const one = { ...snapshot(), kiro: [snapshot().kiro[0]] };
    renderWith("kiro", one);
    expect(await screen.findByText("0.52 크레딧")).toBeTruthy();
  });

  it("adds the monthly limit share of one turn per model only when the limit is known", async () => {
    mockIPC((cmd) => (cmd === "get_rates" ? snapshot() : undefined));
    const { rerender } = render(<RateLine agent="kiro" breakdownDays={14} kiroLimit={5_000} />);
    // 0.52 ÷ (5,000 / 100) = 0.0104%
    expect(await screen.findByText("0.01%")).toBeTruthy();
    rerender(<RateLine agent="kiro" breakdownDays={14} />);
    await waitFor(() => expect(screen.queryByText("0.01%")).toBeNull());
  });

  it("says Kiro records are being collected when no turns were found", async () => {
    renderWith("kiro", { ...snapshot(), kiro: [] });
    expect(await screen.findByText("기록 모으는 중")).toBeTruthy();
  });

  it("shows a Claude token range in the main text when R² is only good enough for a range", async () => {
    const range: CodexRate = {
      ...estimated,
      status: "range_only",
      r2: 0.64,
      windows: 35,
      perPercent: null,
      pattern,
    };
    renderWith("claude", snapshot(estimated, range));
    const value = await screen.findByText("~612.9K~711.2K 토큰");
    expect(screen.getByText(/기록과 덜 맞아 범위로 보여 줘요/)).toBeTruthy();
    expect(value.getAttribute("title")?.replace(/\u00a0/g, " ")).toContain("5시간 구간 35개");
    expect(value.closest("section")!.textContent).not.toContain("R²");
    expect(screen.queryByText(/기록 모으는 중/)).toBeNull();
  });

  it("says Claude can't tell yet without showing R², keeping the window count in the tooltip", async () => {
    renderWith("claude", snapshot(estimated));
    const value = await screen.findByText("아직 알 수 없어요");
    expect(value.closest("section")!.textContent).not.toMatch(/R²|구간/);
    expect(value.getAttribute("title")?.replace(/\u00a0/g, " ")).toBe("5시간 구간 3개");
    expect(screen.queryByText(/^~/)).toBeNull();
  });

  it("tells what to do instead of repeating the title when Claude has no 5-hour windows at all", async () => {
    renderWith("claude", snapshot(estimated, { ...collecting, r2: null, windows: 0 }));
    await screen.findByText("아직 기록이 없어요");
    expect(screen.getByText(/Claude Code 기록이 필요해요/)).toBeTruthy();
    expect(screen.queryByText(/기록 모으는 중/)).toBeNull();
  });

  it("shows the estimated Claude tokens per 1% and request pattern like Codex once the Claude rate is estimated", async () => {
    renderWith("claude", snapshot(collecting, { ...estimated, pattern }));
    const value = await screen.findByText("~646.7K 토큰");
    expect(value.getAttribute("title")).toContain("예상: 기록으로 계산한 값");
    expect(screen.getByText("5시간마다 보통 139번 요청").getAttribute("title")).toContain(
      "정확: 측정한 값",
    );
    expect(screen.queryByText(/기록 모으는 중/)).toBeNull();
  });
});

describe("RateLine cache", () => {
  const minutesAgo = (n: number) => new Date(Date.now() - n * 60_000).toISOString();

  it("shows the last value immediately when the tab is opened again", async () => {
    mockIPC((cmd) => (cmd === "get_rates" ? snapshot() : undefined));
    const first = render(<RateLine agent="kiro" breakdownDays={14} />);
    await screen.findByText("모델마다 달라요");
    first.unmount();

    // 같은 렌더 안에서 바로 보여야 한다(await 없음).
    render(<RateLine agent="kiro" breakdownDays={14} />);
    expect(screen.getByText("모델마다 달라요")).toBeTruthy();
    expect(screen.queryByText("불러오는 중…")).toBeNull();
  });

  it("replaces the value when rates-updated arrives and shows how old it is", async () => {
    mockIPC(
      (cmd) => (cmd === "get_rates" ? { ...snapshot(), computedAt: minutesAgo(12) } : undefined),
      { shouldMockEvents: true },
    );
    render(<RateLine agent="kiro" breakdownDays={14} />);
    expect(await screen.findByText("12분 전 기준")).toBeTruthy();

    await act(() => emit("rates-updated", { ...snapshot(), kiro: [], computedAt: minutesAgo(0) }));
    expect(await screen.findByText("기록 모으는 중")).toBeTruthy();
    expect(screen.getByText("방금")).toBeTruthy();
  });

  it("keeps the newer value when an older response arrives late", async () => {
    mockIPC(
      (cmd) => (cmd === "get_rates" ? { ...snapshot(), computedAt: minutesAgo(30) } : undefined),
      {
        shouldMockEvents: true,
      },
    );
    render(<RateLine agent="kiro" breakdownDays={14} />);
    await screen.findByText("30분 전 기준");
    await act(() => emit("rates-updated", { ...snapshot(), computedAt: minutesAgo(0) }));
    await screen.findByText("방금");
    await act(() => emit("rates-updated", { ...snapshot(), computedAt: minutesAgo(45) }));
    expect(screen.getByText("방금")).toBeTruthy();
  });
});

describe("codexTokensText and codexDetailText", () => {
  it("shows a token range and per-type fold ranges when R² is in the range band", () => {
    const rate: CodexRate = {
      ...estimated,
      status: "range_only",
      r2: 0.5,
      perPercent: null,
      foldRange: { ...estimated.foldRange!, output: bounds(4_900, null) },
      pattern,
    };
    expect(codexTokensText(rate, 14)).toMatchObject({
      value: "~612.9K~711.2K 토큰",
      isNumber: true,
    });
    // 한 항만 상한이 없으면 다른 항이 상한을 정한다. 두 항 모두 없을 때만 "≥".
    const open = {
      ...rate.foldRange!,
      cachedInput: bounds(717_000, null),
      uncachedInput: bounds(56_000, null),
    };
    expect(codexTokensText({ ...rate, foldRange: open }, 14).value).toBe("≥612.9K 토큰");
    expect(codexDetailText(rate)).toBe("1%당 캐시 입력 717K~810K · 입력 56K~76K · 출력 ≥4.9K 토큰");
  });

  it("says no recent use for an empty pattern, but keeps the failure text when the numbers cannot be computed", () => {
    const empty = { ...pattern, requests: { median: 0, p90: 0 } };
    expect(codexTokensText({ ...estimated, pattern: empty }, 7).value).toBe("최근 7일 사용 없음");
    const noCoefficients: CodexRate = {
      ...estimated,
      perPercent: { cachedInput: null, uncachedInput: null, output: 5_500 },
      pattern,
    };
    expect(codexTokensText(noCoefficients, 14)).toMatchObject({
      value: "아직 알 수 없어요",
      hint: "기록만으로는 정확히 계산하지 못했어요.",
    });
  });

  it("says it cannot be known yet when R² is low or there is no data", () => {
    const low: CodexRate = {
      ...estimated,
      status: "unreliable",
      r2: 0.12,
      perPercent: null,
      foldRange: null,
    };
    expect(codexTokensText(low, 14).value).toBe("아직 알 수 없어요");
    expect(codexTokensText({ ...low, status: "no_data", r2: null, windows: 0 }, 14).value).toBe(
      "기록 모으는 중",
    );
  });
});
