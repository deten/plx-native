//! **Home in the Shield layout** (`plx_ui::layout::shield()`; SHIELD-LAYOUT-SPEC §4.1, §5.1, §9.1).
//!
//! The same page, the same catalog projection, keys, groups, paging and press paths; what differs
//! is how the rows sit and what stands above them:
//!
//! - **No billboard.** The hero group holds no controls (only the hub read-out's action while there
//!   are no rows), the hero↔grid dive is pinned on the shelves, and focus seats on the card in the
//!   first row's slot ([`HomeScreen::shield_seat_first`]). UP from the first row has nowhere to go:
//!   no strip is published and no link leads above it. LEFT off a row's first card is the side
//!   panel's door (`Rig::edge_surface`): the row's neighbour step says `Edge` at index 0 exactly as
//!   in Standard.
//! - **A fixed slot.** The rows wear `RowStyle::SHIELD_POSTER`, whose focused card always rests in
//!   the first slot; the focused row's cards always sit at `shield::ROW_CARD_Y`, the others a
//!   `shield::ROW_PITCH` apart around it, gliding on the page's vertical spring
//!   ([`HomeScreen::shield_reveal`]); rows above the slot fade and are cut under the header.
//! - **A header that follows focus** ([`Header`]): the focused card's item — its clearLogo (title
//!   text until the logo lands), facts, summary and cast, all from the catalog row — over its
//!   backdrop and a ground keyed from its UltraBlur colours. The subject changes once focus has
//!   RESTED on a card for [`SETTLE_MS`] (immediately for the first), so a held key running along a
//!   row asks the server for no backdrop or logo it passes; a settled subject warms the next card's
//!   logo and backdrop while the source is idle.
//! - **Live.** The layout is latched once per event ([`HomeScreen::sync_layout`]): a switch while
//!   Home is mounted re-dresses the shelves in place, re-seats the dive, the slot and focus, and
//!   resets the header, so the page is never laid out by one layout and drawn by the other.

use std::sync::Arc;

use plx_data::pms::PmsMovie;
use plx_ui::shield as geo;

use super::*;

/// How long focus must rest on a card before the header takes it as its subject, ms of `Tick`.
pub(super) const SETTLE_MS: u32 = 200;
/// The header text's cross-fade into a new subject (the focus pop's rate: a quick dissolve).
const K_FADE: f32 = K_SCALE;

/// What the header says about its subject, formatted once per subject rather than per frame.
#[derive(Default)]
struct Facts {
    sid: u16,
    logo_rk: String,
    title: String,
    subtitle: String,
    facts: Vec<String>,
    rating: String,
    summary: String,
    cast: String,
}

impl Facts {
    /// An episode or a season shows its SHOW's logo (falling back to the show's title) over its own
    /// name; anything else its own. The facts follow the measured rows: an episode's address, air
    /// date and runtime; a movie's year, runtime and genres; a show's year and genres.
    fn of(m: &PmsMovie) -> Self {
        let child = matches!(m.kind, 2 | 3) && !m.show_rk.is_empty();
        let (logo_rk, title, subtitle) = if child {
            let show = if m.show_title.is_empty() { &m.title } else { &m.show_title };
            (m.show_rk.clone(), show.clone(), m.title.clone())
        } else {
            (m.rk.clone(), m.title.clone(), String::new())
        };
        let runtime = (m.dur_ns > 0).then(|| plx_ui::fmt::dur_short(m.dur_ns / 1_000_000));
        let year = (m.year > 0).then(|| m.year.to_string());
        let genres = (!m.genres.is_empty()).then(|| m.genres.iter().take(2).cloned().collect::<Vec<_>>().join(", "));
        let facts: Vec<String> = match m.kind {
            3 => [
                Some(plx_ui::fmt::episode_address(i64::from(m.season_index), i64::from(m.ep_index))),
                Some(plx_ui::fmt::pretty_date(&m.aired, i64::from(m.year))),
                runtime,
            ].into_iter().flatten().collect(),
            2 => [year].into_iter().flatten().collect(),
            plx_data::pms::KIND_COLLECTION => vec![plx_ui::fmt::item_count(m.child_count)],
            _ => [year, runtime, genres].into_iter().flatten().collect(),
        };
        Facts {
            sid: m.sid.raw(),
            logo_rk,
            title,
            subtitle,
            facts: facts.into_iter().filter(|f| !f.is_empty()).collect(),
            rating: m.rating.clone(),
            summary: m.summary.clone(),
            cast: m.cast.join(", "),
        }
    }

