//! One-time migration from the app's first name, "ClaudeUsage": settings, autostart entry and history.

use windows::core::w;

use crate::app::close_instance;
use crate::registry::{RUN_KEY, SETTINGS_KEY, delete_value, key_exists, set_autostart, value_exists};
use crate::settings::Settings;
use crate::util::{data_dir, home};

pub fn migrate() {
    // Two widgets would draw on the taskbar at once.
    close_instance(w!("ClaudeUsage.Taskbar.Main"));

    let old_key = w!("Software\\ClaudeUsageTaskbar");
    if !key_exists(SETTINGS_KEY) && key_exists(old_key) {
        Settings::load_from(old_key).save();
    }

    let old_run = w!("ClaudeUsageTaskbar");
    if value_exists(RUN_KEY, old_run) {
        delete_value(RUN_KEY, old_run);
        set_autostart(true);
    }

    // Older histories were kept per folder, the newest as `history.csv` for Claude Code's own folder; they
    // now belong to the account signed in there.
    let new_history = crate::history::path();
    for old_history in [home().join(".claude").join("claude-usage-history.csv"), data_dir().join("history.csv")] {
        if old_history != new_history
            && old_history.is_file()
            && !new_history.exists()
            && std::fs::copy(&old_history, &new_history).is_ok()
        {
            let _ = std::fs::remove_file(&old_history);
        }
    }
}
