//! 사용자 설정(docs/decisions/0004 "설정"). `app_config_dir()/settings.json`에 저장한다.
//! TS 쪽 짝은 `src/lib/ipc.ts`의 `Settings`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::Notify;

use crate::error::AppError;
use crate::i18n::Lang;
use crate::tr;
use crate::tuning::{Tuning, TuningFieldSpec};
use crate::usage::Agent;

pub const SETTINGS_UPDATED_EVENT: &str = "settings-updated";
const FILE_NAME: &str = "settings.json";

/// 파일 형식: `{ "enabledAgents": ["claude", "codex", "kiro"], "tuning": { ... }, "language": "auto", "resolvedLanguage": "en" }`.
/// `tuning`은 없어도 되고(기본값), 필드 단위 검증은 `Tuning::from_json`이 한다. 언어 두 칸도 없어도 된다(0021).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsFile {
    enabled_agents: Vec<String>,
    /// 저장할 때 이미 본(설치돼 있던) 에이전트. 없으면(옛 파일) 모두 본 것으로 친다.
    #[serde(default)]
    seen_agents: Option<Vec<String>>,
}

/// 설정 화면의 언어 선택(0021). TS 짝은 `src/lib/ipc.ts`의 `LanguageChoice`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// OS(웹뷰 `navigator.language`)를 따른다. 정하는 쪽은 프론트엔드다.
    #[default]
    Auto,
    Ko,
    En,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 팝오버(탭·개요)·트레이 값에 표시하고 조회하는 에이전트. `Agent::ALL` 순서, 1개 이상.
    pub enabled_agents: Vec<Agent>,
    /// 조절 가능한 값(주기·시간 초과·탐색 범위 등). 0007.
    pub tuning: Tuning,
    /// tuning 필드별 범위·기본값(`Tuning::spec`). 설정 화면이 쓴다. 저장하지 않는다.
    pub tuning_spec: Vec<TuningFieldSpec>,
    /// 파일을 못 읽어 기본값을 쓰는 이유. 저장에 성공하면 사라진다.
    pub reason: Option<String>,
    /// 언어 선택. 기본은 자동.
    pub language: Language,
    /// 프론트엔드가 마지막으로 알려 준 화면 언어(`set_ui_language`). 자동일 때 Rust 문구가 이 언어를 쓴다.
    pub resolved_language: Lang,
    /// 이미 본 에이전트. 여기 없는 에이전트가 새로 설치되면 다음 시작에 저절로 켠다. 화면에는 안 보낸다.
    #[serde(skip)]
    pub seen_agents: Vec<Agent>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled_agents: Agent::ALL.to_vec(),
            tuning: Tuning::default(),
            tuning_spec: Tuning::spec(),
            reason: None,
            language: Language::Auto,
            resolved_language: Lang::default(),
            seen_agents: Agent::ALL.to_vec(),
        }
    }
}

impl Settings {
    pub fn is_enabled(&self, agent: Agent) -> bool {
        self.enabled_agents.contains(&agent)
    }

    /// Rust 문구의 언어: 고른 언어, 자동이면 마지막으로 알려 받은 언어.
    pub fn effective_language(&self) -> Lang {
        match self.language {
            Language::Ko => Lang::Ko,
            Language::En => Lang::En,
            Language::Auto => self.resolved_language,
        }
    }
}

/// 파일의 언어 두 칸. 칸이 없거나 값이 틀리면 기본값. `resolvedLanguage`가 없는 파일은 0021 전 버전(한국어 화면만
/// 있었다)이 쓴 것이라 한국어로 본다.
fn languages(file: Option<&Value>) -> (Language, Lang) {
    fn read<T: serde::de::DeserializeOwned>(
        file: &Value,
        key: &str,
    ) -> Option<serde_json::Result<T>> {
        file.get(key).cloned().map(serde_json::from_value)
    }
    let Some(file) = file else {
        return (Language::Auto, Lang::default());
    };
    let language = read(file, "language")
        .and_then(Result::ok)
        .unwrap_or_default();
    let resolved = match read(file, "resolvedLanguage") {
        Some(Ok(lang)) => lang,
        Some(Err(_)) => Lang::default(),
        None => Lang::Ko,
    };
    (language, resolved)
}

