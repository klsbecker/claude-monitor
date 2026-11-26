//! The context menu (click on the strip or the icon) and its commands.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, HMENU, IDYES, MB_ICONWARNING, MB_YESNO, MENU_ITEM_FLAGS, MF_CHECKED, MF_GRAYED,
    MF_POPUP, MF_SEPARATOR, MF_STRING, MessageBoxW, PostMessageW, SW_SHOWNORMAL, SetForegroundWindow, TPM_BOTTOMALIGN,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
};
use windows::core::{PCWSTR, w};

use crate::api::{LIMIT_KEYS, SESSION, WEEK, default_config_dir, find_config_dirs, label};
use crate::app::{change_settings, cursor_pos, quit, with_app};
use crate::gfx::{ORANGE, Rgb};
use crate::history;
use crate::i18n::{Lang, fill, tr};
use crate::registry::{autostart_enabled, set_autostart};
use crate::settings::{
    COLOR_PRESETS, DEFAULT_RED, DEFAULT_WARN, HEIGHT_MAX, HEIGHT_OFFSETS, INTERVALS, IconColor, POSITION_MAX, POSITION_OFFSETS,
    RED_LEVELS, WARN_LEVELS,
};
use crate::util::{home, wide};

pub const REPO_URL: &str = "https://github.com/klsbecker/claude-monitor";

const CMD_REFRESH: usize = 1;
const CMD_AUTOSTART: usize = 2;
const CMD_QUIT: usize = 3;
const CMD_LOGIN: usize = 4;
const CMD_WEB: usize = 5;
const CMD_BACKGROUND: usize = 6;
const CMD_RED_ICON: usize = 7;
const CMD_CLEAR: usize = 8;
const CMD_COLOR_TASKBAR: usize = 9;
const CMD_COLOR_CLAUDE: usize = 10;
const CMD_COLOR_CUSTOM: usize = 11;
const CMD_ABOUT: usize = 12;
const CMD_COLOR_PRESET: usize = 13;
const CMD_POS_LEFT: usize = 20;
const CMD_POS_RIGHT: usize = 21;
const CMD_POS_FIRST: usize = 22;
const CMD_HEIGHT_UP: usize = 40;
const CMD_HEIGHT_DOWN: usize = 41;
const CMD_HEIGHT_FIRST: usize = 42;
const CMD_INTERVAL_FIRST: usize = 60;
const CMD_TRAY_ICON: usize = 70;
const CMD_LANG_AUTO: usize = 80;
const CMD_LANG_FIRST: usize = 81;
const CMD_CONFIG_DEFAULT: usize = 90;
const CMD_CONFIG_CHOOSE: usize = 91;
const CMD_CONFIG_FIRST: usize = 92;
const CONFIG_MAX: usize = 8;
/// Per-limit items: CMD_LIMIT_BASE + limit index * CMD_LIMIT_STRIDE + one of the LIMIT_* offsets.
const CMD_LIMIT_BASE: usize = 100;
const CMD_LIMIT_STRIDE: usize = 20;
const LIMIT_SHOW: usize = 0;
const LIMIT_RESET: usize = 1;
const LIMIT_RED: usize = 2;
const LIMIT_WARN: usize = 10;

fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, label: &str) {
    let text = wide(label);
    unsafe {
        let _ = AppendMenuW(menu, flags, id, PCWSTR(text.as_ptr()));
    }
}

fn append_sub(menu: HMENU, flags: MENU_ITEM_FLAGS, sub: HMENU, label: &str) {
    append(menu, MF_STRING | MF_POPUP | flags, sub.0 as usize, label);
}

fn checked(on: bool) -> MENU_ITEM_FLAGS {
    if on { MF_STRING | MF_CHECKED } else { MF_STRING }
}

fn enabled(on: bool) -> MENU_ITEM_FLAGS {
    if on { MF_STRING } else { MF_STRING | MF_GRAYED }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches('\\'))
}