    fn view(&self) -> geo::HeroFacts<'_> {
        geo::HeroFacts {
            sid: self.sid,
            logo_rk: &self.logo_rk,
            title: &self.title,
            subtitle: &self.subtitle,
            facts: &self.facts,
            rating: &self.rating,
            summary: &self.summary,
            cast: &self.cast,
        }
    }
}

/// **The focused-item header**: its subject, the debounce that picks it, its formatted facts, the
/// text's cross-fade and the ground under it. Paint state: no focus, reveal or press decision reads
/// it (`LogicalState` leaves it out, as it does the Standard billboard's `Backdrop`).
pub(super) struct Header {
    subject: Option<Arc<PmsMovie>>,
    /// The card focus is on while it has not yet rested long enough to be the subject, and the
    /// `Tick` time it arrived at (an absolute clock, never an accumulated delta).
    pending: Option<(plx_plex::plex::ServerId, String, u32)>,
    fade: Spring,
    facts: Facts,
    ground: geo::HeroGround,
}

impl Header {
    pub(super) fn new() -> Self {
        Self { subject: None, pending: None, fade: Spring::at(1.0), facts: Facts::default(),
            ground: geo::HeroGround::new() }
    }

    pub(super) fn reset(&mut self) {
        *self = Self::new();
    }

    /// The item the header describes.
    #[cfg(test)]
    pub(super) fn subject(&self) -> Option<&PmsMovie> {
        self.subject.as_deref()
    }

    /// Is a focused card still waiting out the settle?
    pub(super) fn settling(&self) -> bool {
        self.pending.is_some()
    }

    /// One frame at `t` with `focused` the card focus is on (`None` on a heading or nothing: the
    /// subject stays). Returns whether the subject changed, for the caller to ask for a frame: the
    /// settle is a TIMER on the page's `Tick`, which runs whether or not a frame presents, and
    /// reports no motion of its own (the billboard's countdown rule).
    pub(super) fn step(&mut self, focused: Option<&Arc<PmsMovie>>, t: Tick) -> bool {
        let dt = t.dt();
        let mut changed = false;
        if let Some(item) = focused.filter(|m| !m.rk.is_empty()) {
            let same = self.subject.as_ref().is_some_and(|s| s.sid == item.sid && s.rk == item.rk);
            if same {
                self.pending = None;
                // the catalog republished the row (a resume point moved): describe the new copy
                if !self.subject.as_ref().is_some_and(|s| Arc::ptr_eq(s, item)) {
                    self.facts = Facts::of(item);
                    self.subject = Some(Arc::clone(item));
                }
            } else {
                if !self.pending.as_ref().is_some_and(|(sid, rk, _)| *sid == item.sid && *rk == item.rk) {
                    self.pending = Some((item.sid, item.rk.clone(), t.ms));
                }
                let since = self.pending.as_ref().map_or(t.ms, |pending| pending.2);
                if self.subject.is_none() || t.ms.wrapping_sub(since) >= SETTLE_MS {
                    self.show(item);
                    changed = true;
                }
            }
        } else {
            self.pending = None;
        }
        let blur = self.subject.as_ref().filter(|m| m.has_blur).map(|m| m.blur);
        self.ground.update(geo::ground_target(blur), dt);
        // decorative: the header's dissolve must not hold the page-freeze snapshot
        plx_machine::idle::decorative(|| self.fade.step(1.0, K_FADE, dt));
        changed
    }

    fn show(&mut self, item: &Arc<PmsMovie>) {
        let first = self.subject.is_none();
        self.facts = Facts::of(item);
        self.subject = Some(Arc::clone(item));
        self.pending = None;
        self.ground.subject((!item.art.is_empty()).then(|| (item.sid.raw(), item.art.as_str())));
        if !first {
            self.fade.jump(0.0);
        }
    }

