//! 트레이(macOS 메뉴바) 아이콘과 팝오버 창(docs/decisions/0004).
//! 아이콘은 1개(`kcal`). 좌클릭: 아이콘 아래에 팝오버를 연다/닫는다. 우클릭: "종료" 메뉴.
//! 에이전트 전환은 팝오버 위쪽 탭이 한다. 팝오버는 포커스를 잃으면 숨는다.
//! 아이콘 이미지는 에이전트별 남은 % 막대(docs/decisions/0012, `icon.rs`).
//! 대시보드 창(docs/decisions/0010)은 시작할 때 만들지 않고 `open_dashboard`가 처음 불릴 때 만든다.

mod icon;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    App, AppHandle, Emitter, LogicalSize, Manager, Monitor, Rect, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use time::OffsetDateTime;

use crate::error::AppError;
use crate::i18n::pick;
use crate::settings::{Settings, SettingsState};
use crate::usage::{Agent, UsageSnapshot, UsageState};

const POPOVER: &str = "popover";
/// `capabilities/default.json`의 `windows`에도 있다.
const DASHBOARD: &str = "dashboard";
const DASHBOARD_SIZE: (f64, f64) = (900.0, 700.0);
const DASHBOARD_MIN_SIZE: (f64, f64) = (560.0, 400.0);
const TRAY_ID: &str = "kcal";
const QUIT_ID: &str = "quit";
/// 팝오버가 열린 채 아이콘을 누르면 blur(숨김) 다음에 click이 온다. 그 click으로 다시 열지 않는다.
const BLUR_CLICK_GUARD: Duration = Duration::from_millis(300);
/// 첫 실행 팝오버는 트레이가 만들어진 뒤 아이콘 위치(rect)가 잡힐 때까지 한 번 기다렸다가 연다.
const FIRST_RUN_DELAY: Duration = Duration::from_millis(500);

/// macOS는 template(검정 + alpha)이라야 라이트·다크에 자동으로 맞는다.
const ICON_IS_TEMPLATE: bool = cfg!(target_os = "macos");

/// 마지막으로 그린 아이콘의 열쇠. 같으면 `set_icon`을 건너뛴다. 바깥 `None` = 아직 안 그림, 안쪽 `None` = 불꽃.
type IconKey = Option<Vec<(Agent, Option<u32>)>>;

#[derive(Default)]
struct IconState {
    last: Mutex<Option<IconKey>>,
}

#[derive(Default)]
struct PopoverState {
    hidden_by_blur_at: Mutex<Option<Instant>>,
}

/// 물리 픽셀 사각형 `(x, y, width, height)`.
type Area = (f64, f64, f64, f64);

/// 팝오버 왼쪽 위 좌표. 아이콘 가운데 아래에 두고, 아래 공간이 없으면(Windows 작업표시줄) 위에 둔다.
/// 작업 영역 밖으로 넘지 않게 자른다.
fn popover_position(icon: Area, window: (f64, f64), area: Area) -> (f64, f64) {
    let (ix, iy, iw, ih) = icon;
    let (ww, wh) = window;
    let (ax, ay, aw, ah) = area;
    let below = iy + ih;
    let y = if below + wh <= ay + ah {
        below
    } else {
        iy - wh
    };
    let x = ix + iw / 2.0 - ww / 2.0;
    (x.min(ax + aw - ww).max(ax), y.min(ay + ah - wh).max(ay))
}

/// 프론트엔드가 재서 보내는 팝오버 높이(논리 px)의 허용 범위.
const HEIGHT_RANGE: std::ops::RangeInclusive<f64> = 100.0..=800.0;

/// 팝오버 높이를 내용에 맞춘다(폭은 그대로). 위치는 다음에 열 때 `toggle_popover`가 다시 잡는다.
// ponytail: 열려 있는 동안 높이가 바뀌면 위에 붙는 배치(Windows 작업표시줄 위)는 아래쪽이 움직인다. 필요하면 resize 뒤 재배치.
#[tauri::command]
pub fn fit_popover_height<R: Runtime>(
    window: WebviewWindow<R>,
    height: f64,
) -> Result<(), AppError> {
    if window.label() != POPOVER {
        return Err(AppError::InvalidInput(
            pick(
                "팝오버 창에서만 높이를 맞춥니다.",
                "Only the popover window can fit its height.",
            )
            .into(),
        ));
    }
    if !HEIGHT_RANGE.contains(&height) {
        eprintln!("fit_popover_height: {height} is outside 100..=800");
        return Err(AppError::Internal);
    }
    resize(&window, height).map_err(|error| {
        eprintln!("fit_popover_height: {error}");
        AppError::Internal
    })
}

