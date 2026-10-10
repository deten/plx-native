//! The view switcher (`views.rs`): where it stands, how focus walks through it, what each view
//! puts on the page, how a library opens under Settings' "Library opens on", what a press records,
//! and the channel-down jump from above the grid. Driven through the navigation suite's fixture
//! (`navigation_tests.rs`): the page's own groups, links, seats and the real focus engine.
use super::*;
use super::navigation_tests::{Fixture, OWNER};
use plx_data::browse::LibraryOpens;
use plx_machine::machine::Tick;
use plx_ui::focus::FocusEngine;

fn group_at(page: &LibraryScreen, engine: &FocusEngine<u32>, fixture: &Fixture) -> Option<GroupId> {
    engine.current(OWNER).and_then(|key| page.group_of(&key.elem, &fixture.cx(engine)))
}

/// The page switches between two libraries' sections as a pill press would land: the store points
/// at `section`, and the page hears of it.
fn open_library(fixture: &mut Fixture, page: &mut LibraryScreen, engine: &mut FocusEngine<u32>, section: usize) {
    fixture.publish(section, 24);
    fixture.step(page, engine, ScreenEvent::StoreChanged(StoreId::Browse.ord(), 1));
}

/// The `LibraryWork::Page` choices a batch of effects recorded, in order.
fn recorded(effects: &[AppFx]) -> Vec<bool> {
    effects.iter().filter_map(|effect| match effect {
        AppFx::Store(StoreId::Browse, StoreCmd::Browse(BrowseCmd::Addressed { work: LibraryWork::Page { all }, .. })) => Some(*all),
        _ => None,
    }).collect()
}

/// Run `body` with "Library opens on" set to `mode`, putting the default back afterwards.
fn opening_on<T>(mode: LibraryOpens, body: impl FnOnce() -> T) -> T {
    plx_data::browse::restore_library_opens(mode);
    let out = body();
    plx_data::browse::restore_library_opens(LibraryOpens::Default);
    out
}

/// **Layout A**: the switcher hangs directly under the SELECTED library pill, left edges aligned,
/// as a smaller row; it follows the selection to another pill; a single library (no pill row)
/// still gets it, at the margin where its pill would stand.
#[test]
fn the_switcher_hangs_under_the_selected_library_pill_and_follows_it() {
    let _guard = plx_base::testlock::serial();
    let mut fixture = Fixture::new(3, 24, 2);
    let mut page = fixture.screen();
    let mut engine = FocusEngine::new();
    let cx = fixture.cx(&engine);
    let pill = page.library_rect(0, &cx);
    let recommended = page.view_rect(VIEW_RECOMMENDED, &cx).expect("a selected library shows the switcher");
    let all = page.view_rect(VIEW_ALL, &cx).unwrap();
    assert_eq!(recommended.x, pill.x, "the switcher starts under the selected pill");
    assert!(recommended.y >= pill.y + pill.h, "…below it");
    assert!(recommended.h < pill.h, "…as the smaller, secondary row");
    assert!(all.x > recommended.x + recommended.w - 0.5, "Recommended, then All");
    assert!(page.layout.shelf_origin(&page.run, 0) + CONTENT_TOP > recommended.y + recommended.h,
        "the first shelf hangs under the switcher");

    open_library(&mut fixture, &mut page, &mut engine, 2);
    let cx = fixture.cx(&engine);
    assert_eq!(page.view_rect(VIEW_RECOMMENDED, &cx).unwrap().x, page.library_rect(2, &cx).x,
        "the switcher moves with the selection");

    let lone = Fixture::new(1, 24, 2);
    let page = lone.screen();
    let engine = FocusEngine::new();
    let cx = lone.cx(&engine);
    assert!(page.libraries.is_empty(), "one library draws no pill row");
    assert_eq!(page.view_rect(VIEW_RECOMMENDED, &cx).map(|rect| rect.x), Some(MARGIN_X),
        "a single library still gets its switcher, where its pill would stand");
    assert_eq!(page.layout.first(), Some(Block::ViewRow));
}

