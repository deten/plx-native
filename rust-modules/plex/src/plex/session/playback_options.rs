//! The per-profile choice for instant D-pad seeking.
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

/// How long without a press before a TV episode is stopped (or, nearly over, let finish) and Up
/// Next stops starting the next episode by itself — one per profile, like [`InstantDpadSeeking`].
/// Absence is `Off`, the playback every build before the preference made. A closed set, like
/// [`super::SkipInterval`], so a hand-edited file cannot ask for a limit of a few seconds. Movies
/// are never touched, and only an advance nobody pressed for is held back
/// (`app::playback::passout_during_playback` and `passout_holds`): a pressed Next Episode always
/// plays.
#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassoutProtection {
    #[default]
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "1h")]
    Hours1,
    #[serde(rename = "2h")]
    Hours2,
    #[serde(rename = "3h")]
    Hours3,
}

impl PassoutProtection {
    /// Every option, Off first and then shortest first — the order the picker lists them in.
    pub const LADDER: [PassoutProtection; 4] = [
        PassoutProtection::Off,
        PassoutProtection::Hours1,
        PassoutProtection::Hours2,
        PassoutProtection::Hours3,
    ];

    /// An in-memory index back to an option — out of range is the default, for the reason
    /// [`super::SubtitleTone::from_index`] gives: the ladder can grow or shrink.
    pub fn from_index(i: u8) -> PassoutProtection {
        Self::LADDER.get(i as usize).copied().unwrap_or_default()
    }

    pub fn index(self) -> u8 {
        Self::LADDER.iter().position(|&p| p == self).unwrap_or(0) as u8
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The limit in hours; `Off` is 0.
    pub fn hours(self) -> u32 {
        match self {
            PassoutProtection::Off => 0,
            PassoutProtection::Hours1 => 1,
            PassoutProtection::Hours2 => 2,
            PassoutProtection::Hours3 => 3,
        }
    }

    /// **Whether the limit is reached**, pure: `idle_ms` after the viewer's last press, an advance
    /// Up Next would make on its own is held back and a playing episode is stopped or let finish.
    /// `Off` never reaches one; a limit is reached from the moment the idle time reaches it.
    pub fn holds(self, idle_ms: u32) -> bool {
        self != PassoutProtection::Off && u64::from(idle_ms) >= u64::from(self.hours()) * 3_600_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plex::session::{effective, own_prefs, put_own, DolbyVisionModePref,
        InstantDpadSeekingPref, PassoutProtectionPref, Preferences, ProfilePrefs};

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

    /// Absent, null, an unknown spelling or a wrong type reads as Off, and Off writes no key; a
    /// profile's own pick is its own and round-trips.
    #[test]
    fn passout_protection_is_defaulted_and_scoped_per_profile() {
        for json in ["{}", r#"{"passout_protection":"4h"}"#, r#"{"passout_protection":null}"#, r#"{"passout_protection":2}"#] {
            let prefs: Preferences = serde_json::from_str(json).unwrap();
            assert_eq!(prefs.passout_protection, PassoutProtection::Off, "{json}");
            let encoded = serde_json::to_value(&prefs).unwrap();
            assert!(encoded.get("passout_protection").is_none());
        }
        let prefs = Preferences::default();
        for protection in PassoutProtection::LADDER.into_iter().filter(|p| !p.is_default()) {
            let mut records = Vec::new();
            assert!(put_own::<PassoutProtectionPref>(&mut records, "alice", "account", protection));
            assert_eq!(effective::<PassoutProtectionPref>(&prefs, own_prefs(&records, "alice", "account")), protection);
            for (user, account) in [("bob", "account"), ("alice", "other")] {
                assert_eq!(effective::<PassoutProtectionPref>(&prefs, own_prefs(&records, user, account)), PassoutProtection::Off);
            }
            let encoded = serde_json::to_string(&records[0].prefs).unwrap();
            assert!(encoded.contains("passout_protection"), "{encoded}");
            assert_eq!(serde_json::from_str::<ProfilePrefs>(&encoded).unwrap(), records[0].prefs);
        }
        for protection in PassoutProtection::LADDER {
            assert_eq!(PassoutProtection::from_index(protection.index()), protection);
        }
        assert_eq!(PassoutProtection::from_index(200), PassoutProtection::Off);
    }

    /// Off never holds an advance back, however long nobody pressed anything; a limit holds one
    /// from the moment the idle time reaches it, and not a millisecond before.
    #[test]
    fn passout_protection_holds_an_advance_once_the_idle_time_reaches_its_limit() {
        const H: u32 = 3_600_000;
        assert_eq!(PassoutProtection::LADDER.map(PassoutProtection::hours), [0, 1, 2, 3]);
        assert!(!PassoutProtection::Off.holds(u32::MAX), "Off never holds one");
        for (protection, hours) in [(PassoutProtection::Hours1, 1), (PassoutProtection::Hours2, 2), (PassoutProtection::Hours3, 3)] {
            assert!(!protection.holds(0), "{protection:?}: a press just now");
            assert!(!protection.holds(hours * H - 1), "{protection:?}: inside the limit");
            assert!(protection.holds(hours * H), "{protection:?}: at the limit");
            assert!(protection.holds(u32::MAX), "{protection:?}: long past it");
        }
    }
}

/// How a detected intro or credits segment is offered. Each kind has its own profile setting.
#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MarkerSkipMode {
    Disabled,
    #[default]
    Manual,
    Automatic,
}
impl MarkerSkipMode {
    pub const LADDER: [Self; 3] = [Self::Disabled, Self::Manual, Self::Automatic];
    pub fn from_index(i: u8) -> Self { Self::LADDER.get(i as usize).copied().unwrap_or_default() }
    pub fn index(self) -> u8 { Self::LADDER.iter().position(|&m| m == self).unwrap_or(1) as u8 }
    pub fn is_default(&self) -> bool { *self == Self::Manual }
}

#[cfg(test)]
mod marker_skip_tests {
    use super::MarkerSkipMode::*;
    use crate::plex::session::{effective, own_prefs, put_own, CreditsSkipPref, IntroSkipPref, Preferences};

    #[test]
    fn marker_skip_defaults_soft_parse_and_profile_isolation() {
        for value in [serde_json::json!({}), serde_json::json!({"intro_skip":null,"credits_skip":"future"})] {
            let prefs: Preferences = serde_json::from_value(value).unwrap();
            assert_eq!((prefs.intro_skip, prefs.credits_skip), (Manual, Manual));
            let encoded = serde_json::to_value(prefs).unwrap();
            assert!(encoded.get("intro_skip").is_none() && encoded.get("credits_skip").is_none());
        }
        let prefs = Preferences::default();
        let mut records = Vec::new();
        assert!(put_own::<IntroSkipPref>(&mut records, "alice", "account", Automatic));
        assert!(put_own::<CreditsSkipPref>(&mut records, "alice", "account", Disabled));
        let own = own_prefs(&records, "alice", "account");
        assert_eq!(effective::<IntroSkipPref>(&prefs, own), Automatic);
        assert_eq!(effective::<CreditsSkipPref>(&prefs, own), Disabled);
        for (user, account) in [("bob", "account"), ("alice", "other")] {
            let own = own_prefs(&records, user, account);
            assert_eq!(effective::<IntroSkipPref>(&prefs, own), Manual);
            assert_eq!(effective::<CreditsSkipPref>(&prefs, own), Manual);
        }
        let round = serde_json::to_value(&records).unwrap();
        assert_eq!(round[0]["prefs"]["intro_skip"], "automatic");
        assert_eq!(round[0]["prefs"]["credits_skip"], "disabled");
    }
}
