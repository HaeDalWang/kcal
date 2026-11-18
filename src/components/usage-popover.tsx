import { ChevronRight, LayoutList, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";

import { AgentIcon } from "@/components/agent-icon";
import {
  AGENT_COLOR,
  startupShortValue,
  StartupSummary,
  startupTitle,
  UsageBars,
} from "@/components/agent-metrics";
import { Dial } from "@/components/dial";
import { reasonStateKind, StateText } from "@/components/state-text";
import { trimText } from "@/lib/check-copy";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  errorMessage,
  getUsage,
  onUsageUpdated,
  type Agent,
  type AgentContext,
  type AgentUsage,
  type LimitWindow,
  type UsageSnapshot,
} from "@/lib/ipc";
import { numberFormat, t } from "@/lib/i18n";
import { formatDuration, paceOf } from "@/lib/pace";
import { contextStore, useSnapshot } from "@/lib/snapshot-store";

export const AGENT_LABEL: Record<Agent, string> = {
  claude: "Claude",
  codex: "Codex",
  kiro: "Kiro",
};
const MINUTE_MS = 60_000;

const integer = numberFormat({ maximumFractionDigits: 0 });

function isResetPassed(window: LimitWindow, now: number): boolean {
  return window.resetPassed || (window.resetsAt !== null && Date.parse(window.resetsAt) <= now);
}

/** 표시용 남은 % (0~100) = 100 − 사용률. 리셋이 지난 창은 100%. */
export function remainingPercent(window: LimitWindow, now: number): number {
  if (isResetPassed(window, now)) return 100;
  return Math.min(100, Math.max(0, 100 - window.usedPercent));
}

/** 남은 %가 `lowPercent`(settings `tuning.lowPercent`) 이하면 경고색. */
export function isLow(remaining: number, lowPercent: number): boolean {
  return remaining <= lowPercent;
}

/** Kiro 크레딧: "3,341 / 5,000 크레딧 남음". 리셋이 지났으면 한도 전체가 남는다. */
function creditsText(window: LimitWindow, now: number): string | null {
  if (window.used === undefined || window.limit === undefined) return null;
  const left = isResetPassed(window, now) ? window.limit : Math.max(0, window.limit - window.used);
  return t.popover.creditsLeft(integer.format(left), integer.format(window.limit));
}

/** 초기화까지 남은 ms. 지났으면 0, 시각이 없으면 null. */
function msToReset(window: LimitWindow, now: number): number | null {
  if (!window.resetsAt) return null;
  const left = Date.parse(window.resetsAt) - now;
  return window.resetPassed || left <= 0 ? 0 : left;
}

/** "4h 35m", 지났으면 "초기화됨" (시각 없으면 null). */
function resetIn(window: LimitWindow, now: number): string | null {
  const left = msToReset(window, now);
  if (left === null) return null;
  return left === 0 ? t.popover.reset : formatDuration(left);
}

/** "3h 41m 후 초기화", 지났으면 "초기화됨". */
function resetText(window: LimitWindow, now: number): string | null {
  const left = msToReset(window, now);
  if (left === null) return null;
  return left === 0 ? t.popover.reset : t.popover.resetsIn(formatDuration(left));
}

/** 가장 급한 창(남은 %가 가장 적은 창)의 남은 %. 창이 없으면 null. Rust `AgentUsage::min_remaining`과 같다. */
export function minRemaining(usage: AgentUsage, now: number): number | null {
  if (usage.windows.length === 0) return null;
  return Math.min(...usage.windows.map((w) => remainingPercent(w, now)));
}

/** 탭의 가장 급한 창이 경고색 기준 이하인가. */
function isTabLow(usage: AgentUsage, now: number, lowPercent: number): boolean {
  const remaining = minRemaining(usage, now);
  return remaining !== null && isLow(remaining, lowPercent);
}

type BarProps = { window: LimitWindow; now: number; lowPercent: number };

type BarLineProps = {
  remaining: number;
  label: string;
  color: string;
  isLow: boolean;
  /** 막대 높이 클래스. */
  className: string;
  /** 균등 사용 기준 남은 %. 있으면 세로 표시선. */
  marker?: number;
};

