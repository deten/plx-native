//! The per-profile choices for instant D-pad seeking, Dolby Vision and the Skip Intro / Skip
//! Credits offers.
use serde::{Deserialize, Serialize};

/// Whether Left/Right on a hidden transport jumps at once instead of first revealing the scrubber —
/// one per profile, like [`super::DeckPress`]. Absence is `Off`, the reveal-first behaviour every
/// build before the preference had.
#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstantDpadSeeking {
    #[default]
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "on")]
    On,
}

impl InstantDpadSeeking {
    /// Every option, the order the picker lists them in.
    pub const LADDER: [InstantDpadSeeking; 2] = [InstantDpadSeeking::Off, InstantDpadSeeking::On];

    /// An in-memory index back to an option — out of range is the default, for the reason
    /// [`super::SubtitleTone::from_index`] gives: the ladder can grow or shrink.
    pub fn from_index(i: u8) -> InstantDpadSeeking {
        Self::LADDER.get(i as usize).copied().unwrap_or_default()
    }

    pub fn index(self) -> u8 {
        Self::LADDER.iter().position(|&m| m == self).unwrap_or(0) as u8
    }

    /// `skip_serializing_if` needs a function, not just `PartialEq` with `Default::default()`.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Whether Dolby Vision is declared when the TV reports support, or never — one per profile, like
/// [`InstantDpadSeeking`]. `Off` sends no Dolby Vision node, so a stream with a cross-compatible
/// base layer (Profile 8.1) plays that layer, and one without (Profile 5) is refused direct play
/// and asked of the server, which may refuse it too (code 2000, `docs/pms-api.md`). Absence is
/// `Auto`, which trusts the firmware capability probe (`tv.config.supportDolbyHDRContents`).
#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DolbyVisionMode {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "off")]
    Off,
}

impl DolbyVisionMode {
    pub const DEFAULT: Self = Self::Auto;
    /// Every option, the order the picker lists them in.
    pub const LADDER: [DolbyVisionMode; 2] = [DolbyVisionMode::Auto, DolbyVisionMode::Off];

    pub fn from_index(i: u8) -> DolbyVisionMode {
        Self::LADDER.get(i as usize).copied().unwrap_or_default()
    }

    pub fn index(self) -> u8 {
        Self::LADDER.iter().position(|&m| m == self).unwrap_or(0) as u8
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The capability a route's presentation is planned from: `Auto` is the set's own answer, `Off`
    /// is `Unsupported` whatever the set reports. Applied where a route is planned
    /// (`route::frozen_dv_decision`), to the presentation only: never to the probe itself and never
    /// to the capability a playing route's `DvDecision` stores, so Diagnostics and the `dv:` log
    /// lines still show what the television said.
    pub fn gate(self, probed: plx_platform::devcaps::dv::DvCapability) -> plx_platform::devcaps::dv::DvCapability {
        match self {
            DolbyVisionMode::Auto => probed,
            DolbyVisionMode::Off => plx_platform::devcaps::dv::DvCapability::Unsupported,
        }
    }
}

/// What the player does when the playhead enters a server intro marker (the Skip intro setting) or
/// a credits marker (Skip credits) — one per profile and per kind, like [`InstantDpadSeeking`]. The
/// official Plex clients offer the same three choices (Manually, Automatically, Disabled). Absence
/// is `Button`, the Skip Intro / Skip Credits offer every build before the preference made.
///
/// `Automatic` performs the offered control's own action the moment the offer appears
/// (`appkit::player_hud::auto_skips`); `Off` withdraws that kind's Skip offer
/// (`appkit::player_hud::slot_for_with`). Neither touches the Up Next countdown's own timing: the
/// tail the app synthesizes for it is never skipped, and Up Next stays the Next episode
/// preference's.
#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerSkip {
    /// Offer the Skip button and wait for a press.
    #[default]
    #[serde(rename = "button")]
    Button,
    /// Skip the segment as soon as it is offered, as a press of the button would.
    #[serde(rename = "automatic")]
    Automatic,
    /// Offer nothing and let the segment play.
    #[serde(rename = "off")]
    Off,
}

impl MarkerSkip {
    /// Every option, the order the picker lists them in.
    pub const LADDER: [MarkerSkip; 3] = [MarkerSkip::Button, MarkerSkip::Automatic, MarkerSkip::Off];

    /// An in-memory index back to an option — out of range is the default, for the reason
    /// [`super::SubtitleTone::from_index`] gives: the ladder can grow or shrink.
    pub fn from_index(i: u8) -> MarkerSkip {
        Self::LADDER.get(i as usize).copied().unwrap_or_default()
    }

