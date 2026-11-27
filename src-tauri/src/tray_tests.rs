//! `tray.rs` 테스트.

use super::*;
use crate::usage::{Agent, AgentUsage, LimitWindow, WindowId};
use time::macros::datetime;

const AREA: Area = (0.0, 50.0, 3000.0, 1900.0);

#[test]
fn places_popover_centered_below_menu_bar_icon() {
    assert_eq!(
        popover_position((1000.0, 0.0, 40.0, 50.0), (720.0, 960.0), AREA),
        (660.0, 50.0)
    );
}

#[test]
fn keeps_popover_inside_right_edge() {
    assert_eq!(
        popover_position((2980.0, 0.0, 40.0, 50.0), (720.0, 960.0), AREA),
        (2280.0, 50.0)
    );
}

#[test]
fn opens_above_icon_when_no_room_below() {
    let area = (0.0, 0.0, 1920.0, 1040.0);
    assert_eq!(
        popover_position((1800.0, 1040.0, 32.0, 40.0), (360.0, 480.0), area),
        (1560.0, 560.0)
    );
}

// 이 Mac 실측 배치(2026-11-09, `NSScreen`): 맥북 1470×956pt 배율 2(주 화면), 그 위에 DELL 1920×1080pt
// 배율 1(x −250부터). 물리 픽셀 값은 tray-icon·tauri가 주는 대로 포인트 × 그 화면 배율.
const MACBOOK_WORK_AREA: Area = (0.0, 66.0, 2940.0, 1846.0);
const DELL_WORK_AREA: Area = (-250.0, -1080.0, 1920.0, 1080.0);

#[test]
fn opens_under_retina_icon_when_popover_was_on_lower_scale_monitor() {
    // 맥북 아이콘 (1200, 0, 40, 33)pt. 팝오버는 마지막에 DELL(배율 1)에 있었다.
    let icon = (2400.0, 0.0, 80.0, 66.0);
    assert_eq!(
        popover_points(icon, MACBOOK_WORK_AREA, 2.0, (310.0, 400.0), 1.0),
        (1065.0, 33.0)
    );
}

#[test]
fn opens_under_external_icon_when_popover_was_on_retina() {
    // DELL 아이콘 (1200, -1080, 40, 24)pt. 팝오버는 마지막에 맥북(배율 2)에 있었다.
    let icon = (1200.0, -1080.0, 40.0, 24.0);
    assert_eq!(
        popover_points(icon, DELL_WORK_AREA, 1.0, (620.0, 800.0), 2.0),
        (1065.0, -1056.0)
    );
}

#[test]
fn finds_the_screen_that_holds_the_icon_rect() {
    // 위 실측 배치: 맥북(배율 2)과 DELL(배율 1). 화면 전체 물리 사각형(메뉴바 포함).
    let screens = [
        ((0.0, 0.0, 2940.0, 1912.0), 2.0),
        ((-250.0, -1080.0, 1920.0, 1080.0), 1.0),
    ];
    assert_eq!(screen_of_icon((2400.0, 0.0), &screens), Some(0));
    assert_eq!(screen_of_icon((1200.0, -1080.0), &screens), Some(1));
    assert_eq!(screen_of_icon((9000.0, 0.0), &screens), None);
}

