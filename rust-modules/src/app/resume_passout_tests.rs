//! Rewind on resume through the production launch policy (`start_playback_with`,
//! `play_item_now_with`), with only resource acceptance substituted, and Passout protection's pure
//! decision (`passout_trips`, `Advance::automatic`).

use super::super::bridge::{AppHost, Bridge};
use super::{passout_trips, play_item_now_with, start_playback_with, Advance, Origin, PlaybackResources};
use plx_media::route::{PassoutProtection, ResumeRewind};
use plx_ui::dispatch::Dispatcher;

const S: i64 = 1_000_000_000;

/// Records the position each start was prepared at; accepts every request and refuses the start
/// itself, so no player page is pushed and nothing but the launch policy runs.
#[derive(Default)]
struct Starts(Vec<i64>);

impl PlaybackResources for Starts {
    fn request_movie(&mut self, _: &mut plx_media::route::PlaybackSession,
        _: &mut plx_data::stores::metadata::MetadataStore, _: &plx_data::pms::PmsMovie) -> bool {
        true
    }
    fn request_episode(&mut self, _: &mut plx_media::route::PlaybackSession,
        _: &mut plx_data::stores::metadata::MetadataStore, _: &str) -> bool {
        true
    }
    fn describe_movie(&mut self, _: &mut plx_data::stores::metadata::MetadataStore,
        _: plx_plex::plex::ServerId, _: &str) {}
    fn prepare_start(&mut self, _: &mut plx_media::route::PlaybackSession,
        _: &mut plx_media::player::adapter::PlayerAdapter, resume_ns: i64) -> bool {
        self.0.push(resume_ns);
        false
    }
}

/// Puts the Rewind on resume choice back whatever the test did.
struct RewindGuard(ResumeRewind);
impl RewindGuard {
    fn set(rewind: ResumeRewind) -> Self {
        let prior = RewindGuard(plx_media::route::resume_rewind());
        plx_media::route::restore_resume_rewind(rewind);
        prior
    }
}
impl Drop for RewindGuard {
    fn drop(&mut self) {
        plx_media::route::restore_resume_rewind(self.0);
    }
}

/// The resume position `start_playback_with` hands the engine for one launch.
fn started_at(resume_ns: i64, from: Origin) -> i64 {
    let mut ps = plx_media::route::PlaybackSession::default();
    let mut pa = plx_media::player::adapter::PlayerAdapter::new(unsafe { plx_base::task::MainThread::assume() });
    let mut pages = Dispatcher::<AppHost>::new();
    let mut bridge = Bridge::for_test(|| 0);
    let mut starts = Starts::default();
    start_playback_with(&mut ps, &mut pa, resume_ns, from, 0, None, &mut pages, &mut bridge, &mut starts);
    assert_eq!(starts.0.len(), 1, "one launch prepares one start");
    starts.0[0]
}

/// **A resume the viewer chose starts the chosen length earlier**, clamped at the start; none
/// leaves it exact.
#[test]
fn a_viewers_resume_is_rewound_by_the_chosen_length() {
    let _serial = plx_base::testlock::serial();
    for (rewind, want) in [
        (ResumeRewind::None, 600 * S),
        (ResumeRewind::Seconds5, 595 * S),
        (ResumeRewind::Seconds10, 590 * S),
        (ResumeRewind::Seconds15, 585 * S),
        (ResumeRewind::Seconds30, 570 * S),
    ] {
        let _rewind = RewindGuard::set(rewind);
        assert_eq!(started_at(600 * S, Origin::Here), want, "{rewind:?}");
    }
    let _rewind = RewindGuard::set(ResumeRewind::Seconds30);
    assert_eq!(started_at(20 * S, Origin::Here), 0, "a rewind past the start starts at the start");
}

/// **What is not a viewer's resume is never rewound**: a start from the beginning stays at 0, and
/// an advance the player made itself (`Origin::Unchanged`, Up Next and the episode chain via
/// `play_up_next`) starts exactly where its successor's own resume point says.
#[test]
fn a_start_from_the_beginning_and_an_up_next_advance_are_not_rewound() {
    let _serial = plx_base::testlock::serial();
    let _rewind = RewindGuard::set(ResumeRewind::Seconds30);
    assert_eq!(started_at(0, Origin::Here), 0, "from the beginning");
    assert_eq!(started_at(600 * S, Origin::Unchanged), 600 * S, "an Up Next advance keeps its position");
    assert_eq!(started_at(0, Origin::Unchanged), 0);
}

/// **A Continue Watching card resumes rewound, and its menu's Play from Start does not**: the
/// captured-row launch (`play_item_now_with`) computes Plex's resume point and hands it to the same
/// policy.
#[test]
fn a_continue_watching_card_resumes_rewound_and_play_from_start_starts_at_zero() {
    let _serial = plx_base::testlock::serial();
    let _session = plx_plex::plex::session::TempSession::new("resume-rewind-card");
    let _rewind = RewindGuard::set(ResumeRewind::Seconds10);
    let row = plx_data::pms::PmsMovie {
        rk: "card-7".into(),
        resume_ms: 600_000,
        dur_ns: 3_600 * S,
        ..Default::default()
    };
    for (from_start, want) in [(false, 590 * S), (true, 0)] {
        let mut ps = plx_media::route::PlaybackSession::default();
        let mut pa = plx_media::player::adapter::PlayerAdapter::new(unsafe { plx_base::task::MainThread::assume() });
        let mut pages = Dispatcher::<AppHost>::new();
        let mut bridge = Bridge::for_test(|| 0);
        let mut starts = Starts::default();
        play_item_now_with(&mut ps, &mut pa, &row, from_start, Origin::Here, 0, None, &mut pages, &mut bridge, &mut starts);
        assert_eq!(starts.0, [want], "from_start={from_start}");
    }
}

/// **Passout protection's decision.** Off never stops an advance; a limit stops an automatic one
/// to an existing successor once reached; a press is never stopped; with nothing queued there is
/// no advance to stop.
#[test]
fn passout_stops_only_an_automatic_advance_past_the_limit() {
    const H: u32 = 3_600_000;
    let auto = |idle_ms| Advance::Automatic { idle_ms };
    assert!(!passout_trips(auto(10 * H), PassoutProtection::Off, true), "Off never stops one");
    assert!(!passout_trips(auto(H - 1), PassoutProtection::Hours1, true), "inside the limit");
    assert!(passout_trips(auto(H), PassoutProtection::Hours1, true), "at the limit");
    assert!(!passout_trips(auto(2 * H), PassoutProtection::Hours3, true), "a longer limit still waits");
    assert!(passout_trips(auto(3 * H), PassoutProtection::Hours3, true));
    assert!(!passout_trips(Advance::Pressed, PassoutProtection::Hours1, true), "a press always advances");
    assert!(!passout_trips(auto(5 * H), PassoutProtection::Hours1, false), "nothing queued, nothing to stop");
}

/// The idle time is measured from the last input on the wrapping millisecond clock, and a stamp
/// ahead of the clock reads as no idle time rather than ~49 days of it.
#[test]
fn the_idle_time_is_measured_on_the_wrapping_clock() {
    assert_eq!(Advance::automatic(5_000, 1_000), Advance::Automatic { idle_ms: 4_000 });
    assert_eq!(Advance::automatic(500, u32::MAX - 499), Advance::Automatic { idle_ms: 1_000 }, "across the wrap");
    assert_eq!(Advance::automatic(1_000, 5_000), Advance::Automatic { idle_ms: 0 }, "a stamp ahead of now");
}
