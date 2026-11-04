//! Settings > Libraries > Library order: its rows, the fit of every string in every shipped
//! language, and the pick-up / carry / put-down interaction against a fixture directory. There is
//! no engine here, so keys arrive as the dispatcher delivers them to the owner before it moves
//! focus.
use super::*;
use super::super::test_support::cx_with;
use plx_data::stores::browse::{DirectorySnapshot, SecKind, SectionView, SrcRow};
use plx_machine::machine::{Source, Stamped};
use plx_machine::present::Present;
use plx_platform::i18n::{language_on_this_thread_for_test, SHIPPED};
use plx_plex::plex::ServerId;

fn section(sid: u16, key: i64, index: usize, kind: SecKind, title: &str, pinned: bool) -> SectionView {
    SectionView {
        sid: Some(ServerId::from_raw(sid)), key, kind,
        row: SrcRow { section: index, title: title.into(), count_line: "12 films".into(), pinned, ..Default::default() },
    }
}

/// Movies (0), TV Shows (1), Family Films (2), a library that is not a favorite (3), Family Series (4).
fn directory(epoch: u32) -> DirectorySnapshot {
    DirectorySnapshot::fixture(epoch, 0, vec![
        section(0, 1, 0, SecKind::Movie, "Movies", true),
        section(0, 2, 1, SecKind::Show, "TV Shows", true),
        section(0, 3, 2, SecKind::Movie, "Family Films", true),
        section(0, 4, 3, SecKind::Movie, "Hidden", false),
        section(0, 5, 4, SecKind::Show, "Family Series", true),
    ])
}

