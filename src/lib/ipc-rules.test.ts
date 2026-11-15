// 0018 작업 B IPC fixture(src-tauri/src/ipc_fixtures.rs `claude_rules_types_match_fixture`)가 ipc.ts 타입과
// 키·enum 값까지 같은지 본다. 공통 계약 테스트는 ipc.test.ts(800줄 아래로 두려고 나눴다).
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import type { AppliedSwitch, Proposal, RuleProblem, RuleProposal, RulesPreview } from "@/lib/ipc";

type Fields<T> = Record<keyof T, "required" | "optional">;

function fixture(name: string): unknown {
  return JSON.parse(readFileSync(resolve("src/test/fixtures/ipc", `${name}.json`), "utf8"));
}

function expectKeys<T>(value: unknown, fields: Fields<T>, where: string): void {
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

const RULES_PREVIEW = {
  id: "required",
  scope: "required",
  folder: "required",
  rules: "required",
  unknownKeys: "required",
} satisfies Fields<RulesPreview>;
const RULE_PROPOSAL = {
  key: "required",
  name: "required",
  chars: "required",
  hasFrontMatter: "required",
  referencedBy: "required",
  beforeHash: "required",
  proposal: "required",
  reason: "required",
  skillExists: "required",
  canApply: "required",
} satisfies Fields<RuleProposal>;
type ProposalOf<C extends Proposal["choice"]> = Extract<Proposal, { choice: C }>;
const PROPOSAL: { [C in Proposal["choice"]]: Fields<ProposalOf<C>> } = {
  paths: { choice: "required", paths: "required" },
  skill: { choice: "required", name: "required", description: "required" },
  keep: { choice: "required" },
  unusable: { choice: "required", problem: "required" },
};
const RULE_PROBLEM = {
  missing: true,
  duplicate: true,
  choice: true,
  skillName: true,
  description: true,
  pathsCount: true,
  pathText: true,
  pathAbsolute: true,
} satisfies Record<RuleProblem, true>;

describe("Rust rules IPC fixtures match ipc.ts types", () => {
  it("rules preview rows have the keys of RuleProposal and every proposal choice", () => {
    const preview = fixture("rules-preview") as RulesPreview;
    expectKeys<RulesPreview>(preview, RULES_PREVIEW, "RulesPreview");
    preview.rules.forEach((r) => {
      expectKeys<RuleProposal>(r, RULE_PROPOSAL, "RuleProposal");
      const fields = PROPOSAL[r.proposal.choice] as Fields<Proposal>;
      expectKeys<Proposal>(r.proposal, fields, `Proposal.${r.proposal.choice}`);
    });
    expect([...new Set(preview.rules.map((r) => r.proposal.choice))].sort()).toEqual(
      Object.keys(PROPOSAL).sort(),
    );
  });

  it("rule problems are exactly the RuleProblem values", () => {
    expect([...(fixture("rule-problems") as string[])].sort()).toEqual(
      Object.keys(RULE_PROBLEM).sort(),
    );
  });

  it("skill move record carries created, old records do not", () => {
    const moved = fixture("rules-applied") as AppliedSwitch;
    expect(moved.created).toBe("/synthetic/.claude/skills/synthetic-skill/SKILL.md");
    const old = fixture("coach-log") as AppliedSwitch[];
    old.forEach((e) => expect("created" in e).toBe(false));
  });
});
