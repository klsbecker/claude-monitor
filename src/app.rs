//! The UI thread: window classes, the message loop and the strip drawn inside the taskbar.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow, DispatchMessageW, EnumChildWindows, FindWindowExW, FindWindowW,
    GetClassNameW, GetClientRect, GetCursorPos, GetMessageW, GetParent, GetWindowRect, HICON, IDC_ARROW, IsWindow,
    IsWindowVisible, LoadCursorW, MSG, PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SW_HIDE,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SetTimer, SetWindowPos, ShowWindow, TranslateMessage, WM_APP, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR, w};

use crate::api::{SESSION, WEEK, find_limit, label};
use crate::gfx::{
    Font, Mask, ORANGE, Rgb, Surface, glyph, level_color, light_taskbar, low_color, over, premul, round_rect_cov, text_mask,
    theme,
};
use crate::i18n::{fill, tr};
use crate::poll::{self, Shared};
use crate::settings::{IconColor, Settings};
use crate::time::{hm, now_unix, parse_iso_unix, when};
use crate::util::log;
use crate::{legacy, menu, tray};

pub const WM_APP_DATA: u32 = WM_APP + 1;
pub const WM_APP_QUIT: u32 = WM_APP + 2;
pub const WM_APP_LOGIN: u32 = WM_APP + 3;
pub const WM_APP_TRAY: u32 = WM_APP + 4;
const WM_MOUSELEAVE: u32 = 0x02A3;

const STRIP_CLASS: PCWSTR = w!("ClaudeMonitor.Strip");
pub const CARD_CLASS: PCWSTR = w!("ClaudeMonitor.Card");
const MAIN_CLASS: PCWSTR = w!("ClaudeMonitor.Main");
const MUTEX_NAME: PCWSTR = w!("Local\\ClaudeMonitor");

const TICK_TIMER: usize = 1;
pub const CARD_TIMER: usize = 2;
pub const NOTIFY_TIMER: usize = 3;

/// Windows that belong to the taskbar itself; any other visible child is another program's widget.
const TASKBAR_PARTS: [&str; 16] = [
    "Start",
    "TrayNotifyWnd",
    "ReBarWindow32",
    "MSTaskSwWClass",
    "MSTaskListWClass",
    "TrayDummySearchControl",
    "Windows.UI.Composition.DesktopWindowContentBridge",
    "Windows.UI.Core.CoreWindow",
    "Windows.UI.Input.InputSite.WindowClass",
    "WorkerW",
    "ClockButton",
    "TrayClockWClass",
    "TrayButton",
    "TrayShowDesktopButtonWClass",
    "SIBTrayButton",
    "ClaudeMonitor.Strip",
];

/// The "TaskbarCreated" message Explorer broadcasts when it restarts.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// What the strip shows; it is redrawn only when this changes.
#[derive(PartialEq)]
pub struct RenderKey {
    text: Vec<(String, Rgb)>,
    dpi: u32,
    h: i32,
    light: bool,
    hover: bool,
    icon: Rgb,
    background: bool,
}

/// Where the strip was placed: notification area rect, taskbar rect, left, top, width, height.
type PlaceKey = ([i32; 4], [i32; 4], i32, i32, i32, i32);

