// 점검 탭의 합성 카드·스위치. 테스트(check-copy·checks-view·checks-tab·App)와 대시보드 미리보기가 같이 쓴다.
// D·C는 이 Mac(2026-11-07 실측, evidence/real-checks.json) 모양, B는 예시다. 경로는 합성이다.
import type {
  AppliedSwitch,
  CheckPreview,
  CoachCheck,
  CoachSwitch,
  HandoffPreview,
  ModelVerdict,
  RuleFact,
  RuleProposal,
  RulesPreview,
  ScopeMovePreview,
  SwitchUsage,
} from "@/lib/ipc";

const SETTINGS = "/Users/me/.claude/settings.json";

/** 적용 기록 이름은 Rust가 화면 언어로 지어 준다(`pick`/`tr!`). 미리보기 `?lang=en`이면 영어 이름, 테스트·그 밖에는 한국어. */
const isEnPreview =
  typeof location !== "undefined" && new URLSearchParams(location.search).get("lang") === "en";
const appliedLabel = (ko: string, en: string) => (isEnPreview ? en : ko);

const verdict = (
  model: string,
  requests: number,
  ownEffort: ModelVerdict["ownEffort"],
  defaultEffort: ModelVerdict["defaultEffort"],
  isIgnored: boolean,
): ModelVerdict => ({ model, requests, ownEffort, defaultEffort, isIgnored });

export const EFFORT_CHECK: CoachCheck = {
  id: "D-claude-effort-level",
  kind: "D",
  agent: "claude",
  facts: {
    type: "effortLevel",
    value: "high",
    days: 30,
    models: [
      verdict("claude-opus-5-5", 9774, "xhigh", "medium", true),
      verdict("claude-sonnet-5-5", 38, "medium", "medium", true),
      verdict("claude-haiku-4-5", 19, null, "high", false),
      verdict("claude-haiku-5-5", 13, null, "medium", true),
    ],
    ignoredBy: ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"],
    movedTo: [],
    current: "claude-opus-5-5",
    unknownModels: [],
  },
  options: [
    {
      id: "remove",
      isRecommended: true,
      isWritable: true,
      instruction: null,
      changes: [{ location: "effortLevel", before: '"high"', after: null }],
    },
  ],
  file: SETTINGS,
  docs: [
    "https://code.claude.com/docs/en/settings-reference#effortlevel",
    "https://code.claude.com/docs/en/model-config#adjust-effort-level",
  ],
  tokens: null,
  isDismissed: false,
};

export const SUBAGENT_EFFORT_CHECK: CoachCheck = {
  id: "D-claude-subagent-effort",
  kind: "D",
  agent: "claude",
  facts: {
    type: "subagentEffort",
    days: 14,
    heavyRequests: 3,
    subagentRequests: 12,
    byEffort: [
      { effort: "high", requests: 9 },
      { effort: "xhigh", requests: 2 },
      { effort: "max", requests: 1 },
    ],
    agents: ["Explore", "researcher"],
    recommended: "high",
  },
  options: [
    {
      id: "set-high",
      isRecommended: true,
      isWritable: true,
      instruction: null,
      changes: [{ location: "env.CLAUDE_CODE_SUBAGENT_EFFORT", before: null, after: '"high"' }],
    },
    { id: "keep", isRecommended: false, isWritable: false, instruction: null, changes: [] },
  ],
  file: SETTINGS,
  docs: [
    "https://code.claude.com/docs/en/sub-agents#choose-an-effort-level",
    "https://code.claude.com/docs/en/env-vars",
  ],
  tokens: null,
  isDismissed: false,
};

