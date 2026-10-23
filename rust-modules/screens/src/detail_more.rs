//! **The Shield layout's More Actions panel** — a registered `Style::Sheet` surface on the shared
//! `ModalStack`, presented over a Detail page by its "⋮" circle while `plx_ui::layout::shield()` is
//! on (SHIELD-LAYOUT-SPEC §6.4, e02). It stands at the right-side panel's fixed frame
//! (`plx_ui::shield_panel`) over the sheet's 50% dim: a title, then one pill row per action with a
//! leading glyph on the 76 pitch, the focused row a white plate.
//!
//! **The rows are the page's actions, never new ones.** The page decides which it offers
//! ([`DetailActions`], on the argument) — *Track information*, *Also available*, going to the show
//! or the described episode, playing from the start or the trailer, Shuffle, the watched and
//! Watchlist toggles, *About* — and OK on a row dismisses the panel and delivers
//! [`AppMsg::DetailAction`] to that page's instance, which performs it by the same path its own
//! control takes (`AltSourceOpen`'s shape: the surface names, the page does). BACK, and LEFT, only
//! dismiss, so focus returns to the "⋮" it was opened from.
//!
//! It belongs to the Shield layout alone: a live switch to Standard while it is up dismisses it.

use std::borrow::Cow;

use crate::registry::{AppLike, AppMsg, DetailAction, DetailMoreArg};
use plx_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputKind, Key,
    LogicalState, Machine, MachineId, NavOp,
};
use plx_ui::frame::Budget;
use plx_ui::icons::Icon;
use plx_ui::screen::{
    Activate, At, AxisMask, Dir, DrawFrame, EdgeRule, ElemKind, Focusable, GroupKind, GroupSpec,
    Hover, Placed, RenderStrategy, Screen, ScreenEvent, Scrim, Seat, Step, Stop,
};
use plx_ui::shield_panel::{self, Line};
use plx_ui::table::{Row, Section};

pub const SHAPE: &str = "DetailMore{rows:[u32],scroll:f32}";

/// A row's words and glyph.
pub fn row_of(action: DetailAction) -> Row {
    use plx_platform::i18n::msg;
    let (label, glyph) = match action {
        DetailAction::TrackInfo => (msg::browse_detail_track_info(), Icon::Audio),
        DetailAction::AlsoAvailable => (msg::browse_detail_also_available(), Icon::Collection),
        DetailAction::GoToShow => (msg::browse_menu_go_show(), Icon::Show),
        DetailAction::GoToEpisode => (msg::browse_menu_go_episode(), Icon::Episode),
        DetailAction::PlayFromStart => (plx_ui::widgets::play_from_start_verb(), Icon::PlayStart),
        DetailAction::PlayTrailer => (plx_ui::widgets::play_trailer_verb(), Icon::Trailer),
        DetailAction::ShuffleShow => (msg::browse_menu_shuffle_show(), Icon::Shuffle),
        DetailAction::ShuffleSeason => (msg::browse_menu_shuffle_season(), Icon::Shuffle),
        DetailAction::MarkWatched => (plx_ui::widgets::mark_watched_verb(), Icon::CheckCircleFill),
        DetailAction::MarkUnwatched => (plx_ui::widgets::mark_unwatched_verb(), Icon::MinusCircleFill),
        DetailAction::WatchlistAdd => (msg::browse_watchlist_add(), Icon::Bookmark),
        DetailAction::WatchlistRemove => (msg::browse_watchlist_remove(), Icon::BookmarkFill),
        DetailAction::About => (msg::browse_detail_about(), Icon::Info),
    };
    Row::new(label).licon(glyph)
}

/// A row's focus key: its action's place in `DetailAction::ALL`, plus one — an identity, never a
/// position in this opening's list.
fn key_of(action: DetailAction) -> u32 {
    DetailAction::ALL.iter().position(|a| *a == action).map_or(0, |i| i as u32 + 1)
}

pub struct DetailMoreScreen {
    entry: EntryId,
    arg: DetailMoreArg,
    rows: Vec<DetailAction>,
    /// The one section the panel's rows are laid out and painted from.
    sections: Vec<Section>,
    lines: Vec<Line>,
    /// The list's scroll: how far it has moved up to keep focus inside the frame.
    scroll: f32,
}

impl DetailMoreScreen {
    pub fn new(entry: EntryId, arg: DetailMoreArg) -> Self {
        let rows: Vec<DetailAction> = arg.actions.iter().collect();
        let mut section = Section::new(String::new());
        for &action in &rows {
            section = section.row(row_of(action));
        }
        let sections = vec![section];
        let lines = shield_panel::lay_out(&sections, Self::title());
        Self { entry, arg, rows, sections, lines, scroll: 0.0 }
    }

