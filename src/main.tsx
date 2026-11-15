import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "@/App";
import { Dashboard } from "@/components/dashboard";
import { currentWindowLabel, DASHBOARD_WINDOW } from "@/lib/ipc";
import { followSystemTheme } from "@/lib/theme";
import "./index.css";

const root = document.getElementById("root");
if (!root) throw new Error("#root 요소가 없습니다.");

followSystemTheme();

// 같은 진입점을 두 창이 쓴다. 대시보드 창(docs/decisions/0010)은 Rust `open_dashboard`가 만든다.
const isDashboard = currentWindowLabel() === DASHBOARD_WINDOW;

createRoot(root).render(<StrictMode>{isDashboard ? <Dashboard /> : <App />}</StrictMode>);
