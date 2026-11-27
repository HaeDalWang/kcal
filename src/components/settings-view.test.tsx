import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it, vi } from "vitest";

import { SettingsView } from "@/components/settings-view";
import { fieldsInMessage, parseField, toDisplay } from "@/components/tuning-form";
import { AGENTS, type Agent, type Settings, type Tuning, type TuningKey } from "@/lib/ipc";
import { DEFAULT_TUNING, TUNING_SPEC } from "@/test/tuning";

function settingsWith(tuning: Tuning = DEFAULT_TUNING, enabledAgents: Agent[] = [...AGENTS]) {
  return {
    enabledAgents,
    trayIcon: "bars",
    tuning,
    tuningSpec: TUNING_SPEC,
    reason: null,
    language: "auto",
    resolvedLanguage: "ko",
  } satisfies Settings;
}

function spec(key: TuningKey) {
  const found = TUNING_SPEC.find((f) => f.key === key);
  if (!found) throw new Error(`no spec for ${key}`);
  return found;
}

/** `set_settings` 호출 인자를 모으고 `reply`로 답한다. */
function mockSave(reply: (args: { enabledAgents: Agent[]; tuning?: Tuning }) => unknown) {
  const calls: { enabledAgents: Agent[]; tuning?: Tuning }[] = [];
  mockIPC((cmd, args) => {
    if (cmd !== "set_settings") return undefined;
    const typed = args as { enabledAgents: Agent[]; tuning?: Tuning };
    calls.push(typed);
    return reply(typed);
  });
  return calls;
}

const input = (label: RegExp) => screen.getByLabelText(label) as HTMLInputElement;
/** 조절 값은 접혀 있고 저장 줄은 펼쳐야 보인다. */
function openTuning() {
  // jsdom은 summary 클릭으로 details를 열지 않는다. 브라우저처럼 open + toggle 이벤트를 낸다.
  const details = screen.getByText(/^조절 값/).closest("details") as HTMLDetailsElement;
  if (details.open) return;
  details.open = true;
  fireEvent(details, new Event("toggle"));
}
const saveButton = () => {
  openTuning();
  return screen.getByRole("button", { name: "저장" });
};

