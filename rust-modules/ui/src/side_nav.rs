//! **The Shield layout's navigation chrome** (`layout::shield()`): the collapsed icon rail at the
//! left edge, the clock pill at the top right, and the side panel the rail stands for. Geometry
//! and paint only — the application draws the rail and the clock in place of the top bar
//! (`Rig::draw_chrome`), and `screens::sidebar` composes the panel as a surface.
//!
//! Every number is measured off the Plex app on Android TV at 1920x1080 (SHIELD-LAYOUT-SPEC §2 and
//! §3.3), which is the same logical space this app draws in, so they transfer 1:1 and are pinned by
//! this module's tests rather than derived from the Standard layout's tokens.
//!
//! **The rail is decoration, never a focus stop**: focus never rests on it. LEFT off a page's
//! left-most content opens the panel (`Rig::edge_surface`), focused on the current page's entry.
//!
//! **The panel OVERLAYS the page** at a fixed frame (`PANEL`): the page keeps its geometry and is
//! dimmed by the surface's scrim, the rail band is covered, and a list taller than the frame scrolls
//! as a whole to keep focus inside it ([`scroll_for`]), clipped at the frame.
//!
//! The panel's entries never elide app-owned words; library and profile names are the server's
//! and elide at [`LABEL_MAX_W`] / [`NAME_MAX_W`] (the caller measures, as `profile_chip_text` does).

use std::ffi::CStr;
use std::os::raw::c_int;

use crate::icons::{self, Icon};
use crate::widgets::ProfileChipRead;
use crate::{theme, Painter, Rect};

// ---- the collapsed rail (spec §2.1) -------------------------------------------------------------

/// The band the layout reserves at the left edge for the rail.
pub const RAIL_BAND: Rect = Rect::new(0.0, 0.0, 128.0, crate::consts::SCR_H);
/// The translucent capsule the rail's marks stand on: a full pill on its 72 width.
pub const RAIL_CAPSULE: Rect = Rect::new(28.0, 432.0, 72.0, 216.0);
/// The vertical centre line every rail mark sits on.
pub const RAIL_CX: f32 = 64.0;
/// The profile's face at the top of the rail, 40 across.
pub const RAIL_AVATAR: Rect = Rect::new(44.0, 448.0, 40.0, 40.0);
/// The rail's glyph box.
pub const RAIL_GLYPH: f32 = 24.0;
/// The centre heights of the rail's three glyphs, top to bottom: Search, Home, the menu mark.
pub const RAIL_GLYPH_CY: [f32; 3] = [524.0, 572.0, 620.0];
/// The rail's glyphs, in [`RAIL_GLYPH_CY`]'s order.
pub const RAIL_GLYPHS: [Icon; 3] = [Icon::Search, Icon::Home, Icon::Menu];

/// The box of the rail glyph centred at height `cy`.
pub fn rail_glyph_rect(cy: f32) -> Rect {
    Rect::new(RAIL_CX - RAIL_GLYPH * 0.5, cy - RAIL_GLYPH * 0.5, RAIL_GLYPH, RAIL_GLYPH)
}

/// The collapsed rail: its capsule, the profile's face and the three glyphs.
pub fn draw_rail(p: Painter, profile: ProfileChipRead<'_>) {
    let cap = theme::NAV_RAIL_CAPSULE;
    p.rect(RAIL_CAPSULE, RAIL_CAPSULE.w * 0.5, cap, cap, 0.0);
    crate::widgets::profile_avatar(p, profile, RAIL_AVATAR, theme::size::LABEL, 9.0, 0.0);
    for (glyph, cy) in RAIL_GLYPHS.into_iter().zip(RAIL_GLYPH_CY) {
        icons::draw(p, glyph, rail_glyph_rect(cy), theme::NAV_RAIL_INK);
    }
}

// ---- the clock pill (spec §3.3) -----------------------------------------------------------------

