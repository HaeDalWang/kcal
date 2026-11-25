// Browser preview used to record the README GIF: a macOS-like backdrop, the 310px popover under a
// menu bar strip, and the dashboard window that "Dashboard" opens. Everything shown comes from
// demo-fixtures.ts (demo data, not measurements); nothing is read from the machine.
// Open: `pnpm exec vite --port 1480 --strictPort`, then http://127.0.0.1:1480/src/test/preview/demo.html
// `?zoom=1.2` scales the popover (the GIF is shown smaller than 1:1 in the README).
// `?scene=diagnose` is the launch video: the Checks tab's Diagnose switches answer from demo-diagnose.ts
// (turn a plugin off, review the line and backup, apply, undo), and clicks show a short ring.
// Not part of the app bundle: index.html does not reference it, like popover.html and dashboard.html.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import { App } from "@/App";
import { Dashboard } from "@/components/dashboard";
import { followSystemTheme } from "@/lib/theme";
import { diagnoseBackend } from "@/test/preview/demo-diagnose";
import { demoFixtures } from "@/test/preview/demo-fixtures";
import { tourBackend, tourFixtures } from "@/test/preview/demo-tour";
import "@/index.css";

const OPEN_DASHBOARD = "demo:open-dashboard";
const params = new URLSearchParams(location.search);
const zoom = Number(params.get("zoom") ?? "1");
const SCENE_H = 860;
const isTourScene = params.get("scene") === "tour";
// The tour reuses the Diagnose backend (apply and Undo) and adds Kiro, sessions and Checks data.
const isDiagnoseScene = params.get("scene") === "diagnose" || isTourScene;
const diagnose = isDiagnoseScene ? diagnoseBackend() : null;
const tour = isTourScene ? tourBackend() : null;
const now = Date.now();
const data = isTourScene ? tourFixtures(now) : demoFixtures(now);

