//! The Shield layout's chrome through the real frame: LEFT off a bar-wearing page opens the side
//! panel on that page's row, OK goes where the row says, RIGHT and BACK close it and give the page
//! its focus back, and the Standard layout is untouched (the strip is still published and LEFT at
//! the edge opens nothing).

use super::*;
use super::test_support::{detail_arg, frame, tick};
use plx_screens::sidebar::{Go, SidebarScreen};

fn sidebar(d: &Dispatcher<AppHost>) -> Option<&SidebarScreen> {
    d.nav.modals.surfaces.iter()
        .find(|s| matches!(s.entry.arg, AppArg::Sidebar(_)) && s.phase != Phase::Hidden && s.phase != Phase::Closing)?
        .entry.inst.as_ref()?.screen.as_any()?.downcast_ref::<SidebarScreen>()
}

fn sidebar_closing(d: &Dispatcher<AppHost>) -> bool {
    d.nav.modals.surfaces.iter().any(|s| matches!(s.entry.arg, AppArg::Sidebar(_)) && s.phase == Phase::Closing)
}

/// Where the open panel's focus would send the application.
fn focused_go(d: &Dispatcher<AppHost>) -> Option<Go> {
    let screen = sidebar(d)?;
    let focus = d.nav.input_owner().and_then(|owner| d.input.engine.current(owner));
    screen.go_at(screen.focused_row(focus)?)
}

fn page_focus(d: &Dispatcher<AppHost>) -> Option<FocusKey<u32>> {
    let entry = d.nav.top_page()?.id;
    d.input.engine.current(InputOwner::Entry(entry))
}

/// Settle `route` and return the next tick.
fn settle(d: &mut Dispatcher<AppHost>, rig: &mut Bridge, route: &AppArg, from: u32, frames: u32) -> u32 {
    for i in from..from + frames {
        frame(d, rig, route.clone(), tick(i), vec![]);
    }
    from + frames
}

fn press(d: &mut Dispatcher<AppHost>, rig: &mut Bridge, route: &AppArg, at: u32, key: Key) -> u32 {
    frame(d, rig, route.clone(), tick(at), script_key(key, tick(at)));
    settle(d, rig, route, at + 1, 3)
}

fn rig_with_libraries() -> Bridge {
    let mut rig = Bridge::for_test(|| 0);
    rig.stores.browse.borrow_mut().seed_two_source_table_for_test();
    rig.refresh_browse_directory();
    rig
}

#[test]
fn left_off_home_opens_the_side_panel_on_home_and_ok_goes_where_the_row_says() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut d = Dispatcher::<AppHost>::new();
        let mut rig = rig_with_libraries();
        let route = AppArg::Home;
        let t = settle(&mut d, &mut rig, &route, 0, 3);
        assert!(sidebar(&d).is_none());
        let t = press(&mut d, &mut rig, &route, t, Key::Left);
        assert!(sidebar(&d).is_some(), "LEFT with nothing to its left opens the side panel");
        assert!(matches!(d.top_arg(), Some(AppArg::Home)), "the panel overlays the page, never replaces it");
        assert_eq!(focused_go(&d), Some(Go::Home), "the panel opens on the current page's row");
        assert_eq!(d.host_policy(), (HostUpdate::Frozen, HostRender::Cached), "a Sheet over the page");

        let t = press(&mut d, &mut rig, &route, t, Key::Up);
        assert_eq!(focused_go(&d), Some(Go::Search), "UP walks the rows: Home → Search");
        let _ = rig.take_reqs();
        press(&mut d, &mut rig, &route, t, Key::Ok);
        assert!(sidebar(&d).is_none(), "OK closes the panel");
        assert_eq!(rig.take_reqs(), vec![LoopReq::Sidebar(Go::Search)], "…and asks for the row's destination");
    });
}

