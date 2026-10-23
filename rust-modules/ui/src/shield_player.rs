//! **The Shield layout's player** (`layout::shield()`): the transport the Plex app on Android TV
//! draws over a playing picture (i04, k02, p01, h03, k11), its Skip Intro / Skip Credits pill
//! (w016, e001) and the post-play page that offers the next episode when one ends (e022) — geometry
//! and paint over plain values the player fills. What a key does on any of them is the player's.
//!
//! - **The transport** ([`draw_transport`]): the title (bold) and a grey meta line with the media's
//!   HDR / 4K tags at the top left, the wall clock at the top right, two round buttons (Subtitles,
//!   Settings) over the bar's right end, and along the foot the glyph slot, the elapsed clock, the
//!   bar (a grey track, the played run in the accent, the playhead's white dot while the bar holds
//!   focus) and the remaining clock. The bar runs between the two clocks, each column as wide as
//!   the item's longest time reads, so a film's bar is shorter than an episode's (i04 241..1697,
//!   p01 216..1722). A chevron under the bar says more is below; with focus there it gives way to
//!   the Info / Chapters pills. A dark ramp at the top and the foot keeps every run legible.
//! - **The glyph slot** ([`Glyph`]) shows what OK on the bar does — the pause bars while playing,
//!   the play triangle while paused (k04, k02) — and, while a hop or a hold is travelling, a jump
//!   arrow with the seconds it has gathered (h03's `↶ 10`, k11's `↷ 90`). It is never focusable.
//! - **The Skip pill** ([`skip_rect`], [`draw_skip`]): white with black words at the bottom right,
//!   drawn on its own over the bare picture.
//! - **The post-play page** ([`draw_post_play`]): a dark navy ground; *Just Played* with the ended
//!   episode's still and a replay glyph; *Up Next* with its countdown in the heading, the next
//!   still ringed and a play glyph; the next episode's title, address and summary beside them; and
//!   *On Deck* below, a row of wide stills captioned like a grid's.

use std::ffi::CStr;
use std::os::raw::c_int;

use crate::card_row::TileLabel;
use crate::consts::{SCR_H, SCR_W};
use crate::icons::{self, Icon};
use crate::text_view::TextView;
use crate::widgets::{Art, Spinner};
use crate::{theme, Env, Painter, Rect, View};
use plx_machine::machine::Measure;

// ---- the transport (i04, k02, p01, h03, k11) ----------------------------------------------------

/// The runs' left edge (the title at x 72) and right edge (the clock, the buttons and the
/// remaining clock end at 1848).
pub const LEFT: f32 = 72.0;
pub const RIGHT: f32 = SCR_W - 72.0;
/// The title: bold ~32, its cap band centred at y 84 (73..95).
pub const TITLE_CY: f32 = 84.0;
pub const TITLE_SIZE: c_int = theme::size::HEADLINE;
/// The meta line under it (~20 measured: the one-line de-emphasised rung), centred at 122, and the
/// air before each media tag ("2006 · 2 hr 24 min   HDR   4K").
pub const META_CY: f32 = 122.0;
pub const META_SIZE: c_int = theme::size::MICRO;
pub const TAG_GAP: f32 = 24.0;
/// The wall clock's figures, centred at 73 (65..81), right-aligned on [`RIGHT`].
pub const CLOCK_CY: f32 = 73.0;
pub const CLOCK_SIZE: c_int = theme::size::CAPTION;
/// How much of the top the title may take before the clock's column.
pub const TITLE_W: f32 = RIGHT - LEFT - 320.0;
/// The bar: 10 tall, centred at 975 (969..980).
pub const BAR_CY: f32 = 975.0;
pub const BAR_H: f32 = 10.0;
/// The playhead's dot while the bar holds focus (963..986).
pub const KNOB: f32 = 22.0;
/// The glyph slot's centre and box (the play triangle's ink at 80..101 x 963..986).
pub const GLYPH_CX: f32 = 90.0;
pub const GLYPH: f32 = 30.0;
/// A jump arrow's box, left of the seconds it has gathered.
pub const JUMP_GLYPH: f32 = 28.0;
pub const JUMP_X: f32 = 70.0;
pub const JUMP_NUMBER_SIZE: c_int = theme::size::MICRO;
/// The clocks: ~24, the elapsed column's left edge (its widest reading starts at 137) and its gap
/// to the bar (17), the bar's gap to the remaining column (~50).
pub const TIME_SIZE: c_int = theme::size::CAPTION;
pub const ELAPSED_X: f32 = 137.0;
pub const TIME_GAP: f32 = 17.0;
pub const REMAINING_GAP: f32 = 50.0;
/// The two round buttons: 64 discs 12 apart, the last ending on [`RIGHT`], their top at 870.
pub const BUTTON: f32 = 64.0;
pub const BUTTON_Y: f32 = 870.0;
pub const BUTTON_GAP: f32 = 12.0;
pub const BUTTON_GLYPH: f32 = 32.0;
/// The chevron under the bar (952..967 x 1007..1015) and the pills that take its place.
pub const CHEVRON: f32 = 24.0;
pub const CHEVRON_CY: f32 = 1011.0;
pub const TABS_CY: f32 = 1022.0;
/// The ramps: black at the top edge to clear at 240; clear at 700 to black at the foot.
pub const TOP_SCRIM_H: f32 = 240.0;
pub const FOOT_SCRIM_Y: f32 = 700.0;
/// The pointer's grab band over the bar: 80 tall around it.
pub const GRAB_H: f32 = 80.0;

