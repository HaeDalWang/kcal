//! 0018 작업 B 감지·맡기기 테스트. 홈은 모두 TempDir이다. 실제 `claude`는 실행하지 않는다(가짜 CLI = 실행 스크립트).
//! 실제 실행은 `#[ignore]` 하나뿐이다(`real_claude_proposes_where_rules_belong`, 조율자가 돌린다).

use std::time::Duration;

use super::super::tests::records;
use super::super::{CheckKind, dismissed};
use super::*;
use crate::context::fsutil::testing::TempDir;

pub(in crate::coach::checks) struct Fixture {
    pub home: TempDir,
    pub work: TempDir,
    pub data: TempDir,
}

pub(in crate::coach::checks) fn fixture(name: &str) -> Fixture {
    Fixture {
        home: TempDir::new(&format!("rules-{name}-home")),
        work: TempDir::new(&format!("rules-{name}-work")),
        data: TempDir::new(&format!("rules-{name}-data")),
    }
}

pub(in crate::coach::checks) fn recent(workspaces: &[&Path]) -> RecentRecords {
    RecentRecords {
        workspaces: workspaces.iter().map(|p| p.to_path_buf()).collect(),
        ..records(&[])
    }
}

impl Fixture {
    pub fn records(&self) -> RecentRecords {
        recent(&[self.work.path()])
    }

    pub fn user_root(&self) -> PathBuf {
        self.home.path().join(".claude").join("rules")
    }

    pub fn user_id(&self) -> String {
        format!("{ID_PREFIX}{}", self.user_root().display())
    }

    pub fn target(&self) -> RulesTarget {
        find_target(
            self.home.path(),
            &self.records(),
            &Tuning::default(),
            &self.user_id(),
        )
        .unwrap()
    }

    /// `rules`(키 → 답)를 Claude 답으로 판정해 보관한다(실행 없이).
    pub fn store(&self, state: &HandoffState, answers: &[RawRule]) -> RulesTarget {
        let target = self.target();
        let answer = Answer {
            rules: answers.to_vec(),
        };
        *state.rules.lock().unwrap() = Some(judge(&target, &answer));
        target
    }
}

pub(in crate::coach::checks) fn raw(key: &str, choice: &str) -> RawRule {
    RawRule {
        key: key.into(),
        choice: choice.into(),
        paths: vec![],
        name: String::new(),
        description: String::new(),
        reason: "이유예요.".into(),
    }
}

pub(in crate::coach::checks) fn paths_answer(key: &str, paths: &[&str]) -> RawRule {
    RawRule {
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        ..raw(key, "paths")
    }
}

pub(in crate::coach::checks) fn skill_answer(key: &str, name: &str, description: &str) -> RawRule {
    RawRule {
        name: name.into(),
        description: description.into(),
        ..raw(key, "skill")
    }
}

/// 이 테스트가 만든 폴더 안의 규칙 카드(id, 규칙 이름들).
fn rule_cards(home: &Path, records: &RecentRecords, under: &[&Path]) -> Vec<(String, Vec<String>)> {
    list(home, records, &Tuning::default())
        .into_iter()
        .filter(|c| under.iter().any(|u| Path::new(&c.file).starts_with(u)))
        .filter_map(|c| match c.facts {
            CheckFacts::RulesPaths { rules, .. } => {
                Some((c.id, rules.into_iter().map(|r| r.name).collect()))
            }
            _ => None,
        })
        .collect()
}

const SCOPED: &str = "---\npaths:\n  - \"src/**\"\n---\nscoped\n";

// ---------- 감지 ----------

#[test]
fn user_and_project_rules_get_one_card_per_scope() {
    let f = fixture("scopes");
    f.home
        .write(".claude/rules/common/testing.md", "Run tests.\n");
    f.home.write(".claude/rules/scoped.md", SCOPED);
    f.work.write(".claude/rules/api.md", "API rules.\n");
    let cards = rule_cards(f.home.path(), &f.records(), &[f.home.path(), f.work.path()]);
    let project = format!(
        "{ID_PREFIX}{}",
        f.work.path().join(".claude/rules").display()
    );
    assert_eq!(
        cards,
        [
            (f.user_id(), vec!["common/testing.md".to_owned()]),
            (project, vec!["api.md".to_owned()]),
        ]
    );
    let card = list(f.home.path(), &f.records(), &Tuning::default())
        .into_iter()
        .find(|c| c.id == f.user_id())
        .unwrap();
    assert_eq!(card.kind, CheckKind::A);
    let options: Vec<(&str, bool)> = card
        .options
        .iter()
        .map(|o| (o.id.as_str(), o.is_recommended))
        .collect();
    assert_eq!(options, [("handoff", true), ("keep", false)]);
    let CheckFacts::RulesPaths { chars, scope, .. } = card.facts else {
        panic!()
    };
    assert_eq!((chars, scope), (11, RulesScope::User));
}

