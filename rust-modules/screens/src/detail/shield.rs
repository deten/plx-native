//! **The Detail page in the Shield layout** (`plx_ui::layout::shield()`; SHIELD-LAYOUT-SPEC §4.2,
//! §4.3, §5.3–§5.5, §6.4, §7, §9.1).
//!
//! The same page — the same item, store requests, restore and reconciliation, element identities,
//! focus groups and press paths — dressed as the Plex app's detail pages on Android TV. What
//! differs is how it stands, and where a control's press goes when the Standard row has no such
//! control:
//!
//! - **The header** (`plx_ui::shield_detail`): the logo or title at 128,128, the facts row, the
//!   app's rating marks, a three-line summary and a "Directed by" line, over the item's UltraBlur
//!   ground with its backdrop at the right (`shield::HeroGround`), which fades as the page scrolls.
//!   No trailer autoplays behind it: the Shield page has no place for one, so its Trailer circle
//!   plays the trailer as a feature instead (`hero::HeroCtl::Trailer`).
//! - **The action row** (`hero::shield_ctls`): the primary pill — Play, `Resume • 36 min left`,
//!   `Watch S2 · E1` — then 64 circles: play from the start, the Trailer, the Watchlist and watched
//!   toggles, Shuffle on a show, and "⋮", which presents the More Actions panel
//!   (`screens::detail_more`) with the rest of the page's actions: *Track information*, *Also
//!   available*, going to the show, *About*, and the toggles again. The pill shows its label only
//!   while the row holds focus ([`Shield::pill`]). The media chips at the row's right end (the
//!   version, the audio track, the subtitles) are its last members and open *Track information*.
//! - **Sections**: Cast & Crew as 240 circles named under each, Extras as captioned 512x288
//!   stills, the collection and Related as captioned posters — the same shelves, restyled
//!   (`cards::Which::shield_style`), each a fixed-left row. The About footer is not drawn; its two
//!   sheets are rows of the More panel. A focused section's heading is brought to y 394.
//! - **A show** carries tabs at the top — *Show*, then one per season — and its Show tab lists
//!   the seasons as captioned posters (the season elements, [`Located::Season`]). OK on a season
//!   tab, or on its poster, opens the **season page** ([`Shield::season_view`]): the season's
//!   episodes as a strip of 512x288 stills at y 570 (the episode elements, stills only), the
//!   header describing the episode focus last rested on ([`Shield::subject`]), and the action row
//!   under the strip acting on THAT episode — play it, from its start, mark it, shuffle the season
//!   — through the episode paths the Standard filmstrip and its menu take. A season tab loads its
//!   season through the Standard tabs' own dwell (`pending_season`), only on OK.
//! - **LEFT** off the first element of any row is the side panel's door (`Rig::edge_surface`):
//!   every group answers `Stop` at its left edge, as in Standard.
//! - **Live.** [`DetailScreen::sync_layout`] latches the layout at the top of every event: a
//!   switch re-dresses the mounted page in place (shelf styles, the plan, the scroll on the focused
//!   block, the Show tab) and the engine's every-frame `reconcile` moves a focus the new layout
//!   cannot hold (a chip, a tab, the About footer, an episode's text block) onto a control it can.

use std::ffi::CString;

use plx_data::metadata::{Detail, Episode};
use plx_data::pms::PmsMovie;
use plx_ui::icons::Icon;
use plx_ui::shield as geo;
use plx_ui::shield_browse as sb;
use plx_ui::shield_detail as sd;

use super::*;
use crate::registry::{DetailAction, DetailActions, ItemAction, ItemMenuReq};

/// The show page's tab group: *Show*, then one tab per season.
pub(super) const TABS_GROUP: GroupId = GroupId(9);
/// The tabs' elements — *Show* at the start of the range, season `i`'s tab at `1 + i`. A SLOT
/// range like the hero's, keyed by position: a tab is a place in the show's season list, and the
/// season it names is the poster's interned identity when it needs one.
pub(super) const TAB_ELEM_RANGE_START: u32 = collection::COLLECTION_ELEM_RANGE_END;
pub(super) const MAX_TABS: usize = 65;
pub(super) const TAB_ELEM_RANGE_END: u32 = TAB_ELEM_RANGE_START + MAX_TABS as u32;
const _: () = assert!(TAB_ELEM_RANGE_END <= FIRST_ITEM_ELEM);

/// How far the page scrolls before the backdrop has gone with the header (e03: gone at 282).
const ART_FADE_SCROLL: f32 = 282.0;

pub(super) fn tab_elem(i: usize) -> Option<u32> {
    (i < MAX_TABS).then(|| TAB_ELEM_RANGE_START + i as u32)
}

pub(super) fn tab_locate(elem: u32) -> Option<usize> {
    (TAB_ELEM_RANGE_START..TAB_ELEM_RANGE_END).contains(&elem).then(|| (elem - TAB_ELEM_RANGE_START) as usize)
}

/// **The Shield half of the page's state.** `on`, `season_view` and `subject` are logical (they
/// decide what is focusable and what a press does) and are hashed while the layout is on; the
/// rest is paint.
pub(super) struct Shield {
    /// `plx_ui::layout::shield()`, latched at the head of every event (`sync_layout`).
    pub(super) on: bool,
    /// A show page's season tab is up: the season page (the Show tab otherwise).
    pub(super) season_view: bool,
    /// The episode the season page describes and its action row acts on: the one focus last
    /// rested on, by rating key. `None` (or a key the loaded season no longer holds) is the
    /// default — the on-deck episode when it is in this season, else the season's first.
    pub(super) subject: Option<String>,
    /// The primary pill's openness: 1 shows its label (the row holds focus), 0 is a circle.
    pub(super) pill: Spring,
    /// The item's UltraBlur ground and its backdrop.
    pub(super) ground: geo::HeroGround,
    /// The Show tab's season posters, and the catalog rows they are drawn from.
    pub(super) seasons: Shelf,
    pub(super) season_rows: Vec<PmsMovie>,
}

impl Shield {
    pub(super) fn new(entry: EntryId) -> Self {
        Self {
            on: plx_ui::layout::shield(),
            season_view: false,
            subject: None,
            pill: Spring::at(1.0),
            ground: geo::HeroGround::new(),
            seasons: Shelf::new(entry, cards::Which::Seasons.shield_style()),
            season_rows: Vec::new(),
        }
    }

    /// The hashed half (`LogicalState::write` appends it while the layout is on).
    pub(super) fn write(&self, w: &mut Canon) {
        w.bool(self.season_view).str(self.subject.as_deref().unwrap_or(""));
    }
}

/// A season of the show as the catalog row its poster is drawn from: the season's own poster
/// (by its key), its count and its watch state.
pub(super) fn season_row(d: &Detail, s: &plx_data::metadata::Season) -> PmsMovie {
    PmsMovie {
        sid: d.sid,
        rk: s.rk.clone(),
        title: s.title.clone(),
        kind: 2,
        thumb: if s.rk.is_empty() { String::new() } else { format!("/library/metadata/{}/thumb", s.rk) },
        show_rk: d.rk.clone(),
        show_title: d.title.clone(),
        season_index: s.index as _,
        child_count: s.leaf_count,
        watched: s.watched(),
        unwatched: s.viewed_leaf_count == 0,
        ..Default::default()
    }
}

/// **The Shield document's vertical plan**, measured once per landing with the rest of the page's
/// layout (`LayoutCache::shield`): the header's lines, the action row, the chips, and each
/// section's heading top and block in document space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Plan {
    pub(super) header: sd::HeaderPlan,
    pub(super) buttons_y: f32,
    pub(super) chips_y: f32,
    /// The season page: the strip stands at `STRIP_Y` and the buttons under it.
    pub(super) strip: bool,
    pub(super) top: [f32; section::SLOTS],
    pub(super) block: [f32; section::SLOTS],
    pub(super) seen: u8,
    pub(super) end: f32,
}