/// What the glyph slot at the bar's left shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    /// Playing: OK pauses (k04).
    Pause,
    /// Paused: OK plays (k02).
    Play,
    /// A hop or a hold is travelling: which way, and the whole seconds it has gathered.
    Jump { forward: bool, secs: i64 },
    /// The pipeline is being waited on with no travel to show: the inline spinner.
    Working,
}

/// Which part of the transport holds focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// The bar: its playhead wears the dot.
    Bar,
    /// Round button `0` (Subtitles) or `1` (Settings).
    Button(usize),
    /// The pills under the bar (Info, Chapters), `0`-based.
    Tabs(usize),
}

/// Everything [`draw_transport`] draws from.
pub struct Transport<'a> {
    /// The playing item's title (server text: elides).
    pub title: &'a str,
    /// The meta line ("2026 · 2 hr 11 min"; "S10 · E7 · Show · 19 min").
    pub meta: &'a str,
    /// The media tags after it ("HDR", "4K").
    pub tags: &'a [&'a str],
    /// The wall clock's figures, when there is a clock.
    pub clock: Option<&'a CStr>,
    /// The displayed playhead and the item's length.
    pub pos_ms: i64,
    pub dur_ms: i64,
    pub glyph: Glyph,
    pub focus: Focus,
    /// The labels the pills under the bar carry (Info, then Chapters when there are any).
    pub tabs: &'a [&'a CStr],
    /// The spinner's phase.
    pub now: u32,
}

/// The widest an item `dur_ms` long reads on a clock: its own length with every digit a 0 — the
/// elapsed column is that wide, and the remaining one that with the minus sign.
pub fn time_template(dur_ms: i64) -> String {
    crate::fmt::clock(dur_ms.max(0)).chars().map(|c| if c.is_ascii_digit() { '0' } else { c }).collect()
}

/// A clock run: the playback clock, signed `-` for the time remaining.
pub fn time_text(ms: i64, remaining: bool) -> String {
    let c = crate::fmt::clock(ms.max(0));
    if remaining { format!("-{c}") } else { c }
}

/// The bar's two ends for an item `dur_ms` long: after the elapsed column, before the remaining
/// one (`(x0, x1)`, `x1 >= x0`).
pub fn bar_span(dur_ms: i64, measure: &dyn Measure) -> (f32, f32) {
    let t = time_template(dur_ms);
    let el_w = measure.width_str(&t, TIME_SIZE, false);
    let rem_w = measure.width_str(&format!("-{t}"), TIME_SIZE, false);
    bar_span_of(el_w, rem_w)
}

