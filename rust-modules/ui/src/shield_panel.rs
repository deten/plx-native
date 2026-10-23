//! **The Shield layout's right-side panel** (`layout::shield()`; SHIELD-LAYOUT-SPEC §6.4): *Sort
//! by*, *Filter by* and the other list panels a chip opens, standing at a fixed frame on the right
//! over the page's 50% dim. The page keeps its geometry under it (§9.1: panels overlay, nothing
//! reflows).
//!
//! It lays out and paints the SAME [`Section`]/[`Row`] model a `table::TableView` menu holds, so a
//! menu keeps one description of its rows — their labels, check marks, switches, values, trailing
//! glyphs and separators — and only its presentation follows the layout: here full-width pill rows
//! on a 72 pitch (76 for a list of leading glyphs), the focused row a white plate with black ink,
//! the current choice's mark or the sort's direction glyph at the row's trailing edge, a later
//! group under its own grey title, and a list taller than the frame scrolling as a whole to keep
//! focus inside it ([`scroll_for`]), clipped at the list window. Geometry and paint only: what a
//! row does is the menu's.
//!
//! Row indexing is the table's own (every row of every section, separators included, headers not),
//! so a menu's `FormTable` keys resolve to the same index in either presentation.
//!
//! **The player's panels** (Subtitles, Settings and the pages behind it; j01, j03-j05, k04) stand at
//! the same frame under the same title, in a second [`Kind`]: rows on a 64 pitch with no gap (88 for
//! a two-line row), a one-of-several choice marked by a radio circle at the trailing edge (the
//! current one filled with the accent), a switch drawn as a switch rather than the word `On`/`Off`,
//! a track's badges spoken in its label ("English (SRT)") and no drill-in chevrons. The trailing
//! mark of a two-line row centres on the whole row, its value on the label's line.

use std::os::raw::c_int;

use crate::icons::{self, Icon};
use crate::table::{Row, Section};
use crate::{theme, Painter, Rect};
use plx_machine::machine::Measure;

/// The panel's fixed frame: 640 wide at x 1248, 32 in from the top and the bottom.
pub const PANEL: Rect = Rect::new(1248.0, 32.0, 640.0, 1016.0);
pub const PANEL_RADIUS: f32 = 16.0;
pub const PANEL_EDGE_W: f32 = 2.0;
/// The title's box: 66 in from the panel's left, 48 tall at y 98.
pub const TITLE: Rect = Rect::new(1314.0, 98.0, 508.0, 48.0);
pub const TITLE_SIZE: c_int = theme::size::HEADLINE;
/// The item column (the focused plate's span) and where the list starts.
pub const ITEM_X: f32 = 1314.0;
pub const ITEM_W: f32 = 508.0;
pub const LIST_TOP: f32 = 210.0;
/// The lowest a row's bottom may sit before the list scrolls: 34 above the frame's bottom.
pub const LIST_BOTTOM: f32 = PANEL.y + PANEL.h - 34.0;
/// A row (64 tall; 96 with a second line), and the gap under it: a 72 pitch.
pub const ITEM_H: f32 = 64.0;
pub const ITEM_H_TALL: f32 = 96.0;
pub const ITEM_GAP: f32 = 8.0;
/// A list of leading glyphs breathes a little more: a 76 pitch.
pub const ITEM_GAP_GLYPHS: f32 = 12.0;
/// A separator's hairline, with an item gap either side of it.
pub const SEP_H: f32 = 2.0;
/// A later group's title: 32 tall, 16 under the row above, 8 above its first row.
pub const GROUP_H: f32 = 32.0;
pub const GROUP_AIR: f32 = 16.0;
/// The label 24 into the plate; a leading glyph (~32) there with its label 40 after it; the
/// trailing mark or value 24 in from the plate's right end.
pub const LABEL_INSET: f32 = 24.0;
pub const GLYPH: f32 = 32.0;
pub const GLYPH_LABEL_GAP: f32 = 8.0;
pub const TRAIL_INSET: f32 = 24.0;
pub const TRAIL_GLYPH: f32 = 28.0;
pub const TRAIL_GAP: f32 = 12.0;
/// A row's label (32 line box, ~24-26), its second line and its trailing value.
pub const LABEL_SIZE: c_int = theme::size::LABEL;
pub const DETAIL_SIZE: c_int = theme::size::CAPTION;
pub const VALUE_SIZE: c_int = theme::size::CAPTION;
pub const GROUP_SIZE: c_int = theme::size::CAPTION;