#[test]
fn right_and_back_close_the_side_panel_and_give_the_page_its_focus_back() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        for close in [Key::Right, Key::Back] {
            let mut d = Dispatcher::<AppHost>::new();
            let mut rig = rig_with_libraries();
            let route = AppArg::Home;
            let t = settle(&mut d, &mut rig, &route, 0, 3);
            let before = page_focus(&d);
            let host = d.nav.top_page().map(|e| e.id);
            let t = press(&mut d, &mut rig, &route, t, Key::Left);
            assert!(sidebar(&d).is_some(), "{close:?}: open first");
            let t = press(&mut d, &mut rig, &route, t, Key::Down);
            let _ = rig.take_reqs();
            press(&mut d, &mut rig, &route, t, close);
            assert!(sidebar(&d).is_none(), "{close:?} closes the panel");
            assert!(rig.take_reqs().is_empty(), "{close:?} navigates nowhere");
            assert_eq!(d.nav.top_page().map(|e| e.id), host, "{close:?}: the page is the same entry");
            assert_eq!(d.nav.input_owner(), host.map(InputOwner::Entry), "{close:?}: input is the page's again");
            assert_eq!(page_focus(&d), before, "{close:?}: focus is where it was");
        }
    });
}

#[test]
fn the_shield_layout_publishes_no_strip_and_the_standard_one_still_does() {
    let _guard = plx_base::testlock::serial();
    for route in [AppArg::Home, AppArg::Search, AppArg::Watchlist] {
        let mut d = Dispatcher::<AppHost>::new();
        let mut rig = rig_with_libraries();
        settle(&mut d, &mut rig, &route, 0, 2);
        rig.capture_chrome(&mut d);
        assert!(!d.nav.tabs.strip.is_empty(), "Standard: the top bar's strip is a focus group");
        plx_ui::layout::with_shield(|| {
            rig.capture_chrome(&mut d);
            assert!(d.nav.tabs.strip.is_empty(), "Shield: no strip for UP or a pointer to land on");
            assert_eq!(d.nav.tabs.strip_fallback, None);
            let mut glass = plx_ui::frame::glass::GlassPlan::new();
            rig.update_home_chrome(&mut d, &mut glass, 1.0 / 60.0);
            assert!(d.nav.tabs.strip.is_empty(), "the per-frame chrome step does not republish it");
        });
        rig.capture_chrome(&mut d);
        assert!(!d.nav.tabs.strip.is_empty(), "back to Standard, the strip is back");
    }
}

#[test]
fn the_standard_layout_opens_nothing_on_left() {
    let _guard = plx_base::testlock::serial();
    let mut d = Dispatcher::<AppHost>::new();
    let mut rig = rig_with_libraries();
    let route = AppArg::Home;
    let t = settle(&mut d, &mut rig, &route, 0, 3);
    press(&mut d, &mut rig, &route, t, Key::Left);
    assert!(sidebar(&d).is_none() && !sidebar_closing(&d), "the Standard layout keeps LEFT inert at the edge");
    assert!(rig.edge_surface(&AppArg::Home, plx_ui::screen::Dir::Left).is_none());
}

/// Home with rows, Shield layout, through the real frame: focus seats on the first row's first
/// card; RIGHT slides the row under the slot and opens nothing; LEFT steps back to the first card,
/// and LEFT there — nothing left of it — opens the side panel on Home.
#[test]
fn left_at_a_rows_first_card_opens_the_side_panel_and_nowhere_else() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut d = Dispatcher::<AppHost>::new();
        let mut rig = Bridge::for_test(|| 0);
        rig.stores.hubs.seed_grid_for_test(3, 8);
        let route = AppArg::Home;
        let t = settle(&mut d, &mut rig, &route, 0, 6);
        let at = |d: &Dispatcher<AppHost>, rig: &Bridge| rig.with_home(d, |s, cx, f| s.grid_position(f, cx)).flatten();
        assert_eq!(at(&d, &rig), Some((0, 0)), "focus seats on the first row's first card");
        let t = press(&mut d, &mut rig, &route, t, Key::Right);
        assert_eq!(at(&d, &rig), Some((0, 1)));
        assert!(sidebar(&d).is_none(), "RIGHT moves along the row");
        let t = press(&mut d, &mut rig, &route, t, Key::Left);
        assert_eq!(at(&d, &rig), Some((0, 0)));
        assert!(sidebar(&d).is_none(), "LEFT with a card to its left moves to it");
        let t = press(&mut d, &mut rig, &route, t, Key::Up);
        assert_eq!(at(&d, &rig), Some((0, 0)), "UP from the first row has nowhere to go");
        assert!(sidebar(&d).is_none());
        let t = press(&mut d, &mut rig, &route, t, Key::Down);
        assert_eq!(at(&d, &rig), Some((1, 0)), "DOWN lands in the next row's slot");
        press(&mut d, &mut rig, &route, t, Key::Left);
        assert!(sidebar(&d).is_some(), "LEFT at a row's first card opens the side panel");
        assert_eq!(focused_go(&d), Some(Go::Home));
    });
}

