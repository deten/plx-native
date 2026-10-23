//! The **Watchlist** page: the profile's Plex Watchlist as one poster grid, newest addition first.
//!
//! A PEER of Home exactly as Search is (`docs/search.md` §1): it wears the shared top bar, its
//! pill sits between the type pills and Search, BACK returns to Home and what it opens stacks.
//! The store (`plx_data::watchlist`) owns the list, the matching against the user's servers and
//! every request; this screen owns presentation and the guid→element registry.
//!
//! The page is a `plx_ui::cards::Stack` of three sections, `collection.rs`'s shape: a heading
//! (`Watchlist · 12 titles`), the grid, and the status read-outs (loading, empty, a profile with no
//! Watchlist, a failure with Retry). A card is one of three things, by what the servers said:
//!
//! - **on a server**, drawn from that server's own row (its artwork, its resume bar); OK opens
//!   that row's detail page, so play and resume work exactly as anywhere else, and a hold opens the
//!   ordinary card menu plus *Remove from Watchlist*;
//! - **on no server**, the provider's artwork dimmed under *Not on your server*
//!   (`widgets::poster_note`); OK or a hold opens a card ([`card::TitleCard`]) with the title, year
//!   and summary and *Remove from Watchlist*;
//! - **still being checked**, drawn plainly; a press waits for the answer.
//!
//! In the Shield layout (`plx_ui::layout::shield()`) the page is dressed as that layout's library
//! Browse grid (SHIELD-LAYOUT-SPEC §6.1; the app it follows was not captured on its own list): the
//! title stands in the tab pills' place with the count beside it, the grid is six captioned
//! 240x360 posters across from x 127 (`RowStyle::SHIELD_GRID`) on the neutral ground, a row after
//! the first held at the Browse grid's focus line with the row above in view, and there is no chip
//! bar, the list having no filter or sort. The cards, their presses, the dimmed *Not on
//! your server* note and the title card are the same; LEFT off the grid's first column is the side
//! panel's door (`Rig::edge_surface`). The layout is latched once per event and a switch re-lays
//! the page in place (`WatchlistScreen::sync_layout`).

mod card;
#[cfg(test)]
mod tests;

use card::{CardStep, TitleCard};
use plx_data::pms::PmsMovie;
use plx_data::stores::watchlist::WatchlistCmd;
use plx_data::stores::{StoreCmd, StoreId};
use plx_data::watchlist::{Availability, Entry, WatchlistStatus, WatchlistView};
use plx_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputEvent, InputKind,
    InstanceId, Key, LogicalState, Machine, MachineId, Measure,
};
use plx_machine::present::Provenance;
use plx_ui::cards::{self as ui_cards, CardEvent, CardSource, GridSpec, Kind, SectionSpec, Stack, StackEvent, StackPage, Tile, TileLabel};
use plx_ui::consts::{MARGIN_X, SCR_H, SCR_W};
use plx_ui::screen::{
    At, AxisMask, By, Dir, DrawFrame, EdgeRule, ElemKind, Enter, FocusSource, FocusTarget, Focusable,
    GroupKind, GroupSpec, HitSource, Link, Placed, RenderStrategy, Screen, ScreenEvent, Seat, Step,
};
use plx_ui::widgets::{self, Art, PageGround, StatusKind, StatusOverlay};
use plx_ui::{Env, Painter, Rect};

use crate::registry::{tile_facts, AppFx, HomeTab, MetadataLike, WatchlistReq};

pub const SHAPE: &str = "WatchlistScreen{entry:u32,instance:u32,next_elem:u32,keys:[{guid:str,elem:u32}],moved:bool,card:Option<str>,stack:Stack{scroll:{pos:f32,vel:f32},target:f32,sections:[Grid{bands,pop}]}}";

const RETRY_ELEM: u32 = 1;
const FIRST_CARD_ELEM: u32 = 0x1000;
/// The strip element of this page's own pill (`app::chrome`'s keys: +4 is the profile chip).
pub const STRIP_WATCHLIST_ELEM: u32 = plx_ui::dispatch::STRIP_BASE + 5;
const STRIP: GroupId = plx_ui::containers::tabs::STRIP;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sec {
    Head,
    Items,
    Status,
}
/// `GroupId(0)`: a tab arrival seats the first group, which is the grid.
const GRID_GROUP: GroupId = GroupId(0);
const HEAD_GROUP: GroupId = GroupId(1);
const STATUS_GROUP: GroupId = GroupId(2);