/// A player row's two-line plate (j04, k04: a 88 pitch with no gap) and where its second line's
/// centre sits under the plate's top (61: "Boost the dialog…" at 262..280 under a row at 210).
pub const PLAYER_ITEM_H_TALL: f32 = 88.0;
pub const PLAYER_DETAIL_CY: f32 = 61.0;
/// The radio circle (j01: a 32 ring at 1766..1797, its centre 16 in from the trailing edge) and the
/// current choice's 16 dot.
pub const RADIO: f32 = 32.0;
pub const RADIO_EDGE_W: f32 = 2.0;
pub const RADIO_DOT: f32 = 16.0;
/// The switch (j04: a 52x32 track at 1746..1798, a 28 knob 2 in from its end; §1.5 radius 16).
pub const SWITCH_W: f32 = 52.0;
pub const SWITCH_H: f32 = 32.0;
pub const SWITCH_KNOB: f32 = 28.0;

/// **Which panel the rows are**: the library and detail menus' pills, or the player's settings
/// rows (see the module doc).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Menu,
    Player,
}

impl Kind {
    /// The gap under a row: the menus' air ([`ITEM_GAP`], [`ITEM_GAP_GLYPHS`]) or none.
    fn gap(self, glyphs: bool) -> f32 {
        match self {
            Kind::Menu if glyphs => ITEM_GAP_GLYPHS,
            Kind::Menu => ITEM_GAP,
            Kind::Player => 0.0,
        }
    }

    /// A two-line row's plate.
    fn tall(self) -> f32 {
        match self {
            Kind::Menu => ITEM_H_TALL,
            Kind::Player => PLAYER_ITEM_H_TALL,
        }
    }
}

/// **How the rows are marked**, beside the [`Section`]s: the panel's [`Kind`] and, for a player
/// panel, which table rows are one-of-several choices (a radio circle; the [`Row::checked`] one
/// filled). Indexed like the rows; a row past the end is no choice.
#[derive(Clone, Copy, Debug, Default)]
pub struct Marks<'a> {
    pub kind: Kind,
    pub choices: &'a [bool],
}

impl Marks<'_> {
    /// The menus' marks: the current choice's check, a switch's word.
    pub const MENU: Marks<'static> = Marks { kind: Kind::Menu, choices: &[] };
}

/// What one line of the laid-out list is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    /// Table row `index` (focusable unless it is a separator or a note), `top`..`top + h`.
    Row { index: usize, top: f32, h: f32 },
    /// A group's title, from section `section`'s header.
    Group { section: usize, top: f32 },
}

/// **The list's layout** in DOCUMENT space (unscrolled screen y): every row of every section in
/// table order, a later section's non-empty header as its group title, and section 0's header left
/// out when it is the panel's own `title` (a one-section menu names itself once).
pub fn lay_out(sections: &[Section], title: &str) -> Vec<Line> {
    lay_out_for(sections, title, Kind::Menu)
}

/// [`lay_out`] for a panel of `kind`: a player panel's rows stand on their 64 / 88 pitch.
pub fn lay_out_for(sections: &[Section], title: &str, kind: Kind) -> Vec<Line> {
    let glyphs = sections.iter().flat_map(|s| &s.rows).any(|r| r.licon.is_some());
    let gap = kind.gap(glyphs);
    let mut out = Vec::new();
    let mut y = LIST_TOP;
    let mut index = 0;
    for (s, section) in sections.iter().enumerate() {
        if !section.header.is_empty() && !(s == 0 && section.header == title) {
            if !out.is_empty() {
                y += GROUP_AIR - gap;
            }
            out.push(Line::Group { section: s, top: y });
            y += GROUP_H + ITEM_GAP;
        }
        for row in &section.rows {
            let h = row_height_for(row, kind);
            out.push(Line::Row { index, top: y, h });
            y += h + gap;
            index += 1;
        }
    }
    out
}

/// A row's height: a separator's hairline, a two-line row's tall plate, else the plate.
pub fn row_height(row: &Row) -> f32 {
    row_height_for(row, Kind::Menu)
}

