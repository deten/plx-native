//! **The Detail page in the Shield layout** (`super::super::shield`): the measured geometry of the
//! movie, show and season pages, where focus goes along and between their rows, what each control
//! and More panel row performs, the Standard page left as it was, and a live switch.

use super::*;
use crate::registry::{DetailAction, ItemAction};
use plx_data::metadata::{Cast, Rating, RatingArt, Stream};
use plx_ui::shield_detail as sd;

type Effect = plx_machine::machine::Stamped<TestHost>;

fn movie() -> Detail {
    Detail {
        sid: ServerId::UNSET,
        rk: "m1".into(),
        kind: "movie".into(),
        title: "Planes".into(),
        year: 1987,
        summary: "word ".repeat(200),
        dur_ms: 92 * 60_000,
        resume_ms: 56 * 60_000,
        part: "/library/parts/1/file.mkv".into(),
        vcodec: "hevc".into(),
        video_resolution: "4k".into(),
        width: 3840,
        height: 2160,
        genres: vec!["Comedy".into(), "Drama".into()],
        rating: "R".into(),
        directors: vec!["John Hughes".into()],
        ratings: vec![Rating { art: RatingArt::Imdb, value: 7.6, critic: false }],
        audio: vec![Stream { lang: "English".into(), codec: "ac3".into(), layout: "5.1(side)".into(), selected: true,
            ..Default::default() }],
        cast: (0..8).map(|i| Cast { tag: format!("Actor {i}"), role: format!("Role {i}"), thumb: String::new(), id: i + 1,
            tag_key: String::new() }).collect(),
        ..Default::default()
    }
}

fn show() -> Detail {
    Detail {
        summary: "word ".repeat(200),
        ratings: vec![Rating { art: RatingArt::Imdb, value: 5.9, critic: false }],
        cast: (0..3).map(|i| Cast { tag: format!("Host {i}"), role: "Self".into(), thumb: String::new(), id: i + 1,
            tag_key: String::new() }).collect(),
        ..detail(ServerId::UNSET, "show")
    }
}

fn placed(screen: &DetailScreen, local: u32, focus: Option<u32>, at: At) -> Option<Rect> {
    let measure = plx_ui::fixture::FixtureMeasure;
    let key = screen.engine_key(local)?;
    let focus = focus.and_then(|f| screen.engine_key(f));
    let context = cx(&measure, focus);
    Focusable::<TestHost>::place(screen, &key, &context, at).map(|p| p.rect)
}

fn neighbour(screen: &DetailScreen, local: u32, dir: Dir) -> Step<u32> {
    let measure = plx_ui::fixture::FixtureMeasure;
    let key = screen.engine_key(local).unwrap();
    let context = cx(&measure, Some(key));
    Focusable::<TestHost>::neighbour(screen, FocusKey { entry: EntryId(7), elem: key }, dir, &context)
}

fn groups(screen: &DetailScreen, focus: Option<u32>) -> Vec<GroupSpec> {
    let measure = plx_ui::fixture::FixtureMeasure;
    let context = cx(&measure, focus.and_then(|f| screen.engine_key(f)));
    let mut out = Vec::new();
    Focusable::<TestHost>::groups(screen, &context, &mut out);
    out
}

fn panel_of(effects: &[Effect]) -> Option<ContentPanel> {
    effects.iter().find_map(|s| match &s.fx {
        Fx::App(AppFx::Content(ContentReq::Panel(panel))) => Some(*panel),
        _ => None,
    })
}

fn item_verb(effects: &[Effect]) -> Option<(ItemAction, bool)> {
    effects.iter().find_map(|s| match &s.fx {
        Fx::App(AppFx::ItemMenu(req)) => Some((req.act.clone(), req.loaded_episode)),
        _ => None,
    })
}

fn played_rk(effects: &[Effect]) -> Option<(String, i64)> {
    effects.iter().find_map(|s| match &s.fx {
        Fx::App(AppFx::Content(ContentReq::Play { play: PlayIntent::Item { rk, .. }, resume_ns })) => Some((rk.clone(), *resume_ns)),
        Fx::App(AppFx::Content(ContentReq::Play { play: PlayIntent::Movie(m), resume_ns })) => Some((m.rk.clone(), *resume_ns)),
        _ => None,
    })
}

