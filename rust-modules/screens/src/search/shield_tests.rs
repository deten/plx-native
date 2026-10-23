//! The Search page in the Shield layout (`shield.rs`): the keyboard where c01 measured it, typing
//! on it into the page's one query, the board's walk and its doors (the field, the results, the side
//! panel's edge), the captioned four-column result grid and its reveal, the recents in the results
//! column, the Standard page untouched, and a live switch in both directions.
use super::*;
use crate::search::shield::{key_elem, KEYS_GROUP};
use plx_ui::shield_search as ss;

/// The key element that types `ch`.
fn key_of(screen: &SearchScreen, ch: char) -> FocusKey<u32> {
    screen.key(key_elem(ss::CHARS.find(ch).expect("a key on the board")))
}

/// The engine seated where a Shield mount seats it: on the first key.
fn on_keys(screen: &SearchScreen, fixture: &Fixture) -> FocusEngine<u32> {
    let mut engine = FocusEngine::new();
    engine.enter(OWNER, screen, FocusTarget::Elem(screen.first_key()), None, &fixture.cx(None));
    engine
}

fn place(screen: &SearchScreen, fixture: &Fixture, focus: Option<FocusKey<u32>>, elem: u32) -> Option<Rect> {
    <SearchScreen as Focusable<HostFixture>>::place(screen, &elem, &fixture.cx(focus), At::SpringTarget).map(|p| p.rect)
}

fn reseated_on(out: &[Stamped<HostFixture>], key: FocusKey<u32>) -> bool {
    out.iter().any(|effect| matches!(&effect.fx, Fx::Deliver(_, Delivery::Screen(ScreenEvent::Enter(Enter::Fresh {
        focus: FocusTarget::Elem(k) }))) if *k == key))
}

fn scoped_query(out: &[Stamped<HostFixture>]) -> Option<String> {
    out.iter().rev().find_map(|effect| match &effect.fx {
        Fx::App(AppFx::Store(StoreId::Search, StoreCmd::Search(SearchCmd::SetQueryScoped { query, .. }))) => Some(query.clone()),
        _ => None,
    })
}

/// c01: the page opens with the first key focused, and every key stands where the dump put it —
/// the place focus answers and the stop a pointer resolves against are both that rect.
#[test]
fn the_page_opens_on_the_first_key_and_the_keys_stand_where_c01_measured_them() {
    let _serial = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let fixture = Fixture::new();
        let mut screen = SearchScreen::new(ENTRY, INSTANCE);
        let (_, out, _) = deliver(&mut screen, &fixture, None, ScreenEvent::Mount);
        assert!(reseated_on(&out, key_of(&screen, 'a')), "a Shield mount seats the first key, not the field");
        for (ch, rect) in [('a', Rect::new(132.0, 128.0, 64.0, 64.0)), ('f', Rect::new(492.0, 128.0, 64.0, 64.0)),
            ('g', Rect::new(132.0, 200.0, 64.0, 64.0)), ('4', Rect::new(492.0, 416.0, 64.0, 64.0)),
            ('0', Rect::new(492.0, 488.0, 64.0, 64.0))] {
            assert_eq!(place(&screen, &fixture, None, key_of(&screen, ch).elem), Some(rect), "key {ch}");
        }
        for (i, x) in [(36, 132.0), (37, 276.0), (38, 420.0)] {
            assert_eq!(place(&screen, &fixture, None, key_elem(i)), Some(Rect::new(x, 560.0, 136.0, 64.0)), "wide key {i}");
        }
        assert_eq!(place(&screen, &fixture, None, FIELD), Some(ss::field_plate()), "the field is its plate");
        let keys: Vec<u32> = (0..ss::KEY_COUNT).map(key_elem).collect();
        assert_eq!(hit(&screen, &fixture, &keys, 164.0, 160.0), Some(key_elem(0)), "a click on a hits a");
        assert_eq!(hit(&screen, &fixture, &keys, 344.0, 592.0), Some(key_elem(37)), "the backspace key's middle");
        assert_eq!(hit(&screen, &fixture, &keys, 200.0, 196.0), None, "the gap between keys is no key");
    });
}

