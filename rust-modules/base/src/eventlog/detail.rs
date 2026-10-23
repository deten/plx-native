//! **The detailed-logging vocabulary** — the pure halves of the lines Settings > Detailed logging
//! adds to the kept history ([`super::history::detail`]).
//!
//! Each line starts with a greppable prefix and is built here, from plain values, so its shape is
//! graded by the host suite rather than read off a television:
//!
//! * `key:` — every remote key the loop receives, one line per press and per release. A held
//!   key's auto-repeats are coalesced ([`KeyTrail`]): the first repeat is a line, the rest are a
//!   count on the release (`key: RIGHT up held 2.4s repeats=46`), so a stuck or phantom key is
//!   visible without a line every 50 ms.
//! * `sub: cue` — each subtitle cue the renderer shows, ONCE when it first appears ([`CueOnce`]),
//!   with its text exactly as the demuxer or the sidecar parser handed it over, before any line
//!   wrapping ([`escape_text`] makes every space and control character visible).
//! * `player: pace` — the playhead against the wall clock over each 10 s of uninterrupted playback
//!   ([`Pace`]), which is what a "playing fast" catch-up looks like.
//! * `nav:` — each change of page, surface or layout ([`NavTrail`]).
//!
//! The `scrub:`, `seek:` and other `sub:` lines are single facts their call sites format inline.
//! Media content (titles, subtitle text) is allowed in these lines; credentials, addresses and
//! account names are not, and [`super::history::detail`] runs every line through
//! [`super::scrub::scrub_local`] as the backstop.

use std::sync::Mutex;

/// `12.345s`, from nanoseconds — the unit every position in these lines is printed in.
pub fn secs(ns: i64) -> String {
    format!("{:.3}s", ns as f64 / 1e9)
}

/// A key event's edge as the loop classified it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEdge {
    Down,
    Repeat,
    Up,
}

/// One key event, as the loop saw it.
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent<'a> {
    /// The key's name (`RIGHT`, `OK`, `RIGHT/FF` for the alternate transport spelling).
    pub name: &'a str,
    pub sym: u32,
    pub wcode: u32,
    pub edge: KeyEdge,
    /// A press the driver stamped as a REPEAT although no key was down: the system keyboard ate
    /// the previous key-up (`App::down_sym`'s doc), so the loop treats it as a fresh press.
    pub phantom: bool,
    /// The route word and its overlay, as the heartbeat prints them.
    pub route: &'a str,
    /// The focused element (`entry/elem`), or `-`.
    pub focus: &'a str,
    pub now_ms: u32,
}

/// The key that is down, for coalescing its repeats.
#[derive(Clone, Debug)]
struct Held {
    sym: u32,
    name: String,
    since_ms: u32,
    repeats: u32,
}

/// **A held key's repeats, coalesced to a count per hold.**
#[derive(Debug, Default)]
pub struct KeyTrail {
    held: Option<Held>,
}

impl KeyTrail {
    /// The `key:` lines one event writes: none for a coalesced repeat, two when a press arrives
    /// while another key was still held (that hold's release never came).
    pub fn note(&mut self, ev: &KeyEvent<'_>) -> Vec<String> {
        let at = format!("route={} focus={}", ev.route, ev.focus);
        let mut out = Vec::new();
        match ev.edge {
            KeyEdge::Down => {
                if let Some(h) = self.held.take() {
                    out.push(format!("key: {} lost-up held {} repeats={} {at}", h.name, held_for(h.since_ms, ev.now_ms), h.repeats));
                }
                out.push(format!(
                    "key: {} down sym={:#x} wcode={:#x}{} {at}",
                    ev.name,
                    ev.sym,
                    ev.wcode,
                    if ev.phantom { " phantom=1" } else { "" }
                ));
                self.held = Some(Held { sym: ev.sym, name: ev.name.to_string(), since_ms: ev.now_ms, repeats: 0 });
            }
            KeyEdge::Repeat => match self.held.as_mut() {
                Some(h) if h.sym == ev.sym => {
                    h.repeats += 1;
                    if h.repeats == 1 {
                        out.push(format!("key: {} repeat {at}", ev.name));
                    }
                }
                _ => out.push(format!("key: {} repeat not-held {at}", ev.name)),
            },
            KeyEdge::Up => match self.held.take() {
                Some(h) if h.sym == ev.sym => {
                    out.push(format!("key: {} up held {} repeats={} {at}", h.name, held_for(h.since_ms, ev.now_ms), h.repeats));
                }
                other => {
                    self.held = other;
                    out.push(format!("key: {} up not-held {at}", ev.name));
                }
            },
        }
        out
    }
}

fn held_for(since_ms: u32, now_ms: u32) -> String {
    format!("{:.1}s", now_ms.wrapping_sub(since_ms) as f64 / 1000.0)
}

