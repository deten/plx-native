//! **The Shield layout's detail pages** (`layout::shield()`; SHIELD-LAYOUT-SPEC §4.2, §4.3, §5.3–§5.5,
//! §7): the movie, show and season pages' header column, action row, media chips, section headings
//! and the season page's landscape episode strip. `shield` is the header type and the ground these
//! pages share with Home; `shield_browse` the tab pills a show page carries; `shield_panel` the More
//! Actions panel.
//!
//! Every number is measured off the Plex app on Android TV at 1920x1080, the logical space this app
//! draws in, so they transfer 1:1 and are pinned by this module's tests. Geometry and paint only:
//! what a button does, which episode the header describes and where the page has scrolled to are
//! the page's (`screens::detail`'s `shield`).
//!
//! - **The column** starts at [`X`] (the detail dumps put it at 128, one right of Home's 127).
//! - **The header** ([`header_plan`], [`draw_header`]): the logo box and facts row Home's header
//!   draws (`shield::draw_title_block`), a ratings row under them, a three-line summary and a quiet
//!   "Directed by" line; the action row follows [`BUTTONS_GAP`] under the last line.
//! - **The action row** ([`action_row`], [`draw_pill`], [`draw_round`]): a 64-tall primary pill
//!   (glyph and label) and 64 circles 32 apart. The pill shows its label only while the row holds
//!   focus; otherwise it is a circle too and the row closes up behind it (`open`).
//! - **The media chips** ([`chip_rects`], [`draw_media_chip`]): small dark plates right-aligned on
//!   the action row's line — the version, the audio track, the subtitle track.
//! - **Sections**: a 48-tall bold heading, the cards [`HEADING_DY`] under it, the next heading
//!   [`SECTION_GAP`] under the block; a focused section's heading is brought to [`FOCUS_HEADING_Y`].
//! - **The season strip** ([`episode_rect`], [`strip_scroll`], [`draw_episode_badge`]): 512x288
//!   stills on a 544 pitch at y 570, focus held in the first slot until the last still reaches the
//!   right edge, each wearing its episode number in a dark corner plate.

use std::ffi::CStr;
use std::os::raw::c_int;

use crate::icons::{self, Icon};
use crate::label::{Label, VAlign};
use crate::shield::{HeroFacts, SUMMARY_LINE, SUMMARY_W};
use crate::text_view::TextView;
use crate::{theme, Painter, Rect};
use plx_machine::machine::Measure;

// ---- the column (spec §1.2, §7.1) ---------------------------------------------------------------

/// The detail pages' content edge: the rail owns 0..128.
pub const X: f32 = 128.0;
/// The content's right edge: the Standard 96 margin (the subtitle chip ends here).
pub const RIGHT: f32 = 1824.0;

// ---- the header (spec §4.2, §4.3) ---------------------------------------------------------------

/// The ratings row: 40 tall at y 340, under the facts row.
pub const RATINGS_Y: f32 = 340.0;
pub const RATINGS_H: f32 = 40.0;
/// The air between one rating group and the next (value to the next mark).
pub const RATINGS_GAP: f32 = 16.0;
/// The summary: three 32px lines 820 wide, at y 404 under a ratings row, 392 with none (the season
/// page's episode, which has no ratings of its own).
pub const SUMMARY_Y: f32 = 404.0;
pub const SUMMARY_Y_BARE: f32 = 392.0;
pub const SUMMARY_LINES: usize = 3;
pub const SUMMARY_SIZE: c_int = theme::size::LABEL;
/// The facts row's foot, for a page with neither ratings nor a summary to stand the buttons on.
pub const META_BOTTOM: f32 = crate::shield::META_Y + crate::shield::META_H;
/// The "Directed by" line: one 28 line, 12 under the summary, quiet ink.
pub const DIRECTOR_GAP: f32 = 12.0;
pub const DIRECTOR_H: f32 = 28.0;
pub const DIRECTOR_W: f32 = crate::shield::CAST_W;
pub const DIRECTOR_SIZE: c_int = theme::size::CAPTION;
/// The action row stands this far under the header's last line.
pub const BUTTONS_GAP: f32 = 24.0;

