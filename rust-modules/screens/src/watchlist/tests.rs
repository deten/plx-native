//! The Watchlist page against a real Watchlist store: the grid it draws, what a press on each kind
//! of card asks for, the title card, the strip, the read-outs and the text that must fit.

use super::*;
use plx_data::stores::watchlist::WatchlistStore;
use plx_data::watchlist::Entry;
use plx_machine::machine::{FocusRead, Host, InputOwner, PressId, PressRead, Source, Stamped, Tick};
use plx_plex::plex::discover::WatchItem;
use plx_ui::fixture::FixtureMeasure;

struct WatchlistHost;
impl Host for WatchlistHost {
    type Arg = super::super::family::SettingsPage;
    type Fx = AppFx;
    type Msg = crate::registry::AppMsg;
    type Elem = u32;
    type Views<'a> = (plx_data::metadata::MetadataView<'a>, WatchlistView<'a>);
    type Init = super::super::family::NoInit;
    type Memory = crate::registry::PageMemory;
}
impl MetadataLike for WatchlistHost {
    fn metadata<'a>(cx: &Cx<'a, Self>) -> plx_data::metadata::MetadataView<'a> { cx.views.0 }
    fn watchlist<'a>(cx: &Cx<'a, Self>) -> Option<WatchlistView<'a>> { Some(cx.views.1) }
}

const ON: &str = "plex://movie/demo-on";
const OFF: &str = "plex://show/demo-off";
const WAIT: &str = "plex://movie/demo-wait";

fn sid() -> plx_plex::plex::ServerId { plx_plex::plex::ServerId::from_raw(3) }

fn item(guid: &str, title: &str) -> WatchItem {
    WatchItem { guid: guid.into(), kind: if guid.contains("show") { "show" } else { "movie" }.into(),
        title: title.into(), year: 2019, summary: format!("{title} summary."),
        thumb: "https://metadata-static.plex.tv/p.jpg".into(), ..Default::default() }
}

fn entries() -> Vec<Entry> {
    vec![
        Entry { item: item(ON, "On a server"), avail: Availability::OnServer(Box::new(PmsMovie {
            sid: sid(), rk: "101".into(), title: "On a server".into(), thumb: "/library/metadata/101/thumb".into(),
            ..Default::default() })) },
        Entry { item: item(OFF, "Not anywhere"), avail: Availability::Missing },
        Entry { item: item(WAIT, "Being checked"), avail: Availability::Checking },
    ]
}

struct Rig {
    meta: plx_data::stores::metadata::MetadataStore,
    store: WatchlistStore,
}

impl Rig {
    fn new(entries: Vec<Entry>) -> Self {
        let mut store = WatchlistStore::default();
        store.install_for_test(entries);
        Self { meta: Default::default(), store }
    }
    fn cx(&self, focus: Option<FocusKey<u32>>) -> Cx<'_, WatchlistHost> {
        Cx { views: (self.meta.view(), self.store.view()), tick: Tick::default(), measure: &FixtureMeasure,
            press: PressRead::default(), focus: FocusRead { current: focus, ..Default::default() },
            owner: InputOwner::Entry(EntryId(9)) }
    }
}

fn step(screen: &mut WatchlistScreen, ev: ScreenEvent<WatchlistHost>, cx: &Cx<'_, WatchlistHost>) -> Vec<Stamped<WatchlistHost>> {
    let mut present = plx_machine::present::Present::new();
    let mut out = Vec::new();
    let mut fx = Effects::new(&mut out, MachineId::Instance(InstanceId(9)), &mut present);
    Machine::<WatchlistHost>::step(screen, &ev, cx, &mut fx);
    drop(fx);
    out
}

fn mounted(rig: &Rig) -> WatchlistScreen {
    let mut screen = WatchlistScreen::new(EntryId(9), InstanceId(9));
    step(&mut screen, ScreenEvent::Tick(Tick { ms: 16, dt_us: 16_667 }), &rig.cx(None));
    screen
}

fn requests(out: &[Stamped<WatchlistHost>]) -> Vec<WatchlistReq> {
    out.iter().filter_map(|e| match &e.fx { Fx::App(AppFx::Watchlist(r)) => Some(r.clone()), _ => None }).collect()
}

fn store_cmds(out: &[Stamped<WatchlistHost>]) -> Vec<String> {
    out.iter().filter_map(|e| match &e.fx {
        Fx::App(AppFx::Store(StoreId::Watchlist, StoreCmd::Watchlist(c))) => Some(match c {
            WatchlistCmd::Open => "open".to_string(),
            WatchlistCmd::Remove { guid } => format!("remove {guid}"),
            WatchlistCmd::Add { item, .. } => format!("add {}", item.guid),
            WatchlistCmd::Reset => "reset".to_string(),
        }),
        _ => None,
    }).collect()
}

