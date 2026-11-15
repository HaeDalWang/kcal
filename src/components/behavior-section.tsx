// 행동 패턴(docs/decisions/0013): Codex 5시간 한도가 보통 무엇으로 찼나·많이 쓴 세션. 대시보드에만 둔다(0019, design.md 1절).
// Kiro 질문 1번의 월 한도 몫은 대시보드 `RateLine`의 "질문당 크레딧" 자세히에 있다.
// 값은 저장소(`ratesStore`·`contextStore`)를 읽기만 한다. 조회는 옆의 `MetricLines`·`RateLine`이 한다.
// 값이 없으면 섹션을 숨긴다(빈 자리·0 없음).
import { useSyncExternalStore } from "react";

import { AGENT_COLOR } from "@/components/agent-metrics";
import { InfoSection } from "@/components/info-section";
import { formatShare, formatTokens } from "@/components/rate-line";
import { compositionOf, type Composition } from "@/lib/breakdown";
import type { CodexRate, CodexSession } from "@/lib/ipc";
import { contextStore, ratesStore } from "@/lib/snapshot-store";
import { numberFormat, t } from "@/lib/i18n";

const integer = numberFormat({ maximumFractionDigits: 0 });
/** 구성 막대 조각의 에이전트 색 비율(%). 나머지는 글자색과 섞는다(라이트는 진하게, 다크는 밝게). 트랙 대비 3:1 이상(0019). */
const PART_STRENGTH = { startup: 100, conversation: 70, output: 45 } as const;
const FULL_PERCENT = 100;

type PartKey = keyof typeof PART_STRENGTH;
export type Part = { key: PartKey; label: string; value: number };

/** 막대·범례 조각. 0 이하 조각은 뺀다. 시작 토큰을 모르면 입력 몫 전체를 "입력" 한 조각으로. */
export function compositionParts(c: Composition): Part[] {
  const parts: Part[] = [
    ...(c.startup === null
      ? []
      : [{ key: "startup" as const, label: t.behavior.parts.startup, value: c.startup }]),
    {
      key: "conversation",
      label: c.startup === null ? t.behavior.parts.input : t.behavior.parts.conversation,
      value: c.conversation,
    },
    { key: "output", label: t.behavior.parts.output, value: c.output },
  ];
  return parts.filter((p) => p.value > 0);
}

/** "설정 ~2.1% · 대화 누적 ~25% · 답변 ~38% (예상)". */
export function legendText(parts: Part[]): string {
  return t.behavior.legend(parts.map((p) => `${p.label} ~${formatShare(p.value)}%`).join(" · "));
}

function partColor(key: PartKey): string {
  return `color-mix(in oklab, ${AGENT_COLOR.codex} ${PART_STRENGTH[key]}%, var(--foreground))`;
}