export const GIT_CHECK: CoachCheck = {
  id: "C-claude-git-instructions",
  kind: "C",
  agent: "claude",
  facts: {
    type: "gitInstructions",
    matches: [
      {
        file: "/Users/me/.claude/rules/common/git-workflow.md",
        line: 3,
        text: "## Commit Message Format",
      },
      {
        file: "/Users/me/.claude/rules/common/git-workflow.md",
        line: 16,
        text: "## Pull Request Workflow",
      },
    ],
    hasOwnTrailer: false,
    preCommitSkills: [],
  },
  options: [
    {
      id: "disable-git-instructions",
      isRecommended: false,
      isWritable: true,
      instruction: null,
      changes: [{ location: "includeGitInstructions", before: null, after: "false" }],
    },
    { id: "keep", isRecommended: true, isWritable: false, instruction: null, changes: [] },
  ],
  file: SETTINGS,
  docs: ["https://code.claude.com/docs/en/settings-reference#includegitinstructions"],
  tokens: null,
  isDismissed: false,
};

export const HOOK_CHECK: CoachCheck = {
  id: "B-claude-agents-md-hook:0",
  kind: "B",
  agent: "claude",
  facts: {
    type: "agentsMdHook",
    settingsFile: "/Users/me/work/app/.claude/settings.json",
    scope: "project",
    command: "cat AGENTS.md",
    agentsMd: "/Users/me/work/app/AGENTS.md",
    workspace: "/Users/me/work/app",
  },
  options: [
    {
      id: "remove-hook",
      isRecommended: true,
      isWritable: false,
      instruction:
        '/Users/me/work/app/.claude/settings.json의 hooks.SessionStart에서 command가 "cat AGENTS.md"인 hook을 지우세요.',
      changes: [],
    },
  ],
  file: "/Users/me/work/app/.claude/settings.json",
  docs: ["https://code.claude.com/docs/en/memory#remove-an-earlier-agents-md-workaround"],
  tokens: 1240,
  isDismissed: false,
};

export const SKILL_CHECK: CoachCheck = {
  id: "B-claude-same-skill:playwright-cli",
  kind: "B",
  agent: "claude",
  facts: {
    type: "sameSkill",
    name: "playwright-cli",
    personalPath: "/Users/me/.claude/skills/playwright-cli/SKILL.md",
    pluginId: "playwright@market",
    pluginPath:
      "/Users/me/.claude/plugins/cache/market/playwright/1.0.0/skills/playwright-cli/SKILL.md",
    isSameContent: true,
  },
  options: [
    {
      id: "hide-personal",
      isRecommended: true,
      isWritable: true,
      instruction: null,
      changes: [
        {
          location: 'skillOverrides."playwright-cli"',
          before: null,
          after: '"user-invocable-only"',
        },
      ],
    },
    { id: "keep", isRecommended: false, isWritable: false, instruction: null, changes: [] },
  ],
  file: SETTINGS,
  docs: ["https://code.claude.com/docs/en/skills#resolve-skills-that-share-a-name"],
  tokens: 85,
  isDismissed: false,
};

const LONG_FILE = "/Users/me/work/app/CLAUDE.md";

/** 긴 안내문 카드(0018 작업 A, 합성). */
export const LONG_CHECK: CoachCheck = {
  id: `B-claude-long-instructions:${LONG_FILE}`,
  kind: "B",
  agent: "claude",
  facts: {
    type: "longInstructions",
    name: "CLAUDE.md",
    scope: "project",
    folder: "/Users/me/work/kcal",
    lines: 340,
    limit: 200,
    imports: ["AGENTS.md", "docs/style.md"],
  },
  options: [
    {
      id: "handoff",
      isRecommended: true,
      isWritable: false,
      instruction: "Claude에게 맡긴 뒤 미리보기에서 바꿔요.",
      changes: [],
    },
    { id: "keep", isRecommended: false, isWritable: false, instruction: null, changes: [] },
  ],
  file: LONG_FILE,
  docs: ["https://code.claude.com/docs/en/memory#write-effective-instructions"],
  tokens: null,
  isDismissed: false,
};

const same = (text: string) => ({ kind: "same" as const, text });
const removed = (text: string) => ({ kind: "removed" as const, text });
const added = (text: string) => ({ kind: "added" as const, text });

