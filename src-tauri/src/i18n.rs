//! 화면 언어(docs/decisions/0021). Rust가 만드는 사용자용 문구(트레이 메뉴·tooltip, 점검 카드 선택지,
//! 오류, 컨텍스트 항목 이름)는 `tr!(한국어, 영어)` 또는 `pick(한국어, 영어)`로 쓴다. 로그(`eprintln!`)는 그대로 둔다.
//! 언어는 프론트엔드가 정해 `set_ui_language`로 알려 준다(자동 = `navigator.language`). 시작할 때는 설정 파일에
//! 남은 마지막 값으로 시작한다(`settings::Settings::effective_language`).

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

/// 화면 언어. TS 짝은 `src/lib/i18n.ts`의 `Lang`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Ko,
    /// 처음 켤 때(설정 파일에 기록 없음) 기본값. 대부분의 새 사용자가 영어권이라 첫 화면이 맞도록 한다.
    #[default]
    En,
}

static IS_EN: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
thread_local! {
    /// 테스트 스레드마다 따로 정하는 언어. 병렬 테스트가 서로의 언어를 바꾸지 않게 한다.
    static TEST_LANG: std::cell::Cell<Option<Lang>> = const { std::cell::Cell::new(None) };
}

/// 지금 언어. 테스트는 `with_lang` 밖에서 늘 한국어다(기존 테스트 기대값).
pub fn current() -> Lang {
    #[cfg(test)]
    if let Some(lang) = TEST_LANG.with(std::cell::Cell::get) {
        return lang;
    }
    if IS_EN.load(Ordering::Relaxed) {
        Lang::En
    } else {
        Lang::Ko
    }
}

/// 언어를 바꾼다. 바뀌었으면 true.
pub fn set(lang: Lang) -> bool {
    IS_EN.swap(lang == Lang::En, Ordering::Relaxed) != (lang == Lang::En)
}

/// 고정 문구 하나를 고른다.
pub fn pick(ko: &'static str, en: &'static str) -> &'static str {
    match current() {
        Lang::Ko => ko,
        Lang::En => en,
    }
}

/// `f`를 그 언어로 돌린다(이 스레드만).
#[cfg(test)]
pub fn with_lang<T>(lang: Lang, f: impl FnOnce() -> T) -> T {
    let previous = TEST_LANG.with(|cell| cell.replace(Some(lang)));
    let result = f();
    TEST_LANG.with(|cell| cell.set(previous));
    result
}

/// `tr!("스킬 {n}개", "{n} skills")`: 지금 언어의 `format!` 결과(String). 두 문구는 같은 인자를 쓴다.
#[macro_export]
macro_rules! tr {
    ($ko:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::current() {
            $crate::i18n::Lang::Ko => format!($ko $(, $arg)*),
            $crate::i18n::Lang::En => format!($en $(, $arg)*),
        }
    };
}

