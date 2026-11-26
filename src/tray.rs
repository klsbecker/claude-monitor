//! Icon mode: the usage as one notification-area icon (session on top, week below) instead of the strip.

use std::ffi::c_void;

use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, RECT};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_DWORD, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegSetKeyValueW,
};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
    NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetWindowRect, HICON, SM_CXSMICON};
use windows::core::{PCWSTR, PWSTR, w};

use crate::api::{SESSION, WEEK, find_limit, label};
use crate::app::{App, WM_APP_TRAY, cursor_pos};
use crate::gfx::{ORANGE, Rgb, level_color, light_taskbar, stacked_hicon};
use crate::registry::{dword_at, string_at};
use crate::util::fill_wide;

/// The id of the usage icon (1 is the temporary notification icon).
pub const TRAY_UID: u32 = 10;

/// The text lines of an icon, each with its color.
type Lines = Vec<(String, Rgb)>;

/// What an icon shows: its lines, tooltip and size.
pub type IconKey = (Lines, String, i32);

/// Windows 11 puts new notification-area icons under the overflow arrow. Since icon mode was chosen to see
/// them, each of ours is shown on the taskbar once, unless the user already decided (IsPromoted set). Returns
/// the ids whose entry exists (Explorer writes it shortly after the icon is added).
fn promote_icons(ids: &[u32]) -> Vec<u32> {
    let Ok(exe) = std::env::current_exe() else { return Vec::new() };
    let exe = exe.display().to_string().to_lowercase();
    let mut handled = Vec::new();
    unsafe {
        let mut root = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, w!("Control Panel\\NotifyIconSettings"), Some(0), KEY_READ | KEY_SET_VALUE, &mut root)
            != ERROR_SUCCESS
        {
            return handled;
        }
        let mut i = 0;
        loop {
            let mut name = [0u16; 64];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(root, i, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) != ERROR_SUCCESS {
                break;
            }
            i += 1;
            let sub = PCWSTR(name.as_ptr());
            let path = string_at(root, sub, w!("ExecutablePath")).unwrap_or_default().to_lowercase();
            let Some(uid) = dword_at(root, sub, w!("UID")) else { continue };
            if path != exe || !ids.contains(&uid) {
                continue;
            }
            if dword_at(root, sub, w!("IsPromoted")).is_none() {
                let one = 1u32;
                let _ = RegSetKeyValueW(root, sub, w!("IsPromoted"), REG_DWORD.0, Some(&one as *const u32 as *const c_void), 4);
            }
            handled.push(uid);
        }
        let _ = RegCloseKey(root);
    }
    handled
}