/** 남은 %를 보여 주는 둥근 막대. 낮으면 destructive, 아니면 에이전트 색. */
export function Bar({ remaining, label, color, isLow, className, marker }: BarLineProps) {
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(remaining)}
      className={`relative w-full rounded-full bg-foreground/10 ${className}`}
    >
      <div
        className={`h-full rounded-full ${isLow ? "bg-destructive" : ""}`}
        style={{ width: `${remaining}%`, ...(isLow ? {} : { backgroundColor: color }) }}
      />
      {marker !== undefined && (
        <span
          aria-hidden
          title={t.pace.marker}
          className="absolute -inset-y-0.5 w-0.5 -translate-x-1/2 rounded-full bg-foreground/70"
          style={{ left: `${marker}%` }}
        />
      )}
    </div>
  );
}

function WindowBlock({ window, now, lowPercent, agent }: BarProps & { agent: Agent }) {
  const remaining = remainingPercent(window, now);
  const reset = resetText(window, now);
  const credits = creditsText(window, now);
  const pace = paceOf(window, remaining, now);
  const label = t.popover.window[window.id];
  return (
    <div className="flex flex-col gap-[6px]">
      <div className="flex items-baseline justify-between gap-[8px]">
        <span className="text-[13px] font-medium tabular-nums">
          {t.popover.left(label, Math.round(remaining))}
        </span>
        {reset && <span className="text-[10px] text-muted-foreground tabular-nums">{reset}</span>}
      </div>
      <Bar
        remaining={remaining}
        label={t.popover.remainingBar(label)}
        color={AGENT_COLOR[agent]}
        isLow={isLow(remaining, lowPercent)}
        className="h-[6px]"
        marker={pace?.expectedRemaining}
      />
      {pace && (
        <p className="line-clamp-2 text-[10px] text-muted-foreground tabular-nums">
          <span className={pace.isShort ? "text-warn" : ""}>{pace.deltaText}</span>
          {` · ${pace.outlookText}`}
        </p>
      )}
      {credits && <p className="text-[10px] text-muted-foreground tabular-nums">{credits}</p>}
    </div>
  );
}

function ago(ms: number): string {
  return ms < MINUTE_MS ? t.age.justNow : t.popover.ago(formatDuration(ms));
}

