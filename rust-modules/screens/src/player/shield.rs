//! **The player in the Shield layout** (`plx_ui::layout::shield()`, latched by [`PlayerScreen`] at
//! the top of every event): the Plex-style transport, the Skip pill drawn on its own, and the
//! post-play page — the presentation and the input routing over the SAME playback state, gesture,
//! cursor and requests the Standard transport uses. Nothing here touches the engine: every action
//! is a [`PlayerReq`] the loop performs exactly as it does for the Standard controls.
//!
//! **The transport's focus** is the Standard `HudNav`, read in Shield terms: `focus` 0 is the bar
//! (LEFT/RIGHT hop, tap-burst and hold-scrub as ever; OK toggles play and pause), 1 the two round
//! buttons over its right end (`btn` 0 Subtitles, 1 Settings; UP from the bar reaches them, UP again
//! hides the transport as before), 2 the Info / Chapters pills under the chevron (DOWN from the bar).
//! OK on Subtitles opens the track menu on its Subtitles tab, on Settings the Settings panel
//! (`plx_appkit::player_settings`), both as right-side panels; every other control-row action of the
//! Standard transport — Audio, Quality, Stats for nerds — is behind Settings.
//!
//! **The Skip pill** ([`SKIP_SHOW_MS`]) stands alone at the bottom right while the transport is
//! down, for the first 15 s of a segment; the transport coming up hides it (the reference's p01),
//! and OK while it shows skips through the control row's own activation, so Manual / Automatic /
//! Off and a final credits segment's "the episode is over" keep their Standard meaning. A new
//! segment (a final credits marker after an intro) is a new 15 s.
//!
//! **The post-play page** is the Shield form of the Up Next offer, mapped mode by mode:
//! - *Countdown* (the default): the page appears when the Standard Up Next tile would — a final
//!   credits marker with a successor queued — with the Up Next card focused and the same
//!   `up_next::Countdown` running its heading; the cancel rule is the Standard one (focus off the
//!   Up Next card cancels, for the segment), and expiry starts the successor through the loop's
//!   own countdown path, Passout protection included. If the stream ends while the page is up and
//!   the countdown still runs, the end advances at once as it does today; if the viewer cancelled
//!   it, the page stays instead ([`EosHold`]).
//! - *After credits*: nothing during the credits; at the end of the stream the page is held over
//!   the ended picture and arms the same countdown, whose expiry advances (Passout protection is
//!   asked at the end and again at expiry, so an empty room never sees it).
//! - *Off*: no page; the end of the stream leaves the player as before.
//!
//! Its cards: *Just Played* replays the ended episode, *Up Next* starts the successor, an *On Deck*
//! card (Continue Watching) plays that item; BACK leaves the player to its origin as ever.
use plx_appkit::player_hud::ControlSlot;
use plx_appkit::shield_hud::{self, PostPlayModel};
use plx_machine::machine::{Cx, Effects, FocusKey, GroupId, Handled};
use plx_ui::screen::{AxisMask, EdgeRule, GroupKind, GroupSpec, Placed, Seat};
use plx_ui::shield_player::{self, Focus, PostFocus};
use plx_ui::Rect;

use super::input::{HudNav, HUD_LINGER_MS};
use super::{overlay, PlayerScreen};
use crate::registry::{AppLike, PlayerLike, PlayerReq, PostPlayReq};
use plx_ui::consts;

/// How long the Skip pill stands on its own after its segment is offered.
pub const SKIP_SHOW_MS: u32 = 15_000;
/// The Skip pill's element, and the post-play page's: Just Played, Up Next, then the deck.
pub const ELEM_SKIP: u32 = 50;
pub const ELEM_POST_JUST: u32 = 60;
pub const ELEM_POST_NEXT: u32 = 61;
pub const ELEM_DECK_BASE: u32 = 62;
pub const ELEM_DECK_END: u32 = ELEM_DECK_BASE + plx_appkit::shield_hud::DECK_MAX as u32;
/// Their groups.
pub const GROUP_SKIP: GroupId = GroupId(5);
pub const GROUP_POST: GroupId = GroupId(6);
pub const GROUP_DECK: GroupId = GroupId(7);
/// `HudNav::focus` on the post-play page's On Deck row (1 is its two leading cards).
pub const FOCUS_DECK: i32 = 3;

