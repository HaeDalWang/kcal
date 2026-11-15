import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, vi } from "vitest";

import { setLanguage } from "@/lib/i18n";
import { contextStore, metricsStore, ratesStore } from "@/lib/snapshot-store";

// 기존 테스트는 한국어 화면을 본다. jsdom의 navigator.language(en-US) 대신 한국어 OS로 둔다(자동 = 한국어).
// 영어 화면 테스트는 설정 `language: "en"`으로 연다(`english-ui.test.tsx`).
Object.defineProperty(window.navigator, "language", { value: "ko-KR", configurable: true });
setLanguage("ko");

/** jsdom에는 matchMedia가 없다. 테스트에서 `prefersDark`와 change 이벤트를 조작할 수 있는 stub. */
export const media = {
  prefersDark: false,
  listeners: new Set<() => void>(),
  setPrefersDark(value: boolean) {
    this.prefersDark = value;
    for (const listener of this.listeners) listener();
  },
};

// jsdom에는 scrollIntoView가 없다(대시보드 점검 탭·효과 보기가 쓴다). 테스트는 spy로 본다.
Element.prototype.scrollIntoView ??= function scrollIntoView() {};

beforeEach(() => {
  vi.stubGlobal("matchMedia", (query: string) => ({
    get matches() {
      return media.prefersDark;
    },
    media: query,
    addEventListener: (_: "change", listener: () => void) => media.listeners.add(listener),
    removeEventListener: (_: "change", listener: () => void) => media.listeners.delete(listener),
  }));
});

afterEach(() => {
  cleanup();
  clearMocks();
  ratesStore.reset();
  contextStore.reset();
  metricsStore.reset();
  vi.unstubAllGlobals();
  localStorage.clear();
  document.documentElement.className = "";
  media.prefersDark = false;
  media.listeners.clear();
  setLanguage("ko");
});