/// The heading's cap top: the Library's content line under the bar.
const HEADING_Y: f32 = plx_ui::consts::GRID_TOP_Y;
/// The first row's poster top, the Collection page's heading-to-poster air (73) under it.
const GRID_TOP: f32 = HEADING_Y + 73.0;
/// The quiet read-outs' region, where the grid would be.
const STATUS_FRAME: Rect = Rect { x: MARGIN_X, y: GRID_TOP, w: SCR_W - 2.0 * MARGIN_X, h: 475.0 };
/// The Shield grid's first row: under the title pill, where the library's Categories tiles start.
const SHIELD_GRID_TOP: f32 = plx_ui::shield_browse::TILE_TOP;
/// The Shield read-outs' region, where that grid would be.
const SHIELD_STATUS_FRAME: Rect = Rect { x: plx_ui::shield::CONTENT_X, y: SHIELD_GRID_TOP,
    w: plx_ui::shield::CONTENT_RIGHT - plx_ui::shield::CONTENT_X, h: STATUS_FRAME.h };

/// The card a press on `entry` names, by what the servers said about it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Press {
    Open { sid: plx_plex::plex::ServerId, rk: String },
    Card,
    Wait,
}

fn press_of(entry: &Entry) -> Press {
    match &entry.avail {
        Availability::OnServer(row) => Press::Open { sid: row.sid, rk: row.rk.clone() },
        Availability::Missing => Press::Card,
        Availability::Checking => Press::Wait,
    }
}

/// The row a card is DRAWN from: the server's own row when one holds the title, else the
/// provider's facts stood up as a row on `art_sid` (whose photo transcoder fetches the provider's
/// absolute artwork URL, as the person page's filmography already does).
fn card_row(entry: &Entry, art_sid: plx_plex::plex::ServerId) -> PmsMovie {
    match &entry.avail {
        Availability::OnServer(row) => (**row).clone(),
        _ => PmsMovie {
            sid: art_sid,
            title: entry.item.title.clone(),
            year: entry.item.year as _,
            thumb: entry.item.thumb.clone(),
            summary: entry.item.summary.clone(),
            kind: if entry.item.is_show() { 1 } else { 0 },
            ..Default::default()
        },
    }
}

/// The grid's content over the page's derived index.
struct Cards<'a> {
    entries: &'a [Entry],
    rows: &'a [PmsMovie],
    elems: &'a [u32],
    note: &'a str,
    /// The Shield layout's ring cards draw their own resume bar (`shield::draw_card`, from
    /// `progress`), inset in the card's foot; the overlay does not draw a second.
    shield: bool,
}

impl<'a> Cards<'a> {
    fn indexed(&self) -> bool { self.elems.len() == self.entries.len() && self.rows.len() == self.entries.len() }
}

impl<H: plx_machine::machine::Host<Elem = u32>> CardSource<H> for Cards<'_> {
    fn len(&self) -> usize { if self.indexed() { self.entries.len() } else { 0 } }
    fn elem(&self, i: usize) -> u32 { self.elems.get(i).copied().unwrap_or(FIRST_CARD_ELEM) }
    fn index_of(&self, e: &u32) -> Option<usize> { self.elems.iter().position(|x| x == e) }
    fn art(&self, i: usize) -> Art<'_> { Art::Poster(self.rows.get(i).map(tile_facts::of)) }
    fn label(&self, i: usize) -> TileLabel {
        self.rows.get(i).map(|row| ui_cards::poster_label(&tile_facts::of(row))).unwrap_or_default()
    }
    fn progress(&self, i: usize) -> Option<f32> {
        self.entries.get(i)?.avail.on_server()?.resume_frac()
    }
    fn overlay(&self, p: Painter, i: usize, tile: &Tile, measure: &dyn Measure) {
        let Some(entry) = self.entries.get(i) else { return };
        match &entry.avail {
            Availability::Missing => widgets::poster_note(p, tile.rect, tile.radius, self.note, measure),
            Availability::OnServer(row) if !self.shield => {
                if let Some(frac) = row.resume_frac() { ui_cards::resume_bar(p, tile.rect, frac, tile.radius); }
            }
            Availability::OnServer(_) => {}
            Availability::Checking => {}
        }
    }
}

