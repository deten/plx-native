//! **The Library in the Shield layout** (`plx_ui::layout::shield()`; SHIELD-LAYOUT-SPEC §3, §4,
//! §5.1, §6, §9.1).
//!
//! The same page, the same store transactions, keys, fades, paging and press paths; what differs
//! is how its three views stand:
//!
//! - **The tabs** replace the library pill row and the view switcher: *Recommended | Browse |
//!   Categories* in a track at the top (`shield_browse::draw_tabs`), the view switcher's own group
//!   (`VIEWS_GROUP`) and its rule — moving along the tabs only moves focus, OK switches the view,
//!   and *Library opens on* still picks the view a library opens in. The side panel chooses the
//!   library (`LibraryCmd::Select`), so no library row is built. LEFT off the first tab, a
//!   shelf's first card, the first chip, the grid's first column or the first tile is the side
//!   panel's door (`Rig::edge_surface`): each of those answers `Edge` to LEFT exactly as in
//!   Standard. UP from the first block of content reaches the tabs (the document links).
//! - **Recommended** is Home's treatment over the library's own shelves
//!   (`registry::shield_header::Header`, `RowStyle::SHIELD_POSTER`): the focused card's item in a
//!   header over its tinted ground, the focused row held at `shield::ROW_CARD_Y` with the rows
//!   sliding under it one `shield::ROW_PITCH` a step, and focus resting in each row's first slot.
//!   The tabs stand still over it. No grid stands under the shelves: Browse is the grid.
//! - **Browse** (`View::All`) is the chip bar over the captioned grid on the neutral ground, with
//!   the A–Z rail at the right; the tabs and chips scroll away with the grid
//!   (`Layout::row_reveal`'s Shield rule holds the focused row at `GRID_FOCUS_ROW_Y`). The chips
//!   are the toolbar's: Filter, Type and Sort open the same menus as Standard's (as right-side
//!   panels, `menu.rs`), the round chip opens the Libraries menu (the library pills' "+N"), and
//!   the item count is a read-out.
//! - **Categories** (`View::Categories`, this layout only) is the listing's genres as tiles; OK on
//!   one shows Browse filtered by that genre. The genres are the Genre filter's own list
//!   (`LibraryWork::Genres`), asked for while the tab is up. It is not a view a library opens in,
//!   nor one *Last* remembers: *Library opens on* offers Recommended and All.
//! - **Live.** The layout is latched once per event ([`LibraryScreen::sync_layout`]): a switch
//!   while the Library is mounted re-dresses its shelves, rebuilds the head (library row or tabs),
//!   closes the grid's caption bands, re-places the scroll on the focused block and re-seats a
//!   focus the new layout cannot hold, so the page is never laid out by one layout and drawn by
//!   the other.

use std::ffi::CString;

use plx_data::stores::browse::QueryEdit;
use plx_ui::icons::Icon;
use plx_ui::screen::{Activate, Hover, Stop};
use plx_ui::shield as geo;
use plx_ui::shield_browse as sb;
use plx_ui::Painter;

use super::*;
use super::views::View;
pub(super) use crate::registry::shield_header::Header;

/// One item of the Browse chip bar: the control it is (`None` for the word and the count), how it
/// is measured, and what it says.
pub(super) struct ShieldChip {
    pub(super) elem: Option<u32>,
    pub(super) lay: sb::ChipLay,
    pub(super) text: CString,
    pub(super) glyph: Option<Icon>,
}

/// The rows' cut: everything under the header's last line.
pub(super) fn shield_clip() -> Rect {
    Rect::new(0.0, geo::ROWS_CLIP_TOP, SCR_W, SCR_H - geo::ROWS_CLIP_TOP)
}

/// The Browse grid's and the tiles' panel: the whole screen left of the rail.
pub(super) fn browse_clip() -> Rect {
    Rect::new(0.0, 0.0, sb::RAIL_X, SCR_H)
}

