//! **The Shield layout's player chrome over the live session** — what `plx_ui::shield_player`
//! draws, resolved from the playback, the metadata store and the hubs: the transport's title block,
//! its glyph slot and displayed playhead, the Skip pill's words, and the post-play page's cards.
//! Values in, paint out; what a key does is `screens::player`'s.
//!
//! The playhead the bar shows is the Standard playbar's own resolution
//! (`player_hud::display_pos_ns`: a loading seek's target, else the scrub preview, else the
//! published position), and the glyph slot reads the same four-state transport mark
//! (`player_hud::transport_mark`), dressed as Plex dresses it: what OK on the bar does (pause
//! while playing, play while paused), a jump arrow with the gathered seconds while the playhead
//! travels, the spinner while the pipeline is waited on with nothing to show.
use std::ffi::CString;
use std::sync::atomic::Ordering::Relaxed;

use plx_data::metadata::MetadataView;
use plx_ui::shield_player::{self, Card, Focus, Glyph, PostFocus, PostPlay, Transport};
use plx_ui::Painter;

use crate::player_hud::{Busy, TransportMark};

/// The transport's top-left block: the title, the meta line and the media tags.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TitleBlock {
    pub title: String,
    pub meta: String,
    pub tags: Vec<String>,
}

/// **The title block for the playing item**: an episode's own name over "S10 · E7 · Show · 19 min";
/// a film's title over "2026 · 2 hr 11 min" with HDR and 4K tags when the media has them; anything
/// else (an extra, a URL play) the route's own title over its context line.
pub fn title_block(ps: &plx_media::route::PlaybackSession, meta: MetadataView<'_>) -> TitleBlock {
    let dur_ms = plx_media::player::duration_ns() / 1_000_000;
    let route_text = |p: *const std::os::raw::c_char| {
        if p.is_null() {
            String::new()
        } else {
            // SAFETY: the route's HUD strings are NUL-terminated buffers the session owns for the
            // frame (`route::title_cptr`'s contract).
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    let Some(n) = meta.now_playing() else {
        return TitleBlock {
            title: route_text(plx_media::route::title_cptr(ps)),
            meta: route_text(plx_media::route::ctxline_cptr(ps)),
            tags: Vec::new(),
        };
    };
    let dur = if n.dur_ms > 0 { n.dur_ms } else { dur_ms };
    let runtime = (dur >= 60_000).then(|| plx_ui::fmt::dur_long(dur));
    let (title, facts): (String, Vec<String>) = if n.is_real_episode {
        let address = plx_ui::fmt::episode_address(n.season, n.index);
        (n.ep_title.clone(), [Some(address), Some(n.title.clone()), runtime].into_iter().flatten().filter(|s| !s.is_empty()).collect())
    } else {
        let year = (n.year > 0).then(|| n.year.to_string());
        (n.title.clone(), [year, runtime].into_iter().flatten().collect())
    };
    TitleBlock { title, meta: facts.join(" \u{b7} "), tags: media_tags(meta) }
}

/// The media's tags: HDR (an HDR transfer or Dolby Vision) and 4K, from the loaded record of the
/// playing item when it is the one on the detail store, else from the playing item's own facts.
pub fn media_tags(meta: MetadataView<'_>) -> Vec<String> {
    let Some(playing) = meta.playing() else { return Vec::new() };
    let loaded = meta.current().filter(|d| plx_plex::plex::same_item((d.sid, &d.rk), (playing.sid, &playing.rk)));
    let hdr = loaded.is_some_and(|d| d.hdr) || playing.dovi.present;
    let res = match loaded {
        Some(d) => plx_ui::fmt::resolution(&d.video_resolution, d.width, d.height),
        None => plx_ui::fmt::resolution("", playing.width, playing.height),
    };
    let mut tags = Vec::new();
    if hdr {
        tags.push(plx_platform::i18n::msg::widgets_player_tag_hdr().to_string());
    }
    if res.as_deref() == Some("4K") {
        tags.push("4K".to_string());
    }
    tags
}

/// PURE: the glyph slot for a transport mark — `travel_ns`/`pos_ns` are the displayed and the
/// published playhead, whose difference is the jump's gathered time.
pub fn glyph_for(mark: TransportMark, travel_ns: i64, pos_ns: i64) -> Glyph {
    let secs = ((travel_ns - pos_ns).abs() + 500_000_000) / 1_000_000_000;
    match mark {
        TransportMark::FastForward => Glyph::Jump { forward: true, secs },
        TransportMark::Rewind => Glyph::Jump { forward: false, secs },
        TransportMark::Working => Glyph::Working,
        TransportMark::Pause => Glyph::Play,
        TransportMark::Play | TransportMark::None => Glyph::Pause,
    }
}

/// **The transport over the live session**, focus as the player holds it.
#[allow(clippy::too_many_arguments)]
pub fn draw_transport(
    ps: &plx_media::route::PlaybackSession,
    busy: Busy,
    focus: Focus,
    has_chapters: bool,
    clock: Option<&std::ffi::CStr>,
    now: u32,
    measure: &dyn plx_machine::machine::Measure,
    meta: MetadataView<'_>,
) {
    let scrub = plx_media::player::TX.scrub_ns.load(Relaxed);
    let livepos = plx_media::player::playpos_ns();
    let shown = crate::player_hud::display_pos_ns(
        plx_media::player::loading(ps),
        plx_media::player::seek_display_ns(),
        scrub,
        livepos,
    );
    let paused = plx_media::player::TX.paused.load(Relaxed);
    let mark = crate::player_hud::transport_mark(paused, busy, scrub >= 0, shown, livepos, None);
    let block = title_block(ps, meta);
    let tags: Vec<&str> = block.tags.iter().map(String::as_str).collect();
    let labels: Vec<&'static std::ffi::CStr> = if has_chapters {
        vec![plx_platform::i18n::msg::widgets_player_info_c(), plx_platform::i18n::msg::widgets_player_chapters_c()]
    } else {
        vec![plx_platform::i18n::msg::widgets_player_info_c()]
    };
    let t = Transport {
        title: &block.title,
        meta: &block.meta,
        tags: &tags,
        clock,
        pos_ms: shown / 1_000_000,
        dur_ms: plx_media::player::duration_ns() / 1_000_000,
        glyph: glyph_for(mark, shown, livepos),
        focus,
        tabs: &labels,
        now,
    };
    shield_player::draw_transport(Painter::root(), &t, measure);
}

/// The Skip pill's rect for the offer `pr`.
pub fn skip_rect(pr: crate::skip_pill::Prompt, measure: &dyn plx_machine::machine::Measure) -> plx_ui::Rect {
    shield_player::skip_rect(measure.width_str(pr.label(), shield_player::SKIP_SIZE, false))
}

/// **The Skip pill** for the offer `pr`, on its own over the picture.
pub fn draw_skip(pr: crate::skip_pill::Prompt, measure: &dyn plx_machine::machine::Measure) {
    if let Ok(label) = CString::new(pr.label()) {
        shield_player::draw_skip(Painter::root(), skip_rect(pr, measure), &label);
    }
}

/// One card of the post-play page, owned.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PostCard {
    pub sid: u16,
    pub rk: String,
    pub thumb: String,
    pub title: String,
    pub caption: String,
}

