//! **The Shield layout's side panel** — a registered `Style::Sheet` surface on the shared
//! `ModalStack`, opened by LEFT off the left-most content of a bar-wearing page while
//! `plx_ui::layout::shield()` is on (`Rig::edge_surface`). It is the Shield layout's whole
//! navigation: what the Standard layout's top bar (the profile chip and the pills) offers, as one
//! column.
//!
//! The rows, top to bottom: the profile (face, name, chevron), Watchlist, Search, Home, a
//! *Libraries* header, then EVERY favourite library by name, in the profile's library order (the
//! list the Library's own library pills and the Movies / TV Shows pills select from), and Settings
//! last. A library row carries a film or a television glyph by its type.
//!
//! **What a row does is the application's, and every row dismisses.** OK sends a
//! [`LoopReq::Sidebar`] naming the destination ([`Go`]) — the same navigation the top bar's pill,
//! chip or library pill performs — and dismisses the panel in the same drain, as the profile menu
//! does. RIGHT and BACK only dismiss, so focus returns to the page exactly where it was (the
//! container restores the page's input scope); RIGHT on the profile row opens what OK opens.
//!
//! **What it knows arrives on its argument or its host**: the page it was opened over
//! ([`Current`], whose row it opens focused on and draws as the current page) and the captured
//! profile ([`Profile`], the same publication the Standard layout's chip draws) on [`SidebarArg`];
//! the libraries from the host's retained Browse directory, re-read when that publication moves.
//! The geometry and paint are `plx_ui::side_nav`'s.

use std::borrow::Cow;
use std::ffi::CString;

use crate::registry::{AppFx, DirectoryLike, LoopReq};
use plx_data::stores::browse::{DirectoryView, SecKind};
use plx_machine::machine::{
    Canon, Cx, Edge, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputKind, Key,
    LogicalState, Machine, NavOp,
};
use plx_plex::plex::ServerId;
use plx_ui::frame::Budget;
use plx_ui::icons::Icon;
use plx_ui::screen::{
    Activate, At, AxisMask, Dir, DrawFrame, EdgeRule, ElemKind, Focusable, GroupKind, GroupSpec,
    Hover, Placed, RenderStrategy, Screen, ScreenEvent, Scrim, Seat, Step, Stop,
};
use plx_ui::side_nav::{self, Line, State};
use plx_ui::widgets::ProfileChipRead;
use plx_ui::Rect;

pub const SHAPE: &str = "Sidebar{rows:[u32],libraries:[(sid:u32,key:i64)],scroll:f32}";

/// The page the panel was opened over: its row opens focused and is drawn as the current page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Current {
    Home,
    Watchlist,
    Search,
    /// The Library, on this library.
    Library { sid: ServerId, key: i64 },
}

/// The profile the panel's first row shows, as the application captured it for the shared chrome
/// (`app::chrome::ChromeSnapshot`): the name the Standard chip unfurls (already the account's
/// one resolver, `account_menu::chip_label`), its initial, and its picture on its server.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Profile {
    pub name: String,
    pub initial: String,
    pub thumb: String,
    /// The raw id of the server `thumb` is a path on.
    pub src: u16,
}

/// What [`crate::registry::AppArg::Sidebar`] carries.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SidebarArg {
    pub current: Current,
    pub profile: Profile,
}

impl LogicalState for SidebarArg {
    fn write(&self, c: &mut Canon) {
        match self.current {
            Current::Home => { c.u32(0); }
            Current::Watchlist => { c.u32(1); }
            Current::Search => { c.u32(2); }
            Current::Library { sid, key } => { c.u32(3).u32(sid.raw() as u32).u64(key as u64); }
        }
        c.str(&self.profile.name).str(&self.profile.initial).str(&self.profile.thumb).u32(self.profile.src as u32);
    }
    fn probe(&self, out: &mut String) {
        out.push_str("sidebar_arg");
    }
}