/// The Rig's answer: only LEFT, only Shield, only a bar-wearing page or a detail page (which wears
/// the Shield rail too) — and the argument names that page's row, Home's for a detail page.
#[test]
fn only_left_off_a_bar_wearing_page_in_shield_presents_the_panel() {
    use plx_screens::sidebar::Current;
    let _guard = plx_base::testlock::serial();
    let mut rig = rig_with_libraries();
    plx_ui::layout::with_shield(|| {
        for (arg, current) in [(AppArg::Home, Current::Home), (AppArg::Search, Current::Search),
            (AppArg::Watchlist, Current::Watchlist)] {
            let (surface, style) = rig.edge_surface(&arg, plx_ui::screen::Dir::Left).expect("the panel");
            assert_eq!(style, Style::Sheet);
            match surface {
                AppArg::Sidebar(sidebar) => assert_eq!(sidebar.current, current),
                _ => panic!("LEFT presented something other than the side panel"),
            }
            for dir in [plx_ui::screen::Dir::Right, plx_ui::screen::Dir::Up, plx_ui::screen::Dir::Down] {
                assert!(rig.edge_surface(&arg, dir).is_none());
            }
        }
        assert!(matches!(rig.edge_surface(&AppArg::Library, plx_ui::screen::Dir::Left),
            Some((AppArg::Sidebar(s), _)) if matches!(s.current, Current::Library { .. })),
            "inside a library the panel opens on that library");
        assert!(matches!(rig.edge_surface(&detail_arg("1"), plx_ui::screen::Dir::Left),
            Some((AppArg::Sidebar(s), Style::Sheet)) if s.current == Current::Home),
            "a detail page opens the panel too, on Home's row");
        assert!(rig.edge_surface(&detail_arg("1"), plx_ui::screen::Dir::Right).is_none());
        for arg in [AppArg::Player, AppArg::Settings(SettingsPage::Root)] {
            assert!(rig.edge_surface(&arg, plx_ui::screen::Dir::Left).is_none(), "a page with no bar has no panel");
        }
    });
    assert!(rig.edge_surface(&detail_arg("1"), plx_ui::screen::Dir::Left).is_none(), "nor in Standard");
}

/// BACK to Home from a top-level page seats the hero in Shield (there is no strip pill to land on)
/// and the pill it came from in Standard, as before.
#[test]
fn back_to_home_seats_the_hero_in_shield_and_the_pill_in_standard() {
    use plx_screens::registry::HomeCmd;
    let _guard = plx_base::testlock::serial();
    let mut d = Dispatcher::<AppHost>::new();
    let mut rig = Bridge::for_test(|| 0);
    nav_tab(&mut d, &mut rig, HomeTab::Home, Some(crate::app::chrome::Pill::Search), None);
    assert_eq!(rig.home_commands.drain(..).collect::<Vec<_>>(), vec![HomeCmd::FocusStrip(HomeTab::Search)]);
    plx_ui::layout::with_shield(|| {
        nav_tab(&mut d, &mut rig, HomeTab::Home, Some(crate::app::chrome::Pill::Search), None);
    });
    assert_eq!(rig.home_commands.drain(..).collect::<Vec<_>>(), vec![HomeCmd::Hero]);
}

