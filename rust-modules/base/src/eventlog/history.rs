//! **The kept event log** — a debug-flavour install's copy of the event log that outlives a launch.
//!
//! `src/main.c` truncates `plxnative-events.log` every time the app starts, so the log of a fault
//! the viewer only noticed afterwards (an episode that ended early, then a relaunch to look at
//! Settings) is gone before anyone fetches it. A test build is installed precisely to catch those,
//! so on the `com.beb.plxnative.debug` install every line [`super::log`] writes is ALSO appended
//! here, after the same [`super::scrub::scrub_local`] pass, with a wall-clock stamp, and nothing
//! truncates it at launch. The once-a-second `loop=` heartbeat is the one event-log line it does
//! not keep ([`kept`]): it is the event log's perf surface, and here it only buried the lines
//! around a fault under a flood of identical ones.
//!
//! **Bounded.** The file is capped at [`CAP_BYTES`]; the write that would cross the cap first
//! renames it to `plxnative-history.log.1` (replacing the previous one), so the two together never
//! exceed twice the cap. Both live in the install's runtime directory
//! (`/tmp/com.beb.plxnative.debug/` on the television), so a TV restart clears them like every
//! other runtime log, and Delete all local data removes them (`paths::runtime_file::LOGS`).
//!
//! **Detailed logging** (Settings > Detailed logging, or the [`MARKER`] file) moves the pair to a
//! directory that survives a TV restart ([`crate::paths::persistent_log_dir_candidates`]), raises
//! the cap to [`DETAILED_CAP_BYTES`], and admits the [`detail`] lines — key presses, scrub and seek
//! decisions, subtitle cues and their text, the playback pace and page changes — which are written
//! here only, never to the event log. Off, the history is exactly what it was. The file a line
//! lands in is decided per line, so the choice applies the moment it is published; each file a
//! process writes to starts with its own `history: launch` header, so every `HH:MM:SS` stamp has
//! a date above it.
//!
//! **Debug flavour only.** The stable app and Nightly keep exactly the logs they always had; the
//! switch is the install's app id ([`crate::paths::flavour`]), which a RELEASE=1 test package keeps.
//!
//! Two event-log line shapes are never kept: the raw `text` and `edit` dumps of the input arm,
//! which carry typed characters as hex that the scrubber cannot recognise. Key presses are kept.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::paths::runtime_file::{HISTORY, HISTORY_OLD};

/// The kept log's size cap. The rotated copy holds the previous [`CAP_BYTES`].
pub const CAP_BYTES: u64 = 1024 * 1024;

/// The cap while detailed logging is on: the extra lines are many, and the point of keeping them
/// across a restart is a fault noticed hours later.
pub const DETAILED_CAP_BYTES: u64 = 4 * 1024 * 1024;

/// **The developer override**: a file of this name in the persistent log directory turns detailed
/// logging on for every launch, whatever the session says, so a test build can be switched over
/// without touching its Settings. Read once per launch, and only by the debug flavour.
pub const MARKER: &str = "plxnative-detailed-logging";

/// Whether this install keeps the history: the debug flavour, and nothing else.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| crate::paths::flavour() == Some("debug"))
}

/// The Detailed logging preference as last published (`Session::detailed_logging`).
static PREFERENCE: AtomicBool = AtomicBool::new(false);

/// Whether the [`detail`] lines are written and the history lives in the persistent directory:
/// the debug flavour, with the preference on or the [`MARKER`] present.
pub fn detailed() -> bool {
    enabled() && (PREFERENCE.load(Ordering::Relaxed) || marker_armed())
}

/// Restore the saved preference without saying anything: boot, before [`announce`].
pub fn restore_detailed(on: bool) {
    PREFERENCE.store(on, Ordering::Relaxed);
}

/// The preference as last published, whether or not this install keeps a history.
pub fn preference() -> bool {
    PREFERENCE.load(Ordering::Relaxed)
}

/// **Settings > Detailed logging lands here** (once the write is durable), and so does a sign-in
/// handoff's restore. A change in what is actually kept is said in the log, with the file it now
/// goes to.
pub fn publish_detailed(on: bool) {
    let was = detailed();
    PREFERENCE.store(on, Ordering::Relaxed);
    if detailed() != was {
        super::log(&format!("history: detailed logging {}, {}", if on { "on" } else { "off" }, describe(&sink_now())));
    }
}