impl PostCard {
    fn card(&self) -> Card<'_> {
        Card { sid: self.sid, thumb: &self.thumb, title: &self.title, caption: &self.caption }
    }
}

/// **The post-play page's content**: the ended episode, its successor (title, address, summary)
/// and the On Deck row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PostPlayModel {
    pub just_played: PostCard,
    pub up_next: PostCard,
    pub next_summary: String,
    pub deck: Vec<PostCard>,
}

/// How many On Deck cards the page carries.
pub const DECK_MAX: usize = 12;

/// The page for this session, or `None` when nothing is queued behind it.
pub fn post_play_model(
    ps: &plx_media::route::PlaybackSession,
    meta: MetadataView<'_>,
    hubs: Option<plx_data::pms::HubsView<'_>>,
) -> Option<PostPlayModel> {
    let u = plx_media::route::up_next(ps)?;
    let playing = meta.playing();
    let sid = playing.map_or_else(|| plx_media::route::item_sid(plx_media::route::cur_sid(ps)).raw(), |p| p.sid.raw());
    let (jp_title, jp_address, jp_thumb) = match meta.now_playing() {
        Some(n) => (n.ep_title.clone(), plx_ui::fmt::episode_address(n.season, n.index), n.thumb.clone()),
        None => (String::new(), String::new(), String::new()),
    };
    let up_next = PostCard {
        sid,
        rk: u.rk.clone(),
        thumb: u.thumb.clone(),
        title: u.ep_title.clone(),
        caption: plx_ui::fmt::episode_address(u.season, u.index),
    };
    let skip = |rk: &str| playing.is_some_and(|p| p.rk == rk) || rk == u.rk;
    let deck = hubs.map(|h| deck_cards(h, &skip)).unwrap_or_default();
    Some(PostPlayModel {
        just_played: PostCard { sid, rk: playing.map(|p| p.rk.clone()).unwrap_or_default(), thumb: jp_thumb, title: jp_title, caption: jp_address },
        up_next,
        next_summary: u.summary.clone(),
        deck,
    })
}

