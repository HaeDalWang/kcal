// Rust가 보내는 IPC 값(src/test/fixtures/ipc/*.json, src-tauri/src/ipc_fixtures.rs가 만든다)이
// ipc.ts 타입과 키·enum 값까지 같은지 런타임에 확인한다. 아래 키 목록은 `satisfies Record<keyof T, …>`로
// 타입에 묶여 있어, 타입 필드가 바뀌면 컴파일 에러, fixture가 바뀌면 테스트 실패가 난다.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import type { Lang, LanguageChoice } from "@/lib/i18n";

import type {
  AdvancedTuning,
  Agent,
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
  AgentContext,
  AgentMetrics,
  AgentComparison,
  HealthReport,
  PeriodStats,
  PrescriptionEffect,
  HourBucket,
  BarUnit,
  AgentUsage,
  AppError,
  Bounds,
  CodexRate,
  CodexSession,
  ContextItem,
  ContextKind,
  ContextSnapshot,
  ContextSource,
  KiroModelCredits,
  LimitWindow,
  Loading,
  MetricsSnapshot,
  RateSnapshot,
  RateStatus,
  Settings,
  Spread,
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
  TokenRange,
  TokenRate,
  Tuning,
  TuningFieldSpec,
  TuningKey,
  UsageBar,
  ReasonKind,
  UsageSnapshot,
  UsageSource,
  WindowId,
  WindowComposition,
  WindowPattern,
} from "@/lib/ipc";
import { isHandoffError, SCOPE_TARGET_PREFIX } from "@/lib/ipc";

type Fields<T> = Record<keyof T, "required" | "optional">;
type Values<T extends string> = Record<T, true>;

function fixture(name: string): unknown {
  // vitest는 저장소 루트에서 돈다.
  return JSON.parse(readFileSync(resolve("src/test/fixtures/ipc", `${name}.json`), "utf8"));
}

/** 키 집합이 타입과 같은가: 모르는 키 없음, 필수 키 빠짐 없음. */
function expectFields<T>(value: unknown, fields: Fields<T>, where: string): void {
  expect(typeof value === "object" && value !== null, `${where} is an object`).toBe(true);
  const keys = Object.keys(value as object);
  const required = Object.entries(fields).flatMap(([k, v]) => (v === "required" ? [k] : []));
  expect(
    keys.filter((k) => !(k in fields)),
    `${where}: keys unknown to ipc.ts`,
  ).toEqual([]);
  expect(
    required.filter((k) => !keys.includes(k)),
    `${where}: required keys missing`,
  ).toEqual([]);
}

/** enum: fixture에 나온 값이 모두 타입의 값이고, 타입의 값이 모두 fixture에 나온다. */
function expectValues<T extends string>(found: unknown[], allowed: Values<T>, where: string): void {
  expect([...new Set(found)].sort(), where).toEqual(Object.keys(allowed).sort());
}

const WINDOW_ID = { "5h": true, "7d": true, month: true } satisfies Values<WindowId>;
const AGENT = { claude: true, codex: true, kiro: true } satisfies Values<Agent>;
const USAGE_SOURCE = { api: true, log: true, cli: true } satisfies Values<UsageSource>;
const REASON_KIND = { setup: true, failure: true } satisfies Values<ReasonKind>;
const CONTEXT_KIND = {
  instructions: true,
  rules: true,
  skills: true,
  agents: true,
  hooks: true,
  mcp: true,
  mcpInstructions: true,
  plugins: true,
  system: true,
  tools: true,
} satisfies Values<ContextKind>;
const CONTEXT_SOURCE = { measured: true, estimated: true } satisfies Values<ContextSource>;
const LOADING = { startup: true, conditional: true, deferred: true } satisfies Values<Loading>;
const RATE_STATUS = {
  estimated: true,
  range_only: true,
  unreliable: true,
  insufficient: true,
  no_data: true,
} satisfies Values<RateStatus>;
const BAR_UNIT = { percent: true, credits: true } satisfies Values<BarUnit>;
const ERROR_KIND = { internal: true, invalid_input: true } satisfies Values<AppError["kind"]>;

const LIMIT_WINDOW = {
  id: "required",
  usedPercent: "required",
  resetsAt: "required",
  resetPassed: "required",
  used: "optional",
  limit: "optional",
} satisfies Fields<LimitWindow>;
const AGENT_USAGE = {
  agent: "required",
  source: "required",
  observedAt: "required",
  windows: "required",
  reason: "required",
  reasonKind: "required",
  checkedAt: "required",
} satisfies Fields<AgentUsage>;
const USAGE_SNAPSHOT = { agents: "required" } satisfies Fields<UsageSnapshot>;