/// A key types into the page's own draft through the same scoped store command the television's
/// keyboard's text takes; space, backspace and clear are the board's three wide keys; and OK on the
/// field still raises the television's keyboard, typing into the same query.
#[test]
fn typing_on_the_board_edits_the_one_query_the_system_keyboard_also_types_into() {
    let _serial = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let fixture = Fixture::new();
        let mut screen = fixture.screen();
        let mut last = None;
        for ch in "sum".chars() {
            let key = key_of(&screen, ch);
            let (handled, out, _) = deliver(&mut screen, &fixture, Some(key), ScreenEvent::Activate(key.elem));
            assert_eq!(handled, Handled::Yes);
            last = scoped_query(&out);
        }
        assert_eq!(screen.draft.query(), "sum");
        assert_eq!(last.as_deref(), Some("sum"), "every key is one scoped store command");
        let wide = |i: usize| Some(FocusKey { entry: ENTRY, elem: key_elem(i) });
        deliver(&mut screen, &fixture, wide(36), ScreenEvent::Activate(key_elem(36)));
        assert_eq!(screen.draft.query(), "sum ", "space");
        deliver(&mut screen, &fixture, wide(37), ScreenEvent::Activate(key_elem(37)));
        assert_eq!(screen.draft.query(), "sum", "backspace");
        let one = key_of(&screen, '1');
        deliver(&mut screen, &fixture, Some(one), ScreenEvent::Activate(one.elem));
        assert_eq!(screen.draft.query(), "sum1", "the digits type too");
        let (_, out, _) = deliver(&mut screen, &fixture, wide(38), ScreenEvent::Activate(key_elem(38)));
        assert_eq!((screen.draft.query(), scoped_query(&out).as_deref()), ("", Some("")), "clear empties the query");
        assert!(!screen.editing, "the board never raises the television's keyboard");

        let field = Some(screen.key(FIELD));
        deliver(&mut screen, &fixture, field, ScreenEvent::Activate(FIELD));
        assert!(screen.editing, "OK on the field still raises the television's keyboard");
        deliver(&mut screen, &fixture, field, text_event(TextEdit::Commit("wal".into())));
        deliver(&mut screen, &fixture, field, key_event(Key::Ok, 0));
        assert!(!screen.editing);
        let l = key_of(&screen, 'l');
        deliver(&mut screen, &fixture, Some(l), ScreenEvent::Activate(l.elem));
        assert_eq!(screen.draft.query(), "wall", "both type into the one query");
    });
}

/// The board walks itself; UP off its first row is the field and DOWN from the field the board
/// again; LEFT off its first column (and off the field) goes nowhere on the page — the side panel's
/// edge; RIGHT off its last column enters the results, and LEFT off a result's first column
/// returns to the key it left.
#[test]
fn the_board_walks_to_the_field_the_results_and_the_side_panels_edge() {
    let _serial = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut fixture = Fixture::new();
        fixture.query("walk").shelves(vec![shelf(Kind::Movie, "m", 6), shelf(Kind::Show, "s", 3)]);
        let mut screen = fixture.screen();
        let mut engine = on_keys(&screen, &fixture);
        let a = key_of(&screen, 'a');
        let doors = links(&screen);
        assert!(matches!(engine.move_dir(OWNER, &screen, &doors, Dir::Left, &fixture.cx(Some(a))),
            Outcome::Edge(_) | Outcome::Nothing), "LEFT off a moves nothing: the dispatcher's edge");
        assert_eq!(step_dir(&mut screen, &fixture, &mut engine, Dir::Up), Some(screen.key(FIELD)));
        let doors = links(&screen);
        assert!(matches!(engine.move_dir(OWNER, &screen, &doors, Dir::Left, &fixture.cx(Some(screen.key(FIELD)))),
            Outcome::Edge(_) | Outcome::Nothing), "LEFT off the field too");
        assert_eq!(step_dir(&mut screen, &fixture, &mut engine, Dir::Down), Some(a), "DOWN returns to the key it left");
        for _ in 0..5 { step_dir(&mut screen, &fixture, &mut engine, Dir::Right); }
        assert_eq!(engine.current(OWNER), Some(key_of(&screen, 'f')));
        let first = step_dir(&mut screen, &fixture, &mut engine, Dir::Right).unwrap();
        assert_eq!(first.elem, screen.rows[0].elems[0], "RIGHT off f: the first result");
        let second = step_dir(&mut screen, &fixture, &mut engine, Dir::Right).unwrap();
        assert_eq!(second.elem, screen.rows[0].elems[1]);
        let fifth = step_dir(&mut screen, &fixture, &mut engine, Dir::Down).unwrap();
        assert_eq!(fifth.elem, screen.rows[0].elems[5], "DOWN within the grid: four across");
        let show = step_dir(&mut screen, &fixture, &mut engine, Dir::Down).unwrap();
        assert_eq!(index_of(&screen, show.elem).map(|(g, _)| g), Some(screen.rows[1].group), "DOWN to the next group");
        step_dir(&mut screen, &fixture, &mut engine, Dir::Up);
        step_dir(&mut screen, &fixture, &mut engine, Dir::Left);
        assert_eq!(engine.current(OWNER).map(|k| k.elem), Some(screen.rows[0].elems[4]));
        assert_eq!(step_dir(&mut screen, &fixture, &mut engine, Dir::Left), Some(key_of(&screen, 'f')),
            "LEFT off the grid's first column: back to the key it left");
        assert_eq!(<SearchScreen as Focusable<HostFixture>>::group_of(&screen, &key_elem(0), &fixture.cx(None)),
            Some(KEYS_GROUP));
    });
}