/// The page's content and derived state; the `Stack` is the screen's other field.
struct Page {
    entry: EntryId,
    /// guid → element, in order of first sight. Removed titles stay as tombstones so an element is
    /// never handed to a different title.
    keys: Vec<(String, u32)>,
    next_elem: u32,
    /// Derived from the store at its `revision` (`synced`): `elems[i]`/`rows[i]` are entry `i`'s.
    elems: Vec<u32>,
    rows: Vec<PmsMovie>,
    count: String,
    note: String,
    synced: Option<u64>,
    status: WatchlistStatus,
    empty: bool,
    /// The layout the page is laid out for (`WatchlistScreen::sync_layout`).
    shield: bool,
}

impl Page {
    fn intern(&mut self, guid: &str) -> u32 {
        if let Some((_, elem)) = self.keys.iter().find(|(g, _)| g == guid) {
            return *elem;
        }
        let elem = self.next_elem;
        self.next_elem = elem.checked_add(1).expect("watchlist element space exhausted");
        self.keys.push((guid.to_string(), elem));
        elem
    }

    fn sync(&mut self, view: WatchlistView<'_>) {
        self.synced = Some(view.revision());
        self.status = view.status();
        let entries = view.entries();
        self.elems = entries.iter().map(|e| e.item.guid.clone()).collect::<Vec<_>>()
            .iter().map(|g| self.intern(g)).collect();
        self.rows = entries.iter().map(|e| card_row(e, view.art_sid())).collect();
        self.empty = entries.is_empty();
        self.count = plx_platform::i18n::msg::browse_watchlist_count(entries.len() as i64);
        self.note = plx_platform::i18n::msg::browse_watchlist_not_on_server().to_string();
    }

    fn view<'a, H: MetadataLike>(cx: &Cx<'a, H>) -> WatchlistView<'a> {
        H::watchlist(cx).unwrap_or_default()
    }

    fn ready(&self) -> bool { self.status == WatchlistStatus::Ready && !self.empty }

    fn cards<'a>(&'a self, view: WatchlistView<'a>) -> Cards<'a> {
        Cards { entries: view.entries(), rows: &self.rows, elems: &self.elems, note: &self.note, shield: self.shield }
    }

    fn entry_of<'a>(&self, view: WatchlistView<'a>, elem: u32) -> Option<&'a Entry> {
        let guid = self.keys.iter().find(|(_, e)| *e == elem).map(|(g, _)| g.as_str())?;
        view.entry(guid)
    }

    fn status_frame(&self) -> Rect {
        if self.shield { SHIELD_STATUS_FRAME } else { STATUS_FRAME }
    }

    fn status_overlay<'a>(&self, tick: u32) -> Option<StatusOverlay<'a>> {
        Some(match self.status {
            WatchlistStatus::Ready if self.empty => StatusOverlay::new(self.status_frame(),
                plx_platform::i18n::msg::browse_watchlist_empty_c(), StatusKind::Empty)
                .reason(plx_platform::i18n::msg::browse_watchlist_empty_reason_c()),
            WatchlistStatus::Ready => return None,
            // placeholder-exempt: builds the value only; counted where drawn (StatusOverlay::draw_geometry, Working)
            WatchlistStatus::Idle | WatchlistStatus::Loading => StatusOverlay::new(self.status_frame(),
                plx_platform::i18n::msg::browse_watchlist_loading_c(), StatusKind::Working).phase(tick),
            WatchlistStatus::Unavailable => StatusOverlay::new(Rect::FULL,
                plx_platform::i18n::msg::browse_watchlist_unavailable_c(), StatusKind::Failed)
                .page(plx_ui::icons::Icon::PersonBadgeXmark)
                .reason(plx_platform::i18n::msg::browse_watchlist_unavailable_reason_c())
                .glyph_ceiling(widgets::TOP_BAR_BOTTOM),
            WatchlistStatus::Failed => StatusOverlay::new(Rect::FULL,
                plx_platform::i18n::msg::browse_watchlist_failed_c(), StatusKind::Failed)
                .page(plx_ui::icons::Icon::CloudBadgeAlert)
                .reason(plx_platform::i18n::msg::browse_watchlist_failed_reason_c())
                .action(plx_platform::i18n::msg::browse_action_retry_c())
                .glyph_ceiling(widgets::TOP_BAR_BOTTOM),
        })
    }

    fn first_group(&self) -> GroupId {
        if self.ready() { GRID_GROUP } else { STATUS_GROUP }
    }
}

