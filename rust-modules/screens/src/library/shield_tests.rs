//! The Library in the Shield layout (`shield.rs`): the document's measured geometry (tabs, chips,
//! the captioned grid, the rail, the tiles, the Recommended slot), the focus paths (UP to the tabs,
//! LEFT off the left-most content an edge for the side panel, a tab switching the view, a tile
//! opening Browse on its genre), the live switch both ways, and the Standard page left as it was.

use super::*;
use super::tests::{Fixture, HostFixture};
use super::views::View;
use plx_ui::focus::{FocusEngine, Outcome};
use plx_machine::machine::{InputOwner, Tick};
use plx_ui::shield as geo;
use plx_ui::shield_browse as sb;

const ENTRY: EntryId = EntryId(81);
const OWNER: InputOwner = InputOwner::Entry(ENTRY);

/// One event through the screen (with `focus` the engine's), discarding its effects.
fn step(page: &mut LibraryScreen, fixture: &Fixture, focus: Option<FocusKey<u32>>, ev: ScreenEvent<HostFixture>) {
    let mut out = Vec::new();
    let mut present = plx_machine::present::Present::new();
    page.step(&ev, &fixture.cx(focus), &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
}

fn tick(page: &mut LibraryScreen, fixture: &Fixture, focus: Option<FocusKey<u32>>) {
    step(page, fixture, focus, ScreenEvent::Tick(Tick { ms: 0, dt_us: 16_667 }));
}

/// A directional press through the engine: the page hears the move; `true` when focus moved.
fn press(page: &mut LibraryScreen, engine: &mut FocusEngine<u32>, fixture: &Fixture, dir: Dir) -> bool {
    let mut links = Vec::new();
    <LibraryScreen as Screen<HostFixture>>::links(page, &mut links);
    match engine.move_dir(OWNER, page, &links, dir, &fixture.cx(engine.current(OWNER))) {
        Outcome::Moved { from, to, by } => {
            step(page, fixture, Some(to), ScreenEvent::FocusMoved { from, to, by });
            true
        }
        Outcome::Edge(_) | Outcome::Nothing => false,
    }
}

/// Put focus on `elem` the way a restore does.
fn seat(page: &mut LibraryScreen, engine: &mut FocusEngine<u32>, fixture: &Fixture, elem: u32) {
    let key = page.key(elem);
    let group = <LibraryScreen as Focusable<HostFixture>>::group_of(page, &elem, &fixture.cx(Some(key)));
    engine.set(OWNER, key, group, By::Restore);
    step(page, fixture, Some(key), ScreenEvent::FocusMoved { from: None, to: key, by: By::Restore });
}

fn place(page: &LibraryScreen, fixture: &Fixture, focus: Option<FocusKey<u32>>, elem: u32, at: At) -> Rect {
    <LibraryScreen as Focusable<HostFixture>>::place(page, &elem, &fixture.cx(focus), at).expect("placed").rect
}

fn near(a: Rect, b: Rect) -> bool {
    [(a.x, b.x), (a.y, b.y), (a.w, b.w), (a.h, b.h)].iter().all(|(p, q)| (p - q).abs() < 0.5)
}

fn shelved() -> Fixture {
    Fixture::shelves(&["movie.inprogress.1", "movie.recentlyadded.1", "movie.recentlyreleased.1"], 12)
}

/// §6.1, §6.3, §5.1: the Shield document — the grid's first row at 204 and the next at 664 on six
/// 282.6 columns from 127; a later row held at 390; the chip bar at 132; the Recommended slot one
/// 472 pitch a shelf; the tiles at 140 on a 312 pitch — and the Standard document beside it as it was.
#[test]
fn the_shield_document_is_the_measured_one() {
    let lay = Layout::new(false, &[], 40, true).with_views(true).with_shield(true);
    assert_eq!((lay.row_y(0, 0.0), lay.row_y(1, 0.0)), (204.0, 664.0));
    assert_eq!((lay.card_w(), lay.card_h(), lay.grid_pitch()), (240.0, 360.0, 460.0));
    assert_eq!(lay.cell_x(0), 127.0);
    assert!((lay.cell_x(5) - 1540.0).abs() < 0.01 && (lay.grid_right() - 1780.0).abs() < 0.01);
    assert_eq!(lay.content_top() + lay.grid_block_top(), sb::CHIP_Y, "the chips at 132");
    assert_eq!(lay.row_reveal(0), 0.0, "the first row shows the tabs and the chips");
    for row in [1, 7, 20] {
        assert_eq!(lay.row_y(row, lay.row_reveal(row)), sb::GRID_FOCUS_ROW_Y, "row {row} held at 390");
    }
    let last = lay.row_reveal(39);
    assert!(lay.row_y(39, last) + lay.grid_pitch() <= SCR_H + 0.01, "the last row's caption on the screen");
    assert_eq!(last, lay.max_scroll());
    assert_eq!(lay.with_grid_focus(Some(3)).row_y(5, 0.0), lay.row_y(5, 0.0), "no Shield row opens a band");

    let pitches = [geo::ROW_PITCH; 5];
    let run = ShelfRun::of(&pitches);
    let rec = Layout::new(false, &pitches, 0, false).with_views(true).with_shield(true);
    assert_eq!(rec.shelf_reveal(&run, 3), 3.0 * geo::ROW_PITCH, "shelf 3 rests in the slot at three pitches");
    assert_eq!(rec.max_scroll(), 4.0 * geo::ROW_PITCH, "the last shelf comes to the slot");
    assert_eq!(rec.with_views(false).first(), Some(Block::Shelf(0)));

    let cats = Layout::new(false, &[], 0, false).with_views(true).with_shield(true).with_categories(18);
    assert_eq!(cats.content_top() + cats.categories_top(), sb::TILE_TOP);
    assert_eq!((cats.category_reveal(0), cats.category_reveal(1), cats.category_reveal(2)), (0.0, 0.0, 312.0));
    assert_eq!(cats.category_reveal(4), cats.max_scroll(), "the last row clamps");
    assert_eq!(cats.with_views(false).first(), Some(Block::Categories));

    let standard = Layout::new(true, &pitches, 40, true).with_views(true);
    assert_eq!(standard, standard.with_shield(false), "the Standard document is the default");
    assert_eq!(standard.content_top(), CONTENT_TOP);
    assert_eq!(standard.cell_x(0), MARGIN_X);
    assert_eq!(standard.left(), MARGIN_X);
}

/// The head: three tabs in the track at y 32 instead of the library row and the switcher, the
/// shelves in the ring style, the tabs' group the view switcher's own.
#[test]
fn the_page_wears_tabs_and_ring_rows_instead_of_the_library_row() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let fixture = shelved();
        let page = fixture.screen();
        assert!(page.shield && page.libraries.is_empty(), "the side panel chooses the library: no pill row");
        assert!(page.layout.views);
        assert_eq!(page.view_elems(), &View::SHIELD_ELEMS);
        let first = place(&page, &fixture, None, VIEW_RECOMMENDED, At::Drawn);
        assert_eq!((first.x, first.y, first.h), (135.0, 40.0, 48.0));
        let next = place(&page, &fixture, None, VIEW_ALL, At::Drawn);
        assert_eq!(next.x, first.x + first.w + sb::TAB_GAP);
        assert!(!page.shelves.is_empty());
        for row in &page.shelves {
            assert_eq!(row.cards.style(), &RowStyle::SHIELD_POSTER);
        }
        let mut groups = Vec::new();
        <LibraryScreen as Focusable<HostFixture>>::groups(&page, &fixture.cx(None), &mut groups);
        assert!(groups.iter().any(|g| g.id == VIEWS_GROUP && g.len == 3));
        assert!(!groups.iter().any(|g| g.id == page.pair.groups_config().detail), "Recommended carries no grid");
        assert!(!groups.iter().any(|g| g.id == LIBRARY_GROUP));
    });
}

