//! **Library order** — a Settings page that arranges the profile's favourite libraries into the
//! order Home's per-library Recently Added shelves (`pms::merge_with_scope`) and the Library's
//! same-type pills and Sources menu (`DirectoryView::rows_for`) follow.
//!
//! One list across every library kind, because Home interleaves them: Movies, then TV Shows, then
//! Kids Movies is one order, and the Movies tab's own list is that order filtered to movies.
//!
//! The interaction is the remote's, not a drag: OK picks the focused library up (its row says
//! *Moving*), UP and DOWN carry it past its neighbours while focus stays on it, and OK — or BACK, or
//! moving focus away — puts it down. Every step is written at once as one
//! `BrowseCmd::SetLibraryOrder`; there is no draft and nothing to cancel, so leaving the page never
//! loses an arrangement. The rows are keyed by SECTION INDEX (an address within the table's epoch),
//! so a reorder moves no focus key; the page reseeds its order whenever the table's epoch or its
//! favourite set changes underneath it.

use std::borrow::Cow;

use plx_data::stores::browse::{BrowseCmd, DirectoryView};
use plx_data::stores::{StoreCmd, StoreId};
use plx_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputEvent,
    InputKind, InstanceId, Key, LogicalState, Machine, MachineId,
};
use plx_machine::present::Provenance;
use plx_ui::form::{Form, FormId, FormSection, FormTable, RowKey, RowKind};
use plx_ui::frame::Budget;
use plx_ui::route_screen::RouteLayout;
use plx_ui::screen::{DrawFrame, Enter, FocusSource, FocusTarget, HitSource, RenderStrategy, Screen, ScreenEvent};
use plx_ui::table::Row;
use plx_ui::table_screen::{Header, TableScreen};
use plx_ui::Rect;

use super::family::{form_activate, form_focus, InnerHost, SettingsPage};
use super::registry::{AppFx, BAND};

/// A row's identity: the library's section index in the table it was read from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct LibId(usize);

impl FormId for LibId {
    fn key(&self) -> RowKey {
        // Section indices are small; the clamp only keeps an absurd one below the band.
        RowKey((self.0 as u32).min(BAND - 1))
    }
}

/// What a row does: pick its library up, or put it down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OrderAction {
    Pick(usize),
}

/// One library row's plain inputs, read from the directory.
#[derive(Clone, PartialEq, Debug)]
struct OrderRow {
    section: usize,
    /// The library's own name — server text.
    title: String,
    /// Its count line ("185 films") — app text.
    detail: String,
}

/// The page's rows from plain inputs: one per favourite library in `order`, the picked-up one
/// carrying the *Moving* read-out; a note instead when there is nothing to arrange.
fn order_form(rows: &[OrderRow], order: &[usize], moving: Option<usize>) -> Form<LibId, OrderAction, SettingsPage> {
    let mut section = FormSection::new("");
    let mut any = false;
    for &index in order {
        let Some(row) = rows.iter().find(|row| row.section == index) else { continue };
        any = true;
        let mut drawn = Row::new(&row.title).server_label().detail(&row.detail);
        if moving == Some(index) {
            drawn = drawn.value(plx_platform::i18n::msg::settings_library_order_moving());
        }
        section = section.item(LibId(index), RowKind::Button, OrderAction::Pick(index), drawn);
    }
    if !any {
        section = section.note(plx_platform::i18n::msg::settings_library_order_empty());
    }
    Form::new().section(section)
}

/// The favourite libraries `directory` offers, in its display order, with their rows.
fn read_directory(directory: DirectoryView<'_>) -> (Vec<usize>, Vec<OrderRow>) {
    let sections = directory.sections();
    let order: Vec<usize> = directory.display_order().into_iter()
        .filter(|&index| sections.get(index).is_some_and(|s| s.row.pinned && s.sid.is_some()))
        .collect();
    let rows = order.iter().filter_map(|&index| sections.get(index).map(|s| OrderRow {
        section: index,
        title: s.row.title.clone(),
        detail: s.row.count_line.clone(),
    })).collect();
    (order, rows)
}

