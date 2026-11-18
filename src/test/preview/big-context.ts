import type { AgentContext, ContextItem, ContextKind, ContextSnapshot } from "@/lib/ipc";

/** 미리보기 전용(`?bigcontext`): 실제 Claude·Codex·Kiro 시작 컨텍스트처럼 긴 목록. */
const it = (
  kind: ContextKind,
  name: string,
  tokens: number | null,
  source: ContextItem["source"] = "estimated",
  loading: ContextItem["loading"] = "startup",
): ContextItem => ({
  kind,
  name,
  bytes: tokens === null ? null : tokens * 4,
  tokens,
  source,
  loading,
});

const many = (
  kind: ContextKind,
  prefix: string,
  sizes: number[],
  source: ContextItem["source"] = "estimated",
) => sizes.map((n, i) => it(kind, `${prefix}${i + 1}`, n, source));

const total = (items: ContextItem[]) =>
  items.reduce((sum, i) => sum + (i.loading === "startup" ? (i.tokens ?? 0) : 0), 0);

export function bigContext(now: number): ContextSnapshot {
  const claudeItems: ContextItem[] = [
    it("tools", "내장 도구 (13개)", 17195, "measured"),
    it("skills", "스킬 목록 (스킬 38개)", 5704, "measured"),
    it("instructions", "AGENTS.md", 5049),
    it("system", "시스템 프롬프트 (블록 3개)", 3358, "measured"),
    it("mcp", "claude.ai Figma", 1043, "measured"),
    it("rules", "simplicity.md", 814),
    ...many("rules", "rule-", [640, 512, 430, 388, 301, 260, 244, 190, 155, 120, 98, 59]),
    ...many("mcp", "mcp-server-", [310, 220, 180, 120, 90, 59, 20, 8], "measured"),
    ...many("plugins", "plugin-", [210, 160, 90, 40, 20]),
    ...many("agents", "agent-", [140, 120, 75, 20]),
    it("rules", "leak-scan.md", 1692, "estimated", "conditional"),
    it("agents", "reviewer.md", 560, "estimated", "conditional"),
    it("hooks", "hook 20개", null, "estimated", "conditional"),
    it("tools", "지연 로딩 도구 목록 (2개)", 74, "measured", "deferred"),
  ];
  const codexItems: ContextItem[] = [
    it("system", "시스템 지시문", 3450, "measured"),
    it("instructions", "AGENTS.md", 3450),
    it("skills", "스킬 목록 (스킬 12개)", 2100),
    ...many("rules", "rule-", [800, 640, 420, 260, 150, 90, 40]),
    ...many("mcp", "mcp-server-", [300, 120]),
    it("hooks", "hook 11개", null, "estimated", "conditional"),
    it("mcp", "mcp-lazy", null, "estimated", "deferred"),
  ];
  const kiroItems: ContextItem[] = [
    it("instructions", "AGENTS.md", 1500),
    it("rules", "steering-1.md", 900),
    it("rules", "steering-2.md", 700),
    it("skills", "스킬 목록 (스킬 9개)", 600),
    ...many("rules", "steering-x", [220, 150, 90, 40]),
    it("skills", "deploy", null, "estimated", "conditional"),
  ];
  const agent = (
    name: AgentContext["agent"],
    items: ContextItem[],
    window: number | null,
    firstRequestTokens: number | null,
    warnings: string[],
  ): AgentContext => ({
    agent: name,
    items,
    totalStartupTokens: total(items),
    firstRequestTokens,
    firstRequestChars: null,
    model: null,
    contextWindow: window,
    warnings,
  });
  return {
    computedAt: new Date(now - 60_000).toISOString(),
    agents: [
      agent("claude", claudeItems, 400000, null, [
        "hook 20개: 출력 크기는 실행할 때 정해져 세지 않았습니다.",
        "실측값은 가장 최근 Claude 세션(작업 폴더 gureumi) 기준입니다.",
      ]),
      agent("codex", codexItems, 475000, 15000, [
        "hook 11개: 출력 크기는 실행할 때 정해져 세지 않았습니다.",
        "MCP 서버 1개: 스키마는 연결해야 알 수 있어 세지 않았습니다.",
        "실측값은 가장 최근 Codex 세션(작업 폴더 gureumi) 기준입니다.",
      ]),
      agent("kiro", kiroItems, null, null, []),
    ],
  };
}
