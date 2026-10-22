//! Home in the Shield layout (`home::shield`): the first row seated, the fixed slot, the header
//! following focus, the doors (LEFT at a row's first card is an edge, nothing above the first row,
//! BACK home then out), the live switch, and the Standard page left as it was.

use super::*;
use plx_ui::shield as geo;

fn grid(state: &mut plx_data::pms::PmsState, rows: usize, items: usize) -> plx_data::pms::HubsSnapshot {
    let adapter = std::sync::Arc::new(plx_data::pms::PmsAdapter::default());
    plx_data::pms::seed_grid_for_test(state, &adapter, rows, items);
    plx_data::pms::hubs_snapshot(state)
}

fn key(s: &HomeScreen, row: usize, col: usize) -> FocusKey<u32> {
    FocusKey { entry: s.entry, elem: s.rows[row].elems[col] }
}

fn tick(s: &mut HomeScreen, view: HubsView<'_>, focus: Option<FocusKey<u32>>, frames: u32) -> Vec<Stamped<TestHost>> {
    tick_at(s, view, focus, 0, frames)
}

/// `frames` 60 Hz ticks from `Tick` time `from_ms`.
fn tick_at(s: &mut HomeScreen, view: HubsView<'_>, focus: Option<FocusKey<u32>>, from_ms: u32, frames: u32)
    -> Vec<Stamped<TestHost>> {
    let mut all = Vec::new();
    for frame in 0..frames {
        let (_, out, _) = step(s, view, focus, &ScreenEvent::Tick(Tick { ms: from_ms + frame * 16, dt_us: 16_667 }));
        all.extend(out);
    }
    all
}

/// The element a `reseat` asked the engine to take, if any.
fn seated(out: &[Stamped<TestHost>]) -> Option<u32> {
    out.iter().find_map(|stamped| match &stamped.fx {
        Fx::Deliver(_, Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus: FocusTarget::Elem(k) }))) => Some(k.elem),
        _ => None,
    })
}

fn back() -> ScreenEvent<TestHost> {
    ScreenEvent::Input(InputEvent {
        at: Tick::default(),
        source: Source::Sdl,
        kind: InputKind::Key { key: Key::Back, sym: 0, wcode: 0, edge: Edge::Down, at_edge: false },
    })
}

fn near(a: Rect, b: Rect) -> bool {
    [(a.x, b.x), (a.y, b.y), (a.w, b.w), (a.h, b.h)].iter().all(|(p, q)| (p - q).abs() < 0.5)
}

/// No billboard: the hero group holds nothing, the dive is pinned on the shelves, the shelves wear
/// the ring style, and focus is seated on the first row's slot card.
#[test]
fn the_page_seats_the_first_row_and_wears_no_billboard() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut state = plx_data::pms::PmsState::default();
        let snapshot = grid(&mut state, 3, 8);
        let view = snapshot.view();
        let mut s = screen(view);
        let out = tick(&mut s, view, None, 1);
        assert_eq!(seated(&out), Some(s.rows[0].elems[0]), "focus seats on the first row's first card");
        assert_eq!((s.snap.pos, s.snap_target), (1.0, 1.0), "the picture is always the shelves");
        assert!(s.grid.shelves.iter().all(|shelf| *shelf.style() == RowStyle::SHIELD_POSTER));
        let mut groups = Vec::new();
        Focusable::<TestHost>::groups(&s, &cx(view, Some(first_card(&s))), &mut groups);
        assert_eq!(groups[0].id, HERO_GROUP);
        assert_eq!(groups[0].len, 0, "no Play / Info controls");
        let row0 = groups.iter().find(|g| g.id == s.rows[0].group).unwrap();
        assert_eq!(row0.extent, Rect::new(127.0, 568.0, 1697.0, 360.0), "the first row in the slot");
    });
}

/// UP from the first row has nowhere to go: no link leads above it, and the hero group (empty)
/// is unreachable by a geometric search.
#[test]
fn up_from_the_first_row_leads_nowhere() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut state = plx_data::pms::PmsState::default();
        let snapshot = grid(&mut state, 3, 8);
        let s = screen(snapshot.view());
        let mut links = Vec::new();
        <HomeScreen as Screen<TestHost>>::links(&s, &mut links);
        assert!(links.iter().all(|l| l.from != s.rows[0].group || l.dir != Dir::Up), "no UP door off the first row");
        assert!(links.iter().all(|l| l.to != plx_ui::containers::tabs::STRIP && l.from != plx_ui::containers::tabs::STRIP));
        let mut groups = Vec::new();
        Focusable::<TestHost>::groups(&s, &cx(snapshot.view(), Some(first_card(&s))), &mut groups);
        assert_eq!(groups[0].reachable, AxisMask(0));
    });
}

