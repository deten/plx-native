//! **The Shield layout's content geometry and paint** (`layout::shield()`): the focused-item header
//! a Home-like page wears above its rows, the ground keyed from that item's artwork, and the poster
//! rows' card treatment. `side_nav` is the same layout's chrome; this is what stands inside it,
//! with `shield_browse` (the library pages' tabs, chips, grid, rail and tiles) and `shield_panel`
//! (the right-side panel).
//!
//! Every number is measured off the Plex app on Android TV at 1920x1080 (SHIELD-LAYOUT-SPEC §1,
//! §4.1, §4.4, §5), the logical space this app draws in, so they transfer 1:1 and are pinned by
//! this module's tests rather than derived from the Standard layout's tokens. Geometry and paint
//! only: which item the header describes, and when, is the page's (`screens::home`'s `shield`).
//!
//! - **The rows**: a fixed slot. The focused row's heading is always at [`HEADING_Y`] and its cards
//!   at [`ROW_CARD_Y`]; the other rows sit [`ROW_PITCH`] apart around it and the rows above it fade
//!   out as they slide under the header ([`row_alpha`], [`ROWS_CLIP_TOP`]). Inside a row the focused
//!   card is always the first slot at [`CONTENT_X`] (`card_row::RowScroll::FixedLeft`).
//! - **The card** ([`draw_card`], `card_row::Treatment::Ring`): flat artwork, radius
//!   [`CARD_RADIUS`]; the focused card scales by [`FOCUS_SCALE`] and wears a [`RING_W`] white ring
//!   [`RING_GAP`] off it; progress is an inset bar ([`progress_track`]); a finished item wears a
//!   dark plate with a check ([`badge_rect`]). No shadow, glow or label.
//! - **The header** ([`draw_hero`]): the item's clearLogo in [`LOGO_BOX`] (a shorter box and a
//!   subtitle line for an episode or a season), a facts row with a content-rating plate, a
//!   two-line summary and a dimmer cast line. No buttons, no pager.
//! - **The ground** ([`HeroGround`]): the item's UltraBlur corners as a four-corner wash
//!   ([`ground_target`]), its backdrop at [`ART`] fading into that wash to the left and below, both
//!   cross-fading when the subject changes.

use std::ffi::CString;
use std::os::raw::c_int;

use crate::consts::{MARGIN_X, SCR_W};
use crate::hero_logo::HeroLogo;
use crate::label::{Label, VAlign};
use crate::text_view::TextView;
use crate::widgets::{AmbientWash, Art, WashArt};
use crate::{theme, Painter, Rect, Spring};
use plx_machine::machine::Measure;

// ---- the content column (spec §1.2) -------------------------------------------------------------

/// The content's left edge: the rail owns 0..128, and the Home and Library dumps put the content at
/// 127.
pub const CONTENT_X: f32 = 127.0;
/// The content's right edge: the Standard 96 margin.
pub const CONTENT_RIGHT: f32 = SCR_W - MARGIN_X;

// ---- the poster rows (spec §5.1, §1.6) ----------------------------------------------------------

pub const CARD_W: f32 = 240.0;
pub const CARD_H: f32 = 360.0;
pub const CARD_GAP: f32 = 32.0;
/// One step along a row: x = 127, 399, 671, ...
pub const CARD_PITCH: f32 = CARD_W + CARD_GAP;
pub const CARD_RADIUS: f32 = 12.0;
/// The focused poster's pop: 240x360 to ~250x375.
pub const FOCUS_SCALE: f32 = 250.0 / CARD_W;
/// The focused row's heading box (48 tall) and its cards' top: fixed, whatever row has focus.
pub const HEADING_Y: f32 = 504.0;
pub const HEADING_H: f32 = 48.0;
pub const ROW_CARD_Y: f32 = 568.0;
/// Heading box top to the row's card top.
pub const HEADING_DY: f32 = ROW_CARD_Y - HEADING_Y;
/// Row to row: the next row's heading at 976, its cards at 1040, cut by the screen's bottom.
pub const ROW_PITCH: f32 = 472.0;
/// Rows are cut here as they slide up under the header (the cast line ends at 456).
pub const ROWS_CLIP_TOP: f32 = 480.0;
/// The ring: 4px solid, standing 2px off the scaled card, so its outer edge is 6px out.
pub const RING_W: f32 = 4.0;
pub const RING_GAP: f32 = 2.0;
/// The progress bar (spec §5.2): a track inset 12 from the card's sides, 8 tall, its bottom 12 above
/// the card's; the fill 4 tall, 2 inside it. All on the RESTING card, scaled with the pop.
pub const PROGRESS_INSET_X: f32 = 12.0;
pub const PROGRESS_H: f32 = 8.0;
pub const PROGRESS_BOTTOM: f32 = 12.0;
pub const PROGRESS_FILL_INSET: f32 = 2.0;
/// The watched plate at the card's top-right corner, its inner corner rounded, and its check.
pub const BADGE: f32 = 40.0;
pub const BADGE_INNER_RADIUS: f32 = 8.0;
pub const BADGE_GLYPH: f32 = 20.0;

