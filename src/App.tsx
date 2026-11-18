import { LayoutDashboard, Power, RefreshCw, Settings, Stethoscope } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

import { SettingsView } from "@/components/settings-view";
import { StateText } from "@/components/state-text";
import { UsagePopover, type PopoverTab } from "@/components/usage-popover";
import { alertCount, alertLabel, trimSummary } from "@/lib/check-copy";
import { t } from "@/lib/i18n";
import {
  errorMessage,
  fitPopoverHeight,
  getChecks,
  getSwitches,
  openDashboard,
  quitApp,
  refreshAll,
  type CoachCheck,
  type CoachSwitch,
} from "@/lib/ipc";
import { Separator } from "@/components/ui/separator";
import { useSettings } from "@/lib/use-settings";

const IS_MAC = navigator.userAgent.includes("Mac");
const hint = (key: string) => (IS_MAC ? `⌘${key}` : `Ctrl+${key}`);
const MENU_ITEM =
  "flex w-full items-center gap-2 rounded-md px-2 h-[24px] text-[14px] hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none active:translate-y-px disabled:pointer-events-none disabled:opacity-50";

/** 아이콘 + 라벨 + 라벨 옆 표시 + 오른쪽 단축키. */
function MenuLabel({
  icon,
  label,
  badge,
  shortcut,
}: {
  icon: ReactNode;
  label: string;
  badge?: ReactNode;
  shortcut?: string;
}) {
  return (
    <>
      <span aria-hidden className="text-muted-foreground [&>svg]:size-[16px]">
        {icon}
      </span>
      <span>{label}</span>
      {badge}
      {shortcut && <span className="ml-auto text-[14px] text-muted-foreground">{shortcut}</span>}
    </>
  );
}

/** "점검" 줄 오른쪽(다른 줄의 단축키 자리) 빨간 점 + 개수. */
function AlertBadge({ count }: { count: number }) {
  return (
    <span
      role="img"
      aria-label={alertLabel(count)}
      className="ml-auto flex items-center gap-1 text-[12px] font-medium text-destructive tabular-nums"
    >
      <span aria-hidden className="size-1.5 rounded-full bg-destructive" />
      <span aria-hidden>{count}</span>
    </span>
  );
}

/** 처음 한 번과 팝오버가 포커스를 받을 때(열릴 때) 다시 읽는다(Rust 캐시라 가볍다, 타이머 없음). 못 읽으면 null. */
function useOnFocus<T>(read: () => Promise<T>, label: string): T | null {
  const [value, setValue] = useState<T | null>(null);
  useEffect(() => {
    let isCancelled = false;
    const load = () => {
      read().then(
        (next) => !isCancelled && setValue(next),
        // 알림 점·입구 줄만 못 붙인다. 화면 흐름은 막지 않고 원인은 개발 콘솔에 남긴다.
        (reason: unknown) => console.error(`${label}:`, errorMessage(reason)),
      );
    };
    load();
    window.addEventListener("focus", load);
    return () => {
      isCancelled = true;
      window.removeEventListener("focus", load);
    };
  }, [read, label]);
  return value;
}

type View = "usage" | "settings";

