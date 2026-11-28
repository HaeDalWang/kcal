//! Claude Code가 실제로 보낸 요청의 크기(ctxmeter `claude-prompt.js`).
//!
//! 2.1.300부터 transcript에 `api-request-shape` 줄이 남는다: 요청의 system 블록과 도구 정의 전부.
//! 크기만 잰다. 같은 파일의 `api-request-blob` 줄(메시지 원문)은 바이트 substring 검사로 걸러
//! 파싱하지 않는다.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::SystemTime;

use regex::Regex;
use serde_json::Value;

use super::paths;

const SHAPE_TYPE: &str = "api-request-shape";
static SHAPE_MARKER: LazyLock<regex::bytes::Regex> = LazyLock::new(|| {
    regex::bytes::Regex::new(&regex::escape(r#""type":"api-request-shape""#)).unwrap()
});
/// 세션 작업 폴더. 최상위 키 `"cwd":"…"`만 바이트로 찾는다(문자열 안의 `\"cwd\"`는 이스케이프돼 안 맞는다).
static CWD_FIELD: LazyLock<regex::bytes::Regex> =
    LazyLock::new(|| regex::bytes::Regex::new(r#""cwd":("(?:[^"\\]|\\.)*")"#).unwrap());
/// 메시지 원문 줄. 안의 같은 글자는 이스케이프돼 이 표시와 맞지 않는다.
const BLOB_MARKER: &[u8] = br#""type":"api-request-blob""#;
/// 요청 줄(`params.model`). `api-request-shape`·`api-request-blob`과 겹치지 않게 뒤의 `,`까지 본다.
const REQUEST_MARKER: &[u8] = br#""type":"api-request","#;
/// 세션 시작 때 첫 메시지에 붙는 첨부(2.1.3xx). 바깥 줄은 `"type":"attachment"`, 안쪽 `attachment.type`이 이 값이다.
const INSTRUCTIONS: &str = "instructions";
const SKILL_LISTING: &str = "skill_listing";
const AGENT_LISTING: &str = "agent_listing_delta";
const MCP_INSTRUCTIONS: &str = "mcp_instructions_delta";
const ATTACHMENTS: [&str; 4] = [INSTRUCTIONS, SKILL_LISTING, AGENT_LISTING, MCP_INSTRUCTIONS];

#[derive(Debug, PartialEq)]
pub struct McpServerShape {
    pub server: String,
    pub tools: usize,
    pub bytes: u64,
}

/// 첨부의 이름 하나와 실린 바이트(내용은 버린다).
#[derive(Debug, PartialEq)]
pub struct NamedBytes {
    pub name: String,
    pub bytes: u64,
}

/// `skill_listing`: 목록 전체 바이트와, `plugin:skill` 이름의 접두사별 묶음.
#[derive(Debug, Default, PartialEq)]
pub struct SkillListing {
    pub skills: usize,
    pub bytes: u64,
    /// 접두사(`my-skills`·`anthropic-skills` 등) → 스킬 수·줄 바이트. 이름순.
    pub by_prefix: Vec<(String, usize, u64)>,
}

/// 세션 시작 때 첫 메시지에 실린 첨부의 크기. `/context`의 Memory files·Skills·Custom agents·MCP server instructions에 해당한다.
/// 각 첨부의 첫 줄(시작 시점)만 본다. 없으면 `None`(옛 버전).
#[derive(Debug, Default, PartialEq)]
pub struct StartupAttachments {
    /// `instructions.files[]`: 경로와 내용 바이트.
    pub memory_files: Option<Vec<NamedBytes>>,
    pub skills: Option<SkillListing>,
    /// `agent_listing_delta`에서 내장(`builtInTypes`·`builtInPluginTypes`)이 아닌 에이전트 줄.
    pub custom_agents: Option<Vec<NamedBytes>>,
    /// `mcp_instructions_delta`: 서버 이름과 안내 블록 바이트.
    pub mcp_instructions: Option<Vec<NamedBytes>>,
}

#[derive(Debug, Default, PartialEq)]
pub struct ShapeSummary {
    pub system_blocks: usize,
    pub system_bytes: u64,
    pub builtin_tools: usize,
    pub builtin_bytes: u64,
    /// 바이트가 큰 순.
    pub mcp: Vec<McpServerShape>,
    /// `defer_loading: true` 자리표시 도구(이름·짧은 설명만 보내고 스키마는 검색할 때).
    pub deferred_tools: usize,
    pub deferred_bytes: u64,
    /// 지연 로딩 도구만 보낸 MCP 서버 이름(`mcp`에 없는 것). 서버 스킬(`server:skill`)을 서버로 묶는 데 쓴다.
    pub deferred_mcp: Vec<String>,
    /// shape를 보낸 세션의 작업 폴더(transcript `cwd`). 전역 모드에서 출처를 밝히는 데 쓴다.
    pub cwd: Option<String>,
    /// 같은 세션의 마지막 `api-request` 줄 `params.model`.
    pub model: Option<String>,
    pub startup: StartupAttachments,
}

fn str_bytes(value: &Value) -> u64 {
    value.as_str().map_or(0, |s| s.len() as u64)
}

fn strs(value: Option<&Value>) -> Vec<&str> {
    value
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// 이름 배열과 같은 순서의 문자열 배열을 (이름, 바이트)로 묶는다.
fn paired(
    names: Option<&Value>,
    texts: Option<&Value>,
    keep: impl Fn(&str) -> bool,
) -> Vec<NamedBytes> {
    let texts = texts
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    strs(names)
        .into_iter()
        .zip(texts)
        .filter(|(name, _)| keep(name))
        .map(|(name, text)| NamedBytes {
            name: name.to_owned(),
            bytes: str_bytes(text),
        })
        .collect()
}

/// 목록 줄 `- <name>: <설명>`을 이름으로 찾아 `plugin:skill` 접두사별로 더한다. 줄바꿈 1바이트 포함.
fn skill_listing(attachment: &Value) -> SkillListing {
    let content = attachment
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("");
    let names = strs(attachment.get("names"));
    let mut by_prefix: Vec<(String, usize, u64)> = Vec::new();
    for line in content.lines() {
        let Some((prefix, _)) = names
            .iter()
            .find(|n| {
                line.strip_prefix("- ")
                    .and_then(|l| l.strip_prefix(**n))
                    .is_some_and(|l| l.starts_with(':'))
            })
            .and_then(|n| n.split_once(':'))
        else {
            continue;
        };
        let bytes = line.len() as u64 + 1;
        match by_prefix.iter_mut().find(|(p, ..)| p == prefix) {
            Some(entry) => {
                entry.1 += 1;
                entry.2 += bytes;
            }
            None => by_prefix.push((prefix.to_owned(), 1, bytes)),
        }
    }
    by_prefix.sort();
    SkillListing {
        skills: attachment
            .get("skillCount")
            .and_then(Value::as_u64)
            .map_or(names.len(), |n| n as usize),
        bytes: content.len() as u64,
        by_prefix,
    }
}

impl StartupAttachments {
    fn is_empty_slot(&self, kind: &str) -> bool {
        match kind {
            INSTRUCTIONS => self.memory_files.is_none(),
            SKILL_LISTING => self.skills.is_none(),
            AGENT_LISTING => self.custom_agents.is_none(),
            _ => self.mcp_instructions.is_none(),
        }
    }

    /// 첨부 하나를 크기로 줄여 채운다. 내용 문자열은 바이트 수만 남는다.
    fn fill(&mut self, kind: &str, a: &Value) {
        match kind {
            INSTRUCTIONS => {
                let files = a
                    .get("files")
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                self.memory_files = Some(
                    files
                        .iter()
                        .map(|f| NamedBytes {
                            name: f
                                .get("path")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned(),
                            bytes: f.get("content").map_or(0, str_bytes),
                        })
                        .collect(),
                );
            }
            SKILL_LISTING => self.skills = Some(skill_listing(a)),
            AGENT_LISTING => {
                let built_in: Vec<&str> = strs(a.get("builtInTypes"))
                    .into_iter()
                    .chain(strs(a.get("builtInPluginTypes")))
                    .collect();
                self.custom_agents = Some(paired(a.get("addedTypes"), a.get("addedLines"), |n| {
                    !built_in.contains(&n)
                }));
            }
            _ => {
                self.mcp_instructions =
                    Some(paired(a.get("addedNames"), a.get("addedBlocks"), |_| true));
            }
        }
    }
}

fn attachment_marker(kind: &str) -> Vec<u8> {
    format!(r#""type":"{kind}""#).into_bytes()
}

/// 표시가 맞는 첨부 줄이면 (종류, 첨부 객체). 안쪽 `attachment.type`까지 확인한다.
fn parse_attachment(
    line: &[u8],
    markers: &[(&'static str, Vec<u8>)],
) -> Option<(&'static str, Value)> {
    let (kind, _) = markers.iter().find(|(_, m)| contains(line, m))?;
    let mut row: Value = serde_json::from_slice(line).ok()?;
    if row.get("type")?.as_str()? != "attachment" {
        return None;
    }
    let attachment = row.get_mut("attachment")?.take();
    (attachment.get("type")?.as_str()? == *kind).then_some((kind, attachment))
}

/// `mcp__<server>__<tool>`. server 이름에도 `_`가 들어갈 수 있다.
fn mcp_server(name: &str) -> Option<&str> {
    static MCP_TOOL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^mcp__(.+?)__[^_].*$").unwrap());
    MCP_TOOL
        .captures(name)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
}

fn json_bytes(value: &Value) -> u64 {
    serde_json::to_vec(value).map_or(0, |v| v.len() as u64)
}

/// 크기만. MCP 서버 이름은 설정이지 내용이 아니라서 남긴다.
pub fn summarise(row: &Value) -> ShapeSummary {
    let empty = Vec::new();
    let list = |key: &str| {
        row.get("shape")
            .and_then(|s| s.get(key))
            .and_then(Value::as_array)
            .unwrap_or(&empty)
    };
    let system = list("system");
    let mut summary = ShapeSummary {
        system_blocks: system.len(),
        system_bytes: system
            .iter()
            .filter_map(|b| b.get("text")?.as_str())
            .map(|t| t.len() as u64)
            .sum(),
        ..ShapeSummary::default()
    };
    for tool in list("tools") {
        let bytes = json_bytes(tool);
        if tool.get("defer_loading") == Some(&Value::Bool(true)) {
            summary.deferred_tools += 1;
            summary.deferred_bytes += bytes;
            let name = tool.get("name").and_then(Value::as_str).unwrap_or("");
            if let Some(server) = mcp_server(name)
                && !summary.deferred_mcp.iter().any(|s| s == server)
            {
                summary.deferred_mcp.push(server.to_owned());
            }
            continue;
        }
        let name = tool.get("name").and_then(Value::as_str).unwrap_or("");
        let Some(server) = mcp_server(name) else {
            summary.builtin_tools += 1;
            summary.builtin_bytes += bytes;
            continue;
        };
        match summary.mcp.iter_mut().find(|s| s.server == server) {
            Some(entry) => {
                entry.tools += 1;
                entry.bytes += bytes;
            }
            None => summary.mcp.push(McpServerShape {
                server: server.to_owned(),
                tools: 1,
                bytes,
            }),
        }
    }
    summary.mcp.sort_by_key(|s| std::cmp::Reverse(s.bytes));
    summary
        .deferred_mcp
        .retain(|d| !summary.mcp.iter().any(|s| &s.server == d));
    summary
}

/// 한 transcript에서 읽은 값. shape 줄 자체는 `summarise`에 넘긴다.
struct TranscriptRead {
    shape: Value,
    cwd: Option<String>,
    model: Option<String>,
    startup: StartupAttachments,
}

/// 파일의 마지막 shape 줄, 세션 작업 폴더, 마지막 요청 모델, 첫 시작 첨부. 표시가 없는 줄(blob 포함)은 JSON으로 읽지 않는다.
fn latest_shape_in(file: &Path) -> Option<TranscriptRead> {
    let mut reader = BufReader::new(std::fs::File::open(file).ok()?);
    let mut line = Vec::new();
    let mut latest = None;
    let mut cwd = None;
    let mut model = None;
    let mut startup = StartupAttachments::default();
    let mut markers: Vec<(&'static str, Vec<u8>)> = ATTACHMENTS
        .iter()
        .map(|k| (*k, attachment_marker(k)))
        .collect();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if contains(&line, BLOB_MARKER) {
            continue;
        }
        if !SHAPE_MARKER.is_match(&line) {
            if cwd.is_none() {
                cwd = session_cwd(&line);
            }
            if contains(&line, REQUEST_MARKER) {
                model = request_model(&line).or(model);
            } else if let Some((kind, attachment)) = parse_attachment(&line, &markers) {
                startup.fill(kind, &attachment);
                // 각 첨부는 첫 줄(세션 시작)만 쓴다.
                markers.retain(|(k, _)| startup.is_empty_slot(k));
            }
            continue;
        }
        // 기록 중인 세션의 잘린 마지막 줄은 파싱에 실패하고 건너뛴다.
        if let Ok(row) = serde_json::from_slice::<Value>(&line)
            && row.get("type").and_then(Value::as_str) == Some(SHAPE_TYPE)
        {
            latest = Some(row);
        }
    }
    latest.map(|shape| TranscriptRead {
        shape,
        cwd,
        model,
        startup,
    })
}

fn request_model(line: &[u8]) -> Option<String> {
    let row: Value = serde_json::from_slice(line).ok()?;
    if row.get("type")?.as_str()? != "api-request" {
        return None;
    }
    Some(row.get("params")?.get("model")?.as_str()?.to_owned())
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// 줄에서 `cwd` 값만 꺼낸다. JSON 문자열 하나만 해석하고 줄 전체는 파싱하지 않는다.
fn session_cwd(line: &[u8]) -> Option<String> {
    let quoted = CWD_FIELD.captures(line)?.get(1)?.as_bytes();
    serde_json::from_slice::<String>(quoted).ok()
}

/// Claude는 작업 폴더마다 경로의 영숫자 아닌 글자를 `-`로 바꾼 이름의 폴더를 둔다.
/// (JS UTF-16 기준이라 BMP 밖 글자는 `-` 두 개.)
pub fn claude_project_dir(home: &Path, workspace: &Path) -> PathBuf {
    let name: String = workspace
        .to_string_lossy()
        .chars()
        .flat_map(|c| {
            let dashes = if c.is_ascii_alphanumeric() {
                0
            } else {
                c.len_utf16()
            };
            std::iter::repeat_n('-', dashes).chain((dashes == 0).then_some(c))
        })
        .collect();
    paths::claude_root(home).join("projects").join(name)
}

fn jsonl_files(directory: &Path) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
        .filter_map(|e| {
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            Some((e.path(), meta.modified().ok()?))
        })
        .collect()
}

/// Claude 세션 기록이 하나라도 있는가(없으면 "shape 없음" 경고가 뜻이 없다: 아직 안 쓴 것이다).
pub fn has_sessions(home: &Path) -> bool {
    !recent_transcripts(home, None, 1).is_empty()
}

/// 최근 세션 transcript, 최신순. `workspace`가 `None`이면 모든 프로젝트에서 고른다.
fn recent_transcripts(
    home: &Path,
    workspace: Option<&Path>,
    sessions_to_search: usize,
) -> Vec<PathBuf> {
    let mut files = match workspace {
        Some(workspace) => jsonl_files(&claude_project_dir(home, workspace)),
        None => std::fs::read_dir(paths::claude_root(home).join("projects"))
            .map(|entries| {
                entries
                    .flatten()
                    .flat_map(|e| jsonl_files(&e.path()))
                    .collect()
            })
            .unwrap_or_default(),
    };
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    files
        .into_iter()
        .take(sessions_to_search)
        .map(|(p, _)| p)
        .collect()
}

/// Diagnose scans recent distinct cwds even when the session has no request shape.
pub fn recent_workspaces(home: &Path, sessions: usize, count: usize) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    for file in recent_transcripts(home, None, sessions) {
        if folders.len() == count {
            break;
        }
        let Ok(file) = std::fs::File::open(file) else {
            continue;
        };
        let cwd = BufReader::new(file)
            .split(b'\n')
            .map_while(Result::ok)
            .find_map(|line| {
                let row: Value = serde_json::from_slice(&line).ok()?;
                row.get("cwd")?.as_str().map(PathBuf::from)
            });
        if let Some(cwd) =
            cwd.filter(|p| p.is_absolute() && p.is_dir() && p != home && p.parent().is_some())
            && !folders.contains(&cwd)
        {
            folders.push(cwd);
        }
    }
    folders
}

/// 최근 세션 `sessions_to_search`개 중 가장 새 shape(지난달 shape는 지금 설정을 말해 주지 않는다). 없으면 `None`.
/// `workspace`가 `Some`이면 그 프로젝트 폴더에서 작업 폴더(`cwd`)가 정확히 같은 세션만 본다
/// (폴더 이름 규칙상 `/a-b`와 `/a/b`가 같은 폴더에 섞일 수 있다).
pub fn latest(
    home: &Path,
    workspace: Option<&Path>,
    sessions_to_search: usize,
) -> Option<ShapeSummary> {
    recent_transcripts(home, workspace, sessions_to_search)
        .iter()
        .filter_map(|f| latest_shape_in(f))
        .find(|read| {
            workspace.is_none_or(|w| read.cwd.as_deref().is_some_and(|c| Path::new(c) == w))
        })
        .map(|read| ShapeSummary {
            cwd: read.cwd,
            model: read.model,
            startup: read.startup,
            ..summarise(&read.shape)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;

    fn shape_line(system: &str, tools: Value) -> String {
        json!({"type": "api-request-shape", "shape": {"system": [{"type": "text", "text": system}], "tools": tools}})
            .to_string()
    }

    #[test]
    fn summarise_splits_builtin_mcp_and_deferred_tools() {
        let tools = json!([
            {"name": "Bash", "input_schema": {}},
            {"name": "mcp__my_server__search", "input_schema": {}},
            {"name": "mcp__my_server__fetch", "input_schema": {}},
            {"name": "mcp__other__x", "description": "longer description"},
            {"name": "mcp__lazy__y", "defer_loading": true},
            {"name": "mcp__other__z", "defer_loading": true},
        ]);
        let row: Value = serde_json::from_str(&shape_line("가나", tools.clone())).unwrap();
        let summary = summarise(&row);
        let bytes = |i: usize| json_bytes(&tools[i]);
        assert_eq!(summary.system_blocks, 1);
        assert_eq!(summary.system_bytes, 6);
        assert_eq!(
            (summary.builtin_tools, summary.builtin_bytes),
            (1, bytes(0))
        );
        assert_eq!(
            summary.mcp,
            [
                McpServerShape {
                    server: "my_server".into(),
                    tools: 2,
                    bytes: bytes(1) + bytes(2)
                },
                McpServerShape {
                    server: "other".into(),
                    tools: 1,
                    bytes: bytes(3)
                },
            ]
        );
        assert_eq!(
            (summary.deferred_tools, summary.deferred_bytes),
            (2, bytes(4) + bytes(5))
        );
        // 도구를 바로 보낸 서버(`other`)는 지연 목록에 다시 넣지 않는다.
        assert_eq!(summary.deferred_mcp, ["lazy"]);
    }

    #[test]
    fn latest_shape_ignores_blob_lines_and_torn_tail() {
        let home = TempDir::new("shape-latest");
        let work = Path::new("/Users/me/work/app");
        let project = claude_project_dir(home.path(), work);
        assert!(project.ends_with("-Users-me-work-app"));
        let older = shape_line("old", json!([]));
        let newer = shape_line("newest", json!([]));
        // blob 줄은 shape처럼 생긴 내용을 담아도 이스케이프돼 있어 표시와 맞지 않는다.
        let blob = json!({"type": "api-request-blob", "content": newer.clone()}).to_string();
        // blob 안의 cwd는 이스케이프돼 있어 안 잡히고, 첫 일반 줄의 cwd를 쓴다.
        let blob_cwd =
            json!({"type": "api-request-blob", "content": "{\"cwd\":\"/secret\"}"}).to_string();
        let meta = json!({"type": "user", "cwd": "/Users/me/work/app"}).to_string();
        let text = format!(
            "{blob_cwd}\n{older}\n{meta}\n{newer}\n{blob}\n{{\"type\":\"api-request-shape\",\"sha"
        );
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("s.jsonl"), text).unwrap();
        let summary = latest(home.path(), Some(work), 5).unwrap();
        assert_eq!(summary.system_bytes, 6);
        assert_eq!(summary.cwd.as_deref(), Some("/Users/me/work/app"));
        // 전역(None)은 모든 프로젝트에서 찾는다.
        assert_eq!(latest(home.path(), None, 5).unwrap().system_bytes, 6);
        assert!(latest(home.path(), Some(Path::new("/other")), 5).is_none());
    }

    #[test]
    fn project_dir_replaces_non_alphanumerics_per_utf16_unit() {
        let dir = claude_project_dir(Path::new("/h"), Path::new("/a/한글 b/😀"));
        assert!(dir.ends_with("-a----b---"), "{dir:?}");
    }
}
