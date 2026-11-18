// 3층 분해(docs/decisions/0008): 시작 컨텍스트 토큰 T가 Codex·Claude 5시간 한도를 창당 몇 %p 먹는가(추론).
// 몫(%p) = T × (N − M) / 캐시입력_1%p당토큰 + T × M / 비캐시입력_1%p당토큰
// N = 창당 요청 수, M = 그중 캐시 미스 수(Rust `WindowPattern`, 실측). 미스 요청은 캐시 항에서 뺀다.
// ponytail: 입력 두 개(환율·창 패턴, 항목 토큰)가 이미 프론트 store에 있어 계산을 TS에 둔다.
// 5층 코칭에서 Rust가 몫을 써야 하면 `src-tauri/src/rate/`로 옮긴다.
import type { Agent, AgentUsage, CodexRate, RateSnapshot } from "@/lib/ipc";
import { hasFeature } from "@/lib/agents";

/** 토큰 환율이 있는 에이전트(Codex·Claude)의 환율. Kiro는 크레딧/질문이라 null. */
export function tokenRateOf(
  rates: RateSnapshot | null | undefined,
  agent: Agent,
): CodexRate | null {
  if (!rates || hasFeature(agent, "creditLimit")) return null;
  return agent === "codex" ? rates.codex : rates.claude;
}

/** 1%p당 토큰. null이면 그 계수가 0이라 그 항은 0. */
type PerPercent = { cachedInput: number | null; uncachedInput: number | null };

/** 창 하나(요청 N, 미스 M)에서 토큰 `tokens`의 몫(%p). */
export function windowShare(
  tokens: number,
  requests: number,
  misses: number,
  rate: PerPercent,
): number {
  const cached = rate.cachedInput === null ? 0 : (tokens * (requests - misses)) / rate.cachedInput;
  const uncached = rate.uncachedInput === null ? 0 : (tokens * misses) / rate.uncachedInput;
  return cached + uncached;
}

/** 점 추정과 범위(%p, 모두 추론). */
export type Share = {
  /** 중앙값 창 × 점 추정 환율. */
  point: number;
  /** 중앙값 창 × LOO fold 중 1%p당 토큰이 가장 큰 쪽. */
  low: number;
  /** p90 창 × LOO fold 중 1%p당 토큰이 가장 작은 쪽. */
  high: number;
};

/** 환율이 `estimated`이고 창 패턴이 있을 때만 몫을 낸다. 아니면 null(화면에 %p를 붙이지 않는다). */
export function shareOf(tokens: number, rate: CodexRate): Share | null {
  const { perPercent, foldRange, pattern } = rate;
  if (rate.status !== "estimated" || !perPercent || !foldRange || !pattern) return null;
  const { requests, misses } = pattern;
  const { cachedInput, uncachedInput } = foldRange;
  return {
    point: windowShare(tokens, requests.median, misses.median, perPercent),
    low: windowShare(tokens, requests.median, misses.median, {
      cachedInput: cachedInput.max,
      uncachedInput: uncachedInput.max,
    }),
    high: windowShare(tokens, requests.p90, misses.p90, {
      cachedInput: cachedInput.min,
      uncachedInput: uncachedInput.min,
    }),
  };
}

/**
 * 1%당 토큰(한 숫자, 입력 토큰) = N / windowShare(1, N, M, perPercent) (docs/decisions/0009).
 * 보통 5시간 구간(요청 N, 캐시 미스 M)에서 입력 토큰 1개가 차지하는 %의 역수. 출력 토큰은 빠진다.
 * 두 계수가 모두 null이면 몫이 0이라 null.
 */
export function tokensPerPercent(
  requests: number,
  misses: number,
  rate: PerPercent,
): number | null {
  const share = windowShare(1, requests, misses, rate);
  return share > 0 ? requests / share : null;
}

/** 보통 5시간 구간이 무엇으로 찼나(%, 예상, docs/decisions/0013). */
export type Composition = {
  /** 시작 설정 몫 = 0008 점 추정(windowShare(시작 합계 토큰, 창 패턴 중앙값 N·M)). 시작 토큰을 모르면 null. */
  startup: number | null;
  /** 대화 누적 = 입력 몫 − 시작 설정 몫(음수면 0). 시작 토큰을 모르면 입력 몫 전체. */
  conversation: number;
  /** 답변(출력) 몫. */
  output: number;
  /** 예상 합 = 입력 몫 + 출력 몫(Rust 값). */
  predicted: number;
  /** 같은 구간의 실제 사용 %(실측). */
  used: number;
};

/**
 * 환율이 `estimated`이고 구간 구성·창 패턴이 있을 때만. 예상 합이 0 이하면 null(화면은 섹션을 숨긴다).
 * 시작 설정 몫은 "세션 시작 토큰" 줄과 같은 값을 쓴다(한 화면에서 두 숫자가 어긋나지 않게).
 */
export function compositionOf(rate: CodexRate, startupTokens: number | null): Composition | null {
  const { composition, perPercent, pattern } = rate;
  if (rate.status !== "estimated" || !perPercent || !composition || !pattern) return null;
  const { inputShare, outputShare, usedPercent } = composition;
  const predicted = inputShare + outputShare;
  if (!(predicted > 0)) return null;
  const { requests, misses } = pattern;
  const startup =
    startupTokens === null
      ? null
      : windowShare(startupTokens, requests.median, misses.median, perPercent);
  // ponytail: 시작 설정이 모든 요청에 그대로 실린다고 보고 입력 몫에서 뺀다. 압축 등으로 일부 요청에서
  // 빠지면 설정 몫이 커 보이고 대화 누적이 0으로 잘린다. 요청별 시작 토큰을 기록하게 되면 그 값으로 뺀다.
  const conversation = Math.max(0, inputShare - (startup ?? 0));
  return { startup, conversation, output: outputShare, predicted, used: usedPercent };
}

/** Kiro 월 한도(크레딧): usage 스냅샷 month 창의 `limit`(실측). 모르거나 0 이하면 null. */
export function kiroMonthlyLimit(usage: AgentUsage | undefined): number | null {
  const limit = usage?.windows.find((w) => w.id === "month")?.limit;
  return limit !== undefined && limit > 0 ? limit : null;
}

/** 한 턴 ≈ 월 한도의 X% = 크레딧/턴 ÷ (한도 / 100). 한도를 모르면 null. */
export function kiroTurnShare(creditsPerTurn: number, limit: number | null): number | null {
  return limit === null ? null : creditsPerTurn / (limit / 100);
}
