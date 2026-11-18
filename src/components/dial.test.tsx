import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Dial } from "@/components/dial";
import { t } from "@/lib/i18n";

const fillOf = (container: HTMLElement) => container.querySelectorAll("circle")[1];

describe("Dial", () => {
  it("names and shows the rounded percent", () => {
    const { container } = render(<Dial percent={42.6} color="red" isLow={false} />);
    expect(screen.getByRole("img").getAttribute("aria-label")).toBe(t.popover.dial(43));
    expect(container.textContent).toContain("43");
    expect(fillOf(container).getAttribute("stroke-dasharray")).toBe("42.6 100");
  });

  it("still draws a sliver at 0 percent", () => {
    const { container } = render(<Dial percent={0} color="red" isLow={false} />);
    expect(fillOf(container).getAttribute("stroke-dasharray")).toBe("0.5 100");
  });

  it("switches to the destructive color when low", () => {
    const { container } = render(<Dial percent={10} color="red" isLow />);
    expect(fillOf(container).getAttribute("stroke")).toBe("currentColor");
    expect(screen.getByRole("img").className).toContain("text-destructive");
  });
});
