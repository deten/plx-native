//! **The kept event log** — a debug-flavour install's copy of the event log that outlives a launch.
//!
//! `src/main.c` truncates `plxnative-events.log` every time the app starts, so the log of a fault
//! the viewer only noticed afterwards (an episode that ended early, then a relaunch to look at
//! Settings) is gone before anyone fetches it. A test build is installed precisely to catch those,
//! so on the `com.beb.plxnative.debug` install every line [`super::log`] writes is ALSO appended
//! here, after the same [`super::scrub::scrub_local`] pass, with a wall-clock stamp, and nothing
//! truncates it at launch.
//!
//! **Bounded.** The file is capped at [`CAP_BYTES`]; the write that would cross the cap first
//! renames it to `plxnative-history.log.1` (replacing the previous one), so the two together never
//! exceed twice the cap. Both live in the install's runtime directory
//! (`/tmp/com.beb.plxnative.debug/` on the television), so a TV restart clears them like every
//! other runtime log, and Delete all local data removes them (`paths::runtime_file::LOGS`).
//!
//! **Debug flavour only.** The stable app and Nightly keep exactly the logs they always had; the
//! switch is the install's app id ([`crate::paths::flavour`]), which a RELEASE=1 test package keeps.
//!
//! Two event-log line shapes are never kept: the raw `text` and `edit` dumps of the input arm,
//! which carry typed characters as hex that the scrubber cannot recognise. Key presses are kept.

use std::io;
use std::path::Path;
use std::sync::{Mutex, Once, OnceLock};

/// The kept log's size cap. The rotated copy holds the previous [`CAP_BYTES`].
pub const CAP_BYTES: u64 = 1024 * 1024;

/// Whether this install keeps the history: the debug flavour, and nothing else.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| crate::paths::flavour() == Some("debug"))
}

/// Append one already-scrubbed event-log line, when this install keeps the history.
pub(super) fn keep(line: &str) {
    if !enabled() || !kept(line) {
        return;
    }
    // One writer at a time, so two threads cannot both rotate on the same crossing.
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = crate::paths::in_runtime_dir(crate::paths::runtime_file::HISTORY);
    let old = crate::paths::in_runtime_dir(crate::paths::runtime_file::HISTORY_OLD);
    static HEADER: Once = Once::new();
    HEADER.call_once(|| {
        let _ = append_capped(&path, &old, &launch_header(now_epoch_ms()), CAP_BYTES);
    });
    let _ = append_capped(&path, &old, &stamped(now_epoch_ms(), line), CAP_BYTES);
}

/// The input arm's raw text and pre-edit dumps carry typed characters as hex: never kept.
fn kept(line: &str) -> bool {
    !(line.contains("] text type=") || line.contains("] edit type="))
}

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `HH:MM:SS.mmm` (UTC) in front of the line.
fn stamped(epoch_ms: u64, line: &str) -> String {
    let s = epoch_ms / 1000;
    format!(
        "{:02}:{:02}:{:02}.{:03} {line}",
        (s / 3600) % 24,
        (s / 60) % 60,
        s % 60,
        epoch_ms % 1000
    )
}

/// The first kept line of every launch: the date the following `HH:MM:SS` stamps belong to.
fn launch_header(epoch_ms: u64) -> String {
    let (y, m, d) = civil_from_days((epoch_ms / 86_400_000) as i64);
    stamped(epoch_ms, &format!("history: launch {y:04}-{m:02}-{d:02} (UTC) pid={}", std::process::id()))
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Append `line` to `path` through the event log's own open discipline
/// ([`super::open_log_append`]), first moving `path` to `old` when the line would take it past
/// `cap`. If the rename is refused, the file is started over instead, so the cap holds either way.
pub fn append_capped(path: &Path, old: &Path, line: &str, cap: u64) -> io::Result<()> {
    let mut file = super::open_log_append(path)?;
    let len = file.metadata()?.len();
    if len > 0 && len + line.len() as u64 + 1 > cap {
        drop(file);
        file = if std::fs::rename(path, old).is_ok() {
            super::open_log_append(path)?
        } else {
            let file = super::open_log_append(path)?;
            file.set_len(0)?;
            file
        };
    }
    super::write_log_line(&mut file, line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("plx-history-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The cap holds: the write that would cross it moves the file aside, and the two files
    /// together never exceed twice the cap, however many lines are written.
    #[test]
    fn the_history_rotates_at_the_cap_and_keeps_one_previous_file() {
        let dir = scratch("rotate");
        let (path, old) = (dir.join("h.log"), dir.join("h.log.1"));
        let cap = 1000;
        for i in 0..200 {
            append_capped(&path, &old, &format!("endreason: eos rk={i} pos=1 dur=2"), cap).unwrap();
        }
        let now = std::fs::metadata(&path).unwrap().len();
        let prev = std::fs::metadata(&old).unwrap().len();
        assert!(now <= cap && prev <= cap, "now={now} prev={prev}");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("rk=199 pos=1 dur=2\n"), "the newest line is in the live file");
        assert!(text.lines().all(|l| l.starts_with("endreason: ")), "whole lines only");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The kept file is opened like the event log: a symlink at the name is refused, untouched.
    #[test]
    fn a_symlinked_history_is_refused() {
        let dir = scratch("symlink");
        let victim = dir.join("victim");
        std::fs::write(&victim, b"unchanged").unwrap();
        let path = dir.join("h.log");
        std::os::unix::fs::symlink(&victim, &path).unwrap();
        assert!(append_capped(&path, &dir.join("h.log.1"), "x", 1000).is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"unchanged");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn typed_text_dumps_are_not_kept_and_keys_are() {
        assert!(!kept("[123] text type=0x303 raw=00ff"));
        assert!(!kept("[123] edit type=0x302 raw=00ff"));
        assert!(kept("[123] key type=0x300 raw=00ff"));
        assert!(kept("endreason: eos rk=1"));
    }

    #[test]
    fn the_stamp_and_the_launch_date_are_utc() {
        // 2026-10-14 13:05:09.042 UTC
        let ms = 1_791_983_109_042;
        assert_eq!(stamped(ms, "x"), "13:05:09.042 x");
        assert!(launch_header(ms).starts_with("13:05:09.042 history: launch 2026-10-14 (UTC) pid="));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    /// Off on a host build and on every non-debug install: no flavour, no history.
    #[test]
    fn the_history_is_off_without_the_debug_flavour() {
        assert_eq!(enabled(), crate::paths::flavour() == Some("debug"));
    }
}
