import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { StateText } from "@/components/state-text";
import { t } from "@/lib/i18n";

describe("StateText", () => {
  it("uses the given empty text and falls back to the default one", () => {
    const { rerender } = render(<StateText kind="empty">First run</StateText>);
    expect(screen.getByText("First run")).toBeTruthy();
    rerender(<StateText kind="empty" />);
    expect(screen.getByText(t.state.noValue)).toBeTruthy();
  });

  it("replaces the alert element instead of editing its text when the message changes", () => {
    const { rerender } = render(<StateText kind="error">first</StateText>);
    const before = screen.getByRole("alert");
    rerender(<StateText kind="error">second</StateText>);
    const after = screen.getByRole("alert");
    expect(after).not.toBe(before);
    expect(after.textContent).toBe("second");
  });

  it("keeps the same element when the message is unchanged", () => {
    const { rerender } = render(<StateText kind="error">same</StateText>);
    const before = screen.getByRole("alert");
    rerender(<StateText kind="error">same</StateText>);
    expect(screen.getByRole("alert")).toBe(before);
  });
});