/// The pill's right edge: on the 96 margin.
pub const CLOCK_RIGHT: f32 = crate::consts::SCR_W - crate::consts::MARGIN_X - 1.0;
/// The pill's top and height: a 48 capsule at y 40.
pub const CLOCK_Y: f32 = 40.0;
pub const CLOCK_H: f32 = 48.0;
/// Text inset each side of the figures.
pub const CLOCK_PAD: f32 = 16.0;
/// The figures' type rung (~24 measured).
pub const CLOCK_SIZE: c_int = theme::size::CAPTION;

/// The pill around figures `text_w` wide, right-aligned on [`CLOCK_RIGHT`].
pub fn clock_rect(text_w: f32) -> Rect {
    let w = text_w + 2.0 * CLOCK_PAD;
    Rect::new(CLOCK_RIGHT - w, CLOCK_Y, w, CLOCK_H)
}

/// The clock pill with its figures (`text`, measured `text_w` wide at [`CLOCK_SIZE`]).
pub fn draw_clock(p: Painter, text: &CStr, text_w: f32) {
    let r = clock_rect(text_w);
    p.rect(r, r.h * 0.5, theme::CLOCK_PILL_FILL, theme::CLOCK_PILL_FILL, 0.0);
    let ty = plx_gfx::text::text_vcenter_y(CLOCK_SIZE, 0, r.cy());
    p.text(text.as_ptr(), r.x + CLOCK_PAD, ty, CLOCK_SIZE, theme::CLOCK_INK, 0, 0);
}

// ---- the side panel (spec §2.2) -----------------------------------------------------------------

/// The panel's fixed frame: 32 in from the top, bottom and left, 388 wide.
pub const PANEL: Rect = Rect::new(32.0, 32.0, 388.0, 1016.0);
pub const PANEL_RADIUS: f32 = 16.0;
/// The panel's border width.
pub const PANEL_EDGE_W: f32 = 1.0;
/// The scrolled list is cut at the frame's inside edge.
pub const PANEL_CLIP: Rect = Rect::new(PANEL.x + 2.0, PANEL.y + 2.0, PANEL.w - 4.0, PANEL.h - 4.0);
/// The entry column: 34 in from each side of the frame, 320 wide.
pub const ITEM_X: f32 = 66.0;
pub const ITEM_W: f32 = 320.0;
/// Where the list starts: the account row's top, 34 under the frame's.
pub const LIST_TOP: f32 = 66.0;
/// The lowest an entry's bottom may sit before the list scrolls: 34 above the frame's bottom.
pub const LIST_BOTTOM: f32 = PANEL.y + PANEL.h - 34.0;
/// The account row's height, and an entry's.
pub const ACCOUNT_H: f32 = 72.0;
pub const ITEM_H: f32 = 64.0;
/// The gap under the account row and under every entry (entry pitch 68).
pub const ITEM_GAP: f32 = 4.0;
/// A section header's text band and the air above and below it (an entry's bottom to the
/// header's text is 16; the text's bottom to the next entry is 16).
pub const HEADER_H: f32 = 24.0;
pub const HEADER_AIR: f32 = 16.0;
/// An entry's leading glyph (~32 at x 82) and where its label starts.
pub const ICON: f32 = 32.0;
pub const ICON_X: f32 = 82.0;
pub const LABEL_X: f32 = 130.0;
/// An entry label's room: from [`LABEL_X`] to 16 inside the entry's right end.
pub const LABEL_MAX_W: f32 = ITEM_X + ITEM_W - 16.0 - LABEL_X;
/// The account row's face (48 at 78,78) and the profile name beside it (box 196 wide at x 142).
pub const AVATAR: f32 = 48.0;
pub const AVATAR_X: f32 = 78.0;
pub const NAME_X: f32 = 142.0;
pub const NAME_MAX_W: f32 = 196.0;
/// The account row's trailing chevron, centred at x ~365.
pub const CHEVRON: f32 = 24.0;
pub const CHEVRON_CX: f32 = 365.0;
/// A header's text starts with the glyphs (x 82); its rule runs from 8 past the text to x 369.
pub const HEADER_X: f32 = ICON_X;
pub const HEADER_RULE_GAP: f32 = 8.0;
pub const HEADER_RULE_END: f32 = 369.0;
/// The current page's outline.
pub const CURRENT_RING_W: f32 = 2.0;
/// The type rungs: an entry and the profile name (24-26 measured), a header (20-21 measured; the
/// one-line de-emphasised label rung).
pub const LABEL_SIZE: c_int = theme::size::LABEL;
pub const HEADER_SIZE: c_int = theme::size::MICRO;