/// [`bar_span`] from the two columns' measured widths.
pub fn bar_span_of(elapsed_w: f32, remaining_w: f32) -> (f32, f32) {
    let x0 = ELAPSED_X + elapsed_w + TIME_GAP;
    let x1 = RIGHT - remaining_w - REMAINING_GAP;
    (x0, x1.max(x0))
}

/// The bar's track between `x0` and `x1`.
pub fn bar_rect(x0: f32, x1: f32) -> Rect {
    Rect::new(x0, BAR_CY - BAR_H * 0.5, x1 - x0, BAR_H)
}

/// The pointer's band over the bar between `x0` and `x1`.
pub fn grab_rect(x0: f32, x1: f32) -> Rect {
    Rect::new(x0, BAR_CY - GRAB_H * 0.5, x1 - x0, GRAB_H)
}

/// Round button `i`: 0 Subtitles, 1 Settings (1708 and 1784 at 870).
pub fn button_rect(i: usize) -> Rect {
    let x = RIGHT - BUTTON - (1 - i.min(1)) as f32 * (BUTTON + BUTTON_GAP);
    Rect::new(x, BUTTON_Y, BUTTON, BUTTON)
}

/// The glyph each round button wears.
pub const BUTTON_GLYPHS: [Icon; 2] = [Icon::Cc, Icon::Gear];

/// The chevron's box, centred under the screen.
pub fn chevron_rect() -> Rect {
    Rect::new(SCR_W * 0.5 - CHEVRON * 0.5, CHEVRON_CY - CHEVRON * 0.5, CHEVRON, CHEVRON)
}

/// The pills under the bar for labels `widths` wide, as a row centred on the screen.
pub fn tab_rects(widths: &[f32]) -> Vec<Rect> {
    let pad = crate::shield_browse::TAB_PAD;
    let gap = crate::shield_browse::TAB_GAP;
    let h = crate::shield_browse::TAB_PILL_H;
    let total: f32 = widths.iter().map(|w| w + 2.0 * pad).sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    let mut x = SCR_W * 0.5 - total * 0.5;
    widths
        .iter()
        .map(|w| {
            let r = Rect::new(x, TABS_CY - h * 0.5, w + 2.0 * pad, h);
            x += r.w + gap;
            r
        })
        .collect()
}

/// The jump arrow's seconds as the slot prints them: whole seconds under 100, else the compact
/// duration ("2m", "1h 5m").
pub fn jump_text(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 100 { secs.to_string() } else { crate::fmt::dur_short(secs * 1000) }
}

/// **The transport**: the ramps, the title block, the clock, the round buttons and the bar line.
pub fn draw_transport(p: Painter, t: &Transport<'_>, measure: &dyn Measure) {
    draw_scrims(p);
    // the title block
    let title = measure.fit_line(t.title, TITLE_W, TITLE_SIZE, true);
    let ty = plx_gfx::text::text_vcenter_y(TITLE_SIZE, 1, TITLE_CY);
    p.text(title.as_ptr(), LEFT, ty, TITLE_SIZE, theme::SHIELD_PLAYER_INK, 0, 1);
    let my = plx_gfx::text::text_vcenter_y(META_SIZE, 0, META_CY);
    let mut x = LEFT;
    if !t.meta.is_empty() {
        let meta = measure.fit_line(t.meta, TITLE_W, META_SIZE, false);
        p.text(meta.as_ptr(), x, my, META_SIZE, theme::SHIELD_PLAYER_META, 0, 0);
        x += measure.width(&meta, META_SIZE, false) + TAG_GAP;
    }
    for tag in t.tags {
        let tag = measure.fit_line(tag, TITLE_W, META_SIZE, false);
        p.text(tag.as_ptr(), x, my, META_SIZE, theme::SHIELD_PLAYER_INK, 0, 0);
        x += measure.width(&tag, META_SIZE, false) + TAG_GAP;
    }
    if let Some(clock) = t.clock {
        let cy = plx_gfx::text::text_vcenter_y(CLOCK_SIZE, 0, CLOCK_CY);
        p.text(clock.as_ptr(), RIGHT, cy, CLOCK_SIZE, theme::SHIELD_PLAYER_CLOCK, 2, 0);
    }
    for (i, glyph) in BUTTON_GLYPHS.iter().enumerate() {
        crate::shield_browse::draw_round_sized(p, button_rect(i), *glyph, BUTTON_GLYPH, t.focus == Focus::Button(i));
    }
    draw_bar_line(p, t, measure);
    match t.focus {
        Focus::Tabs(i) => {
            let widths: Vec<f32> = t.tabs.iter().map(|l| measure.width(l, crate::shield_browse::TAB_SIZE, false)).collect();
            crate::shield_browse::draw_pills(p, t.tabs, &tab_rects(&widths), None, Some(i));
        }
        _ if !t.tabs.is_empty() => icons::draw(p, Icon::ChevronDown, chevron_rect(), theme::SHIELD_PLAYER_CHEVRON),
        _ => {}
    }
}