/// LEFT at a row's first card is an EDGE (what opens the side panel, `Rig::edge_surface`); LEFT
/// and RIGHT elsewhere step along the row.
#[test]
fn left_at_the_first_card_is_the_edge_and_the_row_steps_otherwise() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut state = plx_data::pms::PmsState::default();
        let snapshot = grid(&mut state, 2, 8);
        let view = snapshot.view();
        let s = screen(view);
        let context = cx(view, Some(key(&s, 0, 0)));
        assert!(matches!(Focusable::<TestHost>::neighbour(&s, key(&s, 0, 0), Dir::Left, &context), Step::Edge));
        assert!(matches!(Focusable::<TestHost>::neighbour(&s, key(&s, 1, 0), Dir::Left, &context), Step::Edge));
        assert!(matches!(Focusable::<TestHost>::neighbour(&s, key(&s, 0, 3), Dir::Left, &context),
            Step::Move(k) if k == key(&s, 0, 2)));
        assert!(matches!(Focusable::<TestHost>::neighbour(&s, key(&s, 0, 3), Dir::Right, &context),
            Step::Move(k) if k == key(&s, 0, 4)));
    });
}

/// The fixed slot (spec §5.1, §1.6): focus on row 1, card 2 rests that card at x 127 popped to
/// 250x375 about its centre, row 1's cards at 568, row 2's at 1040 (its heading peeking at 976),
/// row 0 slid up under the header.
#[test]
fn the_focused_card_rests_in_the_slot_and_the_next_row_peeks() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut state = plx_data::pms::PmsState::default();
        let snapshot = grid(&mut state, 3, 8);
        let view = snapshot.view();
        let mut s = screen(view);
        let first = first_card(&s);
        tick(&mut s, view, Some(first), 2);
        let focus = key(&s, 1, 2);
        pop_card(&mut s, view, focus, 240);
        assert!((s.grid.base_y(1) - 568.0).abs() < 0.5, "row 1 in the slot: {}", s.grid.base_y(1));
        assert!((s.grid.base_y(2) - 1040.0).abs() < 0.5);
        assert!((s.grid.base_y(0) - 96.0).abs() < 0.5, "row 0 above, cut and faded");
        assert!((s.grid.shelves[1].scroll() - 2.0 * 272.0).abs() < 0.5, "the row slid under the slot");
        let slot = Rect::new(127.0, 568.0, 240.0, 360.0).scaled(geo::FOCUS_SCALE);
        assert!(near(drawn_rect(&s, view, Some(focus), 1, 2), slot), "{:?}", drawn_rect(&s, view, Some(focus), 1, 2));
        let settled = Focusable::<TestHost>::place(&s, &focus.elem, &cx(view, Some(focus)), At::SpringTarget).unwrap();
        assert!(near(settled.rect, slot));
        let neighbour = Focusable::<TestHost>::place(&s, &key(&s, 1, 3).elem, &cx(view, Some(focus)), At::SpringTarget).unwrap();
        assert!(near(neighbour.rect, Rect::new(399.0, 568.0, 240.0, 360.0)), "the next card one pitch along, at rest");
        let before = drawn_rect(&s, view, Some(focus), 1, 1);
        assert!((before.x - (127.0 - 272.0)).abs() < 0.5, "the card before the slot stays drawn, under the rail");
    });
}

/// DOWN into a row lands on the card in ITS slot, wherever that row was left.
#[test]
fn a_vertical_move_lands_on_the_card_in_the_rows_slot() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut state = plx_data::pms::PmsState::default();
        let snapshot = grid(&mut state, 2, 8);
        let view = snapshot.view();
        let mut s = screen(view);
        let from_key = key(&s, 0, 3);
        pop_card(&mut s, view, from_key, 240);
        let from = Focusable::<TestHost>::place(&s, &from_key.elem, &cx(view, Some(from_key)), At::SpringTarget).unwrap();
        let group = s.rows[1].group;
        assert_eq!(Focusable::<TestHost>::seat(&s, group, from, &cx(view, Some(from_key))), key(&s, 1, 0));
        s.grid.shelves[1].restore_scroll(2.0 * 272.0, 8);
        assert_eq!(Focusable::<TestHost>::seat(&s, group, from, &cx(view, Some(from_key))), key(&s, 1, 2),
            "the row left on its third card takes focus there");
    });
}