/// Recommended's slot: shelf `i`'s cards at 568 when it holds focus, the focused card in the
/// first slot at x 127 popped inside its ring, the rows sliding one 472 pitch a step.
#[test]
fn recommended_holds_the_focused_row_and_card_in_the_slot() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let fixture = shelved();
        let mut page = fixture.screen();
        page.initial = false;
        let mut engine = FocusEngine::new();
        let first = page.shelves[0].elems[0];
        seat(&mut page, &mut engine, &fixture, first);
        assert!(press(&mut page, &mut engine, &fixture, Dir::Down));
        let at = engine.current(OWNER).unwrap();
        assert_eq!(page.shelf_position(Some(at)), Some((1, 0)));
        assert_eq!(page.scroll_target, geo::ROW_PITCH);
        assert!(press(&mut page, &mut engine, &fixture, Dir::Right));
        let at = engine.current(OWNER).unwrap();
        assert_eq!(page.shelf_position(Some(at)), Some((1, 1)));
        let settled = place(&page, &fixture, Some(at), at.elem, At::SpringTarget);
        let slot = Rect::new(geo::CONTENT_X, geo::ROW_CARD_Y, geo::CARD_W, geo::CARD_H).scaled(geo::FOCUS_SCALE);
        assert!(near(settled, slot), "the focused card settles in the slot: {settled:?}");
    });
}