/// [`row_height`] in a panel of `kind`. A player panel's note is one quiet line on a plain row.
pub fn row_height_for(row: &Row, kind: Kind) -> f32 {
    if row.sep && !row.is_note() {
        SEP_H
    } else if !row.detail.is_empty() || (row.is_note() && kind == Kind::Menu) {
        kind.tall()
    } else {
        ITEM_H
    }
}

/// Where table row `index` lies in `lines`: `(top, h)` in document space.
pub fn row_span(lines: &[Line], index: usize) -> Option<(f32, f32)> {
    lines.iter().find_map(|line| match *line {
        Line::Row { index: i, top, h } if i == index => Some((top, h)),
        _ => None,
    })
}

/// Row `index`'s plate in SCREEN space at scroll `scroll`.
pub fn row_rect(lines: &[Line], index: usize, scroll: f32) -> Option<Rect> {
    row_span(lines, index).map(|(top, h)| Rect::new(ITEM_X, top - scroll, ITEM_W, h))
}

/// The scroll (≥ 0) that keeps the focused row `top..top + h` inside [`LIST_TOP`]..[`LIST_BOTTOM`],
/// moving the list as little as possible from `current` — `side_nav::scroll_for`'s rule.
pub fn scroll_for(top: f32, h: f32, current: f32) -> f32 {
    let mut s = current.max(0.0);
    if top + h - s > LIST_BOTTOM {
        s = top + h - LIST_BOTTOM;
    }
    if top - s < LIST_TOP {
        s = top - LIST_TOP;
    }
    s.max(0.0)
}

/// The window the list is cut to: from the title's foot to the frame's inside bottom.
pub fn list_clip() -> Rect {
    let top = TITLE.y + TITLE.h;
    Rect::new(PANEL.x + PANEL_EDGE_W, top, PANEL.w - 2.0 * PANEL_EDGE_W, PANEL.y + PANEL.h - PANEL_EDGE_W - top)
}

/// **The panel**: its opaque ground, `title`, and the laid-out `sections` scrolled by `scroll`, with
/// table row `focused` on the white plate.
pub fn draw(p: Painter, title: &str, sections: &[Section], lines: &[Line], focused: Option<usize>, scroll: f32,
    measure: &dyn Measure) {
    draw_with(p, title, sections, lines, focused, scroll, Marks::MENU, measure);
}

/// [`draw`], with the rows marked as `marks` says (a player panel's radios and switches).
#[allow(clippy::too_many_arguments)]
pub fn draw_with(p: Painter, title: &str, sections: &[Section], lines: &[Line], focused: Option<usize>, scroll: f32,
    marks: Marks<'_>, measure: &dyn Measure) {
    p.rect(PANEL, PANEL_RADIUS, theme::SHIELD_PANEL_FILL, theme::SHIELD_PANEL_FILL, 0.0);
    p.rring(PANEL, PANEL_RADIUS, PANEL_EDGE_W, theme::SHIELD_PANEL_EDGE);
    let head = measure.fit_line(title, TITLE.w, TITLE_SIZE, true);
    let ty = plx_gfx::text::text_vcenter_y(TITLE_SIZE, 1, TITLE.cy());
    p.text(head.as_ptr(), TITLE.x, ty, TITLE_SIZE, theme::SHIELD_PANEL_INK, 0, 1);
    let clip = list_clip();
    p.clip(clip);
    let rows: Vec<&Row> = sections.iter().flat_map(|s| &s.rows).collect();
    let dims: Vec<bool> = sections.iter().flat_map(|s| s.rows.iter().map(move |_| s.dim)).collect();
    for line in lines {
        match *line {
            Line::Group { section, top } => {
                let y = top - scroll;
                if y + GROUP_H < clip.y || y > clip.y + clip.h {
                    continue;
                }
                let Some(section) = sections.get(section) else { continue };
                let text = measure.fit_line(&section.header, ITEM_W - LABEL_INSET, GROUP_SIZE, false);
                let ty = plx_gfx::text::text_vcenter_y(GROUP_SIZE, 0, y + GROUP_H * 0.5);
                let a = if section.dim { 0.5 } else { 1.0 };
                p.alpha(a).text(text.as_ptr(), ITEM_X + LABEL_INSET, ty, GROUP_SIZE, theme::SHIELD_PANEL_VALUE_INK, 0, 0);
            }
            Line::Row { index, top, h } => {
                let r = Rect::new(ITEM_X, top - scroll, ITEM_W, h);
                if r.y + r.h < clip.y || r.y > clip.y + clip.h {
                    continue;
                }
                let Some(row) = rows.get(index) else { continue };
                let dim = row.dim || dims.get(index).copied().unwrap_or(false);
                let choice = marks.choices.get(index).copied().unwrap_or(false);
                let p = p.alpha(if dim { 0.5 } else { 1.0 });
                match marks.kind {
                    Kind::Menu => draw_row(p, r, row, focused == Some(index), measure),
                    Kind::Player => draw_player_row(p, r, row, choice, focused == Some(index), measure),
                }
            }
        }
    }
    p.clip_clear();
}

