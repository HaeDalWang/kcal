# 0021. 영어 화면 (ko + en)

- 상태: 결정(구현함)
- 날짜: 2026-11-11
- 앞선 결정: [0009](0009-ui-plain-language.md) 쉬운 말·용어 표, [0011](0011-visual-design.md) 트레이 크기

## 문제

화면이 한국어뿐이다. 처음 오는 방문자 대부분은 영어 사용자라 첫 화면을 읽을 수 없다.

## 결정

- **새 의존성 없음.** i18n 라이브러리를 쓰지 않는다.
- **프론트 사전**: `src/lib/i18n-ko.ts`(원본, `Messages = typeof ko`)와 `src/lib/i18n-en.ts`(`en: Messages`라 키가 빠지거나 함수 모양이 다르면 `pnpm typecheck`가 실패한다). 끼워 넣는 값은 작은 함수(`(n) => \`${n} windows\``), 영어 복수형은 사전 안에서 처리한다. 두 사전을 합치면 800줄을 넘어 언어별 파일로 나눴다. 사용하는 쪽은 `import { t } from "@/lib/i18n"` 하나다.
- **언어 상태**: `i18n.ts` 모듈에 하나(`t`는 ES module live binding). `useSettings`가 설정을 받을 때(`get_settings`, `settings-updated`, 저장 뒤) state를 바꾸기 전에 언어를 바꾼다. 설정 state가 바뀌면 루트(App·Dashboard)가 다시 그려지고, 이 코드에는 `memo`가 없어 아래가 모두 새 언어로 그려진다. 전역 store·context 없음, 언어가 바뀔 때만 다시 그린다.
- **언어 결정**: settings `language: "auto" | "ko" | "en"`(기본 auto, 이 칸이 없는 옛 파일도 auto). auto는 `navigator.language`가 `ko`로 시작하면 한국어, 아니면 영어. 정하는 쪽은 프론트엔드다.
- **Rust 문구**(트레이 메뉴·tooltip, 대시보드 창 제목, 점검 카드 선택지·적용 기록 이름, 오류·거절 문장, 사용량 실패 이유, 컨텍스트 항목 이름·경고): `src-tauri/src/i18n.rs`의 `tr!("한국어 {x}", "English {x}")`(같은 인자로 `format!`)와 `pick(ko, en)`. 언어는 전역 `AtomicBool` 하나. 로그(`eprintln!`)와 Claude에게 보내는 프롬프트는 한국어 그대로 두고, 프롬프트 안 "답할 언어"(합치거나 뺀 것·고른 이유)만 화면 언어를 따른다.
- **Rust가 언어를 아는 길**: 프론트엔드가 정한 언어를 새 command `set_ui_language(language)`로 알린다. Rust는 그 값을 settings 파일 `resolvedLanguage`에 남기고(같으면 쓰지 않는다), 바뀌었으면 트레이 메뉴·tooltip·대시보드 제목을 바꾸고 컨텍스트를 다시 재고(prewarm) 사용량 새로고침을 요청한다(429 backoff는 그대로, 최소 간격은 건너뜀: 첫 실행에서 영어 이유가 최대 30초 보이던 것, 2026-11-12). 시작할 때는 파일의 `language`(고른 값) 또는 `resolvedLanguage`(auto일 때)로 시작해, 첫 조회 실패 이유·트레이 메뉴가 처음부터 맞는 언어다. `Settings`에 `resolvedLanguage`가 있어 프론트는 다를 때만 command를 부른다.
- **숫자·날짜**: `numberFormat(options)`가 지금 언어의 locale(`ko-KR`/`en-US`)로 `Intl.NumberFormat`을 만든다. 영어 날짜는 `Intl.DateTimeFormat("en-US")`(예: "Fri, Nov 6", "Nov 3, 9:00 PM"). 한국어 날짜 문구는 전과 같다. 토큰 축약("34.2K")은 두 언어 모두 `en-US` compact 그대로다(전부터 그랬다).
- **설정 화면**: 표시할 에이전트 아래 "언어" 줄(`<select>`: 자동 / 한국어 / English). 고르면 바로 저장한다.
- **용어**: 0009 "English terms" 표. 영어 문구는 짧고 개발자가 바로 읽는 말로 쓴다.
- **대시보드 시작 컨텍스트 한 번만**: 에이전트 카드의 지표 칸(`MetricLines`)에 있던 시작 컨텍스트 카드(값 + `/context` 7묶음)와 오른쪽 `ContextList`(값 + 항목 목록)가 같은 합계·같은 한도 몫 문장·두 가지 분해를 두 번 보였다. 대시보드는 `ContextList` 하나로 보이고(`MetricLines hasStartup={false}`), 그 제목·값을 팝오버와 같은 "시작 컨텍스트" · `startupValue`(창 몫 %)로 맞췄다. 팝오버 에이전트 탭은 그대로 카드(7묶음)를 보인다.

