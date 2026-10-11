//! **The view switcher**: a smaller second strip, *Recommended | All*, hanging directly under the
//! selected library pill, or under the focused one while focus walks the library row (at the
//! margin for a section with a single library, which draws no pill row). It switches the view of
//! the library it stands under:
//!
//! - **Recommended** is the page as it always was: the server's Recommended shelves, then the
//!   toolbar and the full grid under them.
//! - **All** hides the shelves, so the toolbar and the grid stand right under the pill rows.
//!
//! The page decides the view when a library OPENS (`LibraryScreen::sync`, when the section's
//! identity changes) from Settings' *Library opens on* (`plx_data::browse::library_opens`) and,
//! for its *Last* choice, the library's remembered view (`ListingView::remembers_all`). A press in
//! the switcher applies at once to the library on screen and is always recorded
//! (`LibraryWork::Page`), whatever the setting says, so choosing *Last* later starts from the
//! viewer's real choices. It is a strip like the library row above it (`widgets::draw_strip_sized`
//! at [`VIEW_SZ`] in a [`VIEW_PILL_H`] row), drawn through the same `TabStrip` capsules.
//!
//! **A library not on screen yet.** A library pill switches the library on OK only, so DOWN from a
//! pill that is highlighted but not selected takes focus into the switcher FOR that library
//! (`LibraryScreen::views_for`): the switcher stays under its pill, offers the view that library
//! would open on ([`LibraryScreen::switcher_view`]), and the page below still shows the library on
//! screen. OK on a view there opens that library straight into the view chosen ([`Opening`]) and
//! records the choice. DOWN there goes nowhere: the content below is the library on screen's, and
//! only OK chooses. UP returns to its pill, and leaving the switcher any other way forgets it.
use super::*;
use super::layout::{VIEW_PILL_H, VIEW_SZ};

/// A library opening from the switcher: the view it opens on in place of the setting's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Opening {
    pub(super) section: LibrarySectionIdentity,
    pub(super) view: View,
}

/// The two views a library page can show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum View {
    #[default]
    Recommended,
    All,
}

impl View {
    /// The switcher's elements, in the order its pills stand.
    pub(super) const ELEMS: [u32; 2] = [VIEW_RECOMMENDED, VIEW_ALL];

    pub(super) fn of(all: bool) -> Self { if all { View::All } else { View::Recommended } }

    pub(super) fn is_all(self) -> bool { self == View::All }

    /// This view's pill in the switcher.
    pub(super) fn elem(self) -> u32 { Self::ELEMS[self.index()] }

    pub(super) fn index(self) -> usize { match self { View::Recommended => 0, View::All => 1 } }

    pub(super) fn from_elem(elem: u32) -> Option<Self> {
        Self::ELEMS.iter().position(|e| *e == elem).map(|i| View::of(i == 1))
    }
}

/// The switcher's labels, in pill order.
fn view_labels() -> [&'static str; 2] {
    [plx_platform::i18n::msg::browse_library_view_recommended(), plx_platform::i18n::msg::browse_library_view_all()]
}

impl LibraryScreen {
    /// The switcher's pills, laid out under the library pill they belong to ([`Self::view_pill`]):
    /// their left edge is that pill's left edge (the margin when there is no pill row), pulled left
    /// only as far as keeps the whole switcher inside the grid's band.
    pub(super) fn view_lays<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Vec<plx_ui::widgets::StripLay> {
        let lay = |left: f32| plx_ui::widgets::strip_layout_measured(
            view_labels().into_iter().map(str::to_owned), left + plx_ui::widgets::STRIP_PAD, VIEW_SZ,
            plx_ui::widgets::STRIP_GAP, cx.measure);
        let at_margin = lay(MARGIN_X);
        let width = at_margin.last().map_or(0.0, |last| {
            let pill = plx_ui::widgets::strip_pill_rect(last, 0.0, VIEW_PILL_H);
            pill.x + pill.w - MARGIN_X
        });
        let under = self.view_pill(cx).map_or(MARGIN_X, |index| self.library_rect(index, cx).x);
        let left = under.min(layout::GRID_RIGHT - width).max(MARGIN_X);
        if left == MARGIN_X { at_margin } else { lay(left) }
    }

    /// The library pill the switcher hangs under: the FOCUSED one while focus walks the library
    /// row, so the switcher moves with the focus mark the viewer is following; the pill DOWN came
    /// from while focus is in the switcher for a library not selected yet ([`Self::views_for`]);
    /// and otherwise the selected one (the library on screen, or on its way in). A library pill
    /// only switches the library on OK, so without the first two rules the switcher stayed under
    /// the library on screen while focus walked away from it. Focus on the "+N" pill keeps it
    /// under the selection: that pill opens the library menu and has no views of its own.
    fn view_pill<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Option<usize> {
        let position = |section: usize| self.libraries.iter().position(|(_, index)| *index == section);
        let focused = cx.focus.current.filter(|key| key.entry == self.entry)
            .and_then(|key| self.libraries.iter().position(|(elem, _)| *elem == key.elem && *elem != MORE));
        focused.or_else(|| self.views_for.filter(|_| self.in_switcher(cx)).and_then(position))
            .or_else(|| self.view_section(cx).and_then(position))
    }

    /// Focus is on one of the switcher's pills.
    fn in_switcher<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> bool {
        self.layout.views && cx.focus.current.is_some_and(|key| key.entry == self.entry && View::ELEMS.contains(&key.elem))
    }