pub struct LibraryOrderPage {
    entry: EntryId,
    form: FormTable<LibId, OrderAction, SettingsPage>,
    state: OrderState,
    rows: Vec<OrderRow>,
    /// The table the local order was seeded from: its epoch and its favourite set (sorted). A
    /// change to either reseeds; anything else — this page's own writes landing — does not.
    seeded: Option<(u32, Vec<usize>)>,
}

struct OrderState {
    /// The focused row's key — an identity, not a position.
    sel: RowKey,
    /// The favourite libraries, by section index, in the order this page shows and writes.
    order: Vec<usize>,
    /// The library picked up, if any.
    moving: Option<usize>,
}

impl LogicalState for OrderState {
    fn write(&self, w: &mut Canon) {
        w.u32(self.sel.0).u32(self.order.len() as u32);
        for &index in &self.order {
            w.u32(index as u32);
        }
        w.bool(self.moving.is_some()).u32(self.moving.unwrap_or(0) as u32);
    }
    fn probe(&self, out: &mut String) {
        out.push_str(&format!("library-order sel={} order={:?} moving={:?}", self.sel.0, self.order, self.moving));
    }
}

impl LibraryOrderPage {
    pub fn new(entry: EntryId, directory: DirectoryView<'_>) -> Self {
        let mut page = Self {
            entry,
            form: FormTable::new(BAND),
            state: OrderState { sel: RowKey(0), order: Vec::new(), moving: None },
            rows: Vec::new(),
            seeded: None,
        };
        page.sync(directory);
        if let Some(&first) = page.state.order.first() {
            page.state.sel = LibId(first).key();
        }
        page
    }

    /// Reseed the order from `directory` when its table or favourite set moved; refresh the rows'
    /// words either way. Answers whether anything the page draws changed.
    fn sync(&mut self, directory: DirectoryView<'_>) -> bool {
        let (order, rows) = read_directory(directory);
        let mut set = order.clone();
        set.sort_unstable();
        let identity = (directory.epoch().unwrap_or(0), set);
        let reseed = self.seeded.as_ref() != Some(&identity);
        let changed = reseed || rows != self.rows;
        if reseed {
            self.seeded = Some(identity);
            self.state.order = order;
            if self.state.moving.is_some_and(|moving| !self.state.order.contains(&moving)) {
                self.state.moving = None;
            }
        }
        self.rows = rows;
        if changed {
            self.rebuild();
        }
        changed
    }

    /// Re-derive the rows, keeping the cursor on its library by identity.
    fn rebuild(&mut self) {
        let keep = self.form.selected_id().copied()
            .or_else(|| self.state.order.iter().copied().find(|&i| LibId(i).key() == self.state.sel).map(LibId));
        self.form.table.compact = false;
        self.form.set(order_form(&self.rows, &self.state.order, self.state.moving), keep.as_ref());
        self.form.table.list_focused = true;
        if let Some(key) = self.form.key_at(self.form.table.sel.max(0) as usize) {
            self.state.sel = key;
        }
    }

    fn view(&self) -> TableScreen<'_> {
        TableScreen::new(
            Header::new(RouteLayout::screen(), Some(plx_platform::i18n::msg::settings_title()),
                plx_platform::i18n::msg::settings_library_order_title(),
                plx_platform::i18n::msg::settings_library_order_copy()),
            &self.form.table, GroupId(0), self.entry,
        ).keyed(&self.form)
    }

    /// OK on a row: pick its library up, or put down the one already held.
    fn activate(&mut self, key: u32, fx: &mut Effects<'_, InnerHost>) {
        let Some(OrderAction::Pick(index)) = form_activate(&self.form, key, fx) else { return };
        self.state.moving = if self.state.moving == Some(index) { None } else { Some(index) };
        self.rebuild();
        fx.invalidate(Provenance::Input);
    }

    /// Carry the held library one place up (`-1`) or down (`+1`) and write the new order. At an
    /// end it stays where it is; the press is still the page's.
    fn shift(&mut self, by: isize, fx: &mut Effects<'_, InnerHost>) {
        let Some(moving) = self.state.moving else { return };
        let Some(at) = self.state.order.iter().position(|&index| index == moving) else { return };
        let Some(to) = at.checked_add_signed(by).filter(|&to| to < self.state.order.len()) else { return };
        self.state.order.swap(at, to);
        if let Some((epoch, _)) = &self.seeded {
            fx.push(Fx::App(AppFx::Store(StoreId::Browse, StoreCmd::Browse(BrowseCmd::SetLibraryOrder {
                epoch: *epoch,
                sections: self.state.order.clone(),
            }))));
        }
        self.rebuild();
        fx.invalidate(Provenance::Input);
    }

    /// Put the held library down, if one is held.
    fn put_down(&mut self, fx: &mut Effects<'_, InnerHost>) -> bool {
        if self.state.moving.take().is_none() {
            return false;
        }
        self.rebuild();
        fx.invalidate(Provenance::Input);
        true
    }
}

