//! The side panel's own suite: its rows from a directory, the measured layout they land on, focus
//! that opens on the current page and skips the header, what OK / RIGHT / BACK ask for, the scroll
//! that keeps focus in the frame, and app-owned words that fit their column in every language.

use super::*;
use crate::registry::{AppMsg, PageMemory};
use plx_data::stores::browse::{DirectorySnapshot, SectionView, SrcRow};
use plx_machine::machine::{FocusRead, Host, InputEvent, InputOwner, InstanceId, MachineId, Source, Stamped, Tick};
use plx_ui::fixture::FixtureMeasure;

struct SideHost;
impl Host for SideHost {
    type Arg = plx_ui::fixture::FixtureArg;
    type Fx = AppFx;
    type Msg = AppMsg;
    type Elem = u32;
    type Views<'a> = DirectoryView<'a>;
    type Init = plx_ui::fixture::FixtureArg;
    type Memory = PageMemory;
}
impl DirectoryLike for SideHost {
    fn directory<'a>(cx: &Cx<'a, Self>) -> DirectoryView<'a> {
        cx.views
    }
}

const ENTRY: EntryId = EntryId(9);

fn sid(n: u16) -> ServerId {
    ServerId::from_raw(n)
}

/// Movies (1), Shows (2), a library that is not a favourite (3), Kids (4), shown in the order
/// 4, 2, 3, 1 — the profile's library order, which the panel must follow.
fn directory() -> DirectorySnapshot {
    let section = |i: usize, kind, title: &str, pinned| SectionView {
        sid: Some(sid(1)),
        key: i as i64 + 1,
        kind,
        row: SrcRow { section: i, title: title.into(), pinned, ..Default::default() },
    };
    DirectorySnapshot::fixture(3, 0, vec![
        section(0, SecKind::Movie, "Movies", true),
        section(1, SecKind::Show, "TV Shows", true),
        section(2, SecKind::Movie, "Hidden", false),
        section(3, SecKind::Movie, "Kids Movies", true),
    ]).with_display_order(vec![3, 1, 2, 0])
}

fn many_libraries(n: usize) -> DirectorySnapshot {
    DirectorySnapshot::fixture(3, 0, (0..n).map(|i| SectionView {
        sid: Some(sid(1)),
        key: i as i64 + 1,
        kind: if i % 2 == 0 { SecKind::Movie } else { SecKind::Show },
        row: SrcRow { section: i, title: format!("Library {i}"), pinned: true, ..Default::default() },
    }).collect())
}

fn panel(current: Current, directory: &DirectorySnapshot) -> SidebarScreen {
    let arg = SidebarArg { current, profile: Profile { name: "Owner".into(), initial: "O".into(), ..Default::default() } };
    SidebarScreen::new(ENTRY, arg, directory.view(), &FixtureMeasure)
}

fn cx<'a>(directory: &'a DirectorySnapshot, focus: Option<u32>) -> Cx<'a, SideHost> {
    Cx {
        views: directory.view(),
        tick: Tick { ms: 0, dt_us: 16_000 },
        measure: &FixtureMeasure,
        press: Default::default(),
        focus: FocusRead { current: focus.map(|elem| FocusKey { entry: ENTRY, elem }), ..Default::default() },
        owner: InputOwner::Entry(ENTRY),
    }
}

fn step(screen: &mut SidebarScreen, ev: ScreenEvent<SideHost>, cx: &Cx<'_, SideHost>) -> (Handled, Vec<Stamped<SideHost>>) {
    let mut out = Vec::new();
    let mut present = plx_machine::present::Present::new();
    let handled = {
        let mut fx = Effects::new(&mut out, MachineId::Instance(InstanceId(1)), &mut present);
        Machine::<SideHost>::step(screen, &ev, cx, &mut fx)
    };
    (handled, out)
}

fn key(k: Key) -> ScreenEvent<SideHost> {
    ScreenEvent::Input(InputEvent {
        at: Tick { ms: 0, dt_us: 0 },
        source: Source::Script,
        kind: InputKind::Key { key: k, sym: 0, wcode: 0, edge: Edge::Down, at_edge: false },
    })
}