pub struct App {
    pub hinst: HINSTANCE,
    pub main: HWND,
    pub taskbar: HWND,
    pub strip: HWND,
    pub card: HWND,
    pub hover: bool,
    pub card_shown: bool,
    pub card_pos: POINT,
    /// The cursor over the card, in card coordinates.
    pub card_mouse: Option<(i32, i32)>,
    pub card_left_at: Option<Instant>,
    /// Extra distance from the notification area given with `--offset`, in px at 96 DPI.
    pub offset: i32,
    pub size: (i32, i32),
    pub rendered: Option<RenderKey>,
    last_place: Option<PlaceKey>,
    pub shared: Arc<Mutex<Shared>>,
    pub wake: Sender<()>,
    pub settings: Settings,
    /// Alerts already shown, as "<kind>:<limit key>@<reset>", so each fires once per window.
    pub alerted: HashSet<String>,
    pub started: i64,
    pub notify_icon: Option<HICON>,
    /// Notification-area icons shown in icon mode: id, what is drawn (to skip redraws), icon.
    pub tray: Vec<(u32, tray::IconKey, HICON)>,
    pub tray_hover: Option<u32>,
    /// Between NIN_POPUPOPEN and NIN_POPUPCLOSE: the cursor rests on one of our icons.
    pub tray_hovering: bool,
    pub tray_point: POINT,
    /// Icons whose Windows 11 "show in the taskbar" entry was already handled.
    pub promoted: HashSet<u32>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the app state, unless it is already borrowed (a modal loop re-entering the window procedure).
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|c| c.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

/// Changes the settings, saves them and redraws.
pub fn change_settings(f: impl FnOnce(&mut Settings)) {
    with_app(|a| {
        f(&mut a.settings);
        a.settings.save();
        a.settings.apply();
        a.rendered = None;
        a.last_place = None;
        a.tick();
    });
}

struct Scan {
    taskbar: HWND,
    strip: HWND,
    found: Vec<(i32, i32)>,
}

/// Collects the horizontal extent of other programs' windows inside the taskbar.
unsafe extern "system" fn scan_child(hwnd: HWND, lp: LPARAM) -> BOOL {
    unsafe {
        let scan = &mut *(lp.0 as *mut Scan);
        if hwnd == scan.strip || GetParent(hwnd).ok() != Some(scan.taskbar) || !IsWindowVisible(hwnd).as_bool() {
            return TRUE;
        }
        let mut name = [0u16; 128];
        let n = GetClassNameW(hwnd, &mut name).max(0) as usize;
        let class = String::from_utf16_lossy(&name[..n]);
        if TASKBAR_PARTS.contains(&class.as_str()) {
            return TRUE;
        }
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        let mut p = POINT { x: r.left, y: r.top };
        let _ = ScreenToClient(scan.taskbar, &mut p);
        scan.found.push((p.x, p.x + r.right - r.left));
        TRUE
    }
}

impl App {
    /// Makes sure the strip exists as a child of the current taskbar (Explorer may have restarted).
    fn ensure_strip(&mut self) -> bool {
        unsafe {
            let alive = !self.strip.is_invalid()
                && IsWindow(Some(self.strip)).as_bool()
                && IsWindow(Some(self.taskbar)).as_bool()
                && GetParent(self.strip).is_ok_and(|p| p == self.taskbar);
            if alive {
                return true;
            }
            if !self.strip.is_invalid() && IsWindow(Some(self.strip)).as_bool() {
                let _ = DestroyWindow(self.strip);
            }
            self.strip = HWND::default();
            self.hover = false;
            let Ok(taskbar) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) else {
                return false;
            };
            if FindWindowExW(Some(taskbar), None, w!("TrayNotifyWnd"), PCWSTR::null()).is_err() {
                return false;
            }
            self.taskbar = taskbar;
            match CreateWindowExW(
                WS_EX_LAYERED | WS_EX_NOACTIVATE,
                STRIP_CLASS,
                w!(""),
                WS_CHILD | WS_CLIPSIBLINGS,
                0,
                0,
                1,
                1,
                Some(taskbar),
                None,
                Some(self.hinst),
                None,
            ) {
                Ok(strip) => {
                    self.strip = strip;
                    self.rendered = None;
                    self.last_place = None;
                    true
                }
                Err(e) => {
                    log(&format!("CreateWindowExW: {}", e.message()));
                    false
                }
            }
        }
    }

    /// The strip's text pieces after the icon: "42%", "•", "18%" (or "!" / "…"), and whether any shown limit
    /// is at its red level.
    fn strip_text(&self, light: bool) -> (Vec<(String, Rgb)>, bool) {
        let fg: Rgb = if light { (0, 0, 0) } else { (255, 255, 255) };
        let dim: Rgb = if light { (110, 110, 110) } else { (150, 150, 150) };
        let s = self.shared.lock().unwrap();
        let Some(items) = &s.items else {
            return (if s.error.is_some() { vec![("!".into(), low_color(light))] } else { vec![("…".into(), dim)] }, false);
        };
        let stale = s.error.is_some();
        let mut out: Vec<(String, Rgb)> = Vec::new();
        let mut red = false;
        for (key, prefs) in &self.settings.limits {
            // The extra weekly limits only show up when the plan reports them.
            let limit = find_limit(items, key);
            if !prefs.show || (limit.is_none() && !matches!(*key, SESSION | WEEK)) {
                continue;
            }
            let v = limit.map(|l| l.pct);
            red |= v.is_some_and(|p| p >= prefs.red as f64);
            if !out.is_empty() {
                out.push(("•".into(), dim));
            }
            let text = v.map_or("--".to_owned(), |p| format!("{p:.0}%"));
            out.push((text, if stale { dim } else { level_color(v, prefs.red, light, fg) }));
        }
        (out, red && !stale)
    }