/// A player row's label: the row's own, with its badges spoken after it ("English (SRT)").
pub fn player_label(row: &Row) -> String {
    if row.badges.is_empty() {
        return row.label.clone();
    }
    let badges: Vec<&str> = row.badges.iter().map(|b| b.text()).collect();
    format!("{} ({})", row.label, badges.join(", "))
}

/// The radio circle centred in `r`: a ring, filled with the accent dot when `on`. On the focused
/// plate the ring takes the plate's ink.
pub fn draw_radio(p: Painter, r: Rect, on: bool, focused: bool) {
    let ring = if focused { theme::SHIELD_FOCUS_INK } else { theme::SHIELD_RADIO_RING };
    p.rring(r, r.w * 0.5, RADIO_EDGE_W, ring);
    if on {
        let dot = Rect::new(r.cx() - RADIO_DOT * 0.5, r.cy() - RADIO_DOT * 0.5, RADIO_DOT, RADIO_DOT);
        p.rect(dot, RADIO_DOT * 0.5, theme::SHIELD_RADIO_DOT, theme::SHIELD_RADIO_DOT, 0.0);
    }
}

/// The switch in its track `r` (§1.4 toggle.on / toggle.off): the knob at the end it is set to.
pub fn draw_switch(p: Painter, r: Rect, on: bool) {
    let (track, knob) = if on {
        (theme::SHIELD_SWITCH_ON_TRACK, theme::SHIELD_SWITCH_ON_KNOB)
    } else {
        (theme::SHIELD_SWITCH_OFF_TRACK, theme::SHIELD_SWITCH_OFF_KNOB)
    };
    p.rect(r, r.h * 0.5, track, track, 0.0);
    let inset = (r.h - SWITCH_KNOB) * 0.5;
    let kx = if on { r.x + r.w - inset - SWITCH_KNOB } else { r.x + inset };
    p.rect(Rect::new(kx, r.y + inset, SWITCH_KNOB, SWITCH_KNOB), SWITCH_KNOB * 0.5, knob, knob, 0.0);
}