/// The post-play page's state on the player's instance.
#[derive(Default)]
pub struct PostPlay {
    /// Held over the ended stream (After credits, or a cancelled countdown at the end).
    pub held: bool,
    /// On screen at the last Tick — the edge that seats and releases its focus.
    pub shown: bool,
    /// The focused On Deck card and the row's scroll.
    pub deck: usize,
    pub scroll: f32,
    /// What the page shows, re-read each Tick while it is up (a render cache).
    pub model: Option<PostPlayModel>,
}

/// The Skip pill's clock: the segment it was last offered for, and when.
#[derive(Default)]
pub struct SkipPill {
    pub offer: Option<(plx_data::metadata::MarkerKind, i64)>,
    pub since: u32,
}

/// What the end of the stream does in the Shield layout ([`eos_hold`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EosHold {
    /// Today's end: the loop's `finish_playback` (advance, or leave).
    Finish,
    /// The page is already held: nothing more.
    Hold,
    /// Hold the page now (after asking Passout protection).
    Open,
}

/// PURE: the end of the stream under the Shield layout. `shown`/`held` are the page's, `armed`
/// whether its countdown runs. Standard (`!shield`), no successor and *Off* are today's end.
pub fn eos_hold(shield: bool, mode: plx_media::route::NextEpisodeMode, has_next: bool, shown: bool, held: bool,
    armed: bool) -> EosHold {
    use plx_media::route::NextEpisodeMode;
    if !shield || !has_next || mode == NextEpisodeMode::Off {
        return EosHold::Finish;
    }
    if held {
        return EosHold::Hold;
    }
    match (shown, armed, mode) {
        // the countdown was still running when the credits ran out: today's immediate advance
        (true, true, _) => EosHold::Finish,
        // the viewer cancelled it: the page stays, the choice theirs
        (true, false, _) => EosHold::Open,
        (false, _, NextEpisodeMode::AfterCredits) => EosHold::Open,
        _ => EosHold::Finish,
    }
}

impl PlayerScreen {
    /// Is the Shield layout live on this instance (latched at the top of the last event)?
    pub fn shield(&self) -> bool {
        self.shield
    }

