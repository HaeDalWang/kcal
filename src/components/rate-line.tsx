import type { ReactNode } from "react";

import { InfoSection } from "@/components/info-section";
import { SnapshotAge } from "@/components/snapshot-age";
import { StateText } from "@/components/state-text";
import { kiroTurnShare, tokensPerPercent } from "@/lib/breakdown";
import {
  type Agent,
  type Bounds,
  type CodexRate,
  type KiroModelCredits,
  type WindowPattern,
} from "@/lib/ipc";
import { ratesStore, useSnapshot } from "@/lib/snapshot-store";
import { numberFormat, t } from "@/lib/i18n";
import { hasFeature } from "@/lib/agents";

const compact = new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 });
const credits = numberFormat({ minimumFractionDigits: 2, maximumFractionDigits: 2 });
/** 요청 수는 정수로 보인다(짝수 개 중앙값·상위 10% 보간의 소수는 반올림). */
const count = numberFormat({ maximumFractionDigits: 0 });
const percent = numberFormat({ maximumFractionDigits: 1 });
const small = numberFormat({ maximumSignificantDigits: 2 });
const whole = numberFormat({ maximumFractionDigits: 0 });

/** 한도 몫(%) 표시: "0.19", "2.1", "40". 1 미만은 유효숫자 2자리, 10 미만은 소수 1자리, 그 위는 정수. */
export function formatShare(value: number): string {
  if (value < 1) return small.format(value);
  return value < 10 ? percent.format(value) : whole.format(value);
}

/** 토큰 수를 "742K", "5.5K", "1.9M"으로. null(계수 0)은 "—". */
export function formatTokens(value: number | null): string {
  return value === null ? "—" : compact.format(value);
}

function formatBounds({ min, max }: Bounds): string {
  if (min === null) return "—";
  return max === null ? `≥${formatTokens(min)}` : `${formatTokens(min)}~${formatTokens(max)}`;
}

/** 값 툴팁에 붙는 표본 크기("5시간 구간 40개"). R²는 사용자에게 보이지 않는다. 구간이 없으면 null. */
function sampleText(rate: CodexRate): string | null {
  return rate.status === "no_data" || rate.windows === 0 ? null : t.rate.windows(rate.windows);
}

/** 값 툴팁 여러 줄. 빈 줄은 뺀다. */
function lines(...parts: (string | null | false | undefined)[]): string | undefined {
  const text = parts.filter(Boolean).join("\n");
  return text || undefined;
}

/** 토큰 종류별 1%당 값(값 툴팁). 숫자가 없으면 null. */
export function codexDetailText(rate: CodexRate): string | null {
  const { perPercent, foldRange } = rate;
  if (rate.status === "estimated" && perPercent) {
    return t.rate.detail(
      formatTokens(perPercent.cachedInput),
      formatTokens(perPercent.uncachedInput),
      formatTokens(perPercent.output),
    );
  }
  if (rate.status === "range_only" && foldRange) {
    return t.rate.detail(
      formatBounds(foldRange.cachedInput),
      formatBounds(foldRange.uncachedInput),
      formatBounds(foldRange.output),
    );
  }
  return null;
}

/** 기본 화면의 1%당 토큰 값. 숫자를 못 내면 상태 문구와 설명. `breakdownDays`는 사용 패턴을 보는 기간(settings `tuning.breakdownDays`). */
export function codexTokensText(
  rate: CodexRate,
  breakdownDays: number,
): {
  value: string;
  isNumber: boolean;
  hint: string;
} {
  const { perPercent, pattern } = rate;
  const isEstimated = rate.status === "estimated" && perPercent;
  // 환율은 있는데 기간 안에 5시간 구간이 없거나 요청이 없으면 값을 곱할 보통 구간이 없다.
  if (isEstimated && (!pattern || pattern.requests.median <= 0)) {
    return {
      value: t.rate.noUse(breakdownDays),
      isNumber: false,
      hint: t.rate.noUseHint,
    };
  }
  if (isEstimated && pattern) {
    const tokens = tokensPerPercent(pattern.requests.median, pattern.misses.median, perPercent);
    if (tokens !== null) {
      return {
        value: t.common.tokens(`~${formatTokens(tokens)}`),
        isNumber: true,
        hint: t.rate.tokensHint,
      };
    }
  }
  // 범위만 믿을 수 있으면 fold 범위로 보통 구간의 1%당 토큰 범위를 보인다. 상한이 없으면 "≥하한".
  if (rate.status === "range_only" && rate.foldRange && pattern && pattern.requests.median > 0) {
    const { cachedInput, uncachedInput } = rate.foldRange;
    const { median: requests } = pattern.requests;
    const misses = pattern.misses.median;
    const low = tokensPerPercent(requests, misses, {
      cachedInput: cachedInput.min,
      uncachedInput: uncachedInput.min,
    });
    const high = tokensPerPercent(requests, misses, {
      cachedInput: cachedInput.max,
      uncachedInput: uncachedInput.max,
    });
    if (low !== null) {
      const range =
        high === null ? `≥${formatTokens(low)}` : `~${formatTokens(low)}~${formatTokens(high)}`;
      return { value: t.common.tokens(range), isNumber: true, hint: t.rate.rangeHint };
    }
  }
  if (rate.status === "no_data") {
    return { value: t.rate.collecting, isNumber: false, hint: t.rate.collectingHint };
  }
  if (rate.status === "insufficient") {
    return { value: t.rate.insufficient, isNumber: false, hint: t.rate.insufficientHint };
  }
  return {
    value: t.rate.unknown,
    isNumber: false,
    hint: t.rate.unknownHint,
  };
}