/// Settings > Layout's seam: a durable write publishes the live layout, and the session keeps it.
#[test]
fn the_layout_write_is_durable_before_it_is_live() {
    use plx_plex::plex::session::LayoutMode;
    let _guard = plx_base::testlock::serial();
    let _session = plx_plex::plex::session::TempSession::new("layout-mode-write");
    assert!(!plx_ui::layout::shield());
    assert!(super::super::preferences::set_layout_mode(LayoutMode::Shield));
    assert!(plx_ui::layout::shield());
    assert_eq!(plx_plex::plex::session::peek().layout_mode(), LayoutMode::Shield);
    assert!(super::super::preferences::set_layout_mode(LayoutMode::Standard));
    assert!(!plx_ui::layout::shield());
    assert_eq!(plx_plex::plex::session::peek().layout_mode(), LayoutMode::Standard);
}

fn library(d: &Dispatcher<AppHost>) -> Option<&plx_screens::library::LibraryScreen> {
    d.nav.top_page()?.inst.as_ref()?.screen.as_any()?.downcast_ref()
}

/// A Library with a listing and Recommended shelves, on one owned server.
fn rig_with_a_library() -> Bridge {
    let mut rig = Bridge::for_test(|| 0);
    plx_plex::plex::reset_servers_for_test();
    let sid = plx_plex::plex::register_for_test("shield-library", "127.0.0.1", 9, "synthetic", "fixture");
    plx_plex::plex::set_current(sid);
    rig.stores.browse.borrow_mut().seed_registered_table_for_test([sid, sid]);
    rig.refresh_browse_directory();
    rig.browse_run(plx_data::stores::browse::BrowseCmd::SetCur(0));
    rig.stores.browse.borrow_mut().seed_items_for_test(120);
    rig.stores.browse.borrow_mut().seed_shelves_for_test(0, &["movie.inprogress.1", "movie.recentlyadded.1"], 10);
    rig
}

/// The Library, Shield layout, through the real frame: focus arrives on Recommended's first row
/// (not the tabs above it); UP reaches the tabs; LEFT at the first tab opens the side panel on
/// this library's row.
#[test]
fn the_library_seats_its_first_row_and_left_at_the_first_tab_opens_the_side_panel() {
    let _guard = plx_base::testlock::serial();
    let session = plx_plex::plex::session::TempSession::new("shield-library-nav");
    session.watching("u-shield-library-nav");
    plx_ui::layout::with_shield(|| {
        let mut d = Dispatcher::<AppHost>::new();
        let mut rig = rig_with_a_library();
        let route = AppArg::Library;
        let t = settle(&mut d, &mut rig, &route, 0, 40);
        let region = |d: &Dispatcher<AppHost>| library(d).map(|l| l.probe_viewport(page_focus(d)).0);
        assert_eq!(library(&d).and_then(|l| l.shelf_position(page_focus(&d))), Some((0, 0)),
            "focus arrives on the first row's first card, region {:?}", region(&d));
        let t = press(&mut d, &mut rig, &route, t, Key::Up);
        assert_eq!(region(&d), Some("views"), "UP from the first row reaches the tabs");
        assert!(sidebar(&d).is_none());
        press(&mut d, &mut rig, &route, t, Key::Left);
        assert!(sidebar(&d).is_some(), "LEFT at the first tab opens the side panel");
        assert!(matches!(focused_go(&d), Some(Go::Library { .. })), "…on this library's row");
    });
    plx_plex::plex::reset_servers_for_test();
}

