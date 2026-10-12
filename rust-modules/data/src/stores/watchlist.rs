//! Physically owned Watchlist model and its worker adapter (`stores/collection.rs`'s shape); the
//! model and its reasoning live in `crate::watchlist`.

use crate::watchlist::{WatchlistAdapter, WatchlistState, WatchlistView};
use plx_machine::machine::{Cx, Effects, Handled, Host, Machine};
use plx_plex::plex::discover::WatchItem;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use super::StoreEv;

#[derive(Clone)]
pub enum WatchlistCmd {
    /// The Watchlist page came up: ask for the list again on the next pump, and retry a profile
    /// the provider refused or a list that failed.
    Open,
    /// Put a title on the Watchlist. `row` is the server's own catalog row when the press came
    /// from a page that already has one, so the list shows the title as available at once.
    Add { item: WatchItem, row: Option<Box<crate::pms::PmsMovie>> },
    /// Take a title off the Watchlist, by its provider guid.
    Remove { guid: String },
    /// Forget everything (sign-out).
    Reset,
}

pub struct WatchlistStore {
    state: WatchlistState,
    adapter: Arc<WatchlistAdapter>,
    notice_gen: AtomicU32,
    notice_dirty: AtomicBool,
}

impl Default for WatchlistStore {
    fn default() -> Self {
        Self { state: Default::default(), adapter: Arc::new(Default::default()),
            notice_gen: AtomicU32::new(0), notice_dirty: AtomicBool::new(false) }
    }
}

impl WatchlistStore {
    fn bump(&self) {
        self.notice_dirty.store(true, Ordering::Relaxed);
        self.notice_gen.fetch_add(1, Ordering::Relaxed);
    }
    pub fn gen(&self) -> u32 { self.notice_gen.load(Ordering::Relaxed) }
    pub fn take_notice(&self) -> Option<u32> {
        self.notice_dirty.swap(false, Ordering::Relaxed).then(|| self.gen())
    }
    pub fn view(&self) -> WatchlistView<'_> { self.state.view() }
    pub fn run(&mut self, cmd: WatchlistCmd) -> bool {
        let changed = self.state.run(&self.adapter, cmd);
        if changed { self.bump(); }
        changed
    }
    /// `wanted`: a page that reads the store is on top (the Watchlist page or a detail page).
    pub fn pump(&mut self, gate: &plx_machine::landgate::Gate, wanted: bool) -> bool {
        let changed = self.state.pump_with_gate(&self.adapter, gate, wanted);
        if changed { self.bump(); }
        changed
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn state_for_test(&mut self) -> &mut WatchlistState { &mut self.state }
    #[cfg(any(test, feature = "test-support"))]
    pub fn adapter_for_test(&self) -> Arc<WatchlistAdapter> { Arc::clone(&self.adapter) }
    #[cfg(any(test, feature = "test-support"))]
    pub fn install_for_test(&mut self, entries: Vec<crate::watchlist::Entry>) {
        self.state.install_for_test(entries);
        self.bump();
    }
}

impl<H: Host> Machine<H> for WatchlistStore {
    type Ev = StoreEv<WatchlistCmd>;
    fn step(&mut self, ev: &Self::Ev, _cx: &Cx<'_, H>, _fx: &mut Effects<'_, H>) -> Handled {
        match ev {
            StoreEv::Cmd(cmd) => { self.run(cmd.clone()); }
            StoreEv::Pump { .. } => { self.pump(&plx_machine::landgate::Gate::default(), false); }
        }
        Handled::Yes
    }
}