/** 캐시되지 않은 요청 %(중앙값끼리 나눈 근사). 요청이 0이면 null. */
export function uncachedPercent({ requests, misses }: WindowPattern): number | null {
  return requests.median > 0 ? (misses.median / requests.median) * 100 : null;
}

/** "claude-sonnet-5.5" → "sonnet-5.5". */
export function modelLabel(model: string): string {
  return model.replace(/^claude-/, "");
}

/** 질문 1번의 월 한도 몫("0.19%", 측정 ÷ 측정이라 `~` 없음). 월 한도를 모르면 null. */
export function kiroTurnText(model: KiroModelCredits, limit: number | null): string | null {
  const share = kiroTurnShare(model.creditsPerTurn, limit);
  return share === null ? null : `${formatShare(share)}%`;
}

/** Kiro 큰 값: 모델이 하나면 그 크레딧, 여럿이면 "모델마다 달라요"(값은 아래 표). 기록이 없으면 null. */
export function kiroHeadline(models: KiroModelCredits[]): string | null {
  if (models.length === 0) return null;
  return models.length > 1
    ? t.rate.perQuestionMixed
    : t.common.credits(credits.format(models[0].creditsPerTurn));
}

function CodexSections({
  rate,
  aside,
  breakdownDays,
}: {
  rate: CodexRate;
  aside: ReactNode;
  breakdownDays: number;
}) {
  const tokens = codexTokensText(rate, breakdownDays);
  const detail = codexDetailText(rate);
  const { pattern } = rate;
  const uncached = pattern && uncachedPercent(pattern);
  return (
    <>
      <InfoSection
        title={t.rate.perPercent}
        value={tokens.value}
        valueTitle={lines(tokens.isNumber && t.info.estimated, detail, sampleText(rate))}
        description={tokens.hint}
        aside={aside}
      />
      {pattern && (
        <InfoSection
          title={t.rate.pattern}
          value={t.rate.patternValue(count.format(pattern.requests.median))}
          valueTitle={lines(
            t.info.measured,
            t.rate.busy(count.format(pattern.requests.p90), count.format(pattern.misses.p90)),
            t.rate.typical(count.format(pattern.misses.median), pattern.windows),
          )}
          description={
            <>
              {uncached !== null && (
                <span title={t.info.estimated} className="tabular-nums">
                  {t.rate.uncached(percent.format(uncached))}
                </span>
              )}
              {t.rate.lastDays(pattern.days)}
            </>
          }
        />
      )}
    </>
  );
}

type KiroSectionProps = { models: KiroModelCredits[]; aside: ReactNode; limit: number | null };

function KiroSection({ models, aside, limit }: KiroSectionProps) {
  const headline = kiroHeadline(models);
  return (
    <InfoSection
      title={t.rate.perQuestion}
      value={headline ?? t.rate.collecting}
      valueTitle={headline ? t.info.measured : undefined}
      description={headline ? t.rate.perQuestionHint : t.rate.perQuestionEmpty}
      aside={aside}
    >
      {headline && (
        <table className="mt-1 w-full text-xs tabular-nums">
          <thead>
            <tr className="text-muted-foreground">
              <th scope="col" className="py-0.5 text-left font-normal">
                {t.rate.colModel}
              </th>
              <th scope="col" className="py-0.5 text-right font-normal">
                {t.rate.colCredits}
              </th>
              <th scope="col" className="py-0.5 pl-4 text-right font-normal">
                {t.rate.colShare}
              </th>
            </tr>
          </thead>
          <tbody>
            {[...models]
              .sort((a, b) => b.turns - a.turns)
              .map((m) => (
                <tr key={m.model} title={t.rate.turns(modelLabel(m.model), m.turns)}>
                  <td className="py-0.5 text-left">{modelLabel(m.model)}</td>
                  <td className="py-0.5 text-right font-medium">
                    {credits.format(m.creditsPerTurn)}
                  </td>
                  <td className="py-0.5 pl-4 text-right text-muted-foreground">
                    {kiroTurnText(m, limit) ?? "—"}
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      )}
    </InfoSection>
  );
}

/** 에이전트 하나의 1%당 토큰·사용 패턴. `kiroLimit`(Kiro 월 한도 크레딧)이 있으면 모델별 한 턴의 월 한도 몫을 더한다. 마지막 값은 메모리에 남고(`ratesStore`), Rust가 갱신하면 event로 바뀐다. */
export function RateLine({
  agent,
  breakdownDays,
  kiroLimit = null,
}: {
  agent: Agent;
  /** 사용 패턴을 보는 기간(일, settings `tuning.breakdownDays`). */
  breakdownDays: number;
  kiroLimit?: number | null;
}) {
  const { value: snapshot, error } = useSnapshot(ratesStore, agent);
  const title = hasFeature(agent, "creditLimit") ? t.rate.perQuestion : t.rate.perPercent;

  if (!snapshot) {
    return (
      <InfoSection title={title}>
        {error ? <StateText kind="error">{error}</StateText> : <StateText kind="loading" />}
      </InfoSection>
    );
  }
  const aside = <SnapshotAge computedAt={snapshot.computedAt} />;
  if (agent === "codex")
    return <CodexSections rate={snapshot.codex} aside={aside} breakdownDays={breakdownDays} />;
  if (hasFeature(agent, "creditLimit"))
    return <KiroSection models={snapshot.kiro} aside={aside} limit={kiroLimit} />;
  const claude = snapshot.claude;
  // Claude도 Codex와 같은 판정·화면(0006). 5시간 구간이 하나도 없을 때만 따로 안내한다.
  if (claude.windows > 0)
    return <CodexSections rate={claude} aside={aside} breakdownDays={breakdownDays} />;
  return (
    <InfoSection
      title={title}
      value={t.rate.claudeNone}
      description={t.rate.claudeNoneHint}
      aside={aside}
    />
  );
}