/// Where a row goes: the application performs it (`LoopReq::Sidebar`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Go {
    /// The profile menu the Standard layout's chip opens.
    Account,
    Watchlist,
    Search,
    Home,
    /// The Library, on this library.
    Library { sid: ServerId, key: i64, kind: SecKind },
    /// The Settings surface.
    Settings,
}

/// One row of the panel.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Row {
    Account,
    Watchlist,
    Search,
    Home,
    Header,
    Library { sid: ServerId, key: i64, kind: SecKind },
    Settings,
}

impl Row {
    fn line(&self) -> Line {
        match self {
            Row::Account => Line::Account,
            Row::Header => Line::Header,
            _ => Line::Item,
        }
    }
    fn go(&self) -> Option<Go> {
        Some(match *self {
            Row::Account => Go::Account,
            Row::Watchlist => Go::Watchlist,
            Row::Search => Go::Search,
            Row::Home => Go::Home,
            Row::Library { sid, key, kind } => Go::Library { sid, key, kind },
            Row::Settings => Go::Settings,
            Row::Header => return None,
        })
    }
    fn glyph(&self) -> Icon {
        match self {
            Row::Watchlist => Icon::Bookmark,
            Row::Search => Icon::Search,
            Row::Library { kind: SecKind::Movie, .. } => Icon::Film,
            Row::Library { kind: SecKind::Show, .. } => Icon::Tv,
            Row::Settings => Icon::Gear,
            Row::Home | Row::Account | Row::Header => Icon::Home,
        }
    }
    fn is(&self, current: Current) -> bool {
        match (self, current) {
            (Row::Home, Current::Home) | (Row::Watchlist, Current::Watchlist) | (Row::Search, Current::Search) => true,
            (Row::Library { sid, key, .. }, Current::Library { sid: s, key: k }) => *sid == s && *key == k,
            _ => false,
        }
    }
}

// Focus keys: an identity, never a position. A library's is its interned (server, section) slot,
// stable for the panel's life whatever the order its directory publishes.
const ACCOUNT: u32 = 1;
const WATCHLIST: u32 = 2;
const SEARCH: u32 = 3;
const HOME: u32 = 4;
const SETTINGS: u32 = 5;
const HEADER: u32 = 6;
const LIBRARY_BASE: u32 = 100;

/// **The rows, from the retained directory** — pure, so a test drives it with any directory. The
/// libraries are the profile's favourites with an address, in its display order (the order the
/// Library order page sets and the Library's same-type pills follow), and `(sid, key, kind,
/// title)` each.
fn library_rows(directory: DirectoryView<'_>) -> Vec<(ServerId, i64, SecKind, String)> {
    let sections = directory.sections();
    directory.display_order().into_iter()
        .filter_map(|i| sections.get(i))
        .filter(|section| section.row.pinned)
        .filter_map(|section| section.sid.map(|sid| (sid, section.key, section.kind, section.row.title.clone())))
        .collect()
}

pub struct SidebarScreen {
    entry: EntryId,
    arg: SidebarArg,
    rows: Vec<Row>,
    /// Each row's focus key, beside `rows`.
    keys: Vec<u32>,
    /// Each row's drawn words: the app's own labels, the server's library names elided to the
    /// column, the header's.
    labels: Vec<CString>,
    /// The header's measured width (its rule starts past it).
    header_w: f32,
    /// Each row's top in document space (`side_nav::tops`).
    tops: Vec<f32>,
    /// The (server, section) slots library keys are interned against.
    libraries: Vec<(ServerId, i64)>,
    /// The list's scroll: how far it has moved up to keep focus inside the frame.
    scroll: f32,
    /// The directory publication the rows were built from.
    built_for: Option<(Option<u32>, u32, u32, usize)>,
    name: CString,
    initial: CString,
}