/// `path` with the home folder shortened to `~`.
fn tilde(path: &Path) -> String {
    match path.strip_prefix(home()) {
        Ok(rest) => format!("~\\{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// The folders listed besides the default: those with a sign-in found in the home folder, plus the chosen one.
fn config_folders(chosen: Option<&PathBuf>) -> Vec<PathBuf> {
    let default = default_config_dir();
    let mut folders: Vec<PathBuf> = find_config_dirs().into_iter().filter(|d| !same_path(d, &default)).collect();
    if let Some(c) = chosen
        && !folders.iter().any(|d| same_path(d, c))
    {
        folders.push(c.clone());
    }
    folders.truncate(CONFIG_MAX);
    folders
}

fn open_url(url: &str) {
    let url = wide(url);
    unsafe {
        let _ = ShellExecuteW(None, w!("open"), PCWSTR(url.as_ptr()), None, None, SW_SHOWNORMAL);
    }
}

/// Builds the menu at the cursor and runs the chosen command.
pub fn show() {
    let Some((main, lines, wake, settings, present)) = with_app(|a| {
        a.hide_card();
        let present: Vec<String> = a.shared.lock().unwrap().items.iter().flatten().map(|l| l.key.clone()).collect();
        (a.main, a.detail_lines(), a.wake.clone(), a.settings.clone(), present)
    }) else {
        return;
    };
    let t = tr();
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        for line in &lines {
            append(menu, MF_STRING | MF_GRAYED, 0, line);
        }
        append(menu, MF_SEPARATOR, 0, "");
        append(menu, MF_STRING, CMD_WEB, t.menu_web);
        append(menu, MF_STRING, CMD_REFRESH, t.menu_refresh);
        if let Ok(sub) = CreatePopupMenu() {
            for (i, m) in INTERVALS.iter().enumerate() {
                let label = if *m == 1 { t.one_minute.to_owned() } else { fill(t.minutes, &[("n", &m.to_string())]) };
                append(sub, checked(*m == settings.interval), CMD_INTERVAL_FIRST + i, &label);
            }
            append_sub(menu, MENU_ITEM_FLAGS(0), sub, t.menu_interval);
        }
        append(menu, MF_SEPARATOR, 0, "");

        // One submenu per limit: shown on the strip, red level, notifications.
        for (i, (key, prefs)) in settings.limits.iter().enumerate() {
            if !matches!(*key, SESSION | WEEK) && !present.iter().any(|k| k == key) {
                continue;
            }
            let Ok(sub) = CreatePopupMenu() else { continue };
            let id = |item: usize| CMD_LIMIT_BASE + i * CMD_LIMIT_STRIDE + item;
            append(sub, checked(prefs.show), id(LIMIT_SHOW), t.menu_show);
            if let Ok(levels) = CreatePopupMenu() {
                for (j, v) in RED_LEVELS.iter().enumerate() {
                    let def = if *v == DEFAULT_RED { t.default_suffix } else { "" };
                    append(levels, checked(*v == prefs.red), id(LIMIT_RED + j), &format!("{v}%{def}"));
                }
                append_sub(sub, MENU_ITEM_FLAGS(0), levels, &fill(t.menu_red_from, &[("pct", &prefs.red.to_string())]));
            }
            if let Ok(levels) = CreatePopupMenu() {
                for (j, v) in WARN_LEVELS.iter().enumerate() {
                    let label = match *v {
                        0 => t.never.to_owned(),
                        v if v == DEFAULT_WARN => format!("{v}%{}", t.default_suffix),
                        v => format!("{v}%"),
                    };
                    append(levels, checked(*v == prefs.warn), id(LIMIT_WARN + j), &label);
                }
                let label = if prefs.warn == 0 {
                    t.menu_warn_never.to_owned()
                } else {
                    fill(t.menu_warn_at, &[("pct", &prefs.warn.to_string())])
                };
                append_sub(sub, MENU_ITEM_FLAGS(0), levels, &label);
            }
            append(sub, checked(prefs.notify_reset), id(LIMIT_RESET), t.menu_notify_reset);
            append_sub(menu, MENU_ITEM_FLAGS(0), sub, label(key));
        }
        append(menu, MF_SEPARATOR, 0, "");

        // Look and place of the strip.
        append(menu, checked(settings.tray_icon), CMD_TRAY_ICON, t.menu_tray_icon);
        // These only change the strip, which icon mode hides.
        let strip_only = if settings.tray_icon { MF_GRAYED } else { MENU_ITEM_FLAGS(0) };
        append(menu, checked(settings.red_icon) | strip_only, CMD_RED_ICON, t.menu_red_icon);
        if let Ok(sub) = CreatePopupMenu() {
            let c = settings.icon_color;
            append(sub, checked(c == IconColor::Taskbar), CMD_COLOR_TASKBAR, t.color_taskbar);
            append(sub, checked(c == IconColor::Claude), CMD_COLOR_CLAUDE, t.color_claude);
            append(sub, MF_SEPARATOR, 0, "");
            let mut preset = false;
            for (j, (rgb, name)) in COLOR_PRESETS.iter().zip(t.color_presets).enumerate() {
                let on = c == IconColor::Rgb(*rgb);
                preset |= on;
                append(sub, checked(on), CMD_COLOR_PRESET + j, name);
            }
            append(sub, MF_SEPARATOR, 0, "");
            append(sub, checked(matches!(c, IconColor::Rgb(_)) && !preset), CMD_COLOR_CUSTOM, t.color_custom);
            append_sub(menu, strip_only, sub, t.menu_icon_color);
        }
        append(menu, checked(settings.background) | strip_only, CMD_BACKGROUND, t.menu_background);
        if let Ok(sub) = CreatePopupMenu() {
            let x = settings.offset_x;
            let left = |px: i32| fill(t.px_left, &[("px", &px.to_string())]);
            for (j, px) in POSITION_OFFSETS.iter().enumerate() {
                let label = if *px == 0 { t.position_default.to_owned() } else { left(*px) };
                append(sub, checked(*px == x), CMD_POS_FIRST + j, &label);
            }
            if !POSITION_OFFSETS.contains(&x) {
                append(sub, MF_STRING | MF_CHECKED | MF_GRAYED, 0, &fill(t.custom, &[("value", &left(x))]));
            }
            append(sub, MF_SEPARATOR, 0, "");
            append(sub, enabled(x < POSITION_MAX), CMD_POS_LEFT, t.move_left);
            append(sub, enabled(x > 0), CMD_POS_RIGHT, t.move_right);
            append_sub(menu, strip_only, sub, t.menu_position);
        }
        if let Ok(sub) = CreatePopupMenu() {
            let y = settings.offset_y;
            let shift = |px: i32| match px {
                0 => t.centered.to_owned(),
                p if p < 0 => fill(t.px_up, &[("px", &(-p).to_string())]),
                p => fill(t.px_down, &[("px", &p.to_string())]),
            };
            for (j, px) in HEIGHT_OFFSETS.iter().enumerate() {
                append(sub, checked(*px == y), CMD_HEIGHT_FIRST + j, &shift(*px));
            }
            if !HEIGHT_OFFSETS.contains(&y) {
                append(sub, MF_STRING | MF_CHECKED | MF_GRAYED, 0, &fill(t.custom, &[("value", &shift(y))]));
            }
            append(sub, MF_SEPARATOR, 0, "");
            append(sub, enabled(y > -HEIGHT_MAX), CMD_HEIGHT_UP, t.move_up);
            append(sub, enabled(y < HEIGHT_MAX), CMD_HEIGHT_DOWN, t.move_down);
            append_sub(menu, strip_only, sub, t.menu_height);
        }
        append(menu, MF_SEPARATOR, 0, "");
        let folders = config_folders(settings.config_dir.as_ref());
        if let Ok(sub) = CreatePopupMenu() {
            let default = fill(t.config_default, &[("path", &tilde(&default_config_dir()))]);
            append(sub, checked(settings.config_dir.is_none()), CMD_CONFIG_DEFAULT, &default);
            for (j, dir) in folders.iter().enumerate() {
                let on = settings.config_dir.as_ref().is_some_and(|c| same_path(c, dir));
                append(sub, checked(on), CMD_CONFIG_FIRST + j, &tilde(dir));
            }
            append(sub, MF_SEPARATOR, 0, "");
            append(sub, MF_STRING, CMD_CONFIG_CHOOSE, t.config_choose);
            append_sub(menu, MENU_ITEM_FLAGS(0), sub, t.menu_config_dir);
        }
        if let Ok(sub) = CreatePopupMenu() {
            append(sub, checked(settings.language.is_none()), CMD_LANG_AUTO, t.language_auto);
            append(sub, MF_SEPARATOR, 0, "");
            for (j, l) in Lang::ALL.iter().enumerate() {
                append(sub, checked(settings.language == Some(*l)), CMD_LANG_FIRST + j, l.native_name());
            }
            append_sub(menu, MENU_ITEM_FLAGS(0), sub, t.menu_language);
        }
        append(menu, MF_STRING, CMD_LOGIN, t.menu_sign_in);
        append(menu, checked(autostart_enabled()), CMD_AUTOSTART, t.menu_autostart);
        append(menu, MF_STRING, CMD_CLEAR, t.menu_clear_history);
        let about = fill(t.menu_about, &[("version", concat!("v", env!("CARGO_PKG_VERSION")))]);
        append(menu, MF_STRING, CMD_ABOUT, &about);
        append(menu, MF_STRING, CMD_QUIT, t.menu_quit);

        let pt = cursor_pos();
        let _ = SetForegroundWindow(main);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, None, main, None);
        let _ = PostMessageW(Some(main), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        run(cmd.0 as usize, main, &wake, &folders);
    }
}

/// The folder picker; `None` when cancelled.
fn pick_folder(owner: HWND, initial: &Path) -> Option<PathBuf> {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
        SIGDN_FILESYSPATH,
    };
    use windows::core::HSTRING;
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog.SetOptions(dialog.GetOptions().ok()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        if let Ok(folder) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(initial.as_os_str()), None) {
            let _ = dialog.SetFolder(&folder);
        }
        dialog.Show(Some(owner)).ok()?;
        let name = dialog.GetResult().ok()?.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(name.0 as *const _));
        path
    }
}