impl Machine<InnerHost> for LibraryOrderPage {
    type Ev = ScreenEvent<InnerHost>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, InnerHost>, fx: &mut Effects<'_, InnerHost>) -> Handled {
        match ev {
            ScreenEvent::Mount => {
                // Seat the first library explicitly, as Language does: the generic modal Enter
                // runs before queued mount effects. With no library there is no row to seat.
                if self.state.order.is_empty() {
                    return Handled::No;
                }
                fx.push(Fx::Deliver(
                    MachineId::Instance(InstanceId(0)),
                    Delivery::Screen(ScreenEvent::Enter(Enter::Fresh {
                        focus: FocusTarget::Elem(FocusKey { entry: self.entry, elem: self.state.sel.0 }),
                    })),
                ));
                Handled::Yes
            }
            ScreenEvent::Tick(t) => {
                if self.sync(cx.views) {
                    fx.invalidate(Provenance::Input);
                }
                self.form.table.update(t.dt(), RouteLayout::screen().sectioned_table().h);
                Handled::Yes
            }
            ScreenEvent::FocusMoved { to, .. } => {
                form_focus(&mut self.form, to.elem);
                if let Some(key) = self.form.key_at(self.form.table.sel.max(0) as usize) {
                    self.state.sel = key;
                }
                // Focus leaving the held library (a pointer, say) puts it down where it is.
                if self.state.moving.is_some_and(|moving| LibId(moving).key().0 != to.elem) {
                    self.put_down(fx);
                }
                Handled::Yes
            }
            ScreenEvent::Activate(key) => {
                self.activate(*key, fx);
                Handled::Yes
            }
            ScreenEvent::Input(InputEvent { kind: InputKind::Key { key, edge, .. }, .. })
                if self.state.moving.is_some() =>
            {
                // While a library is held every direction is the page's: UP/DOWN carry it, and
                // LEFT/RIGHT must neither leave the page nor move focus off it. BACK puts it down
                // rather than leaving.
                let pressed = matches!(edge, Edge::Down | Edge::Repeat);
                match key {
                    Key::Up if pressed => self.shift(-1, fx),
                    Key::Down if pressed => self.shift(1, fx),
                    Key::Back if *edge == Edge::Down => { self.put_down(fx); }
                    Key::Up | Key::Down | Key::Left | Key::Right | Key::Back => {}
                    _ => return Handled::No,
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

plx_ui::focusable_via_view!(LibraryOrderPage, InnerHost, view);

impl Screen<InnerHost> for LibraryOrderPage {
    fn name(&self) -> &'static str { "library-order" }
    fn state(&self) -> &dyn LogicalState { &self.state }
    fn crumb(&self, _cx: &Cx<'_, InnerHost>) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(plx_platform::i18n::msg::settings_title()))
    }
    fn prepare(&mut self, _b: &mut Budget, _cx: &Cx<'_, InnerHost>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, InnerHost>) {
        plx_ui::screen::Part::<InnerHost>::draw(&mut self.view(), f, Rect::FULL);
    }
    fn render(&self) -> RenderStrategy { RenderStrategy::Page }
    fn focus_source(&self) -> FocusSource { FocusSource::Engine }
    fn hit_source(&self) -> HitSource { HitSource::Engine }
}

#[cfg(test)]
#[path = "library_order_tests.rs"]
mod tests;
