//! The background thread that reads the usage and hands it to the UI thread.

use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, SYSTEMTIME, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::api::{self, Fail, Limit, SESSION, WEEK, credentials_mtime, find_limit};
use crate::app::{WM_APP_DATA, WM_APP_LOGIN};
use crate::demo;
use crate::history::{self, Sample};
use crate::i18n::tr;
use crate::settings::INTERVAL_MIN;
use crate::time::{local_now, now_unix};
use crate::util::log;

/// What the poll thread last read, shared with the UI thread.
#[derive(Default)]
pub struct Shared {
    pub items: Option<Vec<Limit>>,
    pub error: Option<String>,
    pub updated: Option<SYSTEMTIME>,
    pub history: Vec<Sample>,
    pub needs_login: bool,
}

/// Reads the usage every `INTERVAL_MIN` minutes, or right away when `wake` gets a message, and posts
/// `WM_APP_DATA` to `main` after each read. Returns when `wake` is dropped.
pub fn run(shared: Arc<Mutex<Shared>>, main: isize, demo: bool, wake: Receiver<()>) {
    let post = |msg: u32| unsafe {
        let _ = PostMessageW(Some(HWND(main as *mut c_void)), msg, WPARAM(0), LPARAM(0));
    };
    let mut file = history::path();
    shared.lock().unwrap().history = if demo { demo::history() } else { history::load(&file) };
    let mut last_error: Option<String> = None;
    let mut prompted = false;
    let mut dir_seen = api::config_dir();
    loop {
        // Another folder, or another account signed in to the same one.
        let file_now = history::path();
        if api::config_dir() != dir_seen || file_now != file {
            dir_seen = api::config_dir();
            file = file_now;
            prompted = false;
            let mut s = shared.lock().unwrap();
            s.history = if demo { demo::history() } else { history::load(&file) };
            s.items = None;
            s.error = None;
        }
        let creds_seen = credentials_mtime();
        let result = if demo { Ok(demo::limits()) } else { api::fetch_usage() };
        let needs_login = matches!(result, Err(Fail::Login(_)));
        shared.lock().unwrap().needs_login = needs_login;
        if needs_login && !prompted {
            prompted = true;
            post(WM_APP_LOGIN);
        }
        let result = result.map_err(Fail::text).and_then(|v| if v.is_empty() { Err(tr().no_limits.to_owned()) } else { Ok(v) });
        let failed = result.is_err();
        {
            let mut s = shared.lock().unwrap();
            match result {
                Err(e) => {
                    if last_error.as_ref() != Some(&e) {
                        log(&format!("error: {e}"));
                    }
                    last_error = Some(e.clone());
                    s.error = Some(e);
                }
                Ok(items) => {
                    if !demo {
                        let pct = |k: &str| find_limit(&items, k).map(|l| l.pct);
                        let sample = (now_unix(), pct(SESSION), pct(WEEK));
                        history::append(&file, &sample);
                        s.history.push(sample);
                        let cutoff = now_unix() - history::KEEP_SECS;
                        s.history.retain(|h| h.0 >= cutoff);
                    }
                    s.items = Some(items);
                    s.error = None;
                    last_error = None;
                }
            }
            s.updated = Some(local_now());
        }
        post(WM_APP_DATA);
        // After a failure, retry as soon as Claude Code rewrites the credentials (e.g. a refreshed token).
        let slice = if failed { Duration::from_secs(5) } else { Duration::from_secs(15) };
        let started = Instant::now();
        loop {
            match wake.recv_timeout(slice) {
                Ok(()) => break,
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
            // Read on every slice, so an interval changed in the menu applies without waiting out the old one.
            let interval = Duration::from_secs(INTERVAL_MIN.load(Ordering::Relaxed) as u64 * 60);
            if started.elapsed() >= interval || (failed && credentials_mtime() != creds_seen) {
                break;
            }
        }
    }
}