const CONTEXT_ITEM = {
  kind: "required",
  name: "required",
  path: "optional",
  bytes: "required",
  tokens: "required",
  source: "required",
  loading: "required",
} satisfies Fields<ContextItem>;
const AGENT_CONTEXT = {
  agent: "required",
  items: "required",
  totalStartupTokens: "required",
  firstRequestTokens: "required",
  firstRequestChars: "required",
  model: "required",
  contextWindow: "required",
  warnings: "required",
} satisfies Fields<AgentContext>;
const CONTEXT_SNAPSHOT = {
  computedAt: "required",
  agents: "required",
} satisfies Fields<ContextSnapshot>;

const TOKEN_RATE = {
  cachedInput: "required",
  uncachedInput: "required",
  output: "required",
} satisfies Fields<TokenRate>;
const BOUNDS = { min: "required", max: "required" } satisfies Fields<Bounds>;
const TOKEN_RANGE = {
  cachedInput: "required",
  uncachedInput: "required",
  output: "required",
} satisfies Fields<TokenRange>;
const CODEX_RATE = {
  status: "required",
  r2: "required",
  windows: "required",
  excludedWindows: "required",
  perPercent: "required",
  foldRange: "required",
  pattern: "required",
  composition: "required",
  sessions: "required",
} satisfies Fields<CodexRate>;
const WINDOW_COMPOSITION = {
  windows: "required",
  requests: "required",
  misses: "required",
  inputShare: "required",
  outputShare: "required",
  usedPercent: "required",
} satisfies Fields<WindowComposition>;
const CODEX_SESSION = {
  name: "required",
  cwd: "required",
  startedAt: "required",
  requests: "required",
  avgInputTokens: "required",
  misses: "required",
  share: "required",
  isMarathon: "required",
  isLargeContext: "required",
} satisfies Fields<CodexSession>;
const SPREAD = { median: "required", p90: "required" } satisfies Fields<Spread>;
const WINDOW_PATTERN = {
  days: "required",
  windows: "required",
  requests: "required",
  misses: "required",
} satisfies Fields<WindowPattern>;
const KIRO_MODEL_CREDITS = {
  model: "required",
  turns: "required",
  creditsPerTurn: "required",
} satisfies Fields<KiroModelCredits>;
const RATE_SNAPSHOT = {
  computedAt: "required",
  codex: "required",
  kiro: "required",
  claude: "required",
} satisfies Fields<RateSnapshot>;

const USAGE_BAR = {
  startsAt: "required",
  value: "required",
  isCurrent: "required",
} satisfies Fields<UsageBar>;
const AGENT_METRICS = {
  agent: "required",
  recentRequests: "required",
  cacheHitRatio: "required",
  barUnit: "required",
  bars: "required",
  hourly: "required",
} satisfies Fields<AgentMetrics>;
const HOUR_BUCKET = {
  startsAt: "required",
  requests: "required",
  used: "required",
} satisfies Fields<HourBucket>;
const METRICS_SNAPSHOT = {
  computedAt: "required",
  agents: "required",
} satisfies Fields<MetricsSnapshot>;

const PERIOD_STATS = {
  from: "required",
  to: "required",
  elapsedDays: "required",
  coverage: "required",
  requests: "required",
  avgInputTokens: "required",
  avgCredits: "required",
  dailyUsed: "required",
  usedUnit: "required",
  compactions: "required",
} satisfies Fields<PeriodStats>;
const AGENT_COMPARISON = {
  agent: "required",
  before: "required",
  after: "required",
} satisfies Fields<AgentComparison>;
const PRESCRIPTION_EFFECT = {
  id: "required",
  at: "required",
  agent: "required",
  name: "required",
  target: "required",
  enabled: "required",
  undoneAt: "required",
  before: "required",
  after: "required",
} satisfies Fields<PrescriptionEffect>;
const HEALTH_REPORT = {
  computedAt: "required",
  compareDays: "required",
  weekly: "required",
  prescriptions: "required",
} satisfies Fields<HealthReport>;

