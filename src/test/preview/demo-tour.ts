// Extra demo data and commands for the full-tour scene (`demo.html?scene=tour`): a third agent (Kiro),
// running Claude sessions, the "why the limit burned" rows, and a Checks tab with real-looking cards.
// Invented names and paths only; nothing is read from the machine. Not part of the app bundle.
import type { InvokeArgs } from "@tauri-apps/api/core";
import type {
  AgentContext,
  AgentMetrics,
  AgentUsage,
  ClaudeSession,
  ClaudeSessions,
  ContextItem,
  HealthReport,
  MetricsSnapshot,
  PeriodStats,
  RateSnapshot,
  Settings,
  UsageBar,
  UsageSnapshot,
} from "@/lib/ipc";
import { DASHBOARD_CHECKS } from "@/test/preview/checks-fixtures";
import { demoFixtures } from "@/test/preview/demo-fixtures";

type Fixtures = ReturnType<typeof demoFixtures>;

const MINUTE_MS = 60_000;
const HOUR_MS = 3_600_000;
const DAY_MS = 24 * HOUR_MS;
const iso = (ms: number) => new Date(ms).toISOString();

const kiroItem = (
  kind: ContextItem["kind"],
  name: string,
  tokens: number,
  path?: string,
): ContextItem => ({
  kind,
  name,
  ...(path && { path }),
  bytes: tokens * 4,
  tokens,
  source: "estimated",
  loading: "startup",
});

const KIRO_ITEMS: ContextItem[] = [
  kiroItem("instructions", "AGENTS.md", 1500, "~/code/web-app/AGENTS.md"),
  kiroItem("rules", "steering-style.md", 900, "~/code/web-app/.kiro/steering"),
  kiroItem("rules", "steering-tests.md", 700, "~/code/web-app/.kiro/steering"),
  kiroItem("skills", "user-skills (4 skills)", 600, "~/.kiro/skills"),
  kiroItem("mcp", "aws-docs", 1120),
];

function kiroUsage(now: number): AgentUsage {
  const seen = iso(now - 2 * MINUTE_MS);
  return {
    agent: "kiro",
    source: "cli",
    observedAt: seen,
    checkedAt: seen,
    reason: null,
    reasonKind: null,
    windows: [
      {
        id: "month",
        usedPercent: 30,
        resetsAt: iso(now + 12 * DAY_MS),
        resetPassed: false,
        used: 1500,
        limit: 5000,
      },
    ],
  };
}

function kiroMetrics(now: number): AgentMetrics {
  const dayStart = Math.floor(now / DAY_MS) * DAY_MS;
  const values = [41, 63, 22, 8, 5, 58, 36];
  const bars: UsageBar[] = values.map((value, i) => ({
    startsAt: iso(dayStart - (7 - i) * DAY_MS),
    value,
    isCurrent: false,
  }));
  bars.push({ startsAt: iso(dayStart), value: 27, isCurrent: true });
  const end = Math.floor(now / HOUR_MS) * HOUR_MS;
  const hourly = Array.from({ length: 8 * 24 }, (_, i) => {
    const start = end - (8 * 24 - 1 - i) * HOUR_MS;
    const date = new Date(start);
    const hour = date.getHours();
    const weekend = date.getDay() === 0 || date.getDay() === 6;
    const weight = hour >= 9 && hour <= 22 ? Math.max(0, 1 - Math.abs(hour - 16) / 7) : 0;
    const jitter = 0.6 + ((date.getDate() * 7 + hour * 3) % 10) / 10;
    const requests = Math.round(weight * 6 * jitter * (weekend ? 0.25 : 1));
    return { startsAt: iso(start), requests, used: requests * 0.9 };
  });
  return {
    agent: "kiro",
    recentRequests: 14,
    cacheHitRatio: null,
    barUnit: "credits",
    bars,
    hourly,
  };
}

function kiroPeriod(
  now: number,
  weeksAgo: 0 | 1,
  avgCredits: number,
  dailyUsed: number,
): PeriodStats {
  return {
    from: iso(now - (weeksAgo + 1) * 7 * DAY_MS),
    to: iso(now - weeksAgo * 7 * DAY_MS),
    elapsedDays: 7,
    coverage: 1,
    requests: weeksAgo === 0 ? 96 : 104,
    avgInputTokens: null,
    avgCredits,
    dailyUsed,
    usedUnit: "credits",
    compactions: null,
    automatedInputShare: null,
    modelInputShares: null,
    codexRate: null,
  };
}

const shares = (a: string, b: string, c: string) => [
  { model: a, share: 50 },
  { model: b, share: 25 },
  { model: c, share: 15 },
  { model: null, share: 10 },
];

