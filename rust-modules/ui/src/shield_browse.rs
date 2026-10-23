//! **The Shield layout's library pages** (`layout::shield()`; SHIELD-LAYOUT-SPEC §3, §5.5, §6): the
//! page tab pills a top-level page carries above its content, the Browse page's chip bar, its
//! captioned poster grid and A–Z jump rail, and the Categories page's genre tiles. `shield` is the
//! focused-item header and the poster rows the Recommended tab shares with Home; `shield_panel` is
//! the right-side panel a chip opens.
//!
//! Every number is measured off the Plex app on Android TV at 1920x1080, the logical space this app
//! draws in, so they transfer 1:1 and are pinned by this module's tests. Geometry and paint only:
//! which tab is selected, what a chip says and does, which letter the grid is under, is the page's
//! (`screens::library`'s `shield`).
//!
//! - **Tabs** ([`tab_pills`], [`draw_tabs`]): 48-tall pills in a 64-tall track at y 32, each its
//!   label plus 16 a side, 16 apart. Idle: a dim label; selected: a 2px outline; focused: a white
//!   plate with black ink, never scaled.
//! - **The chip bar** ([`chip_bar`], [`draw_chip`]): 48-tall full pills at y 132 — filter, type,
//!   the word "by", sort (with its direction glyph), a dark item-count box and a round "more" chip.
//! - **The grid** ([`GRID_COLS`] posters from x 127, [`GRID_ROW_PITCH`] apart): every card wears its
//!   caption under it ([`draw_caption`], `card_row::Treatment::Ring` grids through `cards::Grid`),
//!   title and year centred, the focused card's following its pop and gliding when it overflows.
//! - **The A–Z rail** ([`rail_geom`], [`draw_rail_letter`]): 32-pitch letters on a 44 column at
//!   the right, centred on the page; the grid's letter in a ring, the focused one on a white disc.
//! - **Categories** ([`tile_rect`], [`draw_tile`]): 376x212 genre tiles on a 424x312 pitch, each a
//!   solid wash of its own ([`tile_tint`]) under its centred name, ringed like a card on focus.

use std::ffi::CStr;
use std::os::raw::c_int;

use crate::card_row::TileLabel;
use crate::icons::{self, Icon};
use crate::shield::{ring_radius, ring_rect, CARD_H, CARD_RADIUS, CARD_W, CONTENT_X, RING_W};
use crate::{marquee, theme, Painter, Rect};
use plx_machine::machine::Measure;

// ---- the page tabs (spec §3.1, §3.2) ------------------------------------------------------------

/// The track's left edge, top and height: a full pill on its 64 height.
pub const TAB_TRACK_X: f32 = CONTENT_X;
pub const TAB_TRACK_Y: f32 = 32.0;
pub const TAB_TRACK_H: f32 = 64.0;
/// The pills stand 8 inside the track on every side.
pub const TAB_INSET: f32 = 8.0;
pub const TAB_PILL_H: f32 = TAB_TRACK_H - 2.0 * TAB_INSET;
/// A pill is its label plus 16 each side; pills stand 16 apart.
pub const TAB_PAD: f32 = 16.0;
pub const TAB_GAP: f32 = 16.0;
/// The selected tab's outline.
pub const TAB_RING_W: f32 = 2.0;
/// The labels' type rung (a 32 line box, ~24-26 measured).
pub const TAB_SIZE: c_int = theme::size::LABEL;
/// The band the tabs own above the page's content (`primary-navigation-top`): content starts here.
pub const TAB_BAND_BOTTOM: f32 = 128.0;

/// The pills for labels `widths` wide, their row dropped by `dy` (a page that scrolls them away
/// passes its scroll negated): the first at x 135, y 40.
pub fn tab_pills(widths: &[f32], dy: f32) -> Vec<Rect> {
    let mut x = TAB_TRACK_X + TAB_INSET;
    widths
        .iter()
        .map(|w| {
            let r = Rect::new(x, TAB_TRACK_Y + TAB_INSET + dy, w + 2.0 * TAB_PAD, TAB_PILL_H);
            x += r.w + TAB_GAP;
            r
        })
        .collect()
}

