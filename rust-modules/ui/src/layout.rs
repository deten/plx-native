//! **Which layout the app wears** — PlxNative's own (Standard), or the one that follows the Plex app
//! on Android TV (Shield). The live, process-wide answer the renderers and the shared chrome read.
//!
//! The stored value is the session's `layout_mode` (`plx_plex::plex::session::LayoutMode`); this
//! crate cannot name it, so the application publishes the one fact it decides here: at boot, at
//! every auth landing, and when the Settings picker's write is durable. A read is one relaxed
//! load, cheap enough for every frame and every key.
//!
//! **What Shield changes today** is the CHROME — the bar-wearing pages draw the collapsed icon
//! rail and the clock instead of the top bar, publish no top strip for focus to land on, and LEFT
//! off their left-most content opens the side panel (`side_nav`) — and HOME, which wears a
//! focused-item header over fixed-slot poster rows (`shield`, `screens::home`'s `shield`). Every
//! other page's geometry is unchanged. The swap still applies live: the chrome reads this value per
//! frame, and Home latches it at the top of each event and re-dresses itself in place when it
//! changes, so no page is ever laid out by one layout and drawn by the other.
//!
//! In a test build the value is per THREAD, so a test that switches the layout cannot change what
//! a parallel test's dispatcher, chrome or Home step sees. Production is one atomic, written on
//! the storage worker and read on the frame thread.

#[cfg(not(any(test, feature = "test-support")))]
static SHIELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static SHIELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Is the Shield layout live?
#[cfg(not(any(test, feature = "test-support")))]
pub fn shield() -> bool {
    SHIELD.load(std::sync::atomic::Ordering::Relaxed)
}

/// Is the Shield layout live? (This thread's value: see the module doc.)
#[cfg(any(test, feature = "test-support"))]
pub fn shield() -> bool {
    SHIELD.with(|v| v.get())
}

/// Publish the layout a durable session holds (boot, an auth landing, the picker's write).
#[cfg(not(any(test, feature = "test-support")))]
pub fn publish(shield: bool) {
    SHIELD.store(shield, std::sync::atomic::Ordering::Relaxed);
}

/// Publish the layout for this thread (a test build: see the module doc).
#[cfg(any(test, feature = "test-support"))]
pub fn publish(shield: bool) {
    SHIELD.with(|v| v.set(shield));
}

/// Run `body` with the Shield layout live on this thread, restoring Standard afterwards even if
/// it panics.
#[cfg(any(test, feature = "test-support"))]
pub fn with_shield<T>(body: impl FnOnce() -> T) -> T {
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            publish(false);
        }
    }
    let _restore = Restore;
    publish(true);
    body()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_is_the_default_and_a_publish_is_what_is_read_back() {
        assert!(!shield(), "a fresh thread wears the Standard layout");
        publish(true);
        assert!(shield());
        publish(false);
        assert!(!shield());
    }

    #[test]
    fn with_shield_restores_standard_after_its_body() {
        assert!(with_shield(shield));
        assert!(!shield());
        let unwound = std::panic::catch_unwind(|| with_shield(|| panic!("inside")));
        assert!(unwound.is_err());
        assert!(!shield(), "a panicking body still restores Standard");
    }

    #[test]
    fn another_thread_keeps_its_own_value_in_a_test_build() {
        publish(true);
        assert!(!std::thread::spawn(shield).join().unwrap());
        publish(false);
    }
}