/// §7.1, e01: a part-watched movie with ratings, a three-line summary and a director stands its
/// row at 564 — "Resume • 36 min left" in a 64-tall pill at 128, then 64 circles 96 apart (from
/// the start, the watched toggle, More) — and its media chips right-aligned to 1824, 22 under the
/// row's top. Focus off the row closes the pill to a circle and the circles up behind it.
#[test]
fn the_movie_row_is_the_measured_pill_circles_and_chips() {
    let _guard = install(movie());
    plx_ui::layout::with_shield(|| {
        let screen = bare(&_guard, ServerId::UNSET, "m1");
        let set = screen.hero_set(test_store().view());
        let (ctls, n) = hero::hero_ctls(set);
        assert_eq!(&ctls[..n], &[hero::HeroCtl::Play, hero::HeroCtl::Restart, hero::HeroCtl::MarkWatched,
            hero::HeroCtl::More, hero::HeroCtl::ChipVideo, hero::HeroCtl::ChipAudio, hero::HeroCtl::ChipSubs]);
        let play = placed(&screen, hero::ELEM_PLAY, Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        assert_eq!((play.x, play.y, play.h), (128.0, 564.0, 64.0));
        assert!(play.w > 64.0, "the focused row's pill shows its label");
        let label = screen.pill_label(test_store().view().current().unwrap());
        assert!(label.starts_with("Resume") && label.contains("36"), "{label}");
        let restart = placed(&screen, hero::ELEM_RESTART, Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        let watched = placed(&screen, hero::ELEM_MARK_WATCHED, Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        let more = placed(&screen, hero::ELEM_MORE, Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        assert_eq!(restart.x, play.x + play.w + 32.0);
        assert_eq!((watched.x - restart.x, more.x - watched.x), (96.0, 96.0));
        assert!([restart, watched, more].iter().all(|r| r.w == 64.0 && r.h == 64.0 && r.y == 564.0));
        let subs = placed(&screen, hero::ELEM_CHIP_SUBS, Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        let audio = placed(&screen, hero::ELEM_CHIP_AUDIO, Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        assert_eq!((subs.x + subs.w, subs.y, subs.h), (1824.0, 586.0, 32.0));
        assert_eq!(audio.x + audio.w + sd::CHIP_GAP, subs.x);
        let cast0 = cast::elem(0).unwrap();
        let shut = placed(&screen, hero::ELEM_PLAY, Some(cast0), At::SpringTarget).unwrap();
        let restart_shut = placed(&screen, hero::ELEM_RESTART, Some(cast0), At::SpringTarget).unwrap();
        assert_eq!((shut.x, shut.w), (128.0, 64.0), "off the row the pill is a circle");
        assert_eq!(restart_shut.x, 224.0, "and the circles close up behind it");
    });
}

/// §7.1, §5.4, e01/e03: Cast & Crew's heading at 676, its 240 circles from 740 on the 272 pitch;
/// focusing it brings its heading to 394 (the page moves up 282).
#[test]
fn cast_and_crew_circles_sit_on_the_measured_row_and_scroll_to_394() {
    let _guard = install(movie());
    plx_ui::layout::with_shield(|| {
        let mut screen = bare(&_guard, ServerId::UNSET, "m1");
        let plan = screen.plan(test_store().view(), &plx_ui::fixture::FixtureMeasure).unwrap();
        assert_eq!(plan.top(section::SectionId::Cast.raw()), Some(676.0));
        let c0 = placed(&screen, cast::elem(0).unwrap(), Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        let c1 = placed(&screen, cast::elem(1).unwrap(), Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        assert_eq!(c0, Rect::new(128.0, 740.0, 240.0, 240.0));
        assert_eq!(c1.x - c0.x, 272.0);
        let to = FocusKey { entry: EntryId(7), elem: cast::elem(0).unwrap() };
        step(&mut screen, &ScreenEvent::FocusMoved { from: None, to, by: By::Dir }, Some(cast::elem(0).unwrap()));
        assert_eq!(screen.scroll_target, 282.0);
        assert!(screen.shield_sections(test_store().view().current().unwrap())
            .iter().all(|s| *s != section::SectionId::About.raw()), "no About footer in Shield");
    });
}

/// "⋮" presents the More Actions panel with the page's own actions; each row the panel hands
/// back is performed by the path the page's control for it takes.
#[test]
fn more_opens_the_panel_and_its_rows_are_the_page_actions() {
    let _guard = install(movie());
    plx_ui::layout::with_shield(|| {
        let mut screen = bare(&_guard, ServerId::UNSET, "m1");
        let (_, fx) = step(&mut screen, &ScreenEvent::Activate(hero::ELEM_MORE), Some(hero::ELEM_MORE));
        let Some(ContentPanel::More { actions }) = panel_of(&fx) else { panic!("More presents its panel") };
        let rows: Vec<DetailAction> = actions.iter().collect();
        assert_eq!(rows, [DetailAction::TrackInfo, DetailAction::MarkWatched, DetailAction::About]);
        let (_, fx) = step(&mut screen, &ScreenEvent::App(AppMsg::DetailAction(DetailAction::TrackInfo)), None);
        assert_eq!(panel_of(&fx), Some(ContentPanel::Tracks { page: 1 }));
        let (_, fx) = step(&mut screen, &ScreenEvent::App(AppMsg::DetailAction(DetailAction::About)), None);
        assert_eq!(panel_of(&fx), Some(ContentPanel::About));
        let (_, fx) = step(&mut screen, &ScreenEvent::App(AppMsg::DetailAction(DetailAction::MarkWatched)), None);
        assert!(fx.iter().any(|s| matches!(&s.fx, Fx::App(AppFx::Store(StoreId::ViewState, _)))),
            "the watched row writes as the watched circle does");
        let (_, fx) = step(&mut screen, &ScreenEvent::App(AppMsg::DetailAction(DetailAction::PlayFromStart)), None);
        assert_eq!(played_rk(&fx), Some(("m1".to_string(), 0)), "from the start: no resume");
        let (_, fx) = step(&mut screen, &ScreenEvent::Activate(hero::ELEM_CHIP_AUDIO), Some(hero::ELEM_CHIP_AUDIO));
        assert_eq!(panel_of(&fx), Some(ContentPanel::Tracks { page: 1 }), "a media chip opens Track information");
    });
}

/// §7.2, d03: a show carries *Show | Season 1 | Season 2* at the top (the first pill at 135,40),
/// its row at 524 with Shuffle among the circles, and its seasons as posters under "N Seasons" at
/// 636, the posters from 700 on the 272 pitch.
#[test]
fn a_show_carries_tabs_and_its_seasons_as_posters() {
    let _guard = install(show());
    plx_ui::layout::with_shield(|| {
        let screen = bare(&_guard, ServerId::UNSET, "show");
        let show_tab = placed(&screen, shield::tab_elem(0).unwrap(), None, At::SpringTarget).unwrap();
        assert_eq!((show_tab.x, show_tab.y, show_tab.h), (135.0, 40.0, 48.0));
        let s2 = placed(&screen, shield::tab_elem(2).unwrap(), None, At::SpringTarget).unwrap();
        assert!(s2.x > show_tab.x + show_tab.w);
        let play = placed(&screen, hero::ELEM_PLAY, Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        assert_eq!(play.y, 524.0);
        let set = screen.hero_set(test_store().view());
        assert!(hero::index_of(set, hero::HeroCtl::Shuffle).is_some());
        let plan = screen.plan(test_store().view(), &plx_ui::fixture::FixtureMeasure).unwrap();
        assert_eq!(plan.top(section::SectionId::Season.raw()), Some(636.0));
        let p0 = placed(&screen, season::elem(0).unwrap(), Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        let p1 = placed(&screen, season::elem(1).unwrap(), Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
        assert_eq!(p0, Rect::new(128.0, 700.0, 240.0, 360.0));
        assert_eq!(p1.x - p0.x, 272.0);
        assert!(placed(&screen, episodes::elem(0, episodes::Row::Still).unwrap(), None, At::SpringTarget).is_none(),
            "the Show tab shows no episode strip");
    });
}

/// §7.3, d04/17: OK on a season's tab opens its season page — the store asked for that season
/// through the tabs' own dwell when it is not the one loaded — with the stills at y 570 on the
/// 544 pitch, the row at 898 under them and Cast & Crew at 978; OK on *Show* goes back, and a
/// season's poster opens its page as its tab does, focus following it onto the tab.
#[test]
fn a_season_tab_opens_its_season_page() {
    let _guard = install(show());
    plx_ui::layout::with_shield(|| {
        let mut screen = bare(&_guard, ServerId::UNSET, "show");
        let tab2 = shield::tab_elem(2).unwrap();
        step(&mut screen, &ScreenEvent::Activate(tab2), Some(tab2));
        assert!(screen.shield.season_view);
        assert_eq!((screen.pending_season, screen.season_settle), (Some(1), season::SETTLE_S),
            "the season not loaded is asked for at once");
        step(&mut screen, &ScreenEvent::Activate(shield::tab_elem(1).unwrap()), Some(shield::tab_elem(1).unwrap()));
        let still0 = episodes::elem(0, episodes::Row::Still).unwrap();
        let s0 = placed(&screen, still0, None, At::SpringTarget).unwrap();
        let s1 = placed(&screen, episodes::elem(1, episodes::Row::Still).unwrap(), None, At::SpringTarget).unwrap();
        assert_eq!(s0, Rect::new(128.0, 570.0, 512.0, 288.0));
        assert_eq!(s1.x - s0.x, 544.0);
        assert!(placed(&screen, episodes::elem(0, episodes::Row::Text).unwrap(), None, At::SpringTarget).is_none(),
            "the Shield strip has no text blocks");
        let play = placed(&screen, hero::ELEM_PLAY, Some(still0), At::SpringTarget).unwrap();
        assert_eq!((play.x, play.y, play.w), (128.0, 898.0, 64.0));
        let plan = screen.plan(test_store().view(), &plx_ui::fixture::FixtureMeasure).unwrap();
        assert_eq!(plan.top(section::SectionId::Cast.raw()), Some(978.0));
        assert!(plan.top(section::SectionId::Season.raw()).is_none(), "the season page shows no posters");
        let focused = placed(&screen, still0, Some(still0), At::SpringTarget).unwrap();
        assert!((focused.w - 528.0).abs() < 0.01, "the focused still pops to ~528 across");
        step(&mut screen, &ScreenEvent::Activate(shield::tab_elem(0).unwrap()), Some(shield::tab_elem(0).unwrap()));
        assert!(!screen.shield.season_view, "Show puts the show page back");
        let poster = season::elem(0).unwrap();
        let poster_key = screen.engine_key(poster).unwrap();
        step(&mut screen, &ScreenEvent::Activate(poster_key), Some(poster));
        assert!(screen.shield.season_view, "a season's poster opens its page");
        let measure = plx_ui::fixture::FixtureMeasure;
        let want = FocusKey { entry: EntryId(7), elem: screen.engine_key(poster).unwrap() };
        let got = Focusable::<TestHost>::reconcile(&screen, want, &cx(&measure, Some(want.elem)));
        assert_eq!(got.elem, shield::tab_elem(1).unwrap(), "focus follows the poster onto its season's tab");
    });
}

/// §7.3, 17: the season page describes, and its row acts on, the episode focus last rested on —
/// its Play plays that episode, its watched circle marks that episode through the filmstrip
/// menu's own request, its Shuffle shuffles the season.
#[test]
fn the_season_page_acts_on_the_episode_focus_rested_on() {
    let _guard = install(show());
    plx_ui::layout::with_shield(|| {
        let mut screen = bare(&_guard, ServerId::UNSET, "show");
        step(&mut screen, &ScreenEvent::Activate(shield::tab_elem(1).unwrap()), Some(shield::tab_elem(1).unwrap()));
        let still1 = episodes::elem(1, episodes::Row::Still).unwrap();
        let to = FocusKey { entry: EntryId(7), elem: screen.engine_key(still1).unwrap() };
        step(&mut screen, &ScreenEvent::FocusMoved { from: None, to, by: By::Dir }, Some(still1));
        assert_eq!(screen.shield.subject.as_deref(), Some("e2"));
        let (_, fx) = step(&mut screen, &ScreenEvent::Activate(hero::ELEM_PLAY), Some(hero::ELEM_PLAY));
        assert_eq!(played_rk(&fx).map(|(rk, _)| rk), Some("e2".to_string()));
        let (_, fx) = step(&mut screen, &ScreenEvent::Activate(hero::ELEM_MARK_WATCHED), Some(hero::ELEM_MARK_WATCHED));
        assert!(matches!(item_verb(&fx), Some((ItemAction::MarkWatched(rk), true)) if rk == "e2"));
        let (_, fx) = step(&mut screen, &ScreenEvent::Activate(hero::ELEM_SHUFFLE), Some(hero::ELEM_SHUFFLE));
        assert!(matches!(item_verb(&fx), Some((ItemAction::Shuffle(rk), false)) if rk == "season-1"));
        let (_, fx) = step(&mut screen, &ScreenEvent::Activate(hero::ELEM_MORE), Some(hero::ELEM_MORE));
        let Some(ContentPanel::More { actions }) = panel_of(&fx) else { panic!("More presents its panel") };
        assert!(actions.contains(DetailAction::GoToEpisode) && actions.contains(DetailAction::ShuffleSeason));
        let (_, fx) = step(&mut screen, &ScreenEvent::App(AppMsg::DetailAction(DetailAction::GoToEpisode)), None);
        assert!(fx.iter().any(|s| matches!(&s.fx, Fx::App(AppFx::Content(ContentReq::Push(ContentArg::Detail { rk, .. }))) if rk == "e2")));
    });
}

/// §2.3: LEFT off the first element of every row is an edge the group answers with `Stop` — the
/// side panel's door (`Rig::edge_surface`) — and RIGHT and LEFT walk the tabs and the strip.
#[test]
fn left_off_every_rows_first_element_is_the_side_panels_door() {
    let _guard = install(show());
    plx_ui::layout::with_shield(|| {
        let mut screen = bare(&_guard, ServerId::UNSET, "show");
        assert!(matches!(neighbour(&screen, hero::ELEM_PLAY, Dir::Left), Step::Edge));
        assert!(matches!(neighbour(&screen, shield::tab_elem(0).unwrap(), Dir::Left), Step::Edge));
        assert!(matches!(neighbour(&screen, shield::tab_elem(0).unwrap(), Dir::Right),
            Step::Move(k) if k.elem == shield::tab_elem(1).unwrap()));
        let specs = groups(&screen, Some(hero::ELEM_PLAY));
        assert!(specs.iter().all(|g| g.edge[2] == EdgeRule::Stop), "every group stops at its left edge");
        assert!(specs.iter().any(|g| g.id == shield::TABS_GROUP));
        step(&mut screen, &ScreenEvent::Activate(shield::tab_elem(1).unwrap()), Some(shield::tab_elem(1).unwrap()));
        let still0 = episodes::elem(0, episodes::Row::Still).unwrap();
        assert!(matches!(neighbour(&screen, still0, Dir::Left), Step::Edge));
        assert!(matches!(neighbour(&screen, still0, Dir::Down), Step::Edge), "DOWN leaves by the group's edge");
        assert!(matches!(neighbour(&screen, still0, Dir::Right),
            Step::Move(k) if Some(k.elem) == screen.engine_key(episodes::elem(1, episodes::Row::Still).unwrap())));
        assert!(groups(&screen, Some(still0)).iter().any(|g| g.id == episodes::EPISODES_GROUP
            && matches!(g.kind, GroupKind::Row { .. })));
    });
}

/// The page draws in Shield without a GL context, and registers its row, its tabs and its cards.
#[test]
fn the_shield_page_draws_and_registers_its_controls() {
    let _guard = install(show());
    plx_ui::layout::with_shield(|| {
        let mut screen = bare(&_guard, ServerId::UNSET, "show");
        let measure = plx_ui::fixture::FixtureMeasure;
        let context = cx(&measure, Some(hero::ELEM_PLAY));
        let stops = drawn_stops(&mut screen, &context);
        let keys: Vec<u32> = stops.iter().map(|s| s.key.elem).collect();
        for elem in [hero::ELEM_PLAY, hero::ELEM_MORE, shield::tab_elem(0).unwrap(), shield::tab_elem(2).unwrap(),
            screen.engine_key(season::elem(0).unwrap()).unwrap()] {
            assert!(keys.contains(&elem), "{elem} has a stop: {keys:?}");
        }
        assert!(!keys.contains(&about::CARD_ELEM), "no About footer");
    });
}

/// **Standard is unchanged**: the same item in the Standard layout has its own row (Play at the
/// 96 margin, no More, no chips), no tabs, the About footer, and no Shield bytes in its hash.
#[test]
fn the_standard_page_keeps_its_own_row_and_hash() {
    let _guard = install(movie());
    let screen = bare(&_guard, ServerId::UNSET, "m1");
    let set = screen.hero_set(test_store().view());
    assert!(!set.shield && set.chips == 0);
    assert!(hero::index_of(set, hero::HeroCtl::More).is_none());
    let play = placed(&screen, hero::ELEM_PLAY, Some(hero::ELEM_PLAY), At::SpringTarget).unwrap();
    assert_eq!(play.x, plx_ui::consts::MARGIN_X);
    assert!(placed(&screen, shield::tab_elem(0).unwrap(), None, At::SpringTarget).is_none());
    assert!(groups(&screen, None).iter().any(|g| g.id == about::ABOUT_GROUP));
    assert!(*screen.cast.style() == plx_ui::cards::RowStyle::CAST);
    let standard = screen.hash();
    let mut probe = bare(&_guard, ServerId::UNSET, "m1");
    probe.shield.season_view = true;
    assert_eq!(probe.hash(), standard, "Standard hashes no Shield state");
}

/// **A live switch** re-dresses the mounted page: its shelves take the layout's styles, a focus the
/// new layout cannot hold (the About footer) is reconciled onto Play, and back again.
#[test]
fn a_live_switch_redresses_the_mounted_page() {
    let _guard = install(movie());
    let mut screen = bare(&_guard, ServerId::UNSET, "m1");
    let measure = plx_ui::fixture::FixtureMeasure;
    plx_ui::layout::with_shield(|| {
        step(&mut screen, &ScreenEvent::Tick(plx_machine::machine::Tick { ms: 16, dt_us: 16_667 }), None);
        assert!(screen.shield.on);
        assert!(*screen.cast.style() == plx_ui::cards::RowStyle::SHIELD_CAST);
        assert!(*screen.related.style() == plx_ui::cards::RowStyle::SHIELD_DETAIL_POSTER);
        let about = FocusKey { entry: EntryId(7), elem: about::CARD_ELEM };
        let got = Focusable::<TestHost>::reconcile(&screen, about, &cx(&measure, Some(about.elem)));
        assert_eq!(got.elem, hero::ELEM_PLAY, "the About footer is not on the Shield page");
        let more = FocusKey { entry: EntryId(7), elem: hero::ELEM_MORE };
        assert_eq!(Focusable::<TestHost>::reconcile(&screen, more, &cx(&measure, Some(more.elem))).elem, hero::ELEM_MORE);
    });
    step(&mut screen, &ScreenEvent::Tick(plx_machine::machine::Tick { ms: 32, dt_us: 16_667 }), None);
    assert!(!screen.shield.on);
    assert!(*screen.cast.style() == plx_ui::cards::RowStyle::CAST);
    let more = FocusKey { entry: EntryId(7), elem: hero::ELEM_MORE };
    assert_eq!(Focusable::<TestHost>::reconcile(&screen, more, &cx(&measure, Some(more.elem))).elem, hero::ELEM_PLAY,
        "More is the Shield row's: back in Standard focus lands on Play");
}
