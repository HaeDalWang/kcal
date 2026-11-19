// 코치 스위치·적용 기록 IPC(`ipc.ts`가 다시 내보낸다. 컴포넌트는 `@/lib/ipc`에서 가져온다).
// 타입은 src-tauri/src/coach/와 짝을 이룬다. 계약을 바꾸면 양쪽과 테스트를 같이 고친다.
import { invoke } from "@tauri-apps/api/core";

import type { Agent, Loading, McpScopeGuide } from "@/lib/ipc";

/** Rust `coach::switches::SwitchKind`. */
/** skill = Claude 개인 스킬, projectSkill = 작업 폴더의 프로젝트 스킬(설명을 숨기는 스위치이고 끄는 것이 아니다, 0022 "스킬 단위"). */
export type SwitchKind = "plugin" | "mcp" | "skill" | "projectSkill";

/** Rust `coach::switches::SwitchFormat`. manual = kcal이 쓰지 않고 `instruction`만 보여 준다(Claude MCP). */
export type SwitchFormat = "toml" | "json" | "manual";

/** Rust `coach::switches::CoachSwitch`. 켜고 끄는 설정 값 하나(docs/decisions/0014). */
export type CoachSwitch = {
  /** `codex/plugin:<id>` 등. command에 이 값만 보낸다. */
  target: string;
  agent: Agent;
  kind: SwitchKind;
  name: string;
  enabled: boolean;
  /** 시작 때 실리는 토큰. 모르면 null. */
  tokens: number | null;
  loading: Loading | null;
  /** 끄기 제안 대상(켜져 있고, 시작 때 실리고, 토큰을 알고, 사용 기록이 있으면 최근 N일 0회). 목록은 제안 먼저 토큰 큰 순. */
  isProposed: boolean;
  /** 최근 N일 사용 횟수(Claude만). 모르면 null(Codex·Kiro). */
  usage: SwitchUsage | null;
  /** 지금 켠 범위(Claude 플러그인만). */
  scope: SwitchScope | null;
  /** "프로젝트에서만 켜기" 제안(Claude 플러그인). */
  scopeMove: ScopeMove | null;
  scopeGuide?: McpScopeGuide;
  /** 끄면 함께 꺼지는 것(플러그인 폴더의 이름들). MCP 스위치·폴더를 못 읽으면 null. */
  contents: PluginContents | null;
  /** 스킬 스위치(개인·프로젝트)의 현재 값과 프로젝트 이름. 스킬이 아니면 null. */
  skill: SkillInfo | null;
  format: SwitchFormat;
  file: string | null;
  instruction: string | null;
};

/** Rust `coach::skills::SkillInfo`. */
export type SkillInfo = {
  /** `"on"`이 아닌 현재 `skillOverrides` 값(어느 범위든). 없으면 null. */
  state: string | null;
  /** 프로젝트 스킬이면 그 폴더 이름. */
  project: string | null;
};

/** Rust `coach::contents::NameList`. 이름순 앞 3개와 전체 개수. */
export type NameList = { names: string[]; count: number };

/** Rust `coach::contents::PluginContents`. */
export type PluginContents = {
  skills: NameList;
  agents: NameList;
  commands: NameList;
  mcpServers: NameList;
};

/** Rust `coach::usage::ProjectUsage`. 프로젝트 = 세션을 시작한 폴더. */
export type ProjectUsage = {
  /** 폴더 이름. */
  name: string;
  path: string;
  count: number;
  /** 폴더가 지금 있는가. */
  exists: boolean;
};

/** Rust `coach::usage::SwitchUsage`. */
export type SwitchUsage = {
  days: number;
  total: number;
  /** 많이 쓴 순 위 3곳. */
  projects: ProjectUsage[];
  /** 쓴 프로젝트 수 전체. */
  projectCount: number;
  /** 기록을 일부만 봤다(0회여도 근거가 아니다). */
  isPartial: boolean;
  /** 기간 안에 읽은 Claude 세션 수(하위 에이전트 기록 제외). */
  sessions: number;
  /** 0회를 근거로 보려면 필요한 최소 세션 수(tuning `diagnoseMinSessions`). */
  minSessions: number;
  /** "프로젝트에서만 켜기"를 제안하려면 필요한 최소 사용 횟수(tuning `scopeMinUses`). */
  scopeMinUses: number;
  /** 가장 최근 사용 시각(유닉스 초). 한 번도 안 썼으면 null. */
  lastUsed: number | null;
  /** 가장 최근에 쓴 프로젝트의 폴더 이름. */
  lastProject: string | null;
};

/** Rust `coach::scope::ScopeFile`. local(`.claude/settings.local.json`)이 project(`.claude/settings.json`)보다 우선. */
export type ScopeFile = "project" | "local";

/** Rust `coach::scope::ProjectScope`. */
export type ProjectScope = { name: string; path: string; file: ScopeFile; enabled: boolean };

/** Rust `coach::scope::SwitchScope`. */
export type SwitchScope = {
  /** 사용자 settings 값(true = 모든 프로젝트에서 켬). */
  user: boolean;
  /** 최근 세션 프로젝트 중 그 프로젝트 설정에 값이 적힌 곳. */
  projects: ProjectScope[];
};

/** Rust `coach::scope::ProjectRef`. */
export type ProjectRef = { name: string; path: string };

/** Rust `coach::scope::ScopeMove`. */
export type ScopeMove = {
  /** 켤 프로젝트(지금 있는 폴더). */
  projects: ProjectRef[];
  /** 썼지만 폴더가 없어져 건너뛸 프로젝트 이름. */
  skipped: string[];
};