/// The track around `pills`: 8 out from the first and the last.
pub fn tab_track(pills: &[Rect]) -> Option<Rect> {
    let (first, last) = (pills.first()?, pills.last()?);
    Some(Rect::new(first.x - TAB_INSET, first.y - TAB_INSET, last.x + last.w + TAB_INSET - (first.x - TAB_INSET),
        TAB_TRACK_H))
}

/// **The tab row**: the track, then each pill — `labels` in `pills` (from [`tab_pills`]) with
/// `selected` outlined and `focused` on a white plate (a focused selected tab is simply focused).
pub fn draw_tabs(p: Painter, labels: &[&CStr], pills: &[Rect], selected: Option<usize>, focused: Option<usize>) {
    let Some(track) = tab_track(pills) else { return };
    p.rect(track, track.h * 0.5, theme::SHIELD_TAB_TRACK, theme::SHIELD_TAB_TRACK, 0.0);
    draw_pills(p, labels, pills, selected, focused);
}

/// The pills of a tab row without its track — for a row that scrolls its pills inside a track
/// it draws itself (a show page's season tabs, `shield_detail::tab_track`).
pub fn draw_pills(p: Painter, labels: &[&CStr], pills: &[Rect], selected: Option<usize>, focused: Option<usize>) {
    for (i, (label, r)) in labels.iter().zip(pills).enumerate() {
        let ink = if focused == Some(i) {
            p.rect(*r, r.h * 0.5, theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_FILL, 0.0);
            theme::SHIELD_FOCUS_INK
        } else if selected == Some(i) {
            p.rring(*r, r.h * 0.5, TAB_RING_W, theme::SHIELD_TAB_SELECTED_RING);
            theme::SHIELD_TAB_INK
        } else {
            theme::SHIELD_TAB_IDLE_INK
        };
        let ty = plx_gfx::text::text_vcenter_y(TAB_SIZE, 0, r.cy());
        p.text(label.as_ptr(), r.x + TAB_PAD, ty, TAB_SIZE, ink, 0, 0);
    }
}

// ---- the chip bar (spec §6.1) -------------------------------------------------------------------

/// The bar's top and a chip's height (a full pill), its padding and the gap between items.
pub const CHIP_Y: f32 = 132.0;
pub const CHIP_H: f32 = 48.0;
pub const CHIP_PAD: f32 = 16.0;
pub const CHIP_GAP: f32 = 16.0;
/// A chip's leading glyph (the sort direction) and the air between it and the label.
pub const CHIP_GLYPH: f32 = 24.0;
pub const CHIP_GLYPH_GAP: f32 = 16.0;
/// The labels' type rung (32 line box), and the quieter count's.
pub const CHIP_SIZE: c_int = theme::size::LABEL;
pub const COUNT_SIZE: c_int = theme::size::CAPTION;
/// The count's dark box: 32 tall, centred on the bar, its figures 8 in from each side, radius ~6.
pub const COUNT_H: f32 = 32.0;
pub const COUNT_PAD: f32 = 8.0;
pub const COUNT_RADIUS: f32 = 6.0;
/// A quiet word's line box between chips.
pub const WORD_H: f32 = 32.0;
/// The round "more" chip's glyph.
pub const ROUND_GLYPH: f32 = 24.0;

/// One item of the bar, measured: what [`chip_bar`] lays out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChipLay {
    /// A chip whose label is `w` wide, with a leading glyph or not.
    Chip { w: f32, glyph: bool },
    /// A quiet word `w` wide between chips ("by").
    Word { w: f32 },
    /// The dark item-count box around figures `w` wide.
    Count { w: f32 },
    /// The round icon chip.
    Round,
}