/// The two ramps: black at the top edge clearing by [`TOP_SCRIM_H`], clear at [`FOOT_SCRIM_Y`]
/// darkening to the foot.
pub fn draw_scrims(p: Painter) {
    p.rect(Rect::new(0.0, 0.0, SCR_W, TOP_SCRIM_H), 0.0, theme::SHIELD_PLAYER_SCRIM, theme::SHIELD_PLAYER_SCRIM_CLEAR, 0.0);
    p.rect(Rect::new(0.0, FOOT_SCRIM_Y, SCR_W, SCR_H - FOOT_SCRIM_Y), 0.0, theme::SHIELD_PLAYER_SCRIM_CLEAR,
        theme::SHIELD_PLAYER_SCRIM, 0.0);
}

/// The bar's line: the glyph slot, the elapsed clock, the bar with its played run and dot, and the
/// remaining clock.
fn draw_bar_line(p: Painter, t: &Transport<'_>, measure: &dyn Measure) {
    let (x0, x1) = bar_span(t.dur_ms, measure);
    let track = bar_rect(x0, x1);
    p.rect(track, BAR_H * 0.5, theme::SHIELD_PLAYER_TRACK, theme::SHIELD_PLAYER_TRACK, 0.0);
    let frac = if t.dur_ms > 0 { (t.pos_ms as f64 / t.dur_ms as f64).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let played = track.w * frac;
    if played >= 1.0 {
        let run = Rect::new(track.x, track.y, played, track.h);
        p.rect(run, (played * 0.5).min(BAR_H * 0.5), theme::SHIELD_PLAYER_PLAYED, theme::SHIELD_PLAYER_PLAYED, 0.0);
    }
    if t.focus == Focus::Bar {
        let k = Rect::new(track.x + played - KNOB * 0.5, BAR_CY - KNOB * 0.5, KNOB, KNOB);
        p.rect(k, KNOB * 0.5, theme::SHIELD_PLAYER_KNOB, theme::SHIELD_PLAYER_KNOB, 0.0);
    }
    let ty = plx_gfx::text::text_vcenter_y(TIME_SIZE, 0, BAR_CY);
    if let Ok(el) = std::ffi::CString::new(time_text(t.pos_ms, false)) {
        p.text(el.as_ptr(), x0 - TIME_GAP, ty, TIME_SIZE, theme::SHIELD_PLAYER_INK, 2, 0);
    }
    if t.dur_ms > 0 {
        if let Ok(rem) = std::ffi::CString::new(time_text(t.dur_ms - t.pos_ms, true)) {
            p.text(rem.as_ptr(), RIGHT, ty, TIME_SIZE, theme::SHIELD_PLAYER_INK, 2, 0);
        }
    }
    draw_glyph(p, t.glyph, t.now);
}

/// The glyph slot.
fn draw_glyph(p: Painter, glyph: Glyph, now: u32) {
    let ink = theme::SHIELD_PLAYER_INK;
    let boxed = |g: f32, cx: f32| Rect::new(cx - g * 0.5, BAR_CY - g * 0.5, g, g);
    match glyph {
        Glyph::Pause => icons::draw(p, Icon::Pause, boxed(GLYPH, GLYPH_CX), ink),
        Glyph::Play => icons::draw(p, Icon::Play, boxed(GLYPH, GLYPH_CX), ink),
        Glyph::Jump { forward, secs } => {
            let icon = if forward { Icon::JumpForward } else { Icon::JumpBack };
            icons::draw(p, icon, Rect::new(JUMP_X, BAR_CY - JUMP_GLYPH * 0.5, JUMP_GLYPH, JUMP_GLYPH), ink);
            if let Ok(n) = std::ffi::CString::new(jump_text(secs)) {
                let ny = plx_gfx::text::text_vcenter_y(JUMP_NUMBER_SIZE, 1, BAR_CY);
                p.text(n.as_ptr(), JUMP_X + JUMP_GLYPH + 2.0, ny, JUMP_NUMBER_SIZE, ink, 0, 1);
            }
        }
        // placeholder-exempt: the transport's inline busy mark, reported by the playback pipeline, not a content stand-in
        Glyph::Working => Spinner::new(GLYPH_CX, BAR_CY, Spinner::R_INLINE).phase(now).tint(ink).draw(&Env::inert(), p),
    }
}

// ---- the Skip pill (w016, e001) -----------------------------------------------------------------

/// The pill: 64 tall at y 944, its words ~24 with 24 either side, ending on [`RIGHT`].
pub const SKIP_Y: f32 = 944.0;
pub const SKIP_H: f32 = 64.0;
pub const SKIP_PAD: f32 = 24.0;
pub const SKIP_SIZE: c_int = theme::size::CAPTION;

/// The pill around words `label_w` wide.
pub fn skip_rect(label_w: f32) -> Rect {
    let w = label_w + 2.0 * SKIP_PAD;
    Rect::new(RIGHT - w, SKIP_Y, w, SKIP_H)
}

/// **The Skip pill** in `r`, `label` centred in it.
pub fn draw_skip(p: Painter, r: Rect, label: &CStr) {
    p.rect(r, r.h * 0.5, theme::SHIELD_SKIP_FILL, theme::SHIELD_SKIP_FILL, 0.0);
    let ty = plx_gfx::text::text_vcenter_y(SKIP_SIZE, 0, r.cy());
    p.text(label.as_ptr(), r.cx(), ty, SKIP_SIZE, theme::SHIELD_SKIP_INK, 1, 0);
}

// ---- the post-play page (e022) ------------------------------------------------------------------

/// The page's left edge, its headings (bold ~32: "Just Played" 77..99, "On Deck" 441..463) and
/// the two leading cards: 376x212 stills at y 128, Just Played at 64 and Up Next at 472.
pub const PP_X: f32 = 64.0;
pub const PP_HEADING_SIZE: c_int = theme::size::HEADLINE;
pub const PP_HEADING_CY: f32 = 89.0;
pub const PP_CARD_W: f32 = 376.0;
pub const PP_CARD_H: f32 = 212.0;
pub const PP_CARD_Y: f32 = 128.0;
pub const PP_NEXT_X: f32 = 472.0;
/// A focused card's pop (e022: the Up Next still ~391 wide inside its ring at 458..861).
pub const PP_FOCUS_SCALE: f32 = 1.04;
pub const PP_CARD_RADIUS: f32 = 12.0;
/// The glyph disc over a leading card's still.
pub const PP_DISC: f32 = 64.0;
pub const PP_DISC_GLYPH: f32 = 32.0;
/// The next episode's block beside the cards: its title (bold ~32, centred at 138), its address
/// (199) and its summary (~26 on a 32 line, three lines at most) from x 872.
pub const PP_INFO_X: f32 = 872.0;
pub const PP_INFO_W: f32 = RIGHT - PP_INFO_X;
pub const PP_INFO_TITLE_CY: f32 = 138.0;
pub const PP_INFO_ADDRESS_CY: f32 = 199.0;
pub const PP_SUMMARY_Y: f32 = 236.0;
pub const PP_SUMMARY_SIZE: c_int = theme::size::LABEL;
pub const PP_SUMMARY_LINE: f32 = 32.0;
pub const PP_SUMMARY_LINES: usize = 3;
/// *On Deck*: its heading at 452 and 512x288 stills from y 492, 32 apart.
pub const PP_DECK_HEADING_CY: f32 = 452.0;
pub const PP_DECK_Y: f32 = 492.0;
pub const PP_DECK_W: f32 = 512.0;
pub const PP_DECK_H: f32 = 288.0;
pub const PP_DECK_GAP: f32 = 32.0;
/// The furthest right a focused deck card may stand before the row slides.
pub const PP_DECK_RIGHT: f32 = SCR_W - PP_X;

/// Which card of the post-play page holds focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostFocus {
    JustPlayed,
    UpNext,
    Deck(usize),
}