fn reseats(out: &[Stamped<WatchlistHost>], group: GroupId) -> bool {
    out.iter().any(|e| matches!(&e.fx, Fx::Deliver(_, Delivery::Screen(ScreenEvent::Enter(Enter::Fresh {
        focus: FocusTarget::ContainerGroup(g) }))) if *g == group))
}

fn back() -> ScreenEvent<WatchlistHost> {
    ScreenEvent::Input(InputEvent { at: Tick::default(), source: Source::Script,
        kind: InputKind::Key { key: Key::Back, sym: 0, wcode: 0, edge: Edge::Down, at_edge: true } })
}

fn card_key(screen: &WatchlistScreen, guid: &str) -> FocusKey<u32> {
    let elem = screen.page.keys.iter().find(|(g, _)| g == guid).map(|(_, e)| *e).expect("an interned card");
    screen.key(elem)
}

/// The grid is the store's list in its own order (newest first, as the provider sent it), each
/// card drawn from the server row when one holds it and from the provider's facts otherwise.
#[test]
fn the_grid_is_the_list_in_order_drawn_from_what_the_servers_said() {
    let rig = Rig::new(entries());
    let screen = mounted(&rig);
    let page = &screen.page;
    assert_eq!(page.elems.len(), 3);
    assert_eq!(page.rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["On a server", "Not anywhere", "Being checked"]);
    assert_eq!((page.rows[0].sid, page.rows[0].rk.as_str()), (sid(), "101"), "the server's own row");
    assert_eq!(page.rows[1].thumb, "https://metadata-static.plex.tv/p.jpg", "the provider's artwork");
    assert_eq!(page.rows[1].kind, 1, "a show stands up as a show");
    let cards = page.cards(rig.store.view());
    assert_eq!(<Cards as CardSource<WatchlistHost>>::len(&cards), 3);
    assert!(rig.store.view().entries()[1].avail.missing(), "the dimmed card");
    assert_eq!(screen.page.status, WatchlistStatus::Ready);
}

/// OK on a card a server holds asks for that server row's detail page; a hold asks for the card
/// menu. Both name the server row and the title, which the bridge re-checks.
#[test]
fn ok_on_a_server_card_opens_its_detail_and_a_hold_its_menu() {
    let rig = Rig::new(entries());
    let mut screen = mounted(&rig);
    let key = card_key(&screen, ON);
    let out = step(&mut screen, ScreenEvent::PressCommit(PressId(1)), &rig.cx(Some(key)));
    assert_eq!(requests(&out), [WatchlistReq::Detail { sid: sid(), rk: "101".into(), guid: ON.into() }]);
    let mut out = Vec::new();
    let mut present = plx_machine::present::Present::new();
    let mut fx = Effects::new(&mut out, MachineId::Instance(InstanceId(9)), &mut present);
    screen.press(key.elem, true, &rig.cx(Some(key)), &mut fx);
    drop(fx);
    assert_eq!(requests(&out), [WatchlistReq::ItemMenu { sid: sid(), rk: "101".into(), guid: ON.into() }]);
}

/// OK on a card no server holds opens the title card (focus seated on it, *Close* first); *Remove
/// from Watchlist* there is the store's optimistic remove, and the card closes.
#[test]
fn a_missing_title_opens_its_card_and_remove_takes_it_off_the_list() {
    let rig = Rig::new(entries());
    let mut screen = mounted(&rig);
    let key = card_key(&screen, OFF);
    let out = step(&mut screen, ScreenEvent::PressCommit(PressId(1)), &rig.cx(Some(key)));
    assert!(requests(&out).is_empty(), "nothing to navigate to");
    assert!(screen.card_open());
    assert_eq!(screen.card.title_for_test(), "Not anywhere (2019)");
    assert!(reseats(&out, card::CARD_GROUP), "focus goes to the card");

    // While it is up its two answers are the page's only group, seated on Close.
    let mut groups = Vec::new();
    Focusable::<WatchlistHost>::groups(&screen, &rig.cx(None), &mut groups);
    assert_eq!(groups.iter().map(|g| g.id).collect::<Vec<_>>(), [card::CARD_GROUP]);
    let nowhere = Placed { rect: Rect::FULL, rest_rect: Rect::FULL, clip: Rect::FULL, index: None };
    assert_eq!(Focusable::<WatchlistHost>::seat(&screen, card::CARD_GROUP, nowhere, &rig.cx(None)).elem, card::CLOSE_ELEM);
    let mut links = Vec::new();
    Screen::<WatchlistHost>::links(&screen, &mut links);
    assert!(links.is_empty(), "the bar is out of reach while the card is up");

    let remove = screen.key(card::REMOVE_ELEM);
    step(&mut screen, ScreenEvent::FocusMoved { from: None, to: remove, by: By::Dir }, &rig.cx(Some(remove)));
    let out = step(&mut screen, ScreenEvent::PressCommit(PressId(2)), &rig.cx(Some(remove)));
    assert_eq!(store_cmds(&out), [format!("remove {OFF}")]);
    assert!(!screen.card_open());
}