/** Claude가 고쳐 쓴 결과(합성). `@docs/style.md`가 빠져 경고가 뜬다. */
export const LONG_PREVIEW: HandoffPreview = {
  id: LONG_CHECK.id,
  file: LONG_FILE,
  beforeHash: "0123456789abcdef",
  beforeLines: 340,
  afterLines: 182,
  diff: [
    same("# Project rules"),
    same("@AGENTS.md"),
    removed("@docs/style.md"),
    same(""),
    same("## Commits"),
    removed("- Write commit messages in the imperative mood."),
    removed("- Always: write commit messages in the imperative mood."),
    added("- Write commit messages in the imperative mood (one line, under 72 chars)."),
    same(""),
    same("## Tests"),
    removed("- Run the test suite before you push."),
    removed("- Remember: run the tests before pushing."),
    added("- Run the test suite before you push."),
  ],
  removed: [
    "커밋 메시지 규칙 세 줄을 한 줄로 합쳤어요.",
    "테스트 실행 규칙 두 줄을 합쳤어요.",
    "@docs/style.md 줄을 뺐어요(아래 스타일 규칙과 겹쳐서).",
  ],
  missingImports: ["docs/style.md"],
  canApply: true,
};

export const LONG_APPLIED: AppliedSwitch = {
  id: "2",
  at: "2026-11-07T12:00:00Z",
  action: "apply",
  target: `claude/check:${LONG_CHECK.id}:handoff`,
  agent: "claude",
  name: appliedLabel("CLAUDE.md 줄이기", "Shorten CLAUDE.md"),
  file: LONG_FILE,
  backup: `${LONG_FILE}.kcal-20261107T120000000Z.bak`,
  enabled: false,
  tokens: null,
  afterHash: "fedcba9876543210",
  undoCommand: `cp '${LONG_FILE}.kcal-20261107T120000000Z.bak' '${LONG_FILE}'`,
  undoOf: null,
};

const RULES_ROOT = "/Users/me/.claude/rules";
const rule = (name: string, chars: number, referencedBy = 0): RuleFact => ({
  name,
  chars,
  hasFrontMatter: false,
  referencedBy,
});

/** 늘 실리는 규칙 카드(0018 작업 B). 이 Mac(2026-11-08 실측) 사용자 범위의 이름·글자 수, 경로는 합성이다. */
export const RULES_CHECK: CoachCheck = {
  id: `A2-claude-rules-paths:${RULES_ROOT}`,
  kind: "A",
  agent: "claude",
  facts: {
    type: "rulesPaths",
    scope: "user",
    folder: "/Users/me",
    chars: 20631,
    limit: 40000,
    rules: [
      rule("common/coding-style.md", 2618),
      rule("common/development-workflow.md", 2482, 1),
      rule("common/git-workflow.md", 775, 1),
      rule("common/kiro-delegation.md", 6229),
      rule("common/performance.md", 2911),
      rule("common/security.md", 920),
      rule("common/simplicity.md", 3267, 1),
      rule("common/testing.md", 541, 1),
      rule("common/working-with-me.md", 888),
    ],
  },
  options: [
    {
      id: "handoff",
      isRecommended: true,
      isWritable: false,
      instruction: "Claude에게 맡긴 뒤 규칙마다 골라 바꿔요.",
      changes: [],
    },
    { id: "keep", isRecommended: false, isWritable: false, instruction: null, changes: [] },
  ],
  file: RULES_ROOT,
  docs: [
    "https://code.claude.com/docs/en/memory#path-specific-rules",
    "https://code.claude.com/docs/en/skills#frontmatter-reference",
  ],
  tokens: null,
  isDismissed: false,
};

