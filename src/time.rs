//! Conversions between Windows time types, unix seconds and ISO 8601, and the formats shown to the user.

use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTimeAsFileTime};
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime};

use crate::i18n::tr;

const UNIX_EPOCH_FT: i64 = 116_444_736_000_000_000;
const TICKS_PER_SEC: i64 = 10_000_000;

fn ft_to_i64(ft: FILETIME) -> i64 {
    ((ft.dwHighDateTime as u64) << 32 | ft.dwLowDateTime as u64) as i64
}

fn unix_to_ft(t: i64) -> FILETIME {
    let ticks = t * TICKS_PER_SEC + UNIX_EPOCH_FT;
    FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: ((ticks as u64) >> 32) as u32 }
}

pub fn now_unix() -> i64 {
    (ft_to_i64(unsafe { GetSystemTimeAsFileTime() }) - UNIX_EPOCH_FT) / TICKS_PER_SEC
}

/// Unix seconds of an ISO 8601 timestamp such as `2026-11-25T18:00:00.123+00:00`.
pub fn parse_iso_unix(iso: &str) -> Option<i64> {
    let num = |at: usize, len: usize| -> Option<u16> { iso.get(at..at + len)?.parse().ok() };
    let utc = SYSTEMTIME {
        wYear: num(0, 4)?,
        wMonth: num(5, 2)?,
        wDay: num(8, 2)?,
        wHour: num(11, 2)?,
        wMinute: num(14, 2)?,
        wSecond: num(17, 2)?,
        ..Default::default()
    };
    let tail = iso.get(19..)?.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset_min = match tail.as_bytes().first() {
        Some(b'+') | Some(b'-') => {
            let h: i64 = tail.get(1..3)?.parse().ok()?;
            let m: i64 = tail.get(4..6).and_then(|m| m.parse().ok()).unwrap_or(0);
            let sign = if tail.starts_with('-') { -1 } else { 1 };
            sign * (h * 60 + m)
        }
        _ => 0,
    };
    let mut ft = FILETIME::default();
    unsafe { SystemTimeToFileTime(&utc, &mut ft).ok()? };
    Some((ft_to_i64(ft) - UNIX_EPOCH_FT) / TICKS_PER_SEC - offset_min * 60)
}

/// `t` (unix seconds) as a UTC ISO 8601 timestamp.
pub fn iso_unix(t: i64) -> String {
    let mut st = SYSTEMTIME::default();
    unsafe {
        let _ = FileTimeToSystemTime(&unix_to_ft(t), &mut st);
    }
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00", st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond)
}

pub fn unix_to_local(t: i64) -> SYSTEMTIME {
    let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        let _ = FileTimeToSystemTime(&unix_to_ft(t), &mut utc);
        let _ = SystemTimeToTzSpecificLocalTime(None, &utc, &mut local);
    }
    local
}

pub fn local_now() -> SYSTEMTIME {
    unsafe { GetLocalTime() }
}

/// "14:30".
pub fn hm(t: &SYSTEMTIME) -> String {
    format!("{:02}:{:02}", t.wHour, t.wMinute)
}

/// Short weekday name in the current language.
pub fn weekday(t: &SYSTEMTIME) -> &'static str {
    tr().weekdays[t.wDayOfWeek as usize % 7]
}

/// "25/11" or, in English, "11/25".
pub fn day_month(t: &SYSTEMTIME) -> String {
    if tr().month_first { format!("{:02}/{:02}", t.wMonth, t.wDay) } else { format!("{:02}/{:02}", t.wDay, t.wMonth) }
}

/// "14:30" today, "Fri 09:00" on another day.
pub fn when(at: i64) -> String {
    let st = unix_to_local(at);
    let today = local_now();
    if (st.wYear, st.wMonth, st.wDay) == (today.wYear, today.wMonth, today.wDay) {
        hm(&st)
    } else {
        format!("{} {}", weekday(&st), hm(&st))
    }
}
