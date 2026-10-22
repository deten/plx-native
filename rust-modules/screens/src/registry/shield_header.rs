//! **The Shield layout's focused-item header** (SHIELD-LAYOUT-SPEC §4.1, §4.4, §9.1): the subject
//! a Home-like page describes above its poster rows — Home's, and the Library's Recommended tab's.
//! `plx_ui::shield` draws it (`draw_hero`, `HeroGround`); this is the half that names the catalog
//! row, which `ui` cannot: which item is the subject and when, what its facts say, and which art
//! the ground asks the source for.
//!
//! It lives here, in the registry every screen may reach, because two screens wear it and a
//! screen never names a sibling (`screens/CLAUDE.md`, the layer rule). Paint state for both: no
//! focus, reveal or press decision reads it.
//!
//! - **A subject that settles.** Focus must REST on a card for [`SETTLE_MS`] before it becomes the
//!   subject (the first one is taken at once), so a held key running along a row asks the server
//!   for no backdrop or logo it passes; a settled subject warms the next card's logo and backdrop
//!   while the source is idle ([`Header::warm`]).
//! - **What it says** ([`Facts`]): an episode or a season shows its SHOW's logo (its title until the
//!   logo lands) over its own name; the facts follow the measured rows (an episode's address, air
//!   date and runtime; a movie's year, runtime and genres; a show's year).

use std::sync::Arc;

use plx_data::pms::PmsMovie;
use plx_machine::machine::Tick;
use plx_ui::shield as geo;
use plx_ui::{Painter, Spring};
use plx_machine::machine::Measure;

/// How long focus must rest on a card before the header takes it as its subject, ms of `Tick`.
pub const SETTLE_MS: u32 = 200;
/// The header text's cross-fade into a new subject (the focus pop's rate: a quick dissolve).
const K_FADE: f32 = plx_ui::consts::K_SCALE;

/// A catalog row a page can hand the header: Home's hubs share their rows (`Arc`), the Library's
/// shelves own theirs. The header keeps its own shared copy of the subject either way.
pub trait HeaderItem {
    fn movie(&self) -> &PmsMovie;
    /// The header's own copy of the row.
    fn share(&self) -> Arc<PmsMovie>;
    /// Is `held` this very publication of the row? A republished copy (a resume point moved) is
    /// described afresh; a row that cannot tell answers `true` and keeps the facts it has.
    fn same_copy(&self, held: &Arc<PmsMovie>) -> bool;
}

impl HeaderItem for Arc<PmsMovie> {
    fn movie(&self) -> &PmsMovie {
        self
    }
    fn share(&self) -> Arc<PmsMovie> {
        Arc::clone(self)
    }
    fn same_copy(&self, held: &Arc<PmsMovie>) -> bool {
        Arc::ptr_eq(self, held)
    }
}

impl HeaderItem for PmsMovie {
    fn movie(&self) -> &PmsMovie {
        self
    }
    fn share(&self) -> Arc<PmsMovie> {
        Arc::new(self.clone())
    }
    fn same_copy(&self, _held: &Arc<PmsMovie>) -> bool {
        true
    }
}

/// What the header says about its subject, formatted once per subject rather than per frame.
#[derive(Default)]
pub struct Facts {
    sid: u16,
    logo_rk: String,
    title: String,
    subtitle: String,
    facts: Vec<String>,
    rating: String,
    summary: String,
    cast: String,
}

impl Facts {
    /// An episode or a season shows its SHOW's logo (falling back to the show's title) over its own
    /// name; anything else its own. The facts follow the measured rows: an episode's address, air
    /// date and runtime; a movie's year, runtime and genres; a show's year and genres.
    pub fn of(m: &PmsMovie) -> Self {
        let child = matches!(m.kind, 2 | 3) && !m.show_rk.is_empty();
        let (logo_rk, title, subtitle) = if child {
            let show = if m.show_title.is_empty() { &m.title } else { &m.show_title };
            (m.show_rk.clone(), show.clone(), m.title.clone())
        } else {
            (m.rk.clone(), m.title.clone(), String::new())
        };
        let runtime = (m.dur_ns > 0).then(|| plx_ui::fmt::dur_short(m.dur_ns / 1_000_000));
        let year = (m.year > 0).then(|| m.year.to_string());
        let genres = (!m.genres.is_empty()).then(|| m.genres.iter().take(2).cloned().collect::<Vec<_>>().join(", "));
        let facts: Vec<String> = match m.kind {
            3 => [
                Some(plx_ui::fmt::episode_address(i64::from(m.season_index), i64::from(m.ep_index))),
                Some(plx_ui::fmt::pretty_date(&m.aired, i64::from(m.year))),
                runtime,
            ].into_iter().flatten().collect(),
            2 => [year].into_iter().flatten().collect(),
            plx_data::pms::KIND_COLLECTION => vec![plx_ui::fmt::item_count(m.child_count)],
            _ => [year, runtime, genres].into_iter().flatten().collect(),
        };
        Facts {
            sid: m.sid.raw(),
            logo_rk,
            title,
            subtitle,
            facts: facts.into_iter().filter(|f| !f.is_empty()).collect(),
            rating: m.rating.clone(),
            summary: m.summary.clone(),
            cast: m.cast.join(", "),
        }
    }