/// Where the cards of a row `offset` rows from the focused one sit (`offset` is fractional while
/// the rows glide: `row - focused_row`).
pub fn row_card_top(offset: f32) -> f32 {
    ROW_CARD_Y + offset * ROW_PITCH
}

/// A row's heading box over cards whose top is at `card_top`.
pub fn heading_box(card_top: f32) -> Rect {
    Rect::new(CONTENT_X, card_top - HEADING_DY, CONTENT_RIGHT - CONTENT_X, HEADING_H)
}

/// How much of a row `offset` rows from the focused one is drawn: the focused row and every row
/// below it whole, a row above fading out over the first ~0.4 of a pitch it slides under the
/// header, so the header is never overlaid by the row that just left the slot.
pub fn row_alpha(offset: f32) -> f32 {
    if offset >= 0.0 {
        1.0
    } else {
        (1.0 + offset * 2.5).clamp(0.0, 1.0)
    }
}

/// The ring around a (scaled) card: 6px out, its outer radius following the card's.
pub fn ring_rect(card: Rect) -> Rect {
    card.inset(-(RING_GAP + RING_W))
}

pub fn ring_radius(card_radius: f32) -> f32 {
    card_radius + RING_GAP + RING_W
}

/// The progress track inside a card drawn at `card` (`s` is the pop it was scaled by).
pub fn progress_track(card: Rect, s: f32) -> Rect {
    Rect::new(
        card.x + PROGRESS_INSET_X * s,
        card.y + card.h - (PROGRESS_BOTTOM + PROGRESS_H) * s,
        card.w - 2.0 * PROGRESS_INSET_X * s,
        PROGRESS_H * s,
    )
}

/// The played part of `track` at `frac`.
pub fn progress_fill(track: Rect, frac: f32, s: f32) -> Rect {
    let inner = track.inset(PROGRESS_FILL_INSET * s);
    Rect::new(inner.x, inner.y, inner.w * frac.clamp(0.0, 1.0), inner.h)
}

/// The watched plate on a card drawn at `card`, scaled with it.
pub fn badge_rect(card: Rect, s: f32) -> Rect {
    let b = BADGE * s;
    Rect::new(card.x + card.w - b, card.y, b, b)
}

/// **One card of a ring row**, at its drawn (popped and pressed) rect `rect`, scale `s`: the flat
/// face with its watched plate (`widgets::card_flat`), the progress bar when `resume` has one, and
/// the ring when the card is the focused one. `card_row::draw_tile` / `draw_focused` come here for
/// a [`Treatment::Ring`](crate::card_row::Treatment::Ring) style.
pub fn draw_card(p: Painter, art: Art, rect: Rect, s: f32, sty: &crate::card_row::RowStyle, resume: Option<f32>,
    focused: bool) {
    let rad = sty.tile_radius(rect, s);
    crate::widgets::card_flat(p, rect, art, rad, s);
    if let Some(frac) = resume {
        let track = progress_track(rect, s);
        p.rect(track, track.h * 0.5, theme::SHIELD_PROGRESS_TRACK, theme::SHIELD_PROGRESS_TRACK, 0.0);
        let fill = progress_fill(track, frac, s);
        if fill.w >= 1.0 {
            p.rect(fill, fill.h * 0.5, theme::SHIELD_PROGRESS_FILL, theme::SHIELD_PROGRESS_FILL, 0.0);
        }
    }
    if focused {
        p.rring(ring_rect(rect), ring_radius(rad), RING_W, theme::SHIELD_RING);
    }
}

/// The watched plate: a dark square in the card's top-right corner, its outer corner following the
/// card's radius `rad`, with a white check (`widgets::card_flat` draws it for a finished poster).
pub fn draw_watched(p: Painter, card: Rect, rad: f32, s: f32) {
    let b = badge_rect(card, s);
    p.rrect(b, BADGE_INNER_RADIUS * s, rad, theme::SHIELD_BADGE_FILL);
    // quantized to 4px so the pop reuses a handful of cached masks (the Standard tick's discipline)
    let d = ((BADGE_GLYPH * s) / 4.0).round() * 4.0;
    crate::icons::draw(p, crate::icons::Icon::Check, Rect::new(b.cx() - d * 0.5, b.cy() - d * 0.5, d, d),
        theme::SHIELD_BADGE_INK);
}