const proposalRow = (
  key: string,
  name: string,
  chars: number,
  proposal: RuleProposal["proposal"],
  extra: Partial<RuleProposal> = {},
): RuleProposal => ({
  key,
  name,
  chars,
  hasFrontMatter: false,
  referencedBy: 0,
  beforeHash: `00000000000000${key.slice(1).padStart(2, "0")}`,
  proposal,
  reason: "",
  skillExists: false,
  canApply: proposal.choice === "paths" || proposal.choice === "skill",
  ...extra,
});

/** Claude 제안(합성): paths·스킬·그대로 두기 + 경고 + 검사 탈락 한 줄. */
export const RULES_PREVIEW: RulesPreview = {
  id: RULES_CHECK.id,
  scope: "user",
  folder: "/Users/me",
  rules: [
    proposalRow(
      "r1",
      "common/coding-style.md",
      2618,
      { choice: "paths", paths: ["src/**/*.{ts,tsx}", "src-tauri/src/**/*.rs"] },
      { reason: "코드를 고칠 때만 필요한 규칙이에요." },
    ),
    proposalRow(
      "r2",
      "common/git-workflow.md",
      775,
      {
        choice: "skill",
        name: "git-workflow",
        description: "Use when writing a commit message or opening a pull request.",
      },
      { referencedBy: 1, reason: "커밋하거나 PR을 만들 때만 필요해요." },
    ),
    proposalRow(
      "r3",
      "common/working-with-me.md",
      888,
      { choice: "keep" },
      { reason: "모든 대화에 필요한 말투 규칙이에요." },
    ),
    proposalRow("r4", "common/kiro-delegation.md", 6229, {
      choice: "unusable",
      problem: "skillName",
    }),
  ],
  unknownKeys: 0,
};

export const RULES_APPLIED: AppliedSwitch = {
  id: "3",
  at: "2026-11-08T12:00:00Z",
  action: "apply",
  target: `claude/check:${RULES_CHECK.id}:paths`,
  agent: "claude",
  name: appliedLabel("coding-style.md 필요할 때만 싣기", "Load coding-style.md only when needed"),
  file: `${RULES_ROOT}/common/coding-style.md`,
  backup: `${RULES_ROOT}/common/coding-style.md.kcal-20261108T120000000Z.bak`,
  enabled: false,
  tokens: null,
  afterHash: "fedcba9876543210",
  undoCommand: `cp '${RULES_ROOT}/common/coding-style.md.kcal-20261108T120000000Z.bak' '${RULES_ROOT}/common/coding-style.md'`,
  undoOf: null,
};

/** Codex 빠른 모드 카드(합성, 2026-11-21). 모델이 문서 표에 있어 배수를 말한다. */
export const CODEX_FAST_CHECK: CoachCheck = {
  id: "D-codex-fast-mode",
  kind: "D",
  agent: "codex",
  facts: { type: "codexFastMode", model: "gpt-6.2-astra", multiplier: 2 },
  options: [
    {
      id: "standard",
      isRecommended: true,
      isWritable: false,
      instruction:
        "Codex에서 `/fast`를 한 번 더 입력하세요. Standard로 돌아가고, 그 선택은 Codex가 저장해요.",
      changes: [],
    },
    { id: "keep", isRecommended: false, isWritable: false, instruction: null, changes: [] },
  ],
  file: "/Users/me/.codex/config.toml",
  docs: [
    "https://learn.chatgpt.com/docs/agent-configuration/speed#fast-mode",
    "https://learn.chatgpt.com/docs/pricing#speed-multipliers",
  ],
  tokens: null,
  isDismissed: false,
};

export const THIS_MAC_CHECKS = [EFFORT_CHECK, GIT_CHECK];
export const ALL_CHECKS = [EFFORT_CHECK, GIT_CHECK, HOOK_CHECK, SKILL_CHECK];