/// One player row in its plate `r`: the label (and its second line), the value read-out, and at
/// the trailing edge the switch or the radio circle `choice` asks for.
fn draw_player_row(p: Painter, r: Rect, row: &Row, choice: bool, focused: bool, measure: &dyn Measure) {
    if row.sep && !row.is_note() {
        p.rect(Rect::new(r.x, r.y + (r.h - SEP_H) * 0.5, r.w, SEP_H), 0.0, theme::SHIELD_PANEL_EDGE,
            theme::SHIELD_PANEL_EDGE, 0.0);
        return;
    }
    if row.is_note() {
        let text = measure.fit_line(&row.label, r.w - 2.0 * LABEL_INSET, DETAIL_SIZE, false);
        let ty = plx_gfx::text::text_vcenter_y(DETAIL_SIZE, 0, r.cy());
        p.text(text.as_ptr(), r.x + LABEL_INSET, ty, DETAIL_SIZE, theme::SHIELD_PANEL_VALUE_INK, 0, 0);
        return;
    }
    let (ink, value_ink) = if focused {
        p.rect(r, ITEM_H * 0.5, theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_FILL, 0.0);
        (theme::SHIELD_FOCUS_INK, theme::SHIELD_PANEL_VALUE_FOCUS_INK)
    } else {
        (theme::SHIELD_PANEL_INK, theme::SHIELD_PANEL_VALUE_INK)
    };
    let line_cy = if row.detail.is_empty() { r.cy() } else { r.y + ITEM_H * 0.5 };
    let mut right = r.x + r.w - TRAIL_INSET;
    if let (Some(on), None) = (row.toggle, row.value.as_ref()) {
        draw_switch(p, Rect::new(right - SWITCH_W, r.cy() - SWITCH_H * 0.5, SWITCH_W, SWITCH_H), on);
        right -= SWITCH_W + TRAIL_GAP;
    } else if choice {
        draw_radio(p, Rect::new(right - RADIO, r.cy() - RADIO * 0.5, RADIO, RADIO), row.checked, focused);
        right -= RADIO + TRAIL_GAP;
    } else if let Some(glyph) = row.ticon.filter(|g| *g != Icon::Chevron) {
        icons::draw(p, glyph, Rect::new(right - TRAIL_GLYPH, line_cy - TRAIL_GLYPH * 0.5, TRAIL_GLYPH, TRAIL_GLYPH), ink);
        right -= TRAIL_GLYPH + TRAIL_GAP;
    }
    let x = r.x + LABEL_INSET;
    let label = player_label(row);
    if let Some(value) = row.value.as_deref() {
        // the value takes what the label leaves (j03: "English (AC3 Stereo)" beside "Audio"), never
        // less than half the row
        let label_w = measure.width_str(&label, LABEL_SIZE, false);
        let room = (right - x - label_w - TRAIL_GAP).max((right - x) * 0.5).max(0.0);
        let text = measure.fit_line(value, room, VALUE_SIZE, false);
        let w = measure.width(&text, VALUE_SIZE, false);
        let ty = plx_gfx::text::text_vcenter_y(VALUE_SIZE, 0, line_cy);
        p.text(text.as_ptr(), right, ty, VALUE_SIZE, value_ink, 2, 0);
        right -= w + TRAIL_GAP;
    }
    let label = measure.fit_line(&label, (right - x).max(0.0), LABEL_SIZE, false);
    let ty = plx_gfx::text::text_vcenter_y(LABEL_SIZE, 0, line_cy);
    p.text(label.as_ptr(), x, ty, LABEL_SIZE, ink, 0, 0);
    if !row.detail.is_empty() {
        let detail = measure.fit_line(&row.detail, (right - x).max(0.0), DETAIL_SIZE, false);
        let dy = plx_gfx::text::text_vcenter_y(DETAIL_SIZE, 0, r.y + PLAYER_DETAIL_CY);
        p.text(detail.as_ptr(), x, dy, DETAIL_SIZE, value_ink, 0, 0);
    }
}