    /// The library (section index) the switcher stands under and acts on.
    pub(super) fn switcher_section<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Option<usize> {
        self.view_pill(cx).and_then(|index| self.libraries.get(index)).map(|(_, section)| *section)
            .filter(|section| *section != usize::MAX)
            .or_else(|| self.view_section(cx))
    }

    /// The view the switcher marks as its library's: the view shown for the library on screen; for
    /// one on its way in from the switcher, the view chosen for it; for any other, the view it
    /// would open on now (the setting, and for *Last* its remembered choice).
    pub(super) fn switcher_view<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> View {
        let directory = H::directory(cx);
        let Some(section) = self.switcher_section(cx).filter(|section| Some(*section) != directory.current())
        else { return self.view };
        let incoming = self.pending.section().filter(|target| target.index == section);
        self.opening.as_ref().filter(|opening| incoming.is_some_and(|target| target.identity == opening.section))
            .map_or_else(|| View::of(plx_data::browse::library_opens().opens_all(directory.remembers_all(section))),
                |opening| opening.view)
    }

    /// Focus is in the switcher for a library that is not the selected one: DOWN from its pill
    /// put it there.
    pub(super) fn switcher_for_other<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> bool {
        self.views_for.filter(|_| self.in_switcher(cx)).is_some_and(|section| Some(section) != self.view_section(cx))
    }

    /// Focus moved: DOWN from a library pill that is not the selected one makes the switcher that
    /// library's; a move inside the switcher keeps it; any other move forgets it.
    pub(super) fn track_views_for<H: LibraryLike>(&mut self, from: Option<FocusKey<u32>>, to: FocusKey<u32>, cx: &Cx<'_, H>) {
        let mine = |key: FocusKey<u32>| key.entry == self.entry;
        if !(mine(to) && self.layout.views && View::ELEMS.contains(&to.elem)) {
            self.views_for = None;
            return;
        }
        let Some(from) = from.filter(|key| mine(*key)) else { self.views_for = None; return };
        if View::ELEMS.contains(&from.elem) { return; }
        self.views_for = self.libraries.iter().find(|(elem, _)| *elem == from.elem && *elem != MORE)
            .map(|(_, section)| *section)
            .filter(|section| Some(*section) != self.view_section(cx));
    }

    /// The switcher row's top on screen, rising with the library row above it when the first
    /// shelf's heading lifts (`library_rect`'s own rule).
    pub(super) fn view_y(&self) -> f32 {
        CONTENT_TOP + self.layout.view_row_top() - self.scroll.pos
            - self.shelves.first().map_or(0.0, |row| row.cards.heading_lift())
    }

    /// Where the switcher pill `elem` stands, while the switcher is shown.
    pub(super) fn view_rect<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>) -> Option<Rect> {
        let index = View::ELEMS.iter().position(|e| *e == elem).filter(|_| self.layout.views)?;
        self.view_lays(cx).get(index).map(|lay|
            plx_ui::widgets::strip_pill_rect(lay, self.view_y(), VIEW_PILL_H))
    }

    /// Choose `view` for the library the switcher stands under, and remember the choice for it.
    /// For the library on screen it shows at once. For another one (DOWN came from its pill, or it
    /// is already on its way in) that library opens straight into `view` ([`Opening`]). Choosing
    /// the library on screen while another is on its way in keeps the one on screen, as OK on its
    /// pill would.
    pub(super) fn choose_view<H: LibraryLike>(&mut self, view: View, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        if !self.layout.views { return; }
        let target = self.switcher_section(cx);
        self.views_for = None;
        if let Some(index) = target.filter(|index| Some(*index) != H::directory(cx).current()) {
            let incoming = self.pending.section().is_some_and(|section| section.index == index);
            let address = if incoming { self.requested_address(cx) } else { self.request_library(index, cx, fx) };
            let Some(address) = address else { return };
            self.opening = Some(Opening {
                section: LibrarySectionIdentity { sid: address.sid, key: address.section }, view,
            });
            self.store(address, LibraryWork::Page { all: view.is_all() }, fx);
            fx.invalidate(Provenance::Input);
            return;
        }
        if self.pending.section().is_some() { self.cancel_library_switch(); }
        if view == self.view { return; }
        self.view = view;
        if let Some(target) = self.address(cx) {
            self.store(target, LibraryWork::Page { all: view.is_all() }, fx);
        }
        // The shelves are rebuilt from the published hubs by `sync` (Recommended) or stay away (All).
        self.shelves.clear();
        self.shelf_publication = None;
        self.sync(cx);
        self.scroll_target = self.scroll_target.clamp(0.0, self.target_layout.max_scroll());
        fx.invalidate(Provenance::Input);
    }

    /// Step the switcher's capsules: the selection on the view shown, the focus on its pill.
    pub(super) fn step_views<H: LibraryLike>(&mut self, focused: Option<FocusKey<u32>>, cx: &Cx<'_, H>, dt: f32) {
        let shown = self.layout.views;
        let selected = if shown { self.switcher_view(cx).index() as i32 } else { -1 };
        let focused = focused.filter(|key| key.entry == self.entry && shown)
            .and_then(|key| View::ELEMS.iter().position(|e| *e == key.elem)).map_or(-1, |i| i as i32);
        let spans: Vec<_> = View::ELEMS.iter().filter_map(|elem| self.view_rect(*elem, cx).map(|r| (r.x, r.w))).collect();
        self.view_capsules.update(selected, focused, |i| spans.get(i).copied(), plx_ui::widgets::SelMark::Lands, dt);
        self.view_pop.step((focused >= 0).then_some(0), dt);
    }
}
