//! Small helpers shared by the whole app: UTF-16 strings, paths and the log file.

use std::io::Write;
use std::path::PathBuf;

use windows::Win32::System::SystemInformation::GetLocalTime;

/// `s` as a NUL-terminated UTF-16 string.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Copies `s` into a fixed-size, NUL-terminated UTF-16 field, cut to fit.
pub fn fill_wide(dst: &mut [u16], s: &str) {
    let n = dst.len().saturating_sub(1);
    let src: Vec<u16> = s.encode_utf16().take(n).collect();
    dst[..src.len()].copy_from_slice(&src);
    dst[src.len()] = 0;
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default())
}

/// Where the app keeps its own files: `%LOCALAPPDATA%\ClaudeMonitor`.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map_or_else(|| home().join("AppData").join("Local"), PathBuf::from);
    let dir = base.join("ClaudeMonitor");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Appends a timestamped line to `claude-monitor.log` in the data directory.
pub fn log(msg: &str) {
    let t = unsafe { GetLocalTime() };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(data_dir().join("claude-monitor.log")) {
        let _ =
            writeln!(f, "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {msg}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond);
    }
}
