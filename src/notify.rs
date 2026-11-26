//! Windows notifications: a limit reaching its warning level, and a limit resetting.

use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_TIP, NIIF_LARGE_ICON, NIIF_USER, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, KillTimer, SetTimer};

use crate::api::label;
use crate::app::{App, NOTIFY_TIMER};
use crate::gfx::glyph_hicon;
use crate::i18n::{fill, tr};
use crate::time::{now_unix, parse_iso_unix, when};
use crate::util::{fill_wide, log};

/// The id of the temporary icon that carries a notification.
const NOTIFY_UID: u32 = 1;

impl App {
    /// Fires the notifications that are due, each once per limit window.
    pub fn check_alerts(&mut self) {
        let items = {
            let s = self.shared.lock().unwrap();
            if s.error.is_some() {
                return;
            }
            s.items.clone()
        };
        let Some(items) = items else { return };
        let now = now_unix();
        let mut fire: Option<(String, String)> = None;
        for l in &items {
            let prefs = self.settings.limit(&l.key);
            let name = label(&l.key);
            let reset = l.reset.as_deref().and_then(parse_iso_unix);
            let window = reset.map_or(String::new(), |at| at.to_string());
            if prefs.warn > 0 && l.pct >= prefs.warn as f64 && self.alerted.insert(format!("warn:{}@{window}", l.key)) {
                let pct = format!("{:.0}", l.pct);
                let mut body = fill(tr().warn_body, &[("level", &prefs.warn.to_string())]);
                if let Some(at) = reset {
                    body = format!("{body} {}", fill(tr().warn_resets, &[("when", &when(at))]));
                }
                fire = Some((fill(tr().warn_title, &[("label", name), ("pct", &pct)]), body));
            }
            if let Some(at) = reset
                && prefs.notify_reset
                && now >= at
                && at > self.started
                && self.alerted.insert(format!("reset:{}@{at}", l.key))
            {
                fire = Some((fill(tr().reset_title, &[("label", name)]), tr().reset_body.to_owned()));
                let _ = self.wake.send(());
            }
        }
        if let Some((title, body)) = fire {
            self.notify(&title, &body);
        }
    }

    /// A Windows notification: the balloon of a temporary notification-area icon, which Windows 10 / 11
    /// show as a toast. The icon goes away a minute later.
    fn notify(&mut self, title: &str, body: &str) {
        let Some(icon) = glyph_hicon(32) else { return };
        let mut d = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.main,
            uID: NOTIFY_UID,
            uFlags: NIF_ICON | NIF_TIP | NIF_INFO,
            hIcon: icon,
            dwInfoFlags: NIIF_USER | NIIF_LARGE_ICON,
            ..Default::default()
        };
        fill_wide(&mut d.szTip, "Claude Monitor");
        fill_wide(&mut d.szInfoTitle, title);
        fill_wide(&mut d.szInfo, body);
        unsafe {
            if !Shell_NotifyIconW(NIM_MODIFY, &d).as_bool() {
                let _ = Shell_NotifyIconW(NIM_ADD, &d);
            }
            if let Some(old) = self.notify_icon.replace(icon) {
                let _ = DestroyIcon(old);
            }
            SetTimer(Some(self.main), NOTIFY_TIMER, 60_000, None);
        }
        log(&format!("notification: {title}"));
    }

    pub fn end_notify(&mut self) {
        if let Some(icon) = self.notify_icon.take() {
            let d = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.main,
                uID: NOTIFY_UID,
                ..Default::default()
            };
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &d);
                let _ = DestroyIcon(icon);
                let _ = KillTimer(Some(self.main), NOTIFY_TIMER);
            }
        }
    }
}
