//! The per-show track memory as the resolve applies it: which track of a NEW episode stands in
//! for the one the viewer picked on an earlier episode, and when the memory stays out of the way.

use super::*;
use super::test_support::*;
use plx_plex::plex::session::{ShowTrackEntry, TrackHint};

fn episode(audio: Vec<plx_data::metadata::Stream>, subs: Vec<plx_data::metadata::Stream>) -> plx_data::metadata::PlayingItem {
    plx_data::metadata::PlayingItem {
        sid: ServerId::default(),
        rk: "201".into(),
        show_rk: "100".into(),
        audio,
        subs,
        video_fps: 23.976,
        width: 1920,
        height: 1080,
        bitrate: 8_000,
        has_voice_activity: false,
        dovi: plx_data::metadata::Dovi::NONE,
        markers: Vec::new(),
        chapters: Vec::new(),
        blur: None,
    }
}

fn memory(audio: Option<TrackHint>, subtitle: Option<TrackHint>) -> ResolveEnv {
    ResolveEnv {
        show_tracks: vec![ShowTrackEntry { machine_id: "m".into(), show_rk: "100".into(), audio, subtitle, ..Default::default() }],
        ..Default::default()
    }
}

fn lang(code: &str) -> TrackHint {
    TrackHint { lang: code.into(), ..Default::default() }
}

fn named(mut s: plx_data::metadata::Stream, title: &str) -> plx_data::metadata::Stream {
    s.title = title.into();
    s
}

fn sidecar(id: i64, lang: &str) -> plx_data::metadata::Stream {
    let mut s = sub(id, id, lang, true);
    s.key = format!("/library/streams/{id}");
    s
}

#[test]
fn a_remembered_language_picks_that_track_on_the_next_episode() {
    // The next episode has its own stream ids; the memory matches by language, not by id.
    let item = episode(vec![trk(11, "aac", "eng", true), trk(12, "aac", "jpn", false)], vec![]);
    let env = with_show_memory(&memory(Some(lang("jpn")), None), Some(&item)).expect("memory applies");
    assert_eq!(env.audio_sid, 12);
    assert_eq!(env.subtitle_override, None, "no subtitle memory, no subtitle decision");
}

#[test]
fn an_audio_description_never_stands_in_for_the_main_track() {
    let mut described = trk(13, "aac", "eng", false);
    described.ad = true;
    let item = episode(vec![described, trk(14, "ac3", "eng", true)], vec![]);
    assert_eq!(with_show_memory(&memory(Some(lang("eng")), None), Some(&item)).unwrap().audio_sid, 14);
    let main_only = episode(vec![trk(15, "aac", "eng", true)], vec![]);
    let remembered_description = TrackHint { ad: true, ..lang("eng") };
    assert!(with_show_memory(&memory(Some(remembered_description), None), Some(&main_only)).is_none(),
        "a remembered description track does not become the main audio");
}

#[test]
fn among_same_language_tracks_title_codec_and_channels_break_the_tie() {
    let item = episode(
        vec![named(trk(21, "aac", "eng", true), "Stereo"), named(trk(22, "eac3", "eng", false), "Surround")],
        vec![],
    );
    let hint = TrackHint { codec: "eac3".into(), title: "Surround".into(), ..lang("eng") };
    assert_eq!(with_show_memory(&memory(Some(hint), None), Some(&item)).unwrap().audio_sid, 22);
}

#[test]
fn subtitles_off_is_remembered_as_an_explicit_off() {
    let item = episode(vec![trk(11, "aac", "eng", true)], vec![sub(31, 2, "eng", false)]);
    let off = TrackHint { off: true, ..Default::default() };
    let env = with_show_memory(&memory(None, Some(off)), Some(&item)).unwrap();
    assert_eq!(env.subtitle_override, Some(0));
    assert_eq!(env.audio_sid, 0, "no audio memory, the usual audio choice");
}

#[test]
fn forced_and_full_subtitles_never_stand_in_for_each_other() {
    let mut forced = sub(41, 2, "eng", false);
    forced.forced = true;
    let full = sub(42, 3, "eng", false);
    let item = episode(vec![], vec![forced, full]);
    assert_eq!(with_show_memory(&memory(None, Some(lang("eng"))), Some(&item)).unwrap().subtitle_override, Some(42));
    let remembered_forced = TrackHint { forced: true, ..lang("eng") };
    assert_eq!(with_show_memory(&memory(None, Some(remembered_forced.clone())), Some(&item)).unwrap().subtitle_override, Some(41));
    let full_only = episode(vec![], vec![sub(43, 2, "eng", false)]);
    assert!(with_show_memory(&memory(None, Some(remembered_forced)), Some(&full_only)).is_none(),
        "forced-only subtitles are not replaced by full ones");
}

#[test]
fn sdh_and_an_external_file_are_preferred_when_they_match() {
    let mut sdh = sub(51, 2, "eng", false);
    sdh.sdh = true;
    let item = episode(vec![], vec![sub(52, 3, "eng", false), sdh, sidecar(53, "eng")]);
    let want_sdh = TrackHint { sdh: true, ..lang("eng") };
    assert_eq!(with_show_memory(&memory(None, Some(want_sdh)), Some(&item)).unwrap().subtitle_override, Some(51));
    let want_file = TrackHint { external: true, codec: "srt".into(), ..lang("eng") };
    assert_eq!(with_show_memory(&memory(None, Some(want_file)), Some(&item)).unwrap().subtitle_override, Some(53));
}

#[test]
fn nothing_in_that_language_leaves_the_usual_selection() {
    let item = episode(vec![trk(11, "aac", "eng", true)], vec![sub(31, 2, "eng", false)]);
    assert!(with_show_memory(&memory(Some(lang("fra")), Some(lang("fra"))), Some(&item)).is_none());
}

#[test]
fn a_carried_pick_a_film_a_preview_or_another_show_keeps_the_memory_out() {
    let item = episode(vec![trk(11, "aac", "eng", true), trk(12, "aac", "jpn", false)], vec![sub(31, 2, "eng", false)]);
    let base = memory(Some(lang("jpn")), Some(lang("eng")));

    let carried = ResolveEnv { audio_sid: 11, subtitle_override: Some(0), ..base.clone() };
    assert!(with_show_memory(&carried, Some(&item)).is_none(), "a retry's own selection wins");

    let film = plx_data::metadata::PlayingItem { show_rk: String::new(), ..item.clone() };
    assert!(with_show_memory(&base, Some(&film)).is_none());

    let preview = ResolveEnv { preview: true, ..base.clone() };
    assert!(with_show_memory(&preview, Some(&item)).is_none());

    let other_show = plx_data::metadata::PlayingItem { show_rk: "999".into(), ..item.clone() };
    assert!(with_show_memory(&base, Some(&other_show)).is_none());
}

#[test]
fn forced_direct_play_leaves_an_unplayable_remembered_audio_to_the_usual_choice() {
    let item = episode(vec![trk(11, "aac", "eng", true), trk(12, "truehd", "jpn", false)], vec![]);
    let forced = ResolveEnv { direct_play_mode: DirectPlayMode::Forced, ..memory(Some(lang("jpn")), None) };
    assert!(with_show_memory(&forced, Some(&item)).is_none(), "a refusal would stop playback; the memory steps aside");
    let auto = memory(Some(lang("jpn")), None);
    assert_eq!(with_show_memory(&auto, Some(&item)).unwrap().audio_sid, 12, "outside Forced the server converts it");
}
