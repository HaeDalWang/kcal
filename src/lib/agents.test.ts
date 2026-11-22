import { describe, expect, it } from "vitest";

import { AGENT_DESCRIPTORS, hasFeature, type AgentFeature } from "@/lib/agents";
import { AGENTS, type Agent } from "@/lib/ipc";

// 이름 분기를 걷기 전 화면이 쓰던 값(0027 ①). 바꾸면 화면이 바뀐다.
const OLD_BRANCHES: Record<Agent, { label: string; unit: string; features: AgentFeature[] }> = {
  claude: {
    label: "Claude",
    unit: "request",
    features: ["checks", "usageEvidence", "measuredStartup", "handoff"],
  },
  codex: {
    label: "Codex",
    unit: "request",
    features: ["behaviorDetails", "switchShare", "checks"],
  },
  kiro: { label: "Kiro", unit: "question", features: ["creditLimit", "checks"] },
};
const ALL_FEATURES: AgentFeature[] = [
  "creditLimit",
  "behaviorDetails",
  "switchShare",
  "checks",
  "usageEvidence",
  "measuredStartup",
  "handoff",
];

describe("agent descriptors", () => {
  it("has a descriptor for every agent in AGENTS", () => {
    for (const agent of AGENTS) {
      const d = AGENT_DESCRIPTORS[agent];
      expect(d.id).toBe(agent);
      expect(d.color).toBe(`var(--agent-${agent})`);
      expect(d.mark.d.length).toBeGreaterThan(0);
    }
    expect(Object.keys(AGENT_DESCRIPTORS).sort()).toEqual([...AGENTS].sort());
  });

  it.each(AGENTS)("keeps the label, unit and features the old name branches gave %s", (agent) => {
    const want = OLD_BRANCHES[agent];
    expect(AGENT_DESCRIPTORS[agent].label).toBe(want.label);
    expect(AGENT_DESCRIPTORS[agent].unit).toBe(want.unit);
    for (const feature of ALL_FEATURES) {
      expect(hasFeature(agent, feature)).toBe(want.features.includes(feature));
    }
  });
});
