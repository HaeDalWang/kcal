import { describe, expect, it } from "vitest";

import { errorMessage, isAppError } from "@/lib/ipc";

describe("errorMessage", () => {
  it("shows the Rust AppError message, an Error message, or the string itself", () => {
    expect(errorMessage({ kind: "invalid_input", message: "범위 밖" })).toBe("범위 밖");
    expect(errorMessage(new Error("boom"))).toBe("boom");
    expect(errorMessage("plain tauri error")).toBe("plain tauri error");
  });

  it("does not treat an object without a string kind and message as an AppError", () => {
    expect(isAppError({ kind: "internal" })).toBe(false);
    expect(isAppError({ kind: 1, message: "x" })).toBe(false);
    expect(isAppError(null)).toBe(false);
  });
});
