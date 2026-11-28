// Rust가 보내는 IPC 값(src/test/fixtures/ipc/*.json, src-tauri/src/ipc_fixtures.rs가 만든다)이
// ipc.ts 타입과 키·enum 값까지 같은지 런타임에 확인한다. 아래 키 목록은 `satisfies Record<keyof T, …>`로
// 타입에 묶여 있어, 타입 필드가 바뀌면 컴파일 에러, fixture가 바뀌면 테스트 실패가 난다.
import { describe, expect, it } from "vitest";

import type { Lang, LanguageChoice } from "@/lib/i18n";

import type {
  AdvancedTuning,
  AgentContext,
  AgentMetrics,
  AgentComparison,
  HealthReport,
  PeriodStats,
  PrescriptionEffect,
  HourBucket,
  BarUnit,
  AgentUsage,
  Credits,
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
  MetricsSnapshot,
  RateSnapshot,
  RateStatus,
  Settings,
  TrayIcon,
  Spread,
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
import { AGENT, expectFields, expectValues, fixture, LOADING } from "@/test/ipc-fixture";
import type { Fields, Values } from "@/test/ipc-fixture";

const WINDOW_ID = { "5h": true, "7d": true, month: true } satisfies Values<WindowId>;
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
  group: "optional",
} satisfies Fields<LimitWindow>;
const CREDITS = {
  used: "required",
  limit: "required",
  currency: "required",
} satisfies Fields<Credits>;
const AGENT_USAGE = {
  agent: "required",
  source: "required",
  observedAt: "required",
  windows: "required",
  modelWindows: "optional",
  credits: "optional",
  resetPasses: "optional",
  loginExpiresAt: "optional",
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
  automatedInputShare: "required",
  modelInputShares: "required",
  codexRate: "required",
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
  claudeRateDays: "required",
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
  loginWarnDays: "required",
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
  diagnoseRecentFolders: "required",
  diagnoseUsageDays: "required",
  diagnoseUsageFiles: "required",
  diagnoseUsageMaxMib: "required",
  diagnoseMinSessions: "required",
  scopeMinUses: "required",
  sessionIdleHours: "required",
  sessionChildCpuPercent: "required",
  sessionProtectedFolders: "required",
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
    .filter((k) => k !== "advanced" && k !== "sessionProtectedFolders")
    .map((k) => [k, true]),
  ...Object.keys(ADVANCED_TUNING).map((k) => [`advanced.${k}`, true]),
]) as Values<TuningKey>;
const SETTINGS = {
  enabledAgents: "required",
  trayIcon: "required",
  tuning: "required",
  tuningSpec: "required",
  reason: "required",
  language: "required",
  resolvedLanguage: "required",
} satisfies Fields<Settings>;
const TRAY_ICON = { bars: true, percent: true, icon: true } satisfies Values<TrayIcon>;
const LANGUAGE_CHOICE = { auto: true, ko: true, en: true } satisfies Values<LanguageChoice>;
const LANG = { ko: true, en: true } satisfies Values<Lang>;
const APP_ERROR = { kind: "required", message: "required" } satisfies Fields<AppError>;

const list = (value: unknown) => value as Record<string, unknown>[];

describe("Rust IPC fixtures match ipc.ts types", () => {
  it("usage snapshot has the keys and enum values of UsageSnapshot", () => {
    const snapshot = fixture("usage-snapshot") as UsageSnapshot;
    expectFields<UsageSnapshot>(snapshot, USAGE_SNAPSHOT, "UsageSnapshot");
    const windows = snapshot.agents.flatMap((a) => [...a.windows, ...(a.modelWindows ?? [])]);
    snapshot.agents.forEach(
      (a) => a.credits && expectFields<Credits>(a.credits, CREDITS, "Credits"),
    );
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
    expect(settings.trayIcon in TRAY_ICON, "TrayIcon").toBe(true);
    expect(settings.language in LANGUAGE_CHOICE, "LanguageChoice").toBe(true);
    expect(settings.resolvedLanguage in LANG, "Lang").toBe(true);
  });

  it("settings tuningSpec has one entry per numeric tuning field", () => {
    const { tuningSpec } = fixture("settings") as Settings;
    tuningSpec.forEach((f) =>
      expectFields<TuningFieldSpec>(f, TUNING_FIELD_SPEC, "TuningFieldSpec"),
    );
    expect(tuningSpec.map((f) => f.key).sort()).toEqual(Object.keys(TUNING_KEY).sort());
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
