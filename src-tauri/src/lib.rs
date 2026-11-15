mod cache;
mod coach;
mod context;
mod error;
mod health;
mod i18n;
#[cfg(test)]
mod ipc_fixtures;
mod metrics;
mod rate;
mod settings;
mod tray;
mod tuning;
mod usage;

use tauri::{AppHandle, Manager};

use error::AppError;

/// 새로고침(docs/decisions/0010): 사용량 조회(429 backoff는 지킨다)와 1%당 토큰·세션 시작 토큰·트레이 지표
/// 재계산과 처방 효과 비교를 바로 시작한다. 결과는 기존 event(`usage-updated`, `rates-updated`, `context-updated`,
/// `metrics-updated`, `health-updated`)로 간다.
#[tauri::command]
fn refresh_all(app: AppHandle) -> Result<(), AppError> {
    usage::schedule::request_refresh(&app);
    rate::prewarm(app.clone(), true);
    context::prewarm(app.clone(), true);
    metrics::prewarm(app.clone(), true);
    health::prewarm(app, true);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // 개발 전용 MCP bridge. debug 빌드 + `mcp` feature일 때만 컴파일된다.
    // 기본 bind는 0.0.0.0이라 반드시 loopback으로 고정한다(docs/decisions/0001).
    #[cfg(all(debug_assertions, feature = "mcp"))]
    let builder = builder.plugin(
        tauri_plugin_mcp_bridge::Builder::new()
            .bind_address("127.0.0.1")
            .base_port(9224)
            .build(),
    );

    builder
        .manage(usage::UsageState::default())
        .manage(rate::RatesState::default())
        .manage(context::ContextState::default())
        .manage(metrics::MetricsState::default())
        .manage(health::HealthState::default())
        .manage(coach::checks::ChecksState::default())
        .manage(coach::usage::UsageState::default())
        .manage(coach::checks::handoff::HandoffState::default())
        .invoke_handler(tauri::generate_handler![
            usage::get_usage,
            usage::get_installed_agents,
            context::get_context,
            coach::get_switches,
            coach::preview_switch,
            coach::apply_switch,
            coach::preview_scope_move,
            coach::apply_scope_move,
            coach::undo_switch,
            coach::get_coach_log,
            coach::checks::get_checks,
            coach::checks::preview_check,
            coach::checks::apply_check,
            coach::checks::dismiss_check,
            coach::checks::restore_dismissed_checks,
            coach::checks::handoff::ask_claude_handoff,
            coach::checks::handoff::apply_claude_handoff,
            coach::checks::rules_handoff::ask_claude_rules,
            coach::checks::rules_handoff::apply_claude_rule,
            rate::get_rates,
            metrics::get_metrics,
            health::get_health,
            refresh_all,
            settings::get_settings,
            settings::set_settings,
            settings::set_ui_language,
            tray::fit_popover_height,
            tray::open_dashboard,
            tray::quit_app
        ])
        .setup(|app| {
            // 메뉴바 앱: Dock 아이콘 없음.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            app.manage(settings::SettingsState::from_app(app.handle())?);
            tray::setup(app)?;
            // 첫 실행(설정 파일 없음)이면 팝오버를 한 번 열어 앱이 떴음을 알린다.
            let settings = app.state::<settings::SettingsState>();
            if settings.is_first_run() {
                settings.mark_seen();
                tray::show_once(app.handle());
            }
            usage::schedule::start(app.handle().clone(), tray::update);
            // 첫 화면이 기다리지 않게 환율·컨텍스트·지표를 뒤에서 한 번 미리 계산한다(타이머 아님).
            rate::prewarm(app.handle().clone(), false);
            context::prewarm(app.handle().clone(), false);
            metrics::prewarm(app.handle().clone(), false);
            // 팝오버 "점검" 알림이 처음부터 나오게 점검 기록도 한 번 미리 읽는다(반복 없음, 0017 "화면 자리").
            coach::checks::prewarm(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