impl Plan {
    /// Section `section`'s heading top, when the page shows it.
    pub(super) fn top(&self, section: i32) -> Option<f32> {
        let si = section as usize;
        ((1..section::SLOTS).contains(&si) && self.seen & (1 << si) != 0).then(|| self.top[si])
    }
}

/// The words the header says about its subject.
struct HeaderText {
    sid: u16,
    logo_rk: String,
    title: String,
    subtitle: String,
    facts: Vec<String>,
    rating: String,
    summary: String,
    director: String,
}

impl HeaderText {
    fn view(&self) -> geo::HeroFacts<'_> {
        geo::HeroFacts {
            sid: self.sid,
            logo_rk: &self.logo_rk,
            title: &self.title,
            subtitle: &self.subtitle,
            facts: &self.facts,
            rating: &self.rating,
            summary: &self.summary,
            cast: "",
        }
    }
}

/// The shelf-section heading words, by section.
fn heading_of(section: i32, d: &Detail) -> Option<CString> {
    match section {
        1 => CString::new(plx_platform::i18n::msg::browse_detail_seasons_heading(d.seasons.len() as i64)).ok(),
        3 => Some(plx_platform::i18n::msg::browse_detail_related_c().to_owned()),
        4 => Some(plx_platform::i18n::msg::browse_detail_cast_c().to_owned()),
        6 => Some(plx_platform::i18n::msg::browse_detail_extras_c().to_owned()),
        _ => None,
    }
}

/// Which shelf a section is.
fn which_of(section: i32) -> Option<cards::Which> {
    Some(match section {
        1 => cards::Which::Seasons,
        3 => cards::Which::Related,
        4 => cards::Which::Cast,
        6 => cards::Which::Extras,
        7 => cards::Which::Collection,
        _ => return None,
    })
}

impl DetailScreen {
    // ---- the latch ------------------------------------------------------------------------------

    /// **Latch the live layout for this event.** A switch re-dresses the mounted page in place: the
    /// shelves take the layout's style keeping their place, the Show tab comes back up, the plan is
    /// re-measured and the scroll jumps to where the focused block settles. A focus the new layout
    /// cannot hold is the engine's every-frame `reconcile`'s to move. A trailer playing behind the
    /// Standard hero stops: the Shield page has nowhere to show one. Nothing animates across it.
    pub(super) fn sync_layout<H: ContentLike + crate::registry::MetadataLike>(&mut self, cx: &Cx<'_, H>,
        fx: &mut Effects<'_, H>) {
        let on = plx_ui::layout::shield();
        if on == self.shield.on {
            return;
        }
        self.shield.on = on;
        self.shield.season_view = false;
        self.shield.subject = None;
        self.shield.ground.reset();
        self.layout.set(None);
        let meta = H::metadata(cx);
        let d = self.detail(meta);
        let (related, collection, extras, cast) = d.map_or((0, 0, 0, 0), |d| {
            (d.related.len().min(512), collection::len(d), extras::len(d), d.credits_len().min(512))
        });
        self.related.restyle(cards::Which::Related.style_for(on), related);
        self.collection.restyle(cards::Which::Collection.style_for(on), collection);
        self.extras.restyle(cards::Which::Extras.style_for(on), extras);
        self.cast.restyle(cards::Which::Cast.style_for(on), cast);
        if on && (self.preview_promoted || plx_media::player::preview::view().picture) {
            self.preview_promoted = false;
            self.trailer_ctl.dismiss();
            self.preview_dwell = 0.0;
            self.content(fx, ContentReq::PreviewStop);
        }
        self.reveal_focus(cx.focus.current, cx.measure, meta);
        self.scroll.jump(self.scroll_target);
        fx.invalidate(Provenance::Nav);
    }

    // ---- the plan -------------------------------------------------------------------------------

    /// The sections the Shield page shows, in order: the Show tab's seasons, then Cast & Crew,
    /// Extras, the collection and Related. (The About footer's sheets are More panel rows.)
    pub(super) fn shield_sections(&self, d: &Detail) -> Vec<i32> {
        let mut out = Vec::with_capacity(5);
        if d.is_show && !d.seasons.is_empty() && !self.shield.season_view {
            out.push(section::SectionId::Season.raw());
        }
        if d.credits_len() > 0 {
            out.push(section::SectionId::Cast.raw());
        }
        if !d.extras.is_empty() {
            out.push(section::SectionId::Extras.raw());
        }
        if collection::len(d) > 0 {
            out.push(section::SectionId::Collections.raw());
        }
        if !d.related.is_empty() {
            out.push(section::SectionId::Related.raw());
        }
        out
    }

    /// Is the page standing on its season tab? (A show's, with seasons to show.)
    pub(super) fn season_page(&self, d: &Detail) -> bool {
        self.shield.season_view && d.is_show && !d.seasons.is_empty()
    }

    /// Section `section`'s block height: its heading and cards, and what stands under them.
    fn shield_block(section: i32) -> f32 {
        match section {
            4 => sd::section_block(sd::CAST_D, sd::CAST_BLOCK - sd::CAST_D),
            6 => sd::section_block(sd::EP_H, sd::CAPTION_BAND),
            _ => sd::section_block(geo::CARD_H, sd::CAPTION_BAND),
        }
    }

    /// **Measure the plan** for `d` (`build_layout`, so once per landing and per layout switch).
    pub(super) fn shield_plan(&self, d: &Detail, measure: &dyn plx_machine::machine::Measure) -> Plan {
        let strip = self.season_page(d);
        let director = !d.is_show && !d.directors.is_empty();
        let header = sd::header_plan(!d.ratings.is_empty(), sd::summary_lines(&d.summary, measure), director);
        let buttons_y = if strip { sd::STRIP_ACTION_Y } else { header.buttons_y };
        let chips_y = if strip { sd::STRIP_CHIP_Y } else { buttons_y + sd::CHIP_DY };
        let mut y = if strip { sd::STRIP_SECTIONS_Y } else { buttons_y + sd::BUTTON_H + sd::SECTION_GAP };
        let mut top = [0.0; section::SLOTS];
        let mut block = [0.0; section::SLOTS];
        let mut seen = 0u8;
        for sec in self.shield_sections(d) {
            let si = sec as usize;
            top[si] = y;
            block[si] = Self::shield_block(sec);
            seen |= 1 << si;
            y += block[si] + sd::SECTION_GAP;
        }
        Plan { header, buttons_y, chips_y, strip, top, block, seen, end: y }
    }

