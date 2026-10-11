//! **The view switcher**: a smaller second strip, *Recommended | All*, hanging directly under the
//! selected library pill, or under the focused one while focus walks the library row (at the
//! margin for a section with a single library, which draws no pill row). It switches the view of
//! the library on screen only:
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
use super::*;
use super::layout::{VIEW_PILL_H, VIEW_SZ};

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
    /// row, so the switcher moves with the focus mark the viewer is following, and otherwise the
    /// selected one (the library on screen, or on its way in). A library pill only switches the
    /// library on OK, so without the first rule the switcher stayed under the library on screen
    /// while focus walked away from it. Focus on the "+N" pill keeps it under the selection: that
    /// pill opens the library menu and has no views of its own.
    fn view_pill<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Option<usize> {
        let focused = cx.focus.current.filter(|key| key.entry == self.entry)
            .and_then(|key| self.libraries.iter().position(|(elem, _)| *elem == key.elem && *elem != MORE));
        focused.or_else(|| self.view_section(cx)
            .and_then(|section| self.libraries.iter().position(|(_, index)| *index == section)))
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

    /// Show `view` for the library on screen, at once, and remember the choice for it. A press
    /// while another library is still on its way in changes nothing: the switcher names the
    /// library under it, and that one is decided when it opens.
    pub(super) fn choose_view<H: LibraryLike>(&mut self, view: View, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        if !self.layout.views || self.pending.section().is_some() || view == self.view { return; }
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
        let selected = if shown { self.view.index() as i32 } else { -1 };
        let focused = focused.filter(|key| key.entry == self.entry && shown)
            .and_then(|key| View::ELEMS.iter().position(|e| *e == key.elem)).map_or(-1, |i| i as i32);
        let spans: Vec<_> = View::ELEMS.iter().filter_map(|elem| self.view_rect(*elem, cx).map(|r| (r.x, r.w))).collect();
        self.view_capsules.update(selected, focused, |i| spans.get(i).copied(), plx_ui::widgets::SelMark::Lands, dt);
        self.view_pop.step((focused >= 0).then_some(0), dt);
    }
}
