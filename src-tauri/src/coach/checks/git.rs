//! C. 커밋 안내 두 벌: 사용자 범위 시작 지시문에 커밋·PR 규칙이 있는데 Claude Code 내장 커밋·PR 안내도 켜져 있다.
//! 근거: memory "Claude isn't following my CLAUDE.md"(내 규칙과 내장 규칙이 경쟁), settings-reference `includeGitInstructions`.
//! 프로젝트 글은 보지 않는다(고치는 설정이 전역이라서). 감지 기준은 사용자 결정 2026-11-07.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::duplicates::{PersonalSkill, personal_skills};
use super::{CheckFacts, CheckKind, CheckOption, CoachCheck, Inputs, LineMatch};
use crate::coach::edit::{JsonOp, key};
use crate::context::fsutil::{self, front_matter_key};
use crate::context::{Loading, instructions, paths};
use crate::i18n::pick;
use crate::usage::Agent;

const DOCS: [&str; 3] = [
    "https://code.claude.com/docs/en/settings-reference#includegitinstructions",
    "https://code.claude.com/docs/en/memory#claude-isnt-following-my-claudemd",
    "https://code.claude.com/docs/en/skills#run-your-checks-before-each-commit",
];
const MAX_LINE_CHARS: usize = 120;
/// skills "Run your checks before each commit": 이 이름의 스킬이 있으면 커밋 전에 돌리라는 내장 안내가 붙는다.
const PRE_COMMIT_SKILLS: [&str; 2] = ["verify", "simplify"];

static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s{0,3}#{1,6}\s").unwrap());
static HEADING_WORDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)commit|커밋|pull request|풀 리퀘스트").unwrap());
static PR_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bPR\b").unwrap());
static ANY_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)co-authored-by|commit message|커밋 메시지").unwrap());
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(```|~~~)").unwrap());
/// 실제 서명 줄: `Co-Authored-By: 이름 <메일>`.
static TRAILER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)co-authored-by:\s*\S.*<[^>]+@[^>]+>").unwrap());

/// 커밋·PR 규칙으로 보는 줄(1부터 번호, 글). 제목 줄(코드 블록 밖)의 커밋·PR 낱말, 또는 아무 줄의 서명·커밋 메시지.
pub fn matching_lines(text: &str) -> Vec<(usize, String)> {
    let mut fenced = false;
    let mut found = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if FENCE.is_match(line) {
            fenced = !fenced;
        }
        let is_heading = !fenced
            && HEADING.is_match(line)
            && (HEADING_WORDS.is_match(line) || PR_WORD.is_match(line));
        if is_heading || ANY_LINE.is_match(line) {
            found.push((
                index + 1,
                line.trim().chars().take(MAX_LINE_CHARS).collect(),
            ));
        }
    }
    found
}

fn is_model_invocable(text: &str) -> bool {
    front_matter_key(text, "disable-model-invocation").is_none_or(|v| v != "true")
}

/// 커밋 전 안내가 붙는 개인 스킬·커맨드 이름(Claude가 부를 수 있는 것만).
fn pre_commit_skills(home: &Path, skills: &[PersonalSkill]) -> Vec<String> {
    let commands = paths::claude_root(home).join("commands");
    let mut found: Vec<String> = skills
        .iter()
        .filter(|s| PRE_COMMIT_SKILLS.contains(&s.name.as_str()) && is_model_invocable(&s.text))
        .map(|s| s.name.clone())
        .collect();
    for name in PRE_COMMIT_SKILLS {
        let file = commands.join(format!("{name}.md"));
        if file.is_file()
            && is_model_invocable(&fsutil::read_text(&file))
            && !found.iter().any(|f| f == name)
        {
            found.push(name.to_owned());
        }
    }
    found
}

/// 사용자 범위 시작 지시문(`~/.claude/CLAUDE.md`·@import·`paths` 없는 user rules)의 커밋·PR 줄과 서명 줄 여부.
fn scan_user_instructions(home: &Path) -> (Vec<LineMatch>, bool) {
    let mut matches = Vec::new();
    let mut has_own_trailer = false;
    for item in instructions::claude(home, None).items {
        let Some(path) = item.path.filter(|_| item.loading == Loading::Startup) else {
            continue;
        };
        let text = fsutil::read_text(Path::new(&path));
        has_own_trailer |= TRAILER.is_match(&text);
        matches.extend(
            matching_lines(&text)
                .into_iter()
                .map(|(line, text)| LineMatch {
                    file: path.clone(),
                    line,
                    text,
                }),
        );
    }
    (matches, has_own_trailer)
}

/// `C-claude-git-instructions`.
pub fn git_instructions(input: &Inputs) -> Option<CoachCheck> {
    if !input.file.is_file()
        || input.settings.get("includeGitInstructions") == Some(&Value::Bool(false))
        || input.is_env_on("CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS")
    {
        return None;
    }
    let (matches, has_own_trailer) = scan_user_instructions(input.home);
    if matches.is_empty() {
        return None;
    }
    let pre_commit_skills = pre_commit_skills(input.home, &personal_skills(input.home));
    let is_safe_to_disable = has_own_trailer && pre_commit_skills.is_empty();
    let disable = JsonOp::Set(vec![key("includeGitInstructions")], Value::Bool(false));
    let options = vec![
        CheckOption::write(
            "disable-git-instructions",
            pick(
                "기본 커밋 안내 끄기",
                "Turn off the built-in commit instructions",
            ),
            is_safe_to_disable,
            vec![disable],
        ),
        CheckOption::keep(!is_safe_to_disable),
    ];
    Some(CoachCheck {
        id: "C-claude-git-instructions".into(),
        kind: CheckKind::C,
        agent: Agent::Claude,
        facts: CheckFacts::GitInstructions {
            matches,
            has_own_trailer,
            pre_commit_skills,
        },
        options,
        file: input.file.to_string_lossy().into_owned(),
        docs: DOCS.iter().map(|d| (*d).to_owned()).collect(),
        tokens: None,
        is_dismissed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_sentence_is_not_caught_but_commit_heading_is() {
        assert!(matching_lines("커밋·푸시·배포는 사람이 요청했을 때만 한다.").is_empty());
        assert_eq!(
            matching_lines("intro\n## Commit Message Format\n"),
            [(2, "## Commit Message Format".to_owned())]
        );
    }

    #[test]
    fn pr_word_counts_only_in_headings_and_is_case_sensitive() {
        assert_eq!(matching_lines("# PR 규칙").len(), 1);
        assert!(matching_lines("# pr notes\nopen a PR when ready").is_empty());
        assert_eq!(matching_lines("Add Co-Authored-By at the end").len(), 1);
        assert_eq!(matching_lines("커밋 메시지는 짧게").len(), 1);
    }

    #[test]
    fn headings_inside_code_fences_are_not_rules() {
        assert!(matching_lines("```sh\n# commit everything\n```\n").is_empty());
    }

    #[test]
    fn trailer_needs_a_name_and_an_address() {
        assert!(TRAILER.is_match("Co-Authored-By: Me <me@example.com>"));
        assert!(!TRAILER.is_match("Attribution line comes from the session's system reminder"));
        assert!(!TRAILER.is_match("Co-Authored-By:"));
    }
}