/** "5m 전 업데이트 · API"(`withLabel`이 false면 "5m 전 · API"). 값이 없으면 빈 문자열. */
function metaText(usage: AgentUsage, now: number, withLabel = true): string {
  const observed = usage.observedAt ? Date.parse(usage.observedAt) : null;
  const age = observed === null ? null : ago(now - observed);
  return [
    age !== null && withLabel ? t.popover.updated(age) : age,
    usage.source ? t.popover.source[usage.source] : null,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** 확인 중·값 없음·실패 이유. */
function StatusLines({ usage }: { usage: AgentUsage }) {
  const isPending = usage.checkedAt === null;
  const hasNoValue = usage.windows.length === 0;
  return (
    <>
      {isPending && <StateText kind="loading" />}
      {!isPending && hasNoValue && !usage.reason && <StateText kind="empty" />}
      {usage.reason &&
        (hasNoValue ? (
          <StateText kind={reasonStateKind(usage)}>{usage.reason}</StateText>
        ) : (
          <p className="text-[10px] text-muted-foreground">{usage.reason}</p>
        ))}
    </>
  );
}

type SectionProps = {
  usage: AgentUsage;
  now: number;
  lowPercent: number;
  /** 있으면 이름·갱신 시각·대시보드 chevron 머리말을 보인다(트레이). 대시보드는 자기 제목이 있어 생략. */
  onOpenDashboard?: () => void;
};

export function AgentSection({ usage, now, lowPercent, onOpenDashboard }: SectionProps) {
  const meta = metaText(usage, now);
  return (
    <section role="group" aria-label={AGENT_LABEL[usage.agent]} className="flex flex-col">
      {onOpenDashboard ? (
        <div className="mx-[20px] flex items-center justify-between gap-2 border-b pt-1.5 pb-1.5">
          <div className="flex min-w-0 flex-col gap-[4px]">
            <h2 className="text-[13px] font-bold">{AGENT_LABEL[usage.agent]}</h2>
            {meta && <p className="text-[10px] text-muted-foreground tabular-nums">{meta}</p>}
          </div>
          <button
            type="button"
            aria-label={t.popover.openDashboard}
            onClick={onOpenDashboard}
            className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none active:translate-y-px"
          >
            <ChevronRight className="size-5" aria-hidden />
          </button>
        </div>
      ) : (
        meta && <p className="px-[20px] text-[10px] text-muted-foreground tabular-nums">{meta}</p>
      )}
      <div className="flex flex-col gap-[12px] px-[20px] pt-2.5 pb-1.5">
        {usage.windows.map((window) => (
          <WindowBlock
            key={window.id}
            window={window}
            now={now}
            lowPercent={lowPercent}
            agent={usage.agent}
          />
        ))}
        <StatusLines usage={usage} />
      </div>
    </section>
  );
}

/** 개요의 보조 창 한 줄: "5-hour [막대] 60% left · 2h 40m". 이름 칸·값 칸 폭을 고정해 카드끼리 막대 양 끝이 맞는다.
 * 다이얼처럼 "남음"을 붙이고, 막대도 다이얼처럼 남은 만큼 찬다. */
function OverviewWindow({ window, now, lowPercent, agent }: BarProps & { agent: Agent }) {
  const remaining = remainingPercent(window, now);
  const reset = resetIn(window, now);
  const label = t.popover.windowShort[window.id];
  const left = t.popover.dial(Math.round(remaining));
  return (
    <div className="flex items-center gap-2 text-[10px] tabular-nums">
      <span className="min-w-10 shrink-0 font-medium whitespace-nowrap">{label}</span>
      <Bar
        remaining={remaining}
        label={t.popover.remainingBar(label)}
        color={AGENT_COLOR[agent]}
        isLow={isLow(remaining, lowPercent)}
        className="h-[6px] min-w-8 flex-1"
      />
      <span
        title={reset ? (resetText(window, now) ?? undefined) : undefined}
        className="min-w-[88px] shrink-0 text-right whitespace-nowrap text-muted-foreground"
      >
        {reset ? `${left} · ${reset}` : left}
      </span>
    </div>
  );
}

/** 개요 카드의 시작 컨텍스트 한 줄. 측정 항목이 없으면 숨긴다(1절). */
function StartupContextLine({ context }: { context: AgentContext | undefined }) {
  if (!context || context.items.length === 0) return null;
  return (
    <p
      className="text-[10px] text-muted-foreground tabular-nums"
      title={`${t.context.description}\n${startupTitle(context)}`}
    >
      <span className="font-medium text-foreground">{t.common.startupContext}</span>
      {` · ${startupShortValue(context)}`}
    </p>
  );
}

function OverviewCard({
  usage,
  context,
  now,
  lowPercent,
}: {
  usage: AgentUsage;
  context: AgentContext | undefined;
  now: number;
  lowPercent: number;
}) {
  const meta = metaText(usage, now, false);
  const color = AGENT_COLOR[usage.agent];
  // 가장 급한 창(남은 %가 가장 적은 창). 같으면 앞의 창.
  const urgent = usage.windows.reduce<LimitWindow | null>(
    (best, w) => (best && remainingPercent(best, now) <= remainingPercent(w, now) ? best : w),
    null,
  );
  const urgentRemaining = urgent && remainingPercent(urgent, now);
  const urgentReset = urgent && resetText(urgent, now);
  const isUrgentLow = urgentRemaining !== null && isLow(urgentRemaining, lowPercent);
  return (
    <section
      role="group"
      aria-label={AGENT_LABEL[usage.agent]}
      className="flex items-center gap-3 py-[10px]"
    >
      {/* 값을 모르면 다이얼을 숨기고 이름 줄만 둔다(1절 "—로 채우지 않는다"). */}
      {urgentRemaining !== null && (
        <Dial percent={urgentRemaining} color={color} isLow={isUrgentLow} />
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-[4px]">
        <div className="flex items-center gap-1.5">
          <span aria-hidden className="size-2 rounded-full" style={{ backgroundColor: color }} />
          <span className="text-[13px] font-bold">{AGENT_LABEL[usage.agent]}</span>
          {isUrgentLow && (
            <span className="text-[10px] font-medium text-destructive">{t.popover.low}</span>
          )}
          {meta && (
            <span className="ml-auto text-[10px] text-muted-foreground tabular-nums">{meta}</span>
          )}
        </div>
        {/* 다이얼이 어느 한도인지: 한도 이름만 진하게(시안 K). */}
        {urgent && (
          <p className="text-[10px] text-muted-foreground tabular-nums">
            <span className="font-medium text-foreground">
              {t.popover.limit(t.popover.windowShort[urgent.id])}
            </span>
            {urgentReset && ` · ${urgentReset}`}
          </p>
        )}
        <StartupContextLine context={context} />
        {usage.windows
          .filter((w) => w !== urgent)
          .map((window) => (
            <OverviewWindow
              key={window.id}
              window={window}
              now={now}
              lowPercent={lowPercent}
              agent={usage.agent}
            />
          ))}
        <StatusLines usage={usage} />
      </div>
    </section>
  );
}

function Overview({
  usages,
  now,
  lowPercent,
}: {
  usages: AgentUsage[];
  now: number;
  lowPercent: number;
}) {
  // 개요에 들어올 때 한 번 부른다(Rust가 캐시를 바로 준다). 갱신은 `context-updated` event.
  const { value: contexts } = useSnapshot(contextStore, "overview");
  return (
    <div className="flex flex-col divide-y px-[20px]">
      {usages.map((usage) => (
        <OverviewCard
          key={usage.agent}
          usage={usage}
          context={contexts?.agents.find((a) => a.agent === usage.agent)}
          now={now}
          lowPercent={lowPercent}
        />
      ))}
    </div>
  );
}

/** 탭 이름 아래 얇은 막대. `remaining`이 null이면 빈 트랙. */
function MiniBar({
  remaining,
  lowPercent,
  color,
}: {
  remaining: number | null;
  lowPercent: number;
  color: string;
}) {
  return (
    <span aria-hidden className="block h-[3px] w-full overflow-hidden rounded-full bg-muted">
      {remaining !== null && (
        <span
          className={isLow(remaining, lowPercent) ? "block h-full bg-destructive" : "block h-full"}
          style={{
            width: `${remaining}%`,
            ...(isLow(remaining, lowPercent) ? {} : { backgroundColor: color }),
          }}
        />
      )}
    </span>
  );
}

/** 위쪽 탭: 개요 + 설정에서 켠 에이전트. */
export type PopoverTab = "overview" | Agent;

/** 고른 탭은 개요·에이전트 모두 같은 회색 칸(메뉴 hover와 같은 accent). 에이전트 색은 아래 사용량 막대에만 쓴다. */
const TRIGGER =
  "h-auto flex-col gap-1 px-1.5 py-1 text-[11px] hover:bg-accent/50 group-data-[variant=default]/tabs-list:data-active:shadow-none data-active:bg-accent data-active:text-accent-foreground dark:data-active:border-transparent dark:data-active:bg-accent";

type UsagePopoverProps = {
  /** 탭·개요에 보여 줄 에이전트(설정에서 켠 것). */
  agents: Agent[];
  selected: PopoverTab;
  onSelect: (tab: PopoverTab) => void;
  /** 남은 %가 이 값 이하면 경고색(settings `tuning.lowPercent`). */
  lowPercent: number;
  /** 에이전트 탭 머리말의 chevron. 대시보드 창을 연다. */
  onOpenDashboard: () => void;
  /** 선택한 에이전트에서 줄일 수 있는 것(진단하기 제안). 없으면 null이고 입구 줄을 숨긴다. */
  trim?: { count: number; tokens: number } | null;
  /** 입구 줄을 눌렀다: 진단하기가 있는 대시보드 점검 탭으로 간다. */
  onReview?: () => void;
};

/** 에이전트 탭의 입구 한 줄: 줄일 수 있는 것이 있을 때만 보이고, 누르면 진단하기로 간다. */
function TrimEntry({
  trim,
  onReview,
}: {
  trim: { count: number; tokens: number };
  onReview: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onReview}
      className="mx-[20px] flex items-center gap-2 rounded-md bg-muted px-2 py-1.5 text-left text-[10px] tabular-nums hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none active:translate-y-px"
    >
      <span className="min-w-0 flex-1 text-balance">{trimText(trim.count, trim.tokens)}</span>
      <span className="flex shrink-0 items-center text-[11px] font-medium">
        {t.popover.trimReview}
        <ChevronRight className="size-[12px]" aria-hidden />
      </span>
    </button>
  );
}

/** 한도 스냅샷. `get_usage`로 한 번 읽고 `usage-updated` event로 갱신한다.
 * `now`(남은 시간 기준)는 수신·포커스 때만 다시 잡는다(타이머 없음). */
export function useUsageSnapshot(): {
  snapshot: UsageSnapshot | null;
  error: string | null;
  now: number;
} {
  const [snapshot, setSnapshot] = useState<UsageSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    let isCancelled = false;
    let unlisten: (() => void) | undefined;
    const apply = (next: UsageSnapshot) => {
      setSnapshot(next);
      setError(null);
      setNow(Date.now());
    };
    onUsageUpdated(apply).then(
      (stop) => (isCancelled ? stop() : (unlisten = stop)),
      (reason: unknown) => setError(errorMessage(reason)),
    );
    getUsage().then(
      (next) => !isCancelled && apply(next),
      (reason: unknown) => !isCancelled && setError(errorMessage(reason)),
    );
    // 팝오버가 다시 보일 때 남은 시간을 새로 계산한다(타이머 없이).
    const onFocus = () => setNow(Date.now());
    window.addEventListener("focus", onFocus);
    return () => {
      isCancelled = true;
      unlisten?.();
      window.removeEventListener("focus", onFocus);
    };
  }, []);

  return { snapshot, error, now };
}

/** 개요(켠 에이전트 요약) 또는 에이전트 하나의 결론(사용량 → 최근 구간 막대 → 시작 컨텍스트 한 줄, docs/decisions/0010·0023). */
export function UsagePopover({
  agents,
  selected,
  onSelect,
  lowPercent,
  onOpenDashboard,
  trim = null,
  onReview,
}: UsagePopoverProps) {
  const { snapshot, error, now } = useUsageSnapshot();

  if (error) {
    return (
      <StateText kind="error" className="p-3">
        {error}
      </StateText>
    );
  }
  if (!snapshot) return <StateText kind="loading" className="p-3" />;
  const usages = agents.flatMap((agent) => snapshot.agents.filter((a) => a.agent === agent));
  const usage = usages.find((a) => a.agent === selected);
  return (
    <Tabs value={selected} onValueChange={(value: PopoverTab) => onSelect(value)} className="gap-1">
      <div className="px-2 pt-2">
        <TabsList className="w-full gap-0.5 bg-transparent p-0 group-data-horizontal/tabs:h-auto">
          <TabsTrigger value="overview" className={TRIGGER}>
            <span className="flex flex-col items-center gap-px">
              <LayoutList className="size-[16px]" aria-hidden />
              {t.popover.overview}
            </span>
            <span aria-hidden className="block h-[3px]" />
          </TabsTrigger>
          {usages.map((tab) => (
            <TabsTrigger key={tab.agent} value={tab.agent} className={TRIGGER}>
              <span className="flex flex-col items-center gap-px">
                <AgentIcon agent={tab.agent} className="size-[16px]" />
                <span className="flex items-center gap-0.5">
                  {AGENT_LABEL[tab.agent]}
                  {/* 낮음을 색만으로 알리지 않는다(6절 3). */}
                  {isTabLow(tab, now, lowPercent) && (
                    <TriangleAlert aria-hidden className="size-[10px] text-destructive" />
                  )}
                </span>
              </span>
              <MiniBar
                remaining={minRemaining(tab, now)}
                lowPercent={lowPercent}
                color={AGENT_COLOR[tab.agent]}
              />
            </TabsTrigger>
          ))}
        </TabsList>
      </div>
      <TabsContent value={selected}>
        {selected === "overview" ? (
          <Overview usages={usages} now={now} lowPercent={lowPercent} />
        ) : (
          usage && (
            <div className="flex flex-col gap-3 pb-1.5">
              <AgentSection
                usage={usage}
                now={now}
                lowPercent={lowPercent}
                onOpenDashboard={onOpenDashboard}
              />
              {trim && onReview && <TrimEntry trim={trim} onReview={onReview} />}
              <UsageBars agent={usage.agent} />
              <StartupSummary agent={usage.agent} />
            </div>
          )
        )}
      </TabsContent>
    </Tabs>
  );
}
