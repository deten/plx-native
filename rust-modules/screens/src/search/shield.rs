//! **Search in the Shield layout** (`plx_ui::layout::shield()`; SHIELD-LAYOUT-SPEC §8.1).
//!
//! The same page, the same store transactions, draft, press paths and recents; what differs is how
//! it stands:
//!
//! - **The left column** holds still: the query field at the top (`shield_search::FIELD`) and the
//!   on-screen keyboard under it, a–z and 1–0 with space, backspace and clear. A key types into the
//!   page's own draft through the same edit the system keyboard's text takes ([`SearchScreen::edit`]),
//!   so the store, the debounce and the acknowledgement rules are Standard's. OK on the field still
//!   raises the television's own keyboard: both type into the one query. The page opens with focus
//!   on the first key. The app this layout follows has voice and search-settings buttons beside the
//!   field; PlxNative has neither, so neither is drawn.
//! - **The right column** is one scrolling document (the page's `Stack`, laid out by
//!   [`SearchScreen::shield_sections`]): the hint line, then — before anything is typed — the
//!   recent searches and their Clear chip, or once results land, each kind's group under its heading
//!   as a captioned four-column grid (`RowStyle::SHIELD_SEARCH_GRID`; people as circles). An
//!   episode shows its show's poster, as the layout's rows do. The page scrolls the column itself
//!   ([`SearchScreen::shield_reveal`]): a row is brought up only as far as it must be to show whole.
//! - **Focus.** The keys walk their own board (`shield_search::key_neighbour`); UP off the first
//!   row is the field, RIGHT off the last column the results (the group last visited, else the
//!   first), LEFT off any result's first column the keyboard again, at the key it was left on.
//!   LEFT off the keyboard's first column or off the field is the side panel's door
//!   (`Rig::edge_surface`): both answer `Edge` to LEFT.
//! - **Live.** The layout is latched once per event ([`SearchScreen::sync_layout`]): a switch while
//!   the page is up rebuilds its document for the new layout, keeps the query and the results, and
//!   moves a focus the new layout cannot hold (a key, in Standard) to the field.

use std::borrow::Cow;
use std::ffi::CString;

use plx_ui::cards::GridSpec;
use plx_ui::screen::{Activate, Hover, Stop};
use plx_ui::shield_search as ss;
use plx_ui::Painter;

use super::*;

/// The on-screen keyboard's keys: element `KEY_BASE + i` is key `i` (`shield_search::key_face`).
/// Under `geom::NAMESPACE_FLOOR`, and far above any result the page interns (`next_elem` counts up
/// from 10, one per result identity).
pub(super) const KEY_BASE: u32 = 0x0F00_0000;
pub(super) const KEYS_GROUP: GroupId = GroupId(0x5345_4103);
/// The hint line's block: never focusable, but a group id of its own, apart from the field's.
const HINT_GROUP: GroupId = GroupId(0x5345_4104);

/// Result group `kind`'s heading block (never focusable).
pub(super) fn heading_group(kind: Kind) -> GroupId {
    GroupId(0x5345_4300 + layout::ordinal(kind))
}

/// The key element `elem` is, if it is one.
pub(super) fn key_index(elem: u32) -> Option<usize> {
    elem.checked_sub(KEY_BASE).map(|i| i as usize).filter(|&i| i < ss::KEY_COUNT)
}

pub(super) fn key_elem(i: usize) -> u32 {
    KEY_BASE + i as u32
}

/// The results column's cut: the screen right of the keyboard.
pub(super) fn results_clip() -> Rect {
    Rect::new(ss::RESULTS_CLIP_X, 0.0, SCR_W - ss::RESULTS_CLIP_X, SCR_H)
}

/// What the hint line says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hint {
    /// Nothing searched (or the search still on its way): what can be searched.
    Prompt,
    /// One character typed: a search starts at two.
    OneMore,
    /// Results are up: where they came from (the Standard field's scope line).
    Scope,
    NoResults,
    Failed,
}

/// The Shield field's measured run and the keys' labels, refreshed in `prepare`.
#[derive(Default)]
pub(super) struct Res {
    query: String,
    caret: usize,
    run: CString,
    run_w: f32,
    head_w: f32,
    keys: Vec<CString>,
}