impl<H: MetadataLike> StackPage<H> for Page {
    type Key = Sec;
    type Cards<'a> = Cards<'a>;

    fn revision(&self, _cx: &Cx<'_, H>) -> u64 {
        let status = match self.status {
            WatchlistStatus::Idle => 0, WatchlistStatus::Loading => 1, WatchlistStatus::Ready => 2,
            WatchlistStatus::Unavailable => 3, WatchlistStatus::Failed => 4,
        };
        self.synced.unwrap_or(0) << 5 | u64::from(self.shield) << 4 | status << 1 | u64::from(self.empty)
    }

    fn sections(&self, cx: &Cx<'_, H>, out: &mut Vec<SectionSpec<Sec>>) {
        if self.shield {
            // as the library's Browse grid: a row after the first is held at `GRID_FOCUS_ROW_Y`,
            // the row above it still in view (d02)
            let spec = GridSpec::new(SHIELD_GRID_TOP, plx_ui::shield_browse::GRID_FOCUS_ROW_Y)
                .columns(plx_ui::shield_browse::GRID_COLS, ui_cards::RowStyle::SHIELD_GRID, plx_ui::shield::CONTENT_X)
                .keeping_above();
            out.push(SectionSpec::new(Sec::Head, Kind::Custom { height: SHIELD_GRID_TOP, focusable: false }, HEAD_GROUP).ranked(2));
            out.push(SectionSpec::new(Sec::Items, Kind::Grid { spec }, GRID_GROUP));
        } else {
            out.push(SectionSpec::new(Sec::Head, Kind::Custom { height: GRID_TOP, focusable: false }, HEAD_GROUP).ranked(2));
            out.push(SectionSpec::new(Sec::Items, Kind::Grid { spec: GridSpec::new(GRID_TOP, HEADING_Y) }, GRID_GROUP));
        }
        let retry = self.status_overlay(cx.tick.ms)
            .filter(|_| self.status == WatchlistStatus::Failed)
            .and_then(|o| o.action_frame_measured(cx.measure));
        out.push(SectionSpec::new(Sec::Status,
            Kind::Overlay { rect: retry.unwrap_or(self.status_frame()), focusable: retry.is_some() }, STATUS_GROUP).ranked(1));
    }

    fn fallback(&self, _cx: &Cx<'_, H>, out: &mut Vec<Sec>) {
        out.extend([Sec::Items, Sec::Status]);
    }

