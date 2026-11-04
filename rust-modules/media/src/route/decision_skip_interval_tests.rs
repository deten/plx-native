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

/// **Passout protection takes the same seam**: a durable pick is live, the seated profile's own,
/// and reported once as the limit alone.
#[test]
fn a_durable_passout_protection_pick_is_live_persisted_and_reported_once() {
    use plx_plex::plex::session::{PassoutProtection, PassoutProtectionPref};
    struct Back(PassoutProtection);
    impl Drop for Back {
        fn drop(&mut self) {
            restore_passout_protection(self.0);
        }
    }
    let _g = plx_base::testlock::serial();
    let session = plx_plex::plex::session::TempSession::new("passout-protection-set");
    sign_in(&session);
    let _restore = Back(passout_protection());
    restore_passout_protection(PassoutProtection::Off);

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_passout_protection(PassoutProtection::Hours2));
    assert!(saved);
    assert_eq!(passout_protection(), PassoutProtection::Hours2);
    assert_eq!(picked::<PassoutProtectionPref>(), Some(PassoutProtection::Hours2));
    assert_eq!(
        events,
        [DiagEvent::FeatureUsed { feature: Feature::PassoutProtection(PassoutProtection::Hours2) }]
    );
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