    pub fn index(self) -> u8 {
        Self::LADDER.iter().position(|&m| m == self).unwrap_or(0) as u8
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plex::session::{effective, own_prefs, put_own, DolbyVisionModePref,
        InstantDpadSeekingPref, Preferences, ProfilePrefs, SkipCreditsPref, SkipIntroPref};

    #[test]
    fn instant_dpad_seeking_is_defaulted_and_scoped_per_profile() {
        for json in ["{}", r#"{"instant_dpad_seeking":"future"}"#, r#"{"instant_dpad_seeking":null}"#] {
            let prefs: Preferences = serde_json::from_str(json).unwrap();
            assert_eq!(prefs.instant_dpad_seeking, InstantDpadSeeking::Off);
            let encoded = serde_json::to_value(&prefs).unwrap();
            assert!(encoded.get("instant_dpad_seeking").is_none());
        }
        let prefs = Preferences::default();
        let mut records = Vec::new();
        assert!(put_own::<InstantDpadSeekingPref>(&mut records, "alice", "account", InstantDpadSeeking::On));
        assert_eq!(effective::<InstantDpadSeekingPref>(&prefs, own_prefs(&records, "alice", "account")), InstantDpadSeeking::On);
        for (user, account) in [("bob", "account"), ("alice", "other")] {
            assert_eq!(effective::<InstantDpadSeekingPref>(&prefs, own_prefs(&records, user, account)), InstantDpadSeeking::Off);
        }
        let encoded = serde_json::to_string(&records[0].prefs).unwrap();
        assert_eq!(serde_json::from_str::<ProfilePrefs>(&encoded).unwrap(), records[0].prefs);
    }

    #[test]
    fn dolby_vision_mode_is_defaulted_and_scoped_per_profile() {
        for json in ["{}", r#"{"dolby_vision_mode":"future"}"#, r#"{"dolby_vision_mode":null}"#] {
            let prefs: Preferences = serde_json::from_str(json).unwrap();
            assert_eq!(prefs.dolby_vision_mode, DolbyVisionMode::Auto);
            let encoded = serde_json::to_value(&prefs).unwrap();
            assert!(encoded.get("dolby_vision_mode").is_none());
        }
        let prefs = Preferences::default();
        let mut records = Vec::new();
        assert!(put_own::<DolbyVisionModePref>(&mut records, "alice", "account", DolbyVisionMode::Off));
        assert_eq!(effective::<DolbyVisionModePref>(&prefs, own_prefs(&records, "alice", "account")), DolbyVisionMode::Off);
        for (user, account) in [("bob", "account"), ("alice", "other")] {
            assert_eq!(effective::<DolbyVisionModePref>(&prefs, own_prefs(&records, user, account)), DolbyVisionMode::Auto);
        }
        let encoded = serde_json::to_string(&records[0].prefs).unwrap();
        assert_eq!(serde_json::from_str::<ProfilePrefs>(&encoded).unwrap(), records[0].prefs);
    }

    #[test]
    fn dolby_vision_off_plans_as_an_unsupported_set_and_auto_trusts_the_probe() {
        use plx_platform::devcaps::dv::DvCapability::{Supported, Unknown, Unsupported};
        for probed in [Supported, Unknown, Unsupported] {
            assert_eq!(DolbyVisionMode::Auto.gate(probed), probed);
            assert_eq!(DolbyVisionMode::Off.gate(probed), Unsupported);
        }
    }

    /// Absent, null, an unknown spelling or a wrong type reads as Show button, and Show button writes
    /// no key; a profile's own pick is its own, round-trips, and the two kinds stay apart.
    #[test]
    fn skip_intro_and_skip_credits_are_defaulted_and_scoped_per_profile() {
        for json in [
            "{}",
            r#"{"skip_intro":"future","skip_credits":"future"}"#,
            r#"{"skip_intro":null,"skip_credits":null}"#,
            r#"{"skip_intro":1,"skip_credits":true}"#,
        ] {
            let prefs: Preferences = serde_json::from_str(json).unwrap();
            assert_eq!((prefs.skip_intro, prefs.skip_credits), (MarkerSkip::Button, MarkerSkip::Button), "{json}");
            let encoded = serde_json::to_value(&prefs).unwrap();
            assert!(encoded.get("skip_intro").is_none() && encoded.get("skip_credits").is_none(), "{json}");
        }
        let prefs = Preferences::default();
        for intro in MarkerSkip::LADDER {
            for credits in MarkerSkip::LADDER {
                let mut records = Vec::new();
                put_own::<SkipIntroPref>(&mut records, "alice", "account", intro);
                put_own::<SkipCreditsPref>(&mut records, "alice", "account", credits);
                let own = own_prefs(&records, "alice", "account");
                assert_eq!((effective::<SkipIntroPref>(&prefs, own), effective::<SkipCreditsPref>(&prefs, own)), (intro, credits));
                for (user, account) in [("bob", "account"), ("alice", "other")] {
                    let other = own_prefs(&records, user, account);
                    assert_eq!(effective::<SkipIntroPref>(&prefs, other), MarkerSkip::Button);
                    assert_eq!(effective::<SkipCreditsPref>(&prefs, other), MarkerSkip::Button);
                }
                if let Some(record) = records.first() {
                    let encoded = serde_json::to_string(&record.prefs).unwrap();
                    assert_eq!(serde_json::from_str::<ProfilePrefs>(&encoded).unwrap(), record.prefs, "{encoded}");
                }
            }
        }
        for mode in MarkerSkip::LADDER {
            assert_eq!(MarkerSkip::from_index(mode.index()), mode);
        }
        assert_eq!(MarkerSkip::from_index(200), MarkerSkip::Button);
    }
}
