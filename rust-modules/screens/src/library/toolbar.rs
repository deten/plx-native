//! One value-chip model supplies both focus geometry and paint runs.
//!
//! The Shield layout's chip bar (`shield.rs`, `ShieldChip`) reads the same values ([`type_value`],
//! [`sort_value`], [`filter_value`]: the listing's, or a queued edit's while it is in flight) and
//! adds the round chip that opens the Libraries menu (`MORE`, the library pills' "+N").
//!
//! [`type_value`]: LibraryScreen::type_value
//! [`sort_value`]: LibraryScreen::sort_value
//! [`filter_value`]: LibraryScreen::filter_value
use std::ffi::{CStr, CString};
use super::*;
use plx_ui::value_chip::ValueChip;

pub(super) struct Chip {
    pub name: &'static CStr,
    pub value: CString,
    pub note: Option<CString>,
}
impl Chip {
    pub fn width(&self, measure: &dyn plx_machine::machine::Measure) -> f32 {
        ValueChip::width(measure, self.name, &self.value, self.note.as_deref())
    }
}

impl LibraryScreen {
    /// Every library lists more than one type ([`plx_data::browse::LibraryType::offered`]), so TYPE
    /// always leads; FILTER goes while the listed type takes no filters (collections).
    pub(super) fn toolbar_elems(&self) -> &'static [u32] {
        if self.shield {
            // the measured order: the filter, the type, "by" the sort, the count, the round chip
            return if self.listed.filters() { &[FILTER, TYPE, SORT, MORE] } else { &[TYPE, SORT, MORE] };
        }
        if self.listed.filters() { &[TYPE, SORT, FILTER] } else { &[TYPE, SORT] }
    }

    pub(super) fn view_section<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Option<usize> {
        let directory = H::directory(cx);
        self.pending.section().filter(|target| Some(target.epoch) == directory.epoch())
            .map(|target| target.index).or_else(|| match self.wanted_kind {
                Some(kind) => directory.preferred(kind), None => directory.current(),
            })
            .filter(|index| *index < directory.sections().len())
    }

    pub(super) fn library_label<H: LibraryLike>(&self, section: usize, cx: &Cx<'_, H>) -> CString {
        let directory = H::directory(cx);
        let value = if let Some(section) = directory.sections().get(section) {
            let owner = section.sid.and_then(|sid| directory.sources().iter().find(|(id, _)| *id == sid))
                .map(|(_, source)| source.handle.as_str()).unwrap_or("");
            if owner.is_empty() { section.row.title.clone() } else { format!("{} {owner}", section.row.title) }
        } else {
            let total = self.view_section(cx).map(|section| directory.favorite_sections_for(section).count()).unwrap_or(0);
            format!("+{}", total.saturating_sub(self.libraries.len().saturating_sub(1)))
        };
        CString::new(value).unwrap_or_default()
    }

    pub(super) fn library_lays<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Vec<plx_ui::widgets::StripLay> {
        plx_ui::widgets::strip_layout_measured(
            self.libraries.iter().map(|(_, section)| self.library_label(*section, cx).to_string_lossy().into_owned()),
            MARGIN_X + plx_ui::widgets::STRIP_PAD, plx_ui::theme::size::BODY,
            plx_ui::widgets::STRIP_GAP_WIDE, cx.measure)
    }

    /// The grid edit queued for the listing on screen, while its answer is in flight.
    fn queued_action<'a, H: LibraryLike>(&'a self, cx: &Cx<'_, H>) -> Option<&'a GridAction> {
        let listing = H::listing(cx);
        self.pending.grid().filter(|(target, _)| target.matches(listing)).map(|(_, action)| action)
    }

    /// What the listing lists (the TYPE chip's value).
    pub(super) fn type_value<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> String {
        let kind = match self.queued_action(cx) {
            Some(GridAction::LibraryType(kind)) => *kind,
            _ => H::listing(cx).library_type(),
        };
        kind.title(self.kind).to_owned()
    }

    /// The listing's sort (the SORT chip's value) and whether it runs descending.
    pub(super) fn sort_value<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> (String, bool) {
        let listing = H::listing(cx);
        let (sort, desc) = match self.queued_action(cx) {
            Some(GridAction::Sort { key, desc }) => (listing.sorts().iter().find(|sort| &sort.key == key), *desc),
            _ => (listing.sorts().get(listing.sort_index()), listing.sort_desc()),
        };
        (sort.map_or(plx_platform::i18n::msg::browse_library_title(), |sort| sort.title.as_str()).to_owned(), desc)
    }

    /// The listing's filters, in a word (the FILTER chip's value).
    pub(super) fn filter_value<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> String {
        let listing = H::listing(cx);
        let queued = self.queued_action(cx);
        let genre = match queued {
            Some(GridAction::Genre { id }) => id.as_ref().and_then(|id| listing.genres().iter().find(|genre| &genre.id == id)),
            _ => listing.genre(),
        }.map(|genre| genre.title.as_str());
        let unwatched = match queued {
            Some(GridAction::Unwatched { desired }) => *desired,
            _ => listing.unwatched(),
        };
        match (genre, unwatched) {
            (None, false) => plx_platform::i18n::msg::browse_library_all().into(),
            (None, true) => plx_platform::i18n::msg::browse_library_unwatched().into(),
            (Some(genre), false) => genre.into(),
            (Some(genre), true) => plx_platform::i18n::msg::browse_library_genre_unwatched(genre),
        }
    }

    pub(super) fn toolbar_chip<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>) -> Chip {
        let (name, value) = if elem == TYPE {
            (plx_platform::i18n::msg::browse_library_type_c(), self.type_value(cx))
        } else if elem == SORT {
            (plx_platform::i18n::msg::browse_library_sort_c(), self.sort_value(cx).0)
        } else {
            (plx_platform::i18n::msg::browse_library_filter_c(), self.filter_value(cx))
        };
        Chip { name, value: CString::new(format!(" · {value}")).unwrap_or_default(), note: None }
    }

    pub(super) fn toolbar_chip_rect<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>, at: At) -> Rect {
        if self.shield {
            return self.shield_chip_rect(elem, cx, at);
        }
        let x = MARGIN_X + self.toolbar_elems().iter().take_while(|&&key| key != elem)
            .map(|&key| self.toolbar_chip(key, cx).width(cx.measure) + 16.0).sum::<f32>();
        let (layout, scroll) = match at {
            At::Drawn => (&self.layout, self.scroll.pos),
            At::SpringTarget => (&self.target_layout, self.scroll_target),
        };
        Rect::new(x, CONTENT_TOP + layout.grid_block_top() + plx_ui::consts::TITLE_DY + CARD_DY - scroll,
            self.toolbar_chip(elem, cx).width(cx.measure), 52.0)
    }
}