#[test]
fn same_rule_from_several_workspaces_is_listed_once() {
    let f = fixture("multi");
    f.home.write(".claude/rules/a.md", "A\n");
    let other = TempDir::new("rules-multi-other");
    let records = recent(&[f.work.path(), other.path()]);
    assert_eq!(
        rule_cards(f.home.path(), &records, &[f.home.path()]),
        [(f.user_id(), vec!["a.md".to_owned()])]
    );
}

#[test]
fn no_recent_workspace_means_no_card() {
    let f = fixture("none");
    f.home.write(".claude/rules/a.md", "A\n");
    assert!(rule_cards(f.home.path(), &recent(&[]), &[f.home.path()]).is_empty());
}

#[test]
fn plugin_cache_rules_are_not_targets() {
    let f = fixture("plugin");
    let rule = f
        .home
        .write(".claude/plugins/cache/m/p/1/.claude/rules/x.md", "X\n");
    let workspace = rule.parent().unwrap().parent().unwrap().parent().unwrap();
    let cards = rule_cards(f.home.path(), &recent(&[workspace]), &[f.home.path()]);
    assert!(cards.is_empty(), "{cards:?}");
}

#[cfg(unix)]
#[test]
fn rule_linked_outside_its_rules_folder_is_not_a_target() {
    let f = fixture("outside");
    let dotfiles = TempDir::new("rules-outside-dotfiles");
    let real = dotfiles.write("claude/rules/linked.md", "linked\n");
    f.home.write(".claude/rules/own.md", "own\n");
    std::os::unix::fs::symlink(&real, f.user_root().join("linked.md")).unwrap();
    assert_eq!(
        rule_cards(
            f.home.path(),
            &f.records(),
            &[f.home.path(), dotfiles.path()]
        ),
        [(f.user_id(), vec!["own.md".to_owned()])]
    );
}

#[test]
fn rule_pointed_at_by_other_rules_counts_them() {
    let f = fixture("pointed");
    f.home
        .write(".claude/rules/style.md", "See [simple](./simple.md#top).\n");
    f.home.write(
        ".claude/rules/flow.md",
        "[s](simple.md) and [web](https://x.dev/simple.md)\n",
    );
    f.home.write(
        ".claude/rules/simple.md",
        "Keep it simple. [self](simple.md)\n",
    );
    let target = f.target();
    let counts: Vec<(&str, usize)> = target
        .rules
        .iter()
        .map(|r| (r.fact.name.as_str(), r.fact.referenced_by))
        .collect();
    assert_eq!(counts, [("flow.md", 0), ("simple.md", 2), ("style.md", 0)]);
}

#[test]
fn rules_card_can_be_dismissed_and_counts_as_alert() {
    let f = fixture("dismiss");
    f.home.write(".claude/rules/a.md", "A\n");
    let path = f.data.path().join(dismissed::FILE_NAME);
    let tuning = Tuning::default();
    dismissed::dismiss(f.home.path(), &f.records(), &tuning, &path, &f.user_id()).unwrap();
    let card = dismissed::mark(
        list(f.home.path(), &f.records(), &tuning),
        &dismissed::read(&path),
    )
    .into_iter()
    .find(|c| c.id == f.user_id())
    .unwrap();
    assert!(card.is_dismissed);
    // 알림 개수: 추천이 keep이 아니다.
    assert_ne!(
        card.options
            .iter()
            .find(|o| o.is_recommended)
            .map(|o| o.id.as_str()),
        Some("keep")
    );
}

// ---------- 검사(실행 없이) ----------

#[test]
fn skill_name_and_description_checks_follow_defaults() {
    let ok = |name: &str, description: &str| {
        matches!(
            check_answer(&skill_answer("r1", name, description)),
            Proposal::Skill { .. }
        )
    };
    assert!(ok("run-checks", "Use before a commit."));
    assert!(ok(&"a".repeat(64), "x"));
    assert!(ok("a1", &"설".repeat(1_536)));
    let problem = |name: &str, description: &str| match check_answer(&skill_answer(
        "r1",
        name,
        description,
    )) {
        Proposal::Unusable { problem } => Some(problem),
        _ => None,
    };
    for bad in ["", "Bad", "a_b", "-a", "a-", "한글", &"a".repeat(65)] {
        assert_eq!(problem(bad, "x"), Some(RuleProblem::SkillName), "{bad}");
    }
    assert_eq!(problem("a", "two\nlines"), Some(RuleProblem::Description));
    assert_eq!(problem("a", "  "), Some(RuleProblem::Description));
    assert_eq!(
        problem("a", &"x".repeat(1_537)),
        Some(RuleProblem::Description)
    );
}