/// The switcher always fits beside the selected pill: placed under the LAST pill of a full row it
/// is pulled left rather than past the grid's band, and its app-owned labels never elide (each pill
/// is measured to its label) in any shipped language.
#[test]
fn the_switcher_fits_the_grid_band_in_every_language() {
    use plx_base::fontcov::advances::ShippedMeasure;
    use plx_machine::machine::Measure;
    use plx_platform::i18n::{language_on_this_thread_for_test, SHIPPED};
    let _guard = plx_base::testlock::serial();
    let fixture = Fixture::new(12, 24, 0);
    let mut page = fixture.screen();
    let engine = FocusEngine::new();
    let last = page.libraries.iter().rposition(|(elem, _)| *elem != MORE).unwrap();
    for language in SHIPPED {
        let _language = language_on_this_thread_for_test(language);
        let cx = Cx { measure: &ShippedMeasure, ..fixture.cx(&engine) };
        page.libraries.swap(0, last);
        for rect in [VIEW_RECOMMENDED, VIEW_ALL].map(|elem| page.view_rect(elem, &cx).unwrap()) {
            assert!(rect.x >= MARGIN_X && rect.x + rect.w <= layout::GRID_RIGHT + 0.5,
                "{}: the switcher stays inside the grid band: {rect:?}", language.tag());
        }
        for (label, lay) in [plx_platform::i18n::msg::browse_library_view_recommended(),
            plx_platform::i18n::msg::browse_library_view_all()].iter().zip(page.view_lays(&cx)) {
            let natural = ShippedMeasure.width_str(label, layout::VIEW_SZ, true);
            assert!(lay.w + 0.5 >= natural, "{} {label:?}: an app-owned switcher label never elides", language.tag());
        }
        page.libraries.swap(0, last);
    }
}

/// Focus: DOWN from the library pills lands on the switcher, on the view SHOWN; DOWN again is the
/// first shelf (Recommended); UP retraces both. LEFT/RIGHT walk the two pills and stop at the ends.
#[test]
fn down_from_the_pills_lands_on_the_switcher_then_the_first_shelf() {
    let _guard = plx_base::testlock::serial();
    let fixture = Fixture::new(2, 24, 2);
    let mut page = fixture.screen();
    page.initial = false;
    let mut engine = FocusEngine::new();
    fixture.step(&mut page, &mut engine, ScreenEvent::Enter(Enter::Fresh { focus: FocusTarget::ContainerGroup(LIBRARY_GROUP) }));
    let pill = engine.current(OWNER).unwrap();
    assert_eq!(group_at(&page, &engine, &fixture), Some(LIBRARY_GROUP));
    fixture.direction(&mut page, &mut engine, Dir::Down);
    assert_eq!(engine.current(OWNER), Some(page.key(VIEW_RECOMMENDED)), "DOWN from the pills: the switcher, on the view shown");
    fixture.direction(&mut page, &mut engine, Dir::Right);
    assert_eq!(engine.current(OWNER), Some(page.key(VIEW_ALL)));
    fixture.direction(&mut page, &mut engine, Dir::Right);
    assert_eq!(engine.current(OWNER), Some(page.key(VIEW_ALL)), "the switcher's end is an edge");
    fixture.direction(&mut page, &mut engine, Dir::Down);
    assert_eq!(group_at(&page, &engine, &fixture), Some(page.shelves[0].group), "DOWN from the switcher: the first shelf");
    assert_eq!(page.scroll_target, page.target_layout.shelf_reveal(&page.target_run, 0));
    fixture.direction(&mut page, &mut engine, Dir::Up);
    assert_eq!(engine.current(OWNER), Some(page.key(VIEW_RECOMMENDED)),
        "UP re-enters the switcher on the view shown, not on the pill last walked to");
    assert_eq!(page.scroll_target, 0.0, "the switcher is revealed with the head of the page");
    fixture.direction(&mut page, &mut engine, Dir::Up);
    assert_eq!(engine.current(OWNER), Some(pill), "UP from the switcher: the selected library's pill");
}

