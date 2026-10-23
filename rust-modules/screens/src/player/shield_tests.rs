//! **The player in the Shield layout** (`player::shield`): the key and pointer routes over the
//! Shield transport, the Skip pill's 15 s, the post-play page's focus, cards and its mapping of each
//! Next episode mode at the end of the stream, and the Standard transport left as it was.

use super::input::HudNav;
use super::overlay::OverlayKind;
use super::shield::{self, eos_hold, EosHold, FOCUS_DECK};
use super::PlayerScreen;
use crate::player::overlay_tests::TestHost;
use crate::registry::{AppFx, PlayerReq, PostPlayReq};
use plx_appkit::player_hud::{self, ControlSlot, ELEM_ROW_BASE, ELEM_SCRUB};
use plx_data::metadata::{Marker, MarkerKind};
use plx_machine::machine::{
    Cx, Edge, Effects, EntryId, Fx, Handled, InputEvent, InputKind, InputOwner, InstanceId, Machine, MachineId,
    Source, Tick,
};
use plx_media::route::NextEpisodeMode;
use plx_ui::consts::{SDLK_DOWN, SDLK_LEFT, SDLK_RETURN, SDLK_RIGHT, SDLK_UP};
use plx_ui::fixture::FixtureMeasure;
use plx_ui::screen::{At, Focusable, ScreenEvent};

const ENTRY: EntryId = EntryId(9);
const INST: InstanceId = InstanceId(3);

fn cx() -> Cx<'static, TestHost> {
    Cx {
        views: (),
        tick: Tick::default(),
        measure: &FixtureMeasure,
        focus: Default::default(),
        press: Default::default(),
        owner: InputOwner::Entry(ENTRY),
    }
}

fn run(page: &mut PlayerScreen, kind: InputKind<u32>, ms: u32) -> (Handled, Vec<PlayerReq>) {
    let mut out = Vec::new();
    let mut present = plx_machine::present::Present::new();
    let ev = ScreenEvent::<TestHost>::Input(InputEvent { kind, at: Tick { ms, dt_us: 0 }, source: Source::Sdl });
    let handled = page.step(&ev, &cx(), &mut Effects::new(&mut out, MachineId::Instance(INST), &mut present));
    let reqs = out
        .into_iter()
        .filter_map(|e| match e.fx {
            Fx::App(AppFx::Player(req)) => Some(req),
            _ => None,
        })
        .collect();
    (handled, reqs)
}

fn key(page: &mut PlayerScreen, sym: u32) -> (Handled, Vec<PlayerReq>) {
    let k = InputKind::Key { key: plx_machine::machine::Key::Other, sym, wcode: 0, edge: Edge::Down, at_edge: false };
    run(page, k, 1_000)
}

fn click(page: &mut PlayerScreen, elem: u32) -> (Handled, Vec<PlayerReq>) {
    run(page, InputKind::Click { x: 0.0, y: 0.0, hit: Some(elem) }, 1_000)
}

/// A Shield page with the transport up and focus where `nav` says.
fn shield_page(nav: HudNav) -> PlayerScreen {
    let mut page = PlayerScreen::new(ENTRY);
    page.hud.nav = nav;
    page.hud.visible_at_press = true;
    page
}

fn marker(kind: MarkerKind) -> Marker {
    Marker { kind, start_ms: 1_000, end_ms: 2_000, final_seg: kind == MarkerKind::Credits }
}

/// **Each Next episode mode at the end of the stream.** Standard and Off are today's end; a held
/// page holds; a page still counting down advances at once (today); a page whose countdown the
/// viewer cancelled holds; After credits opens the page at the end; Countdown with no page (no
/// marker ever offered) is today's end; no successor is today's end.
#[test]
fn the_end_of_the_stream_maps_each_next_episode_mode() {
    use NextEpisodeMode::{AfterCredits, Countdown, Off};
    // (shield, mode, has_next, shown, held, armed) -> what the end does
    let cases = [
        (false, Countdown, true, true, false, true, EosHold::Finish),
        (false, AfterCredits, true, false, false, false, EosHold::Finish),
        (true, Off, true, false, false, false, EosHold::Finish),
        (true, Countdown, false, false, false, false, EosHold::Finish),
        (true, Countdown, true, true, true, true, EosHold::Hold),
        (true, Countdown, true, true, false, true, EosHold::Finish),
        (true, Countdown, true, true, false, false, EosHold::Open),
        (true, Countdown, true, false, false, false, EosHold::Finish),
        (true, AfterCredits, true, false, false, false, EosHold::Open),
        (true, AfterCredits, true, false, true, true, EosHold::Hold),
    ];
    for (shield, mode, next, shown, held, armed, want) in cases {
        assert_eq!(eos_hold(shield, mode, next, shown, held, armed), want,
            "shield={shield} {mode:?} next={next} shown={shown} held={held} armed={armed}");
    }
}