/// **The startup line** — where this launch's history is going, written once by boot after the
/// preference is restored. Nothing at all while detailed logging is off, so an ordinary test
/// build's history is unchanged.
pub fn announce() {
    if !detailed() {
        return;
    }
    let why = if PREFERENCE.load(Ordering::Relaxed) { "preference" } else { "marker file" };
    let sink = sink_now();
    super::log(&format!("history: {} (detailed logging by {why}, {})", sink.path.display(), facts(&sink)));
}

/// One detailed-logging line (`key:`, `scrub:`, `seek:`, `sub:`, `player:`, `nav:`), through
/// [`super::scrub::scrub_local`] and into the history only. A no-op unless [`detailed`].
pub fn detail(m: &str) {
    if detailed() {
        append(&super::scrub::scrub_local(m));
    }
}

/// [`detail`] for a line that costs something to build: `f` runs only while detailed logging is on.
pub fn detail_with(f: impl FnOnce() -> String) {
    if detailed() {
        detail(&f());
    }
}

/// Append one already-scrubbed event-log line, when this install keeps the history.
pub(super) fn keep(line: &str) {
    if !enabled() || !kept(line) {
        return;
    }
    append(line);
}

/// Every file the persistent half may have written, for Delete all local data. The runtime half is
/// in `paths::runtime_file::LOGS`; the [`MARKER`] is the developer's own file and is left alone.
pub fn persistent_files() -> Vec<PathBuf> {
    crate::paths::persistent_log_dir_candidates()
        .into_iter()
        .flat_map(|dir| [dir.join(HISTORY), dir.join(HISTORY_OLD)])
        .collect()
}

/// The input arm's raw text and pre-edit dumps carry typed characters as hex: never kept. Nor is
/// the once-a-second `loop=` heartbeat, which stays in the event log for the harness.
fn kept(line: &str) -> bool {
    !(line.contains("] text type=") || line.contains("] edit type=") || line.starts_with("loop="))
}

/// Where a line goes, and how big that file may grow.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Sink {
    path: PathBuf,
    old: PathBuf,
    cap: u64,
    /// The file survives a TV restart.
    persistent: bool,
}

/// The pure half of the choice: detailed logging takes the persistent directory when one was found
/// (and the runtime directory, with the bigger cap, when none was), everything else the runtime
/// directory at [`CAP_BYTES`] — exactly where the history always was.
fn sink_for(detailed: bool, persistent: Option<&Path>, runtime: &Path) -> Sink {
    let (dir, cap) = match (detailed, persistent) {
        (true, Some(dir)) => (dir, DETAILED_CAP_BYTES),
        (true, None) => (runtime, DETAILED_CAP_BYTES),
        (false, _) => (runtime, CAP_BYTES),
    };
    Sink { path: dir.join(HISTORY), old: dir.join(HISTORY_OLD), cap, persistent: detailed && persistent.is_some() }
}

fn sink_now() -> Sink {
    let on = detailed();
    // The persistent directory is resolved (and created) only once detailed logging is on.
    sink_for(on, if on { persistent_dir() } else { None }, crate::paths::runtime_dir())
}

fn describe(sink: &Sink) -> String {
    format!("{} ({})", sink.path.display(), facts(sink))
}

fn facts(sink: &Sink) -> String {
    format!(
        "{} MB cap, {}",
        sink.cap >> 20,
        if sink.persistent { "kept across a TV restart" } else { "cleared by a TV restart" }
    )
}

fn marker_armed() -> bool {
    static ARMED: OnceLock<bool> = OnceLock::new();
    *ARMED.get_or_init(|| enabled() && marker_present(&crate::paths::persistent_log_dir_candidates()))
}

/// Whether any candidate directory holds the [`MARKER`] as a regular file (a symlink does not count).
fn marker_present(dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|dir| std::fs::symlink_metadata(dir.join(MARKER)).is_ok_and(|m| m.file_type().is_file()))
}

fn persistent_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| resolve_dir(&crate::paths::persistent_log_dir_candidates())).as_deref()
}