/// The same arrival from Home through the side panel's library row (`show_library`): the page
/// mounts before its shelves land and still seats on the first row once they do.
#[test]
fn the_library_reached_from_the_side_panel_seats_its_first_row() {
    let _guard = plx_base::testlock::serial();
    let session = plx_plex::plex::session::TempSession::new("shield-library-panel");
    session.watching("u-shield-library-panel");
    plx_ui::layout::with_shield(|| {
        let mut d = Dispatcher::<AppHost>::new();
        let mut rig = rig_with_a_library();
        let t = settle(&mut d, &mut rig, &AppArg::Home, 0, 3);
        let directory = rig.directory.view();
        let section = &directory.sections()[0];
        let (sid, key, kind) = (section.sid.unwrap(), section.key, section.kind);
        rig.show_library(&mut d, sid, key, kind);
        settle(&mut d, &mut rig, &AppArg::Library, t, 40);
        let focus = page_focus(&d);
        assert_eq!(library(&d).and_then(|l| l.shelf_position(focus)), Some((0, 0)),
            "region {:?}", library(&d).map(|l| l.probe_viewport(focus).0));
    });
    plx_plex::plex::reset_servers_for_test();
}

fn search_probe(d: &Dispatcher<AppHost>) -> Option<(&'static str, i64)> {
    let entry = d.nav.top_page()?;
    let page = entry.inst.as_ref()?.screen.as_any()?.downcast_ref::<plx_screens::search::SearchScreen>()?;
    let (zone, _, col, _, _) = page.probe(page_focus(d));
    Some((zone, col))
}

/// Search in Shield, through the real frame: the page opens on its keyboard's first key, RIGHT and
/// LEFT walk the keys, UP reaches the field, and LEFT off the first key (or off the field) — nothing
/// left of either — opens the side panel on Search's row.
#[test]
fn search_opens_on_its_keyboard_and_left_off_it_opens_the_side_panel() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        for via_field in [false, true] {
            let mut d = Dispatcher::<AppHost>::new();
            let mut rig = rig_with_libraries();
            let route = AppArg::Search;
            let t = settle(&mut d, &mut rig, &route, 0, 3);
            assert_eq!(search_probe(&d), Some(("Keyboard", 0)), "the page opens on the first key");
            let t = press(&mut d, &mut rig, &route, t, Key::Right);
            assert_eq!(search_probe(&d), Some(("Keyboard", 1)));
            let t = press(&mut d, &mut rig, &route, t, Key::Left);
            assert_eq!(search_probe(&d), Some(("Keyboard", 0)));
            assert!(sidebar(&d).is_none(), "LEFT with a key to its left moves to it");
            let t = if via_field {
                let t = press(&mut d, &mut rig, &route, t, Key::Up);
                assert_eq!(search_probe(&d).map(|(zone, _)| zone), Some("Field"), "UP off the first row: the field");
                t
            } else {
                t
            };
            press(&mut d, &mut rig, &route, t, Key::Left);
            assert!(sidebar(&d).is_some(), "via_field={via_field}: LEFT off the board's edge opens the side panel");
            assert_eq!(focused_go(&d), Some(Go::Search), "on Search's own row");
        }
    });
}

/// The Watchlist in Shield, through the real frame: LEFT off the grid's first card opens the side
/// panel on the Watchlist's row.
#[test]
fn left_off_the_watchlists_first_card_opens_the_side_panel() {
    use plx_data::watchlist::{Availability, Entry};
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut d = Dispatcher::<AppHost>::new();
        let mut rig = rig_with_libraries();
        let titles = (0..4).map(|n| Entry {
            item: plx_plex::plex::discover::WatchItem { guid: format!("plex://movie/w{n}"), kind: "movie".into(),
                title: format!("Title {n}"), ..Default::default() },
            avail: Availability::Missing,
        }).collect();
        rig.stores.watchlist.install_for_test(titles);
        let route = AppArg::Watchlist;
        let t = settle(&mut d, &mut rig, &route, 0, 4);
        assert_eq!(Bridge::watchlist_focus_index(&d), Some(0), "the grid's first card");
        let t = press(&mut d, &mut rig, &route, t, Key::Right);
        assert_eq!(Bridge::watchlist_focus_index(&d), Some(1));
        let t = press(&mut d, &mut rig, &route, t, Key::Left);
        assert!(sidebar(&d).is_none());
        press(&mut d, &mut rig, &route, t, Key::Left);
        assert!(sidebar(&d).is_some(), "LEFT off the first column opens the side panel");
        assert_eq!(focused_go(&d), Some(Go::Watchlist));
    });
}