    fn title() -> &'static str {
        plx_platform::i18n::msg::browse_detail_more_actions()
    }

    fn index_of(&self, elem: u32) -> Option<usize> {
        self.rows.iter().position(|a| key_of(*a) == elem)
    }

    fn row_rect(&self, i: usize) -> Option<plx_ui::Rect> {
        shield_panel::row_rect(&self.lines, i, self.scroll)
    }

    /// Keep the focused row inside the frame.
    fn reveal(&mut self, elem: u32) {
        if let Some((top, h)) = self.index_of(elem).and_then(|i| shield_panel::row_span(&self.lines, i)) {
            self.scroll = shield_panel::scroll_for(top, h, self.scroll);
        }
    }

    /// OK on a row: dismiss, and hand the row to the page that opened the panel.
    fn activate<H: AppLike>(&mut self, elem: u32, fx: &mut Effects<'_, H>) {
        fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));
        if let Some(&action) = self.index_of(elem).and_then(|i| self.rows.get(i)) {
            fx.push(Fx::Deliver(MachineId::Instance(self.arg.host),
                Delivery::Screen(ScreenEvent::App(AppMsg::DetailAction(action)))));
        }
    }

    /// The actions the panel offers, in order (a test's and a probe's read).
    pub fn actions(&self) -> &[DetailAction] {
        &self.rows
    }

    /// The focus key of the row that performs `action`, for a test or a scripted walk.
    pub fn key_for(&self, action: DetailAction) -> Option<u32> {
        self.rows.contains(&action).then(|| key_of(action))
    }

    /// Each row's plate at the current scroll, for a geometry test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn row_rects(&self) -> Vec<plx_ui::Rect> {
        (0..self.rows.len()).filter_map(|i| self.row_rect(i)).collect()
    }
}

impl<H: AppLike> Machine<H> for DetailMoreScreen {
    type Ev = ScreenEvent<H>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        if !plx_ui::layout::shield() {
            // the Shield layout's panel: a switch to Standard while it is up closes it
            fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));
            return Handled::Yes;
        }
        match ev {
            ScreenEvent::Tick(_) => {
                if let Some(key) = cx.focus.current.filter(|key| key.entry == self.entry) {
                    self.reveal(key.elem);
                }
            }
            ScreenEvent::FocusMoved { to, .. } => {
                self.reveal(to.elem);
                fx.invalidate(plx_machine::present::Provenance::Input);
            }
            ScreenEvent::Activate(elem) => self.activate(*elem, fx),
            ScreenEvent::PressCommit(_) => {
                if let Some(key) = cx.focus.current.filter(|key| key.entry == self.entry) {
                    self.activate(key.elem, fx);
                }
            }
            ScreenEvent::Input(input) => {
                if let InputKind::Key { key: Key::Back | Key::Left, edge: Edge::Down, .. } = input.kind {
                    fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));
                    return Handled::Yes;
                }
            }
            _ => {}
        }
        Handled::No
    }
}

impl<H: AppLike> Focusable<H> for DetailMoreScreen {
    fn groups(&self, _cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        out.push(GroupSpec {
            id: GroupId(0),
            kind: GroupKind::Column,
            seat: Seat::First,
            reachable: AxisMask::BOTH,
            edge: [EdgeRule::Stop; 4],
            extent: shield_panel::PANEL,
            len: self.rows.len(),
            elem: ElemKind::Bare,
        });
    }
    fn group_of(&self, elem: &u32, _: &Cx<'_, H>) -> Option<GroupId> {
        self.index_of(*elem).map(|_| GroupId(0))
    }
    /// UP and DOWN walk the rows; LEFT and RIGHT never move (LEFT closes, in `step`).
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, _: &Cx<'_, H>) -> Step<u32> {
        let Some(i) = self.index_of(key.elem) else { return Step::Edge };
        let next = match dir {
            Dir::Up => i.checked_sub(1),
            Dir::Down => (i + 1 < self.rows.len()).then_some(i + 1),
            Dir::Left | Dir::Right => None,
        };
        next.map_or(Step::Edge, |j| Step::Move(FocusKey { entry: self.entry, elem: key_of(self.rows[j]) }))
    }
    fn place(&self, elem: &u32, _: &Cx<'_, H>, _: At) -> Option<Placed> {
        let i = self.index_of(*elem)?;
        let rect = self.row_rect(i)?;
        Some(Placed { rect, rest_rect: rect, clip: shield_panel::list_clip(), index: Some(i as u32) })
    }
    fn reconcile(&self, want: FocusKey<u32>, _: &Cx<'_, H>) -> FocusKey<u32> {
        match (self.index_of(want.elem), self.rows.first()) {
            (Some(_), _) | (None, None) => want,
            (None, Some(&first)) => FocusKey { entry: self.entry, elem: key_of(first) },
        }
    }
    /// The panel opens on its first row (e02: Subtitles, the file's own row).
    fn seat(&self, _: GroupId, _: Placed, _: &Cx<'_, H>) -> FocusKey<u32> {
        FocusKey { entry: self.entry, elem: self.rows.first().map_or(0, |a| key_of(*a)) }
    }
}