/// Reads the sign-in from `dir` (`None`: Claude Code's own folder) and reads the usage again right away.
fn set_config_dir(dir: Option<PathBuf>, wake: &Sender<()>) {
    change_settings(|s| s.config_dir = dir);
    let _ = wake.send(());
}

fn pick_color(main: HWND, initial: Rgb) -> Option<Rgb> {
    use windows::Win32::UI::Controls::Dialogs::{CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW};
    let mut custom = [COLORREF(0x00FF_FFFF); 16];
    let (r, g, b) = initial;
    let mut cc = CHOOSECOLORW {
        lStructSize: size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: main,
        rgbResult: COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16),
        lpCustColors: custom.as_mut_ptr(),
        Flags: CC_FULLOPEN | CC_RGBINIT,
        ..Default::default()
    };
    if !unsafe { ChooseColorW(&mut cc) }.as_bool() {
        return None;
    }
    let c = cc.rgbResult.0;
    Some((c as u8, (c >> 8) as u8, (c >> 16) as u8))
}

fn clear_history(main: HWND) {
    let text = wide(tr().clear_history_confirm);
    let answer = unsafe { MessageBoxW(Some(main), PCWSTR(text.as_ptr()), w!("Claude Monitor"), MB_YESNO | MB_ICONWARNING) };
    if answer == IDYES {
        history::clear();
        with_app(|a| {
            a.shared.lock().unwrap().history.clear();
            a.tick();
        });
    }
}

