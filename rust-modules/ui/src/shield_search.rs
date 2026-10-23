//! **The Shield layout's Search page** (`layout::shield()`; SHIELD-LAYOUT-SPEC §8.1): the query
//! field and the on-screen keyboard in the left column, the hint line, the result headings and the
//! captioned four-column result grid in the right. `shield_browse` is the caption every grid card
//! wears and the chip the Clear control is; `shield` the ring cards and the row heading.
//!
//! Every number is measured off the Plex app on Android TV at 1920x1080 (c01's dump), the logical
//! space this app draws in, so they transfer 1:1 and are pinned by this module's tests. Geometry
//! and paint only: what a key types, what the hint says and where the results have scrolled to are
//! the page's (`screens::search`'s `shield`).
//!
//! - **The field** ([`FIELD`], [`draw_field`]): the query (or its placeholder) on one 64-tall line
//!   at x 132; focused, it stands on a white plate. The app's voice and search-settings buttons
//!   beside it have no PlxNative counterpart and are not drawn.
//! - **The keyboard** ([`key_rect`], [`key_neighbour`], [`draw_key`]): 64x64 keys on a 72 pitch, six
//!   across (a–z, then 1–0), and a row of three 136-wide keys under them: space, backspace, clear.
//!   Idle keys are dim plates with a white glyph; the focused key is white with a black one.
//! - **The results** ([`HINT`], [`HEAD_H`], [`HEADING_BLOCK`], [`RESULTS_X`]): a hint line at the
//!   column's top, then each group's heading 64 over its cards, four 240x360 posters across
//!   from x 686 on a ~292.7 pitch, rows 460 apart.
//! - **Recent searches** ([`recent_rect`], [`draw_recent`]): the terms the profile searched before,
//!   one 64-tall row each under their heading while nothing is typed.

use std::ffi::CStr;
use std::os::raw::c_int;

use crate::icons::{self, Icon};
use crate::screen::Dir;
use crate::shield::CARD_W;
use crate::{theme, Painter, Rect};
use plx_machine::machine::Measure;

// ---- the field (spec §8.1) ----------------------------------------------------------------------

/// The input row: the query's line box, 298 wide at x 132 (c01: `screen-search-input`).
pub const FIELD: Rect = Rect::new(132.0, 48.0, 298.0, 64.0);
/// The query's type rung (~26 measured, the keys' size).
pub const FIELD_SIZE: c_int = theme::size::LABEL;
/// The focused field's plate: a full pill 48 tall on the line, standing 16 out to the left of the
/// text so the first letter does not touch its edge.
pub const FIELD_PLATE_PAD: f32 = 16.0;
pub const FIELD_PLATE_H: f32 = 48.0;
/// The caret while the system keyboard is up: 3 wide, the cap band tall, 4 after the text.
pub const CARET_W: f32 = 3.0;
pub const CARET_GAP: f32 = 4.0;

/// The focused field's plate around [`FIELD`].
pub fn field_plate() -> Rect {
    Rect::new(FIELD.x - FIELD_PLATE_PAD, FIELD.cy() - FIELD_PLATE_H * 0.5, FIELD.w + FIELD_PLATE_PAD,
        FIELD_PLATE_H)
}

/// Where the field's run and caret sit for a run `run_w` wide whose caret is `head_w` into it: the
/// run from the line's left edge while it fits, slid left just enough to keep the caret on the line
/// once it does not (the caret is reserved only while the system keyboard is up). Answers the run's
/// offset and the caret's x, both from [`FIELD`]'s left edge.
pub fn field_layout(run_w: f32, head_w: f32, editing: bool) -> (f32, f32) {
    let room = (FIELD.w - if editing { CARET_W + CARET_GAP } else { 0.0 }).max(0.0);
    let slide = (head_w - room).max(0.0).min((run_w - room).max(0.0));
    (-slide, (head_w - slide + CARET_GAP).min((FIELD.w - CARET_W).max(0.0)))
}

/// The focused field's white plate (drawn before [`draw_field`], outside the text's clip).
pub fn draw_field_plate(p: Painter) {
    let plate = field_plate();
    p.rect(plate, plate.h * 0.5, theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_FILL, 0.0);
}