/// The doors: UP from the first row reaches the tabs, DOWN comes back; LEFT off a row's first card
/// and off the first tab is an edge (the side panel's door); OK on Browse switches the view, whose
/// chips stand between the tabs and the grid; LEFT off the grid's first column is an edge too.
#[test]
fn up_reaches_the_tabs_left_is_the_side_panel_door_and_a_tab_switches_the_view() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let fixture = shelved();
        let mut page = fixture.screen();
        page.initial = false;
        let mut engine = FocusEngine::new();
        let card = page.shelves[0].elems[0];
        seat(&mut page, &mut engine, &fixture, card);
        assert!(!press(&mut page, &mut engine, &fixture, Dir::Left), "LEFT at a row's first card is the edge");
        assert!(press(&mut page, &mut engine, &fixture, Dir::Up));
        assert_eq!(engine.current(OWNER).map(|k| k.elem), Some(VIEW_RECOMMENDED), "UP lands on the tab shown");
        assert!(!press(&mut page, &mut engine, &fixture, Dir::Left), "LEFT at the first tab is the edge");
        assert!(press(&mut page, &mut engine, &fixture, Dir::Down));
        assert_eq!(engine.current(OWNER).map(|k| k.elem), Some(card), "DOWN returns to the row");
        assert!(press(&mut page, &mut engine, &fixture, Dir::Up));
        assert!(press(&mut page, &mut engine, &fixture, Dir::Right));
        assert_eq!(engine.current(OWNER).map(|k| k.elem), Some(VIEW_ALL));
        assert_eq!(page.view, View::Recommended, "moving along the tabs only moves focus");
        let at = engine.current(OWNER);
        step(&mut page, &fixture, at, ScreenEvent::Activate(VIEW_ALL));
        assert_eq!(page.view, View::All, "OK on Browse shows the grid");
        assert!(page.shelves.is_empty() && page.shows_grid());
        assert_eq!(page.toolbar_elems(), &[FILTER, TYPE, SORT, MORE]);
        assert!(press(&mut page, &mut engine, &fixture, Dir::Down));
        assert!(page.toolbar_elems().contains(&engine.current(OWNER).unwrap().elem), "DOWN from the tabs: the chips");
        while press(&mut page, &mut engine, &fixture, Dir::Left) {}
        assert_eq!(engine.current(OWNER).map(|k| k.elem), Some(FILTER), "LEFT runs out at the first chip: the edge");
        assert!(press(&mut page, &mut engine, &fixture, Dir::Down));
        let at = engine.current(OWNER);
        assert_eq!(page.grid_position(at), Some((0, 0)), "DOWN from the chips: the grid's first card");
        assert!(!press(&mut page, &mut engine, &fixture, Dir::Left), "LEFT at the grid's first column is the edge");
        assert!(press(&mut page, &mut engine, &fixture, Dir::Up));
        assert!(press(&mut page, &mut engine, &fixture, Dir::Up));
        assert!(page.view_elems().contains(&engine.current(OWNER).unwrap().elem), "and on UP to the tabs");
    });
}

