//! The Skip forward (`skip_interval`) and Skip back persistence seams: durable-first, one
//! `feature.used` per durable pick, and the two values independent.
use super::*;
use plx_telemetry::diag::schema::{DiagEvent, Feature};
use plx_plex::plex::session::SkipInterval;

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
    let _session = plx_plex::plex::session::TempSession::new("skip-interval-set");
    let _restore = Restore(skip_interval());
    restore_skip_interval(SkipInterval::Seconds10);

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_skip_interval(SkipInterval::Seconds30));
    assert!(saved);
    assert_eq!(skip_interval(), SkipInterval::Seconds30);
    assert_eq!(plx_plex::plex::session::load().skip_interval(), SkipInterval::Seconds30);
    assert_eq!(
        events,
        [DiagEvent::FeatureUsed { feature: Feature::SkipInterval(SkipInterval::Seconds30) }]
    );

    // choosing the default again removes the key from the file, and is still a (reported) pick
    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_skip_interval(SkipInterval::Seconds10));
    assert!(saved);
    assert_eq!(plx_plex::plex::session::load().skip_interval(), SkipInterval::Seconds10);
    assert_eq!(events.len(), 1);
}

/// **A failed write claims nothing**: the live value stays and nothing is reported.
#[test]
fn a_failed_write_changes_and_reports_nothing() {
    let _g = plx_base::testlock::serial();
    let _session = plx_plex::plex::session::TempSession::new("skip-interval-failed");
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

struct RestoreBack(SkipInterval);
impl Drop for RestoreBack {
    fn drop(&mut self) {
        restore_skip_back(self.0);
    }
}

/// **Skip back is its own value**: a durable back pick is live, persisted and reported under its
/// own feature, and leaves the forward hop (`skip_interval`) where it was, live and on disk.
#[test]
fn a_skip_back_pick_is_its_own_and_leaves_skip_forward_alone() {
    let _g = plx_base::testlock::serial();
    let _session = plx_plex::plex::session::TempSession::new("skip-back-set");
    let _restore = (Restore(skip_interval()), RestoreBack(skip_back()));
    restore_skip_interval(SkipInterval::Seconds10);
    restore_skip_back(SkipInterval::Seconds10);
    assert!(set_skip_interval(SkipInterval::Seconds30));

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_skip_back(SkipInterval::Seconds5));
    assert!(saved);
    assert_eq!((skip_back(), skip_interval()), (SkipInterval::Seconds5, SkipInterval::Seconds30));
    let loaded = plx_plex::plex::session::load();
    assert_eq!((loaded.skip_back(), loaded.skip_interval()), (SkipInterval::Seconds5, SkipInterval::Seconds30));
    assert_eq!(
        events,
        [DiagEvent::FeatureUsed { feature: Feature::SkipBack(SkipInterval::Seconds5) }]
    );
}

/// **A failed Skip back write claims nothing**, like the forward one.
#[test]
fn a_failed_skip_back_write_changes_and_reports_nothing() {
    let _g = plx_base::testlock::serial();
    let _session = plx_plex::plex::session::TempSession::new("skip-back-failed");
    let _restore = RestoreBack(skip_back());
    restore_skip_back(SkipInterval::Seconds10);
    let dir = std::env::temp_dir().join(format!("plxnative-skip-back-blocked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("blocker"), b"x").unwrap();
    plx_plex::plex::session::redirect_for_test(Some(dir.join("blocker").join("auth.json")));

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_skip_back(SkipInterval::Seconds60));
    plx_plex::plex::session::redirect_for_test(None);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!saved);
    assert_eq!(skip_back(), SkipInterval::Seconds10);
    assert!(events.is_empty(), "{events:?}");
}