export function App() {
  const ref = useRef<HTMLDivElement>(null);
  const { settings, setSettings, error: settingsError } = useSettings();
  const [actionError, setActionError] = useState<string | null>(null);
  const [view, setView] = useState<View>("usage");
  // 마지막으로 보던 탭. 메모리에만 둔다(팝오버를 닫았다 열어도 유지).
  const [selected, setSelected] = useState<PopoverTab>("overview");
  const checks = useOnFocus<CoachCheck[]>(getChecks, "get_checks");
  const switches = useOnFocus<CoachSwitch[]>(getSwitches, "get_switches");
  // 꺼 둔 에이전트 탭을 보던 중이면 개요로 보인다.
  const tab: PopoverTab =
    settings && selected !== "overview" && settings.enabledAgents.includes(selected)
      ? selected
      : "overview";
  // 개요는 켠 에이전트 전체, 에이전트 탭은 그 에이전트 카드만 센다(0017 "알림 개수").
  const counted = tab === "overview" ? (settings?.enabledAgents ?? []) : [tab];
  const alerts = checks ? alertCount(checks, counted) : 0;
  const hasAlert = settings !== null && alerts >= settings.tuning.checkAlertMin;

  // 내용 높이가 바뀔 때만(ResizeObserver 이벤트) 창 높이를 맞춘다.
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    let last = 0;
    const observer = new ResizeObserver(() => {
      const height = Math.ceil(element.getBoundingClientRect().height);
      if (height === last) return;
      last = height;
      fitPopoverHeight(height).catch(() => {
        last = 0;
      });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const run = (action: () => Promise<void>) => {
    setActionError(null);
    action().catch((reason: unknown) => setActionError(errorMessage(reason)));
  };

  // 단축키. 설정 화면에서는 대시보드·새로고침·설정 단축키를 무시한다.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(IS_MAC ? event.metaKey : event.ctrlKey)) return;
      const key = event.key.toLowerCase();
      if (key === "q") {
        event.preventDefault();
        void quitApp();
      } else if (view === "usage" && key === "d") {
        event.preventDefault();
        run(openDashboard);
      } else if (view === "usage" && key === "r") {
        event.preventDefault();
        run(refreshAll);
      } else if (view === "usage" && key === ",") {
        event.preventDefault();
        setView("settings");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  return (
    // break-keep: 한국어를 어절 단위로만 줄바꿈한다(영어에는 영향 없음).
    <div ref={ref} className="break-keep select-none">
      {settingsError && (
        <StateText kind="error" className="p-3">
          {settingsError}
        </StateText>
      )}
      {!settingsError && !settings && <StateText kind="loading" className="p-3" />}
      {settings && view === "settings" && (
        <SettingsView settings={settings} onSaved={setSettings} onBack={() => setView("usage")} />
      )}
      {settings && view === "usage" && (
        <UsagePopover
          agents={settings.enabledAgents}
          selected={tab}
          onSelect={setSelected}
          lowPercent={settings.tuning.lowPercent}
          onOpenDashboard={() => run(openDashboard)}
          trim={tab === "overview" || !switches ? null : trimSummary(switches, tab)}
          onReview={() => run(() => openDashboard("checks"))}
        />
      )}
      {actionError && (
        <p role="alert" className="px-[20px] pb-1 text-[10px] text-destructive">
          {actionError}
        </p>
      )}
      <nav aria-label={t.app.menu} className="border-t p-1.5">
        {view === "usage" && (
          <>
            <button type="button" onClick={() => run(openDashboard)} className={MENU_ITEM}>
              <MenuLabel icon={<LayoutDashboard />} label={t.app.dashboard} shortcut={hint("D")} />
            </button>
            {/* 최적화·진단하기는 대시보드 점검 탭(docs/decisions/0017 "화면 자리"). 여기는 알림만. */}
            <button
              type="button"
              onClick={() => run(() => openDashboard("checks"))}
              className={MENU_ITEM}
            >
              <MenuLabel
                icon={<Stethoscope />}
                label={t.app.checks}
                badge={hasAlert ? <AlertBadge count={alerts} /> : undefined}
              />
            </button>
            <Separator className="my-[6px]" />
            <button type="button" onClick={() => run(refreshAll)} className={MENU_ITEM}>
              <MenuLabel icon={<RefreshCw />} label={t.app.refresh} shortcut={hint("R")} />
            </button>
            <button type="button" onClick={() => setView("settings")} className={MENU_ITEM}>
              <MenuLabel icon={<Settings />} label={t.app.settings} shortcut={hint(",")} />
            </button>
          </>
        )}
        <button type="button" onClick={() => void quitApp()} className={MENU_ITEM}>
          <MenuLabel icon={<Power />} label={t.app.quit} shortcut={hint("Q")} />
        </button>
      </nav>
    </div>
  );
}