/// The type of a row heading's title, and of the quiet source after it (~20 measured: the
/// one-line de-emphasised rung).
pub const HEADING_SIZE: c_int = theme::size::HEADLINE;
pub const HEADING_SOURCE_SIZE: c_int = theme::size::MICRO;
/// The air either side of the source's separator dot.
pub const HEADING_SOURCE_PAD: f32 = 8.0;

/// The draw-y of a row heading's title over cards whose top is at `card_top`: its cap band centred
/// in the heading box. What a linked heading (`linked_heading::LinkedHeading::draw`) takes as its
/// `y`, so the plain and the linked heading set their titles on one line.
pub fn heading_text_y(card_top: f32) -> f32 {
    plx_gfx::text::text_vcenter_y(HEADING_SIZE, 1, heading_box(card_top).cy())
}

/// **A row heading** over cards whose top is at `card_top`: the hub's title, bold, and — when the
/// row names its source — a quiet `· source` on the title's baseline. Server text: the title elides
/// at the content's right edge and a source with no room left is dropped.
pub fn draw_row_heading(p: Painter, title: &str, source: &str, card_top: f32, measure: &dyn Measure) {
    let bx = heading_box(card_top);
    let y = heading_text_y(card_top);
    let title = plx_gfx::text::elide_by(title, bx.w, false, |t| measure.width_str(t, HEADING_SIZE, true));
    let Ok(t) = CString::new(title) else { return };
    let mut dx = p.text(t.as_ptr(), bx.x, y, HEADING_SIZE, theme::SHIELD_HEADING, 0, 1);
    if source.is_empty() {
        return;
    }
    let run = format!("\u{b7} {source}");
    let room = bx.w - dx - HEADING_SOURCE_PAD;
    if room < 48.0 {
        return;
    }
    let run = plx_gfx::text::elide_by(&run, room, false, |t| measure.width_str(t, HEADING_SOURCE_SIZE, false));
    let Ok(s) = CString::new(run) else { return };
    dx += HEADING_SOURCE_PAD;
    let sy = plx_gfx::text::baseline_y(HEADING_SOURCE_SIZE, 0, HEADING_SIZE, 1, y);
    p.text(s.as_ptr(), bx.x + dx, sy, HEADING_SOURCE_SIZE, theme::SHIELD_HEADING_SOURCE, 0, 0);
}

// ---- the header (spec §4.1) ---------------------------------------------------------------------

/// The clearLogo's box for a movie or a show: the logo is contained in it, left and top aligned.
pub const LOGO_BOX: Rect = Rect::new(CONTENT_X, 128.0, 720.0, 160.0);
/// The box's height for an episode or a season, whose subtitle line takes the rest.
pub const LOGO_BOX_SHORT_H: f32 = 100.0;
/// The episode's or the season's own name under the short box: a 48 line at y 240, so both variants
/// end at 288.
pub const SUBTITLE_Y: f32 = 240.0;
pub const SUBTITLE_H: f32 = 48.0;
pub const SUBTITLE_SIZE: c_int = theme::size::HEADLINE;
/// The text a logo-less item's title is set in, by box.
pub const LOGO_TEXT_SIZE: c_int = theme::size::HERO;
pub const LOGO_TEXT_SIZE_SHORT: c_int = theme::size::DISPLAY;
/// The facts row: 24-tall items 24 apart at y 302.
pub const META_Y: f32 = 302.0;
pub const META_H: f32 = 24.0;
pub const META_GAP: f32 = 24.0;
pub const META_SIZE: c_int = theme::size::CAPTION;
/// The content-rating plate: 32 tall at y 298 (centred on the facts row), the text padded 9 each
/// side, a 2px outline, radius 6.
pub const RATING_Y: f32 = 298.0;
pub const RATING_H: f32 = 32.0;
pub const RATING_PAD: f32 = 9.0;
pub const RATING_EDGE_W: f32 = 2.0;
pub const RATING_RADIUS: f32 = 6.0;
/// The summary: two 32px lines, 820 wide, at y 352, the second ending in an ellipsis.
pub const SUMMARY_Y: f32 = 352.0;
pub const SUMMARY_W: f32 = 820.0;
pub const SUMMARY_LINE: f32 = 32.0;
pub const SUMMARY_LINES: usize = 2;
pub const SUMMARY_SIZE: c_int = theme::size::LABEL;
/// The cast line: one 28 line, 764 wide, at y 428.
pub const CAST_Y: f32 = 428.0;
pub const CAST_W: f32 = 764.0;
pub const CAST_H: f32 = 28.0;
pub const CAST_SIZE: c_int = theme::size::CAPTION;
/// The item's backdrop: the right two thirds of the screen, 16:9, top-aligned (spec §4.4)…
pub const ART: Rect = Rect::new(640.0, 0.0, 1280.0, 720.0);
/// …dissolving into the ground over its first 360px from the left and its last 240px at the foot.
pub const ART_FADE_W: f32 = 360.0;
pub const ART_FADE_Y: f32 = 480.0;
/// How far the ground leans from the app surface toward the artwork's (luma-capped) corners. Plex
/// paints the server's colours straight; a hair under that keeps the brightest capped corner under
/// the header's grey inks (`the_ground_keeps_the_header_readable`).
pub const GROUND_W: f32 = 0.88;

