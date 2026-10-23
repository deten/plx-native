//! **The Shield layout's player Settings panel** — the right-side panel behind the round gear
//! button over the playbar (j03), and its *Playback Options* page (j04). It gathers what the
//! Standard transport spreads over its Audio and `…` discs:
//!
//! - **Quality**, reading out the current rung, opens the `…` popover's Quality page
//!   (`crate::more_menu`, presented as a Shield panel); absent under Force Direct Play, where no
//!   rung can change what plays (`more_menu`'s own rule).
//! - **Subtitles** and **Audio**, reading out the current track, open the track menu on that tab
//!   (`crate::track_menu`, presented as a Shield panel).
//! - **Playback Options** pushes a page of switches: Boost dialog and Normalize loudness (the Audio
//!   tab's pair, through [`crate::track_menu::enhancement_offer`]: drawn dim with the reason as
//!   their second line when disabled, absent without Plex Pass), Stats for nerds (the `…`
//!   popover's switch) and, in a lab build, Send diagnostics.
//!
//! The panel only DECIDES ([`SettingsOk`]): the overlay that owns it opens the other panels over
//! it, so BACK walks out one panel at a time, and the loop performs every commit exactly as the
//! Standard panels' rows do. Playback Speed, Sleep Timer and Display Mode from the reference are
//! deliberately not offered.
use plx_data::metadata;
use plx_machine::machine::{Canon, Cx, EntryId, FocusKey, GroupId, Host, Measure};
use plx_ui::form::{Activation, Form, FormId, FormSection, FormTable, RowKey, RowKind};
use plx_ui::frame::Budget;
use plx_ui::geom::IndexElem;
use plx_ui::page_stack::{
    popover_group_of, popover_groups, popover_neighbour, popover_place, popover_register_stops, popover_seat,
    PageStack, PopoverPanel, ShieldPage, ShieldPresentation,
};
use plx_ui::panel_motion::PanelMotion;
use plx_ui::screen::{At, Dir, DrawFrame, Focusable, GroupSpec, Part, Placed, Step};
use plx_ui::table::Row;
use plx_ui::Rect;

use crate::track_menu::{EnhancementOffer, TrackCommit};

/// A row of the panel, on either page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetRow {
    Quality,
    Subtitles,
    Audio,
    Options,
    Boost,
    Loudness,
    Stats,
    Diagnostics,
}

impl FormId for SetRow {
    fn key(&self) -> RowKey {
        RowKey(match self {
            SetRow::Quality => 1,
            SetRow::Subtitles => 2,
            SetRow::Audio => 3,
            SetRow::Options => 4,
            SetRow::Boost => 5,
            SetRow::Loudness => 6,
            SetRow::Stats => 7,
            SetRow::Diagnostics => 8,
        })
    }
}

/// The panel's one drill-in page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetPage {
    Options,
}

/// The panel a root row opens over this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opens {
    Quality,
    Subtitles,
    Audio,
}

/// What OK on the focused row decided.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsOk {
    /// Present that panel over this one.
    Open(Opens),
    /// Perform this track commit (a Boost / Loudness flip); the panel stays.
    Commit(TrackCommit),
    /// Perform the `…` popover's action; `keep_open` for a switch, else the panel closes.
    More { action: crate::more_menu::Action, keep_open: bool },
    /// Playback Options opened.
    Navigated,
    /// Nothing to do (a disabled switch).
    Inert,
}

type SetForm = Form<SetRow, (), SetPage>;
type SetTable = FormTable<SetRow, (), SetPage>;

/// What the rows were built from: a change rebuilds them in place.
#[derive(Clone, PartialEq)]
struct Sig {
    quality: Option<String>,
    subtitles: String,
    audio: String,
    offer: EnhancementOffer,
    stats: bool,
    diagnostics: bool,
}

impl Sig {
    fn read(ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) -> Sig {
        let offered = !plx_media::route::forced_direct_play(ps) && !plx_media::route::available_quality_ladder().is_empty();
        Sig {
            quality: offered.then(|| plx_media::route::quality().name()),
            subtitles: subtitle_value(ps, meta),
            audio: audio_value(ps, meta),
            offer: crate::track_menu::enhancement_offer(ps),
            stats: crate::more_menu::stats_shown(),
            diagnostics: plx_platform::labcfg::menu_row_enabled(),
        }
    }
}

