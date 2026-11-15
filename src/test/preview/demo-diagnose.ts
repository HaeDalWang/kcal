// Demo backend for the Diagnose scene (`demo.html?scene=diagnose`): the Checks tab's switch commands
// answer from a small in-memory state, so "Turn off" and "Undo" really flip what the UI shows.
// Invented plugin and paths only; nothing is read from the machine. Not part of the app bundle.
import type { InvokeArgs } from "@tauri-apps/api/core";
import type { AppliedSwitch, CoachSwitch, SwitchPreview } from "@/lib/ipc";
import { DASHBOARD_SWITCHES } from "@/test/preview/checks-fixtures";

const TARGET = "claude/plugin:web-search@market";
const NAME = "web-search@market";
const FILE = "/Users/me/.claude/settings.json";
const LOCATION = 'enabledPlugins."web-search@market"';
const BACKUP = `${FILE}.kcal-20261115T093000000Z.bak`;
const BEFORE_HASH = "8f3a1c52d9e07b64";
const TOKENS = 4200;

/** Same shape as the unused k8s-tools row (0 uses in 30 days, user scope), renamed and larger. */
const unused = DASHBOARD_SWITCHES.find((s) => s.target === "claude/plugin:k8s-tools@market");
if (!unused) throw new Error("k8s-tools demo switch is missing.");
const WEB_SEARCH: CoachSwitch = {
  ...unused,
  target: TARGET,
  name: NAME,
  tokens: TOKENS,
  contents: {
    skills: { names: ["search", "summarize-page"], count: 2 },
    agents: { names: [], count: 0 },
    commands: { names: ["/web"], count: 1 },
    mcpServers: { names: ["web-search"], count: 1 },
  },
};
const OTHERS = DASHBOARD_SWITCHES.filter(
  (s) => s.target !== unused.target && s.target !== "claude/plugin:review-kit@market",
);

const applied = (action: "apply" | "undo", id: string, enabled: boolean): AppliedSwitch => ({
  id,
  at: "2026-11-15T09:30:00Z",
  action,
  target: TARGET,
  agent: "claude",
  name: NAME,
  file: FILE,
  backup: BACKUP,
  enabled,
  tokens: TOKENS,
  afterHash: "c41b7e90a2d35f68",
  undoCommand: `cp '${BACKUP}' '${FILE}'`,
  undoOf: action === "undo" ? "demo-1" : null,
});

const preview = (enabled: boolean): SwitchPreview => ({
  target: TARGET,
  enabled,
  file: FILE,
  location: LOCATION,
  before: `"web-search@market": ${!enabled}`,
  after: `"web-search@market": ${enabled}`,
  beforeHash: BEFORE_HASH,
  changed: true,
});

/** Returns a handler for the Diagnose commands, or `undefined` for commands it does not own. */
export function diagnoseBackend() {
  let isOn = true;
  let log: AppliedSwitch[] = [];
  return (cmd: string, payload: InvokeArgs | undefined): unknown => {
    const args = payload as Record<string, unknown> | undefined;
    switch (cmd) {
      case "get_switches":
        return [{ ...WEB_SEARCH, enabled: isOn, isProposed: isOn }, ...OTHERS];
      case "get_coach_log":
        return log;
      case "get_installed_agents":
        return ["claude", "codex"];
      case "preview_switch":
        return preview(args?.enabled === true);
      case "apply_switch":
        isOn = args?.enabled === true;
        log = [applied("apply", "demo-1", isOn), ...log];
        return log[0];
      case "undo_switch":
        isOn = true;
        log = [applied("undo", "demo-2", true), ...log];
        return log[0];
      default:
        return undefined;
    }
  };
}