const ADVANCED_TUNING = {
  goodR2: "required",
  rangeR2: "required",
  resetTolSecs: "required",
  holdoutTrain: "required",
  minSamplesPerVar: "required",
  cacheMissRatio: "required",
} satisfies Fields<AdvancedTuning>;
const TUNING = {
  apiIntervalSecs: "required",
  kiroIntervalSecs: "required",
  refreshMinGapSecs: "required",
  maxBackoffSecs: "required",
  ratesTtlSecs: "required",
  contextTtlSecs: "required",
  metricsTtlSecs: "required",
  httpTimeoutSecs: "required",
  keychainTimeoutSecs: "required",
  kiroTimeoutSecs: "required",
  codexSessionsToSearch: "required",
  kiroSessionsToSearch: "required",
  shapeSessionsToSearch: "required",
  codexRolloutsChecked: "required",
  claudeLogTailBytes: "required",
  bytesPerToken: "required",
  breakdownDays: "required",
  lowPercent: "required",
  recentBars: "required",
  barDays: "required",
  claudeLogFiles: "required",
  topSessions: "required",
  marathonRequests: "required",
  largeContextTokens: "required",
  dailyGoalPercent: "required",
  kiroDailyCredits: "required",
  compareDays: "required",
  checkModelDays: "required",
  checkAlertMin: "required",
  claudeHandoffTimeoutSecs: "required",
  claudeHandoffMinRemaining: "required",
  diagnoseUsageDays: "required",
  diagnoseUsageFiles: "required",
  diagnoseUsageMaxMib: "required",
  diagnoseMinSessions: "required",
  scopeMinUses: "required",
  advanced: "required",
} satisfies Fields<Tuning>;
const TUNING_FIELD_SPEC = {
  key: "required",
  min: "required",
  max: "required",
  default: "required",
  isInteger: "required",
} satisfies Fields<TuningFieldSpec>;
const TUNING_KEY = Object.fromEntries([
  ...Object.keys(TUNING)
    .filter((k) => k !== "advanced")
    .map((k) => [k, true]),
  ...Object.keys(ADVANCED_TUNING).map((k) => [`advanced.${k}`, true]),
]) as Values<TuningKey>;
const SETTINGS = {
  enabledAgents: "required",
  tuning: "required",
  tuningSpec: "required",
  reason: "required",
  language: "required",
  resolvedLanguage: "required",
} satisfies Fields<Settings>;
const LANGUAGE_CHOICE = { auto: true, ko: true, en: true } satisfies Values<LanguageChoice>;
const LANG = { ko: true, en: true } satisfies Values<Lang>;
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
const APP_ERROR = { kind: "required", message: "required" } satisfies Fields<AppError>;
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

const list = (value: unknown) => value as Record<string, unknown>[];

