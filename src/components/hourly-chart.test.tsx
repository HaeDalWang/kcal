import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { HourlyChart } from "@/components/hourly-chart";

const requests = (entries: Record<number, number>) =>
  Array.from({ length: 24 }, (_, hour) => entries[hour] ?? 0);

describe("HourlyChart", () => {
  it("scales bars to the busiest hour, keeps tiny ones visible, and skips empty hours", () => {
    const { container } = render(
      <HourlyChart
        agent="claude"
        requests={requests({ 3: 100, 9: 50, 15: 1 })}
        noun="요청"
        label="l"
      />,
    );
    const bars = [...container.querySelectorAll("rect")];
    expect(bars.map((bar) => Number(bar.getAttribute("height")))).toEqual([56, 28, 1.5]);
    expect(bars[0].getAttribute("x")).toBe("33");
  });

  it("draws no bars on a day without requests", () => {
    const { container } = render(
      <HourlyChart agent="codex" requests={requests({})} noun="요청" label="l" />,
    );
    expect(container.querySelectorAll("rect")).toHaveLength(0);
  });
});
