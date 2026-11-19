// Demo data for the README GIF, not measurements.
// Every value here is invented to look like a normal heavy user of Claude Code and Codex. No real
// project, path, account or model usage was read to make it. Times are relative to `now`, so the
// pace lines ("7% ahead", "resets in 3d 3h") stay consistent whenever the page is loaded.
// Used only by src/test/preview/demo.html. It is not part of the app bundle.
import type {
  AgentContext,
  AgentMetrics,
  CodexRate,
  ContextItem,
  ContextKind,
  ContextSnapshot,
  HealthReport,
  HourBucket,
  MetricsSnapshot,
  PeriodStats,
  RateSnapshot,
  Settings,
  UsageBar,
  UsageSnapshot,
} from "@/lib/ipc";
import settings from "@/test/fixtures/ipc/settings.json";

const MINUTE_MS = 60_000;
const HOUR_MS = 3_600_000;
const DAY_MS = 24 * HOUR_MS;
const iso = (ms: number) => new Date(ms).toISOString();

/** Tokens per byte is 1/4 in kcal, so the byte count is derived from the token count. */
const item = (
  kind: ContextKind,
  name: string,
  tokens: number | null,
  source: ContextItem["source"],
  loading: ContextItem["loading"] = "startup",
  path?: string,
): ContextItem => ({
  kind,
  name,
  ...(path && { path }),
  bytes: tokens === null ? null : tokens * 4,
  tokens,
  source,
  loading,
});

function demoSettings(): Settings {
  return {
    ...(settings as Settings),
    enabledAgents: ["claude", "codex"],
    language: "en",
    resolvedLanguage: "en",
    reason: null,
  };
}

function demoUsage(now: number): UsageSnapshot {
  const seen = iso(now - 2 * MINUTE_MS);
  return {
    agents: [
      {
        agent: "claude",
        source: "api",
        observedAt: seen,
        checkedAt: seen,
        reason: null,
        reasonKind: null,
        windows: [
          // 5-hour window: 2h 20m in, 40% used, a little ahead of an even pace.
          {
            id: "5h",
            usedPercent: 40,
            resetsAt: iso(now + 2 * HOUR_MS + 40 * MINUTE_MS),
            resetPassed: false,
          },
          // Weekly window: 4d 21h in, 58% used, a little short of an even pace.
          {
            id: "7d",
            usedPercent: 58,
            resetsAt: iso(now + 3 * DAY_MS + 3 * HOUR_MS),
            resetPassed: false,
          },
        ],
      },
      {
        agent: "codex",
        source: "api",
        observedAt: seen,
        checkedAt: seen,
        reason: null,
        reasonKind: null,
        windows: [
          {
            id: "5h",
            usedPercent: 21,
            resetsAt: iso(now + 3 * HOUR_MS + 40 * MINUTE_MS),
            resetPassed: false,
          },
          {
            id: "7d",
            usedPercent: 33,
            resetsAt: iso(now + 4 * DAY_MS + 9 * HOUR_MS),
            resetPassed: false,
          },
        ],
      },
    ],
  };
}