    pub fn dpi(&self) -> u32 {
        match unsafe { GetDpiForWindow(self.taskbar) } {
            0 => 96,
            d => d,
        }
    }

    pub fn render(&mut self) {
        let mut client = RECT::default();
        let _ = unsafe { GetClientRect(self.taskbar, &mut client) };
        let dpi = self.dpi();
        let scale = dpi as f32 / 96.0;
        let px = |v: f32| (v * scale).round() as i32;
        let h = (client.bottom - client.top).clamp(px(24.0), px(64.0));
        let light = light_taskbar();
        let (text, red) = self.strip_text(light);
        let fg: Rgb = if light { (0, 0, 0) } else { (255, 255, 255) };
        let icon_rgb = if red && self.settings.red_icon {
            low_color(light)
        } else {
            match self.settings.icon_color {
                IconColor::Taskbar => fg,
                IconColor::Claude => ORANGE,
                IconColor::Rgb(c) => c,
            }
        };
        let background = self.settings.background;
        let key = RenderKey { text: text.clone(), dpi, h, light, hover: self.hover, icon: icon_rgb, background };
        if self.rendered.as_ref() == Some(&key) {
            return;
        }
        let font = Font::text(px(13.0), 400);
        let icon = glyph(px(16.0));
        let masks: Vec<(Mask, Rgb)> = text.iter().map(|(t, c)| (text_mask(t, font.0, light), *c)).collect();
        let (pad, icon_gap, gap) = (px(8.0), px(5.0), px(5.0));
        let text_w: i32 = masks.iter().map(|(m, _)| m.w).sum::<i32>() + gap * (masks.len() as i32 - 1).max(0);
        let w = pad + icon.w + if masks.is_empty() { 0 } else { icon_gap + text_w } + pad;
        let Some(mut surface) = Surface::new(w, h) else {
            return;
        };
        {
            let mut c = surface.canvas();
            // Alpha 1 instead of 0 keeps the whole strip hoverable and clickable while staying invisible.
            c.px.fill([0, 0, 0, 1]);
            let rect = (0.0, px(4.0) as f32, w as f32, (h - 2 * px(4.0)) as f32);
            let mut plate = |color: Rgb, alpha: f32| {
                for y in 0..h {
                    for x in 0..w {
                        let cov = round_rect_cov(x, y, rect, 4.0 * scale);
                        if cov > 0.0 {
                            let i = (y * w + x) as usize;
                            c.px[i] = over(c.px[i], premul(color, (alpha * cov) as u8));
                        }
                    }
                }
            };
            if background {
                // Keeps the text readable on translucent or busy taskbars.
                plate(theme(light).bg, 220.0);
            }
            if self.hover {
                // The taskbar buttons' own hover plate: a faint rounded rectangle.
                if light { plate((0, 0, 0), 14.0) } else { plate((255, 255, 255), 21.0) }
            }
            c.mask_alpha(&icon, pad, (h - icon.h) / 2, icon_rgb, 1.0);
            let mut x = pad + icon.w + icon_gap;
            for (m, color) in &masks {
                c.mask_alpha(m, x, (h - m.h) / 2, *color, 1.0);
                x += m.w + gap;
            }
        }
        surface.present(self.strip, None);
        self.size = (w, h);
        self.rendered = Some(key);
        self.last_place = None;
    }

    /// Shifts the strip left of any other program's widget it would overlap.
    fn avoid_widgets(&self, mut left: i32, width: i32, gap: i32, bar_width: i32) -> i32 {
        let mut scan = Scan { taskbar: self.taskbar, strip: self.strip, found: Vec::new() };
        unsafe {
            let _ = EnumChildWindows(Some(self.taskbar), Some(scan_child), LPARAM(&mut scan as *mut Scan as isize));
        }
        let others: Vec<(i32, i32)> = scan.found.into_iter().filter(|(a, b)| b - a < bar_width / 2).collect();
        let mut moved = true;
        while moved {
            moved = false;
            for &(a, b) in &others {
                if a < left + width && b > left {
                    left = a - width - gap;
                    moved = true;
                }
            }
        }
        left
    }