const hidden = (check: CoachCheck): CoachCheck => ({ ...check, isDismissed: true });
/** 이 Mac에서 C를 "그대로 두기"로 숨긴 뒤. */
export const MAC_GIT_HIDDEN = [EFFORT_CHECK, hidden(GIT_CHECK)];
/** 보이는 카드가 모두 숨겨진 상태. */
export const ALL_HIDDEN = [hidden(GIT_CHECK)];

export const EFFORT_PREVIEW: CheckPreview = {
  id: EFFORT_CHECK.id,
  option: "remove",
  file: SETTINGS,
  changes: [{ location: "effortLevel", before: '"high"', after: null }],
  beforeHash: "0123456789abcdef",
};

export const EFFORT_APPLIED: AppliedSwitch = {
  id: "1",
  at: "2026-11-07T12:00:00Z",
  action: "apply",
  target: "claude/check:D-claude-effort-level:remove",
  agent: "claude",
  name: appliedLabel("생각 깊이 옛 설정 정리", "Clean up the old thinking-depth setting"),
  file: SETTINGS,
  backup: `${SETTINGS}.kcal-20261107T120000000Z.bak`,
  enabled: false,
  tokens: null,
  afterHash: "fedcba9876543210",
  undoCommand: `cp '${SETTINGS}.kcal-20261107T120000000Z.bak' '${SETTINGS}'`,
  undoOf: null,
};

/** 내용이 달라 "그대로 두기"를 추천하는 같은 스킬 카드를 숨긴 상태. */
export const SKILL_DIFF_HIDDEN: CoachCheck = {
  ...SKILL_CHECK,
  id: "B-claude-same-skill:frontend-slides",
  facts: {
    type: "sameSkill",
    name: "frontend-slides",
    personalPath: "/Users/me/.claude/skills/frontend-slides/SKILL.md",
    pluginId: "slides@market",
    pluginPath:
      "/Users/me/.claude/plugins/cache/market/slides/1.0.0/skills/frontend-slides/SKILL.md",
    isSameContent: false,
  },
  options: [
    { ...SKILL_CHECK.options[0], isRecommended: false },
    { id: "keep", isRecommended: true, isWritable: false, instruction: null, changes: [] },
  ],
  isDismissed: true,
};

/** 대시보드 점검 탭: 이 Mac의 D·C + 숨긴 카드 하나. 알림은 D 하나. */
// dashboard.html?tab=checks&subagent-effort로 새 카드를 미리 본다.
export const DASHBOARD_CHECKS = [
  EFFORT_CHECK,
  GIT_CHECK,
  SKILL_DIFF_HIDDEN,
  ...(typeof location !== "undefined" && new URLSearchParams(location.search).has("subagent-effort")
    ? [SUBAGENT_EFFORT_CHECK]
    : []),
];

const sw = (
  target: string,
  name: string,
  tokens: number | null,
  extra: Partial<CoachSwitch> = {},
): CoachSwitch => {
  const agent = target.split("/")[0] as CoachSwitch["agent"];
  return {
    target,
    agent,
    kind: target.includes("/mcp:") ? "mcp" : "plugin",
    name,
    enabled: true,
    tokens,
    loading: "startup",
    isProposed: tokens !== null,
    usage: null,
    scope: null,
    scopeMove: null,
    contents: null,
    skill: null,
    format: agent === "codex" ? "toml" : "json",
    file: agent === "codex" ? "/Users/me/.codex/config.toml" : "/Users/me/.kiro/settings/mcp.json",
    instruction: null,
    ...extra,
  };
};

const LAST_USED = Date.UTC(2026, 10, 5, 12) / 1000;
const usage = (projects: [string, number][], projectCount = projects.length): SwitchUsage => ({
  days: 30,
  total: projects.reduce((sum, [, n]) => sum + n, 0),
  projects: projects.map(([name, count]) => ({
    name,
    path: `/Users/me/work/${name}`,
    count,
    exists: true,
  })),
  projectCount,
  isPartial: false,
  sessions: 24,
  minSessions: 10,
  scopeMinUses: 3,
  // 11월 5일 정오 UTC: 어느 시간대에서도 같은 날짜.
  lastUsed: projects.length > 0 ? LAST_USED : null,
  lastProject: projects[0]?.[0] ?? null,
});
const ALL_PROJECTS = { user: true, projects: [] };