/// One row in its plate `r`.
fn draw_row(p: Painter, r: Rect, row: &Row, focused: bool, measure: &dyn Measure) {
    if row.sep && !row.is_note() {
        p.rect(Rect::new(r.x, r.y, r.w, SEP_H), 0.0, theme::SHIELD_PANEL_EDGE, theme::SHIELD_PANEL_EDGE, 0.0);
        return;
    }
    if row.is_note() {
        let text = measure.fit_line(&row.label, r.w - 2.0 * LABEL_INSET, DETAIL_SIZE, false);
        let ty = plx_gfx::text::text_vcenter_y(DETAIL_SIZE, 0, r.cy());
        p.text(text.as_ptr(), r.x + LABEL_INSET, ty, DETAIL_SIZE, theme::SHIELD_PANEL_VALUE_INK, 0, 0);
        return;
    }
    let (ink, value_ink) = if focused {
        p.rect(r, ITEM_H * 0.5, theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_FILL, 0.0);
        (theme::SHIELD_FOCUS_INK, theme::SHIELD_PANEL_VALUE_FOCUS_INK)
    } else {
        (theme::SHIELD_PANEL_INK, theme::SHIELD_PANEL_VALUE_INK)
    };
    // the trailing edge, right to left: the glyph (a sort's direction, a drill-in chevron) or the
    // current choice's check, then the value word
    let mut right = r.x + r.w - TRAIL_INSET;
    let trail = row.ticon.or(row.checked.then_some(Icon::Check));
    let line_cy = if row.detail.is_empty() { r.cy() } else { r.y + ITEM_H * 0.5 };
    if let Some(glyph) = trail {
        icons::draw(p, glyph, Rect::new(right - TRAIL_GLYPH, line_cy - TRAIL_GLYPH * 0.5, TRAIL_GLYPH, TRAIL_GLYPH), ink);
        right -= TRAIL_GLYPH + TRAIL_GAP;
    }
    let mut x = r.x + LABEL_INSET;
    if let Some(glyph) = row.licon {
        icons::draw(p, glyph, Rect::new(x, line_cy - GLYPH * 0.5, GLYPH, GLYPH), ink);
        x += GLYPH + GLYPH_LABEL_GAP;
    }
    if let Some(value) = row.readout() {
        let room = ((right - x) * 0.5).max(0.0);
        let text = measure.fit_line(value, room, VALUE_SIZE, false);
        let w = measure.width(&text, VALUE_SIZE, false);
        let ty = plx_gfx::text::text_vcenter_y(VALUE_SIZE, 0, line_cy);
        p.text(text.as_ptr(), right, ty, VALUE_SIZE, value_ink, 2, 0);
        right -= w + TRAIL_GAP;
    }
    let label = measure.fit_line(&row.label, (right - x).max(0.0), LABEL_SIZE, false);
    let ty = plx_gfx::text::text_vcenter_y(LABEL_SIZE, 0, line_cy);
    p.text(label.as_ptr(), x, ty, LABEL_SIZE, ink, 0, 0);
    if !row.detail.is_empty() {
        let detail = measure.fit_line(&row.detail, (r.x + r.w - TRAIL_INSET - x).max(0.0), DETAIL_SIZE, false);
        let dy = plx_gfx::text::text_vcenter_y(DETAIL_SIZE, 0, r.y + ITEM_H + (r.h - ITEM_H) * 0.25);
        p.text(detail.as_ptr(), x, dy, DETAIL_SIZE, value_ink, 0, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sort_like(n: usize) -> Vec<Section> {
        let mut s = Section::new("Sort by");
        for i in 0..n {
            s = s.row(Row::new(format!("Sort {i}")).checked(i == 0));
        }
        vec![s]
    }

    /// §6.4, b06: the 640x1016 frame at x 1248, the title at 1314,98, the first row at 210 on a 72
    /// pitch, each plate 508x64 at x 1314 — "Date Added", the eleventh, at 930.
    #[test]
    fn the_panel_and_its_rows_are_the_measured_ones() {
        assert_eq!(PANEL, Rect::new(1248.0, 32.0, 640.0, 1016.0));
        assert_eq!((PANEL.x + PANEL.w, PANEL.y + PANEL.h), (1888.0, 1048.0));
        assert_eq!(TITLE.x - PANEL.x, 66.0);
        let sections = sort_like(11);
        let lines = lay_out(&sections, "Sort by");
        assert_eq!(lines.len(), 11, "a one-section menu names itself once: no group title");
        assert_eq!(row_rect(&lines, 0, 0.0), Some(Rect::new(1314.0, 210.0, 508.0, 64.0)));
        assert_eq!(row_rect(&lines, 1, 0.0).unwrap().y, 282.0);
        assert_eq!(row_rect(&lines, 10, 0.0).unwrap().y, 930.0);
        assert_eq!(ITEM_X + ITEM_W - TRAIL_INSET, 1798.0, "the trailing mark ends at 1798");
    }

    /// A list of leading glyphs keeps the 76 pitch (More Actions, Settings).
    #[test]
    fn a_list_of_glyphs_breathes_on_a_76_pitch() {
        let s = Section::new("").row(Row::new("Play").licon(Icon::Play)).row(Row::new("Info").licon(Icon::Info));
        let lines = lay_out(&[s], "More");
        assert_eq!(row_rect(&lines, 1, 0.0).unwrap().y - row_rect(&lines, 0, 0.0).unwrap().y, 76.0);
    }

    /// b07: a hairline 8 under "Duplicates" (786..850) and the next row ("Genre") at 868; a later
    /// section is titled in its own group, 16 under the row above.
    #[test]
    fn separators_and_later_groups_keep_the_measured_air() {
        let mut s = Section::new("Filter by");
        for i in 0..9 {
            s = s.row(Row::new(format!("F{i}")));
        }
        s = s.row(Row::separator()).row(Row::new("Genre"));
        let later = Section::new("Server").row(Row::new("Movies").detail("12 films"));
        let lines = lay_out(&[s, later], "Filter by");
        assert_eq!(row_rect(&lines, 8, 0.0).unwrap().y, 786.0);
        assert_eq!(row_span(&lines, 9), Some((858.0, SEP_H)));
        assert_eq!(row_rect(&lines, 10, 0.0).unwrap().y, 868.0);
        let group = lines.iter().find_map(|l| match l { Line::Group { top, .. } => Some(*top), _ => None }).unwrap();
        assert_eq!(group, 868.0 + 64.0 + GROUP_AIR);
        assert_eq!(row_span(&lines, 11), Some((group + GROUP_H + ITEM_GAP, ITEM_H_TALL)), "a two-line row is the tall plate");
    }

    /// j01, j04, k04: a player panel's rows stand on a 64 pitch from 210 with no gap ("None" at
    /// 210..274, "Danish (SRT)" at 274), a two-line row on 88 ("Boost Dialog" 210..298, "Normalize
    /// Loudness" from 298), and the radio's 32 ring ends on the trailing edge at 1798 (1766..1798).
    #[test]
    fn a_player_panel_stands_on_the_measured_pitch() {
        let subs = Section::new("Subtitles").row(Row::new("None").checked(true)).row(Row::new("Danish"))
            .row(Row::new("English"));
        let lines = lay_out_for(&[subs], "Subtitles", Kind::Player);
        assert_eq!(row_rect(&lines, 0, 0.0), Some(Rect::new(1314.0, 210.0, 508.0, 64.0)));
        assert_eq!(row_rect(&lines, 1, 0.0).unwrap().y, 274.0);
        assert_eq!(row_rect(&lines, 2, 0.0).unwrap().y, 338.0);
        let options = Section::new("").row(Row::new("Boost Dialog").detail("Boost the dialog").toggle(false))
            .row(Row::new("Normalize Loudness").detail("Not analyzed").toggle(false)).row(Row::new("Nerd Statistics").toggle(true));
        let lines = lay_out_for(&[options], "Playback Options", Kind::Player);
        assert_eq!(row_span(&lines, 0), Some((210.0, PLAYER_ITEM_H_TALL)));
        assert_eq!(row_span(&lines, 1), Some((298.0, PLAYER_ITEM_H_TALL)));
        assert_eq!(row_span(&lines, 2), Some((386.0, ITEM_H)));
        assert_eq!(ITEM_X + ITEM_W - TRAIL_INSET - RADIO, 1766.0, "the radio ring starts at 1766");
        assert_eq!(ITEM_X + ITEM_W - TRAIL_INSET - SWITCH_W, 1746.0, "the switch track starts at 1746");
        assert_eq!(210.0 + PLAYER_DETAIL_CY, 271.0, "the second line's centre");
        // the menus keep their own pitch
        let menu = lay_out(&[Section::new("Sort by").row(Row::new("A")).row(Row::new("B"))], "Sort by");
        assert_eq!(row_rect(&menu, 1, 0.0).unwrap().y, 282.0);
    }

    /// A track's badges are spoken after its name, the way the reference reads "English (SRT)".
    #[test]
    fn a_player_row_speaks_its_badges_in_its_label() {
        use crate::table::Badge;
        assert_eq!(player_label(&Row::new("English")), "English");
        assert_eq!(player_label(&Row::new("English").badge(Badge::Text("SRT".into()))), "English (SRT)");
        let two = Row::new("English").badge(Badge::Text("SRT".into())).badge(Badge::Text("PGS".into()));
        assert_eq!(player_label(&two), "English (SRT, PGS)");
    }

    /// A long list scrolls just enough to keep focus inside the window, and back.
    #[test]
    fn a_long_list_scrolls_just_enough() {
        let lines = lay_out(&sort_like(30), "Sort by");
        let (top, h) = row_span(&lines, 20).unwrap();
        let s = scroll_for(top, h, 0.0);
        assert_eq!(top + h - s, LIST_BOTTOM);
        let (top0, h0) = row_span(&lines, 0).unwrap();
        assert_eq!(scroll_for(top0, h0, s), 0.0);
        let (top5, h5) = row_span(&lines, 5).unwrap();
        assert_eq!(scroll_for(top5, h5, 0.0), 0.0, "a row already in view moves nothing");
        assert!(list_clip().y >= TITLE.y + TITLE.h);
    }
}