#[test]
fn paths_checks_refuse_absolute_long_or_too_many() {
    let problem = |paths: &[&str]| match check_answer(&paths_answer("r1", paths)) {
        Proposal::Unusable { problem } => Some(problem),
        _ => None,
    };
    assert_eq!(problem(&["src/**/*.ts", "lib/*.rs"]), None);
    assert_eq!(problem(&[]), Some(RuleProblem::PathsCount));
    assert_eq!(problem(&["a"; 21]), Some(RuleProblem::PathsCount));
    assert_eq!(problem(&["/etc/**"]), Some(RuleProblem::PathAbsolute));
    assert_eq!(problem(&["~/x/**"]), Some(RuleProblem::PathAbsolute));
    assert_eq!(problem(&["a\nb"]), Some(RuleProblem::PathText));
    assert_eq!(problem(&[&"a".repeat(201)]), Some(RuleProblem::PathText));
    assert_eq!(
        check_answer(&raw("r1", "rewrite")),
        Proposal::Unusable {
            problem: RuleProblem::Choice
        }
    );
}

#[test]
fn missing_duplicate_and_unknown_keys_are_reported() {
    let f = fixture("keys");
    for name in ["a.md", "b.md", "c.md"] {
        f.home.write(&format!(".claude/rules/{name}"), "rule\n");
    }
    let state = HandoffState::default();
    let target = f.store(
        &state,
        &[
            raw("r1", "keep"),
            raw("r2", "keep"),
            raw("r2", "keep"),
            raw("r9", "keep"),
        ],
    );
    let preview = preview_of(&target, &stored(&state, &f.user_id()).unwrap().unwrap());
    let proposals: Vec<&Proposal> = preview.rules.iter().map(|r| &r.proposal).collect();
    assert_eq!(
        proposals,
        [
            &Proposal::Keep,
            &Proposal::Unusable {
                problem: RuleProblem::Duplicate
            },
            &Proposal::Unusable {
                problem: RuleProblem::Missing
            },
        ]
    );
    assert_eq!(preview.unknown_keys, 1);
    assert!(preview.rules.iter().all(|r| !r.can_apply));
}

#[test]
fn reason_keeps_first_line_only() {
    let answer = RawRule {
        reason: "  첫 줄이에요.\n둘째 줄\n".into(),
        ..raw("r1", "keep")
    };
    assert_eq!(reason_of(&answer), "첫 줄이에요.");
}

// ---------- 가짜 CLI ----------

#[cfg(unix)]
mod fake_cli {
    use super::*;
    use serde_json::{Value, json};
    use std::os::unix::fs::PermissionsExt;