    fn view(&self) -> geo::HeroFacts<'_> {
        geo::HeroFacts {
            sid: self.sid,
            logo_rk: &self.logo_rk,
            title: &self.title,
            subtitle: &self.subtitle,
            facts: &self.facts,
            rating: &self.rating,
            summary: &self.summary,
            cast: &self.cast,
        }
    }
}

/// **The focused-item header**: its subject, the debounce that picks it, its formatted facts, the
/// text's cross-fade and the ground under it. Paint state: no focus, reveal or press decision reads
/// it (a page's `LogicalState` leaves it out, as Home's does the Standard billboard's `Backdrop`).
pub struct Header {
    subject: Option<Arc<PmsMovie>>,
    /// The card focus is on while it has not yet rested long enough to be the subject, and the
    /// `Tick` time it arrived at (an absolute clock, never an accumulated delta).
    pending: Option<(plx_plex::plex::ServerId, String, u32)>,
    fade: Spring,
    facts: Facts,
    ground: geo::HeroGround,
}

impl Default for Header {
    fn default() -> Self {
        Self::new()
    }
}

impl Header {
    pub fn new() -> Self {
        Self { subject: None, pending: None, fade: Spring::at(1.0), facts: Facts::default(),
            ground: geo::HeroGround::new() }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// The item the header describes.
    pub fn subject(&self) -> Option<&PmsMovie> {
        self.subject.as_deref()
    }

    /// Is a focused card still waiting out the settle?
    pub fn settling(&self) -> bool {
        self.pending.is_some()
    }

    /// One frame at `t` with `focused` the card focus is on (`None` on a heading or nothing: the
    /// subject stays). Returns whether the subject changed, for the caller to ask for a frame: the
    /// settle is a TIMER on the page's `Tick`, which runs whether or not a frame presents, and
    /// reports no motion of its own (the billboard's countdown rule).
    pub fn step<I: HeaderItem + ?Sized>(&mut self, focused: Option<&I>, t: Tick) -> bool {
        let dt = t.dt();
        let mut changed = false;
        if let Some(item) = focused.filter(|i| !i.movie().rk.is_empty()) {
            let m = item.movie();
            let same = self.subject.as_ref().is_some_and(|s| s.sid == m.sid && s.rk == m.rk);
            if same {
                self.pending = None;
                // the catalog republished the row (a resume point moved): describe the new copy
                if !self.subject.as_ref().is_some_and(|s| item.same_copy(s)) {
                    self.facts = Facts::of(m);
                    self.subject = Some(item.share());
                }
            } else {
                if !self.pending.as_ref().is_some_and(|(sid, rk, _)| *sid == m.sid && *rk == m.rk) {
                    self.pending = Some((m.sid, m.rk.clone(), t.ms));
                }
                let since = self.pending.as_ref().map_or(t.ms, |pending| pending.2);
                if self.subject.is_none() || t.ms.wrapping_sub(since) >= SETTLE_MS {
                    self.show(item);
                    changed = true;
                }
            }
        } else {
            self.pending = None;
        }
        let blur = self.subject.as_ref().filter(|m| m.has_blur).map(|m| m.blur);
        self.ground.update(geo::ground_target(blur), dt);
        // decorative: the header's dissolve must not hold the page-freeze snapshot
        plx_machine::idle::decorative(|| self.fade.step(1.0, K_FADE, dt));
        changed
    }

    fn show<I: HeaderItem + ?Sized>(&mut self, item: &I) {
        let first = self.subject.is_none();
        let m = item.movie();
        self.facts = Facts::of(m);
        self.ground.subject((!m.art.is_empty()).then(|| (m.sid.raw(), m.art.as_str())));
        self.subject = Some(item.share());
        self.pending = None;
        if !first {
            self.fade.jump(0.0);
        }
    }

    /// While the subject has settled and the source is idle, warm `next`'s logo, then its backdrop:
    /// the card one step along the row, so the common single step finds both resident. One key a
    /// frame, the billboard's prefetch rule.
    pub fn warm(&self, next: Option<&PmsMovie>) {
        use plx_ui::tex::Warm;
        let Some(next) = next.filter(|_| !self.settling() && plx_ui::tex::source_idle()) else { return };
        let logo_rk = Facts::of(next).logo_rk;
        if plx_ui::tex::logo_warm(next.sid.raw(), &logo_rk) == Warm::Claimed {
            return;
        }
        plx_ui::tex::warm_on(next.sid.raw(), &next.art, 1280, 720, false);
    }

    /// The ground: the tinted wash and the subject's backdrop.
    pub fn draw_ground(&self, p: Painter) {
        self.ground.draw(p);
    }

    /// The header's column, at the subject cross-fade.
    pub fn draw_text(&self, p: Painter, measure: &dyn Measure) {
        if self.subject.is_some() {
            geo::draw_hero(p.alpha(self.fade.pos), &self.facts.view(), measure);
        }
    }
}
