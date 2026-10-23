//! One result row's cards, for `plx_ui::cards::Shelf`: the row's items read over the page's
//! element interning and the roster of sources the results came from.
use super::*;
use crate::registry::tile_facts;
use plx_data::search::scope::ScopeSource;
use plx_machine::machine::Measure;
use plx_ui::cards::TileLabel;
use plx_ui::cards::{CardSource, Tile};
use plx_ui::widgets::Art;
use plx_ui::Painter;

/// Row `kind`'s cards. `items[i]` is shown as `elems[i]`; the two are built together by
/// [`SearchScreen::sync`], and a view that has since moved on shows as fewer cards, never as a
/// card with no item.
pub struct RowCards<'a> {
    pub(super) kind: Kind,
    pub(super) elems: &'a [u32],
    pub(super) items: &'a [Item],
    pub(super) sources: &'a [ScopeSource],
    /// The Shield layout's grid: every result a poster (an episode its show's, as that layout's
    /// rows show one), with no still overlay.
    pub(super) shield: bool,
}

impl RowCards<'_> {
    /// The server handle credited under a card of a source that is not the household's own.
    fn handle(&self, item: &Item) -> &str {
        self.sources.iter().find(|source| source.sid == item.sid() && !source.household)
            .map_or("", |source| source.handle.as_str())
    }
}

impl<H: SearchLike> CardSource<H> for RowCards<'_> {
    fn len(&self) -> usize { self.elems.len().min(self.items.len()) }
    fn elem(&self, i: usize) -> u32 { self.elems[i] }
    fn index_of(&self, e: &u32) -> Option<usize> {
        self.elems.iter().position(|elem| elem == e).filter(|&i| i < self.items.len())
    }
    fn art(&self, i: usize) -> Art<'_> {
        match self.items.get(i) {
            Some(Item::Media(media)) if self.shield => Art::Poster(Some(tile_facts::of(media))),
            Some(item) => render::tile_art(self.kind, item),
            None => Art::Poster(None),
        }
    }
    fn label(&self, i: usize) -> TileLabel {
        match self.items.get(i) {
            // The Shield grid shows an episode as its show's poster, so its caption names the show
            // and the episode, as the Standard still's overlay does.
            Some(Item::Media(media)) if self.shield && self.kind == Kind::Episode => {
                let facts = tile_facts::of(media);
                let mut label = TileLabel::title(if facts.show_title.is_empty() { facts.title } else { facts.show_title });
                label.caption = plx_ui::cards::focused_caption(&facts, false);
                label
            }
            Some(item) => TileLabel::titled(item.title(), &render::subtitle(self.kind, item, self.handle(item))),
            None => TileLabel::default(),
        }
    }
    fn progress(&self, i: usize) -> Option<f32> {
        match self.items.get(i) {
            Some(Item::Media(media)) if self.kind != Kind::Episode => media.resume_frac(),
            _ => None,
        }
    }
    fn overlay(&self, p: Painter, i: usize, tile: &Tile, measure: &dyn Measure) {
        if let (Kind::Episode, Some(Item::Media(media)), false) = (self.kind, self.items.get(i), self.shield) {
            plx_ui::widgets::still_overlay(p, &tile_facts::of(media), tile.rect, tile.radius, false, measure);
        }
    }
}