/// §6.1: the chip bar from x 127 at y 132 — filter, type, "by", sort with its direction, the count
/// box, the round chip — and the grid under it: 240x360 cards at 204 and 664 on the 282.6 columns.
#[test]
fn browse_lays_out_the_chip_bar_and_the_captioned_grid() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let fixture = shelved();
        let mut page = fixture.screen();
        page.view = View::All;
        page.sync(&fixture.cx(None));
        let chips = page.shield_chips(&fixture.cx(None));
        let kinds: Vec<_> = chips.iter().map(|c| std::mem::discriminant(&c.lay)).collect();
        let want = [sb::ChipLay::Chip { w: 0.0, glyph: false }, sb::ChipLay::Chip { w: 0.0, glyph: false },
            sb::ChipLay::Word { w: 0.0 }, sb::ChipLay::Chip { w: 0.0, glyph: true }, sb::ChipLay::Count { w: 0.0 },
            sb::ChipLay::Round].map(|l| std::mem::discriminant(&l));
        assert_eq!(kinds, want);
        assert_eq!(chips[3].glyph, Some(plx_ui::icons::Icon::ChevronUp), "an ascending sort");
        let filter = place(&page, &fixture, None, FILTER, At::Drawn);
        assert_eq!((filter.x, filter.y, filter.h), (127.0, 132.0, 48.0));
        let more = place(&page, &fixture, None, MORE, At::Drawn);
        assert_eq!((more.w, more.h), (48.0, 48.0));
        let first = place(&page, &fixture, None, page.pair.detail.elem_at(0).unwrap(), At::SpringTarget);
        assert!(near(first, Rect::new(127.0, 204.0, 240.0, 360.0)), "{first:?}");
        let seventh = place(&page, &fixture, None, page.pair.detail.elem_at(7).unwrap(), At::SpringTarget);
        assert!(near(seventh, Rect::new(127.0 + sb::GRID_PITCH_X, 664.0, 240.0, 360.0)), "{seventh:?}");
    });
}

/// §6.2: with focus in the grid the rail stands on its 44 column at x 1833, one 32 pitch a letter,
/// centred on the page.
#[test]
fn the_rail_stands_on_the_measured_column() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let fixture = Fixture::new();
        let mut page = fixture.screen();
        page.view = View::All;
        page.sync(&fixture.cx(None));
        let focus = Some(page.key(page.pair.detail.elem_at(0).unwrap()));
        let letters = page.pair.master.elems.clone();
        assert_eq!(letters.len(), 2);
        let a = place(&page, &fixture, focus, letters[0], At::Drawn);
        let z = place(&page, &fixture, focus, letters[1], At::Drawn);
        assert_eq!((a.x, a.w, a.h), (sb::RAIL_X, sb::RAIL_W, sb::RAIL_PITCH));
        assert_eq!(z.y - a.y, sb::RAIL_PITCH);
        assert_eq!((a.y + z.y + z.h) * 0.5, sb::RAIL_CY, "a short alphabet centres on the page");
    });
}

fn with_genres(fixture: &mut Fixture, n: usize) {
    let genres = (0..n).map(|i| plx_data::browse::GenreEntry { id: format!("g{i}"), title: format!("Genre {i}") }).collect();
    fixture.listing = fixture.listing.clone().with_genres(genres);
}

/// §6.3: the Categories tab is the listing's genres as 376x212 tiles at x 127, 551, 975, 1399 and
/// y 140, 452; DOWN goes a row, LEFT off the first column is the edge; OK on a tile shows Browse
/// with that genre's filter queued and focus on the chips above the reloading grid.
#[test]
fn categories_tiles_sit_on_the_measured_pitch_and_open_browse_on_their_genre() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut fixture = shelved();
        with_genres(&mut fixture, 10);
        let mut page = fixture.screen();
        page.initial = false;
        page.view = View::Categories;
        page.sync(&fixture.cx(None));
        assert_eq!(page.categories.len(), 10);
        assert!(page.shelves.is_empty() && !page.shows_grid());
        let tile = |page: &LibraryScreen, i: usize| place(page, &fixture, None, page.categories[i], At::Drawn);
        assert!(near(tile(&page, 0), Rect::new(127.0, 140.0, 376.0, 212.0)));
        assert!(near(tile(&page, 5), Rect::new(551.0, 452.0, 376.0, 212.0)));
        assert_eq!(tile(&page, 3).x, 1399.0);
        let mut engine = FocusEngine::new();
        let second = page.categories[1];
        seat(&mut page, &mut engine, &fixture, second);
        assert!(press(&mut page, &mut engine, &fixture, Dir::Down));
        assert_eq!(engine.current(OWNER).map(|k| k.elem), Some(page.categories[5]));
        assert!(press(&mut page, &mut engine, &fixture, Dir::Left));
        assert!(!press(&mut page, &mut engine, &fixture, Dir::Left), "LEFT at the first column is the edge");
        assert!(press(&mut page, &mut engine, &fixture, Dir::Up));
        assert!(press(&mut page, &mut engine, &fixture, Dir::Up));
        assert!(page.view_elems().contains(&engine.current(OWNER).unwrap().elem), "UP from the first row: the tabs");
        let pick = page.categories[2];
        let at = Some(page.key(pick));
        step(&mut page, &fixture, at, ScreenEvent::Activate(pick));
        assert_eq!(page.view, View::All);
        assert!(matches!(page.pending.grid(), Some((_, GridAction::Genre { id: Some(id) })) if id == "g2"),
            "the genre's filter is queued for the grid");
    });
}