/// The longest stretch of cue text a line carries, in characters.
pub const CUE_TEXT_CAP: usize = 200;

/// **A cue's text with nothing hidden**: a line break is `\n` (and `\r`, `\t`), a backslash is
/// doubled so those stay unambiguous, and any other control character or space that is not a
/// plain ASCII space — a no-break space, a zero-width space, a byte-order mark — is `\u{…}`. So a
/// stray gap in "TH E FACE" reads back as exactly the bytes the source carried. At most `cap`
/// characters of the source are shown, then `…`.
pub fn escape_text(text: &str, cap: usize) -> String {
    let mut out = String::with_capacity(text.len().min(cap * 2) + 8);
    for (i, c) in text.chars().enumerate() {
        if i == cap {
            out.push('…');
            break;
        }
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            ' ' => out.push(' '),
            c if c.is_control() || c.is_whitespace() || matches!(c, '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}') => {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// `sub: cue src=<source> track=<n> start=… end=… len=<chars> text="…"`.
pub fn cue_line(source: &str, track: i32, start_ns: i64, end_ns: i64, text: &str) -> String {
    format!(
        "sub: cue src={source} track={track} start={} end={} len={} text=\"{}\"",
        secs(start_ns),
        secs(end_ns),
        text.chars().count(),
        escape_text(text, CUE_TEXT_CAP)
    )
}

/// **Once per cue, not once per frame**: the identity of the last cue logged, so the renderer can
/// ask on every frame it draws. A cue shown again after another one (a seek back) is a new
/// appearance and logs again.
#[derive(Debug, Default)]
pub struct CueOnce {
    last: Option<(&'static str, i64, i64, u64)>,
}

impl CueOnce {
    /// Whether this is the cue's first frame.
    pub fn first(&mut self, source: &'static str, start_ns: i64, end_ns: i64, text: &str) -> bool {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        let key = (source, start_ns, end_ns, h.finish());
        if self.last == Some(key) {
            return false;
        }
        self.last = Some(key);
        true
    }
}

/// The renderer's call, on every frame a text cue is drawn: logs the cue the first time. Costs one
/// atomic read while detailed logging is off.
pub fn cue_drawn(source: &'static str, track: i32, start_ns: i64, end_ns: i64, text: &str) {
    if !super::history::detailed() {
        return;
    }
    static ONCE: Mutex<CueOnce> = Mutex::new(CueOnce { last: None });
    if ONCE.lock().unwrap_or_else(|e| e.into_inner()).first(source, start_ns, end_ns, text) {
        super::history::detail(&cue_line(source, track, start_ns, end_ns, text));
    }
}

/// How often [`Pace`] reports.
pub const PACE_EVERY_MS: u32 = 10_000;

/// **The playhead against the wall clock**, a window at a time. The window restarts whenever
/// playback is not plainly running (paused, buffering, seeking), so a report only ever covers
/// uninterrupted playback and a drift in it is the pipeline's own.
#[derive(Debug, Default)]
pub struct Pace {
    window: Option<(i64, u32)>,
}

impl Pace {
    /// One heartbeat's sample. A `player: pace` line once a window spans [`PACE_EVERY_MS`];
    /// `extra` is appended to it as it is.
    pub fn sample(&mut self, running: bool, pos_ns: i64, now_ms: u32, extra: &str) -> Option<String> {
        if !running {
            self.window = None;
            return None;
        }
        let Some((from_ns, from_ms)) = self.window else {
            self.window = Some((pos_ns, now_ms));
            return None;
        };
        let wall_ms = now_ms.wrapping_sub(from_ms);
        if wall_ms < PACE_EVERY_MS {
            return None;
        }
        self.window = Some((pos_ns, now_ms));
        let media_ms = (pos_ns - from_ns) / 1_000_000;
        Some(format!(
            "player: pace pos={} media={media_ms}ms wall={wall_ms}ms drift={:+}ms rate={:.3}{extra}",
            secs(pos_ns),
            media_ms - i64::from(wall_ms),
            media_ms as f64 / f64::from(wall_ms)
        ))
    }
}

/// Where the app is: the page on top, what is presented over it, how deep each stack is and which
/// layout it wears.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavSpot {
    pub route: &'static str,
    pub overlay: &'static str,
    pub pages: usize,
    pub surfaces: usize,
    pub layout: &'static str,
}

/// **Each change of page, surface or layout**, as one `nav:` line naming what moved.
#[derive(Debug, Default)]
pub struct NavTrail {
    last: Option<NavSpot>,
}

impl NavTrail {
    pub fn note(&mut self, spot: NavSpot) -> Option<String> {
        let verb = match &self.last {
            Some(last) if *last == spot => return None,
            None => "start",
            Some(last) if spot.pages > last.pages => "push",
            Some(last) if spot.pages < last.pages => "pop",
            Some(last) if spot.surfaces > last.surfaces => "present",
            Some(last) if spot.surfaces < last.surfaces => "dismiss",
            Some(last) if spot.layout != last.layout => "layout",
            Some(_) => "replace",
        };
        let line = format!(
            "nav: {verb} route={} overlay={} pages={} surfaces={} layout={}",
            if spot.route.is_empty() { "-" } else { spot.route },
            if spot.overlay.is_empty() { "-" } else { spot.overlay },
            spot.pages,
            spot.surfaces,
            spot.layout
        );
        self.last = Some(spot);
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(name: &'static str, sym: u32, edge: KeyEdge, now_ms: u32) -> KeyEvent<'static> {
        KeyEvent { name, sym, wcode: 0, edge, phantom: false, route: "player overlay=none", focus: "-", now_ms }
    }

    /// A 2.4 s hold of RIGHT: the press, the first repeat, then ONE release line carrying the
    /// repeat count — not 46 repeat lines.
    #[test]
    fn a_held_key_is_three_lines_whatever_its_repeats() {
        let mut t = KeyTrail::default();
        let mut lines = t.note(&ev("RIGHT", 0x4000_004f, KeyEdge::Down, 1000));
        for i in 0..46 {
            lines.extend(t.note(&ev("RIGHT", 0x4000_004f, KeyEdge::Repeat, 1500 + i * 40)));
        }
        lines.extend(t.note(&ev("RIGHT", 0x4000_004f, KeyEdge::Up, 3400)));
        assert_eq!(
            lines,
            [
                "key: RIGHT down sym=0x4000004f wcode=0x0 route=player overlay=none focus=-",
                "key: RIGHT repeat route=player overlay=none focus=-",
                "key: RIGHT up held 2.4s repeats=46 route=player overlay=none focus=-",
            ]
        );
    }

    /// A tap is two lines; the next hold counts its own repeats from zero.
    #[test]
    fn a_tap_has_no_repeat_and_each_hold_counts_its_own() {
        let mut t = KeyTrail::default();
        assert_eq!(t.note(&ev("OK", 0x0d, KeyEdge::Down, 0)).len(), 1);
        assert_eq!(t.note(&ev("OK", 0x0d, KeyEdge::Up, 120)), ["key: OK up held 0.1s repeats=0 route=player overlay=none focus=-"]);
        t.note(&ev("OK", 0x0d, KeyEdge::Down, 200));
        t.note(&ev("OK", 0x0d, KeyEdge::Repeat, 700));
        assert!(t.note(&ev("OK", 0x0d, KeyEdge::Repeat, 750)).is_empty(), "the second repeat is only counted");
        assert!(t.note(&ev("OK", 0x0d, KeyEdge::Up, 800))[0].contains("repeats=2"));
    }

    /// The two shapes a stuck or phantom remote key leaves: a press while another key is still
    /// held (its key-up never arrived), and a release or repeat for a key nobody saw go down.
    #[test]
    fn a_lost_release_and_an_unheld_key_are_named() {
        let mut t = KeyTrail::default();
        t.note(&ev("RIGHT", 1, KeyEdge::Down, 0));
        t.note(&ev("RIGHT", 1, KeyEdge::Repeat, 500));
        let lines = t.note(&ev("LEFT", 2, KeyEdge::Down, 5000));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "key: RIGHT lost-up held 5.0s repeats=1 route=player overlay=none focus=-");
        assert!(lines[1].starts_with("key: LEFT down"));
        assert_eq!(t.note(&ev("UP", 3, KeyEdge::Up, 5100)), ["key: UP up not-held route=player overlay=none focus=-"]);
        assert_eq!(t.note(&ev("UP", 3, KeyEdge::Repeat, 5100)), ["key: UP repeat not-held route=player overlay=none focus=-"]);
        assert!(t.note(&ev("LEFT", 2, KeyEdge::Up, 5200))[0].starts_with("key: LEFT up held 0.2s"), "LEFT was still held");
        let phantom = KeyEvent { phantom: true, focus: "3/7", route: "home", ..ev("OK", 0x0d, KeyEdge::Down, 0) };
        assert_eq!(t.note(&phantom), ["key: OK down sym=0xd wcode=0x0 phantom=1 route=home focus=3/7"]);
    }

    /// Every character that could hide a stray gap is spelled out; plain spaces and letters are
    /// left exactly as they came.
    #[test]
    fn cue_text_is_escaped_so_every_space_shows() {
        assert_eq!(escape_text("TH E FACE IS\nVAG U ELY", 200), "TH E FACE IS\\nVAG U ELY");
        assert_eq!(escape_text("a\r\nb\tc\\N\"q\"", 200), "a\\r\\nb\\tc\\\\N\\\"q\\\"");
        assert_eq!(escape_text("x\u{a0}y\u{200b}z\u{feff}", 200), "x\\u{a0}y\\u{200b}z\\u{feff}");
        assert_eq!(escape_text("  two  spaces ", 200), "  two  spaces ");
        assert_eq!(escape_text("¿Qué pasó? — sí", 200), "¿Qué pasó? — sí");
    }

    /// The cap counts characters, not bytes, so multi-byte text cannot be cut mid-character, and
    /// the line still says how long the cue really was.
    #[test]
    fn cue_text_is_capped_at_200_characters() {
        let long = "é".repeat(250);
        let out = escape_text(&long, CUE_TEXT_CAP);
        assert_eq!(out.chars().count(), 201);
        assert!(out.ends_with('…'));
        assert_eq!(escape_text(&"a".repeat(200), CUE_TEXT_CAP), "a".repeat(200), "exactly the cap is not cut");
        let line = cue_line("sidecar", -1, 61_250_000_000, 63_000_000_000, &long);
        assert!(line.starts_with("sub: cue src=sidecar track=-1 start=61.250s end=63.000s len=250 text=\"éé"), "{line}");
        assert!(line.ends_with("…\""));
    }

    /// One line per cue appearance: every frame of the same cue after the first is silent, the
    /// next cue logs, and the first one shown again after it logs again.
    #[test]
    fn a_cue_is_logged_once_when_it_first_appears() {
        let mut once = CueOnce::default();
        assert!(once.first("embedded", 1, 2, "a"));
        for _ in 0..100 {
            assert!(!once.first("embedded", 1, 2, "a"));
        }
        assert!(once.first("embedded", 3, 4, "b"));
        assert!(once.first("embedded", 1, 2, "a"), "shown again after another cue");
        assert!(once.first("sidecar", 1, 2, "a"), "another source is another cue");
        assert!(once.first("sidecar", 1, 2, "a changed"), "same times, different text");
    }

    /// A 10 s window of running playback reports media time against wall time; anything that is
    /// not plain playback restarts the window, so a seek or a stall never reads as drift.
    #[test]
    fn the_pace_reports_drift_over_uninterrupted_playback() {
        let mut p = Pace::default();
        assert_eq!(p.sample(true, 100_000_000_000, 0, ""), None, "the window opens");
        assert_eq!(p.sample(true, 105_000_000_000, 5_000, ""), None);
        let line = p.sample(true, 110_500_000_000, 10_000, " state=playing").unwrap();
        assert_eq!(line, "player: pace pos=110.500s media=10500ms wall=10000ms drift=+500ms rate=1.050 state=playing");
        assert_eq!(p.sample(false, 111_000_000_000, 11_000, ""), None, "paused: the window closes");
        assert_eq!(p.sample(true, 200_000_000_000, 12_000, ""), None, "and reopens, never across the gap");
        let slow = p.sample(true, 209_000_000_000, 22_000, "").unwrap();
        assert!(slow.contains("drift=-1000ms rate=0.900"), "{slow}");
    }

    #[test]
    fn nav_lines_name_what_moved() {
        let spot = |route, overlay, pages, surfaces, layout| NavSpot { route, overlay, pages, surfaces, layout };
        let mut t = NavTrail::default();
        assert_eq!(t.note(spot("home", "", 1, 0, "standard")).unwrap(), "nav: start route=home overlay=- pages=1 surfaces=0 layout=standard");
        assert_eq!(t.note(spot("home", "", 1, 0, "standard")), None, "nothing moved");
        assert!(t.note(spot("detail", "", 2, 0, "standard")).unwrap().starts_with("nav: push route=detail"));
        assert!(t.note(spot("player", "none", 3, 0, "standard")).unwrap().starts_with("nav: push route=player overlay=none"));
        assert!(t.note(spot("player", "tracks", 3, 1, "standard")).unwrap().starts_with("nav: present"));
        assert!(t.note(spot("player", "none", 3, 0, "standard")).unwrap().starts_with("nav: dismiss"));
        assert!(t.note(spot("detail", "", 2, 0, "standard")).unwrap().starts_with("nav: pop route=detail"));
        assert!(t.note(spot("detail", "", 2, 0, "shield")).unwrap().starts_with("nav: layout"));
        assert!(t.note(spot("search", "", 2, 0, "shield")).unwrap().starts_with("nav: replace route=search"));
    }

    #[test]
    fn positions_print_in_seconds() {
        assert_eq!(secs(0), "0.000s");
        assert_eq!(secs(61_250_000_000), "61.250s");
        assert_eq!(secs(-1_500_000_000), "-1.500s");
    }
}