/// Where a header's lines and the action row under it fall, from what the header has.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeaderPlan {
    pub summary_y: f32,
    pub summary_lines: usize,
    /// The director line's top, when there is one.
    pub director_y: Option<f32>,
    /// The action row's top.
    pub buttons_y: f32,
}

/// The header's plan: a ratings row or not, the summary's line count (at most
/// [`SUMMARY_LINES`]; 0 for none), a director line or not.
pub fn header_plan(ratings: bool, summary_lines: usize, director: bool) -> HeaderPlan {
    let summary_y = if ratings { SUMMARY_Y } else { SUMMARY_Y_BARE };
    let lines = summary_lines.min(SUMMARY_LINES);
    let mut bottom = if lines > 0 {
        summary_y + lines as f32 * SUMMARY_LINE
    } else if ratings {
        RATINGS_Y + RATINGS_H
    } else {
        META_BOTTOM
    };
    let director_y = director.then(|| {
        let y = bottom + DIRECTOR_GAP;
        bottom = y + DIRECTOR_H;
        y
    });
    HeaderPlan { summary_y, summary_lines: lines, director_y, buttons_y: bottom + BUTTONS_GAP }
}

/// How many lines `summary` takes in the header's column (capped at [`SUMMARY_LINES`]).
pub fn summary_lines(summary: &str, measure: &dyn Measure) -> usize {
    if summary.is_empty() {
        return 0;
    }
    let h = summary_view(summary, measure).measure_h(SUMMARY_W);
    ((h / SUMMARY_LINE).round() as usize).clamp(1, SUMMARY_LINES)
}

fn summary_view<'a>(summary: &'a str, measure: &'a dyn Measure) -> TextView<'a> {
    TextView::new(summary, SUMMARY_SIZE, theme::SHIELD_HERO_TEXT)
        .with_measure(measure)
        .leading(SUMMARY_LINE)
        .max_lines(SUMMARY_LINES)
}

/// **The header** through `p` (its alpha is the subject cross-fade): Home's title block at [`X`]
/// (logo or title, the subtitle line, the facts and the content-rating plate), then the summary and
/// the director line where `plan` puts them. The ratings row between is the page's (it is the
/// app's own rating marks, `widgets::rating_group`). Every run is server text and elides.
pub fn draw_header(p: Painter, facts: &HeroFacts<'_>, director: &str, plan: &HeaderPlan, measure: &dyn Measure) {
    crate::shield::draw_title_block(p, facts, X, measure);
    if !facts.summary.is_empty() && plan.summary_lines > 0 {
        let cap_top = plan.summary_y + (SUMMARY_LINE - measure.cap_h(SUMMARY_SIZE)) * 0.5;
        summary_view(facts.summary, measure).draw(p, Rect::new(X, cap_top, SUMMARY_W, 0.0));
    }
    if let (Some(y), false) = (plan.director_y, director.is_empty()) {
        let line = plx_gfx::text::elide_by(director, DIRECTOR_W, false, |t| measure.width_str(t, DIRECTOR_SIZE, false));
        if let Ok(c) = std::ffi::CString::new(line) {
            Label::new(c.as_ptr(), DIRECTOR_SIZE, theme::SHIELD_HERO_CAST).v(VAlign::Middle)
                .draw(p, Rect::new(X, y, DIRECTOR_W, DIRECTOR_H));
        }
    }
}

// ---- the action row (spec §7.1, §9.1) -----------------------------------------------------------

/// A button's height, and a round button's diameter.
pub const BUTTON_H: f32 = 64.0;
/// Between two buttons: a 96 pitch for the circles.
pub const BUTTON_GAP: f32 = 32.0;
/// The pill's glyph (32, 20 in from its left end), its label 60 in, and 16 after the label.
pub const PILL_GLYPH_X: f32 = 20.0;
pub const PILL_GLYPH: f32 = 32.0;
pub const PILL_LABEL_X: f32 = 60.0;
pub const PILL_TRAIL: f32 = 16.0;
pub const PILL_SIZE: c_int = theme::size::LABEL;
/// A round button's glyph.
pub const ROUND_GLYPH: f32 = 32.0;