/// The current subtitle as its row reads it ("English (SRT)"), or Off.
pub fn subtitle_value(ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) -> String {
    let sid = plx_media::route::cur_sub_sid(ps);
    meta.playing()
        .and_then(|item| item.subs.iter().find(|s| sid > 0 && s.id == sid))
        .map(|s| {
            let lang = if s.lang.is_empty() { plx_platform::i18n::msg::widgets_tracks_unknown() } else { s.lang.as_str() };
            if s.codec.is_empty() { lang.to_string() } else { format!("{lang} ({})", s.codec.to_ascii_uppercase()) }
        })
        .unwrap_or_else(|| plx_platform::i18n::msg::widgets_tracks_off().to_string())
}

/// The current audio track as its row reads it ("English (AC-3 5.1)").
pub fn audio_value(ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) -> String {
    let sid = plx_media::route::cur_audio_sid(ps);
    let Some(item) = meta.playing() else { return String::new() };
    let Some(s) = item.audio.iter().find(|s| s.id == sid).or(item.audio.first()) else { return String::new() };
    let lang = if s.lang.is_empty() { plx_platform::i18n::msg::widgets_tracks_unknown() } else { s.lang.as_str() };
    let desc = crate::track_menu::audio_descriptor(s);
    if desc.is_empty() { lang.to_string() } else { format!("{lang} ({desc})") }
}

/// The root as a pure form.
fn root_form(sig: &Sig) -> SetForm {
    use plx_platform::i18n::msg;
    let mut sec = FormSection::new("");
    if let Some(q) = &sig.quality {
        sec = sec.item(SetRow::Quality, RowKind::Button, (), Row::new(msg::widgets_menu_quality()).value(q.clone()));
    }
    let sec = sec
        .item(SetRow::Subtitles, RowKind::Button, (), Row::new(msg::widgets_tracks_subtitles()).value(sig.subtitles.clone()))
        .item(SetRow::Audio, RowKind::Button, (), Row::new(msg::widgets_tracks_audio()).value(sig.audio.clone()))
        .item(SetRow::Options, RowKind::Nav(SetPage::Options), (), Row::new(msg::widgets_player_playback_options()));
    Form::new().section(sec)
}

/// The Playback Options page as a pure form: the dialogue pair (each with its second line), the
/// statistics switch and the lab's diagnostics row.
fn options_form(sig: &Sig) -> SetForm {
    use plx_platform::i18n::msg;
    let mut sec = FormSection::new("");
    let offer = &sig.offer;
    if !offer.hidden() {
        let note = offer.note();
        let shown = offer.shown.unwrap_or(plx_plex::plex::AudioEnhancements::NONE);
        let line = |own: &str| note.clone().unwrap_or_else(|| own.to_string());
        let disabled = offer.disabled.is_some();
        sec = sec
            .item(SetRow::Boost, RowKind::Toggle, (),
                Row::new(msg::widgets_tracks_boost_dialog()).toggle(shown.boost_dialog && !disabled)
                    .detail(line(msg::widgets_player_boost_detail())))
            .disabled(disabled)
            .item(SetRow::Loudness, RowKind::Toggle, (),
                Row::new(msg::widgets_tracks_normalize_loudness()).toggle(shown.normalize_loudness && !disabled)
                    .detail(line(msg::widgets_player_loudness_detail())))
            .disabled(disabled);
    }
    sec = sec.item(SetRow::Stats, RowKind::Toggle, (), Row::new(msg::widgets_menu_stats()).toggle(sig.stats));
    if sig.diagnostics {
        sec = sec.item(SetRow::Diagnostics, RowKind::Button, (), Row::new(msg::widgets_menu_diagnostics()));
    }
    Form::new().section(sec)
}

/// The panel's whole state, owned by the overlay that mounts it.
pub struct SettingsState {
    form: SetTable,
    pages: PageStack<SetPage, SetRow>,
    sig: Sig,
    /// Unused by the fixed panel, but the shared popover queries take one.
    motion: PanelMotion,
    shield: ShieldPresentation,
}

impl SettingsState {
    pub fn new(ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) -> Self {
        let sig = Sig::read(ps, meta);
        let mut form = SetTable::new(plx_ui::table_screen::BAND_BASE);
        form.set_or_open(root_form(&sig), None);
        SettingsState {
            form,
            pages: PageStack::new(),
            sig,
            motion: PanelMotion::new(),
            shield: ShieldPresentation { on: true, scroll: 0.0 },
        }
    }

    /// The page showing: `None` at the root.
    pub fn page(&self) -> Option<SetPage> {
        self.pages.top()
    }