fn resize<R: Runtime>(window: &WebviewWindow<R>, height: f64) -> tauri::Result<()> {
    let scale = window.scale_factor()?;
    let width = window.outer_size()?.width as f64 / scale;
    window.set_size(LogicalSize::new(width, height))
}

/// 대시보드 창의 탭(docs/decisions/0017 "화면 자리"). TS 짝은 `ipc.ts`의 `DashboardTab`.
/// command 인자로 받으므로 이 두 값 밖의 문자열은 역직렬화에서 거부된다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DashboardTab {
    Usage,
    Checks,
}

impl DashboardTab {
    fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::Checks => "checks",
        }
    }
}

/// 이미 열린 대시보드 창에 탭을 바꾸라고 보내는 event. TS 짝은 `ipc.ts`의 `DASHBOARD_TAB_EVENT`.
pub const DASHBOARD_TAB_EVENT: &str = "dashboard-tab";

/// 새 대시보드 창의 앱 경로. 탭이 있으면 `?tab=`로 시작 탭을 넘긴다(프론트가 읽는다).
fn dashboard_path(tab: Option<DashboardTab>) -> String {
    match tab {
        Some(tab) => format!("index.html?tab={}", tab.as_str()),
        None => "index.html".to_owned(),
    }
}

/// 대시보드 창을 보여 준다(없으면 만든다). 팝오버는 숨긴다.
/// `tab`이 있으면 새 창은 그 탭으로 시작하고, 열린 창에는 대시보드 창에만 `DASHBOARD_TAB_EVENT`를 보낸다.
/// 창을 만드는 command는 Windows에서 동기로 부르면 멈추므로 async다.
#[tauri::command]
pub async fn open_dashboard<R: Runtime>(
    app: AppHandle<R>,
    tab: Option<DashboardTab>,
) -> Result<(), AppError> {
    if let Some(popover) = app.get_webview_window(POPOVER) {
        let _ = popover.hide();
    }
    let window = match app.get_webview_window(DASHBOARD) {
        Some(window) => {
            if let Some(tab) = tab {
                app.emit_to(DASHBOARD, DASHBOARD_TAB_EVENT, tab)
                    .map_err(|error| {
                        eprintln!("open_dashboard: {error}");
                        AppError::Internal
                    })?;
            }
            window
        }
        None => {
            WebviewWindowBuilder::new(&app, DASHBOARD, WebviewUrl::App(dashboard_path(tab).into()))
                .title(dashboard_title())
                .inner_size(DASHBOARD_SIZE.0, DASHBOARD_SIZE.1)
                .min_inner_size(DASHBOARD_MIN_SIZE.0, DASHBOARD_MIN_SIZE.1)
                .resizable(true)
                .decorations(true)
                .build()
                .map_err(|error| {
                    eprintln!("open_dashboard: {error}");
                    AppError::Internal
                })?
        }
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub fn quit_app<R: Runtime>(app: AppHandle<R>) {
    app.exit(0);
}

pub fn setup<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    app.manage(PopoverState::default());
    app.manage(IconState::default());

    if let Some(window) = app.get_webview_window(POPOVER) {
        let handle = app.handle().clone();
        let popover = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                let _ = popover.hide();
                if let Ok(mut at) = handle.state::<PopoverState>().hidden_by_blur_at.lock() {
                    *at = Some(Instant::now());
                }
            }
        });
    }
    build_tray(app.handle())?;
    refresh(app.handle());
    Ok(())
}

fn dashboard_title() -> &'static str {
    pick("kcal 대시보드", "kcal Dashboard")
}

/// 우클릭 메뉴. 언어가 바뀌면 `relabel`이 새로 만들어 바꿔 단다.
fn tray_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let quit = MenuItem::with_id(app, QUIT_ID, pick("종료", "Quit"), true, None::<&str>)?;
    Menu::with_items(app, &[&quit])
}