    fn cards<'a>(&'a self, cx: &'a Cx<'_, H>, k: Sec) -> Option<Cards<'a>> {
        match k {
            Sec::Items if self.ready() => Some(self.cards(Self::view(cx))),
            _ => None,
        }
    }

    fn elem_of(&self, k: Sec) -> Option<u32> {
        match k { Sec::Status => Some(RETRY_ELEM), _ => None }
    }

    fn card_has_menu(&self, _cx: &Cx<'_, H>, _k: Sec, _elem: &u32) -> bool { true }

    fn plain_group(&self, _cx: &Cx<'_, H>, _k: Sec, id: GroupId, extent: Rect) -> GroupSpec {
        GroupSpec { id, kind: GroupKind::Free, seat: Seat::First, reachable: AxisMask::BOTH,
            edge: [EdgeRule::Stop; 4], extent, len: 1, elem: ElemKind::Bare }
    }

    fn custom_draw(&self, k: Sec, f: &mut DrawFrame<'_, '_, H>, r: Rect, scroll: f32) {
        let p = f.painter.alpha(f.page_alpha);
        match k {
            Sec::Head if self.shield => self.draw_shield_head(p, r, f.measure),
            Sec::Head => {
                let alpha = (1.0 - scroll / (GRID_TOP - HEADING_Y)).clamp(0.0, 1.0);
                if alpha > 0.0 && self.status == WatchlistStatus::Ready {
                    let note = if self.empty { "" } else { self.count.as_str() };
                    ui_cards::draw_heading(p.alpha(alpha), plx_platform::i18n::msg::browse_watchlist_title(), note,
                        MARGIN_X, HEADING_Y - scroll, SCR_W - 2.0 * MARGIN_X, f.measure);
                }
            }
            Sec::Status => {
                let focused = f.focus.current.is_some_and(|key| key.entry == self.entry && key.elem == RETRY_ELEM);
                if let Some(overlay) = self.status_overlay(f.cx.tick.ms) {
                    overlay.focused(focused).draw_measured(&Env::inert(), p, f.measure);
                }
            }
            Sec::Items => {}
        }
    }
}

impl Page {
    /// **The Shield head**, its top at `r.y` (the document carries it away): the page's title as
    /// the one selected pill in the tab pills' track, and once the list is up its count in the dark
    /// box the Browse chip bar's count wears.
    fn draw_shield_head(&self, p: Painter, r: Rect, measure: &dyn Measure) {
        use plx_ui::shield_browse as sb;
        let title = std::ffi::CString::new(plx_platform::i18n::msg::browse_watchlist_title()).unwrap_or_default();
        let pills = sb::tab_pills(&[measure.width(&title, sb::TAB_SIZE, false)], r.y);
        sb::draw_tabs(p, &[title.as_c_str()], &pills, Some(0), None);
        let Some(track) = sb::tab_track(&pills).filter(|_| self.ready()) else { return };
        let count = std::ffi::CString::new(self.count.as_str()).unwrap_or_default();
        let w = measure.width(&count, sb::COUNT_SIZE, false);
        let rect = Rect::new(track.x + track.w + sb::CHIP_GAP, track.cy() - sb::COUNT_H * 0.5,
            w + 2.0 * sb::COUNT_PAD, sb::COUNT_H);
        sb::draw_count(p, rect, &count);
    }
}

pub struct WatchlistScreen {
    page: Page,
    stack: Stack<Sec>,
    card: TitleCard,
    instance: InstanceId,
    /// The person moved focus themselves since this page came up. Until they do, a list that lands
    /// under a focus parked on the page's own pill (nothing to seat while loading) takes the focus
    /// down into the grid, the way a tab arrival seats the page's first element.
    moved: bool,
    ground: PageGround,
    ground_seeded: bool,
    /// The Shield layout's neutral ground (SHIELD-LAYOUT-SPEC §1.4 bg.neutral); idle in Standard.
    neutral: plx_ui::shield::HeroGround,
}

impl LogicalState for WatchlistScreen {
    fn write(&self, c: &mut Canon) {
        c.u32(self.page.entry.0).u32(self.instance.0).u32(self.page.next_elem).seq(self.page.keys.len());
        for (guid, elem) in &self.page.keys { c.str(guid).u32(*elem); }
        c.bool(self.moved);
        c.option(self.card.subject(), |c, guid| { c.str(guid); });
        self.stack.write(c);
    }
    fn probe(&self, out: &mut String) {
        out.push_str(&format!("watchlist entry={} cards={} status={:?} card_open={}",
            self.page.entry.0, self.page.elems.len(), self.page.status, self.card.is_open()));
    }
}

impl WatchlistScreen {
    pub fn new(entry: EntryId, instance: InstanceId) -> Self {
        let shield = plx_ui::layout::shield();
        Self {
            page: Page { entry, keys: Vec::new(), next_elem: FIRST_CARD_ELEM, elems: Vec::new(), rows: Vec::new(),
                count: String::new(), note: String::new(), synced: None, status: WatchlistStatus::Idle, empty: true,
                shield },
            stack: new_stack(entry, shield),
            card: TitleCard::new(),
            instance,
            moved: false,
            ground: PageGround::new(),
            ground_seeded: false,
            neutral: plx_ui::shield::HeroGround::new(),
        }
    }