    /// The panel's title: Settings, or Playback Options.
    pub fn title(&self) -> &'static str {
        match self.pages.top() {
            Some(SetPage::Options) => plx_platform::i18n::msg::widgets_player_playback_options(),
            None => plx_platform::i18n::msg::widgets_player_settings(),
        }
    }

    fn page_form(&self) -> SetForm {
        match self.pages.top() {
            Some(SetPage::Options) => options_form(&self.sig),
            None => root_form(&self.sig),
        }
    }

    /// The highlighted row (the focus probe's read).
    pub fn sel(&self) -> i32 {
        self.form.table.sel
    }

    /// The highlighted row's identity.
    pub fn selected(&self) -> Option<SetRow> {
        self.form.selected_id().copied()
    }

    /// Every row's identity on the page showing, in order.
    pub fn ids(&self) -> Vec<SetRow> {
        (0..self.form.table.n_rows().max(0) as usize).filter_map(|i| self.form.id_at(i).copied()).collect()
    }

    /// **The replay canon**: the page path and the selected row's key.
    pub fn canon(&self, c: &mut Canon) {
        self.pages.canon(c, |_| 1, |r| r.key().0);
        c.u32(self.form.key_at(self.form.table.sel.max(0) as usize).map_or(u32::MAX, |k| k.0));
    }

    /// Write back the engine's focus cursor.
    pub fn focus_key(&mut self, elem: u32) -> bool {
        let Some(i) = self.form.index_of_key(RowKey(elem)) else { return false };
        self.form.table.sel = i as i32;
        self.follow(false);
        true
    }

    /// Put the cursor on `id` (a test's way of walking to it).
    pub fn focus_id(&mut self, id: SetRow) -> bool {
        self.focus_key(id.key().0) && self.selected() == Some(id)
    }

    fn follow(&mut self, reset: bool) {
        let title = self.title();
        self.shield.follow(&self.form, title, reset);
    }

    fn push(&mut self, page: SetPage) {
        let Some(return_id) = self.form.selected_id().copied() else { return };
        self.pages.push(page, return_id, 0.0);
        let form = self.page_form();
        self.form.open(form, None);
        self.follow(true);
    }

    /// Back to the root, its opener focused; `false` at the root.
    pub fn pop(&mut self) -> bool {
        let Some(saved) = self.pages.pop() else { return false };
        let form = self.page_form();
        self.form.restore(form, Some(&saved.return_id), 0.0);
        self.follow(true);
        true
    }

    /// RIGHT: a Nav row enters its page; anything else stays.
    pub fn on_right(&mut self) {
        let sel = self.form.table.sel.max(0) as usize;
        if let Some(Activation::Push(page)) =
            self.form.binding_at(sel).filter(|b| matches!(b.kind, RowKind::Nav(_))).and_then(|_| self.form.activate(sel))
        {
            self.push(page);
        }
    }

    /// OK on the highlighted row.
    pub fn on_ok(&mut self) -> SettingsOk {
        let sel = self.form.table.sel.max(0) as usize;
        let Some(id) = self.form.id_at(sel).copied() else { return SettingsOk::Inert };
        match id {
            SetRow::Quality => SettingsOk::Open(Opens::Quality),
            SetRow::Subtitles => SettingsOk::Open(Opens::Subtitles),
            SetRow::Audio => SettingsOk::Open(Opens::Audio),
            SetRow::Options => match self.form.activate(sel) {
                Some(Activation::Push(page)) => {
                    self.push(page);
                    SettingsOk::Navigated
                }
                _ => SettingsOk::Inert,
            },
            SetRow::Boost | SetRow::Loudness => {
                match crate::track_menu::enhancement_toggle(&self.sig.offer, id == SetRow::Boost) {
                    Some(commit) => {
                        // what the row reads at once; the live poll settles it to the route's answer
                        if let TrackCommit::AudioEnhancement(a) = &commit {
                            self.sig.offer.shown = Some(*a);
                            let form = self.page_form();
                            self.form.refresh(form);
                        }
                        SettingsOk::Commit(commit)
                    }
                    None => SettingsOk::Inert,
                }
            }
            SetRow::Stats => {
                self.sig.stats = !self.sig.stats;
                let form = self.page_form();
                self.form.refresh(form);
                SettingsOk::More { action: crate::more_menu::Action::ToggleStats, keep_open: true }
            }
            SetRow::Diagnostics => {
                SettingsOk::More { action: crate::more_menu::Action::SendDiagnostics, keep_open: false }
            }
        }
    }

    /// Follow the live session: a change in what any row reads rebuilds the page in place, focus
    /// kept by identity.
    pub fn update(&mut self, ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) {
        let sig = Sig::read(ps, meta);
        if sig != self.sig {
            self.sig = sig;
            let form = self.page_form();
            self.form.refresh(form);
        }
        self.follow(false);
    }

    pub fn draw(&self, appear: f32, measure: &dyn Measure) {
        self.shield.draw(plx_ui::Painter::root().alpha(appear), &self.form, self.title(), measure);
    }
}