/// 화면 언어가 바뀐 뒤(0021): 메뉴·tooltip·열린 대시보드 창 제목을 그 언어로 바꾼다.
pub fn relabel<R: Runtime>(app: &AppHandle<R>) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match tray_menu(app) {
            Ok(menu) => {
                if let Err(error) = tray.set_menu(Some(menu)) {
                    eprintln!("tray menu: {error}");
                }
            }
            Err(error) => eprintln!("tray menu: {error}"),
        }
    }
    if let Some(window) = app.get_webview_window(DASHBOARD) {
        let _ = window.set_title(dashboard_title());
    }
    refresh(app);
}

fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = tray_menu(app)?;
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("kcal")
        .on_menu_event(|app, event| {
            if event.id() == QUIT_ID {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_popover(tray.app_handle(), rect);
            }
        });
    let builder = match flame_icon(app)? {
        Some(icon) => builder.icon(icon),
        None => builder,
    };
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(ICON_IS_TEMPLATE);
    builder.build(app)?;
    Ok(())
}

/// 막대로 그릴 값이 없을 때의 불꽃 아이콘. macOS는 단색 template(원본 `docs/assets/tray.svg`).
fn flame_icon<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Option<Image<'static>>> {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        Image::from_bytes(include_bytes!("../icons/tray.png")).map(Some)
    }
    #[cfg(not(target_os = "macos"))]
    Ok(app.default_window_icon().cloned().map(Image::to_owned))
}

#[derive(Debug, PartialEq, Eq)]
enum ClickAction {
    Hide,
    Show,
    Ignore,
}

/// 아이콘 클릭에 무엇을 할지만 정한다(창 조작 없음). 가드는 시각 비교일 뿐 상태를 소비하지 않는다.
fn decide(
    is_visible: bool,
    hidden_by_blur_at: Option<Instant>,
    now: Instant,
    guard: Duration,
) -> ClickAction {
    if is_visible {
        return ClickAction::Hide;
    }
    match hidden_by_blur_at {
        Some(at) if now.saturating_duration_since(at) < guard => ClickAction::Ignore,
        _ => ClickAction::Show,
    }
}

fn toggle_popover<R: Runtime>(app: &AppHandle<R>, rect: Rect) {
    let Some(window) = app.get_webview_window(POPOVER) else {
        return;
    };
    let hidden_by_blur_at = app
        .state::<PopoverState>()
        .hidden_by_blur_at
        .lock()
        .ok()
        .and_then(|at| *at);
    match decide(
        window.is_visible().unwrap_or(false),
        hidden_by_blur_at,
        Instant::now(),
        BLUR_CLICK_GUARD,
    ) {
        ClickAction::Hide => {
            let _ = window.hide();
            return;
        }
        ClickAction::Ignore => return,
        ClickAction::Show => {}
    }
    place_popover(app, &window, rect);
    let _ = window.show();
    let _ = window.set_focus();
}

/// 첫 실행에 팝오버를 아이콘 아래에 한 번 연다(docs/backlog 첫 실행). 반복하지 않고 한 번만 기다린다.
/// 아이콘 위치(rect)나 놓을 화면을 못 찾으면 아무것도 하지 않는다(엉뚱한 곳에 띄우지 않는다).
/// 보이는 팝오버는 포커스를 잃으면 숨으므로(`setup`), 열 때 포커스를 준다. 아이콘 클릭과 같다.
pub fn show_once<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_RUN_DELAY).await;
        let Some(window) = app.get_webview_window(POPOVER) else {
            return;
        };
        if window.is_visible().unwrap_or(false) {
            return;
        }
        let rect = app
            .tray_by_id(TRAY_ID)
            .and_then(|tray| tray.rect().ok().flatten());
        let Some(rect) = rect else {
            eprintln!("tray: first-run popover skipped, icon position unavailable");
            return;
        };
        if !place_under_icon(&app, &window, rect) {
            eprintln!("tray: first-run popover skipped, could not place it");
            return;
        }
        let _ = window.show();
        let _ = window.set_focus();
    });
}

fn work_area(monitor: &Monitor) -> Area {
    let a = monitor.work_area();
    (
        a.position.x as f64,
        a.position.y as f64,
        a.size.width as f64,
        a.size.height as f64,
    )
}

#[cfg(any(target_os = "macos", test))]
fn to_points(area: Area, scale: f64) -> Area {
    (
        area.0 / scale,
        area.1 / scale,
        area.2 / scale,
        area.3 / scale,
    )
}