/// The rects of `items` along the bar from x 127, their row dropped by `dy` (a scrolled page passes
/// its scroll negated): every item 16 after the last; a word's box and the count's box are their
/// own heights, centred on the bar.
pub fn chip_bar(items: &[ChipLay], dy: f32) -> Vec<Rect> {
    let y = CHIP_Y + dy;
    let mut x = CONTENT_X;
    items
        .iter()
        .map(|item| {
            let r = match *item {
                ChipLay::Chip { w, glyph } => {
                    let lead = if glyph { CHIP_GLYPH + CHIP_GLYPH_GAP } else { 0.0 };
                    Rect::new(x, y, w + lead + 2.0 * CHIP_PAD, CHIP_H)
                }
                ChipLay::Word { w } => Rect::new(x, y + (CHIP_H - WORD_H) * 0.5, w, WORD_H),
                ChipLay::Count { w } => Rect::new(x, y + (CHIP_H - COUNT_H) * 0.5, w + 2.0 * COUNT_PAD, COUNT_H),
                ChipLay::Round => Rect::new(x, y, CHIP_H, CHIP_H),
            };
            x += r.w + CHIP_GAP;
            r
        })
        .collect()
}

/// A chip in `r`: its plate (white when focused), an optional leading glyph and its label.
pub fn draw_chip(p: Painter, r: Rect, label: &CStr, glyph: Option<Icon>, focused: bool) {
    let (fill, ink) = if focused {
        (theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_INK)
    } else {
        (theme::SHIELD_CHIP_FILL, theme::SHIELD_TAB_INK)
    };
    p.rect(r, r.h * 0.5, fill, fill, 0.0);
    let mut x = r.x + CHIP_PAD;
    if let Some(glyph) = glyph {
        icons::draw(p, glyph, Rect::new(x, r.cy() - CHIP_GLYPH * 0.5, CHIP_GLYPH, CHIP_GLYPH), ink);
        x += CHIP_GLYPH + CHIP_GLYPH_GAP;
    }
    let ty = plx_gfx::text::text_vcenter_y(CHIP_SIZE, 0, r.cy());
    p.text(label.as_ptr(), x, ty, CHIP_SIZE, ink, 0, 0);
}

/// The quiet word in `r` (from [`chip_bar`]'s `Word`).
pub fn draw_word(p: Painter, r: Rect, word: &CStr) {
    let ty = plx_gfx::text::text_vcenter_y(CHIP_SIZE, 0, r.cy());
    p.text(word.as_ptr(), r.x, ty, CHIP_SIZE, theme::SHIELD_CHIP_WORD_INK, 0, 0);
}

/// The item count in its dark box `r` (from [`chip_bar`]'s `Count`).
pub fn draw_count(p: Painter, r: Rect, figures: &CStr) {
    p.rect(r, COUNT_RADIUS, theme::SHIELD_COUNT_FILL, theme::SHIELD_COUNT_FILL, 0.0);
    let ty = plx_gfx::text::text_vcenter_y(COUNT_SIZE, 0, r.cy());
    p.text(figures.as_ptr(), r.x + COUNT_PAD, ty, COUNT_SIZE, theme::SHIELD_COUNT_INK, 0, 0);
}

/// The round icon chip in `r`.
pub fn draw_round(p: Painter, r: Rect, glyph: Icon, focused: bool) {
    draw_round_sized(p, r, glyph, ROUND_GLYPH, focused);
}

/// A round icon control in `r` with a `g`-wide glyph: the chip bar's, and the detail pages' 64
/// action circles (`shield_detail::draw_round`).
pub fn draw_round_sized(p: Painter, r: Rect, glyph: Icon, g: f32, focused: bool) {
    let (fill, ink) = if focused {
        (theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_INK)
    } else {
        (theme::SHIELD_CHIP_FILL, theme::SHIELD_TAB_INK)
    };
    p.rect(r, r.h * 0.5, fill, fill, 0.0);
    icons::draw(p, glyph, Rect::new(r.cx() - g * 0.5, r.cy() - g * 0.5, g, g), ink);
}

// ---- the captioned grid (spec §6.1, §5.5) -------------------------------------------------------