    /// While the subject has settled and the source is idle, warm `next`'s logo, then its backdrop:
    /// the card one step along the row, so the common single step finds both resident. One key a
    /// frame, the billboard's prefetch rule.
    pub(super) fn warm(&self, next: Option<&PmsMovie>) {
        use plx_ui::tex::Warm;
        let Some(next) = next.filter(|_| !self.settling() && plx_ui::tex::source_idle()) else { return };
        let logo_rk = Facts::of(next).logo_rk;
        if plx_ui::tex::logo_warm(next.sid.raw(), &logo_rk) == Warm::Claimed {
            return;
        }
        plx_ui::tex::warm_on(next.sid.raw(), &next.art, 1280, 720, false);
    }

    /// The ground: the tinted wash and the subject's backdrop.
    pub(super) fn draw_ground(&self, p: Painter) {
        self.ground.draw(p);
    }

    /// The header's column, at the subject cross-fade.
    pub(super) fn draw_text(&self, p: Painter, measure: &dyn Measure) {
        if self.subject.is_some() {
            geo::draw_hero(p.alpha(self.fade.pos), &self.facts.view(), measure);
        }
    }
}

/// Is a shelf whose cards' top is at `top` on screen below the header's cut?
pub(super) fn shelf_visible_at(top: f32) -> bool {
    top - geo::HEADING_DY < SCR_H && top + geo::CARD_H > geo::ROWS_CLIP_TOP
}