/// One card: its still (`sid`'s path) and its two caption lines.
#[derive(Clone, Copy, Debug)]
pub struct Card<'a> {
    pub sid: u16,
    pub thumb: &'a str,
    pub title: &'a str,
    pub caption: &'a str,
}

/// Everything [`draw_post_play`] draws from.
pub struct PostPlay<'a> {
    pub just_played_heading: &'a str,
    /// "Up Next", with the countdown's seconds after it while it runs.
    pub up_next_heading: &'a str,
    pub deck_heading: &'a str,
    pub just_played: Card<'a>,
    pub up_next: Card<'a>,
    pub next_title: &'a str,
    pub next_address: &'a str,
    pub next_summary: &'a str,
    pub deck: &'a [Card<'a>],
    pub deck_scroll: f32,
    pub focus: PostFocus,
}

/// The Just Played card's resting rect.
pub fn just_played_rect() -> Rect {
    Rect::new(PP_X, PP_CARD_Y, PP_CARD_W, PP_CARD_H)
}

/// The Up Next card's resting rect.
pub fn up_next_rect() -> Rect {
    Rect::new(PP_NEXT_X, PP_CARD_Y, PP_CARD_W, PP_CARD_H)
}

/// Deck card `i`'s resting rect, the row slid left by `scroll`.
pub fn deck_rect(i: usize, scroll: f32) -> Rect {
    Rect::new(PP_X + i as f32 * (PP_DECK_W + PP_DECK_GAP) - scroll, PP_DECK_Y, PP_DECK_W, PP_DECK_H)
}