/// What the header says about its item — plain values the page fills from its catalog row.
#[derive(Clone, Copy, Default)]
pub struct HeroFacts<'a> {
    /// The server `logo_rk` is a key on.
    pub sid: u16,
    /// Whose clearLogo the box shows: an episode's or a season's SHOW.
    pub logo_rk: &'a str,
    /// The run drawn in the box while there is no logo.
    pub title: &'a str,
    /// An episode's or a season's own name: present, the logo box is the short one.
    pub subtitle: &'a str,
    /// The facts, in order ("S5 · E1", a date, "21m", "Family, Fantasy").
    pub facts: &'a [String],
    /// The content rating's plate text, or empty for none.
    pub rating: &'a str,
    pub summary: &'a str,
    pub cast: &'a str,
}

/// The logo box for `facts`: the tall one, or the short one over a subtitle.
pub fn logo_box(facts: &HeroFacts<'_>) -> Rect {
    if facts.subtitle.is_empty() {
        LOGO_BOX
    } else {
        Rect::new(LOGO_BOX.x, LOGO_BOX.y, LOGO_BOX.w, LOGO_BOX_SHORT_H)
    }
}

/// The plate around a content rating `text_w` wide, its left edge at `x`.
pub fn rating_rect(x: f32, text_w: f32) -> Rect {
    Rect::new(x, RATING_Y, text_w + 2.0 * RATING_PAD, RATING_H)
}

/// The summary's first cap top inside its 32px line: the cap band centred on the line box.
pub fn summary_cap_top(measure: &dyn Measure) -> f32 {
    SUMMARY_Y + (SUMMARY_LINE - measure.cap_h(SUMMARY_SIZE)) * 0.5
}

/// **The header**, through `p` (its alpha is the subject cross-fade). Every run is server text and
/// elides; the facts row drops what no longer fits the summary's width.
pub fn draw_hero(p: Painter, facts: &HeroFacts<'_>, measure: &dyn Measure) {
    let short = !facts.subtitle.is_empty();
    let size = if short { LOGO_TEXT_SIZE_SHORT } else { LOGO_TEXT_SIZE };
    HeroLogo::new(facts.sid, facts.logo_rk, facts.title, crate::hero_logo::LogoRung::Hero)
        .draw_boxed(p, logo_box(facts), size, measure);
    if short {
        let line = plx_gfx::text::elide_by(facts.subtitle, LOGO_BOX.w, false,
            |t| measure.width_str(t, SUBTITLE_SIZE, true));
        if let Ok(c) = CString::new(line) {
            Label::new(c.as_ptr(), SUBTITLE_SIZE, theme::SHIELD_HERO_SUBTITLE).bold()
                .draw(p, Rect::new(CONTENT_X, SUBTITLE_Y, LOGO_BOX.w, SUBTITLE_H));
        }
    }
    let mut x = CONTENT_X;
    let right = CONTENT_X + SUMMARY_W;
    for fact in facts.facts.iter().filter(|f| !f.is_empty()) {
        let w = measure.width_str(fact, META_SIZE, false);
        if x + w > right {
            break;
        }
        if let Ok(c) = CString::new(fact.as_str()) {
            Label::new(c.as_ptr(), META_SIZE, theme::SHIELD_HERO_TEXT).draw(p, Rect::new(x, META_Y, w, META_H));
        }
        x += w + META_GAP;
    }
    if !facts.rating.is_empty() {
        let w = measure.width_str(facts.rating, META_SIZE, false);
        let plate = rating_rect(x, w);
        if plate.x + plate.w <= right {
            p.rect(plate, RATING_RADIUS, theme::SHIELD_RATING_FILL, theme::SHIELD_RATING_FILL, 0.0);
            p.rring(plate, RATING_RADIUS, RATING_EDGE_W, theme::SHIELD_RATING_EDGE);
            if let Ok(c) = CString::new(facts.rating) {
                Label::new(c.as_ptr(), META_SIZE, theme::SHIELD_RATING_INK)
                    .h(crate::label::HAlign::Center).draw(p, plate);
            }
        }
    }
    if !facts.summary.is_empty() {
        TextView::new(facts.summary, SUMMARY_SIZE, theme::SHIELD_HERO_TEXT)
            .with_measure(measure)
            .leading(SUMMARY_LINE)
            .max_lines(SUMMARY_LINES)
            .draw(p, Rect::new(CONTENT_X, summary_cap_top(measure), SUMMARY_W, 0.0));
    }
    if !facts.cast.is_empty() {
        let line = plx_gfx::text::elide_by(facts.cast, CAST_W, false, |t| measure.width_str(t, CAST_SIZE, false));
        if let Ok(c) = CString::new(line) {
            Label::new(c.as_ptr(), CAST_SIZE, theme::SHIELD_HERO_CAST).v(VAlign::Middle)
                .draw(p, Rect::new(CONTENT_X, CAST_Y, CAST_W, CAST_H));
        }
    }
}