/// **The field's text**: `run` (the query, or the placeholder when `placeholder`) at `dx` from the
/// line's left edge — the caller clips it to [`FIELD`] — and the caret at `caret` (its x from the
/// edge) while the system keyboard is up. Focused, the ink is black, on [`draw_field_plate`].
pub fn draw_field(p: Painter, run: &CStr, dx: f32, placeholder: bool, focused: bool, caret: Option<f32>) {
    let ink = if focused {
        theme::SHIELD_FOCUS_INK
    } else if placeholder {
        theme::SHIELD_SEARCH_HINT
    } else {
        theme::SHIELD_KEY_INK
    };
    let ty = plx_gfx::text::text_vcenter_y(FIELD_SIZE, 0, FIELD.cy());
    p.text(run.as_ptr(), FIELD.x + dx, ty, FIELD_SIZE, ink, 0, 0);
    if let Some(x) = caret {
        let (top, base) = plx_gfx::text::text_cap_band(FIELD_SIZE, 0);
        p.rect(Rect::new(FIELD.x + x, ty + top, CARET_W, base - top), 0.0, ink, ink, 0.0);
    }
}

// ---- the keyboard (spec §8.1) -------------------------------------------------------------------

/// A key's side and the pitch between keys (8 apart).
pub const KEY: f32 = 64.0;
pub const KEY_PITCH: f32 = 72.0;
/// Six keys a row, the first at x 132; the first row at y 128.
pub const KEY_COLS: usize = 6;
pub const KEY_X: f32 = 132.0;
pub const KEY_Y: f32 = 128.0;
/// What the character keys type, in key order: a–z, then 1–9 and 0.
pub const CHARS: &str = "abcdefghijklmnopqrstuvwxyz1234567890";
/// The character keys (six rows of six).
pub const CHAR_KEYS: usize = 36;
/// The wide keys' row (space, backspace, clear): 136 wide (two keys and their gap), 144 apart.
pub const WIDE_KEYS: usize = 3;
pub const WIDE_W: f32 = 2.0 * KEY + (KEY_PITCH - KEY);
pub const WIDE_Y: f32 = KEY_Y + (CHAR_KEYS / KEY_COLS) as f32 * KEY_PITCH;
/// Every key on the board.
pub const KEY_COUNT: usize = CHAR_KEYS + WIDE_KEYS;
/// The keyboard's block (c01: `keyboard`), a group's extent.
pub const KEYBOARD: Rect = Rect::new(128.0, 124.0, 430.0, 506.0);
/// A key's corner (est. 6-8), its glyph's type rung and an icon key's glyph box.
pub const KEY_RADIUS: f32 = 8.0;
pub const KEY_SIZE: c_int = theme::size::LABEL;
pub const KEY_GLYPH: f32 = 32.0;

/// What a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyFace {
    /// Types its character.
    Char(char),
    Space,
    Backspace,
    /// Empties the query.
    Clear,
}

/// Key `i`'s face (`None` past the board).
pub fn key_face(i: usize) -> Option<KeyFace> {
    match i {
        _ if i < CHAR_KEYS => CHARS.chars().nth(i).map(KeyFace::Char),
        36 => Some(KeyFace::Space),
        37 => Some(KeyFace::Backspace),
        38 => Some(KeyFace::Clear),
        _ => None,
    }
}

/// Key `i`'s rect: a character key in its row and column, a wide key in the last row.
pub fn key_rect(i: usize) -> Rect {
    if i < CHAR_KEYS {
        let (row, col) = (i / KEY_COLS, i % KEY_COLS);
        Rect::new(KEY_X + col as f32 * KEY_PITCH, KEY_Y + row as f32 * KEY_PITCH, KEY, KEY)
    } else {
        let j = (i - CHAR_KEYS).min(WIDE_KEYS - 1);
        Rect::new(KEY_X + j as f32 * 2.0 * KEY_PITCH, WIDE_Y, WIDE_W, KEY)
    }
}