/// BACK closes an open card before it leaves the page, and *Close* removes nothing.
#[test]
fn back_closes_the_card_first_and_close_removes_nothing() {
    let rig = Rig::new(entries());
    let mut screen = mounted(&rig);
    let key = card_key(&screen, OFF);
    step(&mut screen, ScreenEvent::PressCommit(PressId(1)), &rig.cx(Some(key)));
    let close = screen.key(card::CLOSE_ELEM);
    let out = step(&mut screen, back(), &rig.cx(Some(close)));
    assert!(requests(&out).is_empty() && store_cmds(&out).is_empty(), "BACK answered the card");
    assert!(!screen.card_open());
    // the card is still fading: a second BACK during its exit is swallowed, after it the page's
    for n in 0..120 { step(&mut screen, ScreenEvent::Tick(Tick { ms: 100 + n * 16, dt_us: 16_667 }), &rig.cx(Some(key))); }
    let out = step(&mut screen, back(), &rig.cx(Some(key)));
    assert_eq!(requests(&out), [WatchlistReq::Back]);

    step(&mut screen, ScreenEvent::PressCommit(PressId(3)), &rig.cx(Some(key)));
    assert!(screen.card_open());
    let out = step(&mut screen, ScreenEvent::PressCommit(PressId(4)), &rig.cx(Some(close)));
    assert!(store_cmds(&out).is_empty(), "Close is not Remove");
    assert!(!screen.card_open());
}

/// A card still being checked answers nothing yet.
#[test]
fn a_card_still_being_checked_waits_for_its_answer() {
    let rig = Rig::new(entries());
    let mut screen = mounted(&rig);
    let key = card_key(&screen, WAIT);
    let out = step(&mut screen, ScreenEvent::PressCommit(PressId(1)), &rig.cx(Some(key)));
    assert!(requests(&out).is_empty() && !screen.card_open());
}

/// The bar's pills reach their pages from here (the Watchlist's own pill does nothing), the chip
/// opens the account menu, arriving asks the store for a fresh list, and UP/DOWN cross the bar.
#[test]
fn the_strip_reaches_every_destination_and_arriving_refreshes_the_list() {
    let rig = Rig::new(entries());
    let mut screen = mounted(&rig);
    for (n, want) in [(0, Some(WatchlistReq::Tab(HomeTab::Home))), (1, Some(WatchlistReq::Tab(HomeTab::Movies))),
        (2, Some(WatchlistReq::Tab(HomeTab::Shows))), (3, Some(WatchlistReq::Tab(HomeTab::Search))),
        (4, Some(WatchlistReq::Account)), (5, None)] {
        let out = step(&mut screen, ScreenEvent::Activate(plx_ui::dispatch::STRIP_BASE + n), &rig.cx(None));
        assert_eq!(requests(&out), want.into_iter().collect::<Vec<_>>(), "pill +{n}");
    }
    assert_eq!(STRIP_WATCHLIST_ELEM, plx_ui::dispatch::STRIP_BASE + 5, "+5: past the profile chip's +4, as on every page");
    let out = step(&mut screen, ScreenEvent::Enter(Enter::Fresh { focus: FocusTarget::ContainerGroup(GRID_GROUP) }), &rig.cx(None));
    assert_eq!(store_cmds(&out), ["open"]);
    let mut links = Vec::new();
    Screen::<WatchlistHost>::links(&screen, &mut links);
    assert!(links.iter().any(|l| l.from == STRIP && l.dir == Dir::Down && l.to == GRID_GROUP));
    assert!(links.iter().any(|l| l.from == GRID_GROUP && l.dir == Dir::Up && l.to == STRIP));
}

/// A list that lands while focus is parked on the page's own pill (there was nothing to seat while
/// it loaded) takes focus into the grid, unless the person has moved since.
#[test]
fn a_list_landing_under_an_unchosen_pill_focus_seats_the_grid() {
    let pill = FocusKey { entry: EntryId(9), elem: STRIP_WATCHLIST_ELEM };
    for chose in [false, true] {
        let mut rig = Rig::new(Vec::new());
        rig.store.state_for_test().set_status_for_test(WatchlistStatus::Loading);
        let mut screen = WatchlistScreen::new(EntryId(9), InstanceId(9));
        step(&mut screen, ScreenEvent::Tick(Tick { ms: 16, dt_us: 16_667 }), &rig.cx(Some(pill)));
        if chose {
            step(&mut screen, ScreenEvent::FocusMoved { from: None, to: pill, by: By::Dir }, &rig.cx(Some(pill)));
        }
        rig.store.install_for_test(entries());
        let out = step(&mut screen, ScreenEvent::Tick(Tick { ms: 32, dt_us: 16_667 }), &rig.cx(Some(pill)));
        let first = card_key(&screen, ON);
        let seated = out.iter().any(|e| matches!(&e.fx, Fx::Deliver(_, Delivery::Screen(ScreenEvent::Enter(Enter::Fresh {
            focus: FocusTarget::Elem(key) }))) if *key == first));
        assert_eq!(seated, !chose, "chose={chose}: the newest title, not the column under the pill");
    }
}

