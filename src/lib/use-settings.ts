import { useEffect, useState } from "react";

import { resolveLanguage, setLanguage } from "@/lib/i18n";
import {
  errorMessage,
  getSettings,
  onSettingsUpdated,
  setUiLanguage,
  type Settings,
} from "@/lib/ipc";

/** 설정의 언어를 화면에 적용하고, Rust가 아는 언어와 다르면 알린다(0021). state를 바꾸기 전에 부른다. */
function applyLanguage(settings: Settings): void {
  const lang = resolveLanguage(settings.language);
  setLanguage(lang);
  if (lang !== settings.resolvedLanguage) {
    setUiLanguage(lang).catch((reason: unknown) =>
      console.error("set_ui_language:", errorMessage(reason)),
    );
  }
}

/** 설정은 get_settings로 한 번 읽고 settings-updated로 갱신한다(팝오버·대시보드 공통).
 * 언어도 여기서 정한다: 받은 설정을 state에 넣기 전에 언어를 바꾸므로 다시 그릴 때 새 언어로 그린다. */
export function useSettings(): {
  settings: Settings | null;
  setSettings: (next: Settings) => void;
  error: string | null;
} {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isCancelled = false;
    const unlisteners: (() => void)[] = [];
    const keep = (stop: () => void) => (isCancelled ? stop() : unlisteners.push(stop));
    const fail = (reason: unknown) => !isCancelled && setError(errorMessage(reason));
    const apply = (next: Settings | null) => {
      // 테스트 mock처럼 빈 응답이면 아직 설정이 없는 것으로 둔다.
      if (isCancelled || !next) return;
      applyLanguage(next);
      setSettings(next);
    };
    onSettingsUpdated(apply).then(keep, fail);
    getSettings().then(apply, fail);
    return () => {
      isCancelled = true;
      unlisteners.forEach((stop) => stop());
    };
  }, []);

  const save = (next: Settings) => {
    applyLanguage(next);
    setSettings(next);
  };
  return { settings, setSettings: save, error };
}
