# Architecture

Claude Monitor is a single Win32 process written in Rust, using the [`windows`](https://crates.io/crates/windows)
crate directly (no UI framework). It has two threads:

- **UI thread** — owns every window and all drawing. Its state is the `App` struct (`src/app.rs`), kept in a
  thread-local and reached through `with_app`.
- **Poll thread** — reads the usage over HTTPS and hands it over through `Arc<Mutex<Shared>>`, then posts
  `WM_APP_DATA` so the UI redraws.

```
               ┌──────────── poll thread ────────────┐          ┌─────────────── UI thread ───────────────┐
credentials ──►│ api::fetch_usage ─► history::append │─Shared──►│ App::tick ─► strip / tray icon / card   │
               └─────────────────────────────────────┘  +post   │ wndproc ─► menu::show ─► change_settings │
                                                                 └──────────────────────────────────────────┘
```

## Modules

| Module | Responsibility |
| --- | --- |
| `main` | Command line, starts `app::run`. |
| `app` | Window classes, message loop, `App` state, the strip inside the taskbar (create, render, place). |
| `card` | The hover card: layout, charts, hover read-out, placement next to the strip or icon. |
| `chart` | Pure chart logic: plotting, forecast, reading the value under the cursor. |
| `tray` | Icon mode: one notification-area icon with both numbers, its events and Windows 11 promotion. |
| `notify` | Warning and reset notifications (balloon toasts). |
| `menu` | The context menu and every command. |
| `login` | Finding Claude Code and offering to install / sign in. |
| `api` | Credentials, OAuth token refresh, the usage request and its parsing. |
| `http` | A small WinHTTP client. |
| `poll` | The background loop and the data shared with the UI. |
| `history` | The CSV history behind the charts. |
| `settings` | Settings model, defaults and registry persistence. |
| `registry` | Registry helpers and the autostart entry. |
| `i18n` | Every user-visible string, per language. |
| `gfx` | Software drawing: GDI text masks, DIB surfaces, blending, icons, fonts, theme. |
| `time` | Time conversions and display formats. |
| `legacy` | One-time migration from the app's first name (ClaudeUsage). |
| `demo` | Made-up data for `--demo`. |
| `util` | UTF-16 helpers, paths, the log file. |

## The strip

The strip is a `WS_EX_LAYERED` child window of `Shell_TrayWnd`. Every frame is drawn into a 32-bit DIB and
pushed with `UpdateLayeredWindow`, so it has real per-pixel alpha over the (possibly translucent) taskbar.
Text is rendered by GDI with ClearType into a mask and tinted, which matches the taskbar's own text.

Each tick (1 s) the strip is re-placed just left of `TrayNotifyWnd`, skipping any other program's child window
in the taskbar. If Explorer restarts, the strip is recreated on the new taskbar, and the `TaskbarCreated`
message re-adds the notification-area icon.

## Data

- Credentials: `.credentials.json` in the Claude Code folder, written by Claude Code. The folder is the one
  chosen in the menu, else `$CLAUDE_CONFIG_DIR`, else `%USERPROFILE%\.claude`.
  The app only rewrites it after refreshing an expired token, atomically (temporary file + rename).
- Usage: `GET https://api.anthropic.com/api/oauth/usage` with the OAuth bearer token.
- History: `%LOCALAPPDATA%\ClaudeMonitor\history-<account>.csv`, one file per Claude account (the
  `oauthAccount.accountUuid` in Claude Code's `.claude.json`), one `unix,session%,week%` line per read,
  8 days kept.
- Log: `%LOCALAPPDATA%\ClaudeMonitor\claude-monitor.log`.
- Settings: `HKCU\Software\ClaudeMonitor` (DWORD values, plus the `ConfigDir` string); autostart: `HKCU\...\Run\ClaudeMonitor`.

## Single instance

A named mutex (`Local\ClaudeMonitor`) detects a running instance; the new one asks it to quit
(`WM_APP_QUIT`) and takes over, so running a newer `.exe` is enough to update.