/// i04/h03: UP from the bar reaches the round buttons, LEFT/RIGHT walk the two of them and OK opens
/// the Subtitles panel (the track menu on its Subtitles tab) or the Settings panel — at once, with
/// no control-row press dip. OK on the bar toggles play and pause as ever.
#[test]
fn the_round_buttons_open_subtitles_and_settings() {
    plx_ui::layout::with_shield(|| {
        let mut page = shield_page(HudNav::HOME);
        let (_, reqs) = key(&mut page, SDLK_RETURN);
        assert_eq!(reqs, vec![PlayerReq::Transport(None)], "OK on the bar toggles");
        page.hud.visible_at_press = true;
        key(&mut page, SDLK_UP);
        assert_eq!(page.hud.nav.focus, 1, "UP from the bar reaches the buttons");
        assert_eq!(page.hud.nav.btn, 0, "Subtitles first");
        page.hud.visible_at_press = true;
        let (_, reqs) = key(&mut page, SDLK_RETURN);
        assert_eq!(reqs, vec![PlayerReq::OpenOverlay(OverlayKind::Tracks { tab: 1 })]);
        page.hud.visible_at_press = true;
        key(&mut page, SDLK_RIGHT);
        key(&mut page, SDLK_RIGHT);
        assert_eq!(page.hud.nav.btn, 1, "two buttons, whatever occupies the Standard row");
        page.hud.visible_at_press = true;
        let (_, reqs) = key(&mut page, SDLK_RETURN);
        assert_eq!(reqs, vec![PlayerReq::OpenOverlay(OverlayKind::Settings)]);
        // a Skip segment under the playhead changes nothing about the row
        page.slot = player_hud::slot_for(Some(marker(MarkerKind::Intro)), false, NextEpisodeMode::Countdown);
        page.hud.visible_at_press = true;
        key(&mut page, SDLK_LEFT);
        assert_eq!(page.hud.nav.btn, 0);
        // the pointer: a click on a button opens its panel, never the Standard press
        let (handled, reqs) = click(&mut page, ELEM_ROW_BASE + 1);
        assert_eq!((handled, reqs), (Handled::Yes, vec![PlayerReq::OpenOverlay(OverlayKind::Settings)]));
    });
}

/// **Standard is unchanged**: the same OK on the control row arms the deferred press, and LEFT/RIGHT
/// walk the three discs.
#[test]
fn the_standard_row_keeps_its_press_and_its_three_discs() {
    let mut page = shield_page(HudNav { focus: 1, btn: 0, tab: 0 });
    assert!(!page.shield());
    let (_, reqs) = key(&mut page, SDLK_RETURN);
    assert_eq!(reqs, vec![PlayerReq::ArmControlRow]);
    for _ in 0..4 {
        page.hud.visible_at_press = true;
        key(&mut page, SDLK_RIGHT);
    }
    assert_eq!(page.hud.nav.btn, 2, "the Standard row's three discs");
    page.raise_for_offer(1_000, player_hud::slot_for(Some(marker(MarkerKind::Intro)), false, NextEpisodeMode::Countdown));
    assert_eq!(page.hud.nav.focus, 1, "Standard: an offer still raises the transport onto the row");
}

/// w016/e001: the Skip pill stands alone for 15 s after its segment is offered — no transport
/// raised — and OK on the bare picture then skips through the control row's own press; past the
/// 15 s OK is play/pause again. A new segment (the final credits) is a new 15 s.
#[test]
fn the_skip_pill_stands_alone_for_fifteen_seconds() {
    plx_ui::layout::with_shield(|| {
        let _g = plx_base::testlock::serial();
        let mut page = shield_page(HudNav::HOME);
        page.hud.visible_at_press = false;
        let intro = player_hud::slot_for(Some(marker(MarkerKind::Intro)), false, NextEpisodeMode::Countdown);
        page.slot = intro;
        page.raise_for_offer(1_000, intro);
        assert_eq!(page.hud.until, 0, "Shield: the offer raises no transport");
        let tick = |page: &mut PlayerScreen, ms: u32| {
            let mut out = Vec::new();
            let mut present = plx_machine::present::Present::new();
            let ev = ScreenEvent::<TestHost>::Tick(Tick { ms, dt_us: 16_000 });
            page.step(&ev, &cx(), &mut Effects::new(&mut out, MachineId::Instance(INST), &mut present));
        };
        tick(&mut page, 1_000);
        assert!(page.skip_in_window(1_000) && page.skip_in_window(15_999));
        assert!(!page.skip_in_window(16_000), "gone after 15 s");
        page.hud.visible_at_press = false;
        let (_, reqs) = key(&mut page, SDLK_RETURN);
        assert_eq!(reqs, vec![PlayerReq::ArmControlRow], "OK on the pill skips through the row's press");
        // a later segment re-opens the window
        let credits = player_hud::slot_for(Some(Marker { start_ms: 50_000, ..marker(MarkerKind::Credits) }), false,
            NextEpisodeMode::Countdown);
        page.slot = credits;
        tick(&mut page, 40_000);
        assert!(page.skip_in_window(40_000), "the final credits marker is a new offer");
        assert!(matches!(page.slot, ControlSlot::Skip(_)));
    });
}