/// Six posters a row: x = 127, 410, 692, 975, 1258, 1540 (the last ends at 1780).
pub const GRID_COLS: usize = 6;
pub const GRID_LAST_X: f32 = 1540.0;
pub const GRID_PITCH_X: f32 = (GRID_LAST_X - CONTENT_X) / (GRID_COLS as f32 - 1.0);
pub const GRID_GAP: f32 = GRID_PITCH_X - CARD_W;
/// The first row's card top on an unscrolled page, and row to row.
pub const GRID_TOP: f32 = 204.0;
pub const GRID_ROW_PITCH: f32 = 460.0;
/// The band under every row's cards: the caption and the air down to the next row.
pub const GRID_CAPTION_BAND: f32 = GRID_ROW_PITCH - CARD_H;
/// The caption: a title box 32 tall 12 under the card, a year box 24 tall 44 under it.
pub const CAPTION_TITLE_DY: f32 = 12.0;
pub const CAPTION_TITLE_H: f32 = 32.0;
pub const CAPTION_SUB_DY: f32 = 44.0;
pub const CAPTION_SUB_H: f32 = 24.0;
pub const CAPTION_TITLE_SIZE: c_int = theme::size::LABEL;
pub const CAPTION_SUB_SIZE: c_int = theme::size::CAPTION;
/// Where a scrolled page holds the focused row's cards (d02: the focused row's top at y 390, the
/// row above it partly in view).
pub const GRID_FOCUS_ROW_Y: f32 = 390.0;

thread_local! {
    /// The focused caption's marquee clock: one focused card at a time on a Shield grid.
    static CAPTION_CLOCK: marquee::Clock = const { marquee::Clock::new() };
}

/// **A grid card's caption** under its RESTING rect `card`: the title box 12 under it and the year
/// 44 under it, each centred on the card. A focused card's caption drops with its pop (`s`, the
/// drawn scale) so its ring does not cross the title; an unfocused title that does not fit ends in
/// an ellipsis, a focused one glides (`ui/CLAUDE.md`: focused text that does not fit marquees).
pub fn draw_caption(p: Painter, card: Rect, label: &TileLabel, s: f32, focused: bool, measure: &dyn Measure) {
    let drop = if focused { card.h * (s - 1.0).max(0.0) * 0.5 } else { 0.0 };
    let bottom = card.y + card.h + drop;
    let lines = [
        (label.title.as_deref(), CAPTION_TITLE_DY, CAPTION_TITLE_H, CAPTION_TITLE_SIZE, theme::SHIELD_CAPTION_TITLE),
        (label.caption.as_deref(), CAPTION_SUB_DY, CAPTION_SUB_H, CAPTION_SUB_SIZE, theme::SHIELD_CAPTION_SUB),
    ];
    let widths = lines.map(|(text, _, _, size, _)| text.map_or(0.0, |t| measure.width(t, size, false)));
    let over = |i: usize| widths[i] > card.w;
    let block = focused
        .then(|| {
            let run = |i: usize| lines[i].0.map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
            marquee::Block::of((&run(0), over(0).then_some(widths[0])), (&run(1), over(1).then_some(widths[1])))
        })
        .flatten();
    if focused && block.is_none() {
        CAPTION_CLOCK.with(|c| c.release());
    }
    for (i, (text, dy, h, size, ink)) in lines.into_iter().enumerate() {
        let Some(text) = text else { continue };
        let window = Rect::new(card.x, bottom + dy, card.w, h);
        let ty = plx_gfx::text::text_vcenter_y(size, 0, window.cy());
        match block.as_ref().filter(|_| over(i)) {
            Some(block) => CAPTION_CLOCK.with(|clock| {
                let glide = Rect::new(window.x, window.y - 4.0, window.w, window.h + 8.0);
                marquee::TITLE.glide_in(clock, p, &block.key, widths[i], block.cycle_w, glide, |dx| {
                    p.text(text.as_ptr(), window.x + dx, ty, size, ink, 0, 0);
                });
            }),
            None => {
                let fit = measure.fit_line(&text.to_string_lossy(), card.w, size, false);
                p.text(fit.as_ptr(), window.cx(), ty, size, ink, 1, 0);
            }
        }
    }
}