/** 5시간 한도(100%) 위에 조각을 쌓은 막대. 합이 100을 넘으면 합 기준으로 줄인다. */
function CompositionBar({ parts }: { parts: Part[] }) {
  const scale = Math.max(
    FULL_PERCENT,
    parts.reduce((sum, p) => sum + p.value, 0),
  );
  return (
    <div
      role="img"
      aria-label={legendText(parts)}
      className="flex h-1.5 w-full overflow-hidden rounded-full bg-foreground/10"
    >
      {parts.map((p) => (
        <span
          key={p.key}
          className="h-full"
          style={{ width: `${(p.value / scale) * 100}%`, backgroundColor: partColor(p.key) }}
        />
      ))}
    </div>
  );
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

/** "10/30"(지역 날짜). */
function dayText(at: string): string {
  const date = new Date(at);
  return `${pad(date.getMonth() + 1)}/${pad(date.getDate())}`;
}

/** 자세한 값: "요청당 입력 126K 토큰 · 캐시 미스 17번 · 10/30 시작". */
export function sessionDetail(s: CodexSession): string {
  return t.behavior.sessionDetail(formatTokens(s.avgInputTokens), s.misses, dayText(s.startedAt));
}

/** 패턴 배지. 해당 없으면 빈 목록. */
export function sessionBadges(s: CodexSession): string[] {
  return [
    ...(s.isMarathon ? [t.behavior.marathon] : []),
    ...(s.isLargeContext ? [t.behavior.largeContext] : []),
  ];
}

/** "14:05"(지역 시각). */
function timeText(at: string): string {
  const date = new Date(at);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** 경로의 마지막 두 폴더 중 앞쪽(상위 폴더 이름). `/`와 `\`를 구분자로 본다. 없으면 null. */
function parentFolder(cwd: string): string | null {
  const parts = cwd.split(/[/\\]/).filter(Boolean);
  return parts.length >= 2 ? parts[parts.length - 2] : null;
}

/**
 * 세션 줄의 이름과 날짜. 이름이 겹칠 때만 구분한다: 경로가 다르면 `parent/name`으로,
 * 그래도 같은 줄이 있으면(같은 폴더·경로를 모를 때) 날짜 옆에 시작 시각(HH:mm)을 붙인다.
 */
export function sessionLabels(sessions: CodexSession[]): { name: string; when: string }[] {
  const named = sessions.map((s) => {
    const others = sessions.filter((o) => o !== s && o.name === s.name);
    const hasOtherPath = others.some((o) => o.cwd !== s.cwd);
    const parent = hasOtherPath && s.cwd ? parentFolder(s.cwd) : null;
    return parent ? `${parent}/${s.name}` : s.name;
  });
  return sessions.map((s, i) => {
    const when = dayText(s.startedAt);
    const isDuplicate = named.some((name, j) => j !== i && name === named[i]);
    return { name: named[i], when: isDuplicate ? `${when} ${timeText(s.startedAt)}` : when };
  });
}

function SessionRows({ sessions }: { sessions: CodexSession[] }) {
  const labels = sessionLabels(sessions);
  return (
    <ul aria-label={t.behavior.sessions} className="flex flex-col gap-1">
      {sessions.map((s, i) => (
        <li key={`${s.cwd ?? s.name}-${s.startedAt}`} className="flex flex-col">
          <div className="flex items-center gap-1.5 text-[10px] tabular-nums">
            <span className="min-w-0 truncate text-[11px] font-medium">{labels[i].name}</span>
            <span className="shrink-0 text-muted-foreground">{labels[i].when}</span>
            {sessionBadges(s).map((badge) => (
              <span
                key={badge}
                className="shrink-0 rounded bg-muted px-1 text-[10px] text-muted-foreground"
              >
                {badge}
              </span>
            ))}
            <span className="ml-auto shrink-0 text-muted-foreground" title={t.info.measured}>
              {t.behavior.requests(integer.format(s.requests))}
            </span>
            <span className="w-11 shrink-0 text-right font-medium" title={t.info.estimated}>
              {`~${formatShare(s.share)}%`}
            </span>
          </div>
          <span className="text-[10px] text-muted-foreground tabular-nums">{sessionDetail(s)}</span>
        </li>
      ))}
    </ul>
  );
}

/** Codex 환율과 시작 토큰(항목을 알 때만). */
function useCodexInputs(): { rate: CodexRate | null; startupTokens: number | null } {
  const { value: rates } = useSyncExternalStore(ratesStore.subscribe, ratesStore.getSnapshot);
  const { value: contexts } = useSyncExternalStore(
    contextStore.subscribe,
    contextStore.getSnapshot,
  );
  const context = contexts?.agents.find((a) => a.agent === "codex");
  const startupTokens = context && context.items.length > 0 ? context.totalStartupTokens : null;
  return { rate: rates?.codex ?? null, startupTokens };
}

/** 세션 목록 머리 오른쪽: "최근 14일 · 5시간 한도 % 합". */
function sessionsCaption(rate: CodexRate): string {
  return t.behavior.caption(rate.pattern?.days ?? null);
}

/** 검증 한 줄: "실제 68% · 예상 65%". */
export function validationText(c: Composition): string {
  return t.behavior.validation(formatShare(c.used), formatShare(c.predicted));
}

/** 대시보드 Codex: 구성과 검증 한 줄, 세션마다 자세한 값까지 펼쳐 보인다. */
export function CodexBehaviorDetails() {
  const { rate, startupTokens } = useCodexInputs();
  if (!rate) return null;
  const composition = compositionOf(rate, startupTokens);
  const parts = composition ? compositionParts(composition) : [];
  const window = rate.composition;
  return (
    <>
      {composition && window && parts.length > 0 && (
        <InfoSection
          title={t.behavior.composition}
          value={parts.map((p) => `${p.label} ~${formatShare(p.value)}%`).join(" · ")}
          valueTitle={t.info.estimated}
          description={t.behavior.compositionHint}
          details={
            <span>
              {t.behavior.compositionDetail(
                validationText(composition),
                window.requests,
                window.misses,
                window.windows,
              )}
            </span>
          }
        >
          <CompositionBar parts={parts} />
        </InfoSection>
      )}
      {rate.sessions.length > 0 && (
        <InfoSection
          title={t.behavior.sessions}
          description={t.behavior.sessionsHint(sessionsCaption(rate))}
        >
          <SessionRows sessions={rate.sessions} />
        </InfoSection>
      )}
    </>
  );
}