/// One line of the panel's list, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// The profile row: the face, the name and a chevron. A focus stop.
    Account,
    /// A destination: a glyph and a label. A focus stop.
    Item,
    /// A section header ("Libraries") and its rule. Never focused: UP and DOWN skip it.
    Header,
}

impl Line {
    /// The line's height in the list (a header's is its text band).
    pub fn height(self) -> f32 {
        match self {
            Line::Account => ACCOUNT_H,
            Line::Item => ITEM_H,
            Line::Header => HEADER_H,
        }
    }

    /// Whether focus can rest on this line.
    pub fn focusable(self) -> bool {
        !matches!(self, Line::Header)
    }
}

/// **Every line's top, in DOCUMENT space** (unscrolled screen y): the account row at
/// [`LIST_TOP`], entries on a 68 pitch, and a header standing 16 under the entry above it with its
/// next entry 16 under its text. The one layout rule the draw, the focus geometry and the scroll
/// all read.
pub fn tops(lines: &[Line]) -> Vec<f32> {
    let mut out = Vec::with_capacity(lines.len());
    // the bottom of the line above (plus nothing yet: the list starts at LIST_TOP)
    let mut y = LIST_TOP;
    let mut after: Option<Line> = None;
    for &line in lines {
        let top = match (after, line) {
            (None, _) => y,
            (Some(Line::Header), _) => y + HEADER_AIR,
            (Some(_), Line::Header) => y + HEADER_AIR,
            (Some(_), _) => y + ITEM_GAP,
        };
        out.push(top);
        y = top + line.height();
        after = Some(line);
    }
    out
}

/// The scroll (how far the list has moved UP, ≥ 0) that keeps the focused line `top..top + h`
/// inside [`LIST_TOP`]..[`LIST_BOTTOM`], moving the list as little as possible from `current`.
/// The frame never moves; the list scrolls as a whole (spec §2.2, b02: every row up 44).
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

/// A line's rect in SCREEN space at scroll `scroll`.
pub fn line_rect(top: f32, line: Line, scroll: f32) -> Rect {
    Rect::new(ITEM_X, top - scroll, ITEM_W, line.height())
}

/// How a focusable line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    /// The remote is on it: a white full pill with black ink.
    Focused,
    /// The page on screen, while focus is elsewhere: a lifted plate with an outline and white ink.
    Current,
}

impl State {
    /// The ink of the line's label and glyph in this state.
    pub fn ink(self) -> [f32; 4] {
        match self {
            State::Idle => theme::NAV_ITEM_INK,
            State::Focused => theme::NAV_FOCUS_INK,
            State::Current => theme::TEXT_PRIMARY,
        }
    }
}

/// The panel's ground: the opaque black frame and its 1px border.
pub fn draw_ground(p: Painter) {
    p.rect(PANEL, PANEL_RADIUS, theme::NAV_PANEL_FILL, theme::NAV_PANEL_FILL, 0.0);
    p.rring(PANEL, PANEL_RADIUS, PANEL_EDGE_W, theme::NAV_PANEL_EDGE);
}

/// A focusable line's plate for `state` in `r` (nothing at rest).
fn draw_plate(p: Painter, r: Rect, state: State) {
    let rad = r.h * 0.5;
    match state {
        State::Idle => {}
        State::Focused => p.rect(r, rad, theme::NAV_FOCUS_FILL, theme::NAV_FOCUS_FILL, 0.0),
        State::Current => {
            p.rect(r, rad, theme::NAV_CURRENT_FILL, theme::NAV_CURRENT_FILL, 0.0);
            p.rring(r, rad, CURRENT_RING_W, theme::NAV_CURRENT_RING);
        }
    }
}