describe("SettingsView", () => {
  it("renders three tray icon options and immediately saves the selected trayIcon", async () => {
    const onSaved = vi.fn();
    const calls = mockSave((args) => ({ ...settingsWith(), ...args }));
    render(<SettingsView settings={settingsWith()} onSaved={onSaved} onBack={() => {}} />);
    const control = screen.getByRole("combobox", { name: "메뉴바 아이콘" });
    expect(
      within(control)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["막대", "남은 % 숫자", "아이콘만"]);
    expect((control as HTMLSelectElement).value).toBe("bars");
    fireEvent.change(control, { target: { value: "percent" } });
    await waitFor(() =>
      expect(onSaved).toHaveBeenCalledWith({ ...settingsWith(), trayIcon: "percent" }),
    );
    expect(calls).toEqual([{ enabledAgents: [...AGENTS], trayIcon: "percent" }]);
    fireEvent.change(control, { target: { value: "icon" } });
    await waitFor(() => expect(calls).toHaveLength(2));
    expect(calls[1]).toEqual({ enabledAgents: [...AGENTS], trayIcon: "icon" });
  });

  it("keeps the saved tray icon and reports a failed save", async () => {
    mockSave(() => Promise.reject({ kind: "internal", message: "저장 실패" }));
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    const control = screen.getByRole("combobox", { name: "메뉴바 아이콘" });
    fireEvent.change(control, { target: { value: "percent" } });
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "저장 실패");
    expect((control as HTMLSelectElement).value).toBe("bars");
  });

  it("reads OFF from the OS without enabling autostart", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === "get_autostart") return false;
      if (cmd === "get_autostart_path_warning") return false;
    });
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    const control = screen.getByRole("switch", { name: "로그인 시 시작" });
    expect(control.getAttribute("aria-checked")).toBe("false");
    await waitFor(() => expect(control.hasAttribute("data-disabled")).toBe(false));
    expect(calls).toContain("get_autostart");
    expect(calls).not.toContain("set_autostart");
  });

  it("sets autostart and displays the OS readback rather than the requested state", async () => {
    const calls: unknown[] = [];
    let readback = false;
    mockIPC((cmd, args) => {
      if (cmd === "get_autostart") return false;
      if (cmd === "set_autostart") {
        calls.push(args);
        return readback;
      }
    });
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    const control = screen.getByRole("switch", { name: "로그인 시 시작" });
    await waitFor(() => expect(control.hasAttribute("data-disabled")).toBe(false));
    fireEvent.click(control);
    await waitFor(() => expect(calls).toEqual([{ enabled: true }]));
    await waitFor(() => expect(control.hasAttribute("data-disabled")).toBe(false));
    expect(control.getAttribute("aria-checked")).toBe("false");
    readback = true;
    fireEvent.click(control);
    await waitFor(() => expect(control.getAttribute("aria-checked")).toBe("true"));
    readback = false;
    fireEvent.click(control);
    await waitFor(() => expect(control.getAttribute("aria-checked")).toBe("false"));
    expect(calls).toEqual([{ enabled: true }, { enabled: true }, { enabled: false }]);
  });

  it("shows autostart errors and keeps the last OS state", async () => {
    mockIPC((cmd) => {
      if (cmd === "get_autostart") return true;
      if (cmd === "set_autostart")
        return Promise.reject({ kind: "internal", message: "자동 시작을 변경하지 못했어요." });
    });
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    const control = screen.getByRole("switch", { name: "로그인 시 시작" });
    await waitFor(() => expect(control.getAttribute("aria-checked")).toBe("true"));
    fireEvent.click(control);
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "자동 시작을 변경하지 못했어요.",
    );
    expect(control.getAttribute("aria-checked")).toBe("true");
  });

  it("keeps autostart disabled and shows an initial OS read error", async () => {
    mockIPC((cmd) =>
      cmd === "get_autostart"
        ? Promise.reject({ kind: "internal", message: "자동 시작 상태를 읽지 못했어요." })
        : false,
    );
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "자동 시작 상태를 읽지 못했어요.",
    );
    expect(
      screen.getByRole("switch", { name: "로그인 시 시작" }).hasAttribute("data-disabled"),
    ).toBe(true);
  });

  it("shows the saved-path hint only when Rust reports a non-Applications macOS path", async () => {
    mockIPC((cmd) => cmd === "get_autostart_path_warning");
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    expect(await screen.findByText(/경로가 저장돼요/)).toBeTruthy();
  });

  it("shows the app version at the bottom", async () => {
    mockIPC((cmd) => (cmd === "plugin:app|version" ? "0.1.0" : undefined));
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    expect(await screen.findByText("kcal 0.1.0")).toBeTruthy();
  });

  it("disables the last enabled agent so it cannot be turned off", () => {
    mockIPC(() => undefined);
    render(
      <SettingsView
        settings={settingsWith(DEFAULT_TUNING, ["kiro"])}
        onSaved={() => {}}
        onBack={() => {}}
      />,
    );
    const kiro = screen.getByRole("checkbox", { name: "Kiro" });
    expect(kiro.getAttribute("aria-checked")).toBe("true");
    expect(kiro.hasAttribute("data-disabled")).toBe(true);
    expect(screen.getByRole("checkbox", { name: "Claude" }).hasAttribute("data-disabled")).toBe(
      false,
    );
  });

  it("saves the remaining agents when one is turned off", async () => {
    const calls = mockSave((args) => settingsWith(DEFAULT_TUNING, args.enabledAgents));
    const onSaved = vi.fn();
    render(<SettingsView settings={settingsWith()} onSaved={onSaved} onBack={() => {}} />);
    fireEvent.click(screen.getByRole("checkbox", { name: "Kiro" }));
    await waitFor(() =>
      expect(onSaved).toHaveBeenCalledWith(settingsWith(DEFAULT_TUNING, ["claude", "codex"])),
    );
    expect(calls).toEqual([{ enabledAgents: ["claude", "codex"] }]);
  });

  it("keeps the tuning fields folded and hides the save row until opened", () => {
    mockIPC(() => undefined);
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    expect((screen.getByText(/^조절 값/).closest("details") as HTMLDetailsElement).open).toBe(
      false,
    );
    expect(screen.queryByRole("button", { name: "저장" })).toBeNull();
    expect(saveButton().hasAttribute("disabled")).toBe(true);
  });

  it("converts between display units and stored units both ways", () => {
    expect(toDisplay("apiIntervalSecs", 300)).toBe("5");
    expect(parseField("5", spec("apiIntervalSecs"))).toEqual({ value: 300 });
    expect(toDisplay("claudeLogTailBytes", 65536)).toBe("64");
    expect(parseField("64", spec("claudeLogTailBytes"))).toEqual({ value: 65536 });
    expect(toDisplay("httpTimeoutSecs", 15)).toBe("15");
    expect(parseField("0.7", spec("advanced.holdoutTrain"))).toEqual({ value: 0.7 });
    expect(parseField("1.5", spec("apiIntervalSecs"))).toEqual({ error: "정수로 입력하세요." });
    expect(parseField("", spec("lowPercent"))).toEqual({ error: "숫자를 입력하세요." });
  });

  it("finds every Rust tuning field named in a range error, in both languages", () => {
    const keys = TUNING_SPEC.map((s) => s.key);
    const joined = keys.join(", ");
    expect(fieldsInMessage(`Some values are out of range: ${joined}`)).toEqual(keys);
    expect(fieldsInMessage(`tuning 값이 범위를 벗어났어요: ${joined}`)).toEqual(keys);
    expect(fieldsInMessage("toString constructor: __proto__")).toEqual([]);
  });

  it("enables save only after a change and stores minutes as seconds", async () => {
    const calls = mockSave((args) => settingsWith(args.tuning));
    const onSaved = vi.fn();
    render(<SettingsView settings={settingsWith()} onSaved={onSaved} onBack={() => {}} />);
    expect(saveButton().hasAttribute("disabled")).toBe(true);

    fireEvent.change(input(/^Kiro 간격/), { target: { value: "15" } });
    expect(saveButton().hasAttribute("disabled")).toBe(false);
    fireEvent.click(saveButton());

    const expected = { ...DEFAULT_TUNING, kiroIntervalSecs: 900 };
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith(settingsWith(expected)));
    expect(calls).toEqual([{ enabledAgents: [...AGENTS], tuning: expected }]);
  });

  it("saves the refresh minimum gap in seconds", async () => {
    const calls = mockSave((args) => settingsWith(args.tuning));
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    fireEvent.change(input(/^새로고침 최소 간격/), { target: { value: "60" } });
    fireEvent.click(saveButton());
    await waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0].tuning).toEqual({ ...DEFAULT_TUNING, refreshMinGapSecs: 60 });
  });

  it("saves the optimization record days and rejects values outside 1 to 180", async () => {
    const calls = mockSave((args) => settingsWith(args.tuning));
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    const days = input(/^최적화 판정에 쓰는 최근 기록\(일\)/);
    expect(days.value).toBe("30");
    fireEvent.change(days, { target: { value: "181" } });
    fireEvent.click(saveButton());
    expect(calls).toHaveLength(0);
    fireEvent.change(days, { target: { value: "14" } });
    fireEvent.click(saveButton());
    await waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0].tuning).toEqual({ ...DEFAULT_TUNING, checkModelDays: 14 });
  });

  it("saves the check alert card count and rejects values outside 1 to 20", async () => {
    const calls = mockSave((args) => settingsWith(args.tuning));
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);
    const count = input(/^점검 알림을 띄울 카드 수/);
    expect(count.value).toBe("1");
    fireEvent.change(count, { target: { value: "21" } });
    fireEvent.click(saveButton());
    expect(calls).toHaveLength(0);
    fireEvent.change(count, { target: { value: "3" } });
    fireEvent.click(saveButton());
    await waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0].tuning).toEqual({ ...DEFAULT_TUNING, checkAlertMin: 3 });
  });

  it("does not save an out-of-range value and shows the error on that field", () => {
    const calls = mockSave(() => settingsWith());
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);

    fireEvent.change(input(/^경고색 기준/), { target: { value: "100" } });
    fireEvent.click(saveButton());

    expect(calls).toEqual([]);
    expect(input(/^경고색 기준/).getAttribute("aria-invalid")).toBe("true");
    expect(screen.getByText("1–99% 안이어야 해요.")).toBeTruthy();
    expect(input(/^Kiro 간격/).hasAttribute("aria-invalid")).toBe(false);
  });

  it("shows a server invalid_input on every field it names", async () => {
    const message = "tuning 값이 범위를 벗어났어요: advanced.goodR2, advanced.rangeR2";
    mockSave(() => Promise.reject({ kind: "invalid_input", message }));
    render(<SettingsView settings={settingsWith()} onSaved={() => {}} onBack={() => {}} />);

    fireEvent.change(input(/^숫자 표시 R²/), { target: { value: "0.2" } });
    fireEvent.click(saveButton());

    expect(await screen.findByText(message)).toBeTruthy();
    expect(input(/^숫자 표시 R²/).getAttribute("aria-invalid")).toBe("true");
    expect(input(/^범위 표시 R²/).getAttribute("aria-invalid")).toBe("true");
    expect(input(/^Kiro 간격/).hasAttribute("aria-invalid")).toBe(false);
    expect((screen.getByText(/^고급/).closest("details") as HTMLDetailsElement).open).toBe(true);
  });

  it("fills defaults with the reset button and saves the default tuning", async () => {
    const calls = mockSave((args) => settingsWith(args.tuning));
    const changed: Tuning = {
      ...DEFAULT_TUNING,
      apiIntervalSecs: 120,
      lowPercent: 35,
      advanced: { ...DEFAULT_TUNING.advanced, goodR2: 0.9 },
    };
    render(<SettingsView settings={settingsWith(changed)} onSaved={() => {}} onBack={() => {}} />);
    expect(input(/^Claude·Codex 간격/).value).toBe("2");

    fireEvent.click((openTuning(), screen.getByRole("button", { name: "기본값으로" })));
    expect(input(/^Claude·Codex 간격/).value).toBe("5");
    expect(input(/^경고색 기준/).value).toBe("20");
    fireEvent.click(saveButton());

    await waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0].tuning).toEqual(DEFAULT_TUNING);
  });
});
