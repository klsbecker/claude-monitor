//! Made-up data for `--demo`, used for screenshots and for trying the app without signing in.

use crate::api::{Limit, SESSION, SESSION_SECS, WEEK, WEEK_SECS};
use crate::history::Sample;
use crate::time::{iso_unix, now_unix};

const SESSION_RESET_IN: i64 = 133 * 60;
const WEEK_RESET_IN: i64 = 3 * 86400 + 5 * 3600;

pub fn limits() -> Vec<Limit> {
    let now = now_unix();
    vec![
        Limit { key: SESSION.into(), pct: 42.0, reset: Some(iso_unix(now + SESSION_RESET_IN)) },
        Limit { key: WEEK.into(), pct: 18.0, reset: Some(iso_unix(now + WEEK_RESET_IN)) },
    ]
}

pub fn history() -> Vec<Sample> {
    let now = now_unix();
    let session_start = now + SESSION_RESET_IN - SESSION_SECS;
    let week_start = now + WEEK_RESET_IN - WEEK_SECS;
    (0..=(now - week_start) / 1800)
        .map(|i| {
            let t = week_start + i * 1800;
            let f = (t - week_start) as f64 / (now - week_start) as f64;
            let week = 18.0 * (f + 0.08 * (f * 25.0).sin()).clamp(0.0, 1.0);
            let session = (t >= session_start).then(|| {
                let g = (t - session_start) as f64 / (now - session_start) as f64;
                42.0 * g.powf(1.3)
            });
            (t, session, Some(week))
        })
        .collect()
}
