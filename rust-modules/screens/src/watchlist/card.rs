//! **The card a Watchlist title no server holds opens**, its title, year and summary, and the
//! one thing the viewer can do with it from here: take it off the Watchlist.
//!
//! It is the shared `ui::decision_alert` sheet (`open_card_titled`: a provider-named title over
//! body paragraphs, two answers), hosted by the Watchlist page the way
//! `plaintext_question::PlaintextAlert` hosts its question on the pages that ask it: while it is
//! open its two answers are the page's only focus group, BACK answers *Close*, and a fading card
//! swallows every press so nothing under it fires through. *Close* is the seated answer, so an OK
//! held through the press that opened the card never removes anything.

use std::borrow::Cow;
use std::cell::Cell;

use plx_machine::machine::{Cx, Edge, EntryId, FocusKey, GroupId, Handled, Host, InputEvent, InputKind, Key};
use plx_ui::decision_alert::{Answers, Choice, DecisionAlert, Tone};
use plx_ui::screen::{
    Activate, AxisMask, Dir, DrawFrame, EdgeRule, ElemKind, GroupKind, GroupSpec, Hover, Placed,
    ScreenEvent, Seat, Step, Stop,
};
use plx_ui::{Painter, Rect};

pub(super) const CARD_GROUP: GroupId = GroupId(0xF0);
pub(super) const CLOSE_ELEM: u32 = 0x0F00;
pub(super) const REMOVE_ELEM: u32 = 0x0F01;

/// What [`TitleCard::step`] made of an event.
pub(super) enum CardStep {
    /// Not the card's: the page handles it.
    Pass,
    /// The card decided it.
    Done(Handled),
    /// The person answered: `Some(guid)` to remove that title, `None` for *Close*/BACK.
    Answer(Option<String>),
}

pub(super) struct TitleCard {
    alert: DecisionAlert,
    /// The title the open card is about.
    guid: Option<String>,
    frames: Cell<Option<(Rect, Rect)>>,
}

/// The card's heading: the title, with its year when the provider gave one.
pub(super) fn heading(title: &str, year: i64) -> String {
    if year > 0 { format!("{title} ({year})") } else { title.to_string() }
}

impl TitleCard {
    pub(super) fn new() -> Self {
        let mut alert = DecisionAlert::new();
        alert.set_tone(Tone::Destructive);
        Self { alert, guid: None, frames: Cell::new(None) }
    }

    pub(super) fn is_open(&self) -> bool { self.alert.is_open() }
    pub(super) fn visible(&self) -> bool { self.alert.visible() }
    pub(super) fn subject(&self) -> Option<&str> { self.guid.as_deref().filter(|_| self.is_open()) }
    pub(super) fn owns(elem: u32) -> bool { elem == CLOSE_ELEM || elem == REMOVE_ELEM }

    /// Open on a title: heading, the line that says why it cannot be played here, the summary.
    pub(super) fn open(&mut self, item: &plx_plex::plex::discover::WatchItem) {
        let mut body: Vec<Cow<'static, str>> = vec![Cow::Borrowed(plx_platform::i18n::msg::browse_watchlist_missing_note())];
        if !item.summary.trim().is_empty() {
            body.push(Cow::Owned(item.summary.trim().to_string()));
        }
        self.alert.open_card_titled(heading(&item.title, item.year), body, Answers::Two);
        self.guid = Some(item.guid.clone());
    }

    /// The title went off the list under the open card: take it down unanswered.
    pub(super) fn withdraw(&mut self) {
        if self.guid.take().is_some() && self.alert.is_open() {
            self.alert.close();
        }
    }

    pub(super) fn update(&mut self, dt: f32) { self.alert.update(dt); }

    fn answer(&mut self, remove: bool) -> Option<String> {
        self.alert.dismiss();
        let guid = self.guid.take()?;
        remove.then_some(guid)
    }

