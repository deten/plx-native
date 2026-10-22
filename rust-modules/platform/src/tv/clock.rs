//! The television's wall clock as the interface reads it: the local hour and minute, for the
//! Shield layout's clock pill (`plx_ui::side_nav`, drawn by the application's chrome).
//!
//! **Local time is the C library's**: `localtime_r` over `time`, in the zone the process was given
//! (`TZ`, else the system zone file). The first call loads that zone and the C library keeps it, so
//! boot primes it ([`prime`]) before the frame loop rather than paying the read on a frame. Whether
//! the webOS zone file follows the set's own time-zone setting has NOT been verified on the
//! television; the simulator follows its host.
//!
//! This is a read-out, never logical state: nothing branches on it, and a recording replays the
//! interface's state whatever the clock says. It lives here, below the UI and the application,
//! because the wall-clock gate (`ci/check-deps.sh`, `wall`) keeps `SystemTime` reads out of every
//! layer that holds logical state.

/// One reading of the local wall clock, to the minute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    /// 0..=23
    pub hour: u8,
    /// 0..=59
    pub minute: u8,
}

impl LocalTime {
    /// The 12-hour face of the hour, 1..=12, and whether it is after noon.
    pub fn twelve_hour(self) -> (u8, bool) {
        let pm = self.hour >= 12;
        let h = self.hour % 12;
        (if h == 0 { 12 } else { h }, pm)
    }
}

/// The local time now, or `None` when the C library cannot say (no clock, no zone).
pub fn now() -> Option<LocalTime> {
    // SAFETY: `time(NULL)` only returns a value. `localtime_r` writes the zeroed `tm` it is given
    // and nothing else, and its result is checked before the fields are read.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        if t == -1 {
            return None;
        }
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        Some(LocalTime { hour: tm.tm_hour.clamp(0, 23) as u8, minute: tm.tm_min.clamp(0, 59) as u8 })
    }
}

/// Load the zone once, at boot, before the frame loop: the first `localtime_r` reads it.
pub fn prime() {
    let _ = now();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_twelve_hour_face_names_midnight_and_noon_twelve() {
        let at = |hour| LocalTime { hour, minute: 0 }.twelve_hour();
        assert_eq!(at(0), (12, false));
        assert_eq!(at(9), (9, false));
        assert_eq!(at(11), (11, false));
        assert_eq!(at(12), (12, true));
        assert_eq!(at(13), (1, true));
        assert_eq!(at(23), (11, true));
    }

    #[test]
    fn the_host_clock_reads_in_range() {
        let now = now().expect("a host test machine has a clock and a zone");
        assert!(now.hour < 24 && now.minute < 60);
    }
}