/** Connector guidance and the same project MCP in two distinct work folders. */
export const DIAG46_SWITCHES: CoachSwitch[] = [
  sw("claude/connector:claude.ai Figma", "claude.ai Figma", 900, {
    kind: "mcp",
    format: "manual",
    file: null,
    isProposed: false,
    instruction: appliedLabel(
      "하나 끄기: 해당 프로젝트에서 /mcp → claude.ai Figma → 끄기(disabledMcpServers). 또는 deniedMcpServers에 표시 이름을 넣으세요.\n전부 끄기: disableClaudeAiConnectors: true 또는 ENABLE_CLAUDEAI_MCP_SERVERS=false.\nDesktop local/SSH: https://claude.ai/customize/connectors 에서 연결을 끊으세요. kcal은 설정을 바꾸지 않아요.",
      "Turn off one: in that project, /mcp → claude.ai Figma → disable (disabledMcpServers). Or add its display name to deniedMcpServers.\nTurn off all: disableClaudeAiConnectors: true or ENABLE_CLAUDEAI_MCP_SERVERS=false.\nDesktop local/SSH: disconnect at https://claude.ai/customize/connectors. kcal does not change settings.",
    ),
  }),
  ...["kcal", "side-app"].map((folder) =>
    sw(
      `claude/mcp:${JSON.stringify(["project-docs", `/Users/me/work/${folder}`])}`,
      "project-docs",
      null,
      {
        kind: "mcp",
        projectPath: `/Users/me/work/${folder}`,
        format: "manual",
        file: null,
        isProposed: false,
        instruction: appliedLabel(
          "이 폴더의 Claude Code에서 /mcp를 열고 이 서버를 끄세요.",
          "Open /mcp in Claude Code in this folder and disable this server.",
        ),
      },
    ),
  ),
];