function demoContext(now: number): ContextSnapshot {
  const claudeItems: ContextItem[] = [
    item("system", "System prompt", 7900, "measured"),
    item("tools", "System tools", 11600, "measured"),
    item("mcp", "github", 4200, "measured"),
    item("mcp", "postgres", 2600, "measured"),
    item("mcpInstructions", "github", 420, "measured"),
    item("instructions", "CLAUDE.md", 2300, "estimated", "startup", "~/code/api-server/CLAUDE.md"),
    item("instructions", "CLAUDE.md", 1600, "estimated", "startup", "~/.claude/CLAUDE.md"),
    item("skills", "user-skills (6 skills)", 1900, "estimated", "startup", "~/.claude/skills"),
    item("rules", "testing.md", 300, "estimated", "startup", "~/.claude/rules/testing.md"),
    item("rules", "style.md", 225, "estimated", "startup", "~/.claude/rules/style.md"),
    item("agents", "code-reviewer.md", 310, "estimated", "startup", "~/.claude/agents"),
    item("agents", "test-runner.md", 240, "estimated", "startup", "~/.claude/agents"),
    item("rules", "migrations.md", 180, "estimated", "conditional", "~/code/api-server/.claude"),
    item("hooks", "2 hooks", null, "estimated", "conditional"),
  ];
  const claudeTotal = claudeItems.reduce(
    (sum, i) => sum + (i.loading === "startup" ? (i.tokens ?? 0) : 0),
    0,
  );
  const codexItems: ContextItem[] = [
    item("system", "System prompt", 5400, "measured"),
    item("instructions", "AGENTS.md", 1500, "estimated", "startup", "~/code/api-server/AGENTS.md"),
    item("instructions", "AGENTS.md", 900, "estimated", "startup", "~/.codex/AGENTS.md"),
    item("skills", "user-skills (3 skills)", 600, "estimated", "startup", "~/.codex/skills"),
    item("mcp", "github", null, "estimated", "deferred"),
  ];
  const codexTotal = codexItems.reduce(
    (sum, i) => sum + (i.loading === "startup" ? (i.tokens ?? 0) : 0),
    0,
  );
  const claude: AgentContext = {
    agent: "claude",
    items: claudeItems,
    totalStartupTokens: claudeTotal,
    firstRequestTokens: null,
    firstRequestChars: null,
    model: "claude-opus-5-5",
    contextWindow: 200000,
    warnings: [
      "Measured values come from the latest Claude session (folder api-server).",
      "2 hooks: output size is decided when they run, so it isn't counted.",
    ],
  };
  const codex: AgentContext = {
    agent: "codex",
    items: codexItems,
    totalStartupTokens: codexTotal,
    firstRequestTokens: 24300,
    firstRequestChars: null,
    model: "gpt-5.5",
    contextWindow: 272000,
    warnings: ["Measured values come from the latest Codex session (folder web-app)."],
  };
  return { computedAt: iso(now), agents: [claude, codex] };
}

const FOLD = (min: number, max: number) => ({ min, max });

function demoRates(now: number): RateSnapshot {
  const claude: CodexRate = {
    status: "estimated",
    r2: 0.86,
    windows: 24,
    excludedWindows: 1,
    perPercent: { cachedInput: 1_100_000, uncachedInput: 55_000, output: 17_000 },
    foldRange: {
      cachedInput: FOLD(900_000, 1_300_000),
      uncachedInput: FOLD(40_000, 70_000),
      output: FOLD(12_000, 24_000),
    },
    pattern: {
      days: 14,
      windows: 18,
      requests: { median: 112, p90: 186 },
      misses: { median: 3, p90: 7 },
    },
    composition: null,
    sessions: [],
  };
  const codex: CodexRate = {
    status: "estimated",
    r2: 0.82,
    windows: 15,
    excludedWindows: 0,
    perPercent: { cachedInput: 1_400_000, uncachedInput: 70_000, output: 22_000 },
    foldRange: {
      cachedInput: FOLD(1_100_000, 1_700_000),
      uncachedInput: FOLD(52_000, 90_000),
      output: FOLD(15_000, 30_000),
    },
    pattern: {
      days: 14,
      windows: 15,
      requests: { median: 74, p90: 132 },
      misses: { median: 2, p90: 5 },
    },
    composition: {
      windows: 15,
      requests: 74,
      misses: 2,
      inputShare: 14.2,
      outputShare: 2.1,
      usedPercent: 17,
    },
    sessions: [
      {
        name: "api-server",
        cwd: "~/code/api-server",
        startedAt: iso(now - 2 * DAY_MS),
        requests: 210,
        avgInputTokens: 78_000,
        misses: 6,
        share: 38.5,
        isMarathon: true,
        isLargeContext: false,
      },
      {
        name: "web-app",
        cwd: "~/code/web-app",
        startedAt: iso(now - 5 * HOUR_MS),
        requests: 140,
        avgInputTokens: 54_000,
        misses: 4,
        share: 21,
        isMarathon: false,
        isLargeContext: false,
      },
      {
        name: "docs-site",
        cwd: "~/code/docs-site",
        startedAt: iso(now - 4 * DAY_MS),
        requests: 64,
        avgInputTokens: 31_000,
        misses: 2,
        share: 7.2,
        isMarathon: false,
        isLargeContext: false,
      },
    ],
  };
  return { computedAt: iso(now), claude, codex, kiro: [] };
}