impl SidebarScreen {
    pub fn new(entry: EntryId, arg: SidebarArg, directory: DirectoryView<'_>,
        measure: &dyn plx_machine::machine::Measure) -> Self {
        let name = CString::new(plx_gfx::text::elide_by(&arg.profile.name, side_nav::NAME_MAX_W, false, |t| {
            measure.width_str(t, side_nav::LABEL_SIZE, false)
        })).unwrap_or_default();
        let initial = CString::new(arg.profile.initial.clone()).unwrap_or_default();
        let mut s = Self {
            entry, arg, rows: Vec::new(), keys: Vec::new(), labels: Vec::new(), header_w: 0.0,
            tops: Vec::new(), libraries: Vec::new(), scroll: 0.0, built_for: None, name, initial,
        };
        s.rebuild(directory, measure);
        s
    }

    fn publication(directory: DirectoryView<'_>) -> (Option<u32>, u32, u32, usize) {
        (directory.epoch(), directory.sections_gen(), directory.tabs_gen(), directory.section_count())
    }

    /// Re-derive the rows from the directory. Keys are identities, so focus survives a rebuild on
    /// the row it was on; a row that vanished is reconciled to the current page's.
    fn rebuild(&mut self, directory: DirectoryView<'_>, measure: &dyn plx_machine::machine::Measure) {
        self.built_for = Some(Self::publication(directory));
        self.rows = vec![Row::Account, Row::Watchlist, Row::Search, Row::Home];
        let libraries = library_rows(directory);
        if !libraries.is_empty() {
            self.rows.push(Row::Header);
        }
        let mut titles = Vec::with_capacity(libraries.len());
        for (sid, key, kind, title) in libraries {
            self.rows.push(Row::Library { sid, key, kind });
            titles.push(title);
        }
        self.rows.push(Row::Settings);
        let mut titles = titles.into_iter();
        self.keys.clear();
        self.labels.clear();
        for i in 0..self.rows.len() {
            let row = self.rows[i].clone();
            let (key, label) = match row {
                Row::Account => (ACCOUNT, String::new()),
                Row::Watchlist => (WATCHLIST, plx_platform::i18n::msg::browse_watchlist_title().to_string()),
                Row::Search => (SEARCH, plx_platform::i18n::msg::browse_sidebar_search().to_string()),
                Row::Home => (HOME, plx_platform::i18n::msg::browse_chrome_home().to_string()),
                Row::Header => (HEADER, plx_platform::i18n::msg::browse_sidebar_libraries().to_string()),
                Row::Settings => (SETTINGS, plx_platform::i18n::msg::settings_account_settings().to_string()),
                Row::Library { sid, key, .. } => {
                    // a library's name is the server's, so it may elide; the app's words never do
                    let title = titles.next().unwrap_or_default();
                    let title = plx_gfx::text::elide_by(&title, side_nav::LABEL_MAX_W, false, |t| {
                        measure.width_str(t, side_nav::LABEL_SIZE, false)
                    });
                    (self.library_key(sid, key), title)
                }
            };
            self.keys.push(key);
            self.labels.push(CString::new(label).unwrap_or_default());
        }
        let header = self.rows.iter().position(|row| *row == Row::Header);
        self.header_w = header.map_or(0.0, |i| measure.width(&self.labels[i], side_nav::HEADER_SIZE, false));
        let lines: Vec<Line> = self.rows.iter().map(Row::line).collect();
        self.tops = side_nav::tops(&lines);
    }

    fn library_key(&mut self, sid: ServerId, key: i64) -> u32 {
        let slot = self.libraries.iter().position(|&l| l == (sid, key)).unwrap_or_else(|| {
            self.libraries.push((sid, key));
            self.libraries.len() - 1
        });
        LIBRARY_BASE + slot as u32
    }

    fn index_of(&self, elem: u32) -> Option<usize> {
        self.keys.iter().position(|&k| k == elem).filter(|&i| self.rows[i].line().focusable())
    }

    /// The row the panel opens on: the current page's, else Home's.
    fn current_index(&self) -> usize {
        self.rows.iter().position(|row| row.is(self.arg.current))
            .or_else(|| self.rows.iter().position(|row| *row == Row::Home))
            .unwrap_or(0)
    }

    fn row_rect(&self, i: usize) -> Rect {
        side_nav::line_rect(self.tops[i], self.rows[i].line(), self.scroll)
    }