/// **All**: selecting it applies at once. The shelves leave the page, the toolbar and the grid
/// stand right under the switcher, DOWN from the switcher reaches the toolbar and then the grid,
/// and the sort, filter and rail controls are all still there. Recommended brings the shelves
/// straight back. Every press is recorded for the library on screen.
#[test]
fn choosing_all_hides_the_shelves_and_recommended_brings_them_back() {
    let _guard = plx_base::testlock::serial();
    let fixture = Fixture::new(2, 24, 2);
    let mut page = fixture.screen();
    page.initial = false;
    let mut engine = FocusEngine::new();
    assert_eq!(page.shelves.len(), 2);
    let with_shelves = page.layout.grid_block_top();
    engine.set(OWNER, page.key(VIEW_ALL), Some(VIEWS_GROUP), By::Dir);
    let effects = fixture.step(&mut page, &mut engine, ScreenEvent::Activate(VIEW_ALL));
    assert_eq!(page.view, View::All);
    assert_eq!(recorded(&effects), [true], "the choice is recorded for this library");
    assert!(page.shelves.is_empty(), "All shows no shelves");
    assert_eq!(page.layout.shelves, 0);
    assert_eq!(page.layout.grid_block_top(), page.layout.library_h(), "the toolbar sits right under the pill rows");
    assert!(page.layout.grid_block_top() < with_shelves);
    let mut groups = Vec::new();
    page.groups(&fixture.cx(&engine), &mut groups);
    let ids: Vec<_> = groups.iter().map(|group| group.id).collect();
    assert_eq!(&ids[..3], [LIBRARY_GROUP, VIEWS_GROUP, page.toolbar_group()], "pills, switcher, toolbar: no shelf between");
    assert!(ids.contains(&page.pair.groups_config().detail), "the grid is still there");
    assert_eq!(page.toolbar_elems(), [TYPE, SORT, FILTER], "sort and filter still work");
    assert!(!page.pair.master.elems.is_empty(), "the A-Z rail still has its letters beside the grid");

    fixture.direction(&mut page, &mut engine, Dir::Down);
    assert_eq!(group_at(&page, &engine, &fixture), Some(page.toolbar_group()), "DOWN from the switcher: the toolbar");
    assert_eq!(page.scroll_target, 0.0, "with no shelf above it the toolbar keeps the pill rows in view");
    fixture.direction(&mut page, &mut engine, Dir::Down);
    assert_eq!(page.grid_position(engine.current(OWNER)).map(|(row, _)| row), Some(0), "…then the grid");
    fixture.direction(&mut page, &mut engine, Dir::Up);
    fixture.direction(&mut page, &mut engine, Dir::Up);
    assert_eq!(engine.current(OWNER), Some(page.key(VIEW_ALL)), "UP retraces onto the view shown");

    let again = fixture.step(&mut page, &mut engine, ScreenEvent::Activate(VIEW_ALL));
    assert!(recorded(&again).is_empty(), "the view already shown is not a new choice");
    let back = fixture.step(&mut page, &mut engine, ScreenEvent::Activate(VIEW_RECOMMENDED));
    assert_eq!((page.view, recorded(&back)), (View::Recommended, vec![false]));
    assert_eq!(page.shelves.len(), 2, "Recommended shows the published shelves at once");
    assert_eq!(page.layout.grid_block_top(), with_shelves);
}

