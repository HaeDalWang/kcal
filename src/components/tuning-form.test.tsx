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
            trayIcon: "bars",
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

it("saves protected folders as an array alongside numeric session tuning", async () => {
  const { fireEvent, screen, waitFor } = await import("@testing-library/react");
  const onSave = vi.fn(async () => {});
  render(
    <TuningForm
      settings={{
        enabledAgents: ["claude"],
        trayIcon: "bars",
        tuning: DEFAULT_TUNING,
        tuningSpec: TUNING_SPEC,
        reason: null,
        language: "ko",
        resolvedLanguage: "ko",
      }}
      isSaving={false}
      onSave={onSave}
    >
      {null}
    </TuningForm>,
  );
  fireEvent.click(screen.getByText("조절 값 (기본값이면 충분해요)"));
  fireEvent.change(screen.getByLabelText("세션 보호 폴더"), {
    target: { value: "/work/protected\n\n /work/other " },
  });
  fireEvent.click(await screen.findByRole("button", { name: "저장" }));
  await waitFor(() =>
    expect(onSave).toHaveBeenCalledWith({
      ...DEFAULT_TUNING,
      sessionProtectedFolders: ["/work/protected", "/work/other"],
    }),
  );
});