impl PopoverPanel for SettingsState {
    type Id = SetRow;
    type Act = ();
    type Dest = SetPage;
    fn form(&self) -> &FormTable<Self::Id, Self::Act, Self::Dest> {
        &self.form
    }
    fn motion(&self) -> &PanelMotion {
        &self.motion
    }
    fn panel_rect(&self, _measure: &dyn Measure) -> Rect {
        plx_ui::shield_panel::PANEL
    }
    fn shown_rect(&self, _measure: &dyn Measure) -> Rect {
        plx_ui::shield_panel::PANEL
    }
    fn shield(&self) -> Option<ShieldPage> {
        self.shield.page(&self.form, self.title())
    }
}

/// **The Engine-shaped view of the panel**: one `Column` group over the page's rows, the shared
/// popover queries over the Shield panel's geometry (`plx_ui::page_stack`). LEFT and RIGHT are the
/// owning screen's (pop, and enter Playback Options).
pub struct SettingsPart<'a> {
    pub state: &'a SettingsState,
    pub entry: EntryId,
    pub group: GroupId,
}

impl<H: Host> Focusable<H> for SettingsPart<'_>
where
    H::Elem: IndexElem,
{
    fn groups(&self, cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        popover_groups(self.state, self.group, cx.measure, out);
    }
    fn group_of(&self, key: &H::Elem, _cx: &Cx<'_, H>) -> Option<GroupId> {
        popover_group_of(self.state, key, self.group)
    }
    fn neighbour(&self, key: FocusKey<H::Elem>, dir: Dir, _cx: &Cx<'_, H>) -> Step<H::Elem> {
        popover_neighbour(self.state, self.entry, key, dir)
    }
    fn place(&self, key: &H::Elem, cx: &Cx<'_, H>, _at: At) -> Option<Placed> {
        popover_place(self.state, key, cx.measure)
    }
    fn reconcile(&self, want: FocusKey<H::Elem>, _cx: &Cx<'_, H>) -> FocusKey<H::Elem> {
        let kept = want.elem.index().filter(|k| self.state.form.index_of_key(RowKey(*k)).is_some());
        let key = kept
            .or_else(|| self.state.form.selected_key().map(|k| k.0))
            .or_else(|| self.state.form.opening_key().map(|k| k.0));
        FocusKey { entry: self.entry, elem: H::Elem::of_index(key.unwrap_or(0)) }
    }
    fn seat(&self, _g: GroupId, _from: Placed, _cx: &Cx<'_, H>) -> FocusKey<H::Elem> {
        popover_seat(self.state, self.entry)
    }
}

