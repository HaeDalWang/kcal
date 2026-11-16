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
  name: "CLAUDE.md 줄이기",
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
  name: "coding-style.md 필요할 때만 싣기",
  file: `${RULES_ROOT}/common/coding-style.md`,
  backup: `${RULES_ROOT}/common/coding-style.md.kcal-20261108T120000000Z.bak`,
  enabled: false,
  tokens: null,
  afterHash: "fedcba9876543210",
  undoCommand: `cp '${RULES_ROOT}/common/coding-style.md.kcal-20261108T120000000Z.bak' '${RULES_ROOT}/common/coding-style.md'`,
  undoOf: null,
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
  name: "생각 깊이 옛 설정 정리",
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
export const DASHBOARD_CHECKS = [EFFORT_CHECK, GIT_CHECK, SKILL_DIFF_HIDDEN];

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

/** 대시보드 점검 탭의 진단하기 스위치(에이전트마다 몇 개). Claude는 사용 기록·범위가 붙는다(0022). */
export const DASHBOARD_SWITCHES: CoachSwitch[] = [
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
  }),
  sw("codex/plugin:github@openai-curated", "github@openai-curated", 2410),
  sw("codex/mcp:playwright", "playwright", 960),
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
