//! **The wall clock as the Shield layout reads it** ("9:18 AM"): the chrome's clock pill on the
//! bar-wearing pages (`app::chrome`, drawn by `plx_ui::side_nav::draw_clock`) and the player's
//! transport (`player_hud`'s Shield bar) each own one [`ClockReadout`]. The reading itself is
//! `plx_platform::tv::clock`'s; this re-formats and re-measures it only when the minute turns.
//!
//! A read-out, never logical state: nothing branches on it and a recording replays the interface's
//! state whatever the clock says (`plx_platform::tv::clock`'s own doc).
use plx_machine::machine::Measure;

/// The local time as the clock shows it, re-formatted and re-measured only when the minute turns.
#[derive(Default)]
pub struct ClockReadout {
    minute: Option<plx_platform::tv::clock::LocalTime>,
    text: std::ffi::CString,
    width: f32,
}

impl ClockReadout {
    /// Read the clock; `true` when the figures changed (the caller asks for a frame).
    pub fn refresh(&mut self, measure: &dyn Measure) -> bool {
        self.show(plx_platform::tv::clock::now(), measure)
    }

    fn show(&mut self, now: Option<plx_platform::tv::clock::LocalTime>, measure: &dyn Measure) -> bool {
        if now == self.minute {
            return false;
        }
        self.minute = now;
        self.text = now.map(|now| {
            let (hour, pm) = now.twelve_hour();
            let time = format!("{hour}:{:02}", now.minute);
            if pm { plx_platform::i18n::msg::browse_clock_pm(&time) } else { plx_platform::i18n::msg::browse_clock_am(&time) }
        }).and_then(|s| std::ffi::CString::new(s).ok()).unwrap_or_default();
        self.width = measure.width(&self.text, plx_ui::side_nav::CLOCK_SIZE, false);
        true
    }

    /// The figures and their measured width, or `None` when there is no clock to show.
    pub fn reading(&self) -> Option<(&std::ffi::CStr, f32)> {
        self.minute.map(|_| (self.text.as_c_str(), self.width))
    }

    /// The minute the figures read, for a fingerprint that must move when they do.
    pub fn minute_key(&self) -> u32 {
        self.minute.map_or(u32::MAX, |m| u32::from(m.hour) * 60 + u32::from(m.minute))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clock pill reads 12-hour figures and re-formats only when the minute turns.
    #[test]
    fn the_clock_reads_twelve_hour_figures_and_changes_with_the_minute() {
        use plx_platform::tv::clock::LocalTime;
        let mut clock = ClockReadout::default();
        assert!(clock.reading().is_none());
        let at = |hour, minute| Some(LocalTime { hour, minute });
        assert!(clock.show(at(9, 18), &plx_ui::fixture::FixtureMeasure));
        assert_eq!(clock.reading().unwrap().0.to_str().unwrap(), "9:18 AM");
        assert!(!clock.show(at(9, 18), &plx_ui::fixture::FixtureMeasure), "the same minute asks for nothing");
        assert!(clock.show(at(0, 5), &plx_ui::fixture::FixtureMeasure));
        assert_eq!(clock.reading().unwrap().0.to_str().unwrap(), "12:05 AM");
        assert!(clock.show(at(13, 40), &plx_ui::fixture::FixtureMeasure));
        assert_eq!(clock.reading().unwrap().0.to_str().unwrap(), "1:40 PM");
        assert!(clock.reading().unwrap().1 > 0.0);
        assert!(clock.show(None, &plx_ui::fixture::FixtureMeasure));
        assert!(clock.reading().is_none(), "no clock, no pill");
    }

}
