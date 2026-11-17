// Rust가 보내는 코치 IPC 값(스위치·점검·맡기기)이 ipc.ts 타입과 키·enum 값까지 같은지 확인한다. 공통 계약 테스트는 ipc.test.ts(800줄 아래로 두려고 나눴다).
import { describe, expect, it } from "vitest";

import type {
  AppliedSwitch,
  CoachAction,
  CoachSwitch,
  CheckFacts,
  CheckKind,
  CheckOption,
  CheckPreview,
  CoachCheck,
  DiffKind,
  DiffLine,
  HandoffError,
  HandoffPreview,
  Effort,
  JsonChange,
  LineMatch,
  ModelCount,
  ModelVerdict,
  SettingsScope,
  SwitchFormat,
  SwitchKind,
  SwitchPreview,
  SwitchUsage,
  ProjectUsage,
  NameList,
  PluginContents,
  SwitchScope,
  ProjectScope,
  ScopeFile,
  ScopeMove,
  ProjectRef,
  ScopeMovePreview,
  ScopeFileChange,
  LoggedFile,
} from "@/lib/ipc";
import { isHandoffError, SCOPE_TARGET_PREFIX } from "@/lib/ipc";

import { AGENT, expectFields, expectValues, fixture, LOADING } from "@/test/ipc-fixture";
import type { Fields, Values } from "@/test/ipc-fixture";