/// The open pill's width around a label `label_w` wide (Resume • 36m left: 248).
pub fn pill_w(label_w: f32) -> f32 {
    PILL_LABEL_X + label_w + PILL_TRAIL
}

/// **The row at `y`**: the primary pill, `open` of the way from a circle (0) to its full width
/// around a label `label_w` wide (1), then `rounds` circles, each [`BUTTON_GAP`] after the last.
pub fn action_row(y: f32, label_w: f32, open: f32, rounds: usize) -> Vec<Rect> {
    let pill = BUTTON_H + (pill_w(label_w) - BUTTON_H).max(0.0) * open.clamp(0.0, 1.0);
    let mut out = Vec::with_capacity(rounds + 1);
    out.push(Rect::new(X, y, pill, BUTTON_H));
    let mut x = X + pill + BUTTON_GAP;
    for _ in 0..rounds {
        out.push(Rect::new(x, y, BUTTON_H, BUTTON_H));
        x += BUTTON_H + BUTTON_GAP;
    }
    out
}

/// The fill and ink of a control: the white plate with black ink when focused, the idle white 10%
/// with white ink otherwise.
fn control_inks(focused: bool) -> ([f32; 4], [f32; 4]) {
    if focused {
        (theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_INK)
    } else {
        (theme::SHIELD_CHIP_FILL, theme::SHIELD_TAB_INK)
    }
}

/// **The primary pill** in `r` (from [`action_row`]): its plate, its glyph sliding from the circle's
/// centre to the pill's lead as it opens, and its label, which the CALLER clips to `r` (a pill on
/// its way shut shows the part of the label still inside it; this module holds no scissor).
pub fn draw_pill(p: Painter, r: Rect, glyph: Icon, label: &CStr, open: f32, focused: bool) {
    let (fill, ink) = control_inks(focused);
    p.rect(r, r.h * 0.5, fill, fill, 0.0);
    let closed_x = (BUTTON_H - PILL_GLYPH) * 0.5;
    let gx = r.x + closed_x + (PILL_GLYPH_X - closed_x) * open.clamp(0.0, 1.0);
    icons::draw(p, glyph, Rect::new(gx, r.cy() - PILL_GLYPH * 0.5, PILL_GLYPH, PILL_GLYPH), ink);
    if open > 0.01 {
        let ty = plx_gfx::text::text_vcenter_y(PILL_SIZE, 0, r.cy());
        p.alpha(open.clamp(0.0, 1.0)).text(label.as_ptr(), r.x + PILL_LABEL_X, ty, PILL_SIZE, ink, 0, 0);
    }
}

/// A round button in `r`: the control plate and its glyph.
pub fn draw_round(p: Painter, r: Rect, glyph: Icon, focused: bool) {
    crate::shield_browse::draw_round_sized(p, r, glyph, ROUND_GLYPH, focused);
}

// ---- the media chips (spec §7.1) ----------------------------------------------------------------

/// A chip: 32 tall, its text 8 in from either end after an optional 24 glyph, 20 between chips,
/// radius ~6; its top 22 under the action row's.
pub const CHIP_H: f32 = 32.0;
pub const CHIP_PAD: f32 = 8.0;
pub const CHIP_GLYPH: f32 = 24.0;
pub const CHIP_GAP: f32 = 20.0;
pub const CHIP_DY: f32 = 22.0;
pub const CHIP_RADIUS: f32 = 6.0;
pub const CHIP_SIZE: c_int = theme::size::CAPTION;

/// A chip's width around a label `label_w` wide.
pub fn chip_w(label_w: f32, glyph: bool) -> f32 {
    label_w + 2.0 * CHIP_PAD + if glyph { CHIP_GLYPH } else { 0.0 }
}