// ---- the A–Z rail (spec §6.2) -------------------------------------------------------------------

/// The rail's column (44 wide at x 1833) and its letters' pitch.
pub const RAIL_X: f32 = 1833.0;
pub const RAIL_W: f32 = 44.0;
pub const RAIL_PITCH: f32 = 32.0;
/// The height the rail centres on (b04: "#" to "Z" spans 102..966; d02's shorter run centres alike).
pub const RAIL_CY: f32 = 534.0;
/// The rail never runs above or below these lines; a longer alphabet scrolls inside them.
pub const RAIL_TOP_MIN: f32 = 40.0;
pub const RAIL_BOTTOM_MAX: f32 = 1040.0;
/// The current letter's ring (44 across, 2px) and the letters' type rung (cap ~19 measured).
pub const RAIL_RING_D: f32 = 44.0;
pub const RAIL_RING_W: f32 = 2.0;
pub const RAIL_SIZE: c_int = theme::size::LABEL;

/// The rail for `n` letters: `(top, centre x, visible height, max scroll)`, the shape the
/// Standard rail's `rail_geom` answers in.
pub fn rail_geom(n: usize) -> (f32, f32, f32, f32) {
    let room = ((RAIL_BOTTOM_MAX - RAIL_TOP_MIN) / RAIL_PITCH).floor().max(1.0);
    let height = room.min(n as f32) * RAIL_PITCH;
    let top = (RAIL_CY - height * 0.5).max(RAIL_TOP_MIN);
    (top, RAIL_X + RAIL_W * 0.5, height, (n as f32 * RAIL_PITCH - height).max(0.0))
}

/// How a rail letter is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailLetter {
    Idle,
    /// The letter the grid's focus is under: ringed.
    Current,
    /// The remote is on it: a white disc with a black glyph.
    Focused,
}

/// One letter in its slot `slot` (the rail's width, one pitch tall).
pub fn draw_rail_letter(p: Painter, slot: Rect, label: &CStr, state: RailLetter) {
    let disc = Rect::new(slot.cx() - RAIL_RING_D * 0.5, slot.cy() - RAIL_RING_D * 0.5, RAIL_RING_D, RAIL_RING_D);
    let ink = match state {
        RailLetter::Idle => theme::SHIELD_RAIL_INK,
        RailLetter::Current => {
            p.rring(disc, disc.w * 0.5, RAIL_RING_W, theme::SHIELD_RAIL_CURRENT_RING);
            theme::SHIELD_RAIL_CURRENT_INK
        }
        RailLetter::Focused => {
            p.rect(disc, disc.w * 0.5, theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_FILL, 0.0);
            theme::SHIELD_FOCUS_INK
        }
    };
    let ty = plx_gfx::text::text_vcenter_y(RAIL_SIZE, 0, slot.cy());
    p.text(label.as_ptr(), slot.cx(), ty, RAIL_SIZE, ink, 1, 0);
}

// ---- the Categories tiles (spec §6.3) -----------------------------------------------------------

pub const TILE_W: f32 = 376.0;
pub const TILE_H: f32 = 212.0;
pub const TILE_COLS: usize = 4;
/// Four columns at x 127, 551, 975, 1399 (48 apart); rows 312 apart (100 between them).
pub const TILE_PITCH_X: f32 = 424.0;
pub const TILE_PITCH_Y: f32 = 312.0;
/// The first row's top on an unscrolled page.
pub const TILE_TOP: f32 = 140.0;
/// The focused tile's pop: 376 to ~390 across.
pub const TILE_FOCUS_SCALE: f32 = 390.0 / TILE_W;
/// The genre's name: ~32 semibold, centred.
pub const TILE_SIZE: c_int = theme::size::HEADLINE;
/// Where a scrolled page holds the focused row (the second row's place, so the row above shows).
pub const TILE_FOCUS_ROW_Y: f32 = TILE_TOP + TILE_PITCH_Y;