// ---- the ground (spec §1.4 bg.tinted / bg.neutral, §4.4) ----------------------------------------

/// The four corners a page's ground dissolves toward for an item: its server-computed UltraBlur
/// colours (`blur`, corner-major TL, TR, BR, BL, as the catalog carries them), luma-capped and
/// leaned by [`GROUND_W`]; the measured neutral diagonal for an item with none.
pub fn ground_target(blur: Option<[[f32; 3]; 4]>) -> [[f32; 4]; 4] {
    match blur {
        Some(blur) => AmbientWash::keyed(blur, [GROUND_W; 4]),
        None => theme::SHIELD_GROUND_NEUTRAL,
    }
}

/// **The header's ground**: the tinted wash and the subject's backdrop over it, each cross-fading
/// when the subject changes. The art is asked of the source only for the subject the page names
/// ([`subject`](Self::subject)), so a page that debounces its subject debounces its fetches.
pub struct HeroGround {
    wash: AmbientWash,
    /// Has a subject keyed the wash yet? The first one is jumped to, not dissolved from grey.
    keyed: bool,
    /// The subject's backdrop `(server, path)`, and how far it has faded in.
    art: Option<(u16, String)>,
    art_a: Spring,
    /// The previous subject's backdrop, fading out over the incoming one.
    prev: Option<(u16, String)>,
    prev_a: Spring,
}

impl Default for HeroGround {
    fn default() -> Self {
        Self::new()
    }
}

impl HeroGround {
    pub fn new() -> Self {
        Self {
            wash: AmbientWash::flat(theme::SURFACE_APP),
            keyed: false,
            art: None,
            art_a: Spring::at(0.0),
            prev: None,
            prev_a: Spring::at(0.0),
        }
    }

    /// The page's subject changed: its backdrop (`None` for an item with none) becomes the incoming
    /// layer and the one on screen fades out over it. The same backdrop again changes nothing.
    pub fn subject(&mut self, art: Option<(u16, &str)>) {
        let same = match (&self.art, art) {
            (Some((s, a)), Some((t, b))) => *s == t && a == b,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if self.art.is_some() && self.art_a.pos > self.prev_a.pos {
            self.prev = self.art.take();
            self.prev_a.jump(self.art_a.pos);
        }
        self.art = art.filter(|(_, path)| !path.is_empty()).map(|(s, p)| (s, p.to_owned()));
        self.art_a.jump(0.0);
    }

    /// One frame: dissolve the wash toward `target` (jumped to on the first subject), fade the
    /// subject's backdrop in once its texture is resident and the previous one out.
    pub fn update(&mut self, target: [[f32; 4]; 4], dt: f32) {
        if !self.keyed {
            self.wash.jump(target);
            self.keyed = true;
        }
        let resident = resolve(self.art.as_ref()).0 != 0;
        // decorative atmosphere: none of it may hold the page-freeze snapshot (`idle::decorative`)
        plx_machine::idle::decorative(|| {
            self.wash.step(target, AmbientWash::K, dt);
            if resident {
                self.art_a.step(1.0, AmbientWash::K, dt);
            }
            if self.prev.is_some() {
                self.prev_a.step(0.0, AmbientWash::K, dt);
            }
        });
        if self.prev.is_some() && self.prev_a.pos < 0.01 {
            self.prev = None;
            self.prev_a.jump(0.0);
        }
    }

    /// Forget the subject and the colours: the next subject keys the wash afresh.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// The colour the ground puts at `(x, y)`.
    pub fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        self.wash.sample(Rect::FULL, x, y)
    }