const SWITCH_KIND = { plugin: true, mcp: true } satisfies Values<SwitchKind>;
const SWITCH_FORMAT = { toml: true, json: true, manual: true } satisfies Values<SwitchFormat>;
const COACH_ACTION = { apply: true, undo: true } satisfies Values<CoachAction>;
const COACH_SWITCH = {
  target: "required",
  agent: "required",
  kind: "required",
  name: "required",
  enabled: "required",
  tokens: "required",
  loading: "required",
  isProposed: "required",
  usage: "required",
  scope: "required",
  scopeMove: "required",
  contents: "required",
  format: "required",
  file: "required",
  instruction: "required",
} satisfies Fields<CoachSwitch>;
const SWITCH_USAGE = {
  days: "required",
  total: "required",
  projects: "required",
  projectCount: "required",
  isPartial: "required",
  sessions: "required",
  minSessions: "required",
  scopeMinUses: "required",
  lastUsed: "required",
  lastProject: "required",
} satisfies Fields<SwitchUsage>;
const NAME_LIST = { names: "required", count: "required" } satisfies Fields<NameList>;
const PLUGIN_CONTENTS = {
  skills: "required",
  agents: "required",
  commands: "required",
  mcpServers: "required",
} satisfies Fields<PluginContents>;
const PROJECT_USAGE = {
  name: "required",
  path: "required",
  count: "required",
  exists: "required",
} satisfies Fields<ProjectUsage>;
const SWITCH_SCOPE = { user: "required", projects: "required" } satisfies Fields<SwitchScope>;
const PROJECT_SCOPE = {
  name: "required",
  path: "required",
  file: "required",
  enabled: "required",
} satisfies Fields<ProjectScope>;
const SCOPE_FILE = { project: true, local: true } satisfies Values<ScopeFile>;
const SCOPE_MOVE = { projects: "required", skipped: "required" } satisfies Fields<ScopeMove>;
const PROJECT_REF = { name: "required", path: "required" } satisfies Fields<ProjectRef>;
const SCOPE_MOVE_PREVIEW = {
  target: "required",
  changes: "required",
  skipped: "required",
  beforeHash: "required",
} satisfies Fields<ScopeMovePreview>;
const SCOPE_FILE_CHANGE = {
  file: "required",
  location: "required",
  before: "required",
  after: "required",
  isNewFile: "required",
} satisfies Fields<ScopeFileChange>;
const LOGGED_FILE = {
  file: "required",
  backup: "required",
  afterHash: "required",
  createdDir: "required",
} satisfies Fields<LoggedFile>;
const SWITCH_PREVIEW = {
  target: "required",
  enabled: "required",
  file: "required",
  location: "required",
  before: "required",
  after: "required",
  beforeHash: "required",
  changed: "required",
} satisfies Fields<SwitchPreview>;
const APPLIED_SWITCH = {
  id: "required",
  at: "required",
  action: "required",
  target: "required",
  agent: "required",
  name: "required",
  file: "required",
  backup: "required",
  enabled: "required",
  tokens: "required",
  afterHash: "required",
  undoCommand: "required",
  undoOf: "required",
  created: "optional",
  files: "optional",
} satisfies Fields<AppliedSwitch>;
const CHECK_KIND = { A: true, D: true, C: true, B: true } satisfies Values<CheckKind>;
const EFFORT = {
  low: true,
  medium: true,
  high: true,
  xhigh: true,
  max: true,
} satisfies Values<Effort>;
const SETTINGS_SCOPE = { user: true, project: true, local: true } satisfies Values<SettingsScope>;
const COACH_CHECK = {
  id: "required",
  kind: "required",
  agent: "required",
  facts: "required",
  options: "required",
  file: "required",
  docs: "required",
  tokens: "required",
  isDismissed: "required",
} satisfies Fields<CoachCheck>;
const CHECK_OPTION = {
  id: "required",
  isRecommended: "required",
  isWritable: "required",
  instruction: "required",
  changes: "required",
} satisfies Fields<CheckOption>;
const JSON_CHANGE = {
  location: "required",
  before: "required",
  after: "required",
} satisfies Fields<JsonChange>;
const CHECK_PREVIEW = {
  id: "required",
  option: "required",
  file: "required",
  changes: "required",
  beforeHash: "required",
} satisfies Fields<CheckPreview>;
const MODEL_VERDICT = {
  model: "required",
  requests: "required",
  ownEffort: "required",
  defaultEffort: "required",
  isIgnored: "required",
} satisfies Fields<ModelVerdict>;
const MODEL_COUNT = { model: "required", requests: "required" } satisfies Fields<ModelCount>;
const LINE_MATCH = {
  file: "required",
  line: "required",
  text: "required",
} satisfies Fields<LineMatch>;
type FactsOf<T extends CheckFacts["type"]> = Extract<CheckFacts, { type: T }>;
const THINKING_FACTS = {
  type: "required",
  days: "required",
  models: "required",
  ignoredBy: "required",
  affected: "required",
  unknownModels: "required",
} satisfies Fields<FactsOf<"alwaysThinking">>;
const CHECK_FACTS: { [T in CheckFacts["type"]]: Fields<FactsOf<T>> } = {
  effortLevel: {
    type: "required",
    value: "required",
    days: "required",
    models: "required",
    ignoredBy: "required",
    movedTo: "required",
    current: "required",
    unknownModels: "required",
  },
  alwaysThinking: THINKING_FACTS,
  maxThinkingTokens: { ...THINKING_FACTS, value: "required" },
  gitInstructions: {
    type: "required",
    matches: "required",
    hasOwnTrailer: "required",
    preCommitSkills: "required",
  },
  agentsMdHook: {
    type: "required",
    settingsFile: "required",
    scope: "required",
    command: "required",
    agentsMd: "required",
    workspace: "required",
  },
  sameSkill: {
    type: "required",
    name: "required",
    personalPath: "required",
    pluginId: "required",
    pluginPath: "required",
    isSameContent: "required",
  },
  longInstructions: {
    type: "required",
    name: "required",
    scope: "required",
    folder: "required",
    lines: "required",
    limit: "required",
    imports: "required",
  },
  rulesPaths: {
    type: "required",
    scope: "required",
    folder: "required",
    chars: "required",
    limit: "required",
    rules: "required",
  },
};
const HANDOFF_PREVIEW = {
  id: "required",
  file: "required",
  beforeHash: "required",
  beforeLines: "required",
  afterLines: "required",
  diff: "required",
  removed: "required",
  missingImports: "required",
  canApply: "required",
} satisfies Fields<HandoffPreview>;
const DIFF_LINE = { kind: "required", text: "required" } satisfies Fields<DiffLine>;
const DIFF_KIND = { same: true, removed: true, added: true } satisfies Values<DiffKind>;
type ErrorOf<K extends HandoffError["kind"]> = Extract<HandoffError, { kind: K }>;
const HANDOFF_ERROR: { [K in HandoffError["kind"]]: Fields<ErrorOf<K>> } = {
  busy: { kind: "required" },
  limitUnknown: { kind: "required" },
  limitTooLow: { kind: "required", remaining: "required", minRemaining: "required" },
  cliMissing: { kind: "required" },
  opusUnavailable: { kind: "required" },
  timeout: { kind: "required", secs: "required" },
  failed: { kind: "required", reason: "required" },
  refused: { kind: "required", message: "required" },
  internal: { kind: "required" },
};