/// Loading, empty, a profile with no Watchlist and a failure each read out in the app's own
/// patterns: the quiet answers in the grid's region, the verdicts page-placed, Retry only on a
/// failure (and it asks for the list again).
#[test]
fn every_status_reads_out_and_only_a_failure_offers_retry() {
    let mut page = WatchlistScreen::new(EntryId(9), InstanceId(9)).page;
    page.status = WatchlistStatus::Loading;
    let o = page.status_overlay(0).unwrap();
    assert_eq!((o.kind, o.caption), (StatusKind::Working, plx_platform::i18n::msg::browse_watchlist_loading_c()));
    page.status = WatchlistStatus::Ready;
    page.empty = true;
    let o = page.status_overlay(0).unwrap();
    assert_eq!((o.kind, o.reason), (StatusKind::Empty, Some(plx_platform::i18n::msg::browse_watchlist_empty_reason_c())));
    assert!(o.action.is_none());
    page.empty = false;
    assert!(page.status_overlay(0).is_none(), "a list draws no read-out");
    page.status = WatchlistStatus::Unavailable;
    let o = page.status_overlay(0).unwrap();
    assert!(o.kind == StatusKind::Failed && o.page && o.action.is_none(), "no retry for a refusal");
    page.status = WatchlistStatus::Failed;
    let o = page.status_overlay(0).unwrap();
    assert!(o.kind == StatusKind::Failed && o.action == Some(plx_platform::i18n::msg::browse_action_retry_c()));

    let mut rig = Rig::new(Vec::new());
    rig.store.state_for_test().set_status_for_test(WatchlistStatus::Failed);
    let mut screen = mounted(&rig);
    let out = step(&mut screen, ScreenEvent::Activate(RETRY_ELEM), &rig.cx(None));
    assert_eq!(store_cmds(&out), ["open"]);
}

/// A title removed elsewhere (its card menu) while its card is open takes the card with it rather
/// than leaving a card about nothing; the grid follows the store.
#[test]
fn a_title_that_leaves_the_list_takes_its_open_card_with_it() {
    let mut rig = Rig::new(entries());
    let mut screen = mounted(&rig);
    let key = card_key(&screen, OFF);
    step(&mut screen, ScreenEvent::PressCommit(PressId(1)), &rig.cx(Some(key)));
    assert!(screen.card_open());
    rig.store.run(WatchlistCmd::Remove { guid: OFF.into() });
    step(&mut screen, ScreenEvent::Tick(Tick { ms: 48, dt_us: 16_667 }), &rig.cx(None));
    assert!(!screen.card_open());
    assert_eq!(screen.page.elems.len(), 2);
}

/// App-owned text on this page fits, in every shipped language, with the device's own advances:
/// the poster note on a grid card (two lines at most), the heading with its count, the pill.
#[test]
fn the_watchlist_text_fits_in_every_language() {
    use plx_base::fontcov::advances::ShippedMeasure;
    use plx_machine::machine::Measure as _;
    use plx_platform::i18n::{language_on_this_thread_for_test, msg, Preference};
    let mut over = Vec::new();
    for language in [Preference::En, Preference::Es, Preference::Be] {
        let _guard = language_on_this_thread_for_test(language);
        let tag = language.tag();
        let card_w = plx_ui::cards::GRID_STYLE.w;
        if !widgets::poster_note_fits(msg::browse_watchlist_not_on_server(), card_w, &ShippedMeasure) {
            over.push(format!("{tag} not_on_server on a {card_w}px card"));
        }
        for count in [1, 2, 5, 22, 100] {
            let flow = plx_ui::cards::heading_flow(msg::browse_watchlist_title(), &msg::browse_watchlist_count(count),
                |s, _dx, sz, bold, _ink| ShippedMeasure.width_str(s, sz, bold != 0));
            if flow > SCR_W - 2.0 * MARGIN_X {
                over.push(format!("{tag} heading with {count}: {flow}px"));
            }
        }
        let pill = ShippedMeasure.width_str(msg::browse_watchlist_title(), plx_ui::theme::size::BODY, true);
        if pill > 300.0 {
            over.push(format!("{tag} pill {pill}px"));
        }
    }
    assert!(over.is_empty(), "Watchlist text that would not fit:\n  {}", over.join("\n  "));
}