    /// Paint the ground over the whole screen: the wash with the subject's backdrop in one pass,
    /// the outgoing backdrop over it, then the two dissolves of the art's edges into the wash.
    pub fn draw(&self, p: Painter) {
        let cur = resolve(self.art.as_ref());
        if let Some((_, path)) = self.art.as_ref().filter(|_| cur.0 == 0) {
            crate::placeholder::note(p, crate::placeholder::Reason::HomeBackdrop, path);
        }
        let a = if cur.0 != 0 { self.art_a.pos } else { 0.0 };
        let art = (a > 0.01).then(|| WashArt {
            tex: cur.0,
            rect: ART,
            uv: ART.cover_uv(cur.1, cur.2, crate::Crop::Centre),
            tint: theme::with_a(theme::TINT_WHITE, a),
        });
        self.wash.draw_ground(p, Rect::FULL, art, None);
        let prev = resolve(self.prev.as_ref());
        let pa = if prev.0 != 0 { self.prev_a.pos } else { 0.0 };
        if pa > 0.01 {
            p.tex_uv(prev.0, ART.cover_uv(prev.1, prev.2, crate::Crop::Centre), ART, 0.0,
                theme::with_a(theme::TINT_WHITE, pa));
        }
        if a > 0.01 || pa > 0.01 {
            self.draw_fades(p);
        }
    }

    /// The art's left and bottom edges dissolving into the wash: the wash's own colour laid over the
    /// art at a falling alpha (`AmbientWash::sample`'s one job). Each strip keeps ONE colour along
    /// its alpha axis, so the straight-alpha gradient interpolates exactly.
    fn draw_fades(&self, p: Painter) {
        let ink = |c: [f32; 3], a: f32| [c[0], c[1], c[2], a];
        let (top, foot) = (self.sample(ART.x, ART.y), self.sample(ART.x, ART.y + ART.h));
        p.grad4(Rect::new(ART.x, ART.y, ART_FADE_W, ART.h),
            [ink(top, 1.0), ink(top, 0.0), ink(foot, 0.0), ink(foot, 1.0)]);
        let (left, right) = (foot, self.sample(ART.x + ART.w, ART.y + ART.h));
        p.grad4(Rect::new(ART.x, ART_FADE_Y, ART.w, ART.y + ART.h - ART_FADE_Y),
            [ink(left, 0.0), ink(right, 0.0), ink(right, 1.0), ink(left, 1.0)]);
    }
}

/// A backdrop's texture at the hero request size — the Standard hero's own key, so the two layouts
/// share one cached picture per item.
fn resolve(art: Option<&(u16, String)>) -> (u32, f32, f32) {
    art.map_or((0, 0.0, 0.0), |(sid, path)| crate::widgets::resolve_tex_wh_on(*sid, path, 1280, 720, 0))
}