/// What the step asked for: the destination, and whether the panel dismissed itself.
fn asked(out: &[Stamped<SideHost>]) -> (Option<Go>, bool) {
    let go = out.iter().find_map(|s| match &s.fx {
        Fx::App(AppFx::Loop(LoopReq::Sidebar(go))) => Some(*go),
        _ => None,
    });
    let dismissed = out.iter().any(|s| matches!(s.fx, Fx::Nav(NavOp::Dismiss(e)) if e == ENTRY));
    (go, dismissed)
}

fn seat(screen: &SidebarScreen, d: &DirectorySnapshot) -> u32 {
    let placed = Placed { rect: side_nav::PANEL, rest_rect: side_nav::PANEL, clip: Rect::FULL, index: None };
    Focusable::<SideHost>::seat(screen, GroupId(0), placed, &cx(d, None)).elem
}

fn go_of(screen: &SidebarScreen, elem: u32) -> Option<Go> {
    screen.go_at(screen.focused_row(Some(FocusKey { entry: ENTRY, elem }))?)
}

/// The rows, in order: profile, Watchlist, Search, Home, the header, every favourite in the
/// profile's library order (the non-favourite left out), Settings — on the measured pitches.
#[test]
fn the_rows_follow_the_library_order_and_land_on_the_measured_layout() {
    let d = directory();
    let screen = panel(Current::Home, &d);
    let lib = |key, kind| Go::Library { sid: sid(1), key, kind };
    let gos: Vec<_> = (0..screen.row_rects().len()).map(|i| screen.go_at(i)).collect();
    assert_eq!(gos, vec![
        Some(Go::Account), Some(Go::Watchlist), Some(Go::Search), Some(Go::Home), None,
        Some(lib(4, SecKind::Movie)), Some(lib(2, SecKind::Show)), Some(lib(1, SecKind::Movie)),
        Some(Go::Settings),
    ]);
    let tops: Vec<f32> = screen.row_rects().iter().map(|r| r.y).collect();
    assert_eq!(tops, vec![66.0, 142.0, 210.0, 278.0, 358.0, 398.0, 466.0, 534.0, 602.0]);
    for r in screen.row_rects() {
        assert_eq!((r.x, r.w), (66.0, 320.0), "every row sits in the 320 column at x 66");
    }
}

/// No favourite library, no header: the panel still offers every destination the bar did.
#[test]
fn a_profile_with_no_libraries_has_no_header() {
    let d = DirectorySnapshot::fixture(3, 0, Vec::new());
    let screen = panel(Current::Home, &d);
    let gos: Vec<_> = (0..screen.row_rects().len()).map(|i| screen.go_at(i)).collect();
    assert_eq!(gos, vec![Some(Go::Account), Some(Go::Watchlist), Some(Go::Search), Some(Go::Home), Some(Go::Settings)]);
}

/// The panel opens on the current page's row: Home on Home, the library inside it.
#[test]
fn focus_opens_on_the_current_pages_row() {
    let d = directory();
    for (current, want) in [
        (Current::Home, Go::Home),
        (Current::Search, Go::Search),
        (Current::Watchlist, Go::Watchlist),
        (Current::Library { sid: sid(1), key: 2 }, Go::Library { sid: sid(1), key: 2, kind: SecKind::Show }),
    ] {
        let screen = panel(current, &d);
        assert_eq!(go_of(&screen, seat(&screen, &d)), Some(want), "{current:?}");
    }
    let screen = panel(Current::Library { sid: sid(9), key: 99 }, &d);
    assert_eq!(go_of(&screen, seat(&screen, &d)), Some(Go::Home), "a library no longer listed falls back to Home");
}

/// UP and DOWN walk the rows and skip the header; LEFT and RIGHT never move.
#[test]
fn up_and_down_skip_the_header_and_left_right_stay() {
    let d = directory();
    let screen = panel(Current::Home, &d);
    let c = cx(&d, None);
    let home = seat(&screen, &d);
    let at = |elem| FocusKey { entry: ENTRY, elem };
    let moved = |from: u32, dir| match Focusable::<SideHost>::neighbour(&screen, at(from), dir, &c) {
        Step::Move(k) => Some(k.elem),
        _ => None,
    };
    let first_library = moved(home, Dir::Down).expect("a row below Home");
    assert_eq!(go_of(&screen, first_library), Some(Go::Library { sid: sid(1), key: 4, kind: SecKind::Movie }));
    assert_eq!(moved(first_library, Dir::Up), Some(home));
    for dir in [Dir::Left, Dir::Right] {
        assert_eq!(moved(home, dir), None);
    }
    let account = screen.key_of(Go::Account).unwrap();
    assert_eq!(moved(account, Dir::Up), None, "the profile row is the top");
    let settings = screen.key_of(Go::Settings).unwrap();
    assert_eq!(moved(settings, Dir::Down), None, "Settings is the bottom");
}

