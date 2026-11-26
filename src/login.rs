//! Signing in: offers to install Claude Code and/or run its sign-in when the usage cannot be read.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONINFORMATION, MB_YESNO, MessageBoxW};
use windows::core::{PCWSTR, w};

use crate::i18n::tr;
use crate::util::{home, log, wide};

const CREATE_NEW_CONSOLE: u32 = 0x10;

/// The Claude Code CLI, wherever its installers put it.
fn find_claude_cli() -> Option<PathBuf> {
    let env = |k: &str| PathBuf::from(std::env::var_os(k).unwrap_or_default());
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    dirs.push(home().join(".local").join("bin"));
    dirs.push(env("LOCALAPPDATA").join("Microsoft").join("WinGet").join("Links"));
    dirs.push(env("APPDATA").join("npm"));
    dirs.into_iter().flat_map(|d| ["claude.exe", "claude.cmd"].map(|f| d.join(f))).find(|p| p.is_file())
}

/// Asks, and if the user agrees opens PowerShell with the official Claude Code installer (when it is missing)
/// and its sign-in. The poll thread notices the new credentials file by itself.
pub fn offer(owner: HWND) {
    let (question, script) = match find_claude_cli() {
        None => (
            tr().login_install,
            "irm https://claude.ai/install.ps1 | iex; & \"$env:USERPROFILE\\.local\\bin\\claude.exe\" auth login".to_owned(),
        ),
        Some(path) => (tr().login_sign_in, format!("& '{}' auth login", path.display())),
    };
    let text = wide(question);
    let answer = unsafe { MessageBoxW(Some(owner), PCWSTR(text.as_ptr()), w!("Claude Monitor"), MB_YESNO | MB_ICONINFORMATION) };
    if answer != IDYES {
        return;
    }
    let mut command = std::process::Command::new("powershell");
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-NoExit", "-Command", &script]).creation_flags(CREATE_NEW_CONSOLE);
    if let Some(dir) = crate::api::custom_config_dir() {
        command.env("CLAUDE_CONFIG_DIR", dir);
    }
    let spawned = command.spawn();
    if let Err(e) = spawned {
        log(&format!("opening the sign-in: {e}"));
    }
}