/// 에이전트 id 목록을 검증해 `Agent::ALL` 순서로 돌려준다. 빈 목록·알 수 없는 id는 거부한다.
pub fn validate_agents(ids: &[String]) -> Result<Vec<Agent>, String> {
    if let Some(unknown) = ids.iter().find(|id| Agent::from_id(id).is_none()) {
        return Err(tr!(
            "알 수 없는 에이전트: {unknown}",
            "Unknown agent: {unknown}"
        ));
    }
    let agents = agents_in_order(ids);
    if agents.is_empty() {
        return Err(tr!(
            "에이전트를 하나 이상 켜야 합니다",
            "Turn on at least one agent"
        ));
    }
    Ok(agents)
}

/// 파일이 없으면 기본값, 깨졌으면 기본값 + reason. `tuning`은 JSON이 읽히면 에이전트 목록이
/// 틀려도 필드 단위로 살린다.
fn load(path: &Path) -> Settings {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(error) => {
            let kind = error.kind();
            return broken(tr!(
                "설정 파일을 읽지 못했습니다({kind})",
                "Couldn't read the settings file ({kind})"
            ));
        }
    };
    let parsed = serde_json::from_str::<SettingsFile>(&text)
        .map_err(|_| {
            tr!(
                "설정 파일 형식이 잘못됐습니다",
                "The settings file is malformed"
            )
        })
        .and_then(|file| {
            let seen = file
                .seen_agents
                .map(|ids| agents_in_order(&ids))
                .unwrap_or_else(|| Agent::ALL.to_vec());
            validate_agents(&file.enabled_agents).map(|enabled| (enabled, seen))
        });
    let file = serde_json::from_str::<Value>(&text).ok();
    let tuning = file
        .as_ref()
        .and_then(|file| file.get("tuning").map(Tuning::from_json))
        .unwrap_or_default();
    let (language, resolved_language) = languages(file.as_ref());
    let base = match parsed {
        Ok((enabled_agents, seen_agents)) => Settings {
            enabled_agents,
            seen_agents,
            ..Settings::default()
        },
        Err(why) => broken(why),
    };
    Settings {
        tuning,
        language,
        resolved_language,
        ..base
    }
}

/// 첫 실행(설정 파일 없음)에 보일 에이전트: 이 컴퓨터에 있는 것만. 하나도 없으면 Claude(대상 사용자).
/// 파일이 생기기 전까지는 켤 때마다 다시 정해서, 나중에 설치한 에이전트도 저절로 보인다.
fn first_run_agents(home: &Path, path_env: Option<&std::ffi::OsStr>) -> Vec<Agent> {
    let found: Vec<Agent> = Agent::ALL
        .into_iter()
        .filter(|&agent| crate::usage::sources::is_installed(agent, home, path_env))
        .collect();
    if found.is_empty() {
        vec![Agent::Claude]
    } else {
        found
    }
}

/// `load` + 첫 실행이면 있는 에이전트만 켜고, 파일이 있으면 저장 뒤 새로 설치된 에이전트를 더 켠다.
/// 사용자가 끈 에이전트(이미 본 것)는 그대로 둔다. 홈을 모르면 파일(또는 기본값 모두) 그대로.
fn load_first_run(
    path: &Path,
    home: Option<&Path>,
    path_env: Option<&std::ffi::OsStr>,
) -> Settings {
    let settings = load(path);
    let Some(home) = home else { return settings };
    if !path.exists() {
        let found = first_run_agents(home, path_env);
        return Settings {
            seen_agents: found.clone(),
            enabled_agents: found,
            ..settings
        };
    }
    let installed: Vec<Agent> = Agent::ALL
        .into_iter()
        .filter(|&a| crate::usage::sources::is_installed(a, home, path_env))
        .collect();
    let is_on = |a: &Agent| {
        settings.enabled_agents.contains(a)
            || (installed.contains(a) && !settings.seen_agents.contains(a))
    };
    let is_seen = |a: &Agent| settings.seen_agents.contains(a) || installed.contains(a);
    Settings {
        enabled_agents: Agent::ALL.into_iter().filter(is_on).collect(),
        seen_agents: Agent::ALL.into_iter().filter(is_seen).collect(),
        ..settings
    }
}