/// The header follows focus once it has RESTED: the first subject at once, the next only after
/// the settle, and a card merely passed through never.
#[test]
fn the_header_follows_focus_after_it_settles() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut state = plx_data::pms::PmsState::default();
        let snapshot = grid(&mut state, 2, 8);
        let view = snapshot.view();
        let mut s = screen(view);
        let rk = |s: &HomeScreen| s.header.subject().map(|m| m.rk.clone());
        let first = key(&s, 0, 0);
        tick(&mut s, view, Some(first), 1);
        assert_eq!(rk(&s).as_deref(), Some("1"), "the first subject is immediate");
        let pass = key(&s, 0, 1);
        step(&mut s, view, Some(pass), &ScreenEvent::FocusMoved { from: Some(first), to: pass, by: By::Dir });
        tick_at(&mut s, view, Some(pass), 100, 4);
        let rest = key(&s, 0, 2);
        step(&mut s, view, Some(rest), &ScreenEvent::FocusMoved { from: Some(pass), to: rest, by: By::Dir });
        tick_at(&mut s, view, Some(rest), 200, 4);
        assert_eq!(rk(&s).as_deref(), Some("1"), "nothing settled yet: the header still describes card 1");
        tick_at(&mut s, view, Some(rest), 300, 12);
        assert_eq!(rk(&s).as_deref(), Some("3"), "focus rested on card 3");
    });
}

/// BACK on the rows goes home to the first card of the first row; from there it leaves.
#[test]
fn back_goes_home_to_the_first_card_then_out() {
    let _guard = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut state = plx_data::pms::PmsState::default();
        let snapshot = grid(&mut state, 3, 8);
        let view = snapshot.view();
        let mut s = screen(view);
        let deep = key(&s, 2, 4);
        pop_card(&mut s, view, deep, 4);
        let (_, out, _) = step(&mut s, view, Some(deep), &back());
        assert_eq!(seated(&out), Some(s.rows[0].elems[0]));
        assert!(!out.iter().any(|st| matches!(&st.fx, Fx::App(AppFx::Loop(LoopReq::BackAtRoot)))));
        let home = key(&s, 0, 0);
        let (_, out, _) = step(&mut s, view, Some(home), &back());
        assert!(out.iter().any(|st| matches!(&st.fx, Fx::App(AppFx::Loop(LoopReq::BackAtRoot)))), "from home, out");
        assert!(!has_home(&out, |r| matches!(r, HomeReq::FoldToHero)), "there is no billboard to fold to");
    });
}

/// A layout switch while Home is mounted re-dresses it at the next event: the shelves change style
/// keeping their card, the focused one rests in the slot, the dive pins; back to Standard, the
/// billboard style returns and the Standard groups with it.
#[test]
fn a_live_switch_redresses_the_mounted_page() {
    let _guard = plx_base::testlock::serial();
    let mut state = plx_data::pms::PmsState::default();
    let snapshot = grid(&mut state, 2, 8);
    let view = snapshot.view();
    let mut s = screen(view);
    assert!(!s.shield);
    let focus = key(&s, 0, 3);
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    pop_card(&mut s, view, focus, 120);
    plx_ui::layout::with_shield(|| {
        tick(&mut s, view, Some(focus), 1);
        assert!(s.shield);
        assert!(s.grid.shelves.iter().all(|shelf| *shelf.style() == RowStyle::SHIELD_POSTER));
        assert!((s.grid.shelves[0].scroll() - 3.0 * 272.0).abs() < 0.5, "the focused card in the slot at once");
        assert!((s.grid.base_y(0) - 568.0).abs() < 0.5);
        assert_eq!(s.snap.pos, 1.0);
    });
    tick(&mut s, view, Some(focus), 1);
    assert!(!s.shield);
    assert!(s.grid.shelves.iter().all(|shelf| *shelf.style() == RowStyle::HOME));
    assert_eq!(s.snap.pos, 1.0, "focus is on a card: Standard shows the shelves");
    let mut groups = Vec::new();
    Focusable::<TestHost>::groups(&s, &cx(view, Some(focus)), &mut groups);
    assert_eq!(groups[0].len, 2, "the billboard's controls are back");
}