/// Tile `index`'s resting rect when the first row's top is at `top`.
pub fn tile_rect(index: usize, top: f32) -> Rect {
    let (row, col) = (index / TILE_COLS, index % TILE_COLS);
    Rect::new(CONTENT_X + col as f32 * TILE_PITCH_X, top + row as f32 * TILE_PITCH_Y, TILE_W, TILE_H)
}

/// The wash tile `index` wears: the palette's genres in turn, so neighbours never share one.
pub fn tile_tint(index: usize) -> [f32; 4] {
    theme::SHIELD_CATEGORY_TINTS[index % theme::SHIELD_CATEGORY_TINTS.len()]
}

/// **A genre tile** at its drawn rect `r` (popped by `s` when focused): the wash, the name centred
/// (server text: it ends in an ellipsis when it does not fit), and the card's ring on focus.
pub fn draw_tile(p: Painter, r: Rect, s: f32, name: &str, tint: [f32; 4], focused: bool, measure: &dyn Measure) {
    let rad = CARD_RADIUS * s;
    p.rect(r, rad, tint, tint, 0.0);
    let fit = measure.fit_line(name, r.w - 2.0 * crate::shield::CARD_GAP, TILE_SIZE, true);
    let ty = plx_gfx::text::text_vcenter_y(TILE_SIZE, 1, r.cy());
    p.text(fit.as_ptr(), r.cx(), ty, TILE_SIZE, theme::SHIELD_TILE_INK, 1, 1);
    if focused {
        p.rring(ring_rect(r), ring_radius(rad), RING_W, theme::SHIELD_RING);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// §3.1/§3.2, b04's dump: "Recommended" 163, "Browse" 80, "Categories" 116 wide make pills at
    /// x 135 (195 wide), 346 (112) and 474 (148), y 40, 48 tall, in a track 127..630 at y 32.
    #[test]
    fn the_tab_pills_sit_where_the_spec_measured_them() {
        let pills = tab_pills(&[163.0, 80.0, 116.0], 0.0);
        assert_eq!(pills, vec![
            Rect::new(135.0, 40.0, 195.0, 48.0),
            Rect::new(346.0, 40.0, 112.0, 48.0),
            Rect::new(474.0, 40.0, 148.0, 48.0),
        ]);
        let track = tab_track(&pills).unwrap();
        assert_eq!(track, Rect::new(127.0, 32.0, 503.0, 64.0), "503 wide for Recommended/Browse/Categories");
        assert_eq!(track.h * 0.5, 32.0, "a full pill");
        assert_eq!(pills[0].x + pills[0].w + TAB_GAP, pills[1].x, "16 between pills");
        assert!(tab_pills(&[], 0.0).is_empty() && tab_track(&[]).is_none());
        let scrolled = tab_pills(&[163.0], -100.0);
        assert_eq!(scrolled[0].y, -60.0, "a scrolled page carries its tabs away");
    }

    /// §6.1, b04: "All" at 127 (58 wide), "Movies" at 201 (109), "by" at 326, the sort chip at 369
    /// (119 with its glyph), the count box at 504 (32 tall at y 140), the round chip at ~629.
    #[test]
    fn the_chip_bar_lays_out_on_the_measured_spacing() {
        let bar = chip_bar(&[
            ChipLay::Chip { w: 26.0, glyph: false },
            ChipLay::Chip { w: 77.0, glyph: false },
            ChipLay::Word { w: 27.0 },
            ChipLay::Chip { w: 47.0, glyph: true },
            ChipLay::Count { w: 93.0 },
            ChipLay::Round,
        ], 0.0);
        assert_eq!((bar[0].x, bar[0].y, bar[0].w, bar[0].h), (127.0, 132.0, 58.0, 48.0));
        assert_eq!((bar[1].x, bar[1].w), (201.0, 109.0));
        assert_eq!(bar[2].x, 326.0);
        assert_eq!((bar[3].x, bar[3].w), (369.0, 119.0), "16 + glyph 24 + 16 + label 47 + 16");
        assert_eq!((bar[4].x, bar[4].y, bar[4].w, bar[4].h), (504.0, 140.0, 109.0, 32.0));
        assert_eq!((bar[5].x, bar[5].w, bar[5].h), (629.0, 48.0, 48.0));
        assert_eq!(bar[2].cy(), bar[0].cy(), "the word sits on the chips' centre line");
    }

    /// §6.1: six columns from x 127 on a ~282.6 pitch to 1780; rows at 204 and 664; the caption's
    /// title at +12 and year at +44 under a card.
    #[test]
    fn the_grid_columns_rows_and_caption_are_the_measured_ones() {
        let xs: Vec<f32> = (0..GRID_COLS).map(|c| CONTENT_X + c as f32 * GRID_PITCH_X).collect();
        for (x, measured) in xs.iter().zip([127.0, 410.0, 692.0, 975.0, 1258.0, 1540.0]) {
            assert!((x - measured).abs() < 1.0, "column at {x} vs measured {measured}");
        }
        assert!((GRID_PITCH_X - 282.6).abs() < 0.01);
        assert_eq!(GRID_LAST_X + CARD_W, 1780.0);
        assert_eq!(GRID_TOP + GRID_ROW_PITCH, 664.0);
        assert_eq!(GRID_CAPTION_BAND, 100.0);
        assert_eq!(GRID_TOP + CARD_H + CAPTION_TITLE_DY, 576.0);
        assert_eq!(GRID_TOP + CARD_H + CAPTION_SUB_DY, 608.0);
        assert!(CAPTION_SUB_DY + CAPTION_SUB_H < GRID_CAPTION_BAND, "the year ends above the next row");
        assert!(GRID_LAST_X + CARD_W < RAIL_X, "the grid stops short of the rail");
    }

    /// §6.2, b04: 27 letters (# and A–Z) span 102..966 on a 32 pitch in the 44 column at 1833;
    /// a shorter alphabet centres on the same line (d02); a longer one scrolls inside the frame.
    #[test]
    fn the_rail_centres_its_letters_on_the_measured_column() {
        let (top, cx, h, max) = rail_geom(27);
        assert_eq!((top, h, max), (102.0, 864.0, 0.0));
        assert_eq!(top + h, 966.0);
        assert_eq!(cx, 1855.0);
        assert_eq!(RAIL_X + RAIL_W, 1877.0);
        let (top, _, h, _) = rail_geom(16);
        assert_eq!(top + h * 0.5, RAIL_CY, "a short alphabet centres on the same line");
        let (top, _, h, max) = rail_geom(60);
        assert!(top >= RAIL_TOP_MIN && top + h <= RAIL_BOTTOM_MAX && max > 0.0);
    }

    /// §6.3, b08: four columns at 127, 551, 975, 1399; rows at 140, 452, 764; 376x212 tiles.
    #[test]
    fn the_category_tiles_sit_on_the_measured_pitch() {
        let xs: Vec<f32> = (0..4).map(|i| tile_rect(i, TILE_TOP).x).collect();
        assert_eq!(xs, [127.0, 551.0, 975.0, 1399.0]);
        let ys: Vec<f32> = [0, 4, 8].iter().map(|&i| tile_rect(i, TILE_TOP).y).collect();
        assert_eq!(ys, [140.0, 452.0, 764.0]);
        assert_eq!((tile_rect(5, TILE_TOP).w, tile_rect(5, TILE_TOP).h), (376.0, 212.0));
        let popped = tile_rect(0, TILE_TOP).scaled(TILE_FOCUS_SCALE);
        assert!((popped.w - 390.0).abs() < 0.01);
        assert_ne!(tile_tint(0), tile_tint(1), "neighbours never share a wash");
        assert_eq!(tile_tint(0), tile_tint(theme::SHIELD_CATEGORY_TINTS.len()));
    }

    /// Every wash carries the tile's white bold name above the 3:1 large-text floor.
    #[test]
    fn every_category_tint_carries_white_type() {
        for tint in theme::SHIELD_CATEGORY_TINTS {
            let ratio = crate::widgets::contrast(theme::SHIELD_TILE_INK, tint);
            assert!(ratio >= 3.0, "{tint:?}: {ratio:.2}:1");
        }
    }
}