/// The first candidate that is, or can be made, a real directory the history file can be opened in
/// through the event log's own sink checks. Group-readable (0750) like the logs themselves, so a
/// Developer Mode user's Dev Manager can list it. Never logs: it runs under the first line's write.
fn resolve_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|dir| usable_dir(dir)).cloned()
}

fn usable_dir(dir: &Path) -> bool {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if std::fs::DirBuilder::new().mode(0o750).create(dir).is_ok() {
        // `mkdir` applies the umask, and the group bits are the point; only a directory this call
        // just made is widened, never one that was already standing at the name.
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o750));
    }
    let is_dir = std::fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_dir());
    is_dir && super::open_log_append(&dir.join(HISTORY)).is_ok()
}

/// The open file's identity, so each file a process writes to gets its own launch header.
struct Writer {
    headed: Option<PathBuf>,
}

impl Writer {
    fn write(&mut self, sink: &Sink, line: &str, epoch_ms: u64) {
        if self.headed.as_deref() != Some(sink.path.as_path()) {
            self.headed = Some(sink.path.clone());
            let _ = append_capped(&sink.path, &sink.old, &launch_header(epoch_ms), sink.cap);
        }
        let _ = append_capped(&sink.path, &sink.old, &stamped(epoch_ms, line), sink.cap);
    }
}