impl<H: AppLike> Screen<H> for DetailMoreScreen {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn name(&self) -> &'static str {
        crate::registry::word::DETAIL_MORE
    }
    fn state(&self) -> &dyn LogicalState {
        self
    }
    fn crumb(&self, _: &Cx<'_, H>) -> Option<Cow<'_, str>> {
        None
    }
    /// The page behind is dimmed to half (spec §1.7); the panel itself is opaque.
    fn scrim(&self) -> Scrim {
        Scrim::dim(plx_ui::theme::underlay::DIM_SHEET)
    }
    fn prepare(&mut self, _: &mut Budget, _: &Cx<'_, H>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        let p = f.painter.alpha(f.page_alpha);
        let focused = f.cx.focus.current.filter(|key| key.entry == self.entry).and_then(|key| self.index_of(key.elem));
        shield_panel::draw(p, Self::title(), &self.sections, &self.lines, focused, self.scroll, f.measure);
        let clip = shield_panel::list_clip();
        for (i, &action) in self.rows.iter().enumerate() {
            let Some(rect) = self.row_rect(i).filter(|r| r.y + r.h > clip.y && r.y < clip.y + clip.h) else { continue };
            f.stop(p, Stop {
                key: FocusKey { entry: self.entry, elem: key_of(action) },
                rect,
                rest_rect: rect,
                clip,
                hover: Hover::Focus,
                activate: Activate::Immediate,
            });
        }
    }
    fn render(&self) -> RenderStrategy {
        RenderStrategy::Page
    }
}

