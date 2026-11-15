// Browser preview used to record the README GIF: a macOS-like backdrop, the 310px popover under a
// menu bar strip, and the dashboard window that "Dashboard" opens. Everything shown comes from
// demo-fixtures.ts (demo data, not measurements); nothing is read from the machine.
// Open: `pnpm exec vite --port 1480 --strictPort`, then http://127.0.0.1:1480/src/test/preview/demo.html
// `?zoom=1.2` scales the popover (the GIF is shown smaller than 1:1 in the README).
// Not part of the app bundle: index.html does not reference it, like popover.html and dashboard.html.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import { App } from "@/App";
import { Dashboard } from "@/components/dashboard";
import { followSystemTheme } from "@/lib/theme";
import { demoFixtures } from "@/test/preview/demo-fixtures";
import "@/index.css";

const OPEN_DASHBOARD = "demo:open-dashboard";
const zoom = Number(new URLSearchParams(location.search).get("zoom") ?? "1");
const now = Date.now();
const data = demoFixtures(now);

mockWindows("popover", "dashboard");
mockIPC(
  (cmd) => {
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
        height: 860,
        background: "linear-gradient(160deg, #2b3550 0%, #1c2438 55%, #141a2b 100%)",
      }}
    >
      <MenuBar isTrayActive={!isDashboardOpen} />
      {!isDashboardOpen && (
        <div
          className="absolute top-[34px] right-[12px] w-[310px] overflow-hidden rounded-[12px] bg-background text-foreground shadow-2xl ring-1 ring-white/15"
          style={{ zoom }}
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

followSystemTheme();
const style = document.createElement("style");
style.textContent =
  "@keyframes demo-window-in{from{opacity:0;transform:scale(.98)}to{opacity:1;transform:none}}" +
  ".demo-window-in{animation:demo-window-in .18s ease-out}" +
  "html,body{background:#141a2b!important}";
document.head.append(style);
const root = document.getElementById("root");
if (!root) throw new Error("#root element is missing.");
createRoot(root).render(
  <StrictMode>
    <Scene />
  </StrictMode>,
);