/// 영어 단어의 단수·복수. 한국어 문구는 쓰지 않는다.
pub fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_hangul(c: char) -> bool {
        ('\u{AC00}'..='\u{D7A3}').contains(&c)
    }

    fn has_hangul(text: &str) -> bool {
        text.chars().any(is_hangul)
    }

    #[test]
    fn tr_and_pick_follow_the_thread_language() {
        let n = 2;
        assert_eq!(tr!("스킬 {n}개", "{n} skills"), "스킬 2개");
        assert_eq!(
            with_lang(Lang::En, || tr!("스킬 {}개", "{} skills", n)),
            "2 skills"
        );
        assert_eq!(with_lang(Lang::En, || pick("종료", "Quit")), "Quit");
        assert_eq!(pick("종료", "Quit"), "종료");
    }

    #[test]
    fn language_parses_only_ko_and_en() {
        assert_eq!(
            serde_json::from_value::<Lang>("en".into()).unwrap(),
            Lang::En
        );
        assert!(serde_json::from_value::<Lang>("ja".into()).is_err());
    }

    /// 문자열·주석을 가려 읽는 작은 토큰기. 리터럴마다 (본문, 바로 앞 코드 조각, 시작 줄)을 낸다.
    struct Literal {
        text: String,
        before: String,
        line: usize,
    }

    fn literals(source: &str) -> Vec<Literal> {
        let chars: Vec<char> = source.chars().collect();
        let mut out = Vec::new();
        let mut code = String::new();
        let mut line = 1;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();
            if c == '\n' {
                line += 1;
            }
            if c == '/' && next == Some('/') {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            if c == '/' && next == Some('*') {
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 2;
                continue;
            }
            if c == '\'' {
                // 문자 리터럴('x', '\n', '"')만 건너뛴다. 수명('a)은 그대로 둔다.
                if next == Some('\\') {
                    let mut j = i + 2;
                    while j < chars.len() && chars[j] != '\'' {
                        j += 1;
                    }
                    i = j + 1;
                    continue;
                }
                if chars.get(i + 2) == Some(&'\'') {
                    i += 3;
                    continue;
                }
            }
            let is_raw = c == 'r' && matches!(next, Some('"') | Some('#'));
            if c == '"' || is_raw {
                let start_line = line;
                let mut hashes = 0;
                let mut j = i + 1;
                if is_raw {
                    while chars.get(j) == Some(&'#') {
                        hashes += 1;
                        j += 1;
                    }
                    if chars.get(j) != Some(&'"') {
                        code.push(c);
                        i += 1;
                        continue;
                    }
                    j += 1;
                }
                let mut text = String::new();
                while j < chars.len() {
                    let d = chars[j];
                    if d == '\n' {
                        line += 1;
                    }
                    if !is_raw && d == '\\' {
                        text.push(d);
                        if let Some(&e) = chars.get(j + 1) {
                            text.push(e);
                            if e == '\n' {
                                line += 1;
                            }
                        }
                        j += 2;
                        continue;
                    }
                    if d == '"' && (0..hashes).all(|k| chars.get(j + 1 + k) == Some(&'#')) {
                        j += 1 + hashes;
                        break;
                    }
                    text.push(d);
                    j += 1;
                }
                let before: String = code.split_whitespace().collect::<Vec<_>>().join("");
                let tail = before.chars().rev().take(24).collect::<Vec<_>>();
                out.push(Literal {
                    text,
                    before: tail.into_iter().rev().collect(),
                    line: start_line,
                });
                code.push('"');
                i = j;
                continue;
            }
            code.push(c);
            i += 1;
        }
        out
    }

    /// 테스트 모듈(`#[cfg(test)]` 다음 줄이 `mod`)부터는 읽지 않는다.
    fn without_test_module(source: &str) -> &str {
        let mut offset = 0;
        for line in source.split_inclusive('\n') {
            if line.trim_end() == "#[cfg(test)]" {
                let rest = &source[offset + line.len()..];
                if rest.trim_start().starts_with("mod ") {
                    return &source[..offset];
                }
            }
            offset += line.len();
        }
        source
    }

    /// 한글을 둘 수 있는 자리: `tr!`·`pick`의 한국어 칸, 로그·내부 패닉 메시지, 그리고 화면에 보이지 않는 곳
    /// (Claude에게 보내는 프롬프트, 사용자 글을 찾는 정규식). 화면에 보이지 않는 곳은 파일·줄로 적는다.
    const KO_SLOTS: [&str; 5] = ["tr!(", "pick(", "eprintln!(", "expect(", "panic!("];
    const NOT_SHOWN: [(&str, &str); 4] = [
        // 사용자 규칙 글에서 커밋 안내를 찾는 정규식.
        ("coach/checks/git.rs", "커밋"),
        // Claude에게 보내는 프롬프트(화면에 보이지 않는다). 답의 언어는 `pick`으로 화면 언어를 따른다.
        ("coach/checks/handoff.rs", "아래 <file>"),
        ("coach/checks/rules_handoff.rs", "아래 <rule>"),
        ("coach/checks/rules_handoff.rs", "rules("),
    ];

    /// 영어 화면에서 한글이 보이지 않게: 사용자용 문구는 모두 `tr!`/`pick`을 거치고, 그 영어 칸에는 한글이 없다.
    #[test]
    fn english_variants_of_user_facing_rust_strings_have_no_hangul() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        let mut problems = Vec::new();
        let mut checked = 0;
        for path in files {
            let name = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let is_test_file = name.ends_with("tests.rs") || name == "ipc_fixtures.rs";
            if is_test_file {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let found = literals(without_test_module(&source));
            for (index, literal) in found.iter().enumerate() {
                if !has_hangul(&literal.text) {
                    continue;
                }
                let is_ko_slot = KO_SLOTS.iter().any(|slot| literal.before.ends_with(slot));
                let is_not_shown = NOT_SHOWN
                    .iter()
                    .any(|(file, mark)| name == *file && literal.text.contains(mark));
                if is_not_shown {
                    continue;
                }
                if !is_ko_slot {
                    problems.push(format!(
                        "{name}:{} 한글 문구가 tr!/pick 밖에 있다",
                        literal.line
                    ));
                    continue;
                }
                if literal.before.ends_with("tr!(") || literal.before.ends_with("pick(") {
                    checked += 1;
                    match found.get(index + 1) {
                        Some(en) if !has_hangul(&en.text) => {}
                        _ => problems.push(format!("{name}:{} 영어 칸에 한글", literal.line)),
                    }
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
        assert!(
            checked > 100,
            "tr!/pick이 너무 적다({checked}): 토큰기가 깨졌나"
        );
    }

    #[test]
    fn literal_reader_skips_comments_chars_and_raw_strings() {
        let got = literals("// \"주석\"\nlet a = '\"'; let b = r#\"x\"#; tr!(\"가\", \"a\");");
        let texts: Vec<_> = got.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["x", "가", "a"]);
        assert!(got[1].before.ends_with("tr!("));
        assert_eq!(got[1].line, 2);
    }
}