impl Res {
    /// Re-measure the field when the query or its caret moved (and label the keys once).
    pub(super) fn prepare(&mut self, query: &str, caret: usize, measure: &dyn plx_machine::machine::Measure) {
        if self.keys.is_empty() {
            self.keys = (0..ss::KEY_COUNT).map(|i| match ss::key_face(i) {
                Some(ss::KeyFace::Char(c)) => CString::new(c.to_string()).unwrap_or_default(),
                _ => CString::default(),
            }).collect();
        }
        if self.query == query && self.caret == caret && !self.run.is_empty() {
            return;
        }
        self.query = query.into();
        self.caret = caret;
        let blank = query.trim().is_empty();
        self.run = if blank {
            plx_platform::i18n::msg::browse_search_field_c().into()
        } else {
            CString::new(query).unwrap_or_default()
        };
        self.run_w = measure.width(&self.run, ss::FIELD_SIZE, false);
        self.head_w = if blank {
            0.0
        } else {
            let head = &query[..caret.min(query.len())];
            measure.width(&CString::new(head).unwrap_or_default(), ss::FIELD_SIZE, false)
        };
    }
}

impl SearchScreen {
    /// **Latch the live layout for this event.** A change while the page is up rebuilds its
    /// document for the new layout (the shelves become grids and back) and re-derives the result
    /// rows from the store, so each result keeps its element and a focused card stays focused; a
    /// key has no Standard counterpart and gives its focus to the field. Answers whether the layout
    /// changed (the caller then brings the focused result into view). Nothing animates across it.
    pub(super) fn sync_layout<H: SearchLike>(&mut self, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> bool {
        let shield = plx_ui::layout::shield();
        if shield == self.shield {
            return false;
        }
        self.shield = shield;
        self.stack = new_stack(self.entry, shield);
        self.rows_gen = self.rows_gen.wrapping_add(1);
        self.content_dirty = true;
        self.neutral.reset();
        if !shield && cx.focus.current.is_some_and(|key| key.entry == self.entry && key_index(key.elem).is_some()) {
            self.reseat(FocusTarget::Elem(self.key(FIELD)), fx);
        }
        fx.invalidate(Provenance::Nav);
        true
    }

    /// **A key was pressed**: its edit, at the end of the query, through the page's one edit path.
    pub(super) fn type_key<H: SearchLike>(&mut self, i: usize, fx: &mut Effects<'_, H>) {
        let edit = match ss::key_face(i) {
            Some(ss::KeyFace::Char(c)) => TextEdit::Commit(c.to_string().into()),
            Some(ss::KeyFace::Space) => TextEdit::Commit(" ".into()),
            Some(ss::KeyFace::Backspace) => TextEdit::Backspace,
            Some(ss::KeyFace::Clear) => TextEdit::Clear,
            None => return,
        };
        self.draft.to_end();
        self.edit(&edit, fx);
    }

    /// The first key: where the page opens.
    pub(super) fn first_key(&self) -> FocusKey<u32> {
        self.key(key_elem(0))
    }

    /// The key the keyboard was last left on, else the first.
    fn home_key<H: SearchLike>(&self, cx: &Cx<'_, H>) -> FocusKey<u32> {
        cx.focus.remembered(KEYS_GROUP).filter(|elem| key_index(*elem).is_some())
            .map_or(self.first_key(), |elem| self.key(elem))
    }

    // ---- the document --------------------------------------------------------------------------

    /// The right column's sections: the hint line's block, then the recents and their Clear chip
    /// (nothing typed), or each result kind's heading and grid.
    pub(super) fn shield_sections(&self, out: &mut Vec<SectionSpec<Sec>>) {
        use plx_ui::cards::Kind as Block;
        const STOP: [EdgeRule; 4] = [EdgeRule::Stop; 4];
        out.push(SectionSpec::new(Sec::Head, Block::Custom { height: ss::HEAD_H, focusable: false }, HINT_GROUP));
        if !self.recents.is_empty() {
            let shown = self.recents.len().min(layout::RECENT_CAP);
            out.push(SectionSpec::new(Sec::Recents, Block::Custom { height: ss::recents_h(shown), focusable: true },
                RECENTS_GROUP).edges(STOP));
            out.push(SectionSpec::new(Sec::Clear, Block::Custom { height: ss::CLEAR_BLOCK, focusable: true },
                CLEAR_GROUP).edges(STOP));
        }
        for row in &self.rows {
            out.push(SectionSpec::new(Sec::Heading(row.kind, self.rows_gen),
                Block::Custom { height: ss::HEADING_BLOCK, focusable: false }, heading_group(row.kind)));
            let elem = if matches!(row.kind, Kind::Movie | Kind::Show | Kind::Episode) { ElemKind::Card } else { ElemKind::Bare };
            // the page sets the column's scroll itself (`shield_reveal`), so the edge only names the
            // focus line; the rows above the focused one stay drawn while any of them shows
            let spec = GridSpec::new(0.0, ss::FOCUS_ROW_Y)
                .columns(ss::RESULTS_COLS, *layout::shield_style(row.kind), ss::RESULTS_X)
                .keeping_above();
            out.push(SectionSpec::new(Sec::Row(row.kind, self.rows_gen), Block::Grid { spec }, row.group)
                .seated(Seat::Remembered).edges(STOP).of_kind(elem));
        }
    }

    /// A recents or Clear element's rect in its section `section` (screen space).
    pub(super) fn shield_element_rect<H: SearchLike>(&self, cx: &Cx<'_, H>, k: Sec, n: usize, section: Rect) -> Rect {
        match k {
            Sec::Recents => ss::recent_rect(n, section.y),
            Sec::Clear => {
                let w = cx.measure.width(plx_platform::i18n::msg::browse_search_clear_c(), plx_ui::shield_browse::CHIP_SIZE, false);
                ss::clear_rect(w, section.y)
            }
            _ => ss::FIELD,
        }
    }

    /// **Bring the focused result into view** (the page drives the column's scroll itself): the
    /// least move that shows its card and caption whole without lifting it over the focus line,
    /// from `headed`, where the column was going before this event. The field and the keys move
    /// nothing: the column stays where it was headed (the stack's own rule would send it home
    /// for an element outside its document), so RIGHT back into it finds it as it was left.
    pub(super) fn shield_reveal<H: SearchLike>(&mut self, cx: &Cx<'_, H>, headed: f32) {
        let Some(key) = cx.focus.current.filter(|key| key.entry == self.entry) else { return };
        if key.elem == FIELD || key_index(key.elem).is_some() {
            self.stack.scroll_to(headed);
            return;
        }
        let Some(placed) = self.view::<H>().place(&key.elem, cx, At::SpringTarget) else { return };
        // a placement is on screen at the live scroll; a card's rest rect is its focus pop
        let card = self.rows.iter().find(|row| row.elems.contains(&key.elem)).map(|row| layout::shield_style(row.kind).h);
        let (top, h) = match card {
            Some(h) => (placed.rest_rect.cy() - h * 0.5, h),
            None => (placed.rest_rect.y, placed.rest_rect.h),
        };
        let top = top + self.stack.scroll();
        let max = self.stack.max_scroll(&*self, cx);
        self.stack.scroll_to(ss::reveal(headed, top, h, max));
    }

    /// **The card a move into result group `k` lands on**, from `from`: the one whose row is
    /// nearest on screen, then whose column is — so UP from the group below lands on this group's
    /// last row, and RIGHT off the keyboard on the row level with the key. (A grid's own seat is
    /// its first row: right for one grid alone, not for groups stacked one under another.)
    pub(super) fn shield_seat<H: SearchLike>(&self, cx: &Cx<'_, H>, k: Sec, from: Placed) -> Option<u32> {
        let Sec::Row(kind, _) = k else { return None };
        let row = self.rows.iter().find(|row| row.kind == kind)?;
        let view = self.view::<H>();
        let (fx, fy) = (from.rect.cx(), from.rect.cy());
        row.elems.iter().filter_map(|elem| view.place(elem, cx, At::SpringTarget).map(|p| (*elem, p.rest_rect)))
            .min_by(|(_, a), (_, b)| {
                let key = |r: &Rect| ((r.cy() - fy).abs(), (r.cx() - fx).abs());
                let (ka, kb) = (key(a), key(b));
                ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
            })
            .map(|(elem, _)| elem)
    }

    /// What the hint line says now.
    pub(super) fn hint<H: SearchLike>(&self, cx: &Cx<'_, H>) -> Hint {
        if !self.real_query() {
            let n = self.draft.query().trim().chars().count();
            return if n > 0 && n < plx_data::search::MIN_QUERY { Hint::OneMore } else { Hint::Prompt };
        }
        if !self.rows.is_empty() {
            return Hint::Scope;
        }
        if self.draft.pending() {
            return Hint::Prompt;
        }
        match H::search(cx).state() {
            plx_data::search::State::Ready => Hint::NoResults,
            plx_data::search::State::Failed => Hint::Failed,
            _ => Hint::Prompt,
        }
    }

    fn hint_text<H: SearchLike>(&self, cx: &Cx<'_, H>) -> Cow<'_, str> {
        use plx_platform::i18n::msg;
        match self.hint(cx) {
            Hint::Prompt => Cow::Borrowed(msg::browse_search_hint()),
            Hint::OneMore => Cow::Borrowed(msg::browse_search_one_more()),
            Hint::Scope => self.render.scope_line().map_or(Cow::Borrowed(""), |line| line.to_string_lossy()),
            Hint::NoResults => Cow::Owned(msg::browse_search_no_results(self.draft.query().trim())),
            Hint::Failed => Cow::Borrowed(msg::browse_search_failed()),
        }
    }

    // ---- focus -----------------------------------------------------------------------------------

    /// The field's and the keyboard's groups, ahead of the document's.
    pub(super) fn keyboard_groups(&self, out: &mut Vec<GroupSpec>) {
        let stop = [EdgeRule::Stop; 4];
        out.push(GroupSpec { id: FIELD_GROUP, kind: GroupKind::Row { wrap: false }, seat: Seat::First,
            reachable: AxisMask::BOTH, edge: stop, extent: ss::field_plate(), len: 1, elem: ElemKind::Bare });
        out.push(GroupSpec { id: KEYS_GROUP, kind: GroupKind::Free, seat: Seat::Remembered,
            reachable: AxisMask::BOTH, edge: stop, extent: ss::KEYBOARD, len: ss::KEY_COUNT, elem: ElemKind::Bare });
    }

    /// The group of the field or a key, when `elem` is one.
    pub(super) fn keyboard_group_of(&self, elem: u32) -> Option<GroupId> {
        if elem == FIELD { Some(FIELD_GROUP) } else { key_index(elem).map(|_| KEYS_GROUP) }
    }

    /// A move from the field or a key: the board's own walk; its edges are the links' and the side
    /// panel's.
    pub(super) fn keyboard_neighbour(&self, key: FocusKey<u32>, dir: Dir) -> Option<Step<u32>> {
        if key.elem == FIELD {
            return Some(Step::Edge);
        }
        let i = key_index(key.elem)?;
        Some(ss::key_neighbour(i, dir).map_or(Step::Edge, |j| Step::Move(self.key(key_elem(j)))))
    }

    /// Where the field or a key stands (it never moves).
    pub(super) fn keyboard_place(&self, elem: u32) -> Option<Placed> {
        let (rect, index) = if elem == FIELD {
            (ss::field_plate(), 0)
        } else {
            let i = key_index(elem)?;
            (ss::key_rect(i), i as u32)
        };
        Some(Placed { rect, rest_rect: rect, clip: Rect::FULL, index: Some(index) })
    }

    /// The element a move into the field or the keyboard lands on: the field; the key nearest
    /// where the move came from (the board's remembered key comes first, by its seat).
    pub(super) fn keyboard_seat(&self, g: GroupId, from: Placed) -> Option<FocusKey<u32>> {
        match g {
            FIELD_GROUP => Some(self.key(FIELD)),
            KEYS_GROUP => Some(self.key(key_elem(ss::nearest_key(from.rect.cx(), from.rect.cy())))),
            _ => None,
        }
    }

    /// The field and the keys are always there; a result that went away falls back by the
    /// document's rule, except that the field it would end on gives way to the keyboard.
    pub(super) fn keyboard_reconcile<H: SearchLike>(&self, want: FocusKey<u32>, cx: &Cx<'_, H>) -> FocusKey<u32> {
        if want.elem == FIELD || key_index(want.elem).is_some() {
            return want;
        }
        let got = self.view::<H>().reconcile(want, cx);
        if got.elem == FIELD { self.home_key(cx) } else { got }
    }

    /// The Shield page's links: the field over the keyboard, the keyboard and the field beside the
    /// results' first group, every result group back to the keyboard on LEFT, and the document's
    /// own vertical order.
    pub(super) fn shield_links(&self, out: &mut Vec<Link>) {
        out.push(Link { from: FIELD_GROUP, dir: Dir::Down, to: KEYS_GROUP });
        out.push(Link { from: KEYS_GROUP, dir: Dir::Up, to: FIELD_GROUP });
        if let Some(first) = self.first_content() {
            out.push(Link { from: KEYS_GROUP, dir: Dir::Right, to: first });
            out.push(Link { from: FIELD_GROUP, dir: Dir::Right, to: first });
        }
        if !self.recents.is_empty() {
            out.push(Link { from: RECENTS_GROUP, dir: Dir::Down, to: CLEAR_GROUP });
            out.push(Link { from: CLEAR_GROUP, dir: Dir::Up, to: RECENTS_GROUP });
            out.push(Link { from: RECENTS_GROUP, dir: Dir::Left, to: KEYS_GROUP });
            out.push(Link { from: CLEAR_GROUP, dir: Dir::Left, to: KEYS_GROUP });
        }
        for row in &self.rows {
            out.push(Link { from: row.group, dir: Dir::Left, to: KEYS_GROUP });
        }
        for pair in self.rows.windows(2) {
            out.push(Link { from: pair[0].group, dir: Dir::Down, to: pair[1].group });
            out.push(Link { from: pair[1].group, dir: Dir::Up, to: pair[0].group });
        }
    }

    // ---- the draw --------------------------------------------------------------------------------

    /// A plain section of the document, Shield layout: the hint line, the recents, the Clear chip,
    /// a group's heading.
    pub(super) fn shield_custom_draw<H: SearchLike>(&self, k: Sec, f: &mut DrawFrame<'_, '_, H>, r: Rect) {
        let p = f.painter.alpha(f.page_alpha);
        let focus = f.focus.current.filter(|key| key.entry == self.entry).map(|key| key.elem);
        match k {
            Sec::Head => ss::draw_hint(p, &self.hint_text(f.cx), r.y + ss::HINT.y, f.measure),
            Sec::Heading(kind, _) => ss::draw_heading(p, kind.title(), r.y, f.measure),
            Sec::Recents => {
                ss::draw_heading(p, plx_platform::i18n::msg::browse_search_recent_heading(), r.y, f.measure);
                for (n, elem) in self.recents.iter().take(layout::RECENT_CAP).enumerate() {
                    let Some(Identity::Recent(term)) = self.keys.iter().find(|key| key.elem == *elem).map(|key| &key.identity)
                    else { continue };
                    let rect = ss::recent_rect(n, r.y);
                    let fit = f.measure.fit_line(term, rect.w - 2.0 * ss::RECENT_PAD, ss::RECENT_SIZE, false);
                    ss::draw_recent(p, rect, &fit, focus == Some(*elem));
                }
            }
            Sec::Clear => {
                let rect = self.shield_element_rect(f.cx, k, 0, r);
                plx_ui::shield_browse::draw_chip(p, rect, plx_platform::i18n::msg::browse_search_clear_c(), None,
                    focus == Some(CLEAR));
            }
            Sec::Row(..) => {}
        }
    }
}

/// **The page, Shield layout**: the neutral ground, the results column (the document, faded in
/// with each landing), then the field and the keyboard, which never move.
pub(super) fn draw<H: SearchLike>(screen: &SearchScreen, f: &mut DrawFrame<'_, '_, H>) {
    use plx_ui::theme;
    plx_gfx::gfx::frame_clear(theme::CLEAR_RGB.0, theme::CLEAR_RGB.1, theme::CLEAR_RGB.2);
    let p = f.painter.alpha(f.page_alpha);
    screen.neutral.draw(p);
    let parent = f.page_alpha;
    f.page_alpha = parent * screen.fade.alpha();
    screen.stack.view(screen).paint(f);
    f.page_alpha = parent;
    field(screen, f, p);
    keyboard(screen, f, p);
}

fn field<H: SearchLike>(screen: &SearchScreen, f: &mut DrawFrame<'_, '_, H>, p: Painter) {
    let res = &screen.render.shield;
    let focused = f.focus.current == Some(screen.key(FIELD));
    let blank = res.query.trim().is_empty();
    let (dx, caret) = ss::field_layout(res.run_w, res.head_w, screen.editing);
    let caret = (screen.editing && screen.blink_us < BLINK_US).then_some(caret);
    if focused {
        ss::draw_field_plate(p);
    }
    {
        let _clip = f.clip(p, ss::FIELD);
        ss::draw_field(p, &res.run, dx, blank, focused, caret);
    }
    stop(screen, f, p, FIELD, ss::field_plate());
}

fn keyboard<H: SearchLike>(screen: &SearchScreen, f: &mut DrawFrame<'_, '_, H>, p: Painter) {
    let focus = f.focus.current.filter(|key| key.entry == screen.entry).map(|key| key.elem);
    for (i, label) in screen.render.shield.keys.iter().enumerate() {
        let Some(face) = ss::key_face(i) else { continue };
        let rect = ss::key_rect(i);
        ss::draw_key(p, rect, face, label, focus == Some(key_elem(i)));
        stop(screen, f, p, key_elem(i), rect);
    }
}

/// The stop of the field or a key: a bare element, taken at the press (`render::stop`'s rule).
fn stop<H: SearchLike>(screen: &SearchScreen, f: &mut DrawFrame<'_, '_, H>, p: Painter, elem: u32, rect: Rect) {
    if !f.records_stops() {
        return;
    }
    f.stop(p, Stop { key: screen.key(elem), rect, rest_rect: rect, clip: Rect::FULL, hover: Hover::Focus,
        activate: Activate::Immediate });
}