describe("Rust IPC coach fixtures match ipc.ts types", () => {
  it("coach switches, preview and log have the keys and enum values of the coach types", () => {
    const switches = fixture("coach-switches") as CoachSwitch[];
    switches.forEach((s) => expectFields<CoachSwitch>(s, COACH_SWITCH, "CoachSwitch"));
    expectValues(
      switches.map((s) => s.kind),
      SWITCH_KIND,
      "SwitchKind",
    );
    expectValues(
      switches.map((s) => s.format),
      SWITCH_FORMAT,
      "SwitchFormat",
    );
    switches.forEach((s) => expect(s.agent in AGENT, `agent ${s.agent}`).toBe(true));
    switches.forEach((s) => s.loading && expect(s.loading in LOADING).toBe(true));
    expectFields<SwitchPreview>(fixture("switch-preview"), SWITCH_PREVIEW, "SwitchPreview");
    const usages = switches.flatMap((s) => (s.usage ? [s.usage] : []));
    const scopes = switches.flatMap((s) => (s.scope ? [s.scope] : []));
    const moves = switches.flatMap((s) => (s.scopeMove ? [s.scopeMove] : []));
    const contents = switches.flatMap((s) => (s.contents ? [s.contents] : []));
    expect([usages.length, scopes.length, moves.length, contents.length]).toEqual([1, 1, 1, 1]);
    contents.forEach((c) => {
      expectFields<PluginContents>(c, PLUGIN_CONTENTS, "PluginContents");
      [c.skills, c.agents, c.commands, c.mcpServers].forEach((l) =>
        expectFields<NameList>(l, NAME_LIST, "NameList"),
      );
    });
    usages.forEach((u) => {
      expectFields<SwitchUsage>(u, SWITCH_USAGE, "SwitchUsage");
      u.projects.forEach((p) => expectFields<ProjectUsage>(p, PROJECT_USAGE, "ProjectUsage"));
    });
    scopes.forEach((s) => {
      expectFields<SwitchScope>(s, SWITCH_SCOPE, "SwitchScope");
      s.projects.forEach((p) => expectFields<ProjectScope>(p, PROJECT_SCOPE, "ProjectScope"));
      expectValues(
        s.projects.map((p) => p.file),
        SCOPE_FILE,
        "ScopeFile",
      );
    });
    moves.forEach((m) => {
      expectFields<ScopeMove>(m, SCOPE_MOVE, "ScopeMove");
      m.projects.forEach((p) => expectFields<ProjectRef>(p, PROJECT_REF, "ProjectRef"));
    });
    const movePreview = fixture("scope-move-preview") as ScopeMovePreview;
    expectFields<ScopeMovePreview>(movePreview, SCOPE_MOVE_PREVIEW, "ScopeMovePreview");
    movePreview.changes.forEach((c) =>
      expectFields<ScopeFileChange>(c, SCOPE_FILE_CHANGE, "ScopeFileChange"),
    );
    const log = fixture("coach-log") as AppliedSwitch[];
    log.forEach((e) => expectFields<AppliedSwitch>(e, APPLIED_SWITCH, "AppliedSwitch"));
    log.forEach((e) =>
      e.files?.forEach((f) => expectFields<LoggedFile>(f, LOGGED_FILE, "LoggedFile")),
    );
    expect(log.some((e) => e.target.startsWith(SCOPE_TARGET_PREFIX))).toBe(true);
    expectValues(
      log.map((e) => e.action),
      COACH_ACTION,
      "CoachAction",
    );
  });

  it("coach checks and check preview have the keys and enum values of the check types", () => {
    const checks = fixture("coach-checks") as CoachCheck[];
    checks.forEach((c) => {
      expectFields<CoachCheck>(c, COACH_CHECK, "CoachCheck");
      const fields = CHECK_FACTS[c.facts.type] as Fields<CheckFacts>;
      expectFields<CheckFacts>(c.facts, fields, `CheckFacts.${c.facts.type}`);
      c.options.forEach((o) => expectFields<CheckOption>(o, CHECK_OPTION, "CheckOption"));
      c.options
        .flatMap((o) => o.changes)
        .forEach((ch) => expectFields<JsonChange>(ch, JSON_CHANGE, "JsonChange"));
    });
    expectValues(
      checks.map((c) => c.kind),
      CHECK_KIND,
      "CheckKind",
    );
    expectValues(
      checks.map((c) => c.facts.type),
      Object.fromEntries(Object.keys(CHECK_FACTS).map((k) => [k, true])) as Values<
        CheckFacts["type"]
      >,
      "CheckFacts.type",
    );
    const facts = checks.map((c) => c.facts);
    const models = facts.flatMap((f) => ("models" in f ? f.models : []));
    models.forEach((m) => expectFields<ModelVerdict>(m, MODEL_VERDICT, "ModelVerdict"));
    facts
      .flatMap((f) => ("unknownModels" in f ? f.unknownModels : []))
      .forEach((m) => expectFields<ModelCount>(m, MODEL_COUNT, "ModelCount"));
    facts
      .flatMap((f) => (f.type === "gitInstructions" ? f.matches : []))
      .forEach((m) => expectFields<LineMatch>(m, LINE_MATCH, "LineMatch"));
    // fixture가 모든 값을 담지 않는 enum은 나온 값이 타입 안에 있는지만 본다.
    const efforts = models.flatMap((m) => [m.ownEffort, m.defaultEffort]).filter(Boolean);
    efforts.forEach((e) => expect(e! in EFFORT, `effort ${e}`).toBe(true));
    facts.forEach((f) => f.type === "agentsMdHook" && expect(f.scope in SETTINGS_SCOPE).toBe(true));
    const preview = fixture("check-preview") as CheckPreview;
    expectFields<CheckPreview>(preview, CHECK_PREVIEW, "CheckPreview");
    preview.changes.forEach((ch) => expectFields<JsonChange>(ch, JSON_CHANGE, "JsonChange"));
  });

  it("claude handoff preview and errors have the keys and kinds of the handoff types", () => {
    const preview = fixture("handoff-preview") as HandoffPreview;
    expectFields<HandoffPreview>(preview, HANDOFF_PREVIEW, "HandoffPreview");
    preview.diff.forEach((l) => expectFields<DiffLine>(l, DIFF_LINE, "DiffLine"));
    expectValues(
      preview.diff.map((l) => l.kind),
      DIFF_KIND,
      "DiffKind",
    );
    const errors = fixture("handoff-errors") as HandoffError[];
    errors.forEach((e) => {
      expect(isHandoffError(e), `HandoffError.${e.kind}`).toBe(true);
      expectFields<HandoffError>(e, HANDOFF_ERROR[e.kind] as Fields<HandoffError>, e.kind);
    });
    expectValues(
      errors.map((e) => e.kind),
      Object.fromEntries(Object.keys(HANDOFF_ERROR).map((k) => [k, true])) as Values<
        HandoffError["kind"]
      >,
      "HandoffError.kind",
    );
  });
});