impl LibraryScreen {
    /// **Latch the live layout for this event.** A change while the page is mounted re-dresses it
    /// in place rather than remounting: the shelves keep their place in their new pitch
    /// (`Shelf::restyle`), the head is rebuilt for the layout (the library row, or the tabs; a
    /// Categories view has no Standard counterpart and becomes Recommended), the grid's caption
    /// bands close (a ring grid never opens one), the scroll jumps to where the focused block
    /// settles, and a focus the new layout cannot hold re-seats on the head. Nothing animates
    /// across the switch: it is a change of layout, not a move.
    pub(super) fn sync_layout<H: LibraryLike>(&mut self, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        let shield = plx_ui::layout::shield();
        if shield == self.shield {
            return;
        }
        self.shield = shield;
        if !shield && self.view == View::Categories {
            self.view = View::Recommended;
            self.shelves.clear();
            self.shelf_publication = None;
        }
        self.views_for = None;
        self.header.reset();
        self.categories.clear();
        self.pair.detail.reset_bands();
        let focus = cx.focus.current.filter(|key| key.entry == self.entry);
        for row in &mut self.shelves {
            let style = shelf_style(shield, row.landscape);
            let n = row.elems.len();
            row.cards.restyle(style, n);
            if let Some(col) = focus.and_then(|key| row.elems.iter().position(|elem| *elem == key.elem)).filter(|_| shield) {
                row.cards.restore_scroll(plx_ui::cards::fixed_left_scroll(col, n, style), n);
            }
        }
        self.libraries.clear();
        self.sync(cx);
        self.restore_scroll = None;
        let held = focus.filter(|key| <Self as Focusable<H>>::group_of(self, &key.elem, cx).is_some());
        match held {
            Some(key) => self.reveal(key, By::Dir, cx),
            None => {
                self.scroll_target = 0.0;
                self.relayout(focus);
                self.reseat(FocusTarget::ContainerGroup(self.head_group()), fx);
            }
        }
        self.scroll_target = self.scroll_target.clamp(0.0, self.target_layout.max_scroll());
        self.scroll.jump(self.scroll_target);
        self.relayout(held);
        fx.invalidate(Provenance::Nav);
    }

    /// Does the page show the grid? Both Standard views do; in Shield only Browse.
    pub(super) fn shows_grid(&self) -> bool {
        !self.shield || self.view.is_all()
    }

    /// Does the page show the Recommended shelves?
    pub(super) fn shows_shelves(&self) -> bool {
        if self.shield { self.view == View::Recommended } else { !self.view.is_all() }
    }

