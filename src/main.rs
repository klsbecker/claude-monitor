//! Claude Monitor: Claude plan usage (session and week) on the Windows taskbar.
//!
//! The usage comes from the Claude API with the OAuth sign-in that Claude Code saves. It is shown as a strip
//! inside the taskbar, next to the notification area, or as a notification-area icon; hovering shows a card
//! with charts, and clicking opens the menu.

#![windows_subsystem = "windows"]

mod api;
mod app;
mod card;
mod chart;
mod demo;
mod gfx;
mod history;
mod http;
mod i18n;
mod legacy;
mod login;
mod menu;
mod notify;
mod poll;
mod registry;
mod settings;
mod time;
mod tray;
mod util;

/// Command line: `--demo` shows made-up data; `--offset <px>` moves the strip further from the notification area.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let demo = args.iter().any(|a| a == "--demo");
    let offset =
        args.iter().position(|a| a == "--offset").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(0);
    app::run(demo, offset);
}