/** 대시보드 점검 탭의 진단하기 스위치(에이전트마다 몇 개). Claude는 사용 기록·범위가 붙는다(0022). */
export const DASHBOARD_SWITCHES: CoachSwitch[] = [
  ...DIAG46_SWITCHES,
  sw("claude/plugin:k8s-tools@market", "k8s-tools@market", 266, {
    file: "/Users/me/.claude/settings.json",
    usage: usage([]),
    scope: ALL_PROJECTS,
  }),
  sw("claude/plugin:review-kit@market", "review-kit@market", 785, {
    file: "/Users/me/.claude/settings.json",
    isProposed: false,
    usage: usage([["side-app", 4]]),
    scope: ALL_PROJECTS,
    scopeMove: { projects: [{ name: "side-app", path: "/Users/me/work/side-app" }], skipped: [] },
  }),
  sw("claude/plugin:context7@claude-plugins-official", "context7@claude-plugins-official", null, {
    file: "/Users/me/.claude/settings.json",
    usage: usage(
      [
        ["kcal", 5],
        ["side-app", 2],
        ["kb-pipeline", 2],
      ],
      4,
    ),
    scope: ALL_PROJECTS,
  }),
  sw("claude/mcp:context7", "context7", 1830, {
    format: "manual",
    file: null,
    isProposed: false,
    instruction: "Claude Code에서 /mcp를 열고 이 서버를 끄세요.",
    usage: usage([["kcal", 10]]),
    scopeMove: { projects: [{ name: "kcal", path: "/Users/me/work/kcal" }], skipped: [] },
    scopeGuide: {
      commands: [
        "cd '/Users/me/work/kcal' && claude mcp add-json -s local 'context7' -",
        `{"type":"http","url":"https://example.test/mcp"}`,
        "claude mcp remove -s user 'context7'",
      ],
      undoCommands: [
        "claude mcp add-json -s user 'context7' -",
        `{"type":"http","url":"https://example.test/mcp"}`,
        "cd '/Users/me/work/kcal' && claude mcp remove -s local 'context7'",
      ],
    },
  }),
  sw("kiro/mcp:docs", "docs", 500, {
    isProposed: false,
    usage: usage([["kcal", 8]]),
    scopeMove: { projects: [{ name: "kcal", path: "/Users/me/work/kcal" }], skipped: [] },
    scopeGuide: {
      commands: [
        `이 JSON을 '/Users/me/work/kcal/.kiro/settings/mcp.json'에 붙여 넣어요. 다른 항목은 유지해요.`,
        `{"mcpServers":{"docs":{"command":"node","args":["docs.js"],"env":{"TOKEN":"<직접 입력>"},"disabled":false}}}`,
        `프로젝트 설정 확인 후 '~/.kiro/settings/mcp.json'의 mcpServers["docs"]에 "disabled": true를 넣어요.`,
      ],
      undoCommands: [
        `사용자 mcpServers["docs"]의 disabled를 지우거나 false로 바꿔요.`,
        `프로젝트 mcpServers에서 "docs" 항목만 삭제해요.`,
      ],
    },
  }),
  // 스킬 단위(0022 "스킬 단위"): 안 쓴 개인 스킬(제안), 숨겨 둔 프로젝트 스킬(다시 보이기), 사용자가 직접 정한 값(버튼 없음).
  sw("claude/skill:slides-helper", "slides-helper", null, {
    kind: "skill",
    loading: null,
    isProposed: true,
    file: "/Users/me/.claude/settings.json",
    usage: usage([]),
    skill: { state: null, project: null },
  }),
  sw("claude/project-skill:release-notes", "release-notes", null, {
    kind: "projectSkill",
    loading: null,
    enabled: false,
    isProposed: false,
    file: "/Users/me/work/kcal/.claude/settings.local.json",
    usage: usage([]),
    skill: { state: "user-invocable-only", project: "kcal" },
  }),
  sw("claude/skill:old-deploy", "old-deploy", null, {
    kind: "skill",
    loading: null,
    enabled: false,
    isProposed: false,
    format: "manual",
    file: null,
    instruction: "이미 name-only(으)로 정해 둔 스킬이에요.",
    usage: usage([]),
    skill: { state: "name-only", project: null },
  }),
  sw("codex/plugin:github@openai-curated", "github@openai-curated", 2410),
  sw("codex/mcp:playwright", "playwright", 960),
  // Codex MCP "프로젝트에서만 켜기"(0022 Codex 갱신): 코드 모드 호출 기록에서 센 사용 횟수. 크기는 모른다.
  sw("codex/mcp:code-review-graph", "code-review-graph", null, {
    usage: usage([
      ["kcal", 7],
      ["side-app", 2],
    ]),
    scopeMove: {
      projects: [
        { name: "kcal", path: "/Users/me/work/kcal" },
        { name: "side-app", path: "/Users/me/work/side-app" },
      ],
      skipped: [],
    },
  }),
  sw("codex/plugin:figma@openai-curated", "figma@openai-curated", null, {
    enabled: false,
    isProposed: false,
  }),
  sw("kiro/mcp:aws-docs", "aws-docs", 1120),
  sw("kiro/mcp:fetch", "fetch", null, { loading: "deferred" }),
];

/** "프로젝트에서만 켜기" 미리보기(review-kit → side-app). */
export const SCOPE_PREVIEW: ScopeMovePreview = {
  target: "claude/plugin:review-kit@market",
  changes: [
    {
      file: "/Users/me/work/side-app/.claude/settings.local.json",
      location: 'enabledPlugins."review-kit@market"',
      before: null,
      after: "true",
      isNewFile: true,
    },
    {
      file: "/Users/me/.claude/settings.json",
      location: 'enabledPlugins."review-kit@market"',
      before: '"review-kit@market": true',
      after: '"review-kit@market": false',
      isNewFile: false,
    },
  ],
  skipped: [],
  beforeHash: "0011223344556677",
};