/// A Shield page whose focus sits on a billboard control (switched from Standard on the hero)
/// re-seats on the first row.
#[test]
fn a_switch_from_the_billboard_reseats_on_the_first_row() {
    let _guard = plx_base::testlock::serial();
    let mut state = plx_data::pms::PmsState::default();
    let snapshot = grid(&mut state, 2, 8);
    let view = snapshot.view();
    let mut s = screen(view);
    let hero = FocusKey { entry: s.entry, elem: HERO_PLAY_ELEM };
    tick(&mut s, view, Some(hero), 2);
    plx_ui::layout::with_shield(|| {
        let out = tick(&mut s, view, Some(hero), 1);
        assert_eq!(seated(&out), Some(s.rows[0].elems[0]));
    });
}

/// The Standard layout is untouched: the billboard's two controls, the strip doors, the glow
/// style, and a settled card where `card_x` has always put it.
#[test]
fn the_standard_page_is_unchanged() {
    let _guard = plx_base::testlock::serial();
    let mut state = plx_data::pms::PmsState::default();
    let snapshot = grid(&mut state, 2, 8);
    let view = snapshot.view();
    let mut s = screen(view);
    let out = tick(&mut s, view, None, 1);
    assert_ne!(seated(&out), Some(s.rows[0].elems[0]), "Standard seats the billboard, not the rows");
    assert!(s.grid.shelves.iter().all(|shelf| *shelf.style() == RowStyle::HOME));
    let mut groups = Vec::new();
    Focusable::<TestHost>::groups(&s, &cx(view, None), &mut groups);
    assert_eq!(groups[0].len, 2);
    let mut links = Vec::new();
    <HomeScreen as Screen<TestHost>>::links(&s, &mut links);
    assert!(links.iter().any(|l| l.from == HERO_GROUP && l.to == plx_ui::containers::tabs::STRIP));
    let focus = key(&s, 0, 1);
    let placed = Focusable::<TestHost>::place(&s, &focus.elem, &cx(view, Some(focus)), At::SpringTarget).unwrap();
    assert_eq!(placed.rest_rect.w, CARD_W * RowStyle::HOME.focus_scale);
    assert!((placed.rect.cx() - (card_x(1, 0.0) + CARD_W * 0.5)).abs() < 0.01);
}

/// The header animates (ui/CLAUDE.md, "If you add anything that ANIMATES"): a subject change's
/// dissolve and the ground's colour step report motion while they run, and a settled header
/// reports none — and the settle TIMER itself is no motion.
#[test]
fn the_header_reports_while_it_dissolves_and_goes_quiet_at_rest() {
    use super::super::shield::{Header, SETTLE_MS};
    let _guard = plx_base::testlock::serial();
    let item = |rk: &str, c: f32| std::sync::Arc::new(plx_data::pms::PmsMovie {
        rk: rk.into(), title: rk.into(), has_blur: true, blur: [[c, 0.1, 0.1]; 4], ..Default::default()
    });
    let (a, b) = (item("a", 0.6), item("b", 0.1));
    let mut h = Header::new();
    let mut ms = 0u32;
    let mut frame = |h: &mut Header, focused: &std::sync::Arc<plx_data::pms::PmsMovie>| {
        ms += 16;
        plx_machine::idle::scoped_motion(|| h.step(Some(focused), Tick { ms, dt_us: 16_667 }))
    };
    for _ in 0..600 { frame(&mut h, &a); }
    assert_eq!(frame(&mut h, &a), (false, false), "a settled header asks for nothing");
    let waiting = (0..SETTLE_MS / 16 - 1).map(|_| frame(&mut h, &b)).collect::<Vec<_>>();
    assert!(waiting.iter().all(|&(changed, moving)| !changed && !moving), "the settle is a timer, not motion");
    let mut switched = false;
    let mut moved = 0;
    for _ in 0..30 {
        let (changed, moving) = frame(&mut h, &b);
        switched |= changed;
        moved += usize::from(switched && moving);
    }
    assert!(switched && moved > 5, "the dissolve into the new subject reports while it runs ({moved} frames)");
    for _ in 0..600 { frame(&mut h, &b); }
    assert_eq!(frame(&mut h, &b), (false, false), "and goes quiet once it has landed");
}