#[test]
fn dashboard_tab_accepts_only_usage_and_checks() {
    use serde_json::{from_value, json};
    assert_eq!(
        from_value::<DashboardTab>(json!("checks")).unwrap(),
        DashboardTab::Checks
    );
    assert_eq!(
        from_value::<DashboardTab>(json!("usage")).unwrap(),
        DashboardTab::Usage
    );
    for bad in [json!("Checks"), json!("settings"), json!(""), json!(1)] {
        assert!(from_value::<DashboardTab>(bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(
        serde_json::to_value(DashboardTab::Checks).unwrap(),
        "checks"
    );
}

#[test]
fn new_dashboard_starts_on_the_requested_tab() {
    use crate::usage::Agent;
    assert_eq!(dashboard_path(None, None), "index.html");
    assert_eq!(
        dashboard_path(Some(DashboardTab::Checks), None),
        "index.html?tab=checks"
    );
    assert_eq!(
        dashboard_path(Some(DashboardTab::Usage), None),
        "index.html?tab=usage"
    );
    assert_eq!(
        dashboard_path(None, Some(Agent::Kiro)),
        "index.html?agent=kiro"
    );
    assert_eq!(
        dashboard_path(Some(DashboardTab::Usage), Some(Agent::Codex)),
        "index.html?tab=usage&agent=codex"
    );
}

#[test]
fn dashboard_agent_accepts_only_known_ids() {
    use crate::usage::Agent;
    use serde_json::{from_value, json};
    for agent in Agent::ALL {
        assert_eq!(from_value::<Agent>(json!(agent.id())).unwrap(), agent);
    }
    for bad in [json!("Claude"), json!("gemini"), json!(""), json!(1)] {
        assert!(from_value::<Agent>(bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn hides_popover_when_visible() {
    let now = Instant::now();
    assert_eq!(decide(true, None, now, BLUR_CLICK_GUARD), ClickAction::Hide);
}

#[test]
fn shows_popover_when_hidden_and_never_blurred() {
    let now = Instant::now();
    assert_eq!(
        decide(false, None, now, BLUR_CLICK_GUARD),
        ClickAction::Show
    );
}

#[test]
fn ignores_click_right_after_blur_hide() {
    let blurred = Instant::now();
    let now = blurred + Duration::from_millis(100);
    assert_eq!(
        decide(false, Some(blurred), now, BLUR_CLICK_GUARD),
        ClickAction::Ignore
    );
}

#[test]
fn shows_popover_once_guard_has_passed() {
    let blurred = Instant::now();
    let now = blurred + BLUR_CLICK_GUARD;
    assert_eq!(
        decide(false, Some(blurred), now, BLUR_CLICK_GUARD),
        ClickAction::Show
    );
}

#[test]
fn shows_on_next_click_after_an_ignored_one() {
    let blurred = Instant::now();
    let ignored = blurred + Duration::from_millis(100);
    assert_eq!(
        decide(false, Some(blurred), ignored, BLUR_CLICK_GUARD),
        ClickAction::Ignore
    );
    let next = ignored + Duration::from_millis(500);
    assert_eq!(
        decide(false, Some(blurred), next, BLUR_CLICK_GUARD),
        ClickAction::Show
    );
}

fn used(agent: Agent, percents: &[f64]) -> AgentUsage {
    AgentUsage {
        windows: percents
            .iter()
            .map(|&used_percent| LimitWindow {
                id: WindowId::FiveHour,
                used_percent,
                resets_at: Some(datetime!(2026-11-04 15:00 UTC)),
                reset_passed: false,
                used: None,
                limit: None,
                group: None,
            })
            .collect(),
        ..AgentUsage::pending(agent)
    }
}

fn enabled(agents: &[Agent]) -> Settings {
    Settings {
        enabled_agents: agents.to_vec(),
        ..Settings::default()
    }
}

fn snapshot() -> UsageSnapshot {
    UsageSnapshot {
        agents: vec![
            used(Agent::Claude, &[10.0, 30.0]),
            used(Agent::Codex, &[95.0]),
            AgentUsage::pending(Agent::Kiro),
        ],
    }
}

const NOW: OffsetDateTime = datetime!(2026-11-04 12:00 UTC);

#[test]
fn bars_use_lowest_remaining_per_enabled_agent_in_fixed_order() {
    assert_eq!(
        bars(&snapshot(), &enabled(&Agent::ALL), NOW),
        Some(vec![
            (Agent::Claude, Some(70.0)),
            (Agent::Codex, Some(5.0)),
            (Agent::Kiro, None),
        ])
    );
    // settings 순서가 달라도 Claude·Codex·Kiro 순서이고, 꺼진 Codex는 빠진다.
    assert_eq!(
        bars(&snapshot(), &enabled(&[Agent::Kiro, Agent::Claude]), NOW),
        Some(vec![(Agent::Claude, Some(70.0)), (Agent::Kiro, None)])
    );
}

#[test]
fn falls_back_to_flame_icon_when_no_enabled_agent_has_a_value() {
    assert_eq!(bars(&snapshot(), &enabled(&[Agent::Kiro]), NOW), None);
    assert_eq!(bars(&snapshot(), &enabled(&[]), NOW), None);
}

#[test]
fn tooltip_lists_enabled_agents_only() {
    assert_eq!(
        tray_tooltip(&snapshot(), &enabled(&[Agent::Claude, Agent::Kiro]), NOW),
        "Claude 5h 90% 남음 · 5h 70% 남음\nKiro —"
    );
}

#[test]
fn tooltip_and_errors_follow_english_ui() {
    use crate::i18n::{Lang, with_lang};
    let tooltip = with_lang(Lang::En, || {
        tray_tooltip(&snapshot(), &enabled(&[Agent::Claude, Agent::Kiro]), NOW)
    });
    assert_eq!(tooltip, "Claude 5h 90% left · 5h 70% left\nKiro —");
    let error = with_lang(Lang::En, || {
        serde_json::to_value(AppError::Internal).unwrap()
    });
    assert_eq!(error["message"], "Something went wrong inside kcal.");
}

#[test]
fn percent_uses_lowest_enabled_value_and_rounds_without_hiding_zero_or_100() {
    let values = bars(&snapshot(), &enabled(&Agent::ALL), NOW).unwrap();
    assert_eq!(lowest_percent(&values), Some(5));
    let values = bars(&snapshot(), &enabled(&[Agent::Claude, Agent::Kiro]), NOW).unwrap();
    assert_eq!(lowest_percent(&values), Some(70));
    assert_eq!(lowest_percent(&[(Agent::Kiro, None)]), None);
    assert_eq!(lowest_percent(&[(Agent::Claude, Some(61.6))]), Some(62));
    assert_eq!(lowest_percent(&[(Agent::Claude, Some(0.0))]), Some(0));
    assert_eq!(lowest_percent(&[(Agent::Claude, Some(100.0))]), Some(100));
}

#[test]
fn icon_cache_key_tracks_mode_and_number_but_skips_identical_displays() {
    let values = [(Agent::Claude, Some(62.0))];
    let key = |mode| icon_key(mode, Some(&values));
    assert_ne!(key(TrayIcon::Bars), key(TrayIcon::Percent));
    assert_ne!(key(TrayIcon::Percent), key(TrayIcon::Icon));
    assert_ne!(key(TrayIcon::Bars), key(TrayIcon::Icon));
    assert_eq!(key(TrayIcon::Percent), key(TrayIcon::Percent));
    assert_eq!(
        key(TrayIcon::Percent),
        icon_key(TrayIcon::Percent, Some(&[(Agent::Codex, Some(62.1))]))
    );
    assert_ne!(
        key(TrayIcon::Percent),
        icon_key(TrayIcon::Percent, Some(&[(Agent::Claude, Some(63.0))]))
    );
    assert_ne!(
        icon_key(TrayIcon::Bars, None),
        icon_key(TrayIcon::Icon, None)
    );
    assert_eq!(
        percent_title(&key(TrayIcon::Percent)).as_deref(),
        Some("62%")
    );
    assert_eq!(percent_title(&key(TrayIcon::Bars)), None);
    assert_eq!(percent_title(&key(TrayIcon::Icon)), None);
    assert_eq!(percent_title(&icon_key(TrayIcon::Percent, None)), None);
}
