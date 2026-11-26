//! Registry access under HKEY_CURRENT_USER: the app's settings key and the autostart entry.

use std::ffi::c_void;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegCloseKey, RegDeleteKeyValueW,
    RegGetValueW, RegOpenKeyExW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

use crate::util::wide;

pub const SETTINGS_KEY: PCWSTR = w!("Software\\ClaudeMonitor");
pub const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_NAME: PCWSTR = w!("ClaudeMonitor");

/// A REG_DWORD value of `key\sub`.
pub fn dword_at(key: HKEY, sub: PCWSTR, name: PCWSTR) -> Option<u32> {
    let mut v = 0u32;
    let mut len = size_of::<u32>() as u32;
    let r =
        unsafe { RegGetValueW(key, sub, name, RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as *mut c_void), Some(&mut len)) };
    (r == ERROR_SUCCESS).then_some(v)
}

/// A REG_SZ value of `key\sub`.
pub fn string_at(key: HKEY, sub: PCWSTR, name: PCWSTR) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let r = unsafe { RegGetValueW(key, sub, name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut c_void), Some(&mut len)) };
    (r == ERROR_SUCCESS).then(|| String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
}

pub fn read_dword(sub: PCWSTR, name: &str) -> Option<u32> {
    let name = wide(name);
    dword_at(HKEY_CURRENT_USER, sub, PCWSTR(name.as_ptr()))
}

pub fn write_dword(sub: PCWSTR, name: &str, value: u32) {
    let name = wide(name);
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            sub,
            PCWSTR(name.as_ptr()),
            REG_DWORD.0,
            Some(&value as *const u32 as *const c_void),
            size_of::<u32>() as u32,
        );
    }
}

/// A REG_SZ value under HKEY_CURRENT_USER.
pub fn read_string(sub: PCWSTR, name: &str) -> Option<String> {
    let name = wide(name);
    string_at(HKEY_CURRENT_USER, sub, PCWSTR(name.as_ptr()))
}

/// Writes a REG_SZ value, or deletes it when `value` is `None`.
pub fn write_string(sub: PCWSTR, name: &str, value: Option<&str>) {
    let name = wide(name);
    let Some(value) = value else {
        delete_value(sub, PCWSTR(name.as_ptr()));
        return;
    };
    let data = wide(value);
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            sub,
            PCWSTR(name.as_ptr()),
            REG_SZ.0,
            Some(data.as_ptr() as *const c_void),
            (data.len() * 2) as u32,
        );
    }
}

pub fn key_exists(sub: PCWSTR) -> bool {
    let mut key = HKEY::default();
    unsafe {
        let ok = RegOpenKeyExW(HKEY_CURRENT_USER, sub, Some(0), KEY_READ, &mut key) == ERROR_SUCCESS;
        if ok {
            let _ = RegCloseKey(key);
        }
        ok
    }
}

pub fn value_exists(sub: PCWSTR, name: PCWSTR) -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, sub, name, RRF_RT_REG_SZ | RRF_RT_REG_DWORD, None, None, None) == ERROR_SUCCESS }
}

pub fn delete_value(sub: PCWSTR, name: PCWSTR) {
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, sub, name);
    }
}

pub fn autostart_enabled() -> bool {
    value_exists(RUN_KEY, RUN_NAME)
}

/// Adds or removes the Run entry that starts this exe at sign-in.
pub fn set_autostart(on: bool) {
    if !on {
        delete_value(RUN_KEY, RUN_NAME);
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let cmd = wide(&format!("\"{}\"", exe.display()));
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_NAME,
            REG_SZ.0,
            Some(cmd.as_ptr() as *const c_void),
            (cmd.len() * 2) as u32,
        );
    }
}
