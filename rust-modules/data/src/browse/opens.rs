//! **Which view a library opens on** (Settings > Libraries > Library opens on): the live,
//! install-wide value the Library screen reads each time a library opens, and its persistence seam.
//!
//! The stored value is `Session::library_opens`; boot and every auth landing restore it here, and
//! the Settings picker writes it through [`set_library_opens`] on the storage worker. It lives
//! with the browse store rather than in the player's route because only the Library reads it.
//! What the view switcher last chose for each library is not here: that is per profile and per
//! library, kept in the library's remembered view (`LibraryView::all`, written by
//! `BrowseState::note_page_choice`).

use std::sync::atomic::{AtomicU8, Ordering};

pub use plx_plex::plex::session::LibraryOpens;

static LIBRARY_OPENS: AtomicU8 = AtomicU8::new(0); // LibraryOpens::Default's index

/// The live choice.
pub fn library_opens() -> LibraryOpens {
    LibraryOpens::from_index(LIBRARY_OPENS.load(Ordering::Relaxed))
}

/// Publish a choice already in the session (boot, an auth landing, a durable write).
pub fn restore_library_opens(mode: LibraryOpens) {
    #[cfg(any(test, feature = "test-support"))]
    plx_base::testlock::assert_held("library opens preference");
    LIBRARY_OPENS.store(mode.index(), Ordering::Relaxed);
}

/// Blocking persistence seam; Settings dispatches it on the storage worker. The live value
/// changes only once the write is durable, so a failed save claims nothing.
pub fn set_library_opens(mode: LibraryOpens) -> bool {
    let saved = plx_plex::plex::session::update_with_outcome(|s| Some(s.with_library_opens(mode)))
        .is_some_and(|write| matches!(write.classify(),
            plx_plex::plex::session::async_persistence::CompletionOutcome::Durable(_)));
    if saved {
        restore_library_opens(mode);
        plx_machine::idle::invalidate();
    }
    saved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_live_value_is_what_was_last_restored() {
        let _guard = plx_base::testlock::serial();
        for mode in LibraryOpens::LADDER {
            restore_library_opens(mode);
            assert_eq!(library_opens(), mode);
        }
        restore_library_opens(LibraryOpens::Default);
        assert_eq!(library_opens(), LibraryOpens::Default);
    }
}
