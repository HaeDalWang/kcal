//! `codex_fast.rs` 판정 테스트(임시 HOME만 쓴다. 실제 `~/.codex`는 읽지 않는다).

use super::*;

const CHATGPT: &str =
    r#"{"auth_mode":"chatgpt","tokens":{"access_token":"dummy","account_id":"a"}}"#;

fn home(config: Option<&str>, auth: Option<&str>) -> TempDir {
    // 같은 프로세스의 병렬 테스트와 폴더 이름이 겹치지 않게 호출마다 번호를 붙인다.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let home = TempDir::new(&format!("codex-fast-{n}"));
    if let Some(config) = config {
        home.write(".codex/config.toml", config);
    }
    if let Some(auth) = auth {
        home.write(".codex/auth.json", auth);
    }
    home
}

fn card(home: &TempDir) -> Option<CoachCheck> {
    find_check(&checks(home, &records(&[])), "D-codex-fast-mode").cloned()
}

fn multiplier_of(card: &CoachCheck) -> Option<f64> {
    let CheckFacts::CodexFastMode { multiplier, .. } = &card.facts else {
        panic!("다른 사실");
    };
    *multiplier
}

#[test]
fn codex_fast_card_shows_for_saved_fast_with_chatgpt_login() {
    let home = home(
        Some("model = \"gpt-6.2-astra\"\nservice_tier = \"fast\"\n"),
        Some(CHATGPT),
    );
    let card = card(&home).unwrap();
    assert_eq!(card.agent, Agent::Codex);
    assert_eq!(card.kind, CheckKind::D);
    assert_eq!(
        card.file,
        home.path().join(".codex/config.toml").to_string_lossy()
    );
    assert_eq!(card.docs.len(), 2);
    assert_eq!(
        option_ids(&card),
        vec![("standard", true, false), ("keep", false, false)]
    );
    assert_eq!(multiplier_of(&card), Some(2.0));
}

#[test]
fn codex_fast_multiplier_follows_the_model_table() {
    for (model, want) in [
        ("gpt-6.2-astra", Some(2.0)),
        ("gpt-6.2-sol", Some(2.0)),
        ("gpt-6.1-sol", Some(2.5)),
        ("gpt-6.1-terra", Some(2.5)),
        ("gpt-6-astra", Some(2.5)),
        ("gpt-6-sol", Some(2.5)),
        ("gpt-6-luna", Some(2.5)),
        ("gpt-9-unknown", None),
    ] {
        let home = home(
            Some(&format!("model = \"{model}\"\nservice_tier = \"fast\"\n")),
            Some(CHATGPT),
        );
        assert_eq!(multiplier_of(&card(&home).unwrap()), want, "{model}");
    }
    let no_model = home(Some("service_tier = \"fast\"\n"), Some(CHATGPT));
    assert_eq!(multiplier_of(&card(&no_model).unwrap()), None);
}

#[test]
fn codex_fast_card_reads_features_section_and_ignores_other_sections() {
    // 다른 표의 같은 이름 키는 최상위가 아니다.
    let other = home(
        Some("[profiles.x]\nservice_tier = \"fast\"\n"),
        Some(CHATGPT),
    );
    assert!(card(&other).is_none());
    let on = home(
        Some("service_tier = \"fast\" # 속도\n[features]\nfast_mode = true\n"),
        Some(CHATGPT),
    );
    assert!(card(&on).is_some());
}

#[test]
fn codex_fast_card_is_absent_when_not_applicable() {
    let cases: [(&str, Option<&str>, Option<&str>); 8] = [
        ("no config", None, Some(CHATGPT)),
        (
            "no tier",
            Some("model = \"gpt-6.2-astra\"\n"),
            Some(CHATGPT),
        ),
        (
            "default tier",
            Some("service_tier = \"default\"\n"),
            Some(CHATGPT),
        ),
        (
            "fast_mode off",
            Some("service_tier = \"fast\"\n[features]\nfast_mode = false\n"),
            Some(CHATGPT),
        ),
        (
            "api forced",
            Some("service_tier = \"fast\"\nforced_login_method = \"api\"\n"),
            Some(CHATGPT),
        ),
        ("no auth", Some("service_tier = \"fast\"\n"), None),
        (
            "api key login",
            Some("service_tier = \"fast\"\n"),
            Some(r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-dummy"}"#),
        ),
        (
            "broken toml",
            Some("service_tier = \"fast\n[[[ = =\n\u{0}"),
            Some(CHATGPT),
        ),
    ];
    for (name, config, auth) in cases {
        assert!(card(&home(config, auth)).is_none(), "{name}");
    }
}

#[test]
fn codex_fast_card_treats_auth_without_mode_by_tokens_and_ignores_broken_auth() {
    let config = Some("service_tier = \"fast\"\n");
    let legacy = home(config, Some(r#"{"tokens":{"access_token":"dummy"}}"#));
    assert!(card(&legacy).is_some());
    assert!(card(&home(config, Some("not json"))).is_none());
}

#[test]
fn codex_fast_card_option_is_refused_by_apply_without_writing() {
    let home = home(Some("service_tier = \"fast\"\n"), Some(CHATGPT));
    let before = std::fs::read_to_string(home.path().join(".codex/config.toml")).unwrap();
    let log_file = home.path().join("log.jsonl");
    let run = |option: &str| {
        apply(
            home.path(),
            &records(&[]),
            &Tuning::default(),
            &log_file,
            "D-codex-fast-mode",
            option,
            &edit::content_hash(b"x"),
            NOW,
        )
        .map_err(message)
    };
    assert!(run("standard").unwrap_err().contains("/fast"));
    assert!(run("keep").is_err());
    assert_eq!(
        std::fs::read_to_string(home.path().join(".codex/config.toml")).unwrap(),
        before
    );
    assert!(!log_file.exists());
}