/// 아는 id만 `Agent::ALL` 순서로. 모르는 id는 버린다(이미 본 목록은 틀려도 설정을 깨지 않는다).
fn agents_in_order(ids: &[String]) -> Vec<Agent> {
    Agent::ALL
        .into_iter()
        .filter(|a| ids.iter().any(|id| Agent::from_id(id) == Some(*a)))
        .collect()
}

fn broken(why: String) -> Settings {
    Settings {
        reason: Some(tr!(
            "{why}. 기본값(모두 표시)을 씁니다.",
            "{why}. Using the defaults (show all)."
        )),
        ..Settings::default()
    }
}

/// 임시 파일에 쓴 뒤 rename한다(쓰다 끊겨도 기존 파일이 남는다).
fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let ids: Vec<&str> = settings.enabled_agents.iter().map(|a| a.id()).collect();
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "enabledAgents": ids,
        "seenAgents": settings.seen_agents.iter().map(|a| a.id()).collect::<Vec<_>>(),
        "tuning": settings.tuning,
        "language": settings.language,
        "resolvedLanguage": settings.resolved_language,
    }))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

pub struct SettingsState {
    path: PathBuf,
    current: Mutex<Settings>,
    /// 시작할 때 설정 파일이 없었다(첫 실행). 저장해도 바뀌지 않는다.
    first_run: bool,
    /// 설정이 바뀌면 조회 타이머를 깨운다(다시 켠 에이전트를 바로 조회, 바뀐 간격을 다시 읽음).
    pub changed: Arc<Notify>,
}

impl SettingsState {
    #[cfg(test)]
    pub fn load(path: PathBuf) -> Self {
        Self::with(path.clone(), load(&path))
    }

    fn with(path: PathBuf, settings: Settings) -> Self {
        Self {
            first_run: !path.exists() && !path.with_extension("seen").exists(),
            path,
            current: Mutex::new(settings),
            changed: Arc::new(Notify::new()),
        }
    }

    /// 앱 시작용. 파일의 언어로 Rust 문구 언어를 정하고, 바뀌었으면 그 언어로 다시 읽는다(`reason` 문구).
    pub fn from_app<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Self> {
        let path = app.path().app_config_dir()?.join(FILE_NAME);
        let home = std::env::home_dir();
        let path_env = std::env::var_os("PATH");
        let state = Self::with(
            path.clone(),
            load_first_run(&path, home.as_deref(), path_env.as_deref()),
        );
        let lang = state
            .get()
            .map(|s| s.effective_language())
            .unwrap_or_default();
        Ok(if crate::i18n::set(lang) {
            Self::with(
                path.clone(),
                load_first_run(&path, home.as_deref(), path_env.as_deref()),
            )
        } else {
            state
        })
    }

    pub fn is_first_run(&self) -> bool {
        self.first_run
    }

    /// 첫 실행 팝오버를 열었다고 표시한다. settings.json을 쓰지 않는 이유: 쓰면 첫 실행 기본값
    /// (설치된 에이전트 감지)이 굳어 나중에 설치한 에이전트가 안 보인다.
    pub fn mark_seen(&self) {
        if let Err(error) = std::fs::create_dir_all(self.path.parent().unwrap_or(Path::new(".")))
            .and_then(|_| std::fs::write(self.path.with_extension("seen"), ""))
        {
            eprintln!("kcal: 첫 실행 표시 저장 실패: {}", error.kind());
        }
    }

    pub fn get(&self) -> Result<Settings, AppError> {
        self.current
            .lock()
            .map(|s| s.clone())
            .map_err(|_| AppError::Internal)
    }

    /// 지금 tuning. 잠금이 깨졌으면 기본값(조회·계산이 멈추지 않게).
    pub fn tuning(&self) -> Tuning {
        self.get().map(|s| s.tuning).unwrap_or_default()
    }

    /// 검증 → 파일 저장 → state 갱신. 잠금은 저장이 끝난 뒤에만 잡는다.
    /// `tuning`·`language`가 `None`이면 지금 값을 그대로 둔다.
    fn set(
        &self,
        ids: &[String],
        tuning: Option<Tuning>,
        language: Option<Language>,
    ) -> Result<Settings, AppError> {
        let enabled_agents = validate_agents(ids).map_err(AppError::InvalidInput)?;
        let now = self.get()?;
        let tuning = match tuning {
            Some(next) => {
                next.validate().map_err(AppError::InvalidInput)?;
                next
            }
            None => now.tuning,
        };
        let next = Settings {
            enabled_agents,
            tuning,
            language: language.unwrap_or(now.language),
            resolved_language: now.resolved_language,
            seen_agents: now.seen_agents,
            ..Settings::default()
        };
        self.store(next, true)
    }