fn run(cmd: usize, main: HWND, wake: &Sender<()>, folders: &[PathBuf]) {
    let in_range = |first: usize, n: usize| (first..first + n).contains(&cmd).then(|| cmd - first);
    match cmd {
        CMD_REFRESH => {
            let _ = wake.send(());
        }
        CMD_WEB => open_url("https://claude.ai/settings/usage"),
        CMD_ABOUT => open_url(REPO_URL),
        CMD_AUTOSTART => set_autostart(!autostart_enabled()),
        CMD_LOGIN => crate::login::offer(main),
        CMD_BACKGROUND => change_settings(|s| s.background = !s.background),
        CMD_TRAY_ICON => change_settings(|s| s.tray_icon = !s.tray_icon),
        CMD_RED_ICON => change_settings(|s| s.red_icon = !s.red_icon),
        CMD_COLOR_TASKBAR => change_settings(|s| s.icon_color = IconColor::Taskbar),
        CMD_COLOR_CLAUDE => change_settings(|s| s.icon_color = IconColor::Claude),
        CMD_COLOR_CUSTOM => {
            let initial = with_app(|a| match a.settings.icon_color {
                IconColor::Rgb(c) => c,
                _ => ORANGE,
            })
            .unwrap_or(ORANGE);
            if let Some(c) = pick_color(main, initial) {
                change_settings(|s| s.icon_color = IconColor::Rgb(c));
            }
        }
        CMD_POS_LEFT => change_settings(|s| s.offset_x = (s.offset_x + 10).min(POSITION_MAX)),
        CMD_POS_RIGHT => change_settings(|s| s.offset_x = (s.offset_x - 10).max(0)),
        CMD_HEIGHT_UP => change_settings(|s| s.offset_y = (s.offset_y - 10).max(-HEIGHT_MAX)),
        CMD_HEIGHT_DOWN => change_settings(|s| s.offset_y = (s.offset_y + 10).min(HEIGHT_MAX)),
        CMD_LANG_AUTO => change_settings(|s| s.language = None),
        CMD_CONFIG_DEFAULT => set_config_dir(None, wake),
        CMD_CONFIG_CHOOSE => {
            if let Some(dir) = pick_folder(main, &crate::api::config_dir()) {
                let dir = if same_path(&dir, &default_config_dir()) { None } else { Some(dir) };
                set_config_dir(dir, wake);
            }
        }
        CMD_CLEAR => clear_history(main),
        CMD_QUIT => quit(),
        _ => {
            if let Some(j) = in_range(CMD_COLOR_PRESET, COLOR_PRESETS.len()) {
                change_settings(|s| s.icon_color = IconColor::Rgb(COLOR_PRESETS[j]));
            } else if let Some(j) = in_range(CMD_POS_FIRST, POSITION_OFFSETS.len()) {
                change_settings(|s| s.offset_x = POSITION_OFFSETS[j]);
            } else if let Some(j) = in_range(CMD_HEIGHT_FIRST, HEIGHT_OFFSETS.len()) {
                change_settings(|s| s.offset_y = HEIGHT_OFFSETS[j]);
            } else if let Some(j) = in_range(CMD_INTERVAL_FIRST, INTERVALS.len()) {
                change_settings(|s| s.interval = INTERVALS[j]);
            } else if let Some(j) = in_range(CMD_CONFIG_FIRST, folders.len()) {
                set_config_dir(Some(folders[j].clone()), wake);
            } else if let Some(j) = in_range(CMD_LANG_FIRST, Lang::ALL.len()) {
                change_settings(|s| s.language = Some(Lang::ALL[j]));
            } else if let Some(rel) = in_range(CMD_LIMIT_BASE, LIMIT_KEYS.len() * CMD_LIMIT_STRIDE) {
                let (i, item) = (rel / CMD_LIMIT_STRIDE, rel % CMD_LIMIT_STRIDE);
                change_settings(|s| {
                    let p = &mut s.limits[i].1;
                    match item {
                        LIMIT_SHOW => p.show = !p.show,
                        LIMIT_RESET => p.notify_reset = !p.notify_reset,
                        x if (LIMIT_RED..LIMIT_RED + RED_LEVELS.len()).contains(&x) => p.red = RED_LEVELS[x - LIMIT_RED],
                        x if (LIMIT_WARN..LIMIT_WARN + WARN_LEVELS.len()).contains(&x) => p.warn = WARN_LEVELS[x - LIMIT_WARN],
                        _ => {}
                    }
                });
            }
        }
    }
}