/// The row's scroll that keeps deck card `i` inside [`PP_X`]..[`PP_DECK_RIGHT`], moving it as little
/// as possible from `current`.
pub fn deck_scroll_for(i: usize, current: f32) -> f32 {
    let r = deck_rect(i, 0.0);
    let mut s = current.max(0.0);
    if r.x + r.w - s > PP_DECK_RIGHT {
        s = r.x + r.w - PP_DECK_RIGHT;
    }
    if r.x - s < PP_X {
        s = r.x - PP_X;
    }
    s.max(0.0)
}

/// A card drawn at `r`, popped and ringed while focused.
fn draw_pp_card(p: Painter, r: Rect, card: &Card<'_>, res: (c_int, c_int), focused: bool) {
    let s = if focused { PP_FOCUS_SCALE } else { 1.0 };
    let drawn = Rect::new(r.cx() - r.w * s * 0.5, r.cy() - r.h * s * 0.5, r.w * s, r.h * s);
    let rad = PP_CARD_RADIUS * s;
    crate::widgets::card_flat(p, drawn, Art::Thumb { sid: card.sid, key: card.thumb, res }, rad, s);
    if focused {
        p.rring(crate::shield::ring_rect(drawn), crate::shield::ring_radius(rad), crate::shield::RING_W, theme::SHIELD_RING);
    }
}

/// The glyph disc over a leading card.
fn draw_disc(p: Painter, r: Rect, glyph: Icon) {
    let d = Rect::new(r.cx() - PP_DISC * 0.5, r.cy() - PP_DISC * 0.5, PP_DISC, PP_DISC);
    p.rect(d, PP_DISC * 0.5, theme::SHIELD_POSTPLAY_GLYPH_DISC, theme::SHIELD_POSTPLAY_GLYPH_DISC, 0.0);
    let g = Rect::new(r.cx() - PP_DISC_GLYPH * 0.5, r.cy() - PP_DISC_GLYPH * 0.5, PP_DISC_GLYPH, PP_DISC_GLYPH);
    icons::draw(p, glyph, g, theme::SHIELD_POSTPLAY_HEADING);
}