    /// The view elements standing on the page: the switcher's two, or the Shield tabs' three.
    pub(super) fn view_elems(&self) -> &'static [u32] {
        if self.shield { &View::SHIELD_ELEMS } else { &View::ELEMS }
    }

    // ---- the tabs ----------------------------------------------------------------------------

    /// The tabs' labels, in pill order.
    fn tab_labels() -> [&'static str; 3] {
        [
            plx_platform::i18n::msg::browse_library_view_recommended(),
            plx_platform::i18n::msg::browse_library_tab_browse(),
            plx_platform::i18n::msg::browse_library_tab_categories(),
        ]
    }

    /// The tabs' pills: fixed over Recommended's header, carried away with Browse's and
    /// Categories' document.
    fn tab_pills<H: LibraryLike>(&self, cx: &Cx<'_, H>, at: At) -> Vec<Rect> {
        let widths: Vec<f32> = Self::tab_labels().iter().map(|l| cx.measure.width_str(l, sb::TAB_SIZE, false)).collect();
        let scroll = match at { At::Drawn => self.scroll.pos, At::SpringTarget => self.scroll_target };
        let dy = if self.view == View::Recommended { 0.0 } else { -scroll };
        sb::tab_pills(&widths, dy)
    }

    /// Where tab `elem` stands, while the tabs are on the page.
    pub(super) fn shield_tab_rect<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>, at: At) -> Option<Rect> {
        let index = View::SHIELD_ELEMS.iter().position(|e| *e == elem).filter(|_| self.layout.views)?;
        self.tab_pills(cx, at).get(index).copied()
    }

    // ---- the chip bar ------------------------------------------------------------------------

    /// The Browse chip bar, in order: the filter, the type, "by", the sort (with its direction),
    /// the item count, the round Libraries chip.
    pub(super) fn shield_chips<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Vec<ShieldChip> {
        let measure = cx.measure;
        let chip = |elem: u32, text: String, glyph: Option<Icon>| {
            let text = CString::new(text).unwrap_or_default();
            let w = measure.width(&text, sb::CHIP_SIZE, false);
            ShieldChip { elem: Some(elem), lay: sb::ChipLay::Chip { w, glyph: glyph.is_some() }, text, glyph }
        };
        let mut out = Vec::new();
        for &elem in self.toolbar_elems() {
            match elem {
                FILTER => out.push(chip(FILTER, self.filter_value(cx), None)),
                TYPE => {
                    out.push(chip(TYPE, self.type_value(cx), None));
                    let word = CString::new(plx_platform::i18n::msg::browse_library_by()).unwrap_or_default();
                    let w = measure.width(&word, sb::CHIP_SIZE, false);
                    out.push(ShieldChip { elem: None, lay: sb::ChipLay::Word { w }, text: word, glyph: None });
                }
                SORT => {
                    let (title, desc) = self.sort_value(cx);
                    out.push(chip(SORT, title, Some(if desc { Icon::ChevronDown } else { Icon::ChevronUp })));
                    let total = H::listing(cx).total();
                    if total >= 0 {
                        let figures = CString::new(plx_ui::fmt::item_count(total)).unwrap_or_default();
                        let w = measure.width(&figures, sb::COUNT_SIZE, false);
                        out.push(ShieldChip { elem: None, lay: sb::ChipLay::Count { w }, text: figures, glyph: None });
                    }
                }
                _ => out.push(ShieldChip { elem: Some(elem), lay: sb::ChipLay::Round, text: CString::default(),
                    glyph: Some(Icon::MoreVertical) }),
            }
        }
        out
    }

    /// The chip bar's rects, where the document puts it now (`Drawn`) or is taking it.
    fn chip_rects<H: LibraryLike>(&self, chips: &[ShieldChip], at: At) -> Vec<Rect> {
        let (layout, scroll) = match at {
            At::Drawn => (&self.layout, self.scroll.pos),
            At::SpringTarget => (&self.target_layout, self.scroll_target),
        };
        let top = layout.content_top() + layout.grid_block_top() - scroll;
        let lays: Vec<sb::ChipLay> = chips.iter().map(|c| c.lay).collect();
        sb::chip_bar(&lays, top - sb::CHIP_Y)
    }

    /// Where chip `elem` stands.
    pub(super) fn shield_chip_rect<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>, at: At) -> Rect {
        let chips = self.shield_chips(cx);
        let rects = self.chip_rects::<H>(&chips, at);
        chips.iter().zip(rects).find(|(c, _)| c.elem == Some(elem)).map_or(Rect::new(geo::CONTENT_X, sb::CHIP_Y, 0.0, 0.0), |(_, r)| r)
    }

    // ---- the Recommended rows ----------------------------------------------------------------

    /// Shelf `index`'s cards' top at page scroll `scroll`: the focused row's at `ROW_CARD_Y`.
    pub(super) fn shield_shelf_top(&self, run: &ShelfRun, index: usize, scroll: f32) -> f32 {
        geo::ROW_CARD_Y + run.origin(index) - scroll
    }

    /// Where shelf `index` is drawn: its cards' top, cut under the header.
    pub(super) fn shield_shelf_frame(&self, index: usize) -> SectionFrame {
        SectionFrame { y: self.shield_shelf_top(&self.run, index, self.scroll.pos), clip: shield_clip() }
    }

    /// Where card `col` of shelf `index` SETTLES: the focused card in the slot, every other card of
    /// its row a pitch from it, another row's cards where that row rests.
    pub(super) fn shield_shelf_rect_at<H: LibraryLike>(&self, index: usize, col: usize, cx: &Cx<'_, H>) -> Rect {
        let row = &self.shelves[index];
        let style = &RowStyle::SHIELD_POSTER;
        let n = row.elems.len();
        let focused = cx.focus.current.filter(|key| key.entry == self.entry)
            .and_then(|key| row.elems.iter().position(|elem| *elem == key.elem));
        let scroll = focused.map_or(row.cards.scroll(), |c| plx_ui::cards::fixed_left_scroll(c, n, style));
        let top = self.shield_shelf_top(&self.target_run, index, self.scroll_target);
        Rect::new(geo::CONTENT_X + col as f32 * geo::CARD_PITCH - scroll, top, geo::CARD_W, geo::CARD_H)
            .scaled(if focused == Some(col) { geo::FOCUS_SCALE } else { 1.0 })
    }

    /// A linked heading's face over shelf `index`'s cards.
    pub(super) fn shield_heading_rect<H: LibraryLike>(&self, index: usize, cx: &Cx<'_, H>, at: At) -> Option<Rect> {
        let heading = self.heading_widget(index, cx)?;
        let row = &self.shelves[index];
        let focused = cx.focus.current.is_some_and(|key| key.entry == self.entry && Some(key.elem) == row.heading_elem());
        let top = match at {
            At::Drawn => self.shield_shelf_top(&self.run, index, self.scroll.pos),
            At::SpringTarget => self.shield_shelf_top(&self.target_run, index, self.scroll_target),
        };
        Some(heading.face_rect(geo::CONTENT_X, geo::heading_text_y(top), f32::from(focused), &heading.measure(cx.measure)))
    }

    // ---- the Categories tiles ----------------------------------------------------------------

    /// Register the tiles: one per genre of the listing, in its order, keyed by the genre's id so
    /// focus follows a tile when the list lands again.
    pub(super) fn sync_categories<H: LibraryLike>(&mut self, cx: &Cx<'_, H>) {
        self.categories.clear();
        let (Some(section), true) = (self.section.clone(), self.shield && self.view == View::Categories) else { return };
        for (index, genre) in H::listing(cx).genres().iter().enumerate() {
            let elem = self.keys.register(LibraryIdentity::Control { section: section.clone(), kind: "category".into(),
                key: genre.id.clone() }, self.category_group, index);
            self.categories.push(elem);
        }
    }

    /// Tile `index`, drawn now or where the page is taking it: popped when focused, and its
    /// settled popped rect.
    pub(super) fn category_placed<H: LibraryLike>(&self, index: usize, cx: &Cx<'_, H>, at: At) -> Placed {
        let scroll = match at { At::Drawn => self.scroll.pos, At::SpringTarget => self.scroll_target };
        let rest = sb::tile_rect(index, sb::TILE_TOP - scroll);
        let focused = cx.focus.current.is_some_and(|key| key.entry == self.entry && self.categories.get(index) == Some(&key.elem));
        Placed {
            rect: rest.scaled(if focused { sb::TILE_FOCUS_SCALE } else { 1.0 }),
            rest_rect: rest.scaled(sb::TILE_FOCUS_SCALE),
            clip: browse_clip(),
            index: Some(index as u32),
        }
    }

    /// Tile `index`'s rect (see [`Self::category_placed`]).
    pub(super) fn category_rect<H: LibraryLike>(&self, index: usize, cx: &Cx<'_, H>, at: At) -> Rect {
        self.category_placed(index, cx, at).rect
    }

    /// The tile grid's neighbour: four across; UP off the first row and DOWN off the last are the
    /// document's links, LEFT off the first column the side panel's door.
    pub(super) fn category_neighbour(&self, key: FocusKey<u32>, dir: Dir) -> Step<u32> {
        let Some(i) = self.categories.iter().position(|elem| *elem == key.elem) else { return Step::Edge };
        let (n, cols) = (self.categories.len(), sb::TILE_COLS);
        let next = match dir {
            Dir::Left => (i % cols > 0).then(|| i - 1),
            Dir::Right => (i % cols + 1 < cols && i + 1 < n).then_some(i + 1),
            Dir::Up => i.checked_sub(cols),
            Dir::Down => ((i / cols + 1) * cols < n).then(|| (i + cols).min(n - 1)),
        };
        next.map_or(Step::Edge, |j| Step::Move(self.key(self.categories[j])))
    }

    /// OK on a tile: Browse, filtered by its genre, with focus on the chips above the reloading grid.
    pub(super) fn activate_category<H: LibraryLike>(&mut self, elem: u32, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> bool {
        let Some(index) = self.categories.iter().position(|e| *e == elem) else { return false };
        let listing = H::listing(cx);
        let (Some(genre), Some(target)) = (listing.genres().get(index), GridTarget::from_view(listing)) else { return true };
        let id = genre.id.clone();
        let address = SectionAddress { epoch: target.epoch, sid: target.sid, section: target.section };
        self.choose_view(View::All, cx, fx);
        self.step_event(&ScreenEvent::App(AppMsg::LibraryEdit { target: address, edit: QueryEdit::Genre(Some(id)) }), cx, fx);
        self.reseat(FocusTarget::ContainerGroup(self.toolbar), fx);
        true
    }

    // ---- the tick ----------------------------------------------------------------------------

    /// The Shield half of a live tick: the header follows the focused shelf card (and warms the
    /// next one), the neutral ground settles, and the Categories tab asks for its genres.
    pub(super) fn shield_tick<H: LibraryLike>(&mut self, cx: &Cx<'_, H>, t: Tick, fx: &mut Effects<'_, H>) {
        let hubs = H::section_hubs(cx);
        let at = self.shelf_position(cx.focus.current);
        let focused = at.and_then(|(row, col)| hubs.shelves().get(row)?.items.get(col));
        if self.header.step(focused, t) {
            fx.invalidate(Provenance::Input);
        }
        self.header.warm(at.and_then(|(row, col)| hubs.shelves().get(row)?.items.get(col + 1)));
        self.neutral.update(geo::ground_target(None), t.dt());
        if self.view == View::Categories {
            if let Some(target) = self.address(cx) {
                self.store(target, LibraryWork::Genres, fx);
            }
        }
    }

    // ---- the draw ----------------------------------------------------------------------------

    /// **The page, Shield layout**: the ground (the header's over Recommended, neutral otherwise),
    /// the tabs, the view's content, then what the Standard page draws over itself.
    pub(super) fn draw_shield<H: LibraryLike>(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        plx_ui::profile::phase("lb.clear", || {
            plx_gfx::gfx::frame_clear(plx_ui::theme::CLEAR_RGB.0, plx_ui::theme::CLEAR_RGB.1, plx_ui::theme::CLEAR_RGB.2);
        });
        let ground = f.painter.alpha(f.page_alpha);
        let p = f.painter.alpha(f.page_alpha * self.page_fade.alpha());
        plx_ui::profile::phase("lb.ground", || match self.view {
            View::Recommended => {
                self.header.draw_ground(ground);
                if !self.shelves.is_empty() {
                    self.header.draw_text(p, f.measure);
                }
            }
            _ => self.neutral.draw(ground),
        });
        plx_ui::profile::phase("lb.document", || {
            self.draw_shield_tabs(f, p);
            match self.view {
                View::Recommended => plx_ui::profile::phase("lb.shelves", || self.draw_shield_rows(f, p)),
                View::All => {
                    self.draw_shield_chips(f, p);
                    let layout = self.pair.layout();
                    let alpha = self.page_fade.alpha() * self.grid_fade.alpha();
                    plx_ui::profile::phase("lb.grid", || draw_faded(&mut self.pair.detail, f, layout.detail, alpha));
                    plx_ui::profile::phase("lb.rail", || {
                        let index = f.focus.current.filter(|key| key.entry == self.entry)
                            .and_then(|key| self.pair.detail.index_of(key.elem));
                        let parent = f.page_alpha;
                        f.page_alpha = parent * alpha;
                        self.pair.master.draw_with_current(f, layout.master, index);
                        f.page_alpha = parent;
                    });
                }
                View::Categories => self.draw_categories(f, p),
            }
            self.draw_readout(f);
            if !self.plaintext_alert.visible() {
                self.record_shield_stops(f);
            }
        });
        self.hold_hint.draw(f.painter.alpha(f.page_alpha), f.measure);
        self.plaintext_alert.draw(f, self.entry);
    }

    fn draw_shield_tabs<H: LibraryLike>(&self, f: &DrawFrame<'_, '_, H>, p: Painter) {
        if !self.layout.views {
            return;
        }
        let labels = Self::tab_labels().map(|l| CString::new(l).unwrap_or_default());
        let refs: Vec<&std::ffi::CStr> = labels.iter().map(|l| l.as_c_str()).collect();
        let pills = self.tab_pills(f.cx, At::Drawn);
        let focused = f.focus.current.filter(|key| key.entry == self.entry)
            .and_then(|key| View::SHIELD_ELEMS.iter().position(|e| *e == key.elem));
        sb::draw_tabs(p, &refs, &pills, Some(self.view.index()), focused);
    }

    fn draw_shield_chips<H: LibraryLike>(&self, f: &DrawFrame<'_, '_, H>, p: Painter) {
        if !self.layout.grid_head {
            return;
        }
        let chips = self.shield_chips(f.cx);
        let rects = self.chip_rects::<H>(&chips, At::Drawn);
        let focused = f.focus.current.filter(|key| key.entry == self.entry).map(|key| key.elem);
        for (chip, r) in chips.iter().zip(rects) {
            let on = chip.elem.is_some() && chip.elem == focused;
            match chip.lay {
                sb::ChipLay::Chip { .. } => sb::draw_chip(p, r, &chip.text, chip.glyph, on),
                sb::ChipLay::Word { .. } => sb::draw_word(p, r, &chip.text),
                sb::ChipLay::Count { .. } => sb::draw_count(p, r, &chip.text),
                sb::ChipLay::Round => sb::draw_round(p, r, chip.glyph.unwrap_or(Icon::MoreVertical), on),
            }
        }
    }

    /// The rows, cut under the header: each row's heading and resting cards at its fade, then the
    /// focused card last so its ring stands over its neighbours.
    fn draw_shield_rows<H: LibraryLike>(&self, f: &mut DrawFrame<'_, '_, H>, p: Painter) {
        let _clip = f.clip(p, shield_clip());
        let fade = |top: f32| geo::row_alpha((top - geo::ROW_CARD_Y) / geo::ROW_PITCH);
        let hubs = H::section_hubs(f.cx);
        let mut focused_row = None;
        for (index, row) in self.shelves.iter().enumerate() {
            let Some(shelf) = hubs.shelves().get(index) else { continue };
            let top = self.shield_shelf_top(&self.run, index, self.scroll.pos);
            let a = fade(top);
            if a <= 0.01 || top - geo::HEADING_DY >= SCR_H || top + geo::CARD_H <= geo::ROWS_CLIP_TOP {
                continue;
            }
            let pr = p.alpha(a);
            if let Some(heading) = self.heading_widget(index, f.cx) {
                let on = f.focus.current.is_some_and(|key| key.entry == self.entry && Some(key.elem) == row.heading_elem());
                heading.draw(pr, geo::CONTENT_X, geo::heading_text_y(top), f32::from(on), &heading.measure(f.measure), f.measure);
            } else {
                geo::draw_row_heading(pr, &shelf.title, "", top, f.measure);
            }
            let src = self.hub_src(index, f.cx);
            row.cards.paint_resting(f, pr, &src, self.shield_shelf_frame(index));
            if f.focus.current.is_some_and(|key| key.entry == self.entry && row.elems.contains(&key.elem)) {
                focused_row = Some((index, a));
            }
        }
        if let Some((index, a)) = focused_row {
            let src = self.hub_src(index, f.cx);
            self.shelves[index].cards.paint_focused(f, p.alpha(a), &src, self.shield_shelf_frame(index));
        }
    }

    fn draw_categories<H: LibraryLike>(&self, f: &mut DrawFrame<'_, '_, H>, p: Painter) {
        let genres = H::listing(f.cx).genres();
        let focused = f.focus.current.filter(|key| key.entry == self.entry)
            .and_then(|key| self.categories.iter().position(|elem| *elem == key.elem));
        let mut last = None;
        for (index, genre) in genres.iter().enumerate().take(self.categories.len()) {
            let r = self.category_rect(index, f.cx, At::Drawn);
            if r.y + r.h < 0.0 || r.y > SCR_H {
                continue;
            }
            if focused == Some(index) {
                last = Some((index, r));
                continue;
            }
            sb::draw_tile(p, r, 1.0, &genre.title, sb::tile_tint(index), false, f.measure);
        }
        if let Some((index, r)) = last {
            sb::draw_tile(p, r, sb::TILE_FOCUS_SCALE, &genres[index].title, sb::tile_tint(index), true, f.measure);
        }
    }

    /// The Shield page's stops: the tabs, the chips, the rows wholly drawn below the cut (the
    /// focused one always) and their linked headings, the tiles, the read-out's action. The grid
    /// and the rail register their own as they draw.
    pub(super) fn record_shield_stops<H: LibraryLike>(&self, f: &mut DrawFrame<'_, '_, H>) {
        if !f.records_stops() {
            return;
        }
        if self.layout.views {
            for &elem in self.view_elems() {
                self.stop(elem, f);
            }
        }
        match self.view {
            View::Recommended => {
                let focus = f.focus.current;
                for (index, row) in self.shelves.iter().enumerate() {
                    let top = self.shield_shelf_top(&self.run, index, self.scroll.pos);
                    let whole = geo::row_alpha((top - geo::ROW_CARD_Y) / geo::ROW_PITCH) >= 1.0
                        && top - geo::HEADING_DY < SCR_H && top + geo::CARD_H > geo::ROWS_CLIP_TOP;
                    let holds = focus.is_some_and(|key| key.entry == self.entry && row.elems.contains(&key.elem));
                    if !whole && !holds {
                        continue;
                    }
                    let cx = f.cx;
                    row.cards.record_stops(f, f.painter, &self.hub_src(index, cx), self.shield_shelf_frame(index));
                    if let (Some(elem), Some(heading), Some(rect)) = (row.heading_elem(),
                        self.heading_widget(index, f.cx), self.shield_heading_rect(index, f.cx, At::Drawn)) {
                        if rect.y >= geo::ROWS_CLIP_TOP && rect.y + rect.h <= SCR_H {
                            heading.stop(f, rect, self.key(elem));
                        }
                    }
                }
            }
            View::All => {
                if self.layout.grid_head {
                    for &elem in self.toolbar_elems() {
                        self.stop(elem, f);
                    }
                }
            }
            View::Categories => {
                for (index, &elem) in self.categories.iter().enumerate() {
                    let rect = self.category_rect(index, f.cx, At::Drawn);
                    if rect.y + rect.h > 0.0 && rect.y < SCR_H {
                        f.stop(f.painter, Stop { key: self.key(elem), rect, rest_rect: rect, clip: browse_clip(),
                            hover: Hover::Focus, activate: Activate::Press });
                    }
                }
            }
        }
        if self.readout == Readout::Failed {
            self.stop(RETRY, f);
        }
    }
}

/// The style a hub shelf wears in a layout: Shield's rows are all posters (an episode shelf shows
/// its shows' posters, as the measured TV Recommended rows do).
pub(super) fn shelf_style(shield: bool, landscape: bool) -> &'static RowStyle {
    if shield { &RowStyle::SHIELD_POSTER } else if landscape { &RowStyle::EPISODE } else { &RowStyle::HOME }
}

fn draw_faded<H: LibraryLike>(part: &mut impl Part<H>, f: &mut DrawFrame<'_, '_, H>, rect: Rect, alpha: f32) {
    let parent = f.page_alpha;
    f.page_alpha = parent * alpha;
    part.draw(f, rect);
    f.page_alpha = parent;
}
