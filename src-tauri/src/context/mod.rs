//! 세션 시작 컨텍스트 측정(docs/decisions/0005). ctxmeter `scanner.js` 등을 옮긴 것.
//!
//! 읽기 전용이다. 파일 내용은 크기·front matter·@import 해석에만 쓰고, 어떤 값·경고·오류에도
//! 내용을 넣지 않는다. MCP 서버는 실행하지 않는다. TS 쪽 짝은 `src/lib/ipc.ts`의 같은 이름 타입.

mod codex_session;
pub(crate) mod fsutil;
pub(crate) mod instructions;
pub(crate) mod kiro_session;
mod loading;
pub(crate) mod paths;
pub(crate) mod scan;
pub(crate) mod shape;
mod window;

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use time::OffsetDateTime;

use crate::cache::Cache;
use crate::error::AppError;
use crate::settings::SettingsState;
use crate::tuning::Tuning;
use crate::usage::Agent;

/// 항목 종류. 화면은 이 값으로 Claude Code `/context` 묶음에 맞춰 모은다(0005 "`/context` 묶음"):
/// `System` 시스템 프롬프트, `Tools` 내장 도구, `Mcp` MCP 도구, `McpInstructions` MCP 서버 안내,
/// `Agents` 에이전트, `Instructions`·`Rules` 지시 파일(Memory files), `Skills`·`Plugins` 스킬.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextKind {
    Instructions,
    Rules,
    Skills,
    Agents,
    Hooks,
    Mcp,
    McpInstructions,
    Plugins,
    System,
    Tools,
}

/// 숫자 표기 규칙(0003). `Measured`: 에이전트가 실제로 보낸 요청의 크기(Claude `api-request-shape`, Codex `base_instructions`).
/// `Estimated`: 설정 파일 바이트 ÷ tuning `bytesPerToken`(기본 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextSource {
    Measured,
    Estimated,
}

/// 언제 컨텍스트에 들어가는가. `Startup`만 합계에 들어간다.
/// `Conditional`: 조건이 맞을 때(파일 경로·에이전트 선택·hook 이벤트). `Deferred`: 모델이 찾을 때(도구 검색·수동 steering).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Loading {
    Startup,
    Conditional,
    Deferred,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextItem {
    pub kind: ContextKind,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// `None`: 정적으로 알 수 없다(hook 출력, 실행하지 않은 MCP 서버 스키마).
    pub bytes: Option<u64>,
    /// `bytes` ÷ tuning `bytesPerToken`. 생성할 때는 비어 있고 `AgentContext::new`가 채운다.
    pub tokens: Option<u64>,
    pub source: ContextSource,
    pub loading: Loading,
}

impl ContextItem {
    /// 설정 파일 크기로 추정한 항목.
    fn estimated(
        kind: ContextKind,
        name: String,
        path: Option<&Path>,
        bytes: u64,
        loading: Loading,
    ) -> Self {
        Self {
            kind,
            name,
            path: path.map(|p| p.to_string_lossy().into_owned()),
            bytes: Some(bytes),
            tokens: None,
            source: ContextSource::Estimated,
            loading,
        }
    }

    /// 크기를 정적으로 알 수 없는 항목(개수·이름만).
    fn unknown_size(kind: ContextKind, name: String, loading: Loading) -> Self {
        Self {
            kind,
            name,
            path: None,
            bytes: None,
            tokens: None,
            source: ContextSource::Estimated,
            loading,
        }
    }

    /// 실제로 보낸 요청에서 잰 항목. 요청에 들어 있었으므로 시작 시점이다.
    fn measured(kind: ContextKind, name: String, bytes: u64) -> Self {
        Self {
            kind,
            name,
            path: None,
            bytes: Some(bytes),
            tokens: None,
            source: ContextSource::Measured,
            loading: Loading::Startup,
        }
    }