/// c01: the results stand four across at x 686, 979, 1271, 1564, the first row's cards at y 192
/// under its heading at 128, the second at 652; the next group's heading follows the first group's
/// last row and its people are 240 circles on the same columns.
#[test]
fn the_results_are_a_captioned_four_column_grid_under_their_group_headings() {
    let _serial = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut fixture = Fixture::new();
        fixture.query("grid").shelves(vec![shelf(Kind::Movie, "m", 6), shelf(Kind::Person, "p", 2)]);
        let screen = fixture.screen();
        let movies = &screen.rows[0].elems;
        let at = |elem: u32| place(&screen, &fixture, None, elem).expect("a placed card");
        for (i, x) in [686.0, 979.0, 1271.0, 1564.0].into_iter().enumerate() {
            let r = at(movies[i]);
            assert!((r.x - x).abs() < 1.0 && r.y == 192.0 && (r.w, r.h) == (240.0, 360.0), "card {i}: {r:?}");
        }
        assert_eq!((at(movies[4]).x, at(movies[4]).y), (686.0, 652.0), "row two, 460 under row one");
        let people = &screen.rows[1].elems;
        let person = at(people[0]);
        assert_eq!((person.x, person.y, person.w, person.h), (686.0, 192.0 + 2.0 * 460.0 + 64.0, 240.0, 240.0),
            "the people's heading 64 over their circles, after the movies' two rows");
        assert_eq!(screen.probe_below(), "Results");
        assert_eq!(screen.hint(&fixture.cx(None)), shield::Hint::Scope, "the hint line names the results' source");
    });
}

/// A result that does not show whole brings itself up the least that shows it, and never above
/// the first row's line; the first row rests the column at its head. The keyboard never scrolls it.
#[test]
fn a_focused_result_brings_itself_into_view_and_the_keys_scroll_nothing() {
    let _serial = plx_base::testlock::serial();
    plx_ui::layout::with_shield(|| {
        let mut fixture = Fixture::new();
        fixture.query("deep").shelves(vec![shelf(Kind::Movie, "m", 16)]);
        let mut screen = fixture.screen();
        let elems = screen.rows[0].elems.clone();
        let focus = |screen: &mut SearchScreen, elem: u32| {
            let to = screen.key(elem);
            deliver(screen, &fixture, Some(to), ScreenEvent::FocusMoved { from: None, to, by: By::Dir });
            screen.stack.target()
        };
        // a focused poster pops 7.5 down, and its caption with it
        let near = |got: f32, want: f32| (got - want).abs() < 0.01;
        assert!(near(focus(&mut screen, elems[4]), 7.5), "row two comes up just enough for its popped caption");
        assert!(near(focus(&mut screen, elems[8]), 467.5), "row three takes row two's place");
        assert!(near(focus(&mut screen, elems[12]), 927.5));
        assert_eq!(focus(&mut screen, elems[8]), 920.0, "the row above comes down to the first row's line");
        assert_eq!(focus(&mut screen, elems[4]), 460.0, "and the one above it, no further");
        assert_eq!(focus(&mut screen, key_elem(3)), 460.0, "a key leaves the column where it is");
        assert_eq!(focus(&mut screen, elems[1]), 0.0, "the first row rests the column at its head");
    });
}