    /// Route one page event through the card first (see the module doc).
    pub(super) fn step<H: Host<Elem = u32>>(&mut self, ev: &ScreenEvent<H>, cx: &Cx<'_, H>) -> CardStep {
        match ev {
            ScreenEvent::FocusMoved { to, .. } if self.is_open() && Self::owns(to.elem) => {
                self.alert.set_choice(if to.elem == REMOVE_ELEM { Choice::Destructive } else { Choice::Cancel });
                CardStep::Done(Handled::Yes)
            }
            ScreenEvent::Activate(_) if self.visible() => CardStep::Done(Handled::Yes),
            ScreenEvent::PressCommit(_) | ScreenEvent::PressHold(_) if self.visible() => {
                match cx.focus.current.map(|k| k.elem) {
                    Some(elem) if self.is_open() && Self::owns(elem) && matches!(ev, ScreenEvent::PressCommit(_)) =>
                        CardStep::Answer(self.answer(elem == REMOVE_ELEM)),
                    _ => CardStep::Done(Handled::Yes),
                }
            }
            ScreenEvent::Input(InputEvent { kind: InputKind::Key { key, edge, .. }, .. }) if self.visible() => {
                if !self.is_open() {
                    return CardStep::Done(Handled::Yes);
                }
                match key {
                    Key::Back if *edge == Edge::Down => CardStep::Answer(self.answer(false)),
                    Key::Back | Key::Up | Key::Down => CardStep::Done(Handled::Yes),
                    _ => CardStep::Done(Handled::No),
                }
            }
            _ => CardStep::Pass,
        }
    }

    fn rect(&self, elem: u32) -> Rect {
        self.frames.get().map(|(close, remove)| if elem == REMOVE_ELEM { remove } else { close })
            .unwrap_or(Rect::FULL)
    }

    /// `Focusable::groups`: while open, the answers are the ONLY group (`true`).
    pub(super) fn groups(&self, out: &mut Vec<GroupSpec>) -> bool {
        if !self.is_open() {
            return false;
        }
        out.push(GroupSpec {
            id: CARD_GROUP, kind: GroupKind::Row { wrap: false }, seat: Seat::First,
            reachable: AxisMask::BOTH, edge: [EdgeRule::Stop; 4],
            extent: self.rect(CLOSE_ELEM).union(self.rect(REMOVE_ELEM)), len: 2, elem: ElemKind::Control,
        });
        true
    }

    pub(super) fn group_of(&self, key: u32) -> Option<Option<GroupId>> {
        self.is_open().then(|| Self::owns(key).then_some(CARD_GROUP))
    }

    pub(super) fn neighbour(&self, key: FocusKey<u32>, dir: Dir) -> Option<Step<u32>> {
        if !self.is_open() {
            return None;
        }
        Some(match (key.elem, dir) {
            (CLOSE_ELEM, Dir::Right) => Step::Move(FocusKey { entry: key.entry, elem: REMOVE_ELEM }),
            (REMOVE_ELEM, Dir::Left) => Step::Move(FocusKey { entry: key.entry, elem: CLOSE_ELEM }),
            _ => Step::Edge,
        })
    }

    pub(super) fn place(&self, key: u32) -> Option<Option<Placed>> {
        if !self.is_open() {
            return None;
        }
        Some(Self::owns(key).then(|| {
            let rect = self.rect(key);
            Placed { rect, rest_rect: rect, clip: Rect::FULL, index: Some((key == REMOVE_ELEM) as u32) }
        }))
    }

    pub(super) fn reconcile(&self, want: FocusKey<u32>) -> Option<FocusKey<u32>> {
        if !self.is_open() {
            return None;
        }
        if Self::owns(want.elem) {
            return Some(want);
        }
        Some(FocusKey { entry: want.entry, elem: match self.alert.choice() {
            Choice::Cancel => CLOSE_ELEM,
            Choice::Destructive => REMOVE_ELEM,
        } })
    }

    pub(super) fn seat(&self, g: GroupId, entry: EntryId) -> Option<FocusKey<u32>> {
        (g == CARD_GROUP).then_some(FocusKey { entry, elem: CLOSE_ELEM })
    }

    /// Draw over the page, and register the answers once the sheet has settled.
    pub(super) fn draw<H: Host<Elem = u32>>(&mut self, f: &mut DrawFrame<'_, '_, H>, entry: EntryId) {
        if !self.alert.visible() {
            return;
        }
        self.alert.draw_scrim();
        self.alert.draw(plx_platform::i18n::msg::browse_watchlist_close_c(),
            plx_platform::i18n::msg::browse_watchlist_remove_c(), f.measure);
        let frames = self.alert.frames(f.measure);
        self.frames.set(Some(frames));
        if self.alert.is_open() && self.alert.settled() {
            for (elem, rect) in [(CLOSE_ELEM, frames.0), (REMOVE_ELEM, frames.1)] {
                f.stop(Painter::root(), Stop {
                    key: FocusKey { entry, elem }, rect, rest_rect: rect, clip: Rect::FULL,
                    hover: Hover::Focus, activate: Activate::Press,
                });
            }
        }
    }

    #[cfg(test)]
    pub(super) fn title_for_test(&self) -> &str { self.alert.title() }
}