    /// Latch the layout at the top of an event; a change re-dresses the transport in place.
    pub(super) fn latch_shield<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        let shield = plx_ui::layout::shield();
        if shield != self.shield {
            self.shield = shield;
            self.hud.nav = HudNav::HOME;
            self.post = PostPlay::default();
            fx.invalidate(plx_machine::present::Provenance::Nav);
        }
    }

    /// How many items the control row walks: the Shield transport's two round buttons always,
    /// else whatever occupies the Standard row.
    pub(super) fn row_items(&self) -> i32 {
        if self.shield { 2 } else { self.slot.items() }
    }

    /// Is the post-play page on screen this frame?
    pub fn post_play_up(&self, ps: &plx_media::route::PlaybackSession) -> bool {
        self.shield
            && plx_media::route::up_next(ps).is_some()
            && (matches!(self.slot, ControlSlot::UpNext(_)) || self.post.held)
            && !plx_appkit::player_hud::readout_owns_frame(self.busy)
    }

    /// Does the Up Next countdown's offer stand: the row's occupant, or the held page.
    pub(super) fn offers_up_next(&self) -> bool {
        plx_appkit::up_next::is_shown(self.slot) || (self.shield && self.post.held)
    }

    /// The end of the stream, asked by the loop ([`eos_hold`] over this instance's state).
    pub fn eos_hold(&self, ps: &plx_media::route::PlaybackSession) -> EosHold {
        eos_hold(self.shield, plx_media::route::next_episode_mode(), plx_media::route::up_next(ps).is_some(),
            self.post.shown, self.post.held, self.up_next.armed())
    }

    /// **Hold the post-play page over the ended stream** — the loop's answer to [`EosHold::Open`]:
    /// the page comes up (if it is not already) with the Up Next card focused, and its countdown
    /// arms on the next Tick unless the viewer had cancelled it.
    pub fn hold_post_play(&mut self, now: u32) {
        self.post.held = true;
        if !self.post.shown {
            self.hud.nav = HudNav { focus: 1, btn: plx_appkit::up_next::BTN_NEXT, tab: 0 };
        }
        self.hud.dismissed = false;
        self.hud.extend(now, HUD_LINGER_MS);
        self.publish();
    }

    /// **A segment was offered** (the loop's fresh-offer edge): the Standard transport comes up with
    /// the ring on the offer; the Shield Skip pill stands alone instead, and the post-play page
    /// seats its own focus as it appears.
    pub fn raise_for_offer(&mut self, now: u32, slot: ControlSlot) {
        if self.shield {
            return;
        }
        self.hud.raise_for_offer(now, slot.primary_btn());
    }

    /// Is the Skip pill inside its first [`SKIP_SHOW_MS`]?
    pub(super) fn skip_in_window(&self, now: u32) -> bool {
        matches!(self.slot, ControlSlot::Skip(_)) && self.skip.offer.is_some()
            && now.wrapping_sub(self.skip.since) < SKIP_SHOW_MS
    }

    /// The Shield half of the Tick: the clock, the Skip pill's offer clock and the post-play page's
    /// edges (its focus seated as it appears and released as it goes, the transport kept up while
    /// it stands so the loop's auto-hide never re-parks its focus).
    pub(super) fn shield_tick<H: PlayerLike + crate::registry::MetadataLike>(&mut self, cx: &Cx<'_, H>, now: u32) {
        if !self.shield {
            return;
        }
        let ps = H::session(cx);
        self.clock.refresh(cx.measure);
        let offer = match self.slot {
            ControlSlot::Skip(pr) => Some((pr.marker.kind, pr.marker.start_ms)),
            _ => None,
        };
        if offer != self.skip.offer {
            self.skip.offer = offer;
            self.skip.since = now;
        }
        if self.post.held && !plx_media::player::ended() {
            self.post.held = false; // a replay or another start took the stream back
        }
        let up = self.post_play_up(ps);
        if up && !self.post.shown {
            self.hud.nav = HudNav { focus: 1, btn: plx_appkit::up_next::BTN_NEXT, tab: 0 };
            self.post.deck = 0;
            self.post.scroll = 0.0;
        } else if !up && self.post.shown {
            self.hud.nav = HudNav::HOME;
        }
        self.post.shown = up;
        if up {
            self.hud.dismissed = false;
            self.hud.extend(now, HUD_LINGER_MS);
            // replaced only when it changed: a card's artwork is admitted by the identity of its
            // path, which a rebuild every frame would make new every frame
            let model = shield_hud::post_play_model(ps, H::metadata(cx), H::on_deck_hubs(cx));
            if model != self.post.model {
                self.post.model = model;
            }
            let n = self.deck_len();
            if n > 0 && self.post.deck >= n {
                self.post.deck = n - 1;
            }
        } else {
            self.post.model = None;
        }
    }

    fn deck_len(&self) -> usize {
        self.post.model.as_ref().map_or(0, |m| m.deck.len())
    }

    /// The post-play page's focus, from the cursor.
    pub(super) fn post_focus(&self) -> PostFocus {
        match (self.hud.nav.focus, self.hud.nav.btn) {
            (FOCUS_DECK, _) => PostFocus::Deck(self.post.deck),
            (_, b) if b == plx_appkit::up_next::BTN_CREDITS => PostFocus::JustPlayed,
            _ => PostFocus::UpNext,
        }
    }

    /// The transport's focus, from the cursor.
    pub(super) fn transport_focus(&self) -> Focus {
        match self.hud.nav.focus {
            1 => Focus::Button(self.hud.nav.btn.clamp(0, 1) as usize),
            2 => Focus::Tabs(self.hud.nav.tab.max(0) as usize),
            _ => Focus::Bar,
        }
    }

    /// OK on a round button: its panel over the transport.
    fn open_button<H: AppLike>(&mut self, btn: i32, now: u32, fx: &mut Effects<'_, H>) {
        let kind = if btn == 0 { overlay::OverlayKind::Tracks { tab: 1 } } else { overlay::OverlayKind::Settings };
        Self::ask(fx, PlayerReq::OpenOverlay(kind));
        self.hud.extend(now, HUD_LINGER_MS);
        self.publish();
    }

    /// OK on the post-play page's focused card.
    fn post_ok<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        let req = match self.post_focus() {
            PostFocus::UpNext => PostPlayReq::PlayNext,
            PostFocus::JustPlayed => PostPlayReq::Replay,
            PostFocus::Deck(i) => {
                let Some(card) = self.post.model.as_ref().and_then(|m| m.deck.get(i)) else { return };
                PostPlayReq::Play { sid: plx_plex::plex::ServerId::from_raw(card.sid), rk: card.rk.clone() }
            }
        };
        Self::ask(fx, PlayerReq::PostPlay(req));
    }

    /// A direction on the post-play page: LEFT/RIGHT between the two leading cards or along the
    /// deck, DOWN to the deck and UP back to the card above it.
    fn post_move(&mut self, key: consts::Key) {
        use consts::Key;
        let n = self.deck_len();
        match (self.hud.nav.focus, key) {
            (FOCUS_DECK, Key::Up) => {
                self.hud.nav.focus = 1;
                self.hud.nav.btn = plx_appkit::up_next::BTN_CREDITS;
            }
            (FOCUS_DECK, Key::Left { .. }) => self.post.deck = self.post.deck.saturating_sub(1),
            (FOCUS_DECK, Key::Right { .. }) => self.post.deck = (self.post.deck + 1).min(n.saturating_sub(1)),
            (_, Key::Down) if n > 0 => {
                self.hud.nav.focus = FOCUS_DECK;
                self.post.deck = self.post.deck.min(n - 1);
            }
            (_, Key::Left { .. }) => self.hud.nav.btn = plx_appkit::up_next::BTN_CREDITS,
            (_, Key::Right { .. }) => self.hud.nav.btn = plx_appkit::up_next::BTN_NEXT,
            _ => {}
        }
        if self.hud.nav.focus == FOCUS_DECK {
            self.post.scroll = shield_player::deck_scroll_for(self.post.deck, self.post.scroll);
        }
        plx_machine::idle::invalidate();
    }

    /// **The Shield key routes**, ahead of the Standard ladder: the post-play page owns the
    /// directions and OK while it is up; OK on the bare picture while the Skip pill stands skips;
    /// OK on a round button opens its panel. `None` falls through to the Standard ladder, which
    /// keeps every other key (the bar's scrub, UP/DOWN, play/pause, BACK).
    pub(super) fn shield_key<H: AppLike>(&mut self, key: consts::Key, edge: plx_machine::machine::Edge, now: u32,
        fx: &mut Effects<'_, H>) -> Option<Handled> {
        use consts::Key;
        use plx_machine::machine::Edge;
        if self.post.shown {
            return match key {
                Key::Up | Key::Down | Key::Left { .. } | Key::Right { .. } => {
                    if edge != Edge::Up {
                        self.post_move(key);
                    }
                    Some(Handled::Yes)
                }
                Key::Ok => {
                    if edge == Edge::Down {
                        self.post_ok(fx);
                    }
                    Some(Handled::Yes)
                }
                _ => None,
            };
        }
        if !matches!(key, Key::Ok) || edge != Edge::Down {
            return None;
        }
        let vis = self.hud.visible_at_press;
        if !vis && self.skip_in_window(now) {
            // the pill alone: OK is its press, through the control row's own activation
            self.hud.nav = HudNav { focus: 1, btn: 0, tab: self.hud.nav.tab };
            Self::ask(fx, PlayerReq::ArmControlRow);
            return Some(Handled::Yes);
        }
        if vis && self.hud.nav.focus == 1 {
            self.open_button(self.hud.nav.btn, now, fx);
            return Some(Handled::Yes);
        }
        None
    }

    /// **A click on a Shield control** — the pointer twin of [`Self::shield_key`]. `None` leaves
    /// the element to the Standard arms (the bar, the pills, the failure row).
    pub(super) fn shield_click<H: AppLike>(&mut self, elem: u32, now: u32, fx: &mut Effects<'_, H>) -> Option<Handled> {
        match elem {
            e if (plx_appkit::player_hud::ELEM_ROW_BASE..plx_appkit::player_hud::ELEM_ROW_BASE + 2).contains(&e) => {
                let btn = (e - plx_appkit::player_hud::ELEM_ROW_BASE) as i32;
                self.hud.nav = HudNav { focus: 1, btn, tab: self.hud.nav.tab };
                self.open_button(btn, now, fx);
                Some(Handled::Yes)
            }
            ELEM_SKIP => {
                self.hud.nav = HudNav { focus: 1, btn: 0, tab: self.hud.nav.tab };
                Self::ask(fx, PlayerReq::ArmControlRow);
                Some(Handled::Yes)
            }
            ELEM_POST_JUST | ELEM_POST_NEXT => {
                self.hud.nav.focus = 1;
                self.hud.nav.btn = if elem == ELEM_POST_NEXT {
                    plx_appkit::up_next::BTN_NEXT
                } else {
                    plx_appkit::up_next::BTN_CREDITS
                };
                self.post_ok(fx);
                Some(Handled::Yes)
            }
            e if (ELEM_DECK_BASE..ELEM_DECK_END).contains(&e) => {
                self.hud.nav.focus = FOCUS_DECK;
                self.post.deck = (e - ELEM_DECK_BASE) as usize;
                self.post_ok(fx);
                Some(Handled::Yes)
            }
            _ => None,
        }
    }

    /// The Shield transport's groups, in pointer z-order (the bar's band first) — or, while the
    /// post-play page is up, only its cards.
    pub(super) fn shield_groups<H: PlayerLike + crate::registry::MetadataLike>(&self, cx: &Cx<'_, H>,
        out: &mut Vec<GroupSpec>) {
        let spec = |id, len: usize, extent| GroupSpec {
            id,
            kind: GroupKind::Row { wrap: false },
            seat: Seat::First,
            reachable: AxisMask::BOTH,
            edge: [EdgeRule::Screen; 4],
            extent,
            len,
            elem: plx_ui::screen::ElemKind::Control,
        };
        if self.post.shown {
            out.push(spec(GROUP_POST, 2, shield_player::just_played_rect().union(shield_player::up_next_rect())));
            let n = self.deck_len();
            if n > 0 {
                let extent = shield_player::deck_rect(0, 0.0).union(shield_player::deck_rect(n - 1, 0.0));
                out.push(spec(GROUP_DECK, n, extent));
            }
            return;
        }
        out.push(GroupSpec { kind: GroupKind::Free, len: 1, ..spec(super::GROUP_SCRUB, 1, self.grab_rect()) });
        out.push(spec(super::GROUP_ROW, 2, shield_player::button_rect(0).union(shield_player::button_rect(1))));
        let has_ch = plx_appkit::chapters_panel::has_chapters(H::metadata(cx));
        let n = if has_ch { 2 } else { 1 };
        let tabs = (0..n).filter_map(|i| self.tab_rect(i, has_ch, cx.measure)).reduce(|a, b| a.union(b));
        out.push(spec(super::GROUP_TABS, n, tabs.unwrap_or(shield_player::chevron_rect())));
        out.push(spec(GROUP_SKIP, 1, self.skip_rect(cx.measure).unwrap_or(Rect::FULL)));
    }

    /// The bar's pointer band, at the span the last draw measured.
    fn grab_rect(&self) -> Rect {
        shield_player::grab_rect(self.bar_span.0, self.bar_span.1)
    }

    /// Pill `i` under the bar — the pills while focus is on them, else the chevron standing for
    /// Info.
    fn tab_rect(&self, i: usize, has_ch: bool, measure: &dyn plx_machine::machine::Measure) -> Option<Rect> {
        if self.hud.nav.focus != 2 {
            return (i == 0).then(|| shield_player::chevron_rect().inset(-12.0));
        }
        let labels = if has_ch {
            vec![plx_platform::i18n::msg::widgets_player_info_c(), plx_platform::i18n::msg::widgets_player_chapters_c()]
        } else {
            vec![plx_platform::i18n::msg::widgets_player_info_c()]
        };
        let widths: Vec<f32> = labels.iter().map(|l| measure.width(l, plx_ui::shield_browse::TAB_SIZE, false)).collect();
        shield_player::tab_rects(&widths).get(i).copied()
    }

    fn skip_rect(&self, measure: &dyn plx_machine::machine::Measure) -> Option<Rect> {
        match self.slot {
            ControlSlot::Skip(pr) => Some(shield_hud::skip_rect(pr, measure)),
            _ => None,
        }
    }

    /// Where a Shield element is: the bar's band, a round button, a pill, the Skip pill or a
    /// post-play card. `None` leaves it to the Standard geometry (the failure row).
    pub(super) fn shield_place<H: PlayerLike + crate::registry::MetadataLike>(&self, key: u32, cx: &Cx<'_, H>)
        -> Option<Option<Placed>> {
        use plx_appkit::player_hud::{ELEM_FAILURE_BASE, ELEM_ROW_BASE, ELEM_SCRUB, ELEM_TAB_BASE};
        let placed = |rect: Rect| Some(Placed { rect, rest_rect: rect, clip: Rect::FULL, index: None });
        Some(match key {
            ELEM_SCRUB => placed(self.grab_rect()),
            // two round buttons, whatever occupies the Standard row
            e if (ELEM_ROW_BASE..ELEM_TAB_BASE).contains(&e) => {
                (e < ELEM_ROW_BASE + 2).then(|| shield_player::button_rect((e - ELEM_ROW_BASE) as usize)).and_then(placed)
            }
            e if (ELEM_TAB_BASE..ELEM_FAILURE_BASE).contains(&e) => {
                let has_ch = plx_appkit::chapters_panel::has_chapters(H::metadata(cx));
                self.tab_rect((e - ELEM_TAB_BASE) as usize, has_ch, cx.measure).and_then(placed)
            }
            ELEM_SKIP => self.skip_rect(cx.measure).and_then(placed),
            ELEM_POST_JUST => placed(shield_player::just_played_rect()),
            ELEM_POST_NEXT => placed(shield_player::up_next_rect()),
            e if (ELEM_DECK_BASE..ELEM_DECK_END).contains(&e) => {
                let i = (e - ELEM_DECK_BASE) as usize;
                (i < self.deck_len()).then(|| shield_player::deck_rect(i, self.post.scroll)).and_then(placed)
            }
            _ => return None,
        })
    }

    /// The group a Shield-only element belongs to.
    pub(super) fn shield_group_of(key: u32) -> Option<GroupId> {
        match key {
            ELEM_SKIP => Some(GROUP_SKIP),
            ELEM_POST_JUST | ELEM_POST_NEXT => Some(GROUP_POST),
            e if (ELEM_DECK_BASE..ELEM_DECK_END).contains(&e) => Some(GROUP_DECK),
            _ => None,
        }
    }

    /// Element `i` of a Shield-only group.
    pub(super) fn shield_elem_of(g: GroupId, i: u32) -> Option<u32> {
        match g {
            GROUP_SKIP => Some(ELEM_SKIP),
            GROUP_POST => Some(ELEM_POST_JUST + i),
            GROUP_DECK => Some(ELEM_DECK_BASE + i),
            _ => None,
        }
    }

    /// **The Shield frame**: the post-play page over everything while it is up; else the
    /// transport while it is drawn (its middle given back to the Standard band under an Info card
    /// or a Chapters strip), else the Skip pill on its own.
    pub(super) fn draw_shield<H: PlayerLike + crate::registry::MetadataLike>(&mut self, cx: &Cx<'_, H>,
        measure: &dyn plx_machine::machine::Measure, now: u32, hud_drawn: bool) {
        let ps = H::session(cx);
        let meta = H::metadata(cx);
        self.skip_drawn = false;
        if self.post.shown {
            if let Some(model) = &self.post.model {
                shield_hud::draw_post_play(model, self.up_next.seconds_left(now), self.post_focus(), self.post.scroll, measure);
            }
            return;
        }
        if hud_drawn {
            if self.transport {
                self.bar_span = shield_player::bar_span(plx_media::player::duration_ns() / 1_000_000, measure);
                let clock = self.clock.reading().map(|(text, _)| text);
                shield_hud::draw_transport(ps, self.busy, self.transport_focus(),
                    plx_appkit::chapters_panel::has_chapters(meta), clock, now, measure, meta);
            } else {
                self.draw_hud(ps, now, measure, meta);
            }
            return;
        }
        if self.skip_in_window(now) && !plx_appkit::player_hud::readout_owns_frame(self.busy) {
            if let ControlSlot::Skip(pr) = self.slot {
                shield_hud::draw_skip(pr, measure);
                self.skip_drawn = true;
            }
        }
    }

    /// Is a Shield-only group on screen this frame?
    pub(super) fn shield_group_drawn(&self, g: GroupId) -> Option<bool> {
        let failed = plx_appkit::player_hud::readout_owns_frame(self.busy);
        match g {
            GROUP_SKIP => Some(self.skip_drawn),
            GROUP_POST | GROUP_DECK => Some(self.post.shown && !failed),
            _ if self.post.shown => Some(false),
            _ => None,
        }
    }

    /// The Shield terms of [`PlayerScreen::clock_fingerprint`]: the page and its countdown's
    /// seconds, the Skip pill's 15 s, and the wall clock's minute while the transport shows it.
    pub(super) fn shield_fingerprint(&self, hud_up: bool, now: u32) -> u64 {
        if !self.shield {
            return 0;
        }
        let mut h = u64::from(self.post.shown) | u64::from(self.post.held) << 1 | u64::from(self.skip_in_window(now)) << 2;
        if self.post.shown {
            h ^= u64::from(self.up_next.seconds_left(now).unwrap_or(u32::MAX)) << 8;
        }
        if hud_up {
            h ^= u64::from(self.clock.minute_key()) << 40;
        }
        h
    }

    /// The replay canon's Shield terms, written only in the Shield layout so a Standard recording
    /// keeps its shape.
    pub(super) fn write_shield(&self, c: &mut plx_machine::machine::Canon) {
        if self.shield {
            c.bool(self.post.held).bool(self.post.shown).u32(self.post.deck as u32);
        }
    }

    /// The post-play page's element for its cursor (what a test reads).
    pub fn post_focus_key(&self) -> FocusKey<u32> {
        let elem = match self.post_focus() {
            PostFocus::JustPlayed => ELEM_POST_JUST,
            PostFocus::UpNext => ELEM_POST_NEXT,
            PostFocus::Deck(i) => ELEM_DECK_BASE + i as u32,
        };
        FocusKey { entry: self.entry, elem }
    }
}