/// OK goes where the row says and dismisses; RIGHT and BACK only dismiss, except RIGHT on the
/// profile row, which opens what OK opens.
#[test]
fn ok_navigates_and_right_and_back_close() {
    let d = directory();
    for go in [Go::Account, Go::Watchlist, Go::Search, Go::Home, Go::Settings,
        Go::Library { sid: sid(1), key: 2, kind: SecKind::Show }] {
        let mut screen = panel(Current::Home, &d);
        let elem = screen.key_of(go).unwrap();
        let (_, out) = step(&mut screen, ScreenEvent::Activate(elem), &cx(&d, Some(elem)));
        assert_eq!(asked(&out), (Some(go), true), "{go:?}");
    }
    let mut screen = panel(Current::Home, &d);
    let home = screen.key_of(Go::Home).unwrap();
    for k in [Key::Right, Key::Back] {
        let (handled, out) = step(&mut screen, key(k), &cx(&d, Some(home)));
        assert_eq!(handled, Handled::Yes);
        assert_eq!(asked(&out), (None, true), "{k:?} closes and goes nowhere");
    }
    let account = screen.key_of(Go::Account).unwrap();
    let (_, out) = step(&mut screen, key(Key::Right), &cx(&d, Some(account)));
    assert_eq!(asked(&out), (Some(Go::Account), true), "RIGHT on the profile row opens it");
}

/// A list taller than the frame scrolls to keep focus inside it: on the last row, its bottom sits
/// 34 above the frame's (b02), and back on the profile row the list is home again.
#[test]
fn focus_past_the_frame_scrolls_the_list() {
    let d = many_libraries(14);
    let mut screen = panel(Current::Home, &d);
    let settings = screen.key_of(Go::Settings).unwrap();
    let account = screen.key_of(Go::Account).unwrap();
    let moved = |to| ScreenEvent::FocusMoved { from: None, to: FocusKey { entry: ENTRY, elem: to }, by: plx_machine::machine::By::Dir };
    step(&mut screen, moved(settings), &cx(&d, Some(settings)));
    let last = *screen.row_rects().last().unwrap();
    assert_eq!(last.y + last.h, side_nav::LIST_BOTTOM);
    step(&mut screen, moved(account), &cx(&d, Some(account)));
    assert_eq!(screen.row_rects()[0].y, side_nav::LIST_TOP);
}

/// The app's own words in the panel fit their column in every shipped language, at the device's
/// whole-pixel advances; a library's name is the server's and elides instead.
#[test]
fn the_panels_own_words_fit_in_every_shipped_language() {
    use plx_base::fontcov::advances::ShippedMeasure;
    use plx_machine::machine::Measure;
    for language in plx_platform::i18n::SHIPPED {
        let _guard = plx_platform::i18n::language_on_this_thread_for_test(language);
        for word in [
            plx_platform::i18n::msg::browse_watchlist_title(),
            plx_platform::i18n::msg::browse_sidebar_search(),
            plx_platform::i18n::msg::browse_chrome_home(),
            plx_platform::i18n::msg::settings_account_settings(),
        ] {
            let w = ShippedMeasure.width_str(word, side_nav::LABEL_SIZE, false);
            assert!(w <= side_nav::LABEL_MAX_W, "{language:?}: {word:?} is {w}px of {}", side_nav::LABEL_MAX_W);
        }
        let header = plx_platform::i18n::msg::browse_sidebar_libraries();
        let w = ShippedMeasure.width_str(header, side_nav::HEADER_SIZE, false);
        assert!(side_nav::HEADER_X + w + side_nav::HEADER_RULE_GAP < side_nav::HEADER_RULE_END,
            "{language:?}: the header leaves room for its rule");
        for pm in [false, true] {
            let time = if pm { plx_platform::i18n::msg::browse_clock_pm("12:59") } else { plx_platform::i18n::msg::browse_clock_am("12:59") };
            let w = ShippedMeasure.width_str(&time, side_nav::CLOCK_SIZE, false);
            assert!(side_nav::clock_rect(w).x > 1400.0,
                "{language:?}: the widest clock reading stays a pill at the right edge ({w}px)");
        }
    }
}
