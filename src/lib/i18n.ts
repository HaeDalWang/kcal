// 화면 언어(docs/decisions/0021). 문구는 `t.<묶음>.<이름>`으로 읽는다. 사전은 `i18n-ko.ts`(원본)와 `i18n-en.ts`
// (`Messages` 모양이라 키가 빠지면 타입 검사가 실패한다). 언어는 모듈에 하나만 두고 `useSettings`가 설정을 받을 때와
// `settings-updated` 때 바꾼다. 바뀐 설정이 루트(App·Dashboard)를 다시 그리므로 전역 store가 필요 없다.
import { en } from "@/lib/i18n-en";
import { ko, type Messages } from "@/lib/i18n-ko";

/** 화면 언어. Rust `i18n::Lang`과 짝. */
export type Lang = "ko" | "en";
/** 설정의 언어 선택. Rust `settings::Language`와 짝. */
export type LanguageChoice = "auto" | Lang;

export const LANGUAGE_CHOICES: readonly LanguageChoice[] = ["auto", "ko", "en"];

const MESSAGES: Record<Lang, Messages> = { ko, en };
const LOCALE: Record<Lang, string> = { ko: "ko-KR", en: "en-US" };

/** 자동이면 브라우저(웹뷰) 언어가 `ko`로 시작할 때 한국어, 아니면 영어. */
export function resolveLanguage(
  choice: LanguageChoice,
  browser: string = navigator.language,
): Lang {
  if (choice !== "auto") return choice;
  return browser.toLowerCase().startsWith("ko") ? "ko" : "en";
}

let current: Lang = resolveLanguage("auto");
document.documentElement.lang = current;

/** 지금 언어의 문구(ES module live binding: `setLanguage` 뒤 읽으면 새 언어). */
export let t: Messages = MESSAGES[current];

export function currentLanguage(): Lang {
  return current;
}

/** 언어를 바꾼다. 다시 그리는 것은 부르는 쪽(설정 state 갱신)이 한다. */
export function setLanguage(lang: Lang): void {
  current = lang;
  t = MESSAGES[lang];
  document.documentElement.lang = lang;
}

/** 지금 언어의 BCP 47 locale(`Intl`용). */
export function locale(): string {
  return LOCALE[current];
}

/** 지금 언어를 따르는 `Intl.NumberFormat`. 언어마다 한 번만 만든다. */
export function numberFormat(options: Intl.NumberFormatOptions): {
  format: (value: number) => string;
} {
  const cache = new Map<Lang, Intl.NumberFormat>();
  return {
    format(value: number) {
      let formatter = cache.get(current);
      if (!formatter) {
        formatter = new Intl.NumberFormat(LOCALE[current], options);
        cache.set(current, formatter);
      }
      return formatter.format(value);
    },
  };
}
