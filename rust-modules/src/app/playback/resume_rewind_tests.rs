//! Rewind on resume through the production launch policy ([`start_playback_with`],
//! [`play_item_now_with`]), with only resource acceptance substituted: the position each start is
//! prepared at is what the engine would be asked to resume.
use super::*;
use plx_plex::plex::session::ResumeRewind;

const S: i64 = 1_000_000_000;

/// Records the position each start was prepared at; accepts every request and refuses the start
/// itself, so no player page is pushed and nothing but the launch policy runs.
#[derive(Default)]
struct Starts(Vec<i64>);

impl PlaybackResources for Starts {
    fn request_movie(&mut self, _: &mut plx_media::route::PlaybackSession,
        _: &mut plx_data::stores::metadata::MetadataStore, _: &plx_data::pms::PmsMovie,
        _: Option<plx_plex::plex::MediaId>) -> bool {
        true
    }
    fn request_episode(&mut self, _: &mut plx_media::route::PlaybackSession,
        _: &mut plx_data::stores::metadata::MetadataStore, _: &str,
        _: Option<plx_plex::plex::MediaId>) -> bool {
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

/// Sets the live Rewind on resume choice and puts the previous one back on drop.
struct Rewind(ResumeRewind);
impl Rewind {
    fn set(rewind: ResumeRewind) -> Self {
        let prior = Rewind(plx_media::route::resume_rewind());
        plx_media::route::restore_resume_rewind(rewind);
        prior
    }
}
impl Drop for Rewind {
    fn drop(&mut self) {
        plx_media::route::restore_resume_rewind(self.0);
    }
}

/// The position `start_playback_with` hands the engine for one launch.
fn started_at(resume_ns: i64, from: Origin) -> i64 {
    let mut ps = plx_media::route::PlaybackSession::default();
    let mut pa = plx_media::player::adapter::PlayerAdapter::new(unsafe { plx_base::task::MainThread::assume() });
    let mut pages = plx_ui::dispatch::Dispatcher::<super::super::bridge::AppHost>::new();
    let mut bridge = super::super::bridge::Bridge::for_test(|| 0);
    let mut starts = Starts::default();
    start_playback_with(&mut ps, &mut pa, resume_ns, from, 0, None, &mut pages, &mut bridge, &mut starts);
    assert_eq!(starts.0.len(), 1, "one launch prepares one start");
    starts.0[0]
}

/// **Off keeps the exact resume, and each step rewinds a viewer's resume by its length**, never
/// before the start.
#[test]
fn a_viewers_resume_is_rewound_by_the_chosen_length_and_off_keeps_it_exact() {
    let _serial = plx_base::testlock::serial();
    for (rewind, want) in [
        (ResumeRewind::Off, 600 * S),
        (ResumeRewind::Seconds5, 595 * S),
        (ResumeRewind::Seconds10, 590 * S),
        (ResumeRewind::Seconds15, 585 * S),
        (ResumeRewind::Seconds30, 570 * S),
    ] {
        let _rewind = Rewind::set(rewind);
        assert_eq!(started_at(600 * S, Origin::Here), want, "{rewind:?}");
    }
    let _rewind = Rewind::set(ResumeRewind::Seconds30);
    assert_eq!(started_at(20 * S, Origin::Here), 0, "a rewind past the start starts at the start");
}

/// **What is not a viewer's resume is never rewound**: a start from the beginning stays at 0, and a
/// handoff the player makes itself (`Origin::Unchanged`: Up Next and the queue's Next/Previous, via
/// `queue_transport::play`) starts exactly where its item's own resume point says.
#[test]
fn a_start_from_the_beginning_and_a_queue_handoff_are_not_rewound() {
    let _serial = plx_base::testlock::serial();
    let _rewind = Rewind::set(ResumeRewind::Seconds30);
    assert_eq!(started_at(0, Origin::Here), 0, "from the beginning");
    assert_eq!(started_at(600 * S, Origin::Unchanged), 600 * S, "an Up Next / Next / Previous handoff keeps its position");
    assert_eq!(started_at(0, Origin::Unchanged), 0);
}

/// **A Continue Watching card resumes rewound, and Play from Start does not**: the captured-row
/// launch computes Plex's resume point and hands it to the same policy.
#[test]
fn a_continue_watching_card_resumes_rewound_and_play_from_start_starts_at_zero() {
    let _serial = plx_base::testlock::serial();
    let _session = plx_plex::plex::session::TempSession::new("resume-rewind-card");
    plx_plex::plex::reset_servers_for_test();
    let _rewind = Rewind::set(ResumeRewind::Seconds10);
    let row = plx_data::pms::PmsMovie {
        rk: "card-7".into(),
        resume_ms: 600_000,
        dur_ns: 3_600 * S,
        ..Default::default()
    };
    for (from_start, want) in [(false, 590 * S), (true, 0)] {
        let mut ps = plx_media::route::PlaybackSession::default();
        let mut pa = plx_media::player::adapter::PlayerAdapter::new(unsafe { plx_base::task::MainThread::assume() });
        let mut pages = plx_ui::dispatch::Dispatcher::<super::super::bridge::AppHost>::new();
        let mut bridge = super::super::bridge::Bridge::for_test(|| 0);
        let mut starts = Starts::default();
        play_item_now_with(&mut ps, &mut pa, &row, from_start, None, Origin::Here, 0, None, &mut pages, &mut bridge, &mut starts);
        assert_eq!(starts.0, [want], "from_start={from_start}");
    }
    plx_plex::plex::reset_servers_for_test();
}