/// The chips `widths` wide (from [`chip_w`]), left to right, their row right-aligned on [`RIGHT`]
/// with its top at `y`.
pub fn chip_rects(widths: &[f32], y: f32) -> Vec<Rect> {
    let mut right = RIGHT;
    let mut out: Vec<Rect> = widths
        .iter()
        .rev()
        .map(|&w| {
            let r = Rect::new(right - w, y, w, CHIP_H);
            right = r.x - CHIP_GAP;
            r
        })
        .collect();
    out.reverse();
    out
}

/// A media chip in `r`: the dark plate (white when focused), an optional leading glyph, the label.
pub fn draw_media_chip(p: Painter, r: Rect, glyph: Option<Icon>, label: &CStr, focused: bool) {
    let (fill, ink) = if focused {
        (theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_INK)
    } else {
        (theme::SHIELD_COUNT_FILL, theme::SHIELD_COUNT_INK)
    };
    p.rect(r, CHIP_RADIUS, fill, fill, 0.0);
    let mut x = r.x + CHIP_PAD;
    if let Some(glyph) = glyph {
        let g = CHIP_GLYPH * 0.75;
        icons::draw(p, glyph, Rect::new(x + (CHIP_GLYPH - g) * 0.5 - 2.0, r.cy() - g * 0.5, g, g), ink);
        x += CHIP_GLYPH;
    }
    let ty = plx_gfx::text::text_vcenter_y(CHIP_SIZE, 0, r.cy());
    p.text(label.as_ptr(), x, ty, CHIP_SIZE, ink, 0, 0);
}

// ---- sections (spec §5.4, §5.5, §7.1) -----------------------------------------------------------

/// The air from the action row's foot to the first heading, and from a block's foot to the next.
pub const SECTION_GAP: f32 = 48.0;
/// A heading's box, and its top to its cards' top.
pub const HEADING_H: f32 = 48.0;
pub const HEADING_DY: f32 = 64.0;
pub const HEADING_SIZE: c_int = theme::size::HEADLINE;
/// Where a focused section's heading is brought to (e03: the page moves up 282, Cast & Crew at 394).
pub const FOCUS_HEADING_Y: f32 = 394.0;
/// The Cast & Crew circles and the block under each: the name 32 tall 12 under the circle, the
/// role 24 tall 44 under it.
pub const CAST_D: f32 = 240.0;
pub const CAST_BLOCK: f32 = 308.0;
/// The two caption lines under a card (title +12, the quiet line +44 and 24 tall).
pub const CAPTION_BAND: f32 = crate::shield_browse::CAPTION_SUB_DY + crate::shield_browse::CAPTION_SUB_H;

/// A section's block height: its heading, its cards and what stands under them.
pub fn section_block(card_h: f32, under: f32) -> f32 {
    HEADING_DY + card_h + under
}

/// The scroll that brings a section headed at document y `top` to [`FOCUS_HEADING_Y`].
pub fn scroll_to_heading(top: f32) -> f32 {
    (top - FOCUS_HEADING_Y).max(0.0)
}

/// The draw-y of a heading's title in its box at `top` (what a linked heading takes as its `y`).
pub fn heading_text_y(top: f32) -> f32 {
    plx_gfx::text::text_vcenter_y(HEADING_SIZE, 1, top + HEADING_H * 0.5)
}

/// A plain section heading at `top` (app text: it never elides).
pub fn draw_heading(p: Painter, title: &CStr, top: f32) {
    p.text(title.as_ptr(), X, heading_text_y(top), HEADING_SIZE, theme::SHIELD_HEADING, 0, 1);
}

// ---- the season page (spec §7.3, §5.3) ----------------------------------------------------------

/// The season page's fixed bands: the chips at 512 over the strip at 570, the action row at 898
/// under it, the Cast & Crew heading at 978.
pub const STRIP_CHIP_Y: f32 = 512.0;
pub const STRIP_Y: f32 = 570.0;
pub const STRIP_ACTION_Y: f32 = 898.0;
pub const STRIP_SECTIONS_Y: f32 = 978.0;
/// An episode still and the strip's pitch; its pop (512 to ~528 across).
pub const EP_W: f32 = 512.0;
pub const EP_H: f32 = 288.0;
pub const EP_GAP: f32 = 32.0;
pub const EP_PITCH: f32 = EP_W + EP_GAP;
pub const EP_FOCUS_SCALE: f32 = 528.0 / EP_W;
/// The episode-number plate: 40 tall in the still's top-right corner, its figures padded 8 a side
/// (at least square), a check before them on a watched episode.
pub const EP_BADGE_H: f32 = 40.0;
pub const EP_BADGE_PAD: f32 = 8.0;
pub const EP_BADGE_GLYPH: f32 = 24.0;
pub const EP_BADGE_SIZE: c_int = theme::size::CAPTION;