/// macOS 팝오버 왼쪽 위(포인트). macOS의 물리 픽셀은 포인트 × 그 화면 배율이라, 배율이 다른 화면끼리
/// 섞으면 다른 화면에 뜬다. 모든 화면에 하나로 이어진 포인트로 바꿔 계산한다.
/// `icon`·`area`는 누른 화면 배율(`screen_scale`), `window`는 팝오버가 지금 있는 화면 배율(`window_scale`)의 물리 픽셀.
#[cfg(any(target_os = "macos", test))]
fn popover_points(
    icon: Area,
    area: Area,
    screen_scale: f64,
    window: (f64, f64),
    window_scale: f64,
) -> (f64, f64) {
    popover_position(
        to_points(icon, screen_scale),
        (window.0 / window_scale, window.1 / window_scale),
        to_points(area, screen_scale),
    )
}

/// 누른 화면은 커서로 찾는다. macOS `monitor_from_point`는 포인트를 받고(tao `CGDisplayBounds`, tauri#14559),
/// `cursor_position`은 포인트 × 주 화면 배율이다. 위치는 포인트(`LogicalPosition`)로 놓아야
/// 팝오버가 지금 있는 화면 배율로 나뉘지 않는다.
#[cfg(target_os = "macos")]
fn place_popover<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, rect: Rect) -> bool {
    let clicked = app
        .cursor_position()
        .ok()
        .zip(app.primary_monitor().ok().flatten())
        .and_then(|(cursor, primary)| {
            let scale = primary.scale_factor();
            app.monitor_from_point(cursor.x / scale, cursor.y / scale)
                .ok()
                .flatten()
        });
    if clicked.is_none() {
        eprintln!("tray: clicked screen not found, using the popover's screen");
    }
    let Some(monitor) = clicked.or_else(|| window.current_monitor().ok().flatten()) else {
        return false;
    };
    place_on(window, &monitor, rect)
}

/// 아이콘이 있는 화면 번호(`screens`는 화면마다 (물리 사각형, 배율)). 아이콘 rect는 그 화면 배율의 물리 픽셀이라
/// 화면마다 자기 배율로 포인트로 바꿔 화면 범위 안인지 본다.
#[cfg(any(target_os = "macos", test))]
fn screen_of_icon(icon: (f64, f64), screens: &[(Area, f64)]) -> Option<usize> {
    screens.iter().position(|&(area, scale)| {
        let (ax, ay, aw, ah) = to_points(area, scale);
        let (x, y) = (icon.0 / scale, icon.1 / scale);
        (ax..ax + aw).contains(&x) && (ay..ay + ah).contains(&y)
    })
}

/// 클릭이 없는 첫 실행용: 커서가 아니라 아이콘 rect로 화면을 찾는다. 못 찾으면 `false`.
#[cfg(target_os = "macos")]
fn place_under_icon<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, rect: Rect) -> bool {
    let Ok(monitors) = app.available_monitors() else {
        return false;
    };
    let screens: Vec<(Area, f64)> = monitors
        .iter()
        .map(|m| {
            let (p, s) = (m.position(), m.size());
            (
                (p.x as f64, p.y as f64, s.width as f64, s.height as f64),
                m.scale_factor(),
            )
        })
        .collect();
    let icon = rect.position.to_physical::<f64>(1.0);
    match screen_of_icon((icon.x, icon.y), &screens) {
        Some(i) => place_on(window, &monitors[i], rect),
        None => false,
    }
}

#[cfg(target_os = "macos")]
fn place_on<R: Runtime>(window: &WebviewWindow<R>, monitor: &Monitor, rect: Rect) -> bool {
    let (Ok(outer), Ok(window_scale)) = (window.outer_size(), window.scale_factor()) else {
        return false;
    };
    let screen_scale = monitor.scale_factor();
    let position = rect.position.to_physical::<f64>(screen_scale);
    let size = rect.size.to_physical::<f64>(screen_scale);
    let (x, y) = popover_points(
        (position.x, position.y, size.width, size.height),
        work_area(monitor),
        screen_scale,
        (outer.width as f64, outer.height as f64),
        window_scale,
    );
    window
        .set_position(tauri::LogicalPosition::new(x, y))
        .is_ok()
}