impl LogicalState for DetailMoreScreen {
    fn write(&self, c: &mut Canon) {
        c.seq(self.rows.len());
        for &action in &self.rows {
            c.u32(key_of(action));
        }
        c.f32(self.scroll);
    }
    fn probe(&self, out: &mut String) {
        out.push_str("detail_more");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{AppFx, DetailActions, PageMemory};
    use plx_machine::machine::{Host, InputEvent, InstanceId, Source, Stamped, Tick};
    use plx_ui::fixture::FixtureMeasure;

    struct MoreHost;
    impl Host for MoreHost {
        type Arg = plx_ui::fixture::FixtureArg;
        type Fx = AppFx;
        type Msg = AppMsg;
        type Elem = u32;
        type Views<'a> = ();
        type Init = plx_ui::fixture::FixtureArg;
        type Memory = PageMemory;
    }

    const ENTRY: EntryId = EntryId(11);
    const HOST: InstanceId = InstanceId(42);

    fn panel(actions: &[DetailAction]) -> DetailMoreScreen {
        let set = actions.iter().fold(DetailActions::default(), |s, a| s.with(*a, true));
        DetailMoreScreen::new(ENTRY, DetailMoreArg { host: HOST, actions: set })
    }

    fn step(screen: &mut DetailMoreScreen, ev: ScreenEvent<MoreHost>, focus: Option<u32>) -> Vec<Stamped<MoreHost>> {
        let measure = FixtureMeasure;
        let cx = Cx::<MoreHost> {
            views: (),
            tick: Tick::default(),
            measure: &measure,
            press: Default::default(),
            focus: plx_machine::machine::FocusRead { current: focus.map(|elem| FocusKey { entry: ENTRY, elem }), ..Default::default() },
            owner: plx_machine::machine::InputOwner::Entry(ENTRY),
        };
        let mut effects = Vec::new();
        let mut present = plx_machine::present::Present::new();
        let mut fx = Effects::new(&mut effects, MachineId::Instance(InstanceId(1)), &mut present);
        let _ = Machine::<MoreHost>::step(screen, &ev, &cx, &mut fx);
        effects
    }

    fn key(code: Key) -> ScreenEvent<MoreHost> {
        ScreenEvent::Input(InputEvent {
            kind: InputKind::Key { key: code, sym: 0, wcode: 0, edge: Edge::Down, at_edge: false },
            at: Tick::default(),
            source: Source::RemoteFifo,
        })
    }

    /// e02: the panel's rows at the measured frame — 508x64 plates from y 210 on the 76 pitch a
    /// list of leading glyphs keeps — in the vocabulary's order whatever order they were offered.
    #[test]
    fn the_rows_stand_on_the_measured_panel_in_their_order() {
        plx_ui::layout::with_shield(|| {
            let more = panel(&[DetailAction::About, DetailAction::MarkWatched, DetailAction::TrackInfo]);
            assert_eq!(more.actions(), [DetailAction::TrackInfo, DetailAction::MarkWatched, DetailAction::About]);
            let rects = more.row_rects();
            assert_eq!(rects[0], plx_ui::Rect::new(1314.0, 210.0, 508.0, 64.0));
            assert_eq!(rects[1].y - rects[0].y, 76.0);
            assert_eq!(rects[2].y - rects[1].y, 76.0);
            assert!(more.sections[0].rows.iter().all(|r| r.licon.is_some()), "every row leads with its glyph");
        });
    }

    /// OK on a row dismisses the panel and hands the row's action to the Detail page that opened
    /// it; BACK and LEFT only dismiss; DOWN walks the rows and stops at the last.
    #[test]
    fn ok_delivers_the_row_to_its_page_and_back_only_closes() {
        plx_ui::layout::with_shield(|| {
            let mut more = panel(&[DetailAction::TrackInfo, DetailAction::About]);
            let about = more.key_for(DetailAction::About).unwrap();
            let fx = step(&mut more, ScreenEvent::Activate(about), Some(about));
            assert!(fx.iter().any(|s| matches!(s.fx, Fx::Nav(NavOp::Dismiss(ENTRY)))));
            assert!(fx.iter().any(|s| matches!(&s.fx, Fx::Deliver(MachineId::Instance(HOST),
                Delivery::Screen(ScreenEvent::App(AppMsg::DetailAction(DetailAction::About)))))));
            for code in [Key::Back, Key::Left] {
                let fx = step(&mut more, key(code), Some(about));
                assert!(fx.iter().any(|s| matches!(s.fx, Fx::Nav(NavOp::Dismiss(ENTRY)))), "{code:?} closes");
                assert!(!fx.iter().any(|s| matches!(s.fx, Fx::Deliver(..))), "{code:?} performs nothing");
            }
            let measure = FixtureMeasure;
            let cx = Cx::<MoreHost> {
                views: (), tick: Tick::default(), measure: &measure, press: Default::default(),
                focus: Default::default(), owner: plx_machine::machine::InputOwner::Entry(ENTRY),
            };
            let first = more.key_for(DetailAction::TrackInfo).unwrap();
            let down = <DetailMoreScreen as Focusable<MoreHost>>::neighbour(&more, FocusKey { entry: ENTRY, elem: first }, Dir::Down, &cx);
            assert!(matches!(down, Step::Move(FocusKey { entry: ENTRY, elem }) if elem == about));
            assert!(matches!(<DetailMoreScreen as Focusable<MoreHost>>::neighbour(&more, FocusKey { entry: ENTRY, elem: about },
                Dir::Down, &cx), Step::Edge));
            let seat = <DetailMoreScreen as Focusable<MoreHost>>::seat(&more, GroupId(0),
                Placed { rect: plx_ui::Rect::FULL, rest_rect: plx_ui::Rect::FULL, clip: plx_ui::Rect::FULL, index: None }, &cx);
            assert_eq!(seat.elem, first, "the panel opens on its first row");
        });
    }

    /// The panel is the Shield layout's: under Standard it closes itself at its next event.
    #[test]
    fn a_switch_to_standard_closes_the_panel() {
        let mut more = panel(&[DetailAction::About]);
        let fx = step(&mut more, ScreenEvent::Tick(Tick::default()), None);
        assert!(fx.iter().any(|s| matches!(s.fx, Fx::Nav(NavOp::Dismiss(ENTRY)))));
    }

    /// Every row's words are the app's, and fit the panel's label column in every shipped language.
    #[test]
    fn every_row_fits_the_panel_in_every_language() {
        let measure = plx_base::fontcov::advances::ShippedMeasure;
        for lang in plx_platform::i18n::SHIPPED {
            let _scope = plx_platform::i18n::language_on_this_thread_for_test(lang);
            let budget = shield_panel::ITEM_W - shield_panel::LABEL_INSET - shield_panel::GLYPH
                - shield_panel::GLYPH_LABEL_GAP - shield_panel::TRAIL_INSET;
            for action in DetailAction::ALL {
                let row = row_of(action);
                let w = plx_machine::machine::Measure::width_str(&measure, &row.label, shield_panel::LABEL_SIZE, false);
                assert!(w <= budget, "{lang:?} {action:?}: {:?} is {w} wide in {budget}", row.label);
            }
            let title = DetailMoreScreen::title();
            let w = plx_machine::machine::Measure::width_str(&measure, title, shield_panel::TITLE_SIZE, true);
            assert!(w <= shield_panel::TITLE.w, "{lang:?}: the title is {w} wide");
        }
        let _ = FixtureMeasure;
    }
}