/** Hours back from the current window's start, for the earlier 5-hour windows (oldest last). */
const PAST_WINDOW_STARTS_H = [5, 10, 29, 34, 53, 58, 77];

function bars(now: number, resetsIn: number, past: number[], current: number): UsageBar[] {
  const currentStart = now + resetsIn - 5 * HOUR_MS;
  const earlier = past.map((value, i) => ({
    startsAt: iso(currentStart - PAST_WINDOW_STARTS_H[i] * HOUR_MS),
    value,
    isCurrent: false,
  }));
  return [...earlier.reverse(), { startsAt: iso(currentStart), value: current, isCurrent: true }];
}

/** Requests per hour over the last 8 days: working hours, quieter on weekends. Deterministic. */
function hourly(now: number, peak: number, scale: number, perRequest: number): HourBucket[] {
  const end = Math.floor(now / HOUR_MS) * HOUR_MS;
  return Array.from({ length: 8 * 24 }, (_, i) => {
    const start = end - (8 * 24 - 1 - i) * HOUR_MS;
    const date = new Date(start);
    const hour = date.getHours();
    const weekend = date.getDay() === 0 || date.getDay() === 6;
    const weight = hour >= 9 && hour <= 22 ? Math.max(0, 1 - Math.abs(hour - peak) / 7) : 0;
    const jitter = 0.6 + ((date.getDate() * 7 + hour * 3) % 10) / 10;
    const requests = Math.round(weight * scale * jitter * (weekend ? 0.25 : 1));
    return { startsAt: iso(start), requests, used: requests * perRequest };
  });
}

function demoMetrics(now: number): MetricsSnapshot {
  const claude: AgentMetrics = {
    agent: "claude",
    recentRequests: 94,
    cacheHitRatio: 0.96,
    barUnit: "percent",
    bars: bars(now, 2 * HOUR_MS + 40 * MINUTE_MS, [34, 58, 71, 46, 83, 29, 52], 40),
    hourly: hourly(now, 14, 30, 0.045),
  };
  const codex: AgentMetrics = {
    agent: "codex",
    recentRequests: 61,
    cacheHitRatio: 0.93,
    barUnit: "percent",
    bars: bars(now, 3 * HOUR_MS + 40 * MINUTE_MS, [22, 35, 12, 41, 18, 27, 31], 21),
    hourly: hourly(now, 11, 18, 0.05),
  };
  return { computedAt: iso(now), agents: [claude, codex] };
}

function period(
  now: number,
  weeksAgo: 0 | 1,
  stats: Pick<PeriodStats, "requests" | "avgInputTokens" | "dailyUsed" | "compactions">,
): PeriodStats {
  return {
    from: iso(now - (weeksAgo + 1) * 7 * DAY_MS),
    to: iso(now - weeksAgo * 7 * DAY_MS),
    elapsedDays: 7,
    coverage: 1,
    avgCredits: null,
    automatedInputShare: null,
    modelInputShares: null,
    codexRate: null,
    usedUnit: "percent",
    ...stats,
  };
}

function demoHealth(now: number): HealthReport {
  const claudeBefore = {
    requests: 1310,
    avgInputTokens: 64_200,
    dailyUsed: 10.2,
    compactions: null,
  };
  const claudeAfter = { requests: 1190, avgInputTokens: 61_800, dailyUsed: 9.6, compactions: null };
  const codexBefore = { requests: 420, avgInputTokens: 71_500, dailyUsed: 3.1, compactions: 4 };
  const codexAfter = { requests: 380, avgInputTokens: 69_800, dailyUsed: 2.8, compactions: 3 };
  return {
    computedAt: iso(now),
    compareDays: 7,
    weekly: [
      { agent: "claude", before: period(now, 1, claudeBefore), after: period(now, 0, claudeAfter) },
      { agent: "codex", before: period(now, 1, codexBefore), after: period(now, 0, codexAfter) },
    ],
    prescriptions: [],
  };
}

export function demoFixtures(now: number) {
  return {
    settings: demoSettings(),
    usage: demoUsage(now),
    context: demoContext(now),
    rates: demoRates(now),
    metrics: demoMetrics(now),
    health: demoHealth(now),
  };
}