describe("Rust IPC fixtures match ipc.ts types", () => {
  it("usage snapshot has the keys and enum values of UsageSnapshot", () => {
    const snapshot = fixture("usage-snapshot") as UsageSnapshot;
    expectFields<UsageSnapshot>(snapshot, USAGE_SNAPSHOT, "UsageSnapshot");
    const windows = snapshot.agents.flatMap((a) => a.windows);
    snapshot.agents.forEach((a) => expectFields<AgentUsage>(a, AGENT_USAGE, "AgentUsage"));
    windows.forEach((w) => expectFields<LimitWindow>(w, LIMIT_WINDOW, "LimitWindow"));
    expectValues(
      snapshot.agents.map((a) => a.agent),
      AGENT,
      "Agent",
    );
    expectValues(
      snapshot.agents.flatMap((a) => (a.source ? [a.source] : [])),
      USAGE_SOURCE,
      "UsageSource",
    );
    expectValues(
      snapshot.agents.flatMap((a) => (a.reasonKind ? [a.reasonKind] : [])),
      REASON_KIND,
      "ReasonKind",
    );
    expectValues(
      windows.map((w) => w.id),
      WINDOW_ID,
      "WindowId",
    );
  });

  it("context snapshot has the keys and enum values of ContextSnapshot", () => {
    const snapshot = fixture("context-snapshot") as ContextSnapshot;
    expectFields<ContextSnapshot>(snapshot, CONTEXT_SNAPSHOT, "ContextSnapshot");
    const items = snapshot.agents.flatMap((a) => a.items);
    snapshot.agents.forEach((a) => expectFields<AgentContext>(a, AGENT_CONTEXT, "AgentContext"));
    items.forEach((i) => expectFields<ContextItem>(i, CONTEXT_ITEM, "ContextItem"));
    expectValues(
      snapshot.agents.map((a) => a.agent),
      AGENT,
      "Agent",
    );
    expectValues(
      items.map((i) => i.kind),
      CONTEXT_KIND,
      "ContextKind",
    );
    expectValues(
      items.map((i) => i.source),
      CONTEXT_SOURCE,
      "ContextSource",
    );
    expectValues(
      items.map((i) => i.loading),
      LOADING,
      "Loading",
    );
  });

  it("rate snapshot has the keys and status values of RateSnapshot", () => {
    const snapshot = fixture("rate-snapshot") as RateSnapshot;
    expectFields<RateSnapshot>(snapshot, RATE_SNAPSHOT, "RateSnapshot");
    expectFields<CodexRate>(snapshot.codex, CODEX_RATE, "CodexRate");
    expectFields<TokenRate>(snapshot.codex.perPercent, TOKEN_RATE, "TokenRate");
    const range = snapshot.codex.foldRange;
    expectFields<TokenRange>(range, TOKEN_RANGE, "TokenRange");
    [range?.cachedInput, range?.uncachedInput, range?.output].forEach((b) =>
      expectFields<Bounds>(b, BOUNDS, "Bounds"),
    );
    const pattern = snapshot.codex.pattern;
    expectFields<WindowPattern>(pattern, WINDOW_PATTERN, "WindowPattern");
    [pattern?.requests, pattern?.misses].forEach((s) => expectFields<Spread>(s, SPREAD, "Spread"));
    expectFields<WindowComposition>(
      snapshot.codex.composition,
      WINDOW_COMPOSITION,
      "WindowComposition",
    );
    expect(snapshot.codex.sessions.length, "fixture has a session").toBeGreaterThan(0);
    list(snapshot.codex.sessions).forEach((s) =>
      expectFields<CodexSession>(s, CODEX_SESSION, "CodexSession"),
    );
    list(snapshot.kiro).forEach((k) =>
      expectFields<KiroModelCredits>(k, KIRO_MODEL_CREDITS, "KiroModelCredits"),
    );
    expectFields<CodexRate>(snapshot.claude, CODEX_RATE, "ClaudeRate");
    expectValues(fixture("rate-statuses") as RateStatus[], RATE_STATUS, "RateStatus");
  });

  it("metrics snapshot has the keys and unit values of MetricsSnapshot", () => {
    const snapshot = fixture("metrics-snapshot") as MetricsSnapshot;
    expectFields<MetricsSnapshot>(snapshot, METRICS_SNAPSHOT, "MetricsSnapshot");
    snapshot.agents.forEach((a) => expectFields<AgentMetrics>(a, AGENT_METRICS, "AgentMetrics"));
    snapshot.agents
      .flatMap((a) => a.bars)
      .forEach((b) => expectFields<UsageBar>(b, USAGE_BAR, "UsageBar"));
    const hours = snapshot.agents.flatMap((a) => a.hourly);
    expect(hours.length, "fixture has hour buckets").toBeGreaterThan(0);
    hours.forEach((h) => expectFields<HourBucket>(h, HOUR_BUCKET, "HourBucket"));
    expectValues(
      snapshot.agents.map((a) => a.agent),
      AGENT,
      "Agent",
    );
    expectValues(
      snapshot.agents.map((a) => a.barUnit),
      BAR_UNIT,
      "BarUnit",
    );
  });

  it("health report has the keys and unit values of HealthReport", () => {
    const report = fixture("health-report") as HealthReport;
    expectFields<HealthReport>(report, HEALTH_REPORT, "HealthReport");
    report.weekly.forEach((c) =>
      expectFields<AgentComparison>(c, AGENT_COMPARISON, "AgentComparison"),
    );
    expect(report.prescriptions.length, "fixture has prescriptions").toBeGreaterThan(0);
    report.prescriptions.forEach((p) =>
      expectFields<PrescriptionEffect>(p, PRESCRIPTION_EFFECT, "PrescriptionEffect"),
    );
    const periods = [...report.weekly, ...report.prescriptions].flatMap((c) => [c.before, c.after]);
    periods.forEach((p) => expectFields<PeriodStats>(p, PERIOD_STATS, "PeriodStats"));
    expectValues(
      report.weekly.map((c) => c.agent),
      AGENT,
      "Agent",
    );
    expectValues(
      periods.map((p) => p.usedUnit),
      BAR_UNIT,
      "BarUnit",
    );
  });

  it("settings has the keys of Settings, Tuning and AdvancedTuning", () => {
    const settings = fixture("settings") as Settings;
    expectFields<Settings>(settings, SETTINGS, "Settings");
    expectFields<Tuning>(settings.tuning, TUNING, "Tuning");
    expectFields<AdvancedTuning>(settings.tuning.advanced, ADVANCED_TUNING, "AdvancedTuning");
    settings.enabledAgents.forEach((a) => expect(a in AGENT, `enabled agent ${a}`).toBe(true));
    expect(settings.language in LANGUAGE_CHOICE, "LanguageChoice").toBe(true);
    expect(settings.resolvedLanguage in LANG, "Lang").toBe(true);
  });

  it("settings tuningSpec has one entry per tuning field", () => {
    const { tuningSpec } = fixture("settings") as Settings;
    tuningSpec.forEach((f) =>
      expectFields<TuningFieldSpec>(f, TUNING_FIELD_SPEC, "TuningFieldSpec"),
    );
    expect(tuningSpec.map((f) => f.key).sort()).toEqual(Object.keys(TUNING_KEY).sort());
  });

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

  it("app errors have the keys and kinds of AppError", () => {
    const errors = fixture("app-errors") as AppError[];
    errors.forEach((e) => expectFields<AppError>(e, APP_ERROR, "AppError"));
    expectValues(
      errors.map((e) => e.kind),
      ERROR_KIND,
      "AppError.kind",
    );
  });
});