/// One destination entry in `r` (from [`line_rect`]): its plate, glyph and label (already elided
/// to [`LABEL_MAX_W`] by the caller when it is a server's name).
pub fn draw_item(p: Painter, r: Rect, glyph: Icon, label: &CStr, state: State) {
    draw_plate(p, r, state);
    let ink = state.ink();
    icons::draw(p, glyph, Rect::new(ICON_X, r.cy() - ICON * 0.5, ICON, ICON), ink);
    let ty = plx_gfx::text::text_vcenter_y(LABEL_SIZE, 0, r.cy());
    p.text(label.as_ptr(), LABEL_X, ty, LABEL_SIZE, ink, 0, 0);
}

/// The account row in `r`: the profile's face, its name (elided to [`NAME_MAX_W`]) and the chevron
/// that says a press opens more.
pub fn draw_account(p: Painter, r: Rect, profile: ProfileChipRead<'_>, state: State) {
    draw_plate(p, r, state);
    let face = Rect::new(AVATAR_X, r.cy() - AVATAR * 0.5, AVATAR, AVATAR);
    crate::widgets::profile_avatar(p, profile, face, theme::size::BODY, 11.0, 0.0);
    let name_ink = if state == State::Focused { theme::NAV_FOCUS_INK } else { theme::TEXT_PRIMARY };
    let ty = plx_gfx::text::text_vcenter_y(LABEL_SIZE, 0, r.cy());
    p.text(profile.name.as_ptr(), NAME_X, ty, LABEL_SIZE, name_ink, 0, 0);
    let chevron = Rect::new(CHEVRON_CX - CHEVRON * 0.5, r.cy() - CHEVRON * 0.5, CHEVRON, CHEVRON);
    icons::draw(p, Icon::Chevron, chevron, state.ink());
}