    /// **Latch the live layout for this event.** A change while the page is up re-lays it in place:
    /// a fresh document of the new layout's sections (the grid's columns, card style and first row
    /// all change), its scroll home; each card keeps its element, so a focused card stays focused.
    fn sync_layout<H: MetadataLike>(&mut self, fx: &mut Effects<'_, H>) {
        let shield = plx_ui::layout::shield();
        if shield == self.page.shield {
            return;
        }
        self.page.shield = shield;
        self.stack = new_stack(self.entry(), shield);
        self.neutral.reset();
        fx.invalidate(Provenance::Nav);
    }

    fn entry(&self) -> EntryId { self.page.entry }

    #[cfg(test)]
    fn key(&self, elem: u32) -> FocusKey<u32> { FocusKey { entry: self.entry(), elem } }

    fn view<H: MetadataLike>(&self) -> impl Focusable<H> + '_ { self.stack.view(&self.page) }

    fn store<H: MetadataLike>(&self, cmd: WatchlistCmd, fx: &mut Effects<'_, H>) {
        fx.push(Fx::App(AppFx::Store(StoreId::Watchlist, StoreCmd::Watchlist(cmd))));
    }

    fn request<H: MetadataLike>(&self, req: WatchlistReq, fx: &mut Effects<'_, H>) {
        fx.push(Fx::App(AppFx::Watchlist(req)));
    }

    fn reseat<H: MetadataLike>(&self, focus: FocusTarget<u32>, fx: &mut Effects<'_, H>) {
        fx.push(Fx::Deliver(MachineId::Instance(self.instance),
            Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus }))));
    }

    /// Re-derive the page when the store's content moved; answers whether it did.
    fn resync<H: MetadataLike>(&mut self, cx: &Cx<'_, H>) -> bool {
        let view = Page::view(cx);
        if self.page.synced == Some(view.revision()) && self.page.synced.is_some() {
            return false;
        }
        self.page.sync(view);
        // A title that left the list takes its open card with it.
        if let Some(guid) = self.card.subject() {
            if view.entry(guid).is_none() { self.card.withdraw(); }
        }
        true
    }

    /// The press on a card (`held`: a hold), resolved against the store's entry for it.
    fn press<H: MetadataLike>(&mut self, elem: u32, held: bool, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        let Some(entry) = self.page.entry_of(Page::view(cx), elem) else { return };
        match press_of(entry) {
            Press::Open { sid, rk } => {
                let guid = entry.item.guid.clone();
                self.request(if held { WatchlistReq::ItemMenu { sid, rk, guid } }
                    else { WatchlistReq::Detail { sid, rk, guid } }, fx);
            }
            Press::Card => {
                self.card.open(&entry.item);
                self.reseat(FocusTarget::ContainerGroup(card::CARD_GROUP), fx);
                fx.invalidate(Provenance::Input);
            }
            Press::Wait => {}
        }
    }

    fn strip<H: MetadataLike>(&self, elem: u32, fx: &mut Effects<'_, H>) -> Handled {
        let Some(tab) = elem.checked_sub(plx_ui::dispatch::STRIP_BASE) else { return Handled::No };
        let req = match tab {
            0 => WatchlistReq::Tab(HomeTab::Home),
            1 => WatchlistReq::Tab(HomeTab::Movies),
            2 => WatchlistReq::Tab(HomeTab::Shows),
            3 => WatchlistReq::Tab(HomeTab::Search),
            4 => WatchlistReq::Account,
            5 => return Handled::Yes,
            _ => return Handled::No,
        };
        self.request(req, fx);
        Handled::Yes
    }

    fn tick<H: MetadataLike>(&mut self, t: plx_machine::machine::Tick, cx: &Cx<'_, H>) {
        self.card.update(t.dt());
        let focused = cx.focus.current.filter(|key| key.entry == self.entry())
            .and_then(|key| self.page.elems.iter().position(|e| *e == key.elem))
            .and_then(|i| self.page.rows.get(i));
        if self.page.shield {
            // the neutral pages' fixed ground (SHIELD-LAYOUT-SPEC §9.1): no card keys it
            self.neutral.update(plx_ui::shield::ground_target(None), t.dt());
            return;
        }
        let target = PageGround::page_target(focused.or_else(|| self.page.rows.first()).map(tile_facts::of));
        if self.ground_seeded { self.ground.key_target(target, t.dt()); }
        else { self.ground.jump_target(target); self.ground_seeded = true; }
    }

    /// The focused card's store entry (the bridge's selection read for a hold or a press).
    pub fn focused_entry<'a, H: MetadataLike>(&self, focus: Option<FocusKey<u32>>, cx: &Cx<'a, H>) -> Option<&'a Entry> {
        let key = focus.filter(|key| key.entry == self.entry())?;
        self.page.entry_of(Page::view(cx), key.elem)
    }

    /// Whether the title card is up (tests and the focus probe).
    pub fn card_open(&self) -> bool { self.card.is_open() }

    /// The focused card's index in the list (the screenshot pipeline's `watchlist` arm).
    pub fn focused_index(&self, focus: Option<FocusKey<u32>>) -> Option<usize> {
        let key = focus.filter(|key| key.entry == self.entry())?;
        self.page.elems.iter().position(|e| *e == key.elem)
    }
}

