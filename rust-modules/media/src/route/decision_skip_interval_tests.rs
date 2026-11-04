//! The Skip interval persistence seam: durable-first, one `feature.used` per durable pick. The pick is
//! the seated profile's own (P1a), so each test signs a profile in.
use super::prefs_characterisation_tests::{picked, sign_in};
use super::*;
use plx_telemetry::diag::schema::{DiagEvent, Feature};
use plx_plex::plex::session::{SkipInterval, SkipIntervalPref};

struct Restore(SkipInterval);
impl Drop for Restore {
    fn drop(&mut self) {
        restore_skip_interval(self.0);
    }
}

/// **A durable pick is live, read back by the next load, and reported once** (as the length alone).
#[test]
fn a_durable_pick_is_live_persisted_and_reported_once() {
    let _g = plx_base::testlock::serial();
    let session = plx_plex::plex::session::TempSession::new("skip-interval-set");
    sign_in(&session);
    let _restore = Restore(skip_interval());
    restore_skip_interval(SkipInterval::Seconds10);

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_skip_interval(SkipInterval::Seconds30));
    assert!(saved);
    assert_eq!(skip_interval(), SkipInterval::Seconds30);
    assert_eq!(picked::<SkipIntervalPref>(), Some(SkipInterval::Seconds30));
    assert_eq!(
        events,
        [DiagEvent::FeatureUsed { feature: Feature::SkipInterval(SkipInterval::Seconds30) }]
    );

    // choosing the default again is the profile's own value, and still a (reported) pick
    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_skip_interval(SkipInterval::Seconds10));
    assert!(saved);
    assert_eq!(picked::<SkipIntervalPref>(), Some(SkipInterval::Seconds10));
    assert_eq!(events.len(), 1);
}

/// **A failed write claims nothing**: the live value stays and nothing is reported.
#[test]
fn a_failed_write_changes_and_reports_nothing() {
    let _g = plx_base::testlock::serial();
    let session = plx_plex::plex::session::TempSession::new("skip-interval-failed");
    sign_in(&session);
    let _restore = Restore(skip_interval());
    restore_skip_interval(SkipInterval::Seconds10);
    // the session file's parent is a regular file, so no write can land
    let dir = std::env::temp_dir().join(format!("plxnative-skip-interval-blocked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("blocker"), b"x").unwrap();
    plx_plex::plex::session::redirect_for_test(Some(dir.join("blocker").join("auth.json")));

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_skip_interval(SkipInterval::Seconds60));
    plx_plex::plex::session::redirect_for_test(None);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!saved);
    assert_eq!(skip_interval(), SkipInterval::Seconds10);
    assert!(events.is_empty(), "{events:?}");
}

/// **Skip intro and Skip credits take the same seam**: a durable pick is live, the seated profile's
/// own, and reported once as the choice alone, each under its own kind.
#[test]
fn a_durable_skip_intro_or_skip_credits_pick_is_live_persisted_and_reported_once() {
    use plx_plex::plex::session::{MarkerSkip, SkipCreditsPref, SkipIntroPref};
    struct Back(MarkerSkip, MarkerSkip);
    impl Drop for Back {
        fn drop(&mut self) {
            restore_skip_intro(self.0);
            restore_skip_credits(self.1);
        }
    }
    let _g = plx_base::testlock::serial();
    let session = plx_plex::plex::session::TempSession::new("marker-skip-set");
    sign_in(&session);
    let _restore = Back(skip_intro(), skip_credits());
    restore_skip_intro(MarkerSkip::Button);
    restore_skip_credits(MarkerSkip::Button);

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_skip_intro(MarkerSkip::Automatic));
    assert!(saved);
    assert_eq!((skip_intro(), skip_credits()), (MarkerSkip::Automatic, MarkerSkip::Button));
    assert_eq!(picked::<SkipIntroPref>(), Some(MarkerSkip::Automatic));
    assert_eq!(events, [DiagEvent::FeatureUsed { feature: Feature::SkipIntroMode(MarkerSkip::Automatic) }]);

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_skip_credits(MarkerSkip::Off));
    assert!(saved);
    assert_eq!((skip_intro(), skip_credits()), (MarkerSkip::Automatic, MarkerSkip::Off));
    assert_eq!(picked::<SkipCreditsPref>(), Some(MarkerSkip::Off));
    assert_eq!(events, [DiagEvent::FeatureUsed { feature: Feature::SkipCreditsMode(MarkerSkip::Off) }]);
}