fn append(line: &str) {
    let sink = sink_now();
    // One writer at a time, so two threads cannot both rotate on the same crossing.
    static WRITER: Mutex<Writer> = Mutex::new(Writer { headed: None });
    WRITER.lock().unwrap_or_else(|e| e.into_inner()).write(&sink, line, now_epoch_ms());
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

    /// The once-a-second heartbeat stays out of the history in every mode; a line that merely
    /// mentions a loop is kept.
    #[test]
    fn the_heartbeat_is_not_kept() {
        assert!(!kept("loop=62 route=player overlay=none pos=41s play=1000pm fps=60 worstframe=3"));
        assert!(!kept("loop=0 route=home fps=0"));
        assert!(kept("playstate: playing -> buffering rk=1 loop=2"));
        assert!(kept("bootstrap: first-loop tree=00 now=1"));
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

    /// Off on a host build and on every non-debug install: no flavour, no history, no detail.
    #[test]
    fn the_history_is_off_without_the_debug_flavour() {
        assert_eq!(enabled(), crate::paths::flavour() == Some("debug"));
        if !enabled() {
            assert!(!detailed(), "detailed logging needs the debug flavour");
        }
    }

    /// Off is where the history always was: the runtime directory at 1 MB, whatever persistent
    /// directory exists. On takes the persistent directory at 4 MB, or the runtime one at 4 MB
    /// when there is none.
    #[test]
    fn each_mode_writes_where_and_as_much_as_it_says() {
        let runtime = Path::new("/tmp/com.beb.plxnative.debug");
        let kept = Path::new("/media/developer/com.beb.plxnative.debug-logs");
        let off = sink_for(false, Some(kept), runtime);
        assert_eq!(off.path, runtime.join("plxnative-history.log"));
        assert_eq!(off.old, runtime.join("plxnative-history.log.1"));
        assert_eq!((off.cap, off.persistent), (1024 * 1024, false));
        assert_eq!(sink_for(false, None, runtime), off);
        let on = sink_for(true, Some(kept), runtime);
        assert_eq!(on.path, kept.join("plxnative-history.log"));
        assert_eq!(on.old, kept.join("plxnative-history.log.1"));
        assert_eq!((on.cap, on.persistent), (4 * 1024 * 1024, true));
        let stranded = sink_for(true, None, runtime);
        assert_eq!(stranded.path, runtime.join("plxnative-history.log"));
        assert_eq!((stranded.cap, stranded.persistent), (4 * 1024 * 1024, false));
        assert!(describe(&on).contains("kept across a TV restart"), "{}", describe(&on));
        assert!(describe(&off).contains("1 MB cap"), "{}", describe(&off));
    }

    /// The persistent directory is the first candidate that can be made a directory and written:
    /// an unwritable or file-shaped candidate is passed over, a symlinked one is refused.
    #[test]
    fn the_persistent_directory_is_the_first_usable_candidate() {
        let dir = scratch("resolve");
        let blocked = dir.join("not-a-dir");
        std::fs::write(&blocked, b"file").unwrap();
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let linked = dir.join("linked");
        std::os::unix::fs::symlink(&elsewhere, &linked).unwrap();
        let fresh = dir.join("fresh-logs");
        let got = resolve_dir(&[blocked, linked, dir.join("no/such/parent"), fresh.clone()]);
        assert_eq!(got, Some(fresh.clone()));
        assert!(fresh.is_dir() && fresh.join(HISTORY).is_file(), "created and probed");
        assert!(!elsewhere.join(HISTORY).exists(), "nothing written through the symlink");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o7777, 0o750);
        assert_eq!(resolve_dir(&[dir.join("no/such/parent")]), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The marker is a regular file in any candidate directory; a symlink, a directory or another
    /// name does not arm it.
    #[test]
    fn the_marker_file_forces_detailed_logging() {
        let dir = scratch("marker");
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        assert!(!marker_present(&[a.clone(), b.clone()]));
        std::fs::create_dir(a.join(MARKER)).unwrap();
        std::fs::write(b.join("plxnative-detailed-logging.txt"), b"").unwrap();
        std::os::unix::fs::symlink(b.join("plxnative-detailed-logging.txt"), b.join(MARKER)).unwrap();
        assert!(!marker_present(&[a.clone(), b.clone()]), "a directory and a symlink do not count");
        std::fs::remove_file(b.join(MARKER)).unwrap();
        std::fs::write(b.join(MARKER), b"").unwrap();
        assert!(marker_present(&[a, b.clone()]));
        assert!(marker_present(&[dir.join("missing"), b]));
        assert_eq!(MARKER, "plxnative-detailed-logging");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Off is byte-identical**: one sink, one launch header, then each kept line stamped — what
    /// the history always wrote. A switch of file gives the new file its own header, so its stamps
    /// have a date too; the old file is left as it was.
    #[test]
    fn a_launch_header_opens_each_file_once() {
        let dir = scratch("writer");
        let (runtime, kept) = (dir.join("tmp"), dir.join("kept"));
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::create_dir_all(&kept).unwrap();
        let ms = 1_791_983_109_042;
        let mut w = Writer { headed: None };
        let off = sink_for(false, None, &runtime);
        w.write(&off, "endreason: eos rk=1", ms);
        w.write(&off, "advance: to=2", ms + 1);
        let header = launch_header(ms);
        assert_eq!(
            std::fs::read_to_string(&off.path).unwrap(),
            format!("{header}\n13:05:09.042 endreason: eos rk=1\n13:05:09.043 advance: to=2\n")
        );
        let on = sink_for(true, Some(&kept), &runtime);
        w.write(&on, "key: RIGHT down", ms + 2);
        assert_eq!(
            std::fs::read_to_string(&on.path).unwrap(),
            format!("{}\n13:05:09.044 key: RIGHT down\n", launch_header(ms + 2))
        );
        assert_eq!(std::fs::read_to_string(&off.path).unwrap().lines().count(), 3, "the runtime file is untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The startup line's path survives the local scrub whole: it names no host or address.
    #[test]
    fn the_announced_path_survives_the_scrub() {
        let sink = sink_for(true, Some(Path::new("/media/developer/com.beb.plxnative.debug-logs")), Path::new("/tmp"));
        let line = format!("history: {} (detailed logging by preference, {})", sink.path.display(), facts(&sink));
        assert_eq!(super::super::scrub::scrub_local_with(&line, &[]), line);
    }

    /// Delete all local data reaches both files in every candidate directory, never the marker.
    #[test]
    fn the_persistent_files_are_every_candidates_pair() {
        let files = persistent_files();
        let dirs = crate::paths::persistent_log_dir_candidates();
        assert_eq!(files.len(), dirs.len() * 2);
        for dir in dirs {
            assert!(files.contains(&dir.join("plxnative-history.log")));
            assert!(files.contains(&dir.join("plxnative-history.log.1")));
        }
        assert!(!files.iter().any(|f| f.ends_with(MARKER)));
    }
}