/// The Continue Watching hub's items as On Deck cards, leaving out what `skip` names (the ended
/// episode and its successor, already on the page).
pub fn deck_cards(hubs: plx_data::pms::HubsView<'_>, skip: &dyn Fn(&str) -> bool) -> Vec<PostCard> {
    let Some(hub) = (0..hubs.hub_count())
        .filter_map(|i| hubs.hub(i))
        .find(|h| matches!(h.identity, Some(plx_data::pms::HubIdentity::ContinueWatching)))
    else {
        return Vec::new();
    };
    hub.items
        .iter()
        .filter(|m| !m.rk.is_empty() && !skip(&m.rk))
        .take(DECK_MAX)
        .map(|m| {
            let episode = m.kind == 3;
            let runtime = (m.dur_ns >= 60_000_000_000).then(|| plx_ui::fmt::dur_short(m.dur_ns / 1_000_000));
            let caption: Vec<String> = if episode {
                [Some(plx_ui::fmt::episode_address(m.season_index.into(), m.ep_index.into())), Some(m.show_title.clone()), runtime]
                    .into_iter()
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                [(m.year > 0).then(|| m.year.to_string()), runtime].into_iter().flatten().collect()
            };
            let thumb = if episode && !m.still.is_empty() {
                m.still.clone()
            } else if !m.art.is_empty() {
                m.art.clone()
            } else {
                m.thumb.clone()
            };
            PostCard { sid: m.sid.raw(), rk: m.rk.clone(), thumb, title: m.title.clone(), caption: caption.join(" \u{b7} ") }
        })
        .collect()
}

/// **The post-play page**: `seconds` is the countdown's, while it runs.
pub fn draw_post_play(model: &PostPlayModel, seconds: Option<u32>, focus: PostFocus, deck_scroll: f32,
    measure: &dyn plx_machine::machine::Measure) {
    use plx_platform::i18n::msg;
    let heading = match seconds {
        Some(s) => msg::widgets_postplay_up_next_count(i64::from(s)),
        None => msg::widgets_postplay_up_next().to_string(),
    };
    let deck: Vec<Card<'_>> = model.deck.iter().map(PostCard::card).collect();
    let pp = PostPlay {
        just_played_heading: msg::widgets_postplay_just_played(),
        up_next_heading: &heading,
        deck_heading: msg::widgets_postplay_on_deck(),
        just_played: model.just_played.card(),
        up_next: model.up_next.card(),
        next_title: &model.up_next.title,
        next_address: &model.up_next.caption,
        next_summary: &model.next_summary,
        deck: &deck,
        deck_scroll,
        focus,
    };
    shield_player::draw_post_play(Painter::root(), &pp, measure);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// k02/k04/h03/k11: the slot shows what OK does (pause while playing, play while paused), a
    /// jump arrow with the gathered seconds while the playhead travels, the spinner while waited on.
    #[test]
    fn the_glyph_slot_shows_what_ok_does_and_how_far_a_jump_has_gathered() {
        const S: i64 = 1_000_000_000;
        assert_eq!(glyph_for(TransportMark::None, 5 * S, 5 * S), Glyph::Pause);
        assert_eq!(glyph_for(TransportMark::Play, 5 * S, 5 * S), Glyph::Pause);
        assert_eq!(glyph_for(TransportMark::Pause, 5 * S, 5 * S), Glyph::Play);
        assert_eq!(glyph_for(TransportMark::FastForward, 95 * S, 5 * S), Glyph::Jump { forward: true, secs: 90 });
        assert_eq!(glyph_for(TransportMark::Rewind, 5 * S, 15 * S), Glyph::Jump { forward: false, secs: 10 });
        assert_eq!(glyph_for(TransportMark::Working, 5 * S, 5 * S), Glyph::Working);
        // the travel is rounded to the whole second it reads closest to
        assert_eq!(glyph_for(TransportMark::FastForward, 9_600_000_000, 0), Glyph::Jump { forward: true, secs: 10 });
    }
}
