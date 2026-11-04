//! Passout protection's two pure decisions, at an advance ([`passout_holds`]) and while an episode
//! plays ([`passout_during_playback`]), and the idle time both are given ([`idle_ms`]). The loop's
//! two automatic advances, its watch over a playing episode, and a press that always plays, are
//! driven end to end in `run.rs`'s `lifecycle_regression_tests`.
use super::*;
use plx_plex::plex::session::PassoutProtection;

const H: u32 = 3_600_000;

fn auto(idle_ms: u32) -> Advance {
    Advance::Automatic { idle_ms }
}

/// **Off never holds anything back**, however long nobody pressed a button.
#[test]
fn off_never_holds_an_advance_back() {
    for advance in [auto(0), auto(H), auto(10 * H), auto(u32::MAX), Advance::Pressed] {
        assert!(!passout_holds(advance, PassoutProtection::Off, true), "{advance:?}");
    }
}

/// **A limit holds back an automatic advance once that much idle time has passed**, and not a
/// millisecond before; a longer limit still waits.
#[test]
fn a_limit_holds_back_an_automatic_advance_from_the_moment_it_is_reached() {
    assert!(!passout_holds(auto(H - 1), PassoutProtection::Hours1, true), "inside the limit");
    assert!(passout_holds(auto(H), PassoutProtection::Hours1, true), "at the limit");
    assert!(passout_holds(auto(5 * H), PassoutProtection::Hours1, true), "long past it");
    assert!(!passout_holds(auto(2 * H), PassoutProtection::Hours3, true), "a longer limit still waits");
    assert!(passout_holds(auto(3 * H), PassoutProtection::Hours3, true));
}

/// **A pressed Next Episode always advances**, after any idle time and under any limit, and with
/// nothing queued there is no advance to hold back.
#[test]
fn a_press_always_advances_and_nothing_queued_holds_nothing() {
    for protection in PassoutProtection::LADDER {
        assert!(!passout_holds(Advance::Pressed, protection, true), "{protection:?}: a press always advances");
        assert!(!passout_holds(auto(u32::MAX), protection, false), "{protection:?}: nothing queued");
    }
}

/// **A key press resets the clock**: the idle time is measured from the last press on the wrapping
/// millisecond clock, so a press a minute before the advance leaves a minute of idle time however
/// long the room was quiet before it. A stamp ahead of the clock reads as no idle time rather than
/// ~49 days of it.
#[test]
fn the_idle_time_runs_from_the_last_press_on_the_wrapping_clock() {
    let quiet_since = 1_000;
    let now = quiet_since + 2 * H;
    assert!(passout_holds(Advance::automatic(now, quiet_since), PassoutProtection::Hours1, true), "two quiet hours");
    let pressed = now - 60_000;
    assert_eq!(Advance::automatic(now, pressed), auto(60_000));
    assert!(!passout_holds(Advance::automatic(now, pressed), PassoutProtection::Hours1, true), "a press a minute ago");
    assert_eq!(Advance::automatic(500, u32::MAX - 499), auto(1_000), "across the wrap");
    assert_eq!(Advance::automatic(1_000, 5_000), auto(0), "a stamp ahead of now");
}

const MIN: u64 = 60_000;

/// **Movies and every other non-episode are never touched**: a film, trailer, extra or clip plays
/// on however long the room has been quiet, paused or not, whatever is left.
#[test]
fn anything_but_an_episode_is_never_stopped_or_ended_early() {
    for protection in PassoutProtection::LADDER {
        for left in [None, Some(0), Some(5 * MIN), Some(20 * MIN), Some(90 * MIN)] {
            for paused in [false, true] {
                assert_eq!(
                    passout_during_playback(protection, u32::MAX, false, left, paused),
                    Passout::Watching,
                    "{protection:?} left={left:?} paused={paused}",
                );
            }
        }
    }
}

/// **Off never does anything** to an episode either.
#[test]
fn off_never_stops_or_ends_an_episode() {
    for left in [None, Some(0), Some(5 * MIN), Some(90 * MIN)] {
        for paused in [false, true] {
            assert_eq!(passout_during_playback(PassoutProtection::Off, u32::MAX, true, left, paused), Passout::Watching);
        }
    }
}

/// **Inside the limit nothing happens**, however much of the episode is left.
#[test]
fn inside_the_limit_an_episode_plays_on() {
    for (protection, hours) in [(PassoutProtection::Hours1, 1), (PassoutProtection::Hours2, 2), (PassoutProtection::Hours3, 3)] {
        for left in [Some(5 * MIN), Some(90 * MIN)] {
            assert_eq!(passout_during_playback(protection, hours * H - 1, true, left, false), Passout::Watching, "{protection:?}");
            assert_eq!(passout_during_playback(protection, hours * H - 1, true, left, true), Passout::Watching, "{protection:?} paused");
        }
    }
}

/// **At the limit, more than twenty minutes left stops the episode now, twenty or less lets it
/// finish**: the boundary is inclusive, so exactly twenty minutes left plays out and a millisecond
/// more stops.
#[test]
fn at_the_limit_a_long_stretch_left_stops_and_a_short_one_finishes() {
    assert_eq!(PASSOUT_FINISH_WITHIN_MS, 20 * MIN);
    let at = |left| passout_during_playback(PassoutProtection::Hours1, H, true, Some(left), false);
    assert_eq!(at(45 * MIN), Passout::Stop, "most of an episode left");
    assert_eq!(at(20 * MIN + 1), Passout::Stop, "a millisecond past the window");
    assert_eq!(at(20 * MIN), Passout::FinishEpisode, "exactly twenty minutes left");
    assert_eq!(at(3 * MIN), Passout::FinishEpisode, "the credits");
    assert_eq!(at(0), Passout::FinishEpisode, "at the very end");
    assert_eq!(
        passout_during_playback(PassoutProtection::Hours3, 5 * H, true, Some(45 * MIN), false),
        Passout::Stop,
        "long past a longer limit",
    );
}

/// **A paused episode is stopped at the limit whatever is left**: it cannot reach its end on its
/// own, so there is nothing to let finish.
#[test]
fn a_paused_episode_is_stopped_at_the_limit_whatever_is_left() {
    for left in [0, 3 * MIN, 20 * MIN, 45 * MIN] {
        assert_eq!(passout_during_playback(PassoutProtection::Hours1, H, true, Some(left), true), Passout::Stop, "{left}");
    }
}

/// **An episode whose length is not known yet is left alone** until it is.
#[test]
fn an_unknown_length_waits() {
    for paused in [false, true] {
        assert_eq!(passout_during_playback(PassoutProtection::Hours1, u32::MAX, true, None, paused), Passout::Watching);
    }
}

/// **A key press resets the clock** for the watch too: the same idle time as the advance's.
#[test]
fn a_press_inside_the_limit_keeps_a_long_episode_playing() {
    let now = 10 * H;
    let pressed = now - 60_000;
    assert_eq!(idle_ms(now, pressed), 60_000);
    assert_eq!(
        passout_during_playback(PassoutProtection::Hours1, idle_ms(now, pressed), true, Some(45 * MIN), false),
        Passout::Watching,
    );
    assert_eq!(
        passout_during_playback(PassoutProtection::Hours1, idle_ms(now, now - 2 * H), true, Some(45 * MIN), false),
        Passout::Stop,
    );
    assert_eq!(idle_ms(1_000, 5_000), 0, "a stamp ahead of now");
}