    /// The plan for the published item (`None` before it lands).
    pub(super) fn plan(&self, meta: plx_data::metadata::MetadataView<'_>, measure: &dyn plx_machine::machine::Measure)
        -> Option<Plan> {
        let d = self.detail(meta)?;
        self.ensure_layout(d, measure).shield
    }

    // ---- the subject ----------------------------------------------------------------------------

    /// The season page's subject episode: the one focus last rested on, else the on-deck episode
    /// when it is in this season, else the season's first in progress, else its first unwatched,
    /// else its first.
    pub(super) fn subject_index(&self, d: &Detail) -> Option<usize> {
        if d.episodes.is_empty() {
            return None;
        }
        let by_rk = |rk: &str| d.episodes.iter().position(|e| e.rk == rk);
        self.shield.subject.as_deref().and_then(by_rk)
            .or_else(|| d.on_deck.as_ref().and_then(|e| by_rk(&e.rk)))
            .or_else(|| d.episodes.iter().position(|e| episodes::watch_state(e) == PosterMark::InProgress))
            .or_else(|| d.episodes.iter().position(|e| !e.watched))
            .or(Some(0))
    }

    pub(super) fn subject_episode<'a>(&self, d: &'a Detail) -> Option<&'a Episode> {
        self.subject_index(d).and_then(|i| d.episodes.get(i))
    }

    /// The row's control set on the Shield page (`hero_set`'s answer while the layout is on).
    pub(super) fn shield_hero_set(&self, meta: plx_data::metadata::MetadataView<'_>) -> hero::HeroSet {
        let base = hero::HeroSet { shield: true, watchlist: self.watchlisted, ..Default::default() };
        let Some(d) = self.detail(meta) else { return base };
        if self.season_page(d) {
            let ep = self.subject_episode(d);
            return hero::HeroSet {
                restart: ep.is_some_and(|e| plx_data::metadata::resume_ns(e.resume_ms, e.dur_ms) > 0),
                mark: ep.map_or(PosterMark::None, episodes::watch_state),
                shuffle: true,
                ..base
            };
        }
        hero::HeroSet {
            restart: hero::has_restart(hero::hero_resume_ns(d)),
            trailer: hero::trailer_play(d).is_some(),
            alt: self.alt_available(meta),
            mark: hero::hero_mark(d),
            shuffle: d.is_show,
            chips: self.chip_bits(d, meta),
            ..base
        }
    }

    // ---- the header -----------------------------------------------------------------------------

    fn header_text(&self, d: &Detail) -> HeaderText {
        let runtime = |ms: i64| (ms >= 60_000).then(|| plx_ui::fmt::dur_short(ms));
        let genres = (!d.genres.is_empty()).then(|| d.genres.iter().take(2).cloned().collect::<Vec<_>>().join(", "));
        let director = (!d.is_show && !d.directors.is_empty())
            .then(|| format!("{} {}", plx_platform::i18n::msg::browse_detail_directed_by(), d.directors.join(", ")))
            .unwrap_or_default();
        let mut h = HeaderText {
            sid: d.sid.raw(),
            logo_rk: d.rk.clone(),
            title: d.title.clone(),
            subtitle: String::new(),
            facts: Vec::new(),
            rating: d.rating.clone(),
            summary: d.summary.clone(),
            director,
        };
        if self.season_page(d) {
            match self.subject_episode(d) {
                Some(e) => {
                    h.subtitle = if e.title.is_empty() { plx_ui::fmt::episode_ordinal(e.season, e.index) } else { e.title.clone() };
                    h.facts = [Some(plx_ui::fmt::episode_address(e.season, e.index)),
                        Some(plx_ui::fmt::pretty_date(&e.aired, 0)), runtime(e.dur_ms)].into_iter().flatten().collect();
                    h.rating = e.rating.clone();
                    h.summary = e.summary.clone();
                }
                None => {
                    let season = d.seasons.get(d.cur_season);
                    h.subtitle = season.map(|s| s.title.clone()).unwrap_or_default();
                    h.facts = season.filter(|s| s.leaf_count > 0)
                        .map(|s| plx_platform::i18n::msg::browse_detail_episodes(s.leaf_count)).into_iter().collect();
                    h.summary.clear();
                }
            }
            h.director.clear();
        } else if d.kind == "episode" {
            if !d.show_rk.is_empty() {
                h.logo_rk = d.show_rk.clone();
            }
            if !d.show_title.is_empty() {
                h.title = d.show_title.clone();
            }
            h.subtitle = d.title.clone();
            h.facts = [(d.season > 0 && d.index > 0).then(|| plx_ui::fmt::episode_address(d.season, d.index)),
                Some(plx_ui::fmt::pretty_date(&d.aired, d.year)), runtime(d.dur_ms)].into_iter().flatten().collect();
        } else {
            h.facts = [(d.year > 0).then(|| d.year.to_string()), runtime(d.dur_ms), genres].into_iter().flatten().collect();
        }
        h.facts.retain(|f| !f.is_empty());
        h
    }

    /// The primary pill's words: resuming with the time left, the next episode by its address on a
    /// show, else Play.
    pub(super) fn pill_label(&self, d: &Detail) -> String {
        use plx_platform::i18n::msg;
        let (resume_ms, dur_ms, episode) = if self.season_page(d) {
            self.subject_episode(d).map_or((0, 0, None), |e| (e.resume_ms, e.dur_ms, Some(e)))
        } else if d.is_show {
            hero::hero_episode(d).map_or((0, 0, None), |e| (e.resume_ms, e.dur_ms, Some(e)))
        } else {
            (d.resume_ms, d.dur_ms, None)
        };
        if plx_data::metadata::resume_ns(resume_ms, dur_ms) > 0 {
            return msg::browse_detail_resume_left(&plx_ui::fmt::time_left(dur_ms - resume_ms));
        }
        match episode {
            Some(e) if e.season > 0 && e.index > 0 => msg::browse_detail_watch_episode(&plx_ui::fmt::episode_address(e.season, e.index)),
            _ => msg::browse_detail_play().to_string(),
        }
    }

    /// The media chips the item's file has: bit 0 the version, 1 the audio track, 2 the subtitles.
    fn chip_bits(&self, d: &Detail, meta: plx_data::metadata::MetadataView<'_>) -> u8 {
        if d.is_show || !self.tracks_available(meta) {
            return 0;
        }
        let video = plx_ui::fmt::resolution(&d.video_resolution, d.width, d.height).is_some();
        u8::from(video) | (u8::from(!d.audio.is_empty()) << 1) | (1 << 2)
    }

    /// A chip's glyph and words: the version's resolution and codec, the chosen audio track's
    /// language and codec, the chosen subtitle track (or Off).
    fn chip_label(ctl: hero::HeroCtl, d: &Detail) -> (Option<Icon>, String) {
        let pick = |list: &[plx_data::metadata::Stream]| {
            list.iter().find(|s| s.selected).or_else(|| list.iter().find(|s| s.default)).cloned()
        };
        let track = |s: &plx_data::metadata::Stream, extra: &str| {
            let lang = if s.lang.is_empty() { s.title.as_str() } else { s.lang.as_str() };
            let codec = s.codec.to_uppercase();
            let spec = [codec.as_str(), extra].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" ");
            match (lang.is_empty(), spec.is_empty()) {
                (true, _) => spec,
                (false, true) => lang.to_string(),
                (false, false) => format!("{lang} ({spec})"),
            }
        };
        match ctl {
            hero::HeroCtl::ChipVideo => {
                let res = plx_ui::fmt::resolution(&d.video_resolution, d.width, d.height).unwrap_or_default();
                let codec = d.vcodec.to_uppercase();
                (None, if codec.is_empty() { res } else { format!("{res} ({codec})") })
            }
            hero::HeroCtl::ChipAudio => {
                let s = pick(&d.audio).or_else(|| d.audio.first().cloned());
                let layout = s.as_ref().map(|s| s.layout.split('(').next().unwrap_or("").to_string()).unwrap_or_default();
                (Some(Icon::Audio), s.map(|s| track(&s, &layout)).unwrap_or_default())
            }
            _ => {
                let label = pick(&d.subs).filter(|s| s.selected).map(|s| track(&s, ""))
                    .unwrap_or_else(|| plx_platform::i18n::msg::browse_detail_subtitles_off().to_string());
                (Some(Icon::Cc), label)
            }
        }
    }

    // ---- geometry -------------------------------------------------------------------------------

    /// Does focus stand on the action row?
    fn row_focused(&self, focus: Option<FocusKey<u32>>) -> bool {
        focus.is_some_and(|k| k.entry == self.entry && hero::HeroCtl::of_elem(k.elem).is_some())
    }

    /// **The action row's controls where they stand** in DOCUMENT space (unscrolled): the pill
    /// open as it is drawn (`Drawn`) or as it will settle, then the circles, then the chips.
    pub(super) fn shield_ctl_rects<H: ContentLike + crate::registry::MetadataLike>(&self, cx: &Cx<'_, H>, at: At)
        -> Vec<(hero::HeroCtl, Rect)> {
        let meta = H::metadata(cx);
        let measure = cx.measure;
        let set = self.hero_set(meta);
        let (ctls, n) = hero::hero_ctls(set);
        let d = self.detail(meta);
        let (label, plan) = match d {
            Some(d) => (self.pill_label(d), self.plan(meta, measure)),
            None => (plx_platform::i18n::msg::browse_detail_play().to_string(), None),
        };
        let buttons_y = plan.map_or(sd::header_plan(false, 0, false).buttons_y, |p| p.buttons_y);
        let label_w = measure.width_str(&label, sd::PILL_SIZE, false);
        let open = match at {
            At::Drawn => self.shield.pill.pos,
            At::SpringTarget => f32::from(self.row_focused(cx.focus.current)),
        };
        let rounds = ctls[..n].iter().filter(|c| **c != hero::HeroCtl::Play && !c.is_chip()).count();
        let row = sd::action_row(buttons_y, label_w, open, rounds);
        let chips: Vec<hero::HeroCtl> = ctls[..n].iter().copied().filter(|c| c.is_chip()).collect();
        let chip_rects = match (d, plan) {
            (Some(d), Some(plan)) => {
                let widths: Vec<f32> = chips.iter().map(|c| {
                    let (glyph, text) = Self::chip_label(*c, d);
                    sd::chip_w(measure.width_str(&text, sd::CHIP_SIZE, false), glyph.is_some())
                }).collect();
                sd::chip_rects(&widths, plan.chips_y)
            }
            _ => Vec::new(),
        };
        let (mut round, mut chip) = (1, 0);
        ctls[..n].iter().filter_map(|&c| {
            let r = if c == hero::HeroCtl::Play {
                row.first().copied()
            } else if c.is_chip() {
                chip += 1;
                chip_rects.get(chip - 1).copied()
            } else {
                round += 1;
                row.get(round - 1).copied()
            };
            r.map(|r| (c, r))
        }).collect()
    }

    /// The show page's tab words: *Show*, then each season's own title.
    fn tab_labels(d: &Detail) -> Vec<CString> {
        std::iter::once(plx_platform::i18n::msg::browse_detail_tab_show().to_string())
            .chain(d.seasons.iter().take(MAX_TABS - 1).map(|s| s.title.clone()))
            .map(|t| CString::new(t).unwrap_or_default())
            .collect()
    }

    /// The tabs' pills, unscrolled along the row, carried up by the page's `scroll`.
    fn tab_pills(d: &Detail, measure: &dyn plx_machine::machine::Measure, scroll: f32) -> Vec<Rect> {
        let widths: Vec<f32> = Self::tab_labels(d).iter().map(|l| measure.width(l, sb::TAB_SIZE, false)).collect();
        sb::tab_pills(&widths, -scroll)
    }

    /// Does the page carry tabs? (A show with seasons.)
    fn has_tabs(d: &Detail) -> bool {
        d.is_show && !d.seasons.is_empty()
    }

    /// The tab that stands selected: the season page's season (the one loading, while it does),
    /// else *Show*.
    fn selected_tab(&self, d: &Detail) -> usize {
        if self.season_page(d) { 1 + self.pending_season.unwrap_or(d.cur_season) } else { 0 }
    }

    /// The tab row's scroll that keeps tab `focus` in its track.
    fn tab_target(&self, d: &Detail, measure: &dyn plx_machine::machine::Measure, focus: usize) -> f32 {
        sd::tab_scroll(&Self::tab_pills(d, measure, 0.0), focus, self.tab_scroll.pos)
    }

    /// Section `section`'s shelf frame: its cards' top on screen at `vertical` scroll.
    fn shelf_frame(plan: &Plan, section: i32, vertical: f32) -> Option<SectionFrame> {
        plan.top(section).map(|top| SectionFrame { y: top + sd::HEADING_DY - vertical, clip: Rect::FULL })
    }

    /// The season page's strip scroll that brings episode `i` of `n` to rest.
    fn strip_target(i: usize, n: usize) -> f32 {
        sd::strip_scroll(i, n)
    }

    // ---- Focusable ------------------------------------------------------------------------------

    /// The Shield page's groups: the tabs, the action row, the strip or the season posters, and
    /// the shelves, each LEFT edge a `Stop` (the side panel's door).
    pub(super) fn shield_groups<H: ContentLike + crate::registry::MetadataLike>(&self, cx: &Cx<'_, H>,
        out: &mut Vec<GroupSpec>) {
        let meta = H::metadata(cx);
        let measure = cx.measure;
        let set = self.hero_set(meta);
        let (_, hero_n) = hero::hero_ctls(set);
        let rects = self.shield_ctl_rects(cx, At::SpringTarget);
        let row_y = rects.first().map_or(sd::header_plan(false, 0, false).buttons_y, |(_, r)| r.y);
        out.push(GroupSpec {
            id: hero::HERO_GROUP,
            kind: GroupKind::Row { wrap: false },
            seat: Seat::First,
            reachable: AxisMask::BOTH,
            edge: [EdgeRule::Geometric, EdgeRule::Geometric, EdgeRule::Stop, EdgeRule::Stop],
            extent: Rect::new(sd::X, row_y - self.scroll_target, sd::RIGHT - sd::X, sd::BUTTON_H),
            len: hero_n,
            elem: ElemKind::Control,
        });
        let Some(d) = self.detail(meta) else { return };
        let Some(plan) = self.plan(meta, measure) else { return };
        if Self::has_tabs(d) {
            let pills = Self::tab_pills(d, measure, self.scroll_target);
            if let Some(track) = sd::tab_track(&pills) {
                out.push(GroupSpec {
                    id: TABS_GROUP,
                    kind: GroupKind::Row { wrap: false },
                    seat: Seat::Nearest,
                    reachable: AxisMask::BOTH,
                    edge: [EdgeRule::Stop, EdgeRule::Geometric, EdgeRule::Stop, EdgeRule::Stop],
                    extent: track,
                    len: pills.len(),
                    elem: ElemKind::Control,
                });
            }
        }
        if plan.strip && !d.episodes.is_empty() {
            out.push(GroupSpec {
                id: episodes::EPISODES_GROUP,
                kind: GroupKind::Row { wrap: false },
                seat: Seat::Remembered,
                reachable: AxisMask::BOTH,
                edge: [EdgeRule::Geometric, EdgeRule::Geometric, EdgeRule::Stop, EdgeRule::Stop],
                extent: Rect::new(sd::X, sd::STRIP_Y - self.scroll_target, sd::RIGHT - sd::X, sd::EP_H),
                len: d.episodes.len().min(episodes::MAX_ITEMS),
                elem: ElemKind::Card,
            });
        }
        for sec in self.shield_sections(d) {
            let Some(top) = plan.top(sec) else { continue };
            let top = top - self.scroll_target;
            let cards = Rect::new(sd::X, top + sd::HEADING_DY, sd::RIGHT - sd::X, plan.block[sec as usize] - sd::HEADING_DY);
            let (id, len, seat) = match sec {
                1 => (season::SEASON_GROUP, d.seasons.len().min(64), Seat::RememberedFirst),
                3 => (related::RELATED_GROUP, d.related.len().min(512), Seat::RememberedFirst),
                4 => (cast::CAST_GROUP, d.credits_len().min(512), Seat::RememberedFirst),
                6 => (extras::EXTRAS_GROUP, extras::len(d), Seat::RememberedFirst),
                7 => {
                    if let Some(c) = d.collection.as_ref() {
                        out.push(plx_ui::linked_heading::group_spec(collection::HEADING_GROUP,
                            Self::collection_heading_rect(c, top, false, measure)));
                    }
                    (collection::COLLECTION_GROUP, collection::len(d), plx_ui::linked_heading::shelf_seat(
                        &cx.focus, collection::HEADING_ELEM, collection::COLLECTION_GROUP, Seat::RememberedFirst))
                }
                _ => continue,
            };
            out.push(GroupSpec {
                id,
                kind: GroupKind::Row { wrap: false },
                seat,
                reachable: AxisMask::BOTH,
                edge: [EdgeRule::Geometric, EdgeRule::Geometric, EdgeRule::Stop, EdgeRule::Stop],
                extent: cards,
                len,
                elem: ElemKind::Card,
            });
        }
    }

    /// The collection's linked heading's face, its box at `top` on screen.
    fn collection_heading_rect(c: &plx_data::metadata::CollectionShelf, top: f32, focused: bool,
        measure: &dyn plx_machine::machine::Measure) -> Rect {
        let h = collection::heading(c);
        let m = h.measure(measure);
        h.face_rect(sd::X, sd::heading_text_y(top), f32::from(focused), &m)
    }

    /// Where `located` stands on the Shield page (`Focusable::place` while the layout is on).
    pub(super) fn shield_place<H: ContentLike + crate::registry::MetadataLike>(&self, key: &u32, located: Located,
        cx: &Cx<'_, H>, at: At) -> Option<Placed> {
        let meta = H::metadata(cx);
        let measure = cx.measure;
        let vertical = if at == At::Drawn { self.scroll.pos } else { self.scroll_target };
        let placed = |rect: Rect, rest: Rect, index: usize| Some(Placed { rect, rest_rect: rest, clip: Rect::FULL, index: Some(index as u32) });
        if let Located::Hero(ctl) = located {
            let rects = self.shield_ctl_rects(cx, at);
            let i = rects.iter().position(|(c, _)| *c == ctl)?;
            let r = rects[i].1;
            let r = Rect::new(r.x, r.y - vertical, r.w, r.h);
            return placed(r, r, i);
        }
        let d = self.detail(meta)?;
        let plan = self.plan(meta, measure)?;
        match located {
            Located::Tab(i) => {
                let pills = Self::tab_pills(d, measure, vertical);
                let pill = *pills.get(i)?;
                let sx = match at {
                    At::Drawn => self.tab_scroll.pos,
                    At::SpringTarget => self.tab_target(d, measure, i),
                };
                let r = Rect::new(pill.x - sx, pill.y, pill.w, pill.h);
                placed(r, r, i)
            }
            Located::Episode(i, episodes::Row::Still) => {
                let n = d.episodes.len();
                let focused = cx.focus.current.is_some_and(|k| k.entry == self.entry && k.elem == *key);
                let (sx, s) = match at {
                    At::Drawn => (self.episode_scroll.pos, self.episode_scale.get(i).map_or(1.0, |s| s.pos)),
                    At::SpringTarget => (if focused { Self::strip_target(i, n) } else { self.episode_scroll.pos },
                        if focused { sd::EP_FOCUS_SCALE } else { 1.0 }),
                };
                let base = sd::episode_rect(i, sd::STRIP_Y - vertical, sx);
                placed(base.scaled(s), base.scaled(sd::EP_FOCUS_SCALE), i)
            }
            Located::CollectionHeading => {
                let c = d.collection.as_ref()?;
                let top = plan.top(7)? - vertical;
                let focused = cx.focus.current.is_some_and(|k| k.elem == collection::HEADING_ELEM);
                placed(Self::collection_heading_rect(c, top, focused, measure),
                    Self::collection_heading_rect(c, top, true, measure), 0)
            }
            Located::Season(_) | Located::Related(_) | Located::Extras(_) | Located::Collection(_) | Located::Cast(_) => {
                let section = located.section();
                let frame = Self::shelf_frame(&plan, section, vertical)?;
                let which = which_of(section)?;
                self.shelf(which).place(cx, &self.cards(which, d), key, frame, at)
            }
            _ => None,
        }
    }

    /// The shelf a card section is drawn by.
    pub(super) fn shelf(&self, which: cards::Which) -> &Shelf {
        match which {
            cards::Which::Related => &self.related,
            cards::Which::Collection => &self.collection,
            cards::Which::Extras => &self.extras,
            cards::Which::Cast => &self.cast,
            cards::Which::Seasons => &self.shield.seasons,
        }
    }

    /// Where focus seats on entering group `group` of the Shield page; `None` for a group the
    /// Standard rule answers alike.
    pub(super) fn shield_seat<H: ContentLike + crate::registry::MetadataLike>(&self, group: GroupId, from: Placed,
        cx: &Cx<'_, H>) -> Option<FocusKey<u32>> {
        let d = self.detail(H::metadata(cx));
        let local = if group == TABS_GROUP {
            tab_elem(d.map_or(0, |d| self.selected_tab(d)))?
        } else if group == episodes::EPISODES_GROUP {
            episodes::elem(d.and_then(|d| self.subject_index(d)).unwrap_or(0), episodes::Row::Still)?
        } else if group == season::SEASON_GROUP {
            return d.and_then(|d| self.shield.seasons.seat::<H, _>(&self.cards(cards::Which::Seasons, d), from));
        } else {
            return None;
        };
        Some(FocusKey { entry: self.entry, elem: self.engine_key(local)? })
    }

    /// What `want` becomes on the Shield page when the page no longer offers it: a season poster
    /// whose season page has opened is that season's tab; an episode's text block (which the
    /// Shield strip has no room for) is its still; a tab past the seasons is *Show*.
    pub(super) fn shield_reconcile(&self, want: FocusKey<u32>, meta: plx_data::metadata::MetadataView<'_>)
        -> Option<FocusKey<u32>> {
        let d = self.detail(meta)?;
        let local = match self.locate(want.elem, meta)? {
            Located::Season(i) if self.season_page(d) => tab_elem(1 + i)?,
            Located::Episode(i, episodes::Row::Text) => episodes::elem(i, episodes::Row::Still)?,
            Located::Tab(i) if i > d.seasons.len() => tab_elem(0)?,
            _ => return None,
        };
        Some(FocusKey { entry: want.entry, elem: self.engine_key(local)? })
    }

    // ---- events ---------------------------------------------------------------------------------

    /// The Shield half of a tick: the pill opens with the row's focus, the ground follows the item,
    /// the strip and the tabs follow focus, and the stills pop.
    pub(super) fn shield_tick<H: ContentLike + crate::registry::MetadataLike>(&mut self, t: Tick, cx: &Cx<'_, H>,
        focused: Option<Located>, fx: &mut Effects<'_, H>) {
        let dt = t.dt();
        let meta = H::metadata(cx);
        let row = matches!(focused, Some(Located::Hero(_)));
        self.shield.pill.step(f32::from(row), plx_ui::consts::K_SCALE, dt);
        let d = self.detail(meta);
        let art = d.map(|d| if d.art.is_empty() { (d.sid.raw(), d.thumb.as_str()) } else { (d.sid.raw(), d.art.as_str()) })
            .or_else(|| self.selected.as_ref().map(|m| (m.sid.raw(), m.art.as_str())));
        self.shield.ground.subject(art);
        let blur = d.filter(|d| d.has_blur).map(|d| d.blur)
            .or_else(|| self.selected.as_ref().filter(|m| m.has_blur).map(|m| m.blur));
        self.shield.ground.update(geo::ground_target(blur), dt);
        let still = match focused {
            Some(Located::Episode(i, _)) => Some(i),
            _ => None,
        };
        for (i, spring) in self.episode_scale.iter_mut().enumerate() {
            spring.step(if still == Some(i) { sd::EP_FOCUS_SCALE } else { 1.0 }, 300.0, dt);
        }
        if let Some(d) = d {
            if let Some(i) = still {
                self.episode_scroll.step(Self::strip_target(i, d.episodes.len()), K_STRIP_SCROLL, dt);
            }
            if Self::has_tabs(d) {
                let focus = match focused {
                    Some(Located::Tab(i)) => i,
                    _ => self.selected_tab(d),
                };
                let target = self.tab_target(d, cx.measure, focus);
                self.tab_scroll.step(target, K_STRIP_SCROLL, dt);
            }
        }
        if self.shield.pill.vel.abs() > 0.01 || self.episode_scroll.vel.abs() > 0.01 || self.tab_scroll.vel.abs() > 0.01 {
            fx.note(PresentEvent::Motion);
        }
    }

    /// The scroll that brings `located`'s block into view: the header, the tabs, the action row
    /// and the season strip at the top; a section's heading at `FOCUS_HEADING_Y`.
    pub(super) fn shield_reveal(&mut self, located: Located, plan: &Plan) {
        self.scroll_target = match located.section() {
            0 | 2 => 0.0,
            sec => plan.top(sec).map_or(0.0, sd::scroll_to_heading),
        };
    }

    /// OK on tab `i`: *Show* puts the show page up; a season's opens its season page, asking the
    /// store for the season through the Standard tabs' own dwell (`pending_season`) when it is
    /// not the one loaded.
    pub(super) fn select_tab(&mut self, i: usize, meta: plx_data::metadata::MetadataView<'_>) {
        let Some(d) = self.detail(meta) else { return };
        if i == 0 {
            self.shield.season_view = false;
            self.pending_season = None;
            self.season_settle = 0.0;
        } else if i - 1 < d.seasons.len() {
            let index = i - 1;
            if !self.season_page(d) || self.pending_season.unwrap_or(d.cur_season) != index {
                self.shield.subject = None;
            }
            self.shield.season_view = true;
            // the loaded season wants nothing; another is asked for on the next tick
            self.pending_season = (d.cur_season != index).then_some(index);
            self.season_settle = if d.cur_season != index { season::SETTLE_S } else { 0.0 };
        }
        self.layout.set(None);
        self.scroll_target = 0.0;
    }

    /// The season page's subject: the episode focus came to rest on.
    pub(super) fn note_subject(&mut self, located: Located, meta: plx_data::metadata::MetadataView<'_>) {
        if let (Located::Episode(i, _), Some(d)) = (located, self.detail(meta)) {
            if let Some(ep) = d.episodes.get(i) {
                self.shield.subject = Some(ep.rk.clone());
            }
        }
    }

    /// An item-menu verb this page performs itself: the request the menu would send.
    fn item_verb<H: ContentLike>(&self, act: ItemAction, d: &Detail, loaded_episode: bool, fx: &mut Effects<'_, H>) {
        fx.push(Fx::App(AppFx::ItemMenu(ItemMenuReq { act, sid: d.sid, item: None, loaded_episode, from_home: false })));
    }

    /// **A press on a Shield row control** the Standard row has no counterpart for, or that acts
    /// on the season page's episode rather than the show. `false` leaves it to `activate_hero`.
    pub(super) fn shield_activate_hero<H: ContentLike + crate::registry::MetadataLike>(&mut self, ctl: hero::HeroCtl,
        cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> bool {
        let meta = H::metadata(cx);
        let Some(d) = self.detail(meta) else { return ctl == hero::HeroCtl::More };
        let season = self.season_page(d);
        match ctl {
            hero::HeroCtl::More => {
                let actions = self.more_actions(meta);
                self.content(fx, ContentReq::Panel(ContentPanel::More { actions }));
            }
            hero::HeroCtl::ChipVideo | hero::HeroCtl::ChipAudio | hero::HeroCtl::ChipSubs => {
                self.content(fx, ContentReq::Panel(ContentPanel::Tracks { page: 1 }));
            }
            hero::HeroCtl::Shuffle => {
                let rk = if season { d.seasons.get(d.cur_season).map(|s| s.rk.clone()) } else { Some(d.rk.clone()) };
                if let Some(rk) = rk.filter(|rk| !rk.is_empty()) {
                    self.item_verb(ItemAction::Shuffle(rk), d, false, fx);
                }
            }
            hero::HeroCtl::Play | hero::HeroCtl::Restart if season => {
                if let Some(i) = self.subject_index(d) {
                    self.play_episode_at(i, ctl == hero::HeroCtl::Restart, fx, meta);
                }
            }
            hero::HeroCtl::MarkWatched | hero::HeroCtl::MarkUnwatched if season => {
                if let Some(ep) = self.subject_episode(d) {
                    let act = if ctl == hero::HeroCtl::MarkWatched {
                        ItemAction::MarkWatched(ep.rk.clone())
                    } else {
                        ItemAction::MarkUnwatched(ep.rk.clone())
                    };
                    self.item_verb(act, d, true, fx);
                }
            }
            _ => return false,
        }
        true
    }

    /// The More panel's rows for this page as it stands.
    pub(super) fn more_actions(&self, meta: plx_data::metadata::MetadataView<'_>) -> DetailActions {
        use DetailAction as A;
        let Some(d) = self.detail(meta) else { return DetailActions::default().with(A::About, true) };
        let set = self.hero_set(meta);
        let watched = set.mark == PosterMark::Watched;
        let watchlist = self.watchlisted;
        let base = DetailActions::default()
            .with(A::MarkWatched, !watched)
            .with(A::MarkUnwatched, watched)
            .with(A::WatchlistAdd, watchlist == Some(false))
            .with(A::WatchlistRemove, watchlist == Some(true))
            .with(A::About, true);
        if self.season_page(d) {
            let ep = self.subject_episode(d);
            return base
                .with(A::GoToEpisode, ep.is_some_and(|e| !e.rk.is_empty()))
                .with(A::PlayFromStart, set.restart)
                .with(A::ShuffleSeason, d.seasons.get(d.cur_season).is_some_and(|s| !s.rk.is_empty()));
        }
        base.with(A::TrackInfo, self.tracks_available(meta))
            .with(A::AlsoAvailable, set.alt)
            .with(A::GoToShow, d.kind == "episode" && !d.show_rk.is_empty())
            .with(A::ShuffleShow, d.is_show)
    }

    /// **A row of the More panel** (`AppMsg::DetailAction`): performed by the same path the page's
    /// own control for it takes.
    pub(super) fn detail_action<H: ContentLike + crate::registry::MetadataLike>(&mut self, action: DetailAction,
        cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        let meta = H::metadata(cx);
        let Some(d) = self.detail(meta) else {
            if action == DetailAction::About {
                self.content(fx, ContentReq::Panel(ContentPanel::About));
            }
            return;
        };
        let season = self.season_page(d);
        match action {
            DetailAction::TrackInfo => self.content(fx, ContentReq::Panel(ContentPanel::Tracks { page: 1 })),
            DetailAction::About => self.content(fx, ContentReq::Panel(ContentPanel::About)),
            DetailAction::AlsoAvailable => {
                // hung off the "⋮" it was chosen from, as the Standard pill hangs it off itself
                let rect = self.shield_ctl_rects(cx, At::Drawn).into_iter()
                    .find(|(c, _)| *c == hero::HeroCtl::More).map_or(Rect::new(sd::X, 0.0, 0.0, 0.0), |(_, r)| r);
                let rect = Rect::new(rect.x, rect.y - self.scroll.pos, rect.w, rect.h);
                self.content(fx, ContentReq::Panel(ContentPanel::AltSources {
                    anchor: [rect.x, rect.y, rect.w, rect.h].map(f32::to_bits),
                }));
            }
            DetailAction::GoToShow => {
                if !d.show_rk.is_empty() {
                    self.content(fx, ContentReq::Push(ContentArg::Detail { sid: d.sid, rk: d.show_rk.clone() }));
                }
            }
            DetailAction::GoToEpisode => {
                if let Some(ep) = self.subject_episode(d).filter(|e| !e.rk.is_empty()) {
                    self.content(fx, ContentReq::Push(ContentArg::Detail { sid: d.sid, rk: ep.rk.clone() }));
                }
            }
            DetailAction::PlayFromStart => {
                if season {
                    if let Some(i) = self.subject_index(d) {
                        self.play_episode_at(i, true, fx, meta);
                    }
                } else {
                    self.play_hero(true, fx, meta);
                }
            }
            DetailAction::PlayTrailer => self.activate_hero(hero::HeroCtl::Trailer, cx, fx, meta),
            DetailAction::ShuffleShow => {
                if d.is_show && !d.rk.is_empty() {
                    self.item_verb(ItemAction::Shuffle(d.rk.clone()), d, false, fx);
                }
            }
            DetailAction::ShuffleSeason => {
                if let Some(s) = d.seasons.get(d.cur_season).filter(|s| !s.rk.is_empty()) {
                    self.item_verb(ItemAction::Shuffle(s.rk.clone()), d, false, fx);
                }
            }
            DetailAction::MarkWatched | DetailAction::MarkUnwatched => {
                let ctl = if action == DetailAction::MarkWatched { hero::HeroCtl::MarkWatched } else { hero::HeroCtl::MarkUnwatched };
                if !(season && self.shield_activate_hero(ctl, cx, fx)) {
                    self.activate_hero(ctl, cx, fx, meta);
                }
            }
            DetailAction::WatchlistAdd => self.toggle_watchlist(true, fx, meta),
            DetailAction::WatchlistRemove => self.toggle_watchlist(false, fx, meta),
        }
        fx.invalidate(Provenance::Input);
    }

    // ---- the draw -------------------------------------------------------------------------------

    /// **The page, Shield layout**: the ground, the tabs, the header, the chips, the season strip,
    /// the action row and the sections, then the stops.
    pub(super) fn draw_shield<H: ContentLike + crate::registry::MetadataLike>(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        let meta = H::metadata(f.cx);
        self.spot_facts = SpotFacts::of(self, meta);
        let measure = f.cx.measure;
        let _layout = self.pin_layout(meta, measure);
        plx_gfx::gfx::frame_clear(theme::CLEAR_RGB.0, theme::CLEAR_RGB.1, theme::CLEAR_RGB.2);
        let p = f.painter;
        let scroll = self.scroll.pos;
        self.shield.ground.draw_faded(p, 1.0 - scroll / ART_FADE_SCROLL);
        let page = p.translate(0.0, -scroll);
        let focus = f.focus.current.filter(|k| k.entry == self.entry);
        let Some(d) = self.detail(meta) else {
            self.draw_shield_waiting(f, page);
            return;
        };
        let Some(plan) = self.plan(meta, measure) else { return };
        let located = focus.and_then(|k| self.locate(k.elem, meta));
        if Self::has_tabs(d) {
            self.draw_shield_tabs(f, d, located);
        }
        let header = self.header_text(d);
        let hplan = if plan.strip {
            sd::header_plan(false, sd::summary_lines(&header.summary, measure), !header.director.is_empty())
        } else {
            plan.header
        };
        sd::draw_header(page, &header.view(), &header.director, &hplan, measure);
        if !plan.strip && !d.ratings.is_empty() {
            self.draw_ratings_at(page, d, sd::X, sd::RATINGS_Y + sd::RATINGS_H * 0.5, sd::RATINGS_GAP,
                sd::X + geo::SUMMARY_W, measure);
        }
        if plan.strip {
            self.draw_shield_strip(f, d, located);
        }
        self.draw_shield_row(f, d, located);
        for sec in self.shield_sections(d) {
            self.draw_shield_section(f, d, &plan, sec, located);
        }
        self.record_shield_stops(f, d);
        self.hold_hint.draw(f.painter, f.measure);
    }

    /// Before the item lands: the catalog row's title in the header, and the wait.
    fn draw_shield_waiting<H: ContentLike + crate::registry::MetadataLike>(&self, f: &mut DrawFrame<'_, '_, H>, page: Painter) {
        let meta = H::metadata(f.cx);
        let measure = f.cx.measure;
        let (sid, rk, title) = match self.selected() {
            Some(m) => (m.sid.raw(), m.rk.as_str(), m.title.as_str()),
            None => {
                // No item has landed: the header's title IS the "Loading" message (counted at the draw).
                plx_ui::placeholder::note(page, plx_ui::placeholder::Reason::DetailTitle, &self.rk);
                (self.sid.raw(), "", plx_platform::i18n::msg::browse_library_loading())
            }
        };
        let facts = geo::HeroFacts { sid, logo_rk: rk, title, ..Default::default() };
        geo::draw_title_block(page, &facts, sd::X, measure);
        if meta.detail_loading() {
            plx_ui::placeholder::note(page, plx_ui::placeholder::Reason::DetailSpinner, &self.rk);
            plx_ui::widgets::Spinner::new(plx_ui::consts::SCR_W * 0.5, plx_ui::consts::SCR_H * 0.7, 26.0)
                .phase(self.spin_ms as u32)
                .tint(theme::TEXT_SECONDARY)
                .draw(&Env::inert(), page);
        }
    }

    fn draw_shield_tabs<H: ContentLike + crate::registry::MetadataLike>(&self, f: &mut DrawFrame<'_, '_, H>, d: &Detail,
        located: Option<Located>) {
        let p = f.painter;
        let labels = Self::tab_labels(d);
        let refs: Vec<&std::ffi::CStr> = labels.iter().map(|l| l.as_c_str()).collect();
        let pills = Self::tab_pills(d, f.cx.measure, self.scroll.pos);
        let Some(track) = sd::tab_track(&pills) else { return };
        p.rect(track, track.h * 0.5, theme::SHIELD_TAB_TRACK, theme::SHIELD_TAB_TRACK, 0.0);
        let sx = self.tab_scroll.pos;
        let shifted: Vec<Rect> = pills.iter().map(|r| Rect::new(r.x - sx, r.y, r.w, r.h)).collect();
        let focused = match located {
            Some(Located::Tab(i)) => Some(i),
            _ => None,
        };
        let inset = sb::TAB_INSET;
        let _clip = f.clip(p, Rect::new(track.x + inset, track.y, track.w - 2.0 * inset, track.h));
        sb::draw_pills(p, &refs, &shifted, Some(self.selected_tab(d)), focused);
    }

    /// The season page's strip: each still in its slot (the focused one last, popped and ringed),
    /// its progress, its number plate; the wait while a season loads.
    fn draw_shield_strip<H: ContentLike + crate::registry::MetadataLike>(&self, f: &mut DrawFrame<'_, '_, H>, d: &Detail,
        located: Option<Located>) {
        let meta = H::metadata(f.cx);
        let p = f.painter;
        let top = sd::STRIP_Y - self.scroll.pos;
        let focused = match located {
            Some(Located::Episode(i, _)) => Some(i),
            _ => None,
        };
        let alpha = if meta.season_loading() { episodes::STALE_ALPHA } else { 1.0 };
        let pa = p.alpha(alpha);
        let n = d.episodes.len().min(episodes::MAX_ITEMS);
        let draw = |i: usize, s: f32| {
            let ep = &d.episodes[i];
            let rect = sd::episode_rect(i, top, self.episode_scroll.pos).scaled(s);
            let art = plx_ui::widgets::Art::Thumb { sid: d.sid.raw(), key: ep.thumb.as_str(),
                res: (sd::EP_W as i32, sd::EP_H as i32) };
            geo::draw_card(pa, art, rect, s, &plx_ui::cards::RowStyle::SHIELD_LANDSCAPE, episodes::progress(ep),
                focused == Some(i));
            let label = CString::new(format!("E{}", ep.index)).unwrap_or_default();
            sd::draw_episode_badge(pa, rect, s, &label, episodes::watch_state(ep) == PosterMark::Watched, f.cx.measure);
        };
        for i in (0..n).filter(|i| Some(*i) != focused) {
            let r = sd::episode_rect(i, top, self.episode_scroll.pos);
            if r.x + r.w < 0.0 || r.x > plx_ui::consts::SCR_W {
                continue;
            }
            draw(i, 1.0);
        }
        if let Some(i) = focused.filter(|i| *i < n) {
            let key = f.focus.current.filter(|k| k.entry == self.entry);
            let s = self.episode_scale.get(i).map_or(1.0, |s| s.pos) * key.map_or(1.0, |k| f.press.dip_of(&k));
            draw(i, s);
        }
        if meta.season_loading() {
            plx_ui::placeholder::note(p, plx_ui::placeholder::Reason::SeasonSpinner, &self.rk);
            plx_ui::widgets::Spinner::new(plx_ui::consts::SCR_W * 0.5, top + sd::EP_H * 0.5, 26.0)
                .phase(self.spin_ms as u32)
                .tint(theme::TEXT_PRIMARY)
                .draw(&Env::inert(), p);
        }
    }

    /// The action row and the chips.
    fn draw_shield_row<H: ContentLike + crate::registry::MetadataLike>(&self, f: &mut DrawFrame<'_, '_, H>, d: &Detail,
        located: Option<Located>) {
        let p = f.painter;
        let label = CString::new(self.pill_label(d)).unwrap_or_default();
        let open = self.shield.pill.pos;
        for (ctl, r) in self.shield_ctl_rects(f.cx, At::Drawn) {
            let r = Rect::new(r.x, r.y - self.scroll.pos, r.w, r.h);
            if r.y + r.h < 0.0 || r.y > plx_ui::consts::SCR_H {
                continue;
            }
            let on = located == Some(Located::Hero(ctl));
            match ctl {
                hero::HeroCtl::Play => {
                    let _clip = f.clip(p, r);
                    sd::draw_pill(p, r, Icon::Play, &label, open, on);
                }
                c if c.is_chip() => {
                    let (glyph, text) = Self::chip_label(c, d);
                    let text = CString::new(text).unwrap_or_default();
                    sd::draw_media_chip(p, r, glyph, &text, on);
                }
                c => {
                    let glyph = match c {
                        hero::HeroCtl::Restart => Icon::Restart,
                        hero::HeroCtl::Trailer => Icon::Trailer,
                        hero::HeroCtl::MarkWatched => Icon::Check,
                        hero::HeroCtl::MarkUnwatched => Icon::Minus,
                        hero::HeroCtl::WatchlistAdd => Icon::Bookmark,
                        hero::HeroCtl::WatchlistRemove => Icon::BookmarkFill,
                        hero::HeroCtl::Shuffle => Icon::Shuffle,
                        _ => Icon::MoreVertical,
                    };
                    sd::draw_round(p, r, glyph, on);
                }
            }
        }
    }

    /// One section: its heading (the collection's a linked one) and its shelf, every card
    /// captioned (`cards::Cards::overlay`).
    fn draw_shield_section<H: ContentLike + crate::registry::MetadataLike>(&self, f: &mut DrawFrame<'_, '_, H>,
        d: &Detail, plan: &Plan, sec: i32, located: Option<Located>) {
        let Some(top) = plan.top(sec) else { return };
        let top = top - self.scroll.pos;
        let block = plan.block[sec as usize];
        if top > plx_ui::consts::SCR_H || top + block < 0.0 {
            return;
        }
        let p = f.painter;
        if sec == 7 {
            if let Some(c) = d.collection.as_ref() {
                let h = collection::heading(c);
                let m = h.measure(f.cx.measure);
                h.draw(p, sd::X, sd::heading_text_y(top), f32::from(located == Some(Located::CollectionHeading)), &m,
                    f.cx.measure);
            }
        } else if let Some(title) = heading_of(sec, d) {
            sd::draw_heading(p, &title, top);
        }
        let Some(which) = which_of(sec) else { return };
        let frame = SectionFrame { y: top + sd::HEADING_DY, clip: Rect::FULL };
        self.shelf(which).draw(f, p, &self.cards(which, d), frame);
    }

    /// The stops the shelves do not register themselves: the action row, the tabs, the stills and
    /// the collection's heading.
    fn record_shield_stops<H: ContentLike + crate::registry::MetadataLike>(&self, f: &mut DrawFrame<'_, '_, H>, d: &Detail) {
        if !f.records_stops() {
            return;
        }
        let meta = H::metadata(f.cx);
        let set = self.hero_set(meta);
        let (controls, n) = hero::hero_ctls(set);
        let mut elems: Vec<(u32, Activate)> = controls[..n].iter().map(|c| (c.elem(), Activate::Press)).collect();
        if Self::has_tabs(d) {
            elems.extend((0..=d.seasons.len().min(MAX_TABS - 1)).filter_map(|i| tab_elem(i).map(|e| (e, Activate::Press))));
        }
        if self.season_page(d) {
            elems.extend((0..d.episodes.len().min(episodes::MAX_ITEMS))
                .filter_map(|i| episodes::elem(i, episodes::Row::Still).map(|e| (e, Activate::Press))));
        }
        if collection::len(d) > 0 {
            elems.push((collection::HEADING_ELEM, Activate::Direct));
        }
        for (elem, activate) in elems {
            let Some(elem) = self.engine_key(elem) else { continue };
            let Some(placed) = self.place(&elem, f, At::Drawn) else { continue };
            if placed.rect.y + placed.rect.h < 0.0 || placed.rect.y > plx_ui::consts::SCR_H {
                continue;
            }
            f.stop(f.painter, Stop {
                key: FocusKey { entry: self.entry, elem },
                rect: placed.rect,
                rest_rect: placed.rest_rect,
                clip: placed.clip,
                hover: Hover::Focus,
                activate,
            });
        }
    }

    /// The opener lift on the Shield page: the focused still or card drawn again over the dim.
    pub(super) fn shield_redraw_focused<H: ContentLike + crate::registry::MetadataLike>(&self, f: &mut DrawFrame<'_, '_, H>,
        focus: Option<FocusKey<u32>>) {
        let meta = H::metadata(f.cx);
        let measure = f.cx.measure;
        let (Some(d), Some(plan)) = (self.detail(meta), self.plan(meta, measure)) else { return };
        match focus.and_then(|k| self.locate(k.elem, meta)) {
            Some(Located::Episode(..)) => self.draw_shield_strip(f, d, focus.and_then(|k| self.locate(k.elem, meta))),
            Some(located @ (Located::Season(_) | Located::Related(_) | Located::Extras(_) | Located::Collection(_)
                | Located::Cast(_))) => {
                let sec = located.section();
                let (Some(which), Some(frame)) = (which_of(sec), Self::shelf_frame(&plan, sec, self.scroll.pos)) else { return };
                self.shelf(which).redraw_focused(f, f.painter, &self.cards(which, d), frame, focus);
            }
            _ => {}
        }
    }
}