/// The key a move `dir` from key `i` lands on, `None` at the board's edge (whatever is beyond is
/// the page's: the field above, the results to the right, the side panel to the left). A wide key
/// sits under two columns: DOWN from either lands on it, UP from it lands on the left one.
pub fn key_neighbour(i: usize, dir: Dir) -> Option<usize> {
    if i >= KEY_COUNT {
        return None;
    }
    if i < CHAR_KEYS {
        let (row, col) = (i / KEY_COLS, i % KEY_COLS);
        let rows = CHAR_KEYS / KEY_COLS;
        return match dir {
            Dir::Left => (col > 0).then(|| i - 1),
            Dir::Right => (col + 1 < KEY_COLS).then_some(i + 1),
            Dir::Up => (row > 0).then(|| i - KEY_COLS),
            Dir::Down if row + 1 < rows => Some(i + KEY_COLS),
            Dir::Down => Some(CHAR_KEYS + col / 2),
        };
    }
    let j = i - CHAR_KEYS;
    match dir {
        Dir::Left => (j > 0).then(|| i - 1),
        Dir::Right => (j + 1 < WIDE_KEYS).then_some(i + 1),
        Dir::Up => Some(CHAR_KEYS - KEY_COLS + 2 * j),
        Dir::Down => None,
    }
}

/// The key nearest a point: where a move into the board from elsewhere (the results at the right,
/// the field above) lands when the board has no remembered key.
pub fn nearest_key(x: f32, y: f32) -> usize {
    let d = |i: usize| {
        let r = key_rect(i);
        let (dx, dy) = (((r.cx() - x).abs() - r.w * 0.5).max(0.0), ((r.cy() - y).abs() - r.h * 0.5).max(0.0));
        dx * dx + dy * dy
    };
    (0..KEY_COUNT).min_by(|a, b| d(*a).total_cmp(&d(*b))).unwrap_or(0)
}

/// **A key** in `r`: its plate (white when focused) and its glyph — `label` for a character key,
/// the icon for the others.
pub fn draw_key(p: Painter, r: Rect, face: KeyFace, label: &CStr, focused: bool) {
    let (fill, ink) = if focused {
        (theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_INK)
    } else {
        (theme::SHIELD_KEY_FILL, theme::SHIELD_KEY_INK)
    };
    p.rect(r, KEY_RADIUS, fill, fill, 0.0);
    let glyph = match face {
        KeyFace::Char(_) => {
            let ty = plx_gfx::text::text_vcenter_y(KEY_SIZE, 0, r.cy());
            p.text(label.as_ptr(), r.cx(), ty, KEY_SIZE, ink, 1, 0);
            return;
        }
        KeyFace::Space => Icon::Space,
        KeyFace::Backspace => Icon::Backspace,
        KeyFace::Clear => Icon::Close,
    };
    icons::draw(p, glyph, Rect::new(r.cx() - KEY_GLYPH * 0.5, r.cy() - KEY_GLYPH * 0.5, KEY_GLYPH, KEY_GLYPH), ink);
}

// ---- the results (spec §8.1) --------------------------------------------------------------------

/// The results column: from x 686 to the last card's right edge (the scroller itself starts at 622,
/// [`RESULTS_CLIP_X`]).
pub const RESULTS_X: f32 = 686.0;
pub const RESULTS_COLS: usize = 4;
pub const RESULTS_LAST_X: f32 = 1564.0;
pub const RESULTS_RIGHT: f32 = RESULTS_LAST_X + CARD_W;
pub const RESULTS_CLIP_X: f32 = 622.0;
/// Four posters at x 686, 979, 1271, 1564: a ~292.7 pitch.
pub const RESULTS_PITCH_X: f32 = (RESULTS_LAST_X - RESULTS_X) / (RESULTS_COLS as f32 - 1.0);
pub const RESULTS_GAP: f32 = RESULTS_PITCH_X - CARD_W;
/// The hint line: 32 tall at y 48, semibold ~26.
pub const HINT: Rect = Rect::new(RESULTS_X, 48.0, RESULTS_RIGHT - RESULTS_X, 32.0);
pub const HINT_SIZE: c_int = theme::size::LABEL;
/// The document's head: the hint line and the air under it, down to the first heading at y 128.
pub const HEAD_H: f32 = 128.0;
/// A group's heading block: the 48-tall heading, its cards 64 under its top (y 128 to 192).
pub const HEADING_H: f32 = 48.0;
pub const HEADING_BLOCK: f32 = 64.0;
/// The first row's cards on an unscrolled page.
pub const FIRST_ROW_Y: f32 = HEAD_H + HEADING_BLOCK;
/// A focused row's card top is never brought above this line: its heading (the first row of a
/// group) or the row above's captions stay in view over it.
pub const FOCUS_ROW_Y: f32 = FIRST_ROW_Y;
/// The caption under a grid card, down to the year's foot (`shield_browse`'s caption boxes).
pub const CAPTION_FOOT: f32 = crate::shield_browse::CAPTION_SUB_DY + crate::shield_browse::CAPTION_SUB_H;