impl<H: MetadataLike> Machine<H> for WatchlistScreen {
    type Ev = ScreenEvent<H>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        // The layout this event is answered in (`sync_layout`).
        self.sync_layout(fx);
        let landed = self.resync(cx);
        if landed {
            fx.invalidate(Provenance::Landing(fx.from()));
            // The list arrived under a focus nobody chose: take it to the newest title, the
            // grid's first card (not the column under the pill, which is where a geometric seat
            // from the bar would land).
            let on_pill = cx.focus.current.is_none_or(|key| key.elem >= plx_ui::dispatch::STRIP_BASE);
            if !self.moved && self.page.ready() && on_pill && !self.card.is_open() {
                if let Some(&first) = self.page.elems.first() {
                    self.reseat(FocusTarget::Elem(FocusKey { entry: self.entry(), elem: first }), fx);
                }
            }
        }
        match self.card.step(ev, cx) {
            CardStep::Pass => {}
            CardStep::Done(handled) => return handled,
            CardStep::Answer(remove) => {
                if let Some(guid) = remove {
                    self.store(WatchlistCmd::Remove { guid }, fx);
                }
                self.reseat(FocusTarget::ContainerGroup(self.page.first_group()), fx);
                fx.invalidate(Provenance::Input);
                return Handled::Yes;
            }
        }
        let card = match self.stack.on(&self.page, ev, cx, fx) {
            Some(StackEvent::Card(_, card)) => Some(card),
            _ => None,
        };
        match ev {
            ScreenEvent::Enter(_) | ScreenEvent::Uncover => {
                self.store(WatchlistCmd::Open, fx);
                fx.invalidate(Provenance::Nav);
                Handled::Yes
            }
            ScreenEvent::Tick(t) => { self.tick(*t, cx); Handled::Yes }
            ScreenEvent::FocusMoved { to, by, .. } => {
                if matches!(by, By::Dir | By::Pointer) && to.entry == self.entry() { self.moved = true; }
                fx.invalidate(Provenance::Input);
                Handled::Yes
            }
            ScreenEvent::Activate(elem) if *elem == RETRY_ELEM => {
                self.store(WatchlistCmd::Open, fx);
                Handled::Yes
            }
            ScreenEvent::Activate(elem) if *elem >= plx_ui::dispatch::STRIP_BASE => self.strip(*elem, fx),
            ScreenEvent::PressCommit(_) => {
                if let Some(CardEvent::Activate(elem)) = card { self.press(elem, false, cx, fx); }
                Handled::Yes
            }
            ScreenEvent::PressHold(_) => match card {
                Some(CardEvent::Hold(elem)) => { self.press(elem, true, cx, fx); Handled::Yes }
                _ => Handled::No,
            },
            ScreenEvent::Input(InputEvent { kind: InputKind::Key { key: Key::Back, edge: Edge::Down, .. }, .. }) => {
                self.request(WatchlistReq::Back, fx);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// The page's stack: under the Standard bar, the standing strip owns the hits under it; a page
/// that leaves focus rests at its top. The Shield layout has no bar: the page is the whole screen.
fn new_stack(entry: EntryId, shield: bool) -> Stack<Sec> {
    let floor = if shield { 0.0 } else { widgets::TOP_BAR_BOTTOM };
    Stack::new(entry).home_when_unfocused(true).clipped(Rect::new(0.0, floor, SCR_W, SCR_H - floor))
}

/// The card's answers first while it is open; the page's stack otherwise.
impl<H: MetadataLike> Focusable<H> for WatchlistScreen {
    fn groups(&self, cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        if !self.card.groups(out) { self.view::<H>().groups(cx, out); }
    }
    fn group_of(&self, key: &u32, cx: &Cx<'_, H>) -> Option<GroupId> {
        match self.card.group_of(*key) { Some(g) => g, None => self.view::<H>().group_of(key, cx) }
    }
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, cx: &Cx<'_, H>) -> Step<u32> {
        match self.card.neighbour(key, dir) { Some(step) => step, None => self.view::<H>().neighbour(key, dir, cx) }
    }
    fn place(&self, key: &u32, cx: &Cx<'_, H>, at: At) -> Option<Placed> {
        match self.card.place(*key) { Some(p) => p, None => self.view::<H>().place(key, cx, at) }
    }
    fn reconcile(&self, want: FocusKey<u32>, cx: &Cx<'_, H>) -> FocusKey<u32> {
        match self.card.reconcile(want) { Some(k) => k, None => self.view::<H>().reconcile(want, cx) }
    }
    fn seat(&self, g: GroupId, from: Placed, cx: &Cx<'_, H>) -> FocusKey<u32> {
        match self.card.seat(g, self.entry()) { Some(k) => k, None => self.view::<H>().seat(g, from, cx) }
    }
}

impl<H: MetadataLike> Screen<H> for WatchlistScreen {
    fn focused_card<'a>(&self, cx: &Cx<'a, H>, focus: Option<FocusKey<u32>>, at: Option<At>) -> Option<plx_ui::screen::FocusedCard<'a>> {
        let entry = self.focused_entry(focus, cx)?;
        let placed = focus.zip(at).and_then(|(key, at)| Focusable::<H>::place(self, &key.elem, cx, at));
        Some(plx_ui::screen::FocusedCard::new(entry, placed))
    }
    fn redraw_focused(&self, f: &mut DrawFrame<'_, '_, H>, focus: Option<FocusKey<u32>>) {
        self.stack.view(&self.page).redraw_focused(f, focus);
    }
    fn name(&self) -> &'static str { crate::registry::word::WATCHLIST }
    fn state(&self) -> &dyn LogicalState { self }
    fn crumb(&self, _cx: &Cx<'_, H>) -> Option<std::borrow::Cow<'_, str>> { None }
    fn prepare(&mut self, _budget: &mut plx_ui::frame::Budget, _cx: &Cx<'_, H>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        let p = f.painter.alpha(f.page_alpha);
        if self.page.shield { self.neutral.draw(p) } else { self.ground.draw(p, Rect::FULL) }
        self.stack.view(&self.page).paint(f);
        let entry = self.entry();
        self.card.draw(f, entry);
    }
    fn render(&self) -> RenderStrategy { RenderStrategy::Page }
    fn focus_source(&self) -> FocusSource { FocusSource::Engine }
    fn hit_source(&self) -> HitSource { HitSource::Engine }
    fn links(&self, out: &mut Vec<Link>) {
        if self.card.is_open() { return; }
        let first = self.page.first_group();
        out.push(Link { from: STRIP, dir: Dir::Down, to: first });
        out.push(Link { from: first, dir: Dir::Up, to: STRIP });
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> { Some(self) }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> { Some(self) }
}