/// Before anything is typed the results column holds the profile's recent searches under their
/// heading and the Clear chip under them, reached from the board and left back to it.
#[test]
fn the_recent_searches_stand_in_the_results_column() {
    let _serial = plx_base::testlock::serial();
    let session = remembering("shield-search-recents", &["gromit", "wallace"]);
    plx_ui::layout::with_shield(|| {
        let fixture = Fixture::new();
        let mut screen = fixture.screen();
        assert_eq!(screen.recents.len(), 2);
        assert_eq!(screen.hint(&fixture.cx(None)), shield::Hint::Prompt);
        let first = screen.recents[0];
        assert_eq!(place(&screen, &fixture, None, first), Some(ss::recent_rect(0, ss::HEAD_H)));
        assert_eq!(place(&screen, &fixture, None, screen.recents[1]).map(|r| r.y), Some(192.0 + ss::RECENT_PITCH));
        let clear = place(&screen, &fixture, None, CLEAR).expect("the Clear chip");
        assert_eq!((clear.x, clear.y), (ss::RESULTS_X, ss::HEAD_H + ss::recents_h(2) + ss::CLEAR_AIR));
        let mut engine = on_keys(&screen, &fixture);
        for _ in 0..5 { step_dir(&mut screen, &fixture, &mut engine, Dir::Right); }
        assert_eq!(step_dir(&mut screen, &fixture, &mut engine, Dir::Right).map(|k| k.elem), Some(first));
        assert_eq!(step_dir(&mut screen, &fixture, &mut engine, Dir::Down).map(|k| k.elem), Some(screen.recents[1]));
        assert_eq!(step_dir(&mut screen, &fixture, &mut engine, Dir::Down).map(|k| k.elem), Some(CLEAR));
        assert_eq!(step_dir(&mut screen, &fixture, &mut engine, Dir::Left), Some(key_of(&screen, 'f')));
    });
    drop(session);
}

/// The Standard page is what it was: the field focus seats on mount and its document rect, no key
/// is an element of the page, the strip link heads the links, and a key's activation is not the
/// page's.
#[test]
fn the_standard_page_has_no_keyboard_and_keeps_its_field() {
    let _serial = plx_base::testlock::serial();
    let fixture = Fixture::new();
    let mut screen = SearchScreen::new(ENTRY, INSTANCE);
    let (_, out, _) = deliver(&mut screen, &fixture, None, ScreenEvent::Mount);
    assert!(reseated_on(&out, screen.key(FIELD)));
    assert_eq!(place(&screen, &fixture, None, FIELD), Some(layout::FIELD));
    assert_eq!(place(&screen, &fixture, None, key_elem(0)), None);
    assert_eq!(links(&screen).first(), Some(&Link { from: STRIP, dir: Dir::Down, to: FIELD_GROUP }));
    let (handled, _, _) = deliver(&mut screen, &fixture, None, ScreenEvent::Activate(key_elem(0)));
    assert_eq!((handled, screen.draft.query()), (Handled::No, ""));
    let mut groups = Vec::new();
    <SearchScreen as Focusable<HostFixture>>::groups(&screen, &fixture.cx(None), &mut groups);
    assert!(groups.iter().all(|g| g.id != KEYS_GROUP));
}