fn step(page: &mut LibraryOrderPage, ev: ScreenEvent<InnerHost>, directory: DirectoryView<'_>)
    -> (Handled, Vec<Stamped<InnerHost>>) {
    let mut out = Vec::new();
    let mut present = Present::new();
    let mut fx = Effects::new(&mut out, MachineId::Instance(InstanceId(0)), &mut present);
    let handled = page.step(&ev, &cx_with(None, directory), &mut fx);
    (handled, out)
}

fn key(key: Key) -> ScreenEvent<InnerHost> {
    ScreenEvent::Input(InputEvent {
        at: Tick::default(),
        source: Source::Sdl,
        kind: InputKind::Key { key, sym: 0, wcode: 0, edge: Edge::Down, at_edge: false },
    })
}

fn writes(effects: &[Stamped<InnerHost>]) -> Vec<(u32, Vec<usize>)> {
    effects.iter().filter_map(|e| match &e.fx {
        Fx::App(AppFx::Store(StoreId::Browse, StoreCmd::Browse(BrowseCmd::ArrangeLibraries { epoch, sections }))) =>
            Some((*epoch, sections.clone())),
        _ => None,
    }).collect()
}

fn titles(page: &LibraryOrderPage) -> Vec<String> {
    page.form.table.sections.iter().flat_map(|s| s.rows.iter()).map(|r| r.label.clone()).collect()
}

#[test]
fn the_page_lists_favorites_only_in_display_order() {
    let snapshot = directory(7);
    let page = LibraryOrderPage::new(EntryId(0), snapshot.view());
    assert_eq!(page.state.order, vec![0, 1, 2, 4], "a library that is not a favorite is not arranged here");
    assert_eq!(titles(&page), ["Movies", "TV Shows", "Family Films", "Family Series"]);

    let ordered = directory(7).with_display_order(vec![4, 2, 3, 1, 0]);
    let page = LibraryOrderPage::new(EntryId(0), ordered.view());
    assert_eq!(page.state.order, vec![4, 2, 1, 0], "the profile's order, favorites only");
    assert_eq!(page.state.sel, LibId(4).key(), "the first library is where focus starts");
}

#[test]
fn ok_picks_up_down_carries_and_ok_puts_down_writing_every_step() {
    let snapshot = directory(7);
    let view = snapshot.view();
    let mut page = LibraryOrderPage::new(EntryId(0), view);

    let (handled, out) = step(&mut page, key(Key::Down), view);
    assert_eq!(handled, Handled::No, "nothing held: Down is the focus engine's");
    assert!(out.is_empty());

    step(&mut page, ScreenEvent::Activate(LibId(0).key().0), view);
    assert_eq!(page.state.moving, Some(0));
    assert!(page.form.table.sections[0].rows[0].value.is_some(), "the held row says Moving");

    let (handled, out) = step(&mut page, key(Key::Down), view);
    assert_eq!(handled, Handled::Yes, "a held library takes Down from the engine");
    assert_eq!(page.state.order, vec![1, 0, 2, 4]);
    assert_eq!(writes(&out), vec![(7, vec![1, 0, 2, 4])], "each step is written at once");
    assert_eq!(page.form.selected_id(), Some(&LibId(0)), "focus travels with the library");

    step(&mut page, key(Key::Down), view);
    step(&mut page, key(Key::Down), view);
    let (_, out) = step(&mut page, key(Key::Down), view);
    assert_eq!(page.state.order, vec![1, 2, 4, 0]);
    assert!(writes(&out).is_empty(), "past the end it stays put and writes nothing");

    let (handled, out) = step(&mut page, key(Key::Left), view);
    assert_eq!(handled, Handled::Yes, "Left cannot leave the page with a library held");
    assert!(out.is_empty());

    step(&mut page, ScreenEvent::Activate(LibId(0).key().0), view);
    assert_eq!(page.state.moving, None, "OK on the held library puts it down");
    assert!(page.form.table.sections[0].rows.iter().all(|r| r.value.is_none()));
}

#[test]
fn back_puts_the_library_down_instead_of_leaving() {
    let snapshot = directory(3);
    let view = snapshot.view();
    let mut page = LibraryOrderPage::new(EntryId(0), view);
    step(&mut page, ScreenEvent::Activate(LibId(2).key().0), view);
    let (_, out) = step(&mut page, key(Key::Up), view);
    assert_eq!(writes(&out), vec![(3, vec![0, 2, 1, 4])]);
    let (handled, _) = step(&mut page, key(Key::Back), view);
    assert_eq!(handled, Handled::Yes, "BACK is the page's while a library is held");
    assert_eq!(page.state.moving, None);
    let (handled, _) = step(&mut page, key(Key::Back), view);
    assert_eq!(handled, Handled::No, "with nothing held BACK leaves as on every page");
}

#[test]
fn the_local_order_survives_its_own_landing_and_reseeds_on_a_new_table() {
    let snapshot = directory(7);
    let mut page = LibraryOrderPage::new(EntryId(0), snapshot.view());
    page.state.order = vec![2, 0, 1, 4];
    // The store lands this page's write: same epoch, same favorites, any order.
    let landed = directory(7).with_display_order(vec![2, 0, 1, 3, 4]);
    page.sync(landed.view());
    assert_eq!(page.state.order, vec![2, 0, 1, 4]);
    // A re-discovered table is a new epoch: its order wins.
    let fresh = directory(8);
    assert!(page.sync(fresh.view()));
    assert_eq!(page.state.order, vec![0, 1, 2, 4]);
}

#[test]
fn with_no_favorite_the_page_says_so_and_seats_nothing() {
    let snapshot = DirectorySnapshot::fixture(1, 0, vec![section(0, 4, 0, SecKind::Movie, "Hidden", false)]);
    let mut page = LibraryOrderPage::new(EntryId(0), snapshot.view());
    assert!(page.state.order.is_empty());
    assert!(!titles(&page).iter().any(|title| title == "Hidden"), "a library that is not a favorite has no row");
    assert!(page.form.selected_id().is_none(), "there is no library row to stand on");
    let (handled, out) = step(&mut page, ScreenEvent::Mount, snapshot.view());
    assert_eq!(handled, Handled::No);
    assert!(out.is_empty());
}

#[test]
fn every_library_order_row_fits_its_column_in_every_language() {
    let frame_w = RouteLayout::screen().sectioned_table().w;
    let mut out = Vec::new();
    for language in SHIPPED {
        let _guard = language_on_this_thread_for_test(language);
        let tag = language.tag();
        let rows = vec![
            OrderRow { section: 0, title: "A library name a server owner made very long indeed".into(),
                detail: plx_platform::i18n::msg::browse_person_films(12_345) },
            OrderRow { section: 1, title: "TV".into(), detail: plx_platform::i18n::msg::browse_person_shows(1) },
        ];
        for (state, order, moving) in [
            ("rest", vec![0, 1], None),
            ("moving", vec![1, 0], Some(0)),
            ("empty", vec![], None),
        ] {
            let mut form = FormTable::<LibId, OrderAction, SettingsPage>::new(super::super::super::registry::BAND);
            form.table.compact = false;
            form.set(order_form(&rows, &order, moving), None);
            out.extend(form.table.app_fit_failures(frame_w, &format!("{tag} library order {state}")));
        }
    }
    plx_ui::table::assert_no_fit_failures(&out);
}