/// **Library opens on**: Default opens every library on Recommended, All on All, and Last on the
/// view last chosen for that library (Recommended when it never was).
#[test]
fn a_library_opens_on_the_view_the_setting_names() {
    let _guard = plx_base::testlock::serial();
    for (mode, remembered, opens) in [
        (LibraryOpens::Default, false, View::Recommended),
        (LibraryOpens::Default, true, View::Recommended),
        (LibraryOpens::All, false, View::All),
        (LibraryOpens::All, true, View::All),
        (LibraryOpens::Last, false, View::Recommended),
        (LibraryOpens::Last, true, View::All),
    ] {
        let mut fixture = Fixture::new(2, 24, 2);
        fixture.listing = fixture.listing.clone().with_remembered_all(remembered);
        let page = opening_on(mode, || fixture.screen());
        assert_eq!(page.view, opens, "{mode:?} with the last choice All={remembered}");
        assert_eq!(page.shelves.is_empty(), opens.is_all(), "{mode:?}/{remembered}: the shelves follow the view");
    }
}

/// With Default (or All) a switch lasts for the visit: the library keeps it while it is on screen,
/// and the setting, not the switch, decides how it opens next time. The switch is still recorded,
/// so Last would reopen it there.
#[test]
fn a_switch_lasts_for_the_visit_and_the_setting_decides_the_next_opening() {
    let _guard = plx_base::testlock::serial();
    for (mode, reopens) in [(LibraryOpens::Default, View::Recommended), (LibraryOpens::All, View::All)] {
        plx_data::browse::restore_library_opens(mode);
        let mut fixture = Fixture::new(2, 24, 2);
        let mut page = fixture.screen();
        page.initial = false;
        let mut engine = FocusEngine::new();
        let other = if mode == LibraryOpens::All { VIEW_RECOMMENDED } else { VIEW_ALL };
        let effects = fixture.step(&mut page, &mut engine, ScreenEvent::Activate(other));
        assert_eq!(recorded(&effects), [other == VIEW_ALL], "{mode:?}: the switch is recorded whatever the setting");
        assert_eq!(page.view.elem(), other);
        for n in 0..4 {
            fixture.step(&mut page, &mut engine, ScreenEvent::Tick(Tick { ms: n * 16, dt_us: 16_000 }));
        }
        assert_eq!(page.view.elem(), other, "{mode:?}: the switch holds while the library is on screen");
        open_library(&mut fixture, &mut page, &mut engine, 1);
        assert_eq!(page.view, reopens, "{mode:?}: the next library opens as the setting says");
        open_library(&mut fixture, &mut page, &mut engine, 0);
        assert_eq!(page.view, reopens, "{mode:?}: and so does the first one, reopened");
    }
    plx_data::browse::restore_library_opens(LibraryOpens::Default);
}

/// A page brought back from its entry's memory (an evicted body) comes back in the view it was
/// left in, with its scroll; a new table epoch under the same library is not an opening either.
#[test]
fn a_remounted_page_and_a_new_epoch_keep_the_view_shown() {
    let _guard = plx_base::testlock::serial();
    let mut fixture = Fixture::new(2, 24, 2);
    let mut page = fixture.screen();
    let mut engine = FocusEngine::new();
    fixture.step(&mut page, &mut engine, ScreenEvent::Activate(VIEW_ALL));
    assert_eq!(page.view, View::All);
    let memory = page.page_memory();
    assert!(memory.viewports.iter().any(|viewport| viewport.all), "the viewport records its view");
    let mut remounted = LibraryScreen::new(page.entry, page.instance, SecKind::Movie);
    remounted.restore(&memory);
    remounted.sync(&fixture.cx(&engine));
    assert_eq!(remounted.view, View::All, "the remount keeps the view");
    assert!(remounted.shelves.is_empty());

    fixture.epoch += 1;
    fixture.publish(0, 24);
    fixture.step(&mut page, &mut engine, ScreenEvent::StoreChanged(StoreId::Browse.ord(), 2));
    assert_eq!(page.epoch, Some(fixture.epoch));
    assert_eq!(page.view, View::All, "a re-discovered table under the same library keeps its view");
}