    /// 프론트엔드가 정한 화면 언어를 남긴다. 같으면 파일을 쓰지 않는다. 설정 변경 알림(타이머 깨우기)은 하지 않는다.
    fn set_resolved(&self, lang: Lang) -> Result<(), AppError> {
        let now = self.get()?;
        if now.resolved_language == lang {
            return Ok(());
        }
        let next = Settings {
            resolved_language: lang,
            reason: None,
            ..now
        };
        self.store(next, false).map(|_| ())
    }

    fn store(&self, next: Settings, is_change: bool) -> Result<Settings, AppError> {
        save(&self.path, &next).map_err(|error| {
            eprintln!("kcal: 설정 저장 실패: {}", error.kind());
            AppError::Internal
        })?;
        *self.current.lock().map_err(|_| AppError::Internal)? = next.clone();
        if is_change {
            self.changed.notify_one();
        }
        Ok(next)
    }
}

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, SettingsState>) -> Result<Settings, AppError> {
    state.get()
}

/// 켤 에이전트 id 목록과 (선택) tuning 전체·언어 선택을 받아 저장하고 트레이 title·tooltip을 바로 맞춘다.
/// `tuning`을 생략하면 지금 값을 유지하고, 주면 통째로 바꾼다(빠진 필드는 기본값). 범위 밖이면 `invalid_input`.
/// 언어 선택을 바꿔도 Rust 문구 언어는 프론트엔드가 이어서 부르는 `set_ui_language`가 바꾼다.
#[tauri::command]
pub fn set_settings<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, SettingsState>,
    enabled_agents: Vec<String>,
    tuning: Option<Tuning>,
    language: Option<Language>,
) -> Result<Settings, AppError> {
    let next = state.set(&enabled_agents, tuning, language)?;
    crate::tray::refresh(&app);
    if app.emit(SETTINGS_UPDATED_EVENT, &next).is_err() {
        eprintln!("kcal: {SETTINGS_UPDATED_EVENT} event 전송 실패");
    }
    Ok(next)
}