/// The top of the focused card's ring: the rows' clip and the row's own heading must both end above
/// it (the tests hold them to it).
pub fn focused_ring_top() -> f32 {
    ROW_CARD_Y - (CARD_H * (FOCUS_SCALE - 1.0)) * 0.5 - RING_GAP - RING_W
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_row::{RowScroll, RowStyle, Treatment};

    /// The row geometry the spec measured (§5.1): heading 504, cards 568..928, row pitch 472 so
    /// the next heading is at 976; 240x360 posters on a 272 pitch from x 127; the seventh partly
    /// visible.
    #[test]
    fn the_rows_sit_where_the_spec_measured_them() {
        assert_eq!((HEADING_Y, ROW_CARD_Y, ROW_CARD_Y + CARD_H), (504.0, 568.0, 928.0));
        assert_eq!(row_card_top(1.0) - HEADING_DY, 976.0);
        assert_eq!(row_card_top(1.0), 1040.0);
        assert_eq!(heading_box(ROW_CARD_Y), Rect::new(127.0, 504.0, 1697.0, 48.0));
        let sty = RowStyle::SHIELD_POSTER;
        assert_eq!((sty.w, sty.h, sty.gap, sty.margin_x, sty.radius), (240.0, 360.0, 32.0, 127.0, 12.0));
        let xs: Vec<f32> = (0..7).map(|i| crate::card_row::tile_rect(i, sty.margin_x, CARD_PITCH, 0.0, 0.0,
            (sty.w, sty.h)).x).collect();
        assert_eq!(xs, [127.0, 399.0, 671.0, 943.0, 1215.0, 1487.0, 1759.0]);
        assert!(xs[6] < SCR_W && xs[6] + CARD_W > SCR_W, "the seventh card bleeds off the right edge");
        assert_eq!((sty.scroll, sty.treatment), (RowScroll::FixedLeft, Treatment::Ring));
    }

    /// The focus (§1.6): ~250x375 about the centre, a 4px ring 2px off it — the measured ring rect
    /// `[116..377] x [555..940]` within a pixel, its outer radius ~18.
    #[test]
    fn the_focused_card_pops_to_250_by_375_inside_the_measured_ring() {
        let rest = Rect::new(CONTENT_X, ROW_CARD_Y, CARD_W, CARD_H);
        let popped = rest.scaled(FOCUS_SCALE);
        assert!((popped.w - 250.0).abs() < 1e-3 && (popped.h - 375.0).abs() < 1e-3);
        assert_eq!(popped.cy(), rest.cy(), "the pop keeps the centre");
        let ring = ring_rect(popped);
        for (got, want) in [(ring.x, 116.0), (ring.x + ring.w, 377.0), (ring.y, 555.0), (ring.y + ring.h, 940.0)] {
            assert!((got - want).abs() <= 1.5, "ring edge {got} vs measured {want}");
        }
        let r = ring_radius(CARD_RADIUS * FOCUS_SCALE);
        assert!((18.0..=20.0).contains(&r), "outer radius {r}");
        assert!(focused_ring_top() > HEADING_Y + HEADING_H, "the ring clears its own heading");
        assert!(focused_ring_top() > ROWS_CLIP_TOP);
    }

    /// The marks (§5.2): the track 12 in from each side and 8 tall, its bottom 12 above the card's
    /// (y 908..916 on a card ending at 928), the fill 2 inside it; the plate 40x40 at x + 200.
    #[test]
    fn progress_and_the_watched_plate_follow_the_measured_insets() {
        let card = Rect::new(CONTENT_X, ROW_CARD_Y, CARD_W, CARD_H);
        let track = progress_track(card, 1.0);
        assert_eq!(track, Rect::new(139.0, 908.0, 216.0, 8.0));
        assert_eq!(progress_fill(track, 0.5, 1.0), Rect::new(141.0, 910.0, 106.0, 4.0));
        assert_eq!(progress_fill(track, 2.0, 1.0).w, 212.0, "a fraction past the end fills, never overflows");
        assert_eq!(badge_rect(card, 1.0), Rect::new(card.x + 200.0, card.y, 40.0, 40.0));
        let popped = card.scaled(FOCUS_SCALE);
        let b = badge_rect(popped, FOCUS_SCALE);
        assert!((b.x + b.w - (popped.x + popped.w)).abs() < 1e-3 && b.y == popped.y, "the plate rides the pop");
    }

    /// A FixedLeft row brings every cell to the first slot, the last one included, and never past.
    #[test]
    fn a_fixed_left_row_rests_its_focus_in_the_first_slot() {
        let sty = RowStyle::SHIELD_POSTER;
        assert_eq!(crate::card_row::fixed_left_scroll(0, 10, &sty), 0.0);
        assert_eq!(crate::card_row::fixed_left_scroll(2, 10, &sty), 544.0);
        assert_eq!(crate::card_row::fixed_left_scroll(9, 10, &sty), 9.0 * 272.0, "the last card comes to the slot too");
        assert_eq!(sty.max_scroll(10), 9.0 * 272.0);
        assert_eq!(sty.max_scroll(0), 0.0);
        assert_eq!(crate::card_row::heading_clearance(Some(0), &sty, 0.0), 0.0, "a ring row's heading never rises");
        let mut row = crate::card_row::CardRow::new();
        for _ in 0..240 {
            row.update(10, Some(3), &sty, 1.0 / 60.0);
        }
        assert!((row.scroll_x() - 3.0 * 272.0).abs() < 0.01, "the row slid under the slot: {}", row.scroll_x());
        assert!(row.settle_lag(10, 3, &sty).abs() < 0.01);
    }

    /// The Standard presets did not move: same scroll rule, same treatment.
    #[test]
    fn the_standard_presets_keep_their_reveal_and_glow() {
        for sty in [RowStyle::HOME, RowStyle::EPISODE, RowStyle::CAST, RowStyle::PROFILES] {
            assert_eq!((sty.scroll, sty.treatment), (RowScroll::Reveal, Treatment::Glow));
        }
        assert_eq!((RowStyle::HOME.w, RowStyle::HOME.h, RowStyle::HOME.gap, RowStyle::HOME.focus_scale),
            (250.0, 375.0, 30.0, 1.09));
        let vw = SCR_W - 2.0 * RowStyle::HOME.margin_x;
        assert_eq!(RowStyle::HOME.max_scroll(10), 10.0 * 280.0 - 30.0 - vw);
    }

    /// The header column (§4.1): logo box 720x160 at 127,128 (720x100 over a subtitle at 240, so
    /// both end at 288), facts at 302 with the plate centred on them, the summary 820 wide on two
    /// 32 lines at 352, the cast at 428; the art the right two thirds, 16:9.
    #[test]
    fn the_header_column_is_the_measured_one() {
        let movie = HeroFacts::default();
        assert_eq!(logo_box(&movie), Rect::new(127.0, 128.0, 720.0, 160.0));
        let episode = HeroFacts { subtitle: "Soul Custody", ..Default::default() };
        let short = logo_box(&episode);
        assert_eq!(short, Rect::new(127.0, 128.0, 720.0, 100.0));
        assert_eq!(short.y + short.h + 12.0, SUBTITLE_Y);
        assert_eq!(SUBTITLE_Y + SUBTITLE_H, LOGO_BOX.y + LOGO_BOX.h, "both variants end at 288");
        assert_eq!(rating_rect(412.0, 53.0), Rect::new(412.0, 298.0, 71.0, 32.0), "TV-14's measured 71 wide");
        assert_eq!(RATING_Y + RATING_H * 0.5, META_Y + META_H * 0.5, "the plate centres on the facts row");
        assert_eq!((SUMMARY_Y, SUMMARY_W, SUMMARY_LINE * SUMMARY_LINES as f32), (352.0, 820.0, 64.0));
        assert_eq!((CAST_Y, CAST_W, CAST_H), (428.0, 764.0, 28.0));
        assert!(CAST_Y + CAST_H < ROWS_CLIP_TOP && ROWS_CLIP_TOP < HEADING_Y);
        assert_eq!(ART, Rect::new(640.0, 0.0, 1280.0, 720.0));
        assert_eq!(ART.x + ART.w, SCR_W);
        assert!(ART_FADE_Y < ART.y + ART.h && ART.x + ART_FADE_W < SUMMARY_W + CONTENT_X + 200.0);
    }

    /// Rows above the slot are gone before they reach the header; the slot and below are whole.
    #[test]
    fn a_row_leaving_the_slot_fades_before_it_reaches_the_header() {
        assert_eq!(row_alpha(0.0), 1.0);
        assert_eq!(row_alpha(3.0), 1.0);
        assert!(row_alpha(-0.2) > 0.0 && row_alpha(-0.2) < 1.0);
        assert_eq!(row_alpha(-0.4), 0.0);
        // what is still fading is cut at the clip, which the header's last line ends above
        assert!(CAST_Y + CAST_H <= ROWS_CLIP_TOP);
    }

    /// The ground keeps the header readable on the brightest artwork: a heading's white over the
    /// brightest capped corner stays above 4.5:1, and the header's dimmer inks (all at least
    /// `size::CAPTION`, large text) above the 3:1 large-text floor `AmbientWash` holds its grounds to.
    #[test]
    fn the_ground_keeps_the_header_readable() {
        fn over(ink: [f32; 4], g: [f32; 4]) -> [f32; 4] {
            std::array::from_fn(|i| if i == 3 { 1.0 } else { ink[i] * ink[3] + g[i] * (1.0 - ink[3]) })
        }
        let ratio = crate::widgets::contrast;
        for src in [[1.0, 1.0, 1.0], [0.2, 1.0, 0.2], [1.0, 0.9, 0.2], [0.4, 0.6, 1.0]] {
            let g = ground_target(Some([src; 4]))[0];
            let heading = ratio(over(theme::SHIELD_HEADING, g), g);
            let text = ratio(over(theme::SHIELD_HERO_TEXT, g), g);
            let cast = ratio(over(theme::SHIELD_HERO_CAST, g), g);
            assert!(heading >= 4.5, "{src:?}: a heading at {heading:.2}:1");
            assert!(text >= 3.0, "{src:?}: the summary at {text:.2}:1");
            assert!(cast >= 3.0, "{src:?}: the cast line at {cast:.2}:1");
        }
        assert_eq!(ground_target(None), theme::SHIELD_GROUND_NEUTRAL);
    }

    /// A new subject fades in over the old one; the same subject again changes nothing.
    #[test]
    fn a_subject_change_cross_fades_and_a_repeat_is_inert() {
        let mut g = HeroGround::new();
        g.subject(Some((0, "/a")));
        g.art_a.jump(1.0);
        g.subject(Some((0, "/a")));
        assert_eq!(g.art_a.pos, 1.0, "the same backdrop again changes nothing");
        g.subject(Some((0, "/b")));
        assert_eq!(g.prev.as_ref().map(|(_, p)| p.as_str()), Some("/a"), "the old one fades out over the new");
        assert_eq!((g.prev_a.pos, g.art_a.pos), (1.0, 0.0));
        g.subject(None);
        assert!(g.art.is_none());
    }
}