/// Windows는 물리 픽셀 그대로 계산한다. 배율이 다른 다중 모니터는 아직 실기 확인 전이다.
#[cfg(not(target_os = "macos"))]
fn place_popover<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, rect: Rect) -> bool {
    let scale = window.scale_factor().unwrap_or(1.0);
    let position = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    let icon = (position.x, position.y, size.width, size.height);
    let center = (icon.0 + icon.2 / 2.0, icon.1 + icon.3 / 2.0);
    let area = app
        .monitor_from_point(center.0, center.1)
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())
        .map(|m| work_area(&m));
    let (Some(area), Ok(outer)) = (area, window.outer_size()) else {
        return false;
    };
    let (x, y) = popover_position(icon, (outer.width as f64, outer.height as f64), area);
    window
        .set_position(tauri::PhysicalPosition::new(
            x.round() as i32,
            y.round() as i32,
        ))
        .is_ok()
}

/// Windows는 화면을 rect 가운데로 찾으므로 클릭과 같은 코드를 쓴다.
#[cfg(not(target_os = "macos"))]
fn place_under_icon<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, rect: Rect) -> bool {
    place_popover(app, window, rect)
}

/// 막대로 그릴 (에이전트, 남은 %). 켠 에이전트만 Claude·Codex·Kiro 순서로.
/// 켠 에이전트가 없거나 값이 전부 없으면 `None`(불꽃 아이콘).
fn bars(
    snapshot: &UsageSnapshot,
    settings: &Settings,
    now: OffsetDateTime,
) -> Option<Vec<(Agent, Option<f64>)>> {
    let bars: Vec<_> = Agent::ALL
        .into_iter()
        .filter(|&agent| settings.is_enabled(agent))
        .filter_map(|agent| snapshot.agents.iter().find(|u| u.agent == agent))
        .map(|usage| (usage.agent, usage.min_remaining(now)))
        .collect();
    bars.iter().any(|(_, p)| p.is_some()).then_some(bars)
}

/// 켠 에이전트마다 한 줄 요약.
fn tray_tooltip(snapshot: &UsageSnapshot, settings: &Settings, now: OffsetDateTime) -> String {
    snapshot
        .agents
        .iter()
        .filter(|usage| settings.is_enabled(usage.agent))
        .map(|usage| usage.summary(now))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 설정이 바뀌었을 때 현재 스냅샷으로 아이콘·tooltip을 다시 맞춘다.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(snapshot) = app.state::<UsageState>().snapshot() {
        update(app, &snapshot);
    }
}

/// 값이 바뀔 때마다 부른다. 아이콘은 막대(`bars`), tooltip은 한 줄 요약.
pub fn update<R: Runtime>(app: &AppHandle<R>, snapshot: &UsageSnapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let settings = app.state::<SettingsState>().get().unwrap_or_default();
    let now = OffsetDateTime::now_utc();
    let _ = tray.set_tooltip(Some(tray_tooltip(snapshot, &settings, now)));
    set_bars_icon(app, &tray, bars(snapshot, &settings, now));
}

/// 그려야 할 이미지가 마지막과 같으면 건너뛴다. 실패하면 로그만 남기고 다음 갱신 때 다시 시도한다.
fn set_bars_icon<R: Runtime>(
    app: &AppHandle<R>,
    tray: &tauri::tray::TrayIcon<R>,
    bars: Option<Vec<(Agent, Option<f64>)>>,
) {
    let fills: Vec<Option<f64>> = bars.iter().flatten().map(|&(_, percent)| percent).collect();
    let key: IconKey = bars.as_ref().map(|bars| {
        let px = icon::quantize(&fills, &icon::SPEC);
        bars.iter().map(|&(agent, _)| agent).zip(px).collect()
    });
    let state = app.state::<IconState>();
    let Ok(mut last) = state.last.lock() else {
        return;
    };
    if last.as_ref() == Some(&key) {
        return;
    }
    let image = match &bars {
        Some(bars) => {
            let colors: Vec<_> = bars.iter().map(|&(a, _)| icon::agent_rgb(a)).collect();
            let (rgba, w, h) = icon::render(&fills, &colors, &icon::SPEC);
            Ok(Some(Image::new_owned(rgba, w, h)))
        }
        None => flame_icon(app),
    };
    match image.and_then(|image| tray.set_icon_with_as_template(image, ICON_IS_TEMPLATE)) {
        Ok(()) => *last = Some(key),
        Err(error) => eprintln!("tray icon: {error}"),
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(dashboard_path(None), "index.html");
        assert_eq!(
            dashboard_path(Some(DashboardTab::Checks)),
            "index.html?tab=checks"
        );
        assert_eq!(
            dashboard_path(Some(DashboardTab::Usage)),
            "index.html?tab=usage"
        );
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
}
