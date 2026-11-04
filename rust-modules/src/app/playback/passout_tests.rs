//! Passout protection's pure decision ([`passout_holds`]) and the idle time it is given
//! ([`Advance::automatic`]). The loop's two automatic advances, and a press that always plays, are
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