/** Codex MCP "프로젝트에서만 켜기" 미리보기(code-review-graph → kcal, side-app). */
export const CODEX_SCOPE_PREVIEW: ScopeMovePreview = {
  target: "codex/mcp:code-review-graph",
  changes: [
    {
      file: "/Users/me/work/kcal/.codex/config.toml",
      location: "파일 끝에 새 [mcp_servers.code-review-graph] section",
      before: null,
      after: "enabled = true",
      isNewFile: true,
    },
    {
      file: "/Users/me/work/side-app/.codex/config.toml",
      location: "[mcp_servers.code-review-graph] 안 2번째 줄",
      before: "enabled = false",
      after: "enabled = true",
      isNewFile: false,
    },
    {
      file: "/Users/me/.codex/config.toml",
      location: "[mcp_servers.code-review-graph] 바로 아래 (새 줄, 12번째 줄)",
      before: null,
      after: "enabled = false",
      isNewFile: false,
    },
  ],
  skipped: [],
  beforeHash: "8899aabbccddeeff",
};

/** 진단하기 "최근 바꾼 것" 한 줄(Codex). */
export const CODEX_APPLIED: AppliedSwitch = {
  id: "7",
  at: "2026-11-05T09:00:00Z",
  action: "apply",
  target: "codex/plugin:linear@openai-curated",
  agent: "codex",
  name: "linear@openai-curated",
  file: "/Users/me/.codex/config.toml",
  backup: "/Users/me/.codex/config.toml.kcal-20261105T090000000Z.bak",
  enabled: false,
  tokens: 700,
  afterHash: "00112233aabbccdd",
  undoCommand:
    "cp '/Users/me/.codex/config.toml.kcal-20261105T090000000Z.bak' '/Users/me/.codex/config.toml'",
  undoOf: null,
};

/** All six saved-setting cards, synthetic only. */
export const SAVED_ADVICE_CHECKS: CoachCheck[] = [
  ["reasoning-effort", "D", "codex", ['model_reasoning_effort = "max"']],
  [
    "instructions-size",
    "A",
    "codex",
    ["project_doc_max_bytes = 32768", "individual-over: 40000 bytes"],
  ],
  ["duplicate-hooks", "B", "codex", ["SessionStart", "/Users/me/.codex/hooks.json"]],
  [
    "tool-search-v3",
    "A",
    "kiro",
    ['"toolSearch.enabled": true', "toolSearch.minPct = 5", "toolSearch.minTokens = 50000"],
  ],
  [
    "tool-search-v2",
    "A",
    "kiro",
    ['"toolSearch.enabled": true', "toolSearch.minPct = 5", "toolSearch.minTokens = 50000"],
  ],
  [
    "auto-compaction",
    "D",
    "kiro",
    ['"chat.disableAutoCompaction": false', "compaction.autoCompactionThreshold = 80%"],
  ],
].map(([topic, kind, agent, evidence]) => ({
  id: `${kind}-${agent}-${topic}`,
  kind,
  agent,
  facts: { type: "savedAdvice", topic, evidence },
  options: [
    {
      id: "guide",
      isRecommended: true,
      isWritable: false,
      instruction: "Advice only",
      changes: [],
    },
    { id: "keep", isRecommended: false, isWritable: false, instruction: null, changes: [] },
  ],
  file:
    agent === "codex"
      ? "/Users/me/.codex/config.toml"
      : "/Users/me/project/.kiro/settings/cli.json",
  docs: [
    agent === "codex"
      ? "https://learn.chatgpt.com/docs/models"
      : "https://kiro.dev/docs/mcp/tool-search/",
  ],
  tokens: null,
  isDismissed: false,
})) as CoachCheck[];