/// A heading's line at `cy` from `x`.
fn draw_heading(p: Painter, text: &str, x: f32, cy: f32, w: f32, measure: &dyn Measure) {
    let line = measure.fit_line(text, w, PP_HEADING_SIZE, true);
    let ty = plx_gfx::text::text_vcenter_y(PP_HEADING_SIZE, 1, cy);
    p.text(line.as_ptr(), x, ty, PP_HEADING_SIZE, theme::SHIELD_POSTPLAY_HEADING, 0, 1);
}

/// A card's two caption lines under its resting rect `r` (the grid's caption: title, then the
/// quieter line; a focused title that does not fit glides).
fn draw_card_caption(p: Painter, r: Rect, card: &Card<'_>, focused: bool, measure: &dyn Measure) {
    let label = TileLabel::titled(card.title, card.caption);
    let s = if focused { PP_FOCUS_SCALE } else { 1.0 };
    crate::shield_browse::draw_caption(p, r, &label, s, focused, measure);
}

/// **The post-play page**, opaque over the whole screen.
pub fn draw_post_play(p: Painter, pp: &PostPlay<'_>, measure: &dyn Measure) {
    p.ambient(Rect::FULL, 1.0, theme::SHIELD_POSTPLAY_GROUND.map(|c| [c[0], c[1], c[2]]));
    draw_heading(p, pp.just_played_heading, PP_X, PP_HEADING_CY, PP_NEXT_X - PP_X - 16.0, measure);
    draw_heading(p, pp.up_next_heading, PP_NEXT_X, PP_HEADING_CY, PP_INFO_X - PP_NEXT_X, measure);
    let still = (480, 270);
    let jp = just_played_rect();
    draw_pp_card(p, jp, &pp.just_played, still, pp.focus == PostFocus::JustPlayed);
    draw_disc(p, jp, Icon::Restart);
    draw_card_caption(p, jp, &pp.just_played, pp.focus == PostFocus::JustPlayed, measure);
    let un = up_next_rect();
    draw_pp_card(p, un, &pp.up_next, still, pp.focus == PostFocus::UpNext);
    draw_disc(p, un, Icon::Play);
    draw_card_caption(p, un, &pp.up_next, pp.focus == PostFocus::UpNext, measure);
    draw_heading(p, pp.next_title, PP_INFO_X, PP_INFO_TITLE_CY, PP_INFO_W, measure);
    draw_heading(p, pp.next_address, PP_INFO_X, PP_INFO_ADDRESS_CY, PP_INFO_W, measure);
    if !pp.next_summary.is_empty() {
        TextView::new(pp.next_summary, PP_SUMMARY_SIZE, theme::SHIELD_HERO_TEXT)
            .with_measure(measure)
            .leading(PP_SUMMARY_LINE)
            .max_lines(PP_SUMMARY_LINES)
            .draw(p, Rect::new(PP_INFO_X, PP_SUMMARY_Y, PP_INFO_W, 0.0));
    }
    if pp.deck.is_empty() {
        return;
    }
    draw_heading(p, pp.deck_heading, PP_X, PP_DECK_HEADING_CY, PP_INFO_W, measure);
    for (i, card) in pp.deck.iter().enumerate() {
        let r = deck_rect(i, pp.deck_scroll);
        if r.x > SCR_W || r.x + r.w < 0.0 {
            continue;
        }
        let focused = pp.focus == PostFocus::Deck(i);
        draw_pp_card(p, r, card, (PP_DECK_W as c_int, PP_DECK_H as c_int), focused);
        draw_card_caption(p, r, card, focused, measure);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// i04/p01: the bar runs between the clock columns — a film's (H:MM:SS) shorter than an
    /// episode's (MM:SS) — the elapsed clock ends 17 before it and the remaining clock ends on 1848.
    #[test]
    fn the_bar_runs_between_the_two_clock_columns() {
        assert_eq!(time_template(2 * 3_600_000 + 11 * 60_000), "0:00:00");
        assert_eq!(time_template(19 * 60_000), "00:00");
        assert_eq!(time_text(105_000, false), "1:45");
        assert_eq!(time_text(17 * 60_000 + 49_000, true), "-17:49");
        // the measured columns: "0:00:00" ~87 and "-0:00:00" ~95 put the bar at 241..1703
        let (x0, x1) = bar_span_of(87.0, 95.0);
        assert_eq!((x0, x1), (241.0, 1703.0));
        // an episode's narrower columns lengthen it at both ends
        let (e0, e1) = bar_span_of(62.0, 74.0);
        assert!(e0 < x0 && e1 > x1);
        assert_eq!(bar_rect(x0, x1), Rect::new(241.0, 970.0, 1462.0, 10.0));
        // a measure with no width keeps the bar at least a point long
        let (z0, z1) = bar_span_of(2000.0, 0.0);
        assert!(z1 >= z0);
    }

    /// i04/h03: the round buttons are 64 discs at 1708 and 1784 (12 apart, the last ending on 1848),
    /// their top at 870; the chevron centres at 960,1011.
    #[test]
    fn the_round_buttons_and_the_chevron_stand_where_they_were_measured() {
        assert_eq!(button_rect(0), Rect::new(1708.0, 870.0, 64.0, 64.0));
        assert_eq!(button_rect(1), Rect::new(1784.0, 870.0, 64.0, 64.0));
        assert_eq!(chevron_rect().cx(), 960.0);
        assert_eq!(chevron_rect().cy(), 1011.0);
        let pills = tab_rects(&[60.0, 110.0]);
        assert_eq!(pills.len(), 2);
        assert!((pills[0].x + (pills[1].x + pills[1].w) - SCR_W).abs() < 0.01, "the pills centre on the screen");
        assert!(pills.iter().all(|r| r.y + r.h <= SCR_H));
    }

    /// w016: the Skip pill is 64 tall at 944, its right end on 1848, 24 of air either side of
    /// its words ("Skip Intro" ~98 makes 1702..1848).
    #[test]
    fn the_skip_pill_ends_on_the_right_margin() {
        let r = skip_rect(98.0);
        assert_eq!(r, Rect::new(1702.0, 944.0, 146.0, 64.0));
        assert_eq!(r.x + r.w, 1848.0);
    }

    /// The jump arrow prints whole seconds, and a long hold's gathered time compactly.
    #[test]
    fn the_jump_arrow_prints_its_seconds() {
        assert_eq!(jump_text(10), "10");
        assert_eq!(jump_text(90), "90");
        assert_eq!(jump_text(-3), "0");
        assert_eq!(jump_text(300), crate::fmt::dur_short(300_000));
    }

    /// e022: Just Played at 64,128 and Up Next at 472,128, 376x212 each; On Deck's 512x288 stills
    /// from 64,492 on a 544 pitch; a focused card past the right margin slides the row just enough.
    #[test]
    fn the_post_play_page_stands_where_it_was_measured() {
        assert_eq!(just_played_rect(), Rect::new(64.0, 128.0, 376.0, 212.0));
        assert_eq!(up_next_rect(), Rect::new(472.0, 128.0, 376.0, 212.0));
        assert_eq!(deck_rect(0, 0.0), Rect::new(64.0, 492.0, 512.0, 288.0));
        assert_eq!(deck_rect(1, 0.0).x, 608.0);
        assert_eq!(deck_rect(2, 0.0).x, 1152.0);
        assert_eq!(deck_scroll_for(2, 0.0), 0.0, "the third card ends at 1664, inside the margin");
        let s = deck_scroll_for(3, 0.0);
        assert_eq!(deck_rect(3, s).x + PP_DECK_W, PP_DECK_RIGHT);
        assert_eq!(deck_scroll_for(0, s), 0.0, "back to the first card, the row returns");
        // the Up Next ring: the popped still inside a 4px ring 2px out (e022: 458..861)
        let s = PP_FOCUS_SCALE;
        let un = up_next_rect();
        let drawn = Rect::new(un.cx() - un.w * s * 0.5, un.cy() - un.h * s * 0.5, un.w * s, un.h * s);
        let ring = crate::shield::ring_rect(drawn);
        assert!((ring.x - 458.0).abs() < 1.0 && (ring.x + ring.w - 861.5).abs() < 1.0, "{ring:?}");
    }
}