/// A switch while the page is up re-lays it in place: the results become the grid and back, the
/// focused card keeps its focus, the query is untouched, and a key — which Standard does not have —
/// gives its focus to the field.
#[test]
fn a_live_switch_relays_the_page_and_keeps_its_query_and_its_card() {
    let _serial = plx_base::testlock::serial();
    let mut fixture = Fixture::new();
    fixture.query("live").shelves(vec![shelf(Kind::Movie, "m", 6)]);
    let mut screen = fixture.screen();
    let card = screen.key(screen.rows[0].elems[1]);
    let standard = place(&screen, &fixture, Some(card), card.elem).unwrap();
    assert!(!screen.shield);
    plx_ui::layout::with_shield(|| {
        let (_, out, _) = deliver(&mut screen, &fixture, Some(card), ScreenEvent::Tick(tick(100)));
        assert!(screen.shield, "the first event after the switch latches it");
        assert!(!reseated_on(&out, screen.key(FIELD)), "a card is a card in both layouts");
        let shield = place(&screen, &fixture, Some(card), card.elem).unwrap();
        assert!((shield.cx() - (979.0 + 120.0)).abs() < 1.0, "the card is the grid's second: {shield:?}");
        assert_eq!(screen.rows[0].elems[1], card.elem, "the same element");
        assert_eq!(screen.draft.query(), "live");
    });
    let b = key_of(&screen, 'b');
    let (_, out, _) = deliver(&mut screen, &fixture, Some(b), ScreenEvent::Tick(tick(101)));
    assert!(!screen.shield);
    assert!(reseated_on(&out, screen.key(FIELD)), "a key's focus goes to the field");
    assert_eq!(place(&screen, &fixture, Some(card), card.elem), Some(standard), "and the shelf is back where it was");
}

/// App-owned text on the Shield page fits its line in every shipped language, with the device's
/// own advances: the field's placeholder, the hint line in each of its app-written states, the
/// recents' heading and the Clear chip.
#[test]
fn the_shield_search_text_fits_in_every_language() {
    use plx_base::fontcov::advances::ShippedMeasure;
    use plx_machine::machine::Measure as _;
    use plx_platform::i18n::{language_on_this_thread_for_test, msg, Preference};
    let column = ss::RESULTS_RIGHT - ss::RESULTS_X;
    let mut over = Vec::new();
    for language in [Preference::En, Preference::Es, Preference::Be] {
        let _guard = language_on_this_thread_for_test(language);
        let tag = language.tag();
        for (what, text, room, size, bold) in [
            ("field", msg::browse_search_field(), ss::FIELD.w, ss::FIELD_SIZE, false),
            ("hint", msg::browse_search_hint(), ss::HINT.w, ss::HINT_SIZE, true),
            ("one more", msg::browse_search_one_more(), ss::HINT.w, ss::HINT_SIZE, true),
            ("failed", msg::browse_search_failed(), ss::HINT.w, ss::HINT_SIZE, true),
            ("recents heading", msg::browse_search_recent_heading(), column, plx_ui::shield::HEADING_SIZE, true),
            ("clear", msg::browse_search_clear(), column - 2.0 * plx_ui::shield_browse::CHIP_PAD,
                plx_ui::shield_browse::CHIP_SIZE, false),
        ] {
            let w = ShippedMeasure.width_str(text, size, bold);
            if w > room {
                over.push(format!("{tag} {what}: {w}px in {room}px"));
            }
        }
    }
    assert!(over.is_empty(), "Shield Search text that would not fit:\n  {}", over.join("\n  "));
}

/// The grid is all posters: an episode wears its show's poster (no still overlay), and its caption
/// names the show and the episode; the Standard shelf keeps its still, titled by the episode.
#[test]
fn an_episode_result_is_its_shows_poster_named_by_show_and_episode() {
    use plx_ui::cards::CardSource;
    use plx_ui::widgets::Art;
    let items = [Item::Media(plx_data::pms::PmsMovie { rk: "9".into(), title: "Pilot".into(), show_title: "Show".into(),
        kind: 3, season_index: 1, ep_index: 2, year: 2001, ..Default::default() })];
    let elems = [10];
    let cards = |shield| cards::RowCards { kind: Kind::Episode, elems: &elems, items: &items, sources: &[], shield };
    let shield = cards(true);
    assert!(matches!(CardSource::<HostFixture>::art(&shield, 0), Art::Poster(Some(_))));
    let label = CardSource::<HostFixture>::label(&shield, 0);
    assert_eq!(label.title.as_deref().map(|t| t.to_str().unwrap()), Some("Show"));
    assert_eq!(label.caption.as_deref().map(|c| c.to_string_lossy().into_owned()),
        Some(plx_platform::i18n::msg::widgets_card_season_episode(2, 1)));
    let standard = cards(false);
    assert!(matches!(CardSource::<HostFixture>::art(&standard, 0), Art::Still(Some(_))));
    assert_eq!(CardSource::<HostFixture>::label(&standard, 0).title.as_deref().map(|t| t.to_str().unwrap()), Some("Pilot"));
}
