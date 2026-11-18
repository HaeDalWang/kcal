import { ChevronLeft } from "lucide-react";
import { useEffect, useState } from "react";

import { CAPTION_CLASS, GROUP_CLASS, ROW_CLASS, TuningForm } from "@/components/tuning-form";
import { Checkbox } from "@/components/ui/checkbox";
import { AGENT_LABEL } from "@/lib/agents";
import { LANGUAGE_CHOICES, t, type LanguageChoice } from "@/lib/i18n";
import {
  AGENTS,
  errorMessage,
  getAppVersion,
  setSettings,
  type Agent,
  type Settings,
  type Tuning,
} from "@/lib/ipc";

type SettingsViewProps = {
  settings: Settings;
  onSaved: (settings: Settings) => void;
  onBack: () => void;
};

/** 팝오버 안의 설정 화면. 표시할 에이전트(팝오버 탭·개요, 트레이 값)와 언어를 고르고(바로 저장), 그 아래에서 tuning을 고친다("저장" 버튼).
 * 끈 에이전트는 조회하지 않는다. 마지막 하나는 끌 수 없다. */
export function SettingsView({ settings, onSaved, onBack }: SettingsViewProps) {
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const enabled = settings.enabledAgents;
  const isLastOne = enabled.length === 1;

  // 마운트 때 한 번만 읽는다. 실패하거나 문자열이 아니면 표시하지 않는다.
  useEffect(() => {
    getAppVersion().then(
      (v) => setVersion(typeof v === "string" && v ? v : null),
      () => setVersion(null),
    );
  }, []);

  // 저장은 한 번에 하나만(토글과 tuning 저장이 서로의 값을 덮어쓰지 않게).
  const save = (agents: Agent[], tuning?: Tuning, language?: LanguageChoice) => {
    setIsSaving(true);
    return setSettings(agents, tuning, language).then(
      (saved) => {
        setIsSaving(false);
        onSaved(saved);
      },
      (reason: unknown) => {
        setIsSaving(false);
        throw reason;
      },
    );
  };

  const report = (saving: Promise<void>) =>
    saving.then(
      () => setError(null),
      (reason: unknown) => setError(errorMessage(reason)),
    );

  const toggle = (agent: Agent, checked: boolean) => {
    const next = AGENTS.filter((a) => (a === agent ? checked : enabled.includes(a)));
    report(save(next));
  };

  const pickLanguage = (language: LanguageChoice) => report(save(enabled, undefined, language));

  return (
    <div
      className="flex flex-col"
      onKeyDown={(event) => {
        // 입력 중인 칸·열린 목록의 Escape는 그 칸이 쓴다.
        const target = event.target as HTMLElement;
        if (event.key === "Escape" && target.tagName !== "INPUT" && target.tagName !== "SELECT") {
          onBack();
        }
      }}
    >
      <div className="px-3 pt-2.5">
        <button
          type="button"
          autoFocus
          onClick={onBack}
          className="-ml-1 flex w-fit items-center gap-0.5 rounded-md py-0.5 pr-1.5 text-[15px] font-semibold hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none active:translate-y-px"
        >
          <ChevronLeft className="size-4" aria-hidden />
          {t.settings.title}
        </button>
      </div>
      <TuningForm
        settings={settings}
        isSaving={isSaving}
        onSave={(tuning) => save(enabled, tuning)}
        footer={
          version && (
            <p className="pb-0.5 text-center text-xs text-muted-foreground tabular-nums">
              kcal {version}
            </p>
          )
        }
      >
        <div className="flex flex-col gap-3">
          <fieldset className="flex flex-col">
            <legend className={CAPTION_CLASS}>{t.settings.agents}</legend>
            <div className={GROUP_CLASS}>
              {AGENTS.map((agent) => {
                const isChecked = enabled.includes(agent);
                return (
                  <label key={agent} className={`${ROW_CLASS} text-sm`}>
                    <span className="flex-1">{AGENT_LABEL[agent]}</span>
                    <Checkbox
                      checked={isChecked}
                      disabled={isSaving || (isChecked && isLastOne)}
                      onCheckedChange={(checked) => toggle(agent, checked)}
                    />
                  </label>
                );
              })}
            </div>
            {isLastOne && (
              <p className="mt-1 px-1.5 text-xs text-muted-foreground">{t.settings.minOne}</p>
            )}
          </fieldset>
          <div className="flex flex-col">
            <div className={GROUP_CLASS}>
              <label className={`${ROW_CLASS} text-sm`}>
                <span className="flex-1">{t.lang.title}</span>
                <select
                  value={settings.language}
                  disabled={isSaving}
                  onChange={(event) => pickLanguage(event.target.value as LanguageChoice)}
                  className="h-6 rounded-md border border-input bg-background px-1.5 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring disabled:opacity-50"
                >
                  {LANGUAGE_CHOICES.map((choice) => (
                    <option key={choice} value={choice}>
                      {t.lang[choice]}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            {settings.reason && (
              <p className="mt-1 px-1.5 text-xs text-muted-foreground">{settings.reason}</p>
            )}
          </div>
          {error && (
            <p role="alert" className="px-1.5 text-xs text-destructive">
              {error}
            </p>
          )}
        </div>
      </TuningForm>
    </div>
  );
}