/// **Live switch, both ways.** Standard → Shield re-dresses the shelves in the ring style with the
/// focused card in its row's slot, drops the library row and puts the scroll where the slot holds
/// the focused row; Shield → Standard from Categories comes back on Recommended with the
/// Standard shelves and library row.
#[test]
fn switching_layout_live_redresses_the_page_in_place() {
    let _guard = plx_base::testlock::serial();
    let fixture = shelved();
    let mut page = fixture.screen();
    page.initial = false;
    assert!(!page.shield);
    let standard_libraries = page.libraries.len();
    let key = page.key(page.shelves[1].elems[3]);
    tick(&mut page, &fixture, Some(key));
    plx_ui::layout::with_shield(|| {
        tick(&mut page, &fixture, Some(key));
        assert!(page.shield);
        assert!(page.libraries.is_empty());
        assert_eq!(page.shelves[1].cards.style(), &RowStyle::SHIELD_POSTER);
        assert_eq!(page.shelves[1].cards.scroll(), 3.0 * geo::CARD_PITCH, "the focused card comes to the slot");
        assert_eq!((page.scroll.pos, page.scroll_target), (geo::ROW_PITCH, geo::ROW_PITCH));
        page.view = View::Categories;
        page.sync(&fixture.cx(None));
    });
    let at = Some(page.key(VIEW_CATEGORIES));
    tick(&mut page, &fixture, at);
    assert!(!page.shield);
    assert_eq!(page.view, View::Recommended, "Standard has no Categories view");
    assert!(!page.shelves.is_empty());
    assert_eq!(page.shelves[0].cards.style(), &RowStyle::HOME);
    assert_eq!(page.libraries.len(), standard_libraries, "the library row is back");
}

/// The Standard page is the page it was: the two-pill switcher, the TYPE/SORT/FILTER toolbar, Home
/// shelves, the grid from the 96 margin under the top bar, and every grid card's caption band
/// collapsed but the focused row's.
#[test]
fn the_standard_page_is_unchanged() {
    let _guard = plx_base::testlock::serial();
    let fixture = shelved();
    let page = fixture.screen();
    assert!(!page.shield);
    assert_eq!(page.view_elems(), &View::ELEMS);
    assert_eq!(page.toolbar_elems(), &[TYPE, SORT, FILTER]);
    assert!(page.shelves.iter().all(|row| row.cards.style() == &RowStyle::HOME || row.cards.style() == &RowStyle::EPISODE));
    assert!(page.shows_grid() && page.shows_shelves());
    assert_eq!(page.layout.content_top(), CONTENT_TOP);
    assert_eq!(page.layout.cell_x(0), MARGIN_X);
    assert_eq!(page.layout.style().treatment, plx_ui::cards::Treatment::Glow);
    assert!(!page.layout.is_shield());
}

/// The page seats its own focus on Recommended's first row (Home's seat), not on the tabs.
#[test]
fn the_first_seat_is_the_first_rows_card() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let fixture = shelved();
        let mut page = fixture.screen();
        let mut out = Vec::new();
        let mut present = plx_machine::present::Present::new();
        page.step(&ScreenEvent::Tick(Tick { ms: 0, dt_us: 16_667 }), &fixture.cx(None),
            &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
        let seated: Vec<_> = out.iter().filter_map(|e| match &e.fx {
            Fx::Deliver(_, plx_machine::machine::Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus }))) => Some(*focus),
            _ => None,
        }).collect();
        eprintln!("seated {seated:?} first {:?} head {:?} shelf0 {:?} views {:?}", page.layout.first(), page.head_group(), page.shelves.first().map(|s| s.group), VIEWS_GROUP);
        let row = page.shelves[0].entry_group();
        assert!(matches!(seated.as_slice(), [FocusTarget::ContainerGroup(g)] if *g == row), "{seated:?}");
    });
}