    /// 인자·stdin·실행 횟수를 스크립트 옆에 남기고 `response.json`을 돌려주는 가짜 claude.
    pub fn cli(dir: &TempDir, response: &Value, tail: &str) -> PathBuf {
        dir.write("bin/response.json", &response.to_string());
        let script = format!(
            "#!/bin/sh\nd=\"$(dirname \"$0\")\"\nprintf '%s\\n' \"$@\" > \"$d/args\"\n\
             echo run >> \"$d/runs\"\ncat > \"$d/stdin\"\ncat \"$d/response.json\"\n{tail}\n"
        );
        let path = dir.write("bin/claude", &script);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    pub fn success(rules: Value) -> Value {
        json!({"type": "result", "is_error": false, "result": "{}", "structured_output": {"rules": rules}})
    }

    pub fn read(dir: &TempDir, name: &str) -> String {
        std::fs::read_to_string(dir.path().join("bin").join(name)).unwrap_or_default()
    }

    pub fn ask_once(
        f: &Fixture,
        state: &HandoffState,
        cli: PathBuf,
    ) -> Result<RulesPreview, HandoffError> {
        let spec = RunSpec {
            cli: Some(cli),
            dir: f.data.path().join(super::super::super::handoff::DIR_NAME),
            timeout: Duration::from_secs(60),
        };
        tauri::async_runtime::block_on(ask(state, f.target(), Some(64.0), 20, spec))
    }
}

#[cfg(unix)]
use fake_cli::{ask_once, cli, read, success};

#[cfg(unix)]
fn three_rules(f: &Fixture) {
    f.home.write(
        ".claude/rules/common/api.md",
        "API handlers validate input.\n",
    );
    f.home.write(
        ".claude/rules/common/release.md",
        "When releasing, bump the version.\n",
    );
    f.home
        .write(".claude/rules/common/tone.md", "Answer briefly.\n");
}

/// 인자: 규칙 스키마·safe mode, 파일 내용 없음. 프롬프트(stdin): 반드시 넣을 줄, 상대 이름, 절대 경로 없음.
#[cfg(unix)]
fn expect_rules_args_and_prompt(f: &Fixture) {
    let args = read(&f.data, "args");
    assert!(args.lines().any(|l| l == SCHEMA), "rules schema: {args}");
    assert!(args.lines().any(|l| l == "--safe-mode"));
    let stdin = read(&f.data, "stdin");
    for line in [
        "번역하지 마라",
        "같은 언어",
        "한국어 한 문장",
        "name=\"common/api.md\"",
    ] {
        assert!(stdin.contains(line), "{line} missing: {stdin}");
    }
    let home = f.home.path().to_string_lossy().into_owned();
    assert!(!stdin.contains(&home), "absolute path in prompt");
    assert!(
        !args.contains("API handlers"),
        "file content must not be in args"
    );
}

#[cfg(unix)]
#[test]
fn rules_success_sends_relative_names_and_required_prompt_lines() {
    let f = fixture("success");
    three_rules(&f);
    let rules = serde_json::json!([
        {"key": "r1", "choice": "paths", "paths": ["src/api/**/*.ts"], "name": "", "description": "", "reason": "API 파일에만 필요해요."},
        {"key": "r2", "choice": "skill", "paths": [], "name": "release", "description": "Use when releasing.", "reason": "배포할 때만 필요해요."},
        {"key": "r3", "choice": "keep", "paths": [], "name": "", "description": "", "reason": "늘 필요해요."}
    ]);
    let path = cli(&f.data, &success(rules), "");
    let preview = ask_once(&f, &HandoffState::default(), path).unwrap();
    expect_rules_args_and_prompt(&f);
    let rows: Vec<(&str, &Proposal, bool)> = preview
        .rules
        .iter()
        .map(|r| (r.name.as_str(), &r.proposal, r.can_apply))
        .collect();
    assert_eq!(
        rows,
        [
            (
                "common/api.md",
                &Proposal::Paths {
                    paths: vec!["src/api/**/*.ts".into()]
                },
                true
            ),
            (
                "common/release.md",
                &Proposal::Skill {
                    name: "release".into(),
                    description: "Use when releasing.".into()
                },
                true
            ),
            ("common/tone.md", &Proposal::Keep, false),
        ]
    );
    assert_eq!(preview.rules[0].reason, "API 파일에만 필요해요.");
}

#[cfg(unix)]
#[test]
fn answers_failing_checks_cannot_be_applied() {
    let f = fixture("failing");
    three_rules(&f);
    f.home.write(".claude/rules/common/zz.md", "zz\n");
    let rules = serde_json::json!([
        {"key": "r1", "choice": "paths", "paths": ["/Users/me/src/**"], "name": "", "description": "", "reason": "x"},
        {"key": "r2", "choice": "skill", "paths": [], "name": "Release_Notes", "description": "x", "reason": "x"},
        {"key": "r3", "choice": "skill", "paths": [], "name": "tone", "description": "one\ntwo", "reason": "x"},
        {"key": "r7", "choice": "keep", "paths": [], "name": "", "description": "", "reason": "x"}
    ]);
    let path = cli(&f.data, &success(rules), "");
    let preview = ask_once(&f, &HandoffState::default(), path).unwrap();
    let problems: Vec<&Proposal> = preview.rules.iter().map(|r| &r.proposal).collect();
    let unusable = |problem| Proposal::Unusable { problem };
    assert_eq!(
        problems,
        [
            &unusable(RuleProblem::PathAbsolute),
            &unusable(RuleProblem::SkillName),
            &unusable(RuleProblem::Description),
            &unusable(RuleProblem::Missing),
        ]
    );
    assert_eq!(preview.unknown_keys, 1);
    assert!(preview.rules.iter().all(|r| !r.can_apply));
}

#[cfg(unix)]
#[test]
fn rules_handoff_reuses_task_a_error_kinds() {
    let f = fixture("errors");
    three_rules(&f);
    let not_found = serde_json::json!({"type": "result", "is_error": true, "terminal_reason": "api_error",
        "api_error_status": 404, "result": "There's an issue with the selected model"});
    let path = cli(&f.data, &not_found, "exit 1");
    let state = HandoffState::default();
    assert_eq!(
        ask_once(&f, &state, path),
        Err(HandoffError::OpusUnavailable)
    );
    let wrong = serde_json::json!({"type": "result", "is_error": false, "structured_output": {"text": "x"}});
    let path = cli(&f.data, &wrong, "");
    assert_eq!(
        ask_once(&f, &state, path),
        Err(HandoffError::Failed {
            reason: "Claude 응답이 약속한 모양이 아니에요".into()
        })
    );
    let spec = RunSpec {
        cli: None,
        dir: f.data.path().join("x"),
        timeout: Duration::from_secs(60),
    };
    let missing = tauri::async_runtime::block_on(ask(&state, f.target(), Some(64.0), 20, spec));
    assert_eq!(missing, Err(HandoffError::CliMissing));
    let spec = RunSpec {
        cli: None,
        dir: f.data.path().join("x"),
        timeout: Duration::from_secs(60),
    };
    let low = tauri::async_runtime::block_on(ask(&state, f.target(), Some(5.0), 20, spec));
    assert_eq!(
        low,
        Err(HandoffError::LimitTooLow {
            remaining: 5.0,
            min_remaining: 20
        })
    );
}

#[cfg(unix)]
#[test]
fn same_card_with_unchanged_files_does_not_run_again() {
    let f = fixture("reuse");
    three_rules(&f);
    let rules = serde_json::json!([
        {"key": "r1", "choice": "keep", "paths": [], "name": "", "description": "", "reason": "x"},
        {"key": "r2", "choice": "keep", "paths": [], "name": "", "description": "", "reason": "x"},
        {"key": "r3", "choice": "keep", "paths": [], "name": "", "description": "", "reason": "x"}
    ]);
    let path = cli(&f.data, &success(rules), "");
    let state = HandoffState::default();
    let first = ask_once(&f, &state, path.clone()).unwrap();
    assert_eq!(ask_once(&f, &state, path.clone()), Ok(first));
    assert_eq!(read(&f.data, "runs").lines().count(), 1);
    // 파일 하나가 바뀌면 다시 맡긴다.
    f.home
        .write(".claude/rules/common/tone.md", "Answer very briefly.\n");
    ask_once(&f, &state, path).unwrap();
    assert_eq!(read(&f.data, "runs").lines().count(), 2);
}

// ---------- 실제 실행(돌리지 않는다) ----------

/// 실행: `cargo test --lib real_claude_proposes_where_rules_belong -- --ignored --nocapture`(src-tauri에서).
/// 실제 `claude`를 한 번 실행해 한도를 쓴다. 한도 문은 거치지 않는다(남은 한도를 먼저 직접 확인한다).
#[test]
#[ignore = "실제 claude CLI를 실행해 한도를 쓴다"]
fn real_claude_proposes_where_rules_belong() {
    let f = fixture("real");
    f.home.write(
        ".claude/rules/frontend/react-components.md",
        "# React components\n\n- Components in `src/components/*.tsx` use function components and hooks.\n- Keep props typed with a `Props` type next to the component.\n- Co-locate the test as `<name>.test.tsx`.\n",
    );
    f.home.write(
        ".claude/rules/release.md",
        "# Releasing\n\nWhen the user asks to cut a release:\n1. Bump the version in `package.json` and `Cargo.toml`.\n2. Update `CHANGELOG.md` with the merged PRs.\n3. Tag `vX.Y.Z` and push the tag.\n",
    );
    f.home.write(
        ".claude/rules/communication.md",
        "# Communication\n\n- Answer in short, plain sentences.\n- Say when you are unsure instead of guessing.\n",
    );
    let real_home = std::env::home_dir().unwrap();
    let path_env = std::env::var_os("PATH");
    let cli = find_cli(&real_home, path_env.as_deref()).expect("claude CLI 없음");
    let spec = RunSpec {
        cli: Some(cli.clone()),
        dir: f.data.path().join(super::super::handoff::DIR_NAME),
        timeout: Duration::from_secs(300),
    };
    println!("cli={}", cli.display());
    let result = tauri::async_runtime::block_on(ask(
        &HandoffState::default(),
        f.target(),
        Some(100.0),
        0,
        spec,
    ));
    match result {
        Ok(p) => {
            for r in &p.rules {
                println!(
                    "{} {} can_apply={} {:?} reason={}",
                    r.key, r.name, r.can_apply, r.proposal, r.reason
                );
            }
            println!("unknown_keys={}", p.unknown_keys);
        }
        Err(e) => println!("error={e:?}"),
    }
}
