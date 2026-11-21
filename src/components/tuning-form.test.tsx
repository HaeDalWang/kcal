import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { TuningForm } from "@/components/tuning-form";
import { DEFAULT_TUNING, TUNING_SPEC } from "@/test/tuning";

describe("TuningForm platform fields", () => {
  it.each([
    ["Mozilla/5.0 (Windows NT 10.0; Win64; x64)", false],
    ["Mozilla/5.0 (Macintosh; Intel Mac OS X)", true],
  ])("shows the Keychain timeout only on macOS (%s)", (userAgent, hasKeychain) => {
    const agent = vi.spyOn(navigator, "userAgent", "get").mockReturnValue(userAgent);
    try {
      const { container } = render(
        <TuningForm
          settings={{
            enabledAgents: ["claude"],
            tuning: DEFAULT_TUNING,
            tuningSpec: TUNING_SPEC,
            reason: null,
            language: "auto",
            resolvedLanguage: "ko",
          }}
          isSaving={false}
          onSave={async () => {}}
        >
          {null}
        </TuningForm>,
      );
      expect(Boolean(container.querySelector("#tuning-keychainTimeoutSecs"))).toBe(hasKeychain);
      expect(container.querySelector("#tuning-httpTimeoutSecs")).not.toBeNull();
    } finally {
      agent.mockRestore();
    }
  });
});
