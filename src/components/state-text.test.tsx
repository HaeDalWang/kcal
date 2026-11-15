import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { StateText } from "@/components/state-text";

describe("StateText", () => {
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
