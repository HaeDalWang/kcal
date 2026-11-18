//! Codex 시작 컨텍스트 읽기(`scan.rs`에서 나눔).

use super::*;

static SECTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\[([^\]]+)\]").unwrap());

/// 진짜 table 헤더의 section 이름. 여러 줄 배열 안의 `["a"],` 같은 줄은 헤더가 아니다(편집기와 같은 판정을 쓴다).
fn section_headers(config: &str) -> Vec<Option<String>> {
    let lines: Vec<&str> = config.split('\n').collect();
    let mut sections = vec![None; lines.len()];
    for i in crate::coach::edit::header_indices(&lines) {
        sections[i] = SECTION.captures(lines[i]).map(|c| c[1].to_owned());
    }
    sections
}

/// Codex `config.toml`의 켜고 끄는 section 하나(5층 코칭이 같은 결과로 스위치 목록을 만든다, 0014).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TomlSwitchState {
    pub name: String,
    /// 파일에 적힌 그대로의 section 이름(`plugins."a@m"`, `mcp_servers.one`). 고칠 때 이 헤더를 찾는다.
    pub section: String,
    pub enabled: bool,
}

/// `[plugins."<id>"]` 전부와 켜짐 여부. `enabled = true`가 없으면 꺼짐.
pub(crate) fn codex_plugin_states(config: &str) -> Vec<TomlSwitchState> {
    static PLUGIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^plugins\."(.+)"$"#).unwrap());
    static ENABLED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*enabled\s*=\s*true(?:\s*(?:#.*)?)?$").unwrap());
    let mut states: Vec<TomlSwitchState> = Vec::new();
    let mut current: Option<usize> = None;
    let headers = section_headers(config);
    for (line, header) in config.split('\n').zip(&headers) {
        if let Some(section) = header {
            current = PLUGIN.captures(section).map(|c| {
                states.push(TomlSwitchState {
                    name: c[1].to_owned(),
                    section: section.to_owned(),
                    enabled: false,
                });
                states.len() - 1
            });
        } else if let Some(i) = current
            && ENABLED.is_match(line)
        {
            states[i].enabled = true;
        }
    }
    states
}

pub(super) fn codex_enabled_plugins(config: &str) -> Vec<String> {
    codex_plugin_states(config)
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| s.name)
        .collect()
}

/// `[mcp_servers.<name>]` 전부와 켜짐 여부. `enabled = false`가 없으면 켜짐(플러그인과 반대 기본값).
pub(crate) fn codex_mcp_server_states(config: &str) -> Vec<TomlSwitchState> {
    static SERVER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^mcp_servers\.(?:"([^"]+)"|([^.]+))$"#).unwrap());
    static DISABLED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*enabled\s*=\s*false(?:\s*(?:#.*)?)?$").unwrap());
    let mut states = Vec::new();
    let mut current: Option<TomlSwitchState> = None;
    let headers = section_headers(config);
    for (line, header) in config.split('\n').zip(&headers) {
        if let Some(section) = header {
            states.extend(current.take());
            // `.env` 같은 하위 section은 이름을 비워 그 안의 키를 서버 키로 읽지 않는다.
            current = SERVER
                .captures(section)
                .and_then(|c| c.get(1).or(c.get(2)))
                .map(|m| TomlSwitchState {
                    name: m.as_str().to_owned(),
                    section: section.to_owned(),
                    enabled: true,
                });
        } else if let Some(state) = current.as_mut()
            && DISABLED.is_match(line)
        {
            state.enabled = false;
        }
    }
    states.extend(current);
    states
}

pub(super) fn codex_enabled_mcp_servers(config: &str) -> Vec<String> {
    codex_mcp_server_states(config)
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| s.name)
        .collect()
}

/// Codex가 skill을 싣는 위치, 이긴 순서(0005). `/etc/codex/skills`(관리자)는 넣지 않았다.
fn codex_skill_places(home: &Path, workspace: Option<&Path>) -> Vec<(&'static str, Vec<PathBuf>)> {
    let root = paths::codex_root(home);
    let repo = workspace
        .map(|w| {
            instructions::codex_project_directories(w)
                .into_iter()
                .rev()
                .map(|d| d.join(".agents").join("skills"))
                .collect()
        })
        .unwrap_or_default();
    vec![
        ("repo-skills", repo),
        (
            "home-agent-skills",
            vec![home.join(".agents").join("skills")],
        ),
        ("agent-skills", vec![root.join(".agents").join("skills")]),
        ("user-and-system-skills", vec![root.join("skills")]),
    ]
}

pub fn codex(home: &Path, workspace: Option<&Path>, tuning: &Tuning) -> AgentContext {
    let root = paths::codex_root(home);
    let config = fsutil::read_text(&root.join("config.toml"));
    let found = instructions::codex(home, workspace);
    let mut items = found.items;
    let mut warnings = Vec::new();
    if found.truncated {
        warnings.push(
            pick(
                "AGENTS.md 파일이 Codex의 크기 제한(project_doc_max_bytes)에서 잘렸어요.",
                "AGENTS.md files were cut off by Codex's size limit (project_doc_max_bytes).",
            )
            .into(),
        );
    }
    items.extend(file_items(
        ContextKind::Agents,
        &root.join("agents"),
        ".toml",
        "",
        Loading::Conditional,
    ));
    items.extend(named_skill_items(&codex_skill_places(home, workspace)));
    for id in codex_enabled_plugins(&config) {
        match plugin_cache_directory(&root, &id) {
            Some(directory) => items.extend(skill_item(
                ContextKind::Plugins,
                &id,
                &directory.join("skills"),
                PLUGIN_CACHE,
            )),
            None => warnings.push(plugin_missing(&id)),
        }
    }
    let hooks = read_json(&root.join("hooks.json")).unwrap_or(Value::Null);
    hook_item(count_hooks(hooks.get("hooks")), &mut items, &mut warnings);
    let servers = codex_enabled_mcp_servers(&config)
        .into_iter()
        .map(|name| (name, Loading::Deferred))
        .collect();
    mcp_items(servers, &mut items, &mut warnings);

    // base_instructions는 Codex 내장 프롬프트라 AGENTS.md·skill 추정 항목과 겹치지 않는다(0005).
    let sent = codex_session::latest(home, workspace, tuning.codex_sessions_to_search);
    if let Some(sent) = &sent {
        if workspace.is_none() {
            warnings.push(global_session_note("Codex", sent.cwd.as_deref()));
        }
        if let Some(bytes) = sent.system_bytes {
            items.push(ContextItem::measured(
                ContextKind::System,
                pick("시스템 지시문", "System instructions").into(),
                bytes,
            ));
        }
    }
    let mut context = AgentContext::new(Agent::Codex, items, warnings, tuning.bytes_per_token);
    if let Some(sent) = sent {
        context.first_request_tokens = sent.first_request_tokens;
        context.model = sent.model;
        context.context_window = sent.context_window;
    }
    context
}