mockWindows("popover", "dashboard");
mockIPC(
  (cmd, args) => {
    const handled = tour?.(cmd, args) ?? diagnose?.(cmd, args);
    if (handled !== undefined) return handled;
    switch (cmd) {
      case "get_settings":
        return data.settings;
      case "get_usage":
        return data.usage;
      case "get_context":
        return data.context;
      case "get_rates":
        return data.rates;
      case "get_metrics":
        return data.metrics;
      case "get_health":
        return data.health;
      case "get_checks":
        return [];
      case "open_dashboard":
        // The Dashboard reads its first tab from `?tab=`; the real app sends it as an event.
        if (diagnose && (args as { tab?: string } | undefined)?.tab === "checks") {
          const url = new URL(location.href);
          url.searchParams.set("tab", "checks");
          history.replaceState(null, "", url);
        }
        window.dispatchEvent(new Event(OPEN_DASHBOARD));
        return null;
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

const CLOCK = new Intl.DateTimeFormat("en-US", {
  weekday: "short",
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
}).format(now);

/** The kcal flame from docs/assets/tray.svg, in the menu bar's white. */
function TrayIcon() {
  return (
    <svg viewBox="62 64 388 388" className="size-[15px]" aria-hidden>
      <defs>
        <mask id="cut">
          <rect x="0" y="0" width="512" height="512" fill="#fff" />
          <polyline
            points="96,334 196,334 222,286 254,386 282,300 300,334 416,334"
            fill="none"
            stroke="#000"
            strokeWidth="30"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </mask>
      </defs>
      <path
        mask="url(#cut)"
        fill="#fff"
        d="M256 70 C 300 146, 388 196, 388 314 C 388 392, 328 446, 256 446 C 184 446, 124 392, 124 314 C 124 244, 168 204, 196 154 C 204 204, 224 230, 248 240 C 240 184, 236 126, 256 70 Z"
      />
    </svg>
  );
}

function MenuBar({ isTrayActive }: { isTrayActive: boolean }) {
  return (
    <div className="absolute inset-x-0 top-0 flex h-[28px] items-center justify-end gap-4 bg-black/30 px-4 text-[13px] text-white/90">
      <span
        className={`flex h-[22px] items-center rounded-md px-2 ${isTrayActive ? "bg-white/25" : ""}`}
      >
        <TrayIcon />
      </span>
      <span className="tabular-nums">{CLOCK}</span>
    </div>
  );
}

function Scene() {
  const [isDashboardOpen, setIsDashboardOpen] = useState(false);
  useEffect(() => {
    const open = () => setIsDashboardOpen(true);
    window.addEventListener(OPEN_DASHBOARD, open);
    return () => window.removeEventListener(OPEN_DASHBOARD, open);
  }, []);
  return (
    <div
      className="relative overflow-hidden"
      style={{
        width: 1000,
        height: SCENE_H,
        background: "linear-gradient(160deg, #2b3550 0%, #1c2438 55%, #141a2b 100%)",
      }}
    >
      <MenuBar isTrayActive={!isDashboardOpen} />
      {!isDashboardOpen && (
        <div
          className="absolute top-[34px] right-[12px] w-[310px] overflow-hidden rounded-[12px] bg-background text-foreground shadow-2xl ring-1 ring-white/15"
          // The real window caps its height (the tray scrolls inside it); the tour does the same so a long
          // Claude tab stays inside the scene. `data-demo="popover"` is the scroll box the recorder moves.
          data-demo="popover"
          style={{
            zoom,
            ...(isTourScene && {
              maxHeight: `${(SCENE_H - 48) / zoom}px`,
              overflowY: "auto",
              scrollbarWidth: "none",
            }),
          }}
        >
          <App />
        </div>
      )}
      {isDashboardOpen && (
        <div
          data-demo="dashboard"
          style={{ width: 900, height: 780 }}
          className="demo-window-in absolute top-[44px] left-[50px] flex flex-col overflow-hidden rounded-[12px] bg-background shadow-2xl ring-1 ring-white/15"
        >
          <div className="flex h-[28px] shrink-0 items-center gap-2 border-b bg-card px-3">
            <span className="size-3 rounded-full bg-[#ff5f57]" />
            <span className="size-3 rounded-full bg-[#febc2e]" />
            <span className="size-3 rounded-full bg-[#28c840]" />
          </div>
          <div className="min-h-0 flex-1">
            <Dashboard />
          </div>
        </div>
      )}
    </div>
  );
}

/** A macOS-style arrow that glides to a point before a click. Preview-only DOM; page pointer events ignore it.
 * The recording script calls `window.demoCursor.to(x, y)` (CSS transition), waits, then clicks. */
function installCursor() {
  const arrow = document.createElement("div");
  arrow.id = "demo-cursor";
  arrow.innerHTML =
    '<svg width="22" height="30" viewBox="0 0 22 30"><path d="M2 1.5v22.2l5.6-5.2 3.6 8.2 3.6-1.6-3.6-8h8z" fill="#000" stroke="#fff" stroke-width="1.8" stroke-linejoin="round"/></svg>';
  document.body.append(arrow);
  const move = (x: number, y: number) => {
    arrow.style.transform = `translate(${x}px, ${y}px)`;
  };
  // The confirm dialog portals to <body>; centre it (and its dimmer) on the popover, not the whole scene.
  const placement = document.createElement("style");
  placement.textContent =
    // Settings' own scroll area is sized for the 800px tray window; the tour's scroll box is shorter.
    `form>div.overflow-y-auto{max-height:560px!important}` +
    `[role="dialog"]{left:calc(var(--pop-x) + var(--pop-w) / 2)!important;top:calc(var(--pop-y) + var(--pop-h) / 2)!important}` +
    `.fixed.inset-0{inset:var(--pop-y) auto auto var(--pop-x)!important;width:var(--pop-w);height:var(--pop-h);border-radius:12px}`;
  document.head.append(placement);
  const syncPopoverBox = () => {
    const box = document.querySelector('[data-demo="popover"]')?.getBoundingClientRect();
    if (!box) return;
    const root = document.documentElement.style;
    root.setProperty("--pop-x", `${box.x}px`);
    root.setProperty("--pop-y", `${box.y}px`);
    root.setProperty("--pop-w", `${box.width}px`);
    root.setProperty("--pop-h", `${box.height}px`);
  };
  syncPopoverBox();
  new MutationObserver(syncPopoverBox).observe(document.body, { childList: true, subtree: true });
  move(560, 430);
  Object.assign(window, { demoCursor: { to: move } });
  document.addEventListener("pointerdown", () => arrow.classList.add("is-down"), true);
  document.addEventListener("pointerup", () => arrow.classList.remove("is-down"), true);
}

followSystemTheme();
const style = document.createElement("style");
style.textContent =
  "@keyframes demo-window-in{from{opacity:0;transform:scale(.98)}to{opacity:1;transform:none}}" +
  ".demo-window-in{animation:demo-window-in .18s ease-out}" +
  "html,body{background:#141a2b!important}" +
  "@keyframes demo-ring{from{opacity:.9;transform:translate(-50%,-50%) scale(.4)}to{opacity:0;transform:translate(-50%,-50%) scale(1.6)}}" +
  "#demo-cursor{position:fixed;left:0;top:0;z-index:10000;pointer-events:none;transition:transform .45s cubic-bezier(.45,.05,.25,1);filter:drop-shadow(0 2px 3px rgba(0,0,0,.45))}" +
  "#demo-cursor svg{display:block;transform-origin:2px 2px;transition:transform .08s}" +
  "#demo-cursor.is-down svg{transform:scale(.85)}" +
  ".demo-ring{position:fixed;z-index:9999;width:36px;height:36px;border-radius:9999px;border:3px solid #fff;pointer-events:none;animation:demo-ring .7s ease-out forwards}";
document.head.append(style);
if (isDiagnoseScene) {
  // A short ring where a click lands, so a viewer can follow the clicks (no cursor in the recording).
  document.addEventListener(
    "click",
    (e) => {
      const ring = document.createElement("div");
      ring.className = "demo-ring";
      ring.style.left = `${e.clientX}px`;
      ring.style.top = `${e.clientY}px`;
      document.body.append(ring);
      setTimeout(() => ring.remove(), 800);
    },
    true,
  );
}
if (isTourScene) installCursor();
const root = document.getElementById("root");
if (!root) throw new Error("#root element is missing.");
createRoot(root).render(
  <StrictMode>
    <Scene />
  </StrictMode>,
);