    /// Keep the focused row inside the frame.
    fn reveal(&mut self, elem: u32) {
        if let Some(i) = self.index_of(elem) {
            self.scroll = side_nav::scroll_for(self.tops[i], self.rows[i].line().height(), self.scroll);
        }
    }

    fn profile(&self) -> ProfileChipRead<'_> {
        ProfileChipRead { src: self.arg.profile.src, thumb: &self.arg.profile.thumb,
            initial: &self.initial, name: &self.name, name_w: 0.0 }
    }

    /// OK on a row: ask the application to go there, and dismiss.
    fn activate<H: DirectoryLike>(&mut self, elem: u32, fx: &mut Effects<'_, H>) {
        if let Some(go) = self.index_of(elem).and_then(|i| self.rows[i].go()) {
            fx.push(Fx::App(AppFx::Loop(LoopReq::Sidebar(go))));
        }
        fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));
    }

    /// The focused row's index, for a probe or a test — a READ of the cursor the engine holds.
    pub fn focused_row(&self, focus: Option<FocusKey<u32>>) -> Option<usize> {
        focus.filter(|k| k.entry == self.entry).and_then(|k| self.index_of(k.elem))
    }

    /// Where focused row `i` would send the application, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn go_at(&self, i: usize) -> Option<Go> {
        self.rows.get(i).and_then(Row::go)
    }

    /// The focus key of the row that goes to `go`, for a test or a scripted walk.
    #[cfg(any(test, feature = "test-support"))]
    pub fn key_of(&self, go: Go) -> Option<u32> {
        self.rows.iter().position(|row| row.go() == Some(go)).map(|i| self.keys[i])
    }

    /// Each row's drawn rect at the current scroll, for a geometry test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn row_rects(&self) -> Vec<Rect> {
        (0..self.rows.len()).map(|i| self.row_rect(i)).collect()
    }
}

