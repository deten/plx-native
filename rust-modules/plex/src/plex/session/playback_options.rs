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

/// Whether the server is asked to re-time a client-drawn external subtitle to the film's dialogue
/// (Plex Pass subtitle auto-sync, `autoAdjustSubtitle=1`, docs/pms-api.md "Subtitle auto-sync"),
/// one per profile like [`InstantDpadSeeking`]. The Subtitles menu is its only writer (no Settings
/// row, like the subtitle tone) and offers it only for a subtitle the server says it can sync.
/// Absence is `Off`: a viewer opts in, so nothing new is asked of a server until then, and a timing
/// correction tuned against the file as it is keeps applying to that same file.
#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleAutoSync {
    #[default]
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "on")]
    On,
}

impl SubtitleAutoSync {
    /// Every option, in index order.
    pub const LADDER: [SubtitleAutoSync; 2] = [SubtitleAutoSync::Off, SubtitleAutoSync::On];

    pub fn from_index(i: u8) -> SubtitleAutoSync {
        Self::LADDER.get(i as usize).copied().unwrap_or_default()
    }

    pub fn index(self) -> u8 {
        Self::LADDER.iter().position(|&m| m == self).unwrap_or(0) as u8
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn is_on(self) -> bool {
        self == SubtitleAutoSync::On
    }

    pub fn of(on: bool) -> SubtitleAutoSync {
        if on { SubtitleAutoSync::On } else { SubtitleAutoSync::Off }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plex::session::{effective, own_prefs, put_own, DolbyVisionModePref,
        InstantDpadSeekingPref, Preferences, ProfilePrefs, SubtitleAutoSyncPref};

    #[test]
    fn subtitle_auto_sync_is_off_by_default_and_scoped_per_profile() {
        for json in ["{}", r#"{"subtitle_auto_sync":"future"}"#, r#"{"subtitle_auto_sync":null}"#] {
            let prefs: Preferences = serde_json::from_str(json).unwrap();
            assert_eq!(prefs.subtitle_auto_sync, SubtitleAutoSync::Off);
            let encoded = serde_json::to_value(&prefs).unwrap();
            assert!(encoded.get("subtitle_auto_sync").is_none());
        }
        let prefs = Preferences::default();
        let mut records = Vec::new();
        assert!(put_own::<SubtitleAutoSyncPref>(&mut records, "alice", "account", SubtitleAutoSync::On));
        assert_eq!(effective::<SubtitleAutoSyncPref>(&prefs, own_prefs(&records, "alice", "account")), SubtitleAutoSync::On);
        for (user, account) in [("bob", "account"), ("alice", "other")] {
            assert_eq!(effective::<SubtitleAutoSyncPref>(&prefs, own_prefs(&records, user, account)), SubtitleAutoSync::Off);
        }
        let encoded = serde_json::to_string(&records[0].prefs).unwrap();
        assert!(encoded.contains(r#""subtitle_auto_sync":"on""#), "{encoded}");
        assert_eq!(serde_json::from_str::<ProfilePrefs>(&encoded).unwrap(), records[0].prefs);
        for i in 0..3 {
            assert_eq!(SubtitleAutoSync::from_index(SubtitleAutoSync::from_index(i).index()), SubtitleAutoSync::from_index(i));
        }
        assert!(SubtitleAutoSync::of(true).is_on() && !SubtitleAutoSync::of(false).is_on());
    }

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