/// The strip's scroll with episode `i` of `n` focused: the focused still in the first slot, until
/// the last still has reached the right edge (17-show-detail: E10 of 10 rests at x 1312).
pub fn strip_scroll(i: usize, n: usize) -> f32 {
    let run = (n as f32 * EP_PITCH - EP_GAP - (RIGHT - X)).max(0.0);
    (i as f32 * EP_PITCH).min(run)
}

/// Episode `i`'s resting still with the strip's top at `top`, scrolled by `scroll`.
pub fn episode_rect(i: usize, top: f32, scroll: f32) -> Rect {
    Rect::new(X + i as f32 * EP_PITCH - scroll, top, EP_W, EP_H)
}

/// The plate's width around figures `text_w` wide.
pub fn episode_badge_w(text_w: f32, watched: bool) -> f32 {
    let glyph = if watched { EP_BADGE_GLYPH + EP_BADGE_PAD } else { 0.0 };
    (text_w + 2.0 * EP_BADGE_PAD + glyph).max(EP_BADGE_H)
}

/// The episode-number plate on a still drawn at `card` (popped by `s`): the dark corner plate, its
/// outer corner on the card's radius, a check on a watched episode, the figures.
pub fn draw_episode_badge(p: Painter, card: Rect, s: f32, label: &CStr, watched: bool, measure: &dyn Measure) {
    let text_w = measure.width(label, EP_BADGE_SIZE, false);
    let w = episode_badge_w(text_w, watched) * s;
    let h = EP_BADGE_H * s;
    let r = Rect::new(card.x + card.w - w, card.y, w, h);
    let rad = crate::shield::CARD_RADIUS * s;
    p.rrect(r, crate::shield::BADGE_INNER_RADIUS * s, rad, theme::SHIELD_BADGE_FILL);
    let mut x = r.x + (r.w - (text_w + if watched { EP_BADGE_GLYPH + EP_BADGE_PAD } else { 0.0 }) * s) * 0.5;
    if watched {
        let g = ((EP_BADGE_GLYPH * s) / 4.0).round() * 4.0;
        icons::draw(p, Icon::Check, Rect::new(x, r.cy() - g * 0.5, g, g), theme::SHIELD_BADGE_INK);
        x += (EP_BADGE_GLYPH + EP_BADGE_PAD) * s;
    }
    let ty = plx_gfx::text::text_vcenter_y(EP_BADGE_SIZE, 0, r.cy());
    p.text(label.as_ptr(), x, ty, EP_BADGE_SIZE, theme::SHIELD_BADGE_INK, 0, 0);
}

// ---- the show page's tabs (spec §3.2, §7.2) -----------------------------------------------------

/// The widest the show page's tab track grows before its pills scroll inside it (17-show-detail:
/// 128..946, the summary's column).
pub const TAB_TRACK_MAX_W: f32 = SUMMARY_W;

/// The scroll (≥ 0) that keeps pill `focus` of `pills` (laid out unscrolled) inside a track no
/// wider than [`TAB_TRACK_MAX_W`], moving as little as possible from `current`.
pub fn tab_scroll(pills: &[Rect], focus: usize, current: f32) -> f32 {
    let (Some(first), Some(last), Some(pill)) = (pills.first(), pills.last(), pills.get(focus)) else { return 0.0 };
    let inset = crate::shield_browse::TAB_INSET;
    let window_l = first.x;
    let window_r = first.x - inset + TAB_TRACK_MAX_W - inset;
    let max = (last.x + last.w - window_r).max(0.0);
    let mut s = current.clamp(0.0, max);
    if pill.x + pill.w - s > window_r {
        s = pill.x + pill.w - window_r;
    }
    if pill.x - s < window_l {
        s = pill.x - window_l;
    }
    s.clamp(0.0, max)
}