impl<H: DirectoryLike> Machine<H> for SidebarScreen {
    type Ev = ScreenEvent<H>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        match ev {
            ScreenEvent::Tick(_) => {
                let directory = H::directory(cx);
                if self.built_for != Some(Self::publication(directory)) {
                    self.rebuild(directory, cx.measure);
                    fx.invalidate(plx_machine::present::Provenance::Landing(fx.from()));
                }
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
                if let InputKind::Key { key, edge: Edge::Down, .. } = input.kind {
                    match key {
                        // BACK closes, and focus returns to the page where it was
                        Key::Back => {
                            fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));
                            return Handled::Yes;
                        }
                        // RIGHT closes too, except on the profile row, which it opens like OK
                        Key::Right => {
                            match cx.focus.current.filter(|key| key.entry == self.entry) {
                                Some(focus) if focus.elem == ACCOUNT => self.activate(ACCOUNT, fx),
                                _ => fx.push(Fx::Nav(NavOp::Dismiss(self.entry))),
                            }
                            return Handled::Yes;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Handled::No
    }
}

impl<H: DirectoryLike> Focusable<H> for SidebarScreen {
    fn groups(&self, _cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        out.push(GroupSpec {
            id: GroupId(0),
            kind: GroupKind::Column,
            seat: Seat::Remembered,
            reachable: AxisMask::BOTH,
            edge: [EdgeRule::Stop; 4],
            extent: side_nav::PANEL,
            len: self.rows.iter().filter(|row| row.line().focusable()).count(),
            elem: ElemKind::Bare,
        });
    }
    fn group_of(&self, elem: &u32, _: &Cx<'_, H>) -> Option<GroupId> {
        self.index_of(*elem).map(|_| GroupId(0))
    }
    /// UP and DOWN walk the rows, skipping the header; LEFT and RIGHT never move.
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, _: &Cx<'_, H>) -> Step<u32> {
        let Some(i) = self.index_of(key.elem) else { return Step::Edge };
        let next = match dir {
            Dir::Up => (0..i).rev().find(|&j| self.rows[j].line().focusable()),
            Dir::Down => (i + 1..self.rows.len()).find(|&j| self.rows[j].line().focusable()),
            Dir::Left | Dir::Right => None,
        };
        next.map_or(Step::Edge, |j| Step::Move(FocusKey { entry: self.entry, elem: self.keys[j] }))
    }
    fn place(&self, elem: &u32, _: &Cx<'_, H>, _: At) -> Option<Placed> {
        let i = self.index_of(*elem)?;
        let rect = self.row_rect(i);
        Some(Placed { rect, rest_rect: rect, clip: side_nav::PANEL_CLIP, index: Some(i as u32) })
    }
    fn reconcile(&self, want: FocusKey<u32>, _: &Cx<'_, H>) -> FocusKey<u32> {
        if self.index_of(want.elem).is_some() {
            want
        } else {
            FocusKey { entry: self.entry, elem: self.keys[self.current_index()] }
        }
    }
    /// The panel opens on the current page's row (spec §2.3: Home on Home, the library inside one).
    fn seat(&self, _: GroupId, _: Placed, _: &Cx<'_, H>) -> FocusKey<u32> {
        FocusKey { entry: self.entry, elem: self.keys[self.current_index()] }
    }
}

impl<H: DirectoryLike> Screen<H> for SidebarScreen {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn name(&self) -> &'static str {
        crate::registry::word::SIDEBAR
    }
    fn state(&self) -> &dyn LogicalState {
        self
    }
    fn crumb(&self, _: &Cx<'_, H>) -> Option<Cow<'_, str>> {
        None
    }
    /// The page behind is dimmed to half (spec §1.7: every sample scales by exactly 0.50); the
    /// panel itself is opaque, so nothing is lifted out of the dim.
    fn scrim(&self) -> Scrim {
        Scrim::dim(plx_ui::theme::underlay::DIM_SHEET)
    }
    fn prepare(&mut self, _: &mut Budget, _: &Cx<'_, H>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        let p = f.painter.alpha(f.page_alpha);
        side_nav::draw_ground(p);
        let focus = f.cx.focus.current.filter(|key| key.entry == self.entry).map(|key| key.elem);
        let mut stops = Vec::new();
        {
            let _clip = f.clip(p, side_nav::PANEL_CLIP);
            for i in 0..self.rows.len() {
                let r = self.row_rect(i);
                if r.y + r.h < side_nav::PANEL_CLIP.y || r.y > side_nav::PANEL_CLIP.y + side_nav::PANEL_CLIP.h {
                    continue;
                }
                let row = &self.rows[i];
                let state = if focus == Some(self.keys[i]) {
                    State::Focused
                } else if row.is(self.arg.current) {
                    State::Current
                } else {
                    State::Idle
                };
                match row {
                    Row::Header => side_nav::draw_header(p, r.y, &self.labels[i], self.header_w),
                    Row::Account => side_nav::draw_account(p, r, self.profile(), state),
                    _ => side_nav::draw_item(p, r, row.glyph(), &self.labels[i], state),
                }
                if row.line().focusable() {
                    stops.push(Stop {
                        key: FocusKey { entry: self.entry, elem: self.keys[i] },
                        rect: r,
                        rest_rect: r,
                        clip: side_nav::PANEL_CLIP,
                        hover: Hover::Focus,
                        activate: Activate::Immediate,
                    });
                }
            }
        }
        for stop in stops {
            f.stop(p, stop);
        }
    }
    fn render(&self) -> RenderStrategy {
        RenderStrategy::Page
    }
}

impl LogicalState for SidebarScreen {
    fn write(&self, c: &mut Canon) {
        c.seq(self.keys.len());
        for &key in &self.keys {
            c.u32(key);
        }
        c.seq(self.libraries.len());
        for &(sid, key) in &self.libraries {
            c.u32(sid.raw() as u32).u64(key as u64);
        }
        c.f32(self.scroll);
    }
    fn probe(&self, out: &mut String) {
        out.push_str("sidebar");
    }
}

#[cfg(test)]
#[path = "sidebar_tests.rs"]
mod tests;