/// e022: the page's two leading cards walk with LEFT/RIGHT, DOWN reaches the deck when it has
/// cards, and OK asks for what the focused card names (Up Next, Just Played). Focus on Up Next is
/// the Standard countdown's "may run" seat, and leaving it is its cancel.
#[test]
fn the_post_play_cards_ask_for_their_starts() {
    plx_ui::layout::with_shield(|| {
        let mut page = shield_page(HudNav { focus: 1, btn: plx_appkit::up_next::BTN_NEXT, tab: 0 });
        page.post.shown = true;
        assert!(plx_appkit::up_next::countdown_may_run(true, page.hud.nav.focus == 1, page.hud.nav.btn),
            "the Up Next card holds the countdown's seat");
        let (_, reqs) = key(&mut page, SDLK_RETURN);
        assert_eq!(reqs, vec![PlayerReq::PostPlay(PostPlayReq::PlayNext)]);
        key(&mut page, SDLK_LEFT);
        assert_eq!(page.post_focus_key().elem, shield::ELEM_POST_JUST);
        assert!(!plx_appkit::up_next::countdown_may_run(true, page.hud.nav.focus == 1, page.hud.nav.btn),
            "moving off Up Next cancels the countdown (the Standard rule)");
        let (_, reqs) = key(&mut page, SDLK_RETURN);
        assert_eq!(reqs, vec![PlayerReq::PostPlay(PostPlayReq::Replay)]);
        key(&mut page, SDLK_DOWN);
        assert_eq!(page.hud.nav.focus, 1, "no deck, nowhere to go down");
        // with a deck: DOWN, RIGHT, OK plays that card through the Continue Watching start
        page.post.model = Some(plx_appkit::shield_hud::PostPlayModel {
            deck: (0..3).map(|i| plx_appkit::shield_hud::PostCard { sid: 2, rk: format!("{}", 100 + i), ..Default::default() })
                .collect(),
            ..Default::default()
        });
        key(&mut page, SDLK_DOWN);
        assert_eq!(page.hud.nav.focus, FOCUS_DECK);
        key(&mut page, SDLK_RIGHT);
        let (_, reqs) = key(&mut page, SDLK_RETURN);
        assert_eq!(reqs, vec![PlayerReq::PostPlay(PostPlayReq::Play {
            sid: plx_plex::plex::ServerId::from_raw(2), rk: "101".into() })]);
        key(&mut page, SDLK_UP);
        assert_eq!((page.hud.nav.focus, page.hud.nav.btn), (1, plx_appkit::up_next::BTN_CREDITS));
        // the pointer reaches every card where it is drawn
        for elem in [shield::ELEM_POST_JUST, shield::ELEM_POST_NEXT, shield::ELEM_DECK_BASE + 2] {
            let placed = Focusable::<TestHost>::place(&page, &elem, &cx(), At::Drawn).expect("placed");
            assert!(placed.rect.w > 300.0, "{elem}: {placed:?}");
            let (handled, reqs) = click(&mut page, elem);
            assert_eq!(handled, Handled::Yes);
            assert!(matches!(reqs.as_slice(), [PlayerReq::PostPlay(_)]), "{elem}: {reqs:?}");
        }
    });
}

/// The Shield transport's stops stand where it is drawn: the round buttons at 1708/1784, the bar's
/// band over the bar line, the chevron for Info.
#[test]
fn the_shield_transport_places_its_controls_where_it_draws_them() {
    plx_ui::layout::with_shield(|| {
        let page = shield_page(HudNav::HOME);
        let place = |elem: u32| Focusable::<TestHost>::place(&page, &elem, &cx(), At::Drawn).map(|p| p.rect);
        assert_eq!(place(ELEM_ROW_BASE), Some(plx_ui::shield_player::button_rect(0)));
        assert_eq!(place(ELEM_ROW_BASE + 1), Some(plx_ui::shield_player::button_rect(1)));
        assert_eq!(place(ELEM_ROW_BASE + 2), None, "two buttons");
        let band = place(ELEM_SCRUB).expect("the bar");
        assert!(band.y < plx_ui::shield_player::BAR_CY && band.y + band.h > plx_ui::shield_player::BAR_CY);
        let chevron = place(player_hud::ELEM_TAB_BASE).expect("the chevron stands for Info");
        let c = plx_ui::shield_player::chevron_rect();
        assert!(chevron.x <= c.x && chevron.y <= c.y && chevron.x + chevron.w >= c.x + c.w && chevron.y + chevron.h >= c.y + c.h);
    });
}