/// **CH▼ / PageDown above the grid** jumps straight to the grid's first row, past every shelf, and
/// the page scrolls with it: from a library pill, the switcher, a shelf card or the toolbar. CH▲
/// up there is not the page's, as before.
#[test]
fn channel_down_above_the_grid_jumps_to_its_first_row() {
    let _guard = plx_base::testlock::serial();
    let fixture = Fixture::new(2, 24, 2);
    let probe = fixture.screen();
    let starts = [
        (probe.libraries[0].0, LIBRARY_GROUP),
        (VIEW_RECOMMENDED, VIEWS_GROUP),
        (probe.shelves[0].elems[2], probe.shelves[0].group),
        (probe.shelves[1].elems[0], probe.shelves[1].group),
        (SORT, probe.toolbar_group()),
    ];
    for (elem, group) in starts {
        let mut page = fixture.screen();
        page.initial = false;
        let mut engine = FocusEngine::new();
        engine.set(OWNER, page.key(elem), Some(group), By::Dir);
        let mut out = Vec::new();
        let mut present = plx_machine::present::Present::new();
        let up = page.step(&ScreenEvent::App(AppMsg::Library(LibraryCmd::Page(-1))), &fixture.cx(&engine),
            &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
        assert_eq!(up, Handled::No, "CH▲ above the grid is not the page's");
        fixture.step(&mut page, &mut engine, ScreenEvent::App(AppMsg::Library(LibraryCmd::Page(1))));
        assert_eq!(engine.current(OWNER), Some(page.key(page.pair.detail.elem_at(0).unwrap())),
            "CH▼ from {elem:#x} lands on the grid's first row");
        assert_eq!(page.scroll_target, page.target_layout.row_reveal(0), "…and the page follows it there");
    }
}

/// The switcher is logical state: the view shown and its capsules' motion enter the page's hash,
/// and the capsules come to rest after a switch (a settled page stops changing).
#[test]
fn the_switcher_enters_canonical_state_and_settles() {
    let _guard = plx_base::testlock::serial();
    let hash = |page: &LibraryScreen| { let mut c = Canon::new(); page.write(&mut c); c.finish() };
    let fixture = Fixture::new(2, 24, 2);
    let mut page = fixture.screen();
    page.initial = false;
    let mut engine = FocusEngine::new();
    engine.set(OWNER, page.key(VIEW_RECOMMENDED), Some(VIEWS_GROUP), By::Dir);
    let before = hash(&page);
    fixture.step(&mut page, &mut engine, ScreenEvent::Activate(VIEW_ALL));
    assert_ne!(before, hash(&page), "the view shown is logical state");
    let mut ms = 0;
    let mut tick = |page: &mut LibraryScreen, engine: &mut FocusEngine<u32>| {
        ms += 16;
        fixture.step(page, engine, ScreenEvent::Tick(Tick { ms, dt_us: 16_000 }));
    };
    tick(&mut page, &mut engine);
    let moving = hash(&page);
    tick(&mut page, &mut engine);
    assert_ne!(moving, hash(&page), "the capsules travel to the new view and the focused pill");
    for _ in 0..240 { tick(&mut page, &mut engine); }
    let settled = hash(&page);
    tick(&mut page, &mut engine);
    assert_eq!(settled, hash(&page), "at rest the switcher changes nothing frame to frame");
}

/// The failed read-out keeps the head it always had: no switcher over a source that did not answer.
#[test]
fn a_failed_source_draws_no_switcher() {
    let _guard = plx_base::testlock::serial();
    let mut fixture = Fixture::new(2, 0, 2);
    fixture.listing = fixture.listing.clone().with_fetch(SecFetch::Failed, -1);
    let page = fixture.screen();
    let engine = FocusEngine::new();
    assert!(!page.layout.views);
    assert!(page.view_rect(VIEW_RECOMMENDED, &fixture.cx(&engine)).is_none());
    assert_eq!(page.group_of(&VIEW_ALL, &fixture.cx(&engine)), None);
}