    /// 요청에 실렸지만 `defer_loading`이라 모델 컨텍스트에는 찾을 때만 들어가는 항목(`/context`도 합계에서 뺀다).
    fn measured_deferred(kind: ContextKind, name: String, bytes: u64) -> Self {
        Self {
            loading: Loading::Deferred,
            ..Self::measured(kind, name, bytes)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContext {
    pub agent: Agent,
    /// 시작 → 조건부 → 지연 순, 같은 시점 안에서는 토큰이 큰 순(모름은 끝).
    pub items: Vec<ContextItem>,
    /// `loading == Startup`이고 토큰을 아는 항목의 합.
    pub total_startup_tokens: u64,
    /// 첫 요청의 실제 토크나이저 입력 토큰(Codex `token_count`). 첫 사용자 메시지가 들어 있어
    /// 시작 합계와 같은 양이 아니다. 모르면 `None`.
    pub first_request_tokens: Option<u64>,
    /// Kiro 세션 시작에 조립된 문자 수(실측). 토큰 수가 아니다. 모르면 `None`.
    pub first_request_chars: Option<u64>,
    /// 실측값을 잰 세션의 모델(Claude `api-request.params.model`, Codex `turn_context.model`). 모르면 `None`.
    pub model: Option<String>,
    /// 그 모델의 컨텍스트 창(토큰). 시작 합계 ÷ 이 값 = 창 몫. 모르면 `None`(화면은 %를 숨긴다).
    pub context_window: Option<u64>,
    /// 세지 못한 것과 그 이유. 사용자에게 그대로 보여 줄 문장.
    pub warnings: Vec<String>,
}

impl AgentContext {
    /// 크기를 아는 항목의 토큰을 `bytes_per_token`으로 어림해 채우고 정렬·합산한다.
    fn new(
        agent: Agent,
        items: Vec<ContextItem>,
        warnings: Vec<String>,
        bytes_per_token: u64,
    ) -> Self {
        let mut items: Vec<ContextItem> = items
            .into_iter()
            .map(|item| ContextItem {
                tokens: item.bytes.map(|b| fsutil::estimate(b, bytes_per_token)),
                ..item
            })
            .collect();
        items.sort_by(|a, b| {
            a.loading
                .cmp(&b.loading)
                .then_with(|| b.tokens.is_some().cmp(&a.tokens.is_some()))
                .then_with(|| b.tokens.cmp(&a.tokens))
        });
        let total_startup_tokens = items
            .iter()
            .filter(|i| i.loading == Loading::Startup)
            .filter_map(|i| i.tokens)
            .sum();
        Self {
            agent,
            items,
            total_startup_tokens,
            first_request_tokens: None,
            first_request_chars: None,
            model: None,
            context_window: None,
            warnings,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSnapshot {
    /// 이 값을 잰 시각(RFC 3339).
    #[serde(with = "time::serde::rfc3339")]
    pub computed_at: OffsetDateTime,
    pub agents: Vec<AgentContext>,
}

/// 절대 경로이고 `..`이 없는, 존재하는 폴더만 받는다.
fn validate_workspace(raw: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(raw);
    if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
        return Err(AppError::InvalidInput(
            crate::i18n::pick(
                "작업 폴더는 `..` 없는 절대 경로여야 합니다.",
                "The folder must be an absolute path without `..`.",
            )
            .into(),
        ));
    }
    if !path.is_dir() {
        return Err(AppError::InvalidInput(
            crate::i18n::pick(
                "작업 폴더가 없거나 폴더가 아닙니다.",
                "The folder doesn't exist or isn't a folder.",
            )
            .into(),
        ));
    }
    // 끝의 `/`·`.`을 정리한다. 링크는 풀지 않는다(Claude 프로젝트 폴더 이름이 입력 경로 기준이다).
    Ok(path.components().collect())
}

/// `env`: Claude 도구 검색 판정에 쓰는 실행 환경 변수(테스트는 빈 값). `tuning`: 세션 탐색 범위·토큰 어림 비율.
pub fn measure(
    home: &Path,
    workspace: Option<&Path>,
    env: &HashMap<String, String>,
    tuning: &Tuning,
) -> ContextSnapshot {
    ContextSnapshot {
        computed_at: OffsetDateTime::now_utc(),
        agents: Agent::ALL
            .into_iter()
            .map(|agent| match agent {
                Agent::Claude => scan::claude(home, workspace, env, tuning),
                Agent::Codex => scan::codex(home, workspace, tuning),
                Agent::Kiro => scan::kiro(home, workspace, tuning),
            })
            .collect(),
    }
}

pub const CONTEXT_UPDATED_EVENT: &str = "context-updated";

/// 전역 설정만 보는 측정(`workspace == None`)의 마지막 결과. 작업 폴더를 준 호출은 캐시하지 않는다.
pub struct ContextState(Cache<ContextSnapshot>);

impl Default for ContextState {
    fn default() -> Self {
        Self(Cache::new())
    }
}

fn measure_with_process_env(
    home: &Path,
    workspace: Option<&Path>,
    tuning: &Tuning,
) -> ContextSnapshot {
    measure(home, workspace, &std::env::vars().collect(), tuning)
}

/// `is_forced`(새로고침)면 TTL을 0으로 보고 바로 다시 잰다.
async fn global_context(app: AppHandle, is_forced: bool) -> Result<ContextSnapshot, AppError> {
    let home = std::env::home_dir().ok_or(AppError::Internal)?;
    let cache = app.state::<ContextState>().0.clone();
    // 다음 측정부터 바뀐 값을 쓴다. 이미 캐시된 값은 다음 갱신 때 바뀐다.
    let tuning = app.state::<SettingsState>().tuning();
    let ttl = if is_forced {
        std::time::Duration::ZERO
    } else {
        tuning.context_ttl()
    };
    cache
        .get(
            ttl,
            move || measure_with_process_env(&home, None, &tuning),
            move |snapshot| {
                if app.emit(CONTEXT_UPDATED_EVENT, snapshot).is_err() {
                    eprintln!("kcal: context-updated event 전송 실패");
                }
            },
        )
        .await
}

/// 전역 측정의 마지막 결과(없으면 잰다). 5층 코칭이 스위치 토큰을 이 값에서 이름으로 맞춘다(0014).
pub async fn global_snapshot(app: &AppHandle) -> Result<ContextSnapshot, AppError> {
    global_context(app.clone(), false).await
}

/// 에이전트별 세션 시작 컨텍스트. `workspace`가 `None`이면 전역 설정만 보며, 마지막 결과를 바로
/// 돌려주고 오래됐으면 뒤에서 다시 재서 `context-updated`로 보낸다. 파일·세션 기록을 훑으므로 blocking 스레드에서 한다.
#[tauri::command]
pub async fn get_context(
    app: AppHandle,
    workspace: Option<String>,
) -> Result<ContextSnapshot, AppError> {
    let Some(raw) = workspace else {
        return global_context(app, false).await;
    };
    let workspace = validate_workspace(&raw)?;
    let home = std::env::home_dir().ok_or(AppError::Internal)?;
    let tuning = app.state::<SettingsState>().tuning();
    tauri::async_runtime::spawn_blocking(move || {
        measure_with_process_env(&home, Some(&workspace), &tuning)
    })
    .await
    .map_err(|_| AppError::Internal)
}

/// 앱 시작 때 한 번 미리 잰다. `is_forced`면 캐시가 새것이어도 다시 잰다(새로고침).
pub fn prewarm(app: AppHandle, is_forced: bool) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = global_context(app, is_forced).await {
            eprintln!("kcal: 컨텍스트 측정 실패({e:?})");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsutil::testing::TempDir;
    use std::path::MAIN_SEPARATOR as SEP;

    #[test]
    fn workspace_must_be_an_existing_absolute_directory() {
        let dir = TempDir::new("validate");
        let file = dir.write("file.txt", "x");
        assert!(validate_workspace("relative/path").is_err());
        assert!(validate_workspace(&format!("{}{SEP}..{SEP}x", dir.path().display())).is_err());
        assert!(validate_workspace(&file.to_string_lossy()).is_err());
        assert!(validate_workspace(&dir.path().join("missing").to_string_lossy()).is_err());
        assert_eq!(
            validate_workspace(&format!("{}{SEP}", dir.path().display())).unwrap(),
            dir.path()
        );
    }

    #[test]
    fn startup_total_skips_conditional_and_unknown_items() {
        let items = vec![
            ContextItem::estimated(
                ContextKind::Agents,
                "a".into(),
                None,
                400,
                Loading::Conditional,
            ),
            ContextItem::unknown_size(ContextKind::Hooks, "h".into(), Loading::Startup),
            ContextItem::measured(ContextKind::System, "s".into(), 800),
            ContextItem::estimated(ContextKind::Rules, "r".into(), None, 40, Loading::Startup),
        ];
        let context = AgentContext::new(Agent::Claude, items, Vec::new(), 4);
        assert_eq!(context.total_startup_tokens, 210);
        let names: Vec<&str> = context.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["s", "r", "h", "a"]);
    }

    #[test]
    fn tokens_follow_the_bytes_per_token_setting() {
        let items = || {
            vec![
                ContextItem::estimated(ContextKind::Rules, "r".into(), None, 400, Loading::Startup),
                ContextItem::measured(ContextKind::System, "s".into(), 800),
            ]
        };
        let tokens = |bytes_per_token| {
            let context = AgentContext::new(Agent::Claude, items(), Vec::new(), bytes_per_token);
            (
                context.items.iter().map(|i| i.tokens).collect::<Vec<_>>(),
                context.total_startup_tokens,
            )
        };
        assert_eq!(tokens(4), (vec![Some(200), Some(100)], 300));
        assert_eq!(tokens(8), (vec![Some(100), Some(50)], 150));
    }

    #[test]
    fn serializes_item_with_camel_case_and_lowercase_enums() {
        let item = ContextItem::estimated(
            ContextKind::Instructions,
            "CLAUDE.md".into(),
            Some(Path::new("/h/.claude/CLAUDE.md")),
            8,
            Loading::Startup,
        );
        let context = AgentContext::new(Agent::Claude, vec![item], Vec::new(), 4);
        assert_eq!(
            serde_json::to_value(&context.items[0]).unwrap(),
            serde_json::json!({
                "kind": "instructions", "name": "CLAUDE.md", "path": "/h/.claude/CLAUDE.md",
                "bytes": 8, "tokens": 2, "source": "estimated", "loading": "startup"
            })
        );
    }
}