## 판정(Rulings)

- 영어 화면의 언어 선택 이름은 "Korean"이다(한국어 화면은 "한국어"). 목표가 "영어에서 한글이 한 글자도 보이지 않는다"라서다. 한국어 사용자가 영어 화면에서 찾기는 조금 어렵다.
- `resolvedLanguage`가 없는 옛 settings 파일은 마지막 화면 언어를 한국어로 본다(0021 전에는 한국어 화면만 있었다). 파일이 아예 없는 첫 실행은 영어로 시작하고, 한국어 OS면 프론트가 곧바로 `set_ui_language("ko")`로 바꾼다.
- 적용 기록(`coach-log`)의 이름은 적용한 때의 언어로 저장된다. 언어를 바꿔도 옛 기록 이름은 그대로다(기록은 고쳐 쓰지 않는다).

## 확인

- `src/components/english-ui.test.tsx`: 팝오버 개요·에이전트 탭 3개·설정, 대시보드 사용량·점검 탭을 IPC fixture로 영어로 그려 글자·aria-label·title에 한글(U+AC00–U+D7A3)이 없는지 본다.
- `src-tauri/src/i18n.rs` `english_variants_of_user_facing_rust_strings_have_no_hangul`: 테스트 밖 Rust 소스의 한글 문자열이 모두 `tr!`/`pick`의 한국어 칸(또는 로그·프롬프트·정규식 허용 목록)에 있고, 짝인 영어 칸에 한글이 없는지 본다.
- `settings.rs` 테스트: `language`가 없는 옛 파일은 auto로 읽힌다.

## 위험

- WKWebView의 `navigator.language`가 앱 번들의 지역화 설정을 따를 수 있다(번들에 `.lproj`가 없으면 OS가 한국어여도 `en-US`일 수 있다). dev 앱 실측은 아래 "실측"에 적는다. 틀리면 auto가 한국어 사용자에게 영어를 보인다. 고치는 길: Rust가 OS 언어(macOS `AppleLanguages`)를 읽어 프론트에 준다.

## 실측 (2026-11-11, 이 Mac: macOS 한국어, `AppleLanguages = ko-KR`)

- dev 앱(번들 아님) 팝오버 웹뷰: `navigator.language = "ko-KR"`, `navigator.languages = ["ko-KR"]` → 자동 = 한국어(확인됨). 설치한 `.app` 번들에서도 같은지는 확인하지 않았다(추정: 같다. 릴리스 앱에서 `navigator.language`를 한 번 찍어 보면 확인된다).
- 언어를 English로 저장하자 `resolvedLanguage`가 `en`이 되고 대시보드 창 제목이 "kcal Dashboard", 컨텍스트 항목이 "home-agent-skills (9 skills)", "11 hooks"처럼 영어로 다시 만들어졌다(확인됨).
- 영어 개요 카드의 "Startup context 7.6% of the window (30.2K / 400K)"가 310px 폭에서 두 줄이 되어, 개요 카드만 "7.6% of the window"로 줄이고 토큰 수는 툴팁에 둔다(`metrics.startupShort`). 에이전트 탭·대시보드는 토큰 수까지 보인다.
