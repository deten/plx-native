//! The Rewind on resume and Passout protection persistence seams (durable-first, one
//! `feature.used` per durable pick) and the rewind read a viewer's launch makes.
use super::*;
use plx_telemetry::diag::schema::{DiagEvent, Feature};
use plx_plex::plex::session::{PassoutProtection, ResumeRewind};

struct Restore(ResumeRewind, PassoutProtection);
impl Drop for Restore {
    fn drop(&mut self) {
        restore_resume_rewind(self.0);
        restore_passout_protection(self.1);
    }
}

/// **A durable Rewind on resume pick is live, read back by the next load, and reported once.**
#[test]
fn a_durable_rewind_pick_is_live_persisted_and_reported_once() {
    let _g = plx_base::testlock::serial();
    let _session = plx_plex::plex::session::TempSession::new("resume-rewind-set");
    let _restore = Restore(resume_rewind(), passout_protection());
    restore_resume_rewind(ResumeRewind::None);

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_resume_rewind(ResumeRewind::Seconds15));
    assert!(saved);
    assert_eq!(resume_rewind(), ResumeRewind::Seconds15);
    assert_eq!(plx_plex::plex::session::load().resume_rewind(), ResumeRewind::Seconds15);
    assert_eq!(events, [DiagEvent::FeatureUsed { feature: Feature::ResumeRewind(ResumeRewind::Seconds15) }]);
}

/// **The launch's read follows the live choice**: none leaves a resume exact, a pick moves it
/// earlier and clamps at the start, and a start from the beginning stays one.
#[test]
fn the_rewound_resume_follows_the_live_choice() {
    let _g = plx_base::testlock::serial();
    let _restore = Restore(resume_rewind(), passout_protection());
    const S: i64 = 1_000_000_000;
    restore_resume_rewind(ResumeRewind::None);
    assert_eq!(rewound_resume_ns(90 * S), 90 * S);
    restore_resume_rewind(ResumeRewind::Seconds10);
    assert_eq!(rewound_resume_ns(90 * S), 80 * S);
    assert_eq!(rewound_resume_ns(4 * S), 0);
    assert_eq!(rewound_resume_ns(0), 0);
}

/// **A durable Passout protection pick is live, read back by the next load, and reported once.**
#[test]
fn a_durable_passout_pick_is_live_persisted_and_reported_once() {
    let _g = plx_base::testlock::serial();
    let _session = plx_plex::plex::session::TempSession::new("passout-protection-set");
    let _restore = Restore(resume_rewind(), passout_protection());
    restore_passout_protection(PassoutProtection::Off);

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| set_passout_protection(PassoutProtection::Hours2));
    assert!(saved);
    assert_eq!(passout_protection(), PassoutProtection::Hours2);
    assert_eq!(plx_plex::plex::session::load().passout_protection(), PassoutProtection::Hours2);
    assert_eq!(events, [DiagEvent::FeatureUsed { feature: Feature::PassoutProtection(PassoutProtection::Hours2) }]);
}

/// **A failed write claims nothing** for either preference: the live values stay and nothing is
/// reported.
#[test]
fn a_failed_write_changes_and_reports_nothing() {
    let _g = plx_base::testlock::serial();
    let _session = plx_plex::plex::session::TempSession::new("resume-passout-failed");
    let _restore = Restore(resume_rewind(), passout_protection());
    restore_resume_rewind(ResumeRewind::None);
    restore_passout_protection(PassoutProtection::Off);
    // the session file's parent is a regular file, so no write can land
    let dir = std::env::temp_dir().join(format!("plxnative-resume-passout-blocked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("blocker"), b"x").unwrap();
    plx_plex::plex::session::redirect_for_test(Some(dir.join("blocker").join("auth.json")));

    let (saved, events) = plx_telemetry::diag::test_events::capture(|| {
        (set_resume_rewind(ResumeRewind::Seconds30), set_passout_protection(PassoutProtection::Hours1))
    });
    plx_plex::plex::session::redirect_for_test(None);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(saved, (false, false));
    assert_eq!((resume_rewind(), passout_protection()), (ResumeRewind::None, PassoutProtection::Off));
    assert!(events.is_empty(), "{events:?}");
}
