//! User settings, saved as DWORD values under `HKCU\Software\ClaudeMonitor`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::PCWSTR;

use crate::api::{self, LIMIT_KEYS, SESSION, WEEK};
use crate::gfx::Rgb;
use crate::i18n::{self, Lang};
use crate::registry::{SETTINGS_KEY, read_dword, read_string, write_dword, write_string};

/// Refresh intervals offered in the menu, in minutes.
pub const INTERVALS: [u32; 5] = [1, 2, 5, 10, 30];
const DEFAULT_INTERVAL: u32 = 5;
pub const RED_LEVELS: [u32; 6] = [50, 60, 70, 80, 90, 95];
pub const DEFAULT_RED: u32 = 80;
/// 0 = no notification.
pub const WARN_LEVELS: [u32; 6] = [0, 50, 75, 80, 90, 95];
pub const DEFAULT_WARN: u32 = 90;
/// Distance from the notification area, in px at 96 DPI.
pub const POSITION_OFFSETS: [i32; 11] = [0, 20, 40, 60, 80, 100, 120, 140, 160, 180, 200];
pub const POSITION_MAX: i32 = 1000;
/// Shift from the taskbar's vertical center, in px at 96 DPI.
pub const HEIGHT_OFFSETS: [i32; 7] = [-15, -10, -5, 0, 5, 10, 15];
pub const HEIGHT_MAX: i32 = 40;
pub const COLOR_PRESETS: [Rgb; 6] = [(46, 204, 64), (0, 120, 215), (0, 183, 195), (155, 89, 182), (232, 17, 123), (247, 99, 12)];

/// Minutes between reads; the poll thread reads it before each wait.
pub static INTERVAL_MIN: AtomicU32 = AtomicU32::new(DEFAULT_INTERVAL);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IconColor {
    /// The taskbar's text color.
    Taskbar,
    Claude,
    Rgb(Rgb),
}

impl IconColor {
    pub fn encode(self) -> u32 {
        match self {
            IconColor::Taskbar => 0,
            IconColor::Claude => 1,
            IconColor::Rgb((r, g, b)) => 0x0100_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32,
        }
    }

    pub fn decode(v: u32) -> IconColor {
        match v {
            1 => IconColor::Claude,
            v if v & 0x0100_0000 != 0 => IconColor::Rgb(((v >> 16) as u8, (v >> 8) as u8, v as u8)),
            _ => IconColor::Taskbar,
        }
    }
}

/// Per-limit options, saved as `<Option>.<limit key>`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LimitPrefs {
    pub show: bool,
    pub red: u32,
    pub warn: u32,
    pub notify_reset: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    pub interval: u32,
    pub tray_icon: bool,
    pub background: bool,
    pub offset_x: i32,
    pub offset_y: i32,
    pub red_icon: bool,
    pub icon_color: IconColor,
    /// `None` follows the Windows display language.
    pub language: Option<Lang>,
    /// The Claude Code folder to read the sign-in from; `None` is Claude Code's own.
    pub config_dir: Option<PathBuf>,
    pub limits: Vec<(&'static str, LimitPrefs)>,
}

impl Settings {
    pub fn load() -> Settings {
        Settings::load_from(SETTINGS_KEY)
    }

    /// The settings saved under `key` (any missing or invalid value gets its default).
    pub fn load_from(key: PCWSTR) -> Settings {
        let get = |name: &str| read_dword(key, name);
        let limits = LIMIT_KEYS
            .iter()
            .map(|key| {
                let prefs = LimitPrefs {
                    show: get(&format!("Show.{key}")).map_or(matches!(*key, SESSION | WEEK), |v| v != 0),
                    red: get(&format!("Red.{key}")).filter(|v| RED_LEVELS.contains(v)).unwrap_or(DEFAULT_RED),
                    warn: get(&format!("Warn.{key}")).filter(|v| WARN_LEVELS.contains(v)).unwrap_or(DEFAULT_WARN),
                    notify_reset: get(&format!("NotifyReset.{key}")).is_some_and(|v| v != 0),
                };
                (*key, prefs)
            })
            .collect();
        Settings {
            interval: get("Interval").filter(|v| INTERVALS.contains(v)).unwrap_or(DEFAULT_INTERVAL),
            tray_icon: get("TrayIcons").is_some_and(|v| v != 0),
            background: get("Background").is_some_and(|v| v != 0),
            offset_x: get("OffsetX").map_or(0, |v| (v as i32).clamp(0, POSITION_MAX)),
            offset_y: get("OffsetY").map_or(0, |v| (v as i32).clamp(-HEIGHT_MAX, HEIGHT_MAX)),
            red_icon: get("RedIcon").is_some_and(|v| v != 0),
            icon_color: get("IconColor").map_or(IconColor::Taskbar, IconColor::decode),
            language: get("Language").and_then(Lang::from_code),
            config_dir: read_string(key, "ConfigDir").filter(|d| !d.is_empty()).map(PathBuf::from),
            limits,
        }
    }

    pub fn save(&self) {
        let set = |name: &str, value: u32| write_dword(SETTINGS_KEY, name, value);
        set("Interval", self.interval);
        set("TrayIcons", self.tray_icon as u32);
        set("Background", self.background as u32);
        set("OffsetX", self.offset_x as u32);
        set("OffsetY", self.offset_y as u32);
        set("RedIcon", self.red_icon as u32);
        set("IconColor", self.icon_color.encode());
        set("Language", self.language.map_or(0, Lang::code));
        write_string(SETTINGS_KEY, "ConfigDir", self.config_dir.as_ref().map(|d| d.to_string_lossy()).as_deref());
        for (key, p) in &self.limits {
            set(&format!("Show.{key}"), p.show as u32);
            set(&format!("Red.{key}"), p.red);
            set(&format!("Warn.{key}"), p.warn);
            set(&format!("NotifyReset.{key}"), p.notify_reset as u32);
        }
    }

    /// Makes the settings that other threads and modules read globally take effect.
    pub fn apply(&self) {
        INTERVAL_MIN.store(self.interval, Ordering::Relaxed);
        i18n::set_lang(self.language.unwrap_or_else(Lang::system));
        api::set_config_dir(self.config_dir.clone());
    }

    pub fn limit(&self, key: &str) -> LimitPrefs {
        self.limits.iter().find(|(k, _)| *k == key).map(|l| l.1).unwrap_or(LimitPrefs {
            show: false,
            red: DEFAULT_RED,
            warn: 0,
            notify_reset: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_color_round_trips() {
        for c in [IconColor::Taskbar, IconColor::Claude, IconColor::Rgb((1, 2, 3)), IconColor::Rgb((0, 0, 0))] {
            assert_eq!(IconColor::decode(c.encode()), c);
        }
    }
}