/// **The scroll that shows a focused result** whose cards' top is at `top` in the document and
/// that is `h` tall, from the scroll `from` the page is headed to: the least move that shows the
/// popped card and its caption (which drops with the pop) whole, without bringing the card above
/// [`FOCUS_ROW_Y`]; nothing when it already shows, and the page's head (0) for a result on the
/// first row. Clamped to `max`.
pub fn reveal(from: f32, top: f32, h: f32, max: f32) -> f32 {
    if top <= FIRST_ROW_Y {
        return 0.0;
    }
    let pop = h * (crate::shield::FOCUS_SCALE - 1.0) * 0.5;
    let highest = top - FOCUS_ROW_Y;
    let lowest = top + h + pop + CAPTION_FOOT - crate::consts::SCR_H;
    from.clamp(lowest.min(highest), highest).clamp(0.0, max.max(0.0))
}

/// **The hint line**, its top dropped to `y` (the page's scroll carries it away): `text` in the
/// quiet semibold, elided at the column's edge.
pub fn draw_hint(p: Painter, text: &str, y: f32, measure: &dyn Measure) {
    let fit = measure.fit_line(text, HINT.w, HINT_SIZE, true);
    let ty = plx_gfx::text::text_vcenter_y(HINT_SIZE, 1, y + HINT.h * 0.5);
    p.text(fit.as_ptr(), HINT.x, ty, HINT_SIZE, theme::SHIELD_SEARCH_HINT, 0, 1);
}

/// **A group's heading** in its block at `top`: the row heading's bold title in the results
/// column, 64 over the group's cards.
pub fn draw_heading(p: Painter, title: &str, top: f32, measure: &dyn Measure) {
    crate::shield::draw_row_heading_in(p, Rect::new(RESULTS_X, top, RESULTS_RIGHT - RESULTS_X, HEADING_H), title, "",
        measure);
}

// ---- recent searches ----------------------------------------------------------------------------

/// A recent search's row: 64 tall on the keys' 72 pitch, wide enough for a long term.
pub const RECENT_H: f32 = KEY;
pub const RECENT_PITCH: f32 = KEY_PITCH;
pub const RECENT_W: f32 = 560.0;
/// The term's inset from the row's left edge, and its type rung.
pub const RECENT_PAD: f32 = 24.0;
pub const RECENT_SIZE: c_int = theme::size::LABEL;
/// The Clear chip's air above it and below it.
pub const CLEAR_AIR: f32 = 16.0;
/// The Clear block: the chip and its air.
pub const CLEAR_BLOCK: f32 = CLEAR_AIR + crate::shield_browse::CHIP_H + CLEAR_AIR;

/// The height of the recents block for `n` terms: the heading block and a row each.
pub fn recents_h(n: usize) -> f32 {
    HEADING_BLOCK + n as f32 * RECENT_PITCH
}

/// Term `n`'s row in a recents block whose top is at `top`.
pub fn recent_rect(n: usize, top: f32) -> Rect {
    Rect::new(RESULTS_X, top + HEADING_BLOCK + n as f32 * RECENT_PITCH, RECENT_W, RECENT_H)
}

/// The Clear chip, `w` wide (its label's width), in a block whose top is at `top`.
pub fn clear_rect(label_w: f32, top: f32) -> Rect {
    Rect::new(RESULTS_X, top + CLEAR_AIR, label_w + 2.0 * crate::shield_browse::CHIP_PAD, crate::shield_browse::CHIP_H)
}