/// The track around `pills` (unscrolled), capped at [`TAB_TRACK_MAX_W`].
pub fn tab_track(pills: &[Rect]) -> Option<Rect> {
    let mut track = crate::shield_browse::tab_track(pills)?;
    track.w = track.w.min(TAB_TRACK_MAX_W);
    Some(track)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::FixtureMeasure;

    /// §4.2/§7.1, e01: a movie with ratings, a three-line summary and a director: summary at 404,
    /// "Directed by" at 512, the buttons at 564; the show page (d03, no director) at 524; the season
    /// page's episode (17, no ratings) has its summary at 392 and its director at 500.
    #[test]
    fn the_header_lines_and_the_buttons_follow_the_measured_column() {
        let movie = header_plan(true, 3, true);
        assert_eq!((movie.summary_y, movie.director_y, movie.buttons_y), (404.0, Some(512.0), 564.0));
        let show = header_plan(true, 3, false);
        assert_eq!((show.director_y, show.buttons_y), (None, 524.0));
        let episode = header_plan(false, 3, true);
        assert_eq!((episode.summary_y, episode.director_y), (392.0, Some(500.0)));
        assert_eq!(header_plan(true, 7, false).summary_lines, 3, "never more than three lines");
        assert_eq!(header_plan(true, 0, false).buttons_y, RATINGS_Y + RATINGS_H + BUTTONS_GAP);
        assert_eq!(header_plan(false, 0, false).buttons_y, META_BOTTOM + BUTTONS_GAP);
        assert_eq!(summary_lines("", &FixtureMeasure), 0);
        assert_eq!(summary_lines("Short.", &FixtureMeasure), 1);
        assert_eq!(summary_lines(&"word ".repeat(400), &FixtureMeasure), 3);
    }

    /// §7.1, e01/e02: "Resume • 36m left" (172 wide) makes a 248 pill at 128, the circles at 408,
    /// 504, 600, 696, 792; with focus off the row the pill is a 64 circle and they close up to 224,
    /// 320, ... (d03: "Watch S2 E1", 116 wide, a 192 pill and circles from 352).
    #[test]
    fn the_action_row_is_the_measured_pill_and_circles() {
        let open = action_row(564.0, 172.0, 1.0, 5);
        assert_eq!(open[0], Rect::new(128.0, 564.0, 248.0, 64.0));
        let xs: Vec<f32> = open[1..].iter().map(|r| r.x).collect();
        assert_eq!(xs, [408.0, 504.0, 600.0, 696.0, 792.0]);
        assert!(open[1..].iter().all(|r| r.w == 64.0 && r.h == 64.0), "the circles are 64 across");
        let shut = action_row(564.0, 172.0, 0.0, 5);
        assert_eq!(shut[0], Rect::new(128.0, 564.0, 64.0, 64.0));
        let xs: Vec<f32> = shut.iter().map(|r| r.x).collect();
        assert_eq!(xs, [128.0, 224.0, 320.0, 416.0, 512.0, 608.0]);
        let show = action_row(524.0, 116.0, 1.0, 4);
        assert_eq!((show[0].w, show[1].x), (192.0, 352.0));
        let half = action_row(564.0, 172.0, 0.5, 1);
        assert!(half[0].w > 64.0 && half[0].w < 248.0, "the pill opens continuously");
    }

    /// §7.1, e01: video 294, audio 186 (a glyph and 146 of text), subtitles 69 (a glyph and 29),
    /// 20 apart, the last ending on the 1824 edge; 32 tall, 22 under the buttons' top.
    #[test]
    fn the_media_chips_hang_right_aligned_on_the_action_row() {
        assert_eq!(chip_w(146.0, true), 186.0);
        assert_eq!(chip_w(29.0, true), 69.0);
        let rects = chip_rects(&[294.0, 186.0, 69.0], 564.0 + CHIP_DY);
        assert_eq!(rects[2], Rect::new(1755.0, 586.0, 69.0, 32.0));
        assert_eq!(rects[1].x, 1549.0);
        assert_eq!(rects[0].x, 1235.0);
        assert_eq!(rects[0].x + rects[0].w + CHIP_GAP, rects[1].x);
        assert!(chip_rects(&[], 0.0).is_empty());
    }

    /// §7.1, §5.4: Cast & Crew's heading 48 under the buttons' foot (676), its circles 64 under
    /// that (740), the block 308 tall, the next heading at 1096; a focused section is brought to 394.
    #[test]
    fn sections_stack_on_the_measured_pitch() {
        let buttons_foot = 564.0 + BUTTON_H;
        let heading = buttons_foot + SECTION_GAP;
        assert_eq!(heading, 676.0);
        assert_eq!(heading + HEADING_DY, 740.0);
        let cast = section_block(CAST_D, CAST_BLOCK - CAST_D);
        assert_eq!(heading + cast + SECTION_GAP, 1096.0);
        assert_eq!(scroll_to_heading(heading), 282.0, "e03: the page moves up 282");
        assert_eq!(scroll_to_heading(200.0), 0.0);
        assert_eq!(CAPTION_BAND, 68.0);
    }

    /// §7.3, §5.3: stills 512x288 on a 544 pitch at y 570; the focused one rests in the first slot
    /// until the strip's end reaches the right edge (17: E10 of 10 at 1312, E9 at 768, E8 at 224).
    #[test]
    fn the_season_strip_holds_focus_left_until_its_end() {
        assert_eq!(episode_rect(0, STRIP_Y, 0.0), Rect::new(128.0, 570.0, 512.0, 288.0));
        assert_eq!(episode_rect(1, STRIP_Y, 0.0).x, 672.0);
        assert_eq!(strip_scroll(2, 10), 2.0 * 544.0);
        let s = strip_scroll(9, 10);
        assert_eq!(episode_rect(9, STRIP_Y, s).x, 1312.0);
        assert_eq!(episode_rect(8, STRIP_Y, s).x, 768.0);
        assert_eq!(episode_rect(7, STRIP_Y, s).x, 224.0);
        assert_eq!(strip_scroll(1, 2), 0.0, "a strip that fits never scrolls");
        let popped = episode_rect(0, STRIP_Y, 0.0).scaled(EP_FOCUS_SCALE);
        assert!((popped.w - 528.0).abs() < 1e-3);
        assert_eq!(STRIP_ACTION_Y - (STRIP_Y + EP_H), 40.0);
        assert_eq!(STRIP_SECTIONS_Y, 978.0);
    }

    /// §5.2: the plate is 40 tall, at least square, the check and its air (32) before the figures
    /// on a watched episode (d04: "✓ E1" is 67 wide around 19 of figures).
    #[test]
    fn the_episode_plate_fits_its_figures() {
        assert_eq!(episode_badge_w(19.0, true), 67.0);
        assert_eq!(episode_badge_w(19.0, false), 40.0);
        assert_eq!(episode_badge_w(40.0, false), 56.0);
    }

    /// §3.2, 17: a long tab row scrolls inside a track no wider than the summary's column, just
    /// far enough to keep focus inside it.
    #[test]
    fn a_long_tab_row_scrolls_inside_its_track() {
        let pills = crate::shield_browse::tab_pills(&[110.0; 12], 0.0);
        let track = tab_track(&pills).unwrap();
        assert_eq!((track.x, track.w), (127.0, TAB_TRACK_MAX_W));
        assert_eq!(tab_scroll(&pills, 0, 0.0), 0.0);
        let s = tab_scroll(&pills, 11, 0.0);
        assert!(s > 0.0 && pills[11].x + pills[11].w - s <= track.x + track.w - 8.0 + 0.01);
        assert_eq!(tab_scroll(&pills, 0, s), 0.0, "back to the first");
        let short = crate::shield_browse::tab_pills(&[60.0, 95.0, 100.0], 0.0);
        assert_eq!(tab_scroll(&short, 2, 0.0), 0.0, "a row that fits never scrolls");
    }
}