/** Rust `coach::scope::ScopeFileChange`. */
export type ScopeFileChange = {
  file: string;
  location: string;
  before: string | null;
  after: string;
  isNewFile: boolean;
};

/** Rust `coach::scope::ScopeMovePreview`. `beforeHash`를 `applyScopeMove`에 그대로 보낸다. */
export type ScopeMovePreview = {
  target: string;
  /** 프로젝트 파일들, 마지막이 사용자 설정. */
  changes: ScopeFileChange[];
  skipped: string[];
  beforeHash: string;
};

/** Rust `coach::scope::SCOPE_TARGET_PREFIX`. "프로젝트에서만 켜기" 적용 기록의 `target` 앞부분. */
export const SCOPE_TARGET_PREFIX = "claude/plugin-scope:";

/** Rust `coach::scope::codex::TARGET_PREFIX`. Codex MCP 서버 "프로젝트에서만 켜기" 적용 기록의 `target` 앞부분. */
export const CODEX_SCOPE_TARGET_PREFIX = "codex/mcp-scope:";

/** Rust `coach::skills::PERSONAL_PREFIX`·`PROJECT_PREFIX`. 스킬 설명 숨기기 적용 기록의 `target` 앞부분. */
export const SKILL_TARGET_PREFIX = "claude/skill:";
export const PROJECT_SKILL_TARGET_PREFIX = "claude/project-skill:";

/** Rust `coach::SwitchPreview`. `beforeHash`를 `applySwitch`에 그대로 보낸다. */
export type SwitchPreview = {
  target: string;
  enabled: boolean;
  file: string;
  location: string;
  /** 바꾸기 전 줄. 새로 넣는 줄이면 null. */
  before: string | null;
  after: string;
  beforeHash: string;
  /** false면 이미 그 값이다. */
  changed: boolean;
};

/** Rust `coach::log::CoachAction`. */
export type CoachAction = "apply" | "undo";

/** Rust `coach::log::AppliedSwitch`. 적용 기록 한 줄(`coach-log.jsonl`). */
export type AppliedSwitch = {
  id: string;
  at: string;
  action: CoachAction;
  target: string;
  agent: Agent;
  name: string;
  file: string;
  backup: string;
  /** 이 기록 뒤의 상태. */
  enabled: boolean;
  tokens: number | null;
  afterHash: string;
  /** 앱 없이 되돌리는 명령. */
  undoCommand: string;
  /** 되돌리기 기록이면 되돌린 적용 기록 ID. */
  undoOf: string | null;
  /** 이 적용이 새로 만든 파일(규칙을 스킬로 옮기면 그 `SKILL.md`). 없으면 키가 없다(예전 기록 포함). */
  created?: string;
  /** 같은 적용이 함께 바꾼 프로젝트 파일("프로젝트에서만 켜기"). 없으면 키가 없다. */
  files?: LoggedFile[];
};

/** Rust `coach::log::LoggedFile`. */
export type LoggedFile = {
  file: string;
  /** null이면 이 적용이 새로 만든 파일. */
  backup: string | null;
  afterHash: string;
  createdDir: boolean;
};

/** 이 컴퓨터에서 찾은 에이전트(Rust `usage::get_installed_agents`). */
export function getInstalledAgents(): Promise<Agent[]> {
  return invoke<Agent[]>("get_installed_agents");
}

export function getSwitches(): Promise<CoachSwitch[]> {
  return invoke<CoachSwitch[]>("get_switches");
}

/** 쓰지 않고 바꿀 줄을 계산한다. 목록 밖 대상·Claude MCP·고칠 수 없는 파일은 `invalid_input`(거부 이유). */
export function previewSwitch(target: string, enabled: boolean): Promise<SwitchPreview> {
  return invoke<SwitchPreview>("preview_switch", { target, enabled });
}

/** 백업 뒤 값 하나를 바꾼다. 미리보기 뒤 파일이 바뀌었으면 `invalid_input`으로 거부한다. */
export function applySwitch(
  target: string,
  enabled: boolean,
  beforeHash: string,
): Promise<AppliedSwitch> {
  return invoke<AppliedSwitch>("apply_switch", { target, enabled, beforeHash });
}

/** "프로젝트에서만 켜기" 미리보기. 쓰지 않는다. 제안이 없는 대상은 `invalid_input`. */
export function previewScopeMove(target: string): Promise<ScopeMovePreview> {
  return invoke<ScopeMovePreview>("preview_scope_move", { target });
}

/** 사용자 설정에서 끄고 쓴 프로젝트의 `.claude/settings.local.json`에서 켠다(백업 뒤, 한 기록). 되돌리기는 `undoSwitch`. */
export function applyScopeMove(target: string, beforeHash: string): Promise<AppliedSwitch> {
  return invoke<AppliedSwitch>("apply_scope_move", { target, beforeHash });
}

/** 적용 기록 하나를 백업으로 되돌린다. 적용 뒤 파일이 바뀌었으면 거부하고 `cp` 명령을 메시지에 준다. */
export function undoSwitch(id: string): Promise<AppliedSwitch> {
  return invoke<AppliedSwitch>("undo_switch", { id });
}

/** 적용 기록, 최근순. */
export function getCoachLog(): Promise<AppliedSwitch[]> {
  return invoke<AppliedSwitch[]>("get_coach_log");
}