/** The "why the limit burned" rows (decision 0028): automated share, model shares, Codex rate. */
function burnDetails(health: HealthReport): HealthReport["weekly"] {
  return health.weekly.map((c) => {
    const isClaude = c.agent === "claude";
    return {
      ...c,
      before: {
        ...c.before,
        automatedInputShare: isClaude ? 12 : 8,
        modelInputShares: isClaude
          ? shares("claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5")
          : shares("gpt-5.5", "gpt-5.4", "gpt-5.3-codex"),
        codexRate: isClaude
          ? null
          : {
              status: "estimated" as const,
              windows: 11,
              perPercent: { cachedInput: 1_450_000, uncachedInput: 62_000, output: 20_000 },
              foldRange: null,
            },
      },
      after: {
        ...c.after,
        automatedInputShare: isClaude ? 31 : 9,
        modelInputShares: isClaude
          ? [
              { model: "claude-opus-5-5", share: 78 },
              { model: "claude-sonnet-5-5", share: 22 },
            ]
          : [
              { model: "gpt-5.5", share: 88 },
              { model: "gpt-5.4", share: 12 },
            ],
        codexRate: isClaude
          ? null
          : {
              status: "estimated" as const,
              windows: 15,
              perPercent: { cachedInput: 1_400_000, uncachedInput: 70_000, output: 22_000 },
              foldRange: null,
            },
      },
    };
  });
}

export function tourFixtures(now: number): Fixtures {
  const base = demoFixtures(now);
  const settings: Settings = { ...base.settings, enabledAgents: ["claude", "codex", "kiro"] };
  const usage: UsageSnapshot = { agents: [...base.usage.agents, kiroUsage(now)] };
  const kiroContext: AgentContext = {
    agent: "kiro",
    items: KIRO_ITEMS,
    totalStartupTokens: KIRO_ITEMS.reduce((sum, i) => sum + (i.tokens ?? 0), 0),
    firstRequestTokens: null,
    firstRequestChars: null,
    model: null,
    contextWindow: 200000,
    warnings: [],
  };
  const context = { ...base.context, agents: [...base.context.agents, kiroContext] };
  const metrics: MetricsSnapshot = {
    ...base.metrics,
    agents: [...base.metrics.agents, kiroMetrics(now)],
  };
  const health: HealthReport = {
    ...base.health,
    weekly: [
      ...burnDetails(base.health),
      { agent: "kiro", before: kiroPeriod(now, 1, 1.3, 44), after: kiroPeriod(now, 0, 1.1, 39) },
    ],
  };
  const rates: RateSnapshot = {
    ...base.rates,
    kiro: [
      { model: "claude-sonnet-5-5", turns: 64, creditsPerTurn: 1.2 },
      { model: "claude-haiku-5", turns: 28, creditsPerTurn: 0.4 },
    ],
  };
  return { ...base, settings, usage, context, metrics, health, rates };
}

const session = (
  pid: number,
  status: string,
  cwd: string,
  idleHours: number | null,
  isEligible = false,
): ClaudeSession => ({
  pid,
  sessionId: `demo-session-${pid}`,
  cwd,
  status,
  startedAt: null,
  kind: "interactive",
  idleSeconds: idleHours === null ? null : Math.round(idleHours * 3600),
  stopEligible: isEligible,
  exclusion: null,
});

const SESSIONS: ClaudeSession[] = [
  session(41820, "waiting", "/Users/me/code/api-server", 0.1),
  session(41207, "waiting", "/Users/me/code/web-app", 0.5),
  session(40116, "busy", "/Users/me/code/docs-site", null),
  session(38542, "idle", "/Users/me/code/api-server", 31, true),
  session(37990, "idle", "/Users/me/code/web-app", 27, true),
  session(36411, "idle", "/Users/me/code/infra", 52, true),
  session(35208, "idle", "/Users/me/code/mobile-app", 6),
  session(34877, "idle", "/Users/me/code/docs-site", 3),
];

/** Handler for the tour's extra commands, or `undefined` for commands it does not own. */
export function tourBackend() {
  let running = SESSIONS;
  return (cmd: string, payload: InvokeArgs | undefined): unknown => {
    switch (cmd) {
      case "list_claude_sessions":
        return { sessions: running, canStop: true, idleHours: 24 } satisfies ClaudeSessions;
      case "stop_idle_claude_sessions": {
        const stopped = ((payload as { targets?: unknown[] } | undefined)?.targets ?? []).length;
        running = running.filter((s) => !s.stopEligible);
        return { stopped, skipped: [] };
      }
      case "get_checks":
        return DASHBOARD_CHECKS;
      case "get_installed_agents":
        return ["claude", "codex", "kiro"];
      case "plugin:app|version":
        return "0.1.4";
      case "get_autostart":
        return true;
      case "get_autostart_path_warning":
        return false;
      default:
        return undefined;
    }
  };
}