    /// Keeps the strip just left of the notification area.
    fn place(&mut self) {
        unsafe {
            let Ok(tray) = FindWindowExW(Some(self.taskbar), None, w!("TrayNotifyWnd"), PCWSTR::null()) else {
                return;
            };
            let (mut tr, mut bar, mut client) = (RECT::default(), RECT::default(), RECT::default());
            let _ = GetWindowRect(tray, &mut tr);
            let _ = GetWindowRect(self.taskbar, &mut bar);
            let _ = GetClientRect(self.taskbar, &mut client);
            if bar.bottom - bar.top > bar.right - bar.left {
                return;
            }
            let scale = self.dpi() as f32 / 96.0;
            let (w, h) = self.size;
            let mut p = POINT { x: tr.left, y: bar.top };
            let _ = ScreenToClient(self.taskbar, &mut p);
            let left = p.x - w - ((2 + self.offset + self.settings.offset_x) as f32 * scale).round() as i32;
            let left = self.avoid_widgets(left, w, (4.0 * scale).round() as i32, bar.right - bar.left);
            let top = ((client.bottom - h) / 2 + (self.settings.offset_y as f32 * scale).round() as i32)
                .clamp(0, (client.bottom - h).max(0));
            let key: PlaceKey =
                ([tr.left, tr.top, tr.right, tr.bottom], [bar.left, bar.top, bar.right, bar.bottom], left, top, w, h);
            if self.last_place == Some(key) && IsWindowVisible(self.strip).as_bool() {
                return;
            }
            self.last_place = Some(key);
            let _ = SetWindowPos(self.strip, None, left, top, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    }

    /// Brings everything up to date: notifications, the notification-area icon, the strip and the card.
    pub fn tick(&mut self) {
        self.check_alerts();
        self.sync_tray();
        if self.ensure_strip() {
            if self.settings.tray_icon {
                unsafe {
                    let _ = ShowWindow(self.strip, SW_HIDE);
                }
                self.last_place = None;
            } else {
                self.render();
                self.place();
            }
            if self.card_shown {
                self.show_card();
            }
        }
    }

    /// The menu's header lines: each limit with its reset, the last error and when the usage was read.
    pub fn detail_lines(&self) -> Vec<String> {
        let s = self.shared.lock().unwrap();
        let mut lines = Vec::new();
        for l in s.items.iter().flatten() {
            let reset = l.reset.as_deref().and_then(parse_iso_unix).map(|at| fill(tr().detail_resets, &[("when", &when(at))]));
            lines.push(format!("{}: {:.0}%  {}", label(&l.key), l.pct, reset.unwrap_or_default()).trim_end().to_owned());
        }
        if let Some(e) = &s.error {
            lines.push(fill(tr().error, &[("error", e)]));
        }
        if let Some(t) = s.updated {
            lines.push(fill(tr().checked_at, &[("time", &hm(&t))]));
        }
        if lines.is_empty() {
            lines.push(tr().loading.into());
        }
        lines
    }

    /// Takes the notification-area icons and the strip away before quitting.
    fn shut_down(&mut self) {
        self.end_notify();
        self.remove_tray();
        if !self.strip.is_invalid() {
            unsafe {
                let _ = DestroyWindow(self.strip);
            }
        }
    }
}

pub fn quit() {
    with_app(App::shut_down);
    unsafe { PostQuitMessage(0) };
}

enum Action {
    Handled,
    Menu,
    Default,
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_APP_QUIT {
        with_app(App::shut_down);
        unsafe {
            let _ = DestroyWindow(hwnd);
            PostQuitMessage(0);
        }
        return LRESULT(0);
    }
    if msg == WM_APP_LOGIN {
        crate::login::offer(hwnd);
        return LRESULT(0);
    }
    // try_borrow: the popup menu's modal loop re-enters here; a tick skipped meanwhile is harmless.
    let action = APP.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return Action::Default;
        };
        let Some(app) = guard.as_mut() else {
            return Action::Default;
        };
        if hwnd == app.main {
            return match msg {
                WM_TIMER if wp.0 == CARD_TIMER => {
                    app.card_tick();
                    Action::Handled
                }
                WM_TIMER if wp.0 == NOTIFY_TIMER => {
                    app.end_notify();
                    Action::Handled
                }
                WM_TIMER | WM_APP_DATA => {
                    app.tick();
                    Action::Handled
                }
                WM_APP_TRAY => {
                    if app.tray_event(lp) {
                        Action::Menu
                    } else {
                        Action::Handled
                    }
                }
                m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
                    // Explorer restarted: its notification area forgot our icons.
                    for (_, _, icon) in std::mem::take(&mut app.tray) {
                        unsafe {
                            let _ = DestroyIcon(icon);
                        }
                    }
                    app.tick();
                    Action::Handled
                }
                _ => Action::Default,
            };
        }
        if hwnd != app.strip {
            return Action::Default;
        }
        match msg {
            WM_MOUSEMOVE => {
                if !app.hover {
                    app.hover = true;
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = unsafe { TrackMouseEvent(&mut tme) };
                    app.render();
                    app.show_card();
                }
                Action::Handled
            }
            WM_MOUSELEAVE => {
                app.hover = false;
                app.render();
                Action::Handled
            }
            WM_LBUTTONUP | WM_RBUTTONUP => Action::Menu,
            _ => Action::Default,
        }
    });
    match action {
        Action::Handled => LRESULT(0),
        Action::Menu => {
            menu::show();
            LRESULT(0)
        }
        Action::Default => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// Asks the window of class `class` to quit and waits up to 5 s for it to go away.
pub fn close_instance(class: PCWSTR) {
    unsafe {
        if let Ok(old) = FindWindowW(class, PCWSTR::null()) {
            let _ = PostMessageW(Some(old), WM_APP_QUIT, WPARAM(0), LPARAM(0));
            for _ in 0..50 {
                if !IsWindow(Some(old)).as_bool() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Starts the app and runs the message loop until it quits.
pub fn run(demo: bool, offset: i32) {
    unsafe {
        // A new launch replaces the running instance, so an updated exe takes effect without killing the old one.
        let mut mutex = CreateMutexW(None, false, MUTEX_NAME);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(h) = mutex {
                let _ = CloseHandle(h);
            }
            close_instance(MAIN_CLASS);
            mutex = CreateMutexW(None, false, MUTEX_NAME);
            if GetLastError() == ERROR_ALREADY_EXISTS {
                log("another instance is still running");
                return;
            }
        }
        let _mutex = mutex;
        // The folder picker is a COM object.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        legacy::migrate();
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        for class in [MAIN_CLASS, STRIP_CLASS, CARD_CLASS] {
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: hinst,
                hCursor: cursor,
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassExW(&wc);
        }
        let Ok(main) = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            MAIN_CLASS,
            w!("Claude Monitor"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        ) else {
            log("could not create the main window");
            return;
        };
        let settings = Settings::load();
        settings.apply();
        let (wake_tx, wake_rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        APP.with(|c| {
            *c.borrow_mut() = Some(App {
                hinst,
                main,
                taskbar: HWND::default(),
                strip: HWND::default(),
                card: HWND::default(),
                hover: false,
                card_shown: false,
                card_pos: POINT::default(),
                card_mouse: None,
                card_left_at: None,
                offset,
                size: (0, 0),
                rendered: None,
                last_place: None,
                shared: shared.clone(),
                wake: wake_tx,
                settings,
                alerted: HashSet::new(),
                started: now_unix(),
                notify_icon: None,
                tray: Vec::new(),
                tray_hover: None,
                tray_hovering: false,
                tray_point: POINT::default(),
                promoted: HashSet::new(),
            })
        });
        let main_raw = main.0 as isize;
        std::thread::spawn(move || poll::run(shared, main_raw, demo, wake_rx));
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        SetTimer(Some(main), TICK_TIMER, 1000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// The cursor position on screen.
pub fn cursor_pos() -> POINT {
    let mut pt = POINT::default();
    let _ = unsafe { GetCursorPos(&mut pt) };
    pt
}