/// 프론트엔드가 정한 화면 언어(0021). 남겨 두고(다음 시작 때 그 언어로 시작), 바뀌었으면 Rust 문구를 다시 만든다:
/// 트레이 메뉴·tooltip·대시보드 창 제목, 컨텍스트 항목 이름(다시 측정), 사용량 실패 이유(바로 다시 조회, 최소 간격은 건너뛰고 429 backoff는 지킨다).
/// 점검 카드 선택지는 `get_checks`가 부를 때마다 만들어 따로 할 일이 없다.
#[tauri::command]
pub fn set_ui_language(
    app: AppHandle,
    state: tauri::State<'_, SettingsState>,
    language: Lang,
) -> Result<(), AppError> {
    state.set_resolved(language)?;
    if crate::i18n::set(language) {
        crate::tray::relabel(&app);
        crate::context::prewarm(app.clone(), true);
        crate::usage::schedule::request_language_refresh(&app);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kcal-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    #[test]
    fn rejects_empty_agent_list() {
        assert!(validate_agents(&[]).is_err());
    }

    #[test]
    fn rejects_unknown_agent() {
        let error = validate_agents(&ids(&["claude", "gemini"])).unwrap_err();
        assert!(error.contains("gemini"));
    }

    #[test]
    fn orders_and_dedups_agents() {
        assert_eq!(
            validate_agents(&ids(&["kiro", "claude", "kiro"])).unwrap(),
            [Agent::Claude, Agent::Kiro]
        );
    }

    #[test]
    fn missing_file_loads_defaults_without_reason() {
        assert_eq!(load(&temp_file("missing")), Settings::default());
    }

    #[test]
    fn first_run_flag_is_true_without_settings_file_and_false_with_one() {
        let path = temp_file("first-run-flag");
        let state = SettingsState::load(path.clone());
        assert!(state.is_first_run());
        // 저장해도 이번 실행의 값은 그대로다(팝오버를 한 번만 열기 위해).
        state.set(&ids(&["claude"]), None, None).unwrap();
        assert!(state.is_first_run());
        assert!(!SettingsState::load(path.clone()).is_first_run());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn mark_seen_ends_first_run_without_writing_settings() {
        let path = temp_file("first-run-seen");
        let state = SettingsState::load(path.clone());
        assert!(state.is_first_run());
        state.mark_seen();
        assert!(!path.exists());
        assert!(!SettingsState::load(path.clone()).is_first_run());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn first_run_shows_only_installed_agents_and_claude_when_none() {
        let home = std::env::temp_dir().join(format!("kcal-settings-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let path = temp_file("first-run");
        let empty_path = Some(std::ffi::OsStr::new(""));
        assert_eq!(first_run_agents(&home, empty_path), [Agent::Claude]);
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        assert_eq!(first_run_agents(&home, empty_path), [Agent::Codex]);
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        assert_eq!(
            first_run_agents(&home, empty_path),
            [Agent::Claude, Agent::Codex]
        );
        // 파일이 있으면 사용자가 고른 목록이 그대로다.
        let state = SettingsState::load(path.clone());
        state.set(&ids(&["kiro"]), None, None).unwrap();
        assert_eq!(
            load_first_run(&path, Some(&home), empty_path).enabled_agents,
            [Agent::Kiro]
        );
        // 저장할 때 못 본 에이전트가 나중에 설치되면 켜고, 본 것 중 끈 것은 그대로 둔다.
        std::fs::write(
            &path,
            r#"{"enabledAgents":["claude"],"seenAgents":["claude"]}"#,
        )
        .unwrap();
        let later = load_first_run(&path, Some(&home), empty_path);
        assert_eq!(later.enabled_agents, [Agent::Claude, Agent::Codex]);
        assert_eq!(later.seen_agents, [Agent::Claude, Agent::Codex]);
        let saved = SettingsState::with(path.clone(), later);
        saved.set(&ids(&["claude"]), None, None).unwrap();
        assert_eq!(
            load_first_run(&path, Some(&home), empty_path).enabled_agents,
            [Agent::Claude]
        );
        // 이미 본 목록이 없는 옛 파일은 모두 본 것으로 쳐서 아무것도 더 켜지 않는다.
        std::fs::write(&path, r#"{"enabledAgents":["claude"]}"#).unwrap();
        assert_eq!(
            load_first_run(&path, Some(&home), empty_path).enabled_agents,
            [Agent::Claude]
        );
        let _ = std::fs::remove_dir_all(home);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn broken_file_loads_defaults_with_reason() {
        let path = temp_file("broken");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for text in [
            "{not json",
            r#"{"enabledAgents":[]}"#,
            r#"{"enabledAgents":["x"]}"#,
        ] {
            std::fs::write(&path, text).unwrap();
            let settings = load(&path);
            assert_eq!(settings.enabled_agents, Agent::ALL);
            assert!(settings.reason.is_some(), "{text}");
        }
    }

    #[test]
    fn saved_settings_load_back_and_clear_reason() {
        let path = temp_file("roundtrip");
        let state = SettingsState::load(path.clone());
        let saved = state.set(&ids(&["codex"]), None, None).unwrap();
        assert_eq!(saved.enabled_agents, [Agent::Codex]);
        assert_eq!(load(&path), saved);
        assert!(!path.with_extension("json.tmp").exists());
        assert!(matches!(
            state.set(&[], None, None),
            Err(AppError::InvalidInput(_))
        ));
        assert_eq!(state.get().unwrap(), saved);
    }

    #[test]
    fn file_without_tuning_loads_default_tuning() {
        let path = temp_file("no-tuning");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"enabledAgents":["claude"]}"#).unwrap();
        let settings = load(&path);
        assert_eq!(settings.enabled_agents, [Agent::Claude]);
        assert_eq!(settings.tuning, Tuning::default());
        assert_eq!(settings.reason, None);
    }

    #[test]
    fn old_file_without_language_loads_as_auto_with_korean_last_language() {
        let path = temp_file("no-language");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"enabledAgents":["claude"]}"#).unwrap();
        let settings = load(&path);
        assert_eq!(settings.language, Language::Auto);
        assert_eq!(settings.resolved_language, Lang::Ko);
        assert_eq!(settings.effective_language(), Lang::Ko);
        assert_eq!(settings.reason, None);
        // 처음 켤 때(파일 없음)는 영어로 시작한다.
        assert_eq!(load(&temp_file("fresh")).effective_language(), Lang::En);
    }

    #[test]
    fn language_choice_and_last_language_round_trip() {
        let path = temp_file("language");
        let state = SettingsState::load(path.clone());
        state
            .set(&ids(&["claude"]), None, Some(Language::Ko))
            .unwrap();
        assert_eq!(load(&path).effective_language(), Lang::Ko);
        state.set_resolved(Lang::En).unwrap();
        state
            .set(&ids(&["claude"]), None, Some(Language::Auto))
            .unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.language, Language::Auto);
        assert_eq!(loaded.effective_language(), Lang::En);
        // 언어 칸 값이 틀리면 그 칸만 기본값.
        std::fs::write(
            &path,
            r#"{"enabledAgents":["claude"],"language":"ja","resolvedLanguage":1}"#,
        )
        .unwrap();
        let bad = load(&path);
        assert_eq!(
            (bad.language, bad.resolved_language),
            (Language::Auto, Lang::En)
        );
        assert_eq!(bad.enabled_agents, [Agent::Claude]);
    }

    #[test]
    fn out_of_range_tuning_field_falls_back_alone_on_load() {
        let path = temp_file("bad-tuning");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"enabledAgents":["codex"],"tuning":{"apiIntervalSecs":120,"bytesPerToken":0}}"#,
        )
        .unwrap();
        let settings = load(&path);
        assert_eq!(settings.enabled_agents, [Agent::Codex]);
        assert_eq!(settings.tuning.api_interval_secs, 120);
        assert_eq!(settings.tuning.bytes_per_token, 4);
        assert_eq!(settings.reason, None);
    }

    #[test]
    fn set_rejects_out_of_range_tuning_and_keeps_the_saved_one() {
        let path = temp_file("set-tuning");
        let state = SettingsState::load(path.clone());
        let faster = Tuning {
            api_interval_secs: 120,
            ..Tuning::default()
        };
        state.set(&ids(&["claude"]), Some(faster), None).unwrap();
        let bad = Tuning {
            bytes_per_token: 0,
            ..Tuning::default()
        };
        let error = state.set(&ids(&["claude"]), Some(bad), None).unwrap_err();
        assert!(matches!(error, AppError::InvalidInput(m) if m.contains("bytesPerToken")));
        // tuning을 생략한 저장은 지금 값을 유지하고, 파일에서도 그대로 읽힌다.
        state.set(&ids(&["codex"]), None, None).unwrap();
        assert_eq!(state.tuning(), faster);
        assert_eq!(load(&path).tuning, faster);
    }

    #[test]
    fn failed_save_keeps_the_previous_settings_in_memory() {
        let path = temp_file("save-fail");
        let state = SettingsState::load(path.clone());
        state
            .set(&ids(&["claude"]), None, Some(Language::Ko))
            .unwrap();
        // 임시 파일 자리가 폴더라 저장이 실패한다.
        std::fs::create_dir_all(path.with_extension("json.tmp")).unwrap();
        let error = state
            .set(&ids(&["codex"]), None, Some(Language::En))
            .unwrap_err();
        assert!(matches!(error, AppError::Internal));
        let now = state.get().unwrap();
        assert_eq!(
            (now.enabled_agents, now.language),
            (vec![Agent::Claude], Language::Ko)
        );
        assert!(state.set_resolved(Lang::Ko).is_err());
        assert_eq!(state.get().unwrap().resolved_language, Lang::default());
        assert_eq!(load(&path).enabled_agents, [Agent::Claude]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn set_resolved_writes_the_file_only_when_the_language_changes() {
        let path = temp_file("resolved-same");
        let state = SettingsState::load(path.clone());
        state.set_resolved(Lang::default()).unwrap();
        assert!(!path.exists());
        state.set_resolved(Lang::Ko).unwrap();
        assert!(path.exists());
        std::fs::remove_file(&path).unwrap();
        state.set_resolved(Lang::Ko).unwrap();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