impl HomeScreen {
    /// The row style this page's shelves wear in its latched layout.
    pub(super) fn row_style(&self) -> &'static RowStyle {
        if self.shield { &RowStyle::SHIELD_POSTER } else { &RowStyle::HOME }
    }

    /// **Latch the live layout for this event.** A change while the page is mounted re-dresses it
    /// in place rather than remounting: every shelf keeps its place in its new pitch
    /// (`Shelf::restyle`), the focused one rests its card in the slot (Shield); the dive pins on
    /// the shelves (Shield) or follows focus (Standard); the vertical scroll jumps to where the new
    /// layout settles; a focus the new layout cannot hold (a billboard control in Shield) re-seats
    /// on the first row. Nothing animates across the switch: it is a change of layout, not a move.
    pub(super) fn sync_layout<H: HomeLike>(&mut self, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        let shield = plx_ui::layout::shield();
        if shield == self.shield {
            return;
        }
        self.shield = shield;
        let style = self.row_style();
        let focus = self.focused_grid(cx.focus.current);
        for (row, shelf) in self.grid.shelves.iter_mut().enumerate() {
            let n = self.rows.get(row).map_or(0, |r| r.elems.len());
            shelf.restyle(style, n);
            if let Some((_, col)) = focus.filter(|(r, _)| shield && *r == row) {
                shelf.restore_scroll(ui_cards::fixed_left_scroll(col, n, style), n);
            }
        }
        self.header.reset();
        self.visible_activation = None;
        if shield {
            let on_rows = !self.rows.is_empty();
            self.snap_target = f32::from(on_rows);
            self.snap.jump(self.snap_target);
            self.grid.scroll_target = self.slot_row(cx.focus.current) as f32 * geo::ROW_PITCH;
        } else {
            let on_grid = self.focused_loc(cx.focus.current).is_some_and(Located::on_grid);
            self.snap_target = f32::from(on_grid && !self.rows.is_empty());
            self.snap.jump(self.snap_target);
            self.grid.scroll_target = match self.focused_loc(cx.focus.current) {
                Some(Located::Item(row, _) | Located::Heading(row)) => {
                    let (lo, hi) = row_reveal_band(shelf_top_settled(row, row));
                    ui_cards::reveal(0.0, lo, hi, grid_max_scroll(self.rows.len()))
                }
                _ => 0.0,
            };
        }
        self.grid.scroll_y.jump(self.grid.scroll_target);
        self.layout_grid();
        if self.wants_shield_seat(cx.focus.current, H::hubs(cx)) {
            self.shield_seat_first(cx, fx);
        }
        fx.invalidate(Provenance::Nav);
    }

    /// The row the slot holds for `focus`: the focused card's or linked heading's row, else the
    /// row the vertical spring is going to.
    pub(super) fn slot_row(&self, focus: Option<FocusKey<u32>>) -> usize {
        match self.focused_loc(focus) {
            Some(Located::Item(row, _) | Located::Heading(row)) => row,
            _ => (self.grid.scroll_target / geo::ROW_PITCH).round().max(0.0) as usize,
        }
    }

    /// Where row `row`'s cards SETTLE for `focus` (the focus engine's `At::SpringTarget`).
    pub(super) fn shield_card_top_target(&self, row: usize, focus: Option<FocusKey<u32>>) -> f32 {
        geo::row_card_top(row as f32 - self.slot_row(focus) as f32)
    }

    /// Lay every shelf out at the live vertical scroll: the shelf's `base_y` is its cards' top.
    pub(super) fn shield_layout(&mut self) {
        let scroll = self.grid.scroll_y.pos;
        for (row, shelf) in self.grid.shelves.iter_mut().enumerate() {
            shelf.set_base_y(geo::ROW_CARD_Y + row as f32 * geo::ROW_PITCH - scroll);
        }
    }

    /// The vertical half of a tick: the focused (or heading's) row is the slot's, and the page
    /// glides there.
    pub(super) fn shield_reveal(&mut self, focus: Option<FocusKey<u32>>, dt: f32) {
        if let Some(Located::Item(row, _) | Located::Heading(row)) = self.focused_loc(focus) {
            if row < self.rows.len() {
                self.grid.scroll_target = row as f32 * geo::ROW_PITCH;
            }
        }
        self.grid.scroll_y.step(self.grid.scroll_target, K_SCROLL, dt);
        self.shield_layout();
    }

    /// Row `row`'s cards as the shelf draws them: their top, and the header's cut.
    pub(super) fn shield_section(&self, row: usize) -> SectionFrame {
        SectionFrame { y: self.grid.base_y(row), clip: shield_clip() }
    }

    /// The card in row `row`'s slot: the one its scroll rests in the first slot.
    fn slot_col(&self, row: usize) -> usize {
        let n = self.rows.get(row).map_or(0, |r| r.elems.len());
        ((self.grid.scroll_x(row) / geo::CARD_PITCH).round().max(0.0) as usize).min(n.saturating_sub(1))
    }

    /// Does focus need seating on the rows: there are rows, no read-out, no question, and focus is
    /// nowhere, on a strip pill, or on a billboard control this layout does not draw.
    pub(super) fn wants_shield_seat(&self, focus: Option<FocusKey<u32>>, view: HubsView<'_>) -> bool {
        self.shield
            && !self.rows.is_empty()
            && status_read(view).is_none()
            && !self.plaintext_alert.is_open()
            && focus.is_none_or(|key| {
                key.elem >= plx_ui::dispatch::STRIP_BASE
                    || matches!(self.locate(key.elem), None | Some(Located::Hero(_)))
            })
    }

    /// Seat focus on the card in the first row's slot.
    pub(super) fn shield_seat_first<H: HomeLike>(&mut self, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        let Some(&elem) = self.rows.first().and_then(|row| row.elems.get(self.slot_col(0))) else { return };
        self.reseat(FocusTarget::Elem(FocusKey { entry: self.entry, elem }), cx, fx);
    }

    /// BACK on the rows: home to the first card of the first row, and from there out of the app,
    /// as the billboard's fold and its BACK do in Standard.
    pub(super) fn shield_back<H: HomeLike>(&mut self, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        let first = self.rows.first().and_then(|row| row.elems.first().copied());
        let home = matches!(self.focused_loc(cx.focus.current), Some(Located::Item(0, 0)));
        match first.filter(|_| !home && self.focused_loc(cx.focus.current).is_some_and(Located::on_grid)) {
            Some(elem) => {
                self.reseat(FocusTarget::Elem(FocusKey { entry: self.entry, elem }), cx, fx);
                fx.invalidate(Provenance::Input);
            }
            None => fx.push(Fx::App(AppFx::Loop(LoopReq::BackAtRoot))),
        }
        Handled::Yes
    }

    /// A linked heading's face over its row's cards.
    pub(super) fn shield_heading_rect(&self, view: HubsView<'_>, row: usize, measure: &dyn Measure, at: At,
        focus: Option<FocusKey<u32>>) -> Option<Rect> {
        let heading = self.heading_widget(view, row)?;
        let m = heading.measure(measure);
        let focused = self.focused_heading(focus) == Some(row);
        let top = match at {
            At::Drawn => self.grid.base_y(row),
            At::SpringTarget => self.shield_card_top_target(row, focus),
        };
        Some(heading.face_rect(geo::CONTENT_X, geo::heading_text_y(top), f32::from(focused), &m))
    }

    /// Where card `key` of row `row` settles: the focused card in the slot, every other card of its
    /// row a pitch apart from it, another row's cards where that row rests.
    pub(super) fn shield_place<H: HomeLike>(&self, row: usize, key: u32, cx: &Cx<'_, H>) -> Option<Placed> {
        let col = self.item_col(row, key)?;
        let n = self.rows.get(row)?.elems.len();
        let style = &RowStyle::SHIELD_POSTER;
        let focused = self.focused_grid(cx.focus.current);
        let scroll = match focused {
            Some((r, c)) if r == row => ui_cards::fixed_left_scroll(c, n, style),
            _ => self.grid.scroll_x(row),
        };
        let base = Rect::new(geo::CONTENT_X + col as f32 * geo::CARD_PITCH - scroll,
            self.shield_card_top_target(row, cx.focus.current), geo::CARD_W, geo::CARD_H);
        let scale = if focused == Some((row, col)) { geo::FOCUS_SCALE } else { 1.0 };
        Some(Placed {
            rect: base.scaled(scale),
            rest_rect: base.scaled(geo::FOCUS_SCALE),
            clip: shield_clip(),
            index: Some(col as u32),
        })
    }

    /// The card a vertical move into row `row` lands on: the one in its slot (the card nearest the
    /// slot's centre, which is where the card it came from stands).
    pub(super) fn shield_seat(&self, row: usize, from: Placed) -> Option<FocusKey<u32>> {
        let projection = self.rows.get(row)?;
        let col = ui_cards::column_near_x(from.rect.cx(), geo::CONTENT_X, geo::CARD_PITCH, geo::CARD_W,
            self.grid.scroll_x(row), projection.elems.len(), from.index.unwrap_or(0) as usize);
        Some(FocusKey { entry: self.entry, elem: *projection.elems.get(col)? })
    }

    /// The Shield layout's links: a collection shelf's heading between its cards and the row above
    /// — and nothing above the first row, which has no billboard and no strip to lead to.
    pub(super) fn shield_links(&self, out: &mut Vec<Link>) {
        for (row, projection) in self.rows.iter().enumerate() {
            if projection.link.is_none() {
                continue;
            }
            let heading = heading_group(projection.group);
            out.extend(linked_heading::links(heading, projection.group));
            if let Some(above) = row.checked_sub(1).map(|r| self.rows[r].group) {
                out.push(Link { from: heading, dir: Dir::Up, to: above });
                out.push(Link { from: above, dir: Dir::Down, to: heading });
            }
        }
    }

    /// A restored page opens on its saved card in the slot, its row in the slot.
    pub(super) fn shield_restore(&mut self, row: usize, col: usize) {
        if let (Some(shelf), Some(hub)) = (self.grid.shelves.get_mut(row), self.rows.get(row)) {
            let n = hub.elems.len();
            shelf.restore_scroll(ui_cards::fixed_left_scroll(col, n, &RowStyle::SHIELD_POSTER), n);
        }
        self.grid.scroll_target = row as f32 * geo::ROW_PITCH;
        self.grid.scroll_y.jump(self.grid.scroll_target);
    }

    /// The header's tick: its subject follows the visible focus, and a settled one warms the next
    /// card along its row.
    pub(super) fn shield_header<H: HomeLike>(&mut self, view: HubsView<'_>, focus: Option<FocusKey<u32>>,
        t: Tick, fx: &mut Effects<'_, H>) {
        let at = self.focused_grid(focus);
        let focused = at.and_then(|(row, col)| self.hub(view, row)?.items.get(col));
        if self.header.step(focused, t) {
            fx.invalidate(Provenance::Input);
        }
        let next = at.and_then(|(row, col)| self.item_at(view, row, col + 1));
        self.header.warm(next);
    }

    /// **The page, Shield layout**: the ground and the header, the rows in their slot, then
    /// everything the Standard page draws over itself (the read-out, the hold hint, the question).
    pub(super) fn draw_shield<H: HomeLike>(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        plx_gfx::gfx::frame_clear(theme::CLEAR_RGB.0, theme::CLEAR_RGB.1, theme::CLEAR_RGB.2);
        let view = H::hubs(f.cx);
        let visible_focus = self.visible_focus(f.focus.current);
        let env = self.env(0.0);
        let focus = self.focused_loc(visible_focus);
        let p = f.painter.alpha(f.page_alpha);
        plx_ui::profile::phase("hm.backdrop", || self.header.draw_ground(p));
        if !self.rows.is_empty() {
            plx_ui::profile::phase("hm.hero", || self.header.draw_text(p, f.measure));
        }
        plx_ui::profile::phase("hm.grid", || {
            let heading = self.focused_heading(visible_focus);
            self.draw_shield_rows(f, view, p, heading)
        });
        plx_ui::profile::phase("hm.status", || self.draw_status(view, &env, p, focus));
        plx_ui::testpat::underlay(p);
        plx_ui::testpat::draw(p);
        self.hold_hint.draw(p, f.measure);
        if !self.plaintext_alert.visible() {
            self.record_stops(f, view);
        }
        self.plaintext_alert.draw(f, self.entry);
    }

    /// The rows, cut under the header: each row's heading and resting cards at its fade, then the
    /// focused card last so its ring stands over its neighbours.
    fn draw_shield_rows<H: HomeLike>(&self, f: &mut DrawFrame<'_, '_, H>, view: HubsView<'_>, p: Painter,
        heading: Option<usize>) {
        let _clip = f.clip(p, shield_clip());
        let fade = |top: f32| geo::row_alpha((top - geo::ROW_CARD_Y) / geo::ROW_PITCH);
        for row in 0..self.rows.len() {
            let Some(hub) = self.hub(view, row) else { continue };
            let top = self.grid.base_y(row);
            let a = fade(top);
            if a <= 0.01 || !shelf_visible_at(top) {
                continue;
            }
            let pr = p.alpha(a);
            if let Some(linked) = self.heading_widget(view, row) {
                let m = linked.measure(f.measure);
                linked.draw(pr, geo::CONTENT_X, geo::heading_text_y(top), f32::from(heading == Some(row)), &m, f.measure);
            } else {
                geo::draw_row_heading(pr, hub.title, hub.source, top, f.measure);
            }
            if let Some(shelf) = self.grid.shelves.get(row) {
                shelf.paint_resting(f, pr, &self.cards(view, row), self.section(row));
            }
        }
        if let Some((row, _)) = self.focused_grid(self.visible_focus(f.focus.current)) {
            if let (Some(shelf), Some(_)) = (self.grid.shelves.get(row), self.hub(view, row)) {
                let a = fade(self.grid.base_y(row));
                shelf.paint_focused(f, p.alpha(a), &self.cards(view, row), self.section(row));
            }
        }
    }

    /// The rows' stops: the cards of every row wholly drawn below the cut (the focused row's
    /// always), a pointer taking none of them by passing — a passing pointer would slide the row
    /// under itself — and each linked heading in view.
    pub(super) fn record_shield_stops<H: HomeLike>(&self, f: &mut DrawFrame<'_, '_, H>, view: HubsView<'_>) {
        let focus = f.focus.current;
        let focused_row = self.focused_grid(focus).map(|(row, _)| row);
        for (index, row) in self.rows.iter().enumerate() {
            let top = self.grid.base_y(index);
            let whole = geo::row_alpha((top - geo::ROW_CARD_Y) / geo::ROW_PITCH) >= 1.0 && shelf_visible_at(top);
            if !whole && focused_row != Some(index) {
                continue;
            }
            let src = HomeCards { hover: Hover::OnlyIfFocused, ..self.cards(view, index) };
            if let Some(shelf) = self.grid.shelves.get(index) {
                shelf.record_stops(f, f.painter, &src, self.section(index));
            }
            if let (Some(heading), Some(rect)) = (
                self.heading_widget(view, index),
                self.shield_heading_rect(view, index, f.measure, At::Drawn, focus),
            ) {
                if rect.y >= geo::ROWS_CLIP_TOP && rect.y + rect.h <= SCR_H {
                    heading.stop(f, rect, FocusKey { entry: self.entry, elem: heading_elem(row.group) });
                }
            }
        }
    }
}

/// The rows' cut: everything under the header's last line.
pub(super) fn shield_clip() -> Rect {
    Rect::new(0.0, geo::ROWS_CLIP_TOP, SCR_W, SCR_H - geo::ROWS_CLIP_TOP)
}