/// **A recent search** in its row `r`: the term (already fitted to the row), on a white full-pill
/// plate with black ink when focused.
pub fn draw_recent(p: Painter, r: Rect, term: &CStr, focused: bool) {
    let ink = if focused {
        p.rect(r, r.h * 0.5, theme::SHIELD_FOCUS_FILL, theme::SHIELD_FOCUS_FILL, 0.0);
        theme::SHIELD_FOCUS_INK
    } else {
        theme::SHIELD_KEY_INK
    };
    let ty = plx_gfx::text::text_vcenter_y(RECENT_SIZE, 0, r.cy());
    p.text(term.as_ptr(), r.x + RECENT_PAD, ty, RECENT_SIZE, ink, 0, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// c01's dump: `keyboard-key-a` at [132,128][196,192], `-f` at [492,128][556,192], `-g` at
    /// [132,200], `-0` at [492,488][556,552]; space, backspace and clear at [132|276|420,560] 136x64.
    #[test]
    fn the_keys_sit_where_the_dump_put_them() {
        assert_eq!(key_rect(0), Rect::new(132.0, 128.0, 64.0, 64.0));
        assert_eq!(key_rect(5), Rect::new(492.0, 128.0, 64.0, 64.0));
        assert_eq!(key_rect(6), Rect::new(132.0, 200.0, 64.0, 64.0));
        assert_eq!(key_rect(25), Rect::new(204.0, 416.0, 64.0, 64.0), "z");
        assert_eq!(key_rect(35), Rect::new(492.0, 488.0, 64.0, 64.0), "0");
        let wide: Vec<Rect> = (36..39).map(key_rect).collect();
        assert_eq!(wide, vec![
            Rect::new(132.0, 560.0, 136.0, 64.0),
            Rect::new(276.0, 560.0, 136.0, 64.0),
            Rect::new(420.0, 560.0, 136.0, 64.0),
        ]);
        for i in 0..KEY_COUNT {
            let r = key_rect(i);
            assert!(r.x >= KEYBOARD.x && r.y >= KEYBOARD.y && r.x + r.w <= KEYBOARD.x + KEYBOARD.w
                && r.y + r.h <= KEYBOARD.y + KEYBOARD.h, "key {i} inside the keyboard block");
        }
    }

    /// The keys' order is the dump's: a–f, g–l, m–r, s–x, y z 1–4, 5–0, then the three wide keys.
    #[test]
    fn the_board_reads_a_to_z_then_one_to_zero() {
        let rows: Vec<String> = (0..6).map(|r| (0..6).filter_map(|c| match key_face(r * 6 + c) {
            Some(KeyFace::Char(ch)) => Some(ch),
            _ => None,
        }).collect()).collect();
        assert_eq!(rows, ["abcdef", "ghijkl", "mnopqr", "stuvwx", "yz1234", "567890"]);
        assert_eq!((36..39).map(key_face).collect::<Vec<_>>(),
            [Some(KeyFace::Space), Some(KeyFace::Backspace), Some(KeyFace::Clear)]);
        assert_eq!(key_face(KEY_COUNT), None);
    }

    /// A move stays on the board until its edge, and the wide row is reached from the two columns
    /// over each of its keys.
    #[test]
    fn moves_walk_the_board_and_stop_at_its_edges() {
        assert_eq!(key_neighbour(0, Dir::Left), None, "LEFT off a: the page's (the side panel)");
        assert_eq!(key_neighbour(0, Dir::Up), None, "UP off a: the page's (the field)");
        assert_eq!(key_neighbour(5, Dir::Right), None, "RIGHT off f: the page's (the results)");
        assert_eq!(key_neighbour(0, Dir::Right), Some(1));
        assert_eq!(key_neighbour(0, Dir::Down), Some(6));
        for (from, wide) in [(30, 36), (31, 36), (32, 37), (33, 37), (34, 38), (35, 38)] {
            assert_eq!(key_neighbour(from, Dir::Down), Some(wide), "DOWN from {from}");
        }
        assert_eq!(key_neighbour(37, Dir::Up), Some(32));
        assert_eq!(key_neighbour(36, Dir::Left), None);
        assert_eq!(key_neighbour(38, Dir::Right), None);
        assert_eq!(key_neighbour(37, Dir::Down), None);
        assert_eq!(key_neighbour(KEY_COUNT, Dir::Up), None);
    }

    #[test]
    fn a_move_into_the_board_lands_on_the_nearest_key() {
        assert_eq!(nearest_key(800.0, 160.0), 5, "from the first result row: f");
        assert_eq!(nearest_key(800.0, 590.0), 38, "from low on the right: clear");
        assert_eq!(nearest_key(250.0, 40.0), 1, "from the field: the key under it");
        assert_eq!(nearest_key(key_rect(20).cx(), key_rect(20).cy()), 20);
    }

    /// c01: the input at [132,48][430,112]; the hint at 686,48 (32 tall); the heading at 686,128
    /// (48); cards at x 686, 979, 1271, 1564, y 192 and 652 (460 apart).
    #[test]
    fn the_results_column_is_the_measured_one() {
        assert_eq!(FIELD, Rect::new(132.0, 48.0, 298.0, 64.0));
        assert_eq!((HINT.x, HINT.y, HINT.h), (686.0, 48.0, 32.0));
        assert_eq!(HEAD_H, 128.0);
        assert_eq!(HEAD_H + HEADING_BLOCK, 192.0);
        let xs: Vec<f32> = (0..RESULTS_COLS).map(|c| RESULTS_X + c as f32 * RESULTS_PITCH_X).collect();
        for (x, measured) in xs.iter().zip([686.0, 979.0, 1271.0, 1564.0]) {
            assert!((x - measured).abs() < 1.0, "column at {x} vs measured {measured}");
        }
        assert!((RESULTS_GAP - 52.67).abs() < 0.01);
        assert_eq!(RESULTS_RIGHT, 1804.0);
        assert!(KEYBOARD.x + KEYBOARD.w < RESULTS_CLIP_X, "the results never run under the keyboard");
        assert_eq!(FIRST_ROW_Y + crate::shield_browse::GRID_ROW_PITCH, 652.0);
    }

    /// The first row rests on the page's head; a row that does not show whole — its popped card
    /// and the caption that drops with the pop — is brought up the least that shows it; none is
    /// brought above the focus line.
    #[test]
    fn the_reveal_shows_a_row_whole_and_keeps_it_under_the_focus_line() {
        let h = crate::shield::CARD_H;
        let near = |got: f32, want: f32| (got - want).abs() < 0.01;
        assert_eq!(reveal(300.0, FIRST_ROW_Y, h, 9999.0), 0.0, "the first row is the head's");
        assert!(near(reveal(0.0, 652.0, h, 9999.0), 7.5), "the second row's popped caption (7.5 down) ends on the foot");
        assert!(near(reveal(0.0, 1112.0, h, 9999.0), 467.5), "the third brings itself to the second's place");
        assert_eq!(reveal(9999.0, 1112.0, h, 99999.0), 1112.0 - FOCUS_ROW_Y, "going up stops at the focus line");
        assert_eq!(reveal(500.0, 1112.0, h, 99999.0), 500.0, "a row that shows stays put");
        assert_eq!(reveal(0.0, 1112.0, h, 100.0), 100.0, "never past the document's end");
    }

    #[test]
    fn recents_stack_under_their_heading_and_clear_follows() {
        assert_eq!(recent_rect(0, HEAD_H), Rect::new(686.0, 192.0, RECENT_W, 64.0));
        assert_eq!(recent_rect(2, HEAD_H).y, 192.0 + 144.0);
        assert_eq!(recents_h(3), 64.0 + 216.0);
        assert_eq!(clear_rect(100.0, 500.0), Rect::new(686.0, 516.0, 132.0, 48.0));
        assert_eq!(CLEAR_BLOCK, 80.0);
    }

    /// A run that fits starts at the line's edge with the caret after it; a longer one slides left
    /// so the caret stays on the line, and only by as much as it overflows.
    #[test]
    fn the_field_run_slides_only_to_keep_its_caret_on_the_line() {
        assert_eq!(field_layout(100.0, 100.0, true), (0.0, 104.0));
        assert_eq!(field_layout(100.0, 40.0, false), (0.0, 44.0));
        let (dx, caret) = field_layout(500.0, 500.0, true);
        assert_eq!(dx, -(500.0 - (FIELD.w - CARET_W - CARET_GAP)));
        assert!(caret + CARET_W <= FIELD.w, "the caret is on the line");
        assert_eq!(field_layout(500.0, 500.0, false).0, -(500.0 - FIELD.w), "no caret: the run's end on the edge");
        assert_eq!(field_layout(0.0, 0.0, true), (0.0, CARET_GAP));
    }

    /// The field's plate stays inside the input row's own column and never reaches the keyboard.
    #[test]
    fn the_field_plate_stands_on_its_line() {
        let plate = field_plate();
        assert_eq!(plate, Rect::new(116.0, 56.0, 314.0, 48.0));
        assert!(plate.y + plate.h < KEY_Y, "clear of the first key row");
        assert!(plate.x >= crate::shield::CONTENT_X - 12.0, "and of the rail band");
    }
}