impl App {
    /// In icon mode, one notification-area icon with the session % on top and the week % under it (only the
    /// limits marked "show on the taskbar"); otherwise none. Windows decides its place (and may hide it under
    /// the overflow arrow).
    pub fn sync_tray(&mut self) {
        let light = light_taskbar();
        let fg: Rgb = if light { (0, 0, 0) } else { (255, 255, 255) };
        let mut wanted: Vec<(u32, Lines, String)> = Vec::new();
        if self.settings.tray_icon {
            let s = self.shared.lock().unwrap();
            let stale = s.error.is_some();
            let dim: Rgb = if light { (110, 110, 110) } else { (150, 150, 150) };
            let mut lines = Vec::new();
            let mut tip = vec!["Claude".to_owned()];
            for key in [SESSION, WEEK] {
                let prefs = self.settings.limit(key);
                if !prefs.show {
                    continue;
                }
                let v = s.items.as_ref().and_then(|items| find_limit(items, key)).map(|l| l.pct);
                let text = match (v, &s.error) {
                    (Some(p), _) => format!("{p:.0}"),
                    (None, Some(_)) => "!".to_owned(),
                    (None, None) => "…".to_owned(),
                };
                lines.push((text, if stale { dim } else { level_color(v, prefs.red, light, fg) }));
                if let Some(p) = v {
                    tip.push(format!("{}: {p:.0}%", label(key)));
                }
            }
            if lines.is_empty() {
                // Nothing marked to show: the icon still has to exist to reach the card and the menu.
                lines.push(("C".to_owned(), ORANGE));
            }
            wanted.push((TRAY_UID, lines, tip.join(" · ")));
        }
        let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
        for (id, lines, tip) in &wanted {
            let key = (lines.clone(), tip.clone(), size);
            if self.tray.iter().any(|t| t.0 == *id && t.1 == key) {
                continue;
            }
            let Some(icon) = stacked_hicon(lines, size) else { continue };
            let mut d = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.main,
                uID: *id,
                uFlags: NIF_ICON | NIF_TIP | NIF_MESSAGE,
                uCallbackMessage: WM_APP_TRAY,
                hIcon: icon,
                ..Default::default()
            };
            fill_wide(&mut d.szTip, tip);
            unsafe {
                let known = self.tray.iter().any(|t| t.0 == *id);
                if !known || !Shell_NotifyIconW(NIM_MODIFY, &d).as_bool() {
                    let _ = Shell_NotifyIconW(NIM_ADD, &d);
                    // Version 4: hover and click arrive as NIN_POPUPOPEN / NIN_SELECT / WM_CONTEXTMENU,
                    // and without NIF_SHOWTIP Windows leaves the hover to the card.
                    d.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                    let _ = Shell_NotifyIconW(NIM_SETVERSION, &d);
                }
            }
            if let Some(pos) = self.tray.iter().position(|t| t.0 == *id) {
                let old = self.tray.remove(pos);
                unsafe {
                    let _ = DestroyIcon(old.2);
                }
            }
            self.tray.push((*id, key, icon));
        }
        if self.tray.iter().any(|t| !self.promoted.contains(&t.0)) {
            let ids: Vec<u32> = self.tray.iter().map(|t| t.0).collect();
            self.promoted.extend(promote_icons(&ids));
        }
        let gone: Vec<usize> = (0..self.tray.len()).rev().filter(|&i| !wanted.iter().any(|w| w.0 == self.tray[i].0)).collect();
        for i in gone {
            let (id, _, icon) = self.tray.remove(i);
            self.remove_tray_icon(id, icon);
        }
    }

    fn remove_tray_icon(&self, id: u32, icon: HICON) {
        let d = NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.main, uID: id, ..Default::default() };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &d);
            let _ = DestroyIcon(icon);
        }
    }

    pub fn remove_tray(&mut self) {
        for (id, _, icon) in std::mem::take(&mut self.tray) {
            self.remove_tray_icon(id, icon);
        }
    }

    /// Handles a notification-area icon event; returns whether to open the menu.
    pub fn tray_event(&mut self, lp: LPARAM) -> bool {
        // NOTIFYICON_VERSION_4: event in the low word of lParam, icon id in the high word.
        const NIN_SELECT: u32 = 0x400;
        const NIN_KEYSELECT: u32 = 0x401;
        const NIN_POPUPOPEN: u32 = 0x406;
        const NIN_POPUPCLOSE: u32 = 0x407;
        const WM_CONTEXTMENU: u32 = 0x7B;
        let (event, id) = ((lp.0 & 0xFFFF) as u32, ((lp.0 >> 16) & 0xFFFF) as u32);
        match event {
            NIN_POPUPOPEN => {
                self.tray_hover = Some(id);
                self.tray_hovering = true;
                self.card_left_at = None;
                self.tray_point = cursor_pos();
                self.show_card();
                false
            }
            NIN_POPUPCLOSE => {
                self.tray_hovering = false;
                false
            }
            NIN_SELECT | NIN_KEYSELECT | WM_CONTEXTMENU => true,
            _ => false,
        }
    }

    /// What the card hangs from: the strip, or in icon mode the notification-area icon last hovered.
    pub fn anchor_rect(&self) -> RECT {
        let mut r = RECT::default();
        if self.settings.tray_icon {
            let nid = NOTIFYICONIDENTIFIER {
                cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.main,
                uID: self.tray_hover.unwrap_or(TRAY_UID),
                ..Default::default()
            };
            // For icons in the overflow flyout Windows may report another place (or none): then the point
            // where the hover began.
            let p = self.tray_point;
            r = unsafe { Shell_NotifyIconGetRect(&nid) }
                .ok()
                .filter(|r| p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom)
                .unwrap_or(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 });
        } else {
            let _ = unsafe { GetWindowRect(self.strip, &mut r) };
        }
        r
    }
}