impl<H: Host> Part<H> for SettingsPart<'_>
where
    H::Elem: IndexElem,
{
    fn prepare(&mut self, _b: &mut Budget, _cx: &Cx<'_, H>) {}
    /// Registers every row's stop on the panel ([`popover_register_stops`]); the paint is
    /// [`SettingsState::draw`]'s, from the owning screen.
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>, _rect: Rect) {
        popover_register_stops(self.state, self.entry, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(quality: bool, offer: EnhancementOffer) -> Sig {
        Sig {
            quality: quality.then(|| "Original".to_string()),
            subtitles: "English (SRT)".into(),
            audio: "English (AC-3 5.1)".into(),
            offer,
            stats: false,
            diagnostics: false,
        }
    }

    fn offer(shown: Option<plx_plex::plex::AudioEnhancements>, disabled: Option<plx_media::route::DisabledReason>) -> EnhancementOffer {
        EnhancementOffer { shown, route: shown.map(|_| plx_media::route::EnhancementRoute::Remux), disabled,
            effect: plx_media::route::SubtitleEffect::None }
    }

    fn table(form: SetForm) -> SetTable {
        let mut t = SetTable::new(plx_ui::table_screen::BAND_BASE);
        t.set_or_open(form, None);
        t
    }

    fn rows(t: &SetTable) -> Vec<SetRow> {
        (0..t.table.n_rows().max(0) as usize).filter_map(|i| t.id_at(i).copied()).collect()
    }

    /// j03: Quality, Subtitles and Audio read out what is current, Playback Options leads on; no
    /// Quality row where no rung can change what plays (Force Direct Play, an empty ladder).
    #[test]
    fn the_root_reads_out_quality_subtitles_and_audio_then_playback_options() {
        let t = table(root_form(&sig(true, offer(None, None))));
        assert_eq!(rows(&t), [SetRow::Quality, SetRow::Subtitles, SetRow::Audio, SetRow::Options]);
        let values: Vec<Option<String>> = t.table.sections[0].rows.iter().map(|r| r.value.clone()).collect();
        assert_eq!(values[0].as_deref(), Some("Original"));
        assert_eq!(values[1].as_deref(), Some("English (SRT)"));
        assert_eq!(values[2].as_deref(), Some("English (AC-3 5.1)"));
        assert!(matches!(t.binding_at(3).map(|b| &b.kind), Some(RowKind::Nav(SetPage::Options))));
        let forced = table(root_form(&sig(false, offer(None, None))));
        assert_eq!(rows(&forced), [SetRow::Subtitles, SetRow::Audio, SetRow::Options]);
    }

    /// j04: the dialogue pair with a second line each, then the statistics switch; a disabled pair
    /// is dim, reads Off and names why on its second line; without Plex Pass it is absent.
    #[test]
    fn playback_options_hold_the_dialogue_pair_and_the_statistics_switch() {
        use plx_plex::plex::AudioEnhancements;
        let on = AudioEnhancements { boost_dialog: true, ..AudioEnhancements::NONE };
        let t = table(options_form(&sig(true, offer(Some(on), None))));
        assert_eq!(rows(&t), [SetRow::Boost, SetRow::Loudness, SetRow::Stats]);
        let r = &t.table.sections[0].rows;
        assert_eq!((r[0].toggle, r[1].toggle), (Some(true), Some(false)));
        assert!(!r[0].detail.is_empty() && !r[1].detail.is_empty(), "each switch has its second line");
        assert!(!r[0].dim);
        let dis = table(options_form(&sig(true, offer(None, Some(plx_media::route::DisabledReason::NotAnalyzed)))));
        let r = &dis.table.sections[0].rows;
        assert!(r[0].dim && r[1].dim, "a disabled pair is dim");
        assert_eq!(r[1].toggle, Some(false));
        assert_eq!(r[1].detail, crate::track_menu::TrackMenuState::enhancement_reason(plx_media::route::DisabledReason::NotAnalyzed));
        let hidden = table(options_form(&sig(true, offer(None, None))));
        assert_eq!(rows(&hidden), [SetRow::Stats], "no Plex Pass: the one silent gap");
    }

    /// The root rows ask for the other panels; Playback Options pushes its page and BACK returns to
    /// the opener; a disabled switch does nothing.
    #[test]
    fn the_rows_open_their_panels_and_back_walks_out_page_by_page() {
        let mut st = SettingsState {
            form: table(root_form(&sig(true, offer(None, Some(plx_media::route::DisabledReason::ServerRefused))))),
            pages: PageStack::new(),
            sig: sig(true, offer(None, Some(plx_media::route::DisabledReason::ServerRefused))),
            motion: PanelMotion::new(),
            shield: ShieldPresentation { on: true, scroll: 0.0 },
        };
        assert!(st.focus_id(SetRow::Quality));
        assert_eq!(st.on_ok(), SettingsOk::Open(Opens::Quality));
        assert!(st.focus_id(SetRow::Subtitles));
        assert_eq!(st.on_ok(), SettingsOk::Open(Opens::Subtitles));
        assert!(st.focus_id(SetRow::Audio));
        assert_eq!(st.on_ok(), SettingsOk::Open(Opens::Audio));
        assert!(st.focus_id(SetRow::Options));
        assert_eq!(st.on_ok(), SettingsOk::Navigated);
        assert_eq!(st.page(), Some(SetPage::Options));
        assert_eq!(st.title(), plx_platform::i18n::msg::widgets_player_playback_options());
        assert!(st.focus_id(SetRow::Boost));
        assert_eq!(st.on_ok(), SettingsOk::Inert, "a disabled switch is inert");
        assert!(st.focus_id(SetRow::Stats));
        assert_eq!(st.on_ok(), SettingsOk::More { action: crate::more_menu::Action::ToggleStats, keep_open: true });
        assert_eq!(st.ids().last(), Some(&SetRow::Stats));
        assert!(st.pop(), "BACK on the page returns to the root");
        assert_eq!(st.selected(), Some(SetRow::Options), "on the row that opened it");
        assert!(!st.pop(), "the root has no page to pop: BACK closes the panel");
        // the panel stands at the Shield frame, its rows on the player pitch
        let page = PopoverPanel::shield(&st).expect("always the Shield panel");
        assert_eq!(plx_ui::shield_panel::row_rect(&page.lines, 0, 0.0), Some(Rect::new(1314.0, 210.0, 508.0, 64.0)));
        assert_eq!(plx_ui::shield_panel::row_rect(&page.lines, 1, 0.0).unwrap().y, 274.0);
    }

    /// **The panel's own words fit it, in every shipped language** (whole-pixel device advances):
    /// each title on the panel's title line, a row label beside the widest a value may take (half
    /// the plate) or beside a switch, and a switch's second line beside the switch.
    #[test]
    fn every_app_label_fits_the_panel_in_every_language() {
        use plx_base::fontcov::advances::ShippedMeasure;
        use plx_platform::i18n::{language_on_this_thread_for_test, msg, SHIPPED};
        use plx_ui::shield_panel::{self as sp, ITEM_W, LABEL_INSET, LABEL_SIZE, SWITCH_W, TITLE, TITLE_SIZE, TRAIL_GAP, TRAIL_INSET};
        let m = ShippedMeasure;
        let plate = ITEM_W - LABEL_INSET - TRAIL_INSET;
        let beside_switch = plate - SWITCH_W - TRAIL_GAP;
        let beside_value = plate * 0.5;
        let mut out = Vec::new();
        for language in SHIPPED {
            let _guard = language_on_this_thread_for_test(language);
            let mut fits = |text: &str, size: i32, bold: bool, room: f32| {
                let w = plx_machine::machine::Measure::width_str(&m, text, size, bold);
                if w > room {
                    out.push(format!("{}: {text:?} is {w:.0}px in {room:.0}", language.tag()));
                }
            };
            for title in [msg::widgets_player_settings(), msg::widgets_player_playback_options(),
                msg::widgets_menu_quality(), msg::widgets_tracks_subtitles(), msg::widgets_tracks_audio()] {
                fits(title, TITLE_SIZE, true, TITLE.w);
            }
            for label in [msg::widgets_menu_quality(), msg::widgets_tracks_subtitles(), msg::widgets_tracks_audio()] {
                fits(label, LABEL_SIZE, false, beside_value);
            }
            fits(msg::widgets_player_playback_options(), LABEL_SIZE, false, plate);
            for label in [msg::widgets_tracks_boost_dialog(), msg::widgets_tracks_normalize_loudness(), msg::widgets_menu_stats()] {
                fits(label, LABEL_SIZE, false, beside_switch);
            }
            for line in [msg::widgets_player_boost_detail(), msg::widgets_player_loudness_detail()] {
                fits(line, sp::DETAIL_SIZE, false, beside_switch);
            }
        }
        assert!(out.is_empty(), "app text that would end in an ellipsis:
{}", out.join("
"));
    }

    /// A switch flip reads its new state at once and asks for the same commit the Audio tab makes.
    #[test]
    fn a_boost_flip_commits_the_whole_preference_and_reads_on() {
        use plx_plex::plex::AudioEnhancements;
        let o = offer(Some(AudioEnhancements::NONE), None);
        let mut st = SettingsState {
            form: table(options_form(&sig(true, o))),
            pages: PageStack::new(),
            sig: sig(true, o),
            motion: PanelMotion::new(),
            shield: ShieldPresentation { on: true, scroll: 0.0 },
        };
        st.pages.push(SetPage::Options, SetRow::Options, 0.0);
        assert!(st.focus_id(SetRow::Boost));
        let want = AudioEnhancements { boost_dialog: true, ..AudioEnhancements::NONE };
        assert_eq!(st.on_ok(), SettingsOk::Commit(TrackCommit::AudioEnhancement(want)));
        assert_eq!(st.form.table.sections[0].rows[0].toggle, Some(true));
        assert_eq!(st.selected(), Some(SetRow::Boost), "focus stays on the switch");
    }
}
