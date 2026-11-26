//! The usage history behind the charts: one CSV line per reading, `unix seconds,session %,week %`.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::time::now_unix;
use crate::util::data_dir;

/// One reading: unix seconds, session %, week %.
pub type Sample = (i64, Option<f64>, Option<f64>);

/// A little more than the week window, so the week chart is always complete.
pub const KEEP_SECS: i64 = 8 * 86400;

/// One file per Claude account, since the same folder can be signed in to another account and the same
/// account can be read from two folders; mixing accounts draws jumps in the charts. Without a known account,
/// one file per folder.
pub fn path() -> PathBuf {
    if let Some(id) = crate::api::account_id().filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
        return data_dir().join(format!("history-{id}.csv"));
    }
    match crate::api::custom_config_dir() {
        None => data_dir().join("history.csv"),
        Some(dir) => {
            // FNV-1a of the folder, so the name is stable and needs no escaping.
            let hash = dir
                .to_string_lossy()
                .to_lowercase()
                .bytes()
                .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
            data_dir().join(format!("history-{:08x}.csv", hash as u32))
        }
    }
}

/// The saved samples in `file` that are still recent; the file is rewritten without the older ones.
pub fn load(file: &Path) -> Vec<Sample> {
    let cutoff = now_unix() - KEEP_SECS;
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let history: Vec<Sample> = text.lines().filter_map(parse_line).filter(|s| s.0 >= cutoff).collect();
    let body: String = history.iter().map(format_line).collect();
    let _ = std::fs::write(file, body);
    history
}

pub fn append(file: &Path, s: &Sample) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
        let _ = f.write_all(format_line(s).as_bytes());
    }
}

pub fn clear() {
    let _ = std::fs::remove_file(path());
}

fn parse_line(line: &str) -> Option<Sample> {
    let mut f = line.split(',');
    let t: i64 = f.next()?.parse().ok()?;
    let session = f.next().and_then(|v| v.parse().ok());
    let week = f.next().and_then(|v| v.parse().ok());
    Some((t, session, week))
}

fn format_line(s: &Sample) -> String {
    let opt = |v: Option<f64>| v.map(|v| format!("{v}")).unwrap_or_default();
    format!("{},{},{}\n", s.0, opt(s.1), opt(s.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_round_trip() {
        for s in [(1_700_000_000, Some(42.5), Some(18.0)), (1_700_000_060, None, Some(3.0)), (1_700_000_120, None, None)] {
            assert_eq!(parse_line(format_line(&s).trim_end()), Some(s));
        }
        assert_eq!(parse_line("garbage"), None);
    }
}