/// A section header whose text top is at `top` (screen space): its words, measured `label_w`
/// wide, and the rule that runs from 8 past them to [`HEADER_RULE_END`] on their mid-line.
pub fn draw_header(p: Painter, top: f32, label: &CStr, label_w: f32) {
    let cy = top + HEADER_H * 0.5;
    let ty = plx_gfx::text::text_vcenter_y(HEADER_SIZE, 0, cy);
    p.text(label.as_ptr(), HEADER_X, ty, HEADER_SIZE, theme::NAV_HEADER_INK, 0, 0);
    let x0 = HEADER_X + label_w + HEADER_RULE_GAP;
    if x0 < HEADER_RULE_END {
        p.rect(Rect::new(x0, cy.floor(), HEADER_RULE_END - x0, 1.0), 0.0, theme::NAV_PANEL_EDGE, theme::NAV_PANEL_EDGE, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The collapsed rail at the measured positions (spec §2.1): the capsule, the face, and three
    /// glyphs on a 48 pitch with the face 56 above Search, all on the x 64 centre line.
    #[test]
    fn the_rail_sits_where_the_spec_measured_it() {
        assert_eq!(RAIL_CAPSULE, Rect::new(28.0, 432.0, 72.0, 216.0));
        assert_eq!(RAIL_CAPSULE.cx(), RAIL_CX);
        assert_eq!(RAIL_AVATAR, Rect::new(44.0, 448.0, 40.0, 40.0));
        assert_eq!(RAIL_AVATAR.cx(), RAIL_CX);
        assert_eq!(RAIL_AVATAR.cy(), 468.0);
        assert_eq!(RAIL_GLYPH_CY, [524.0, 572.0, 620.0]);
        assert_eq!(RAIL_GLYPH_CY[0] - RAIL_AVATAR.cy(), 56.0);
        for pair in RAIL_GLYPH_CY.windows(2) {
            assert_eq!(pair[1] - pair[0], 48.0);
        }
        for cy in RAIL_GLYPH_CY {
            let r = rail_glyph_rect(cy);
            assert_eq!((r.cx(), r.cy(), r.w, r.h), (RAIL_CX, cy, 24.0, 24.0));
            assert!(r.y >= RAIL_CAPSULE.y && r.y + r.h <= RAIL_CAPSULE.y + RAIL_CAPSULE.h);
        }
        assert_eq!(RAIL_GLYPHS, [Icon::Search, Icon::Home, Icon::Menu]);
        assert!(RAIL_CAPSULE.x + RAIL_CAPSULE.w <= RAIL_BAND.w);
    }

    /// The clock pill: 48 tall at y 40, its right edge on the 96 margin, 16 of padding — the
    /// measured `[1689..1823] x [40..88]` for the measured 102-wide "9:18 AM".
    #[test]
    fn the_clock_pill_hangs_off_the_right_margin() {
        let r = clock_rect(102.0);
        assert_eq!((r.x, r.y, r.x + r.w, r.y + r.h), (1689.0, 40.0, 1823.0, 88.0));
        assert_eq!(clock_rect(40.0).x + clock_rect(40.0).w, 1823.0, "a narrower reading keeps the right edge");
    }

    /// The panel frame and its column (spec §2.2): 388x1016 at 32,32, radius 16; a 320 column
    /// 34 in from each side.
    #[test]
    fn the_panel_frame_and_column_are_the_measured_ones() {
        assert_eq!(PANEL, Rect::new(32.0, 32.0, 388.0, 1016.0));
        assert_eq!(PANEL.x + PANEL.w, 420.0);
        assert_eq!(PANEL.y + PANEL.h, 1048.0);
        assert_eq!(PANEL_RADIUS, 16.0);
        assert_eq!(ITEM_X - PANEL.x, 34.0);
        assert_eq!(PANEL.x + PANEL.w - (ITEM_X + ITEM_W), 34.0);
        assert_eq!((ITEM_X, ITEM_W), (66.0, 320.0));
        assert_eq!((ICON_X, LABEL_X, NAME_X), (82.0, 130.0, 142.0));
        assert!(LABEL_MAX_W > 200.0 && LABEL_X + LABEL_MAX_W < ITEM_X + ITEM_W);
    }

    /// The b01 capture's list: the account row at 66, My Lists 142, Search 210, Home 278, the
    /// "Libraries" header's text at 358 and the first library at 398, then a 68 pitch.
    #[test]
    fn the_list_lays_out_on_the_measured_pitches() {
        use Line::*;
        let lines = [Account, Item, Item, Item, Header, Item, Item, Item, Item, Item, Item, Item];
        let tops = tops(&lines);
        assert_eq!(&tops[..5], &[66.0, 142.0, 210.0, 278.0, 358.0]);
        assert_eq!(tops[5], 398.0);
        for pair in tops[5..].windows(2) {
            assert_eq!(pair[1] - pair[0], 68.0, "entries under a header keep the 68 pitch");
        }
        assert_eq!(tops[3] + ITEM_H + HEADER_AIR, tops[4], "16 from an entry's bottom to the header's text");
        assert_eq!(tops[4] + HEADER_H + HEADER_AIR, tops[5], "16 from the header's text to the next entry");
        assert!(!Header.focusable() && Account.focusable() && Item.focusable());
    }

    /// The list scrolls only when focus would leave the 66..1014 window, by exactly enough — the
    /// b02 capture: with focus on an entry whose bottom would sit at 1058, every row moves up 44.
    #[test]
    fn the_list_scrolls_just_enough_to_keep_focus_inside_the_frame() {
        assert_eq!(LIST_BOTTOM, 1014.0);
        assert_eq!(scroll_for(278.0, ITEM_H, 0.0), 0.0, "a line already inside moves nothing");
        assert_eq!(scroll_for(994.0, ITEM_H, 0.0), 44.0);
        assert_eq!(scroll_for(278.0, ITEM_H, 44.0), 44.0, "a scrolled list stays put while focus fits");
        assert_eq!(scroll_for(LIST_TOP, ACCOUNT_H, 44.0), 0.0, "back to the top row unscrolls");
        assert_eq!(scroll_for(142.0, ITEM_H, 120.0), 76.0, "an entry above the window scrolls back just to the top edge");
        let r = line_rect(994.0, Line::Item, 44.0);
        assert_eq!(r.y + r.h, LIST_BOTTOM);
    }
}
