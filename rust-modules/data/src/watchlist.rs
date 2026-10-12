//! The signed-in profile's **Plex Watchlist**: one physically owned model, one generation-stamped
//! mailbox and one small worker at a time, in the shape of `crate::collection`.
//!
//! The list itself is an ADJACENT catalog: its rows come from the Discover provider
//! (`plx_plex::plex::discover`), not from a server, so each row is matched by its portable guid
//! against the user's own servers (`/library/all?guid=…`, the query *Also available* already
//! makes) to say whether OK can open a real detail page. Four jobs share the one mailbox, in
//! priority order: a queued add/remove (a person pressed something), the list, and the matching in
//! batches of [`MATCH_BATCH`]. Matching is cached per guid for the life of the profile and the
//! server roster, so returning to the page costs one list request and no server queries.
//!
//! **Nothing here runs unless a page wants it.** The application pumps the store with `wanted`
//! true only while the Watchlist page or a detail page is on top (`app::run`), so a session that
//! never looks at either sends nothing to plex.tv. Edits are the exception: an accepted press is
//! sent wherever the person has walked to since. Under the controlled record/replay domain
//! (`stores::tape`) the store stays idle, so a recording never depends on a network it did not
//! record.
//!
//! **Credentials.** The current profile's own plex.tv credential (`session::plex_tv_credential`),
//! or the account token for the owner with no Home profile selected. A managed profile with no
//! credential, or one plex.tv refuses (401/403), reads as [`WatchlistStatus::Unavailable`]: the
//! Watchlist page says so and the detail page offers no Watchlist control. A developer build whose
//! plex.tv is the loopback stand-in also accepts the injected test token, which can reach only
//! that stand-in.

use plx_plex::plex::discover::{WatchItem, WatchlistFailure, WatchlistPage};
use plx_plex::plex::ServerId;
use crate::pms::PmsMovie;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// Guids matched against the servers per job. Small enough that the first cards resolve quickly
/// and a slow server delays only a dozen of them; the batches follow each other back to back.
pub const MATCH_BATCH: usize = 12;
/// Frames before a failed list or match is asked again (about ten seconds).
const RETRY_FRAMES: u32 = 600;

/// What the store can say about the list as a whole.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WatchlistStatus {
    /// Nothing asked yet for this profile.
    #[default]
    Idle,
    /// The first list is on its way.
    Loading,
    /// A list has landed (possibly empty). Later refreshes keep this while they run.
    Ready,
    /// This profile has no Watchlist the app may read: no plex.tv credential, or plex.tv refused
    /// it. Not retried until the profile changes or the page asks again.
    Unavailable,
    /// The list could not be fetched; retried on a timer while a page wants it.
    Failed,
}

/// Whether one of the user's servers holds a watchlist title.
#[derive(Clone)]
pub enum Availability {
    /// Not asked yet, or the answer is on its way.
    Checking,
    /// The first server (current one first) that holds it, as that server's own catalog row: OK
    /// opens that row's detail page, so play and resume work exactly as anywhere else.
    OnServer(Box<PmsMovie>),
    /// No reachable server holds it.
    Missing,
}

impl Availability {
    pub fn on_server(&self) -> Option<&PmsMovie> {
        match self {
            Self::OnServer(row) => Some(row),
            _ => None,
        }
    }
    pub fn missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

/// One title on the list, with what the servers said about it.
#[derive(Clone)]
pub struct Entry {
    pub item: WatchItem,
    pub avail: Availability,
}

/// The read surface a screen gets for one frame.
#[derive(Clone, Copy, Default)]
pub struct WatchlistView<'a> {
    state: Option<&'a WatchlistState>,
}

impl<'a> WatchlistView<'a> {
    pub fn status(self) -> WatchlistStatus {
        self.state.map_or(WatchlistStatus::Idle, |s| s.status)
    }
    /// The list, newest addition first.
    pub fn entries(self) -> &'a [Entry] {
        self.state.map_or(&[], |s| &s.entries)
    }
    /// Moves whenever anything a reader draws moves: the list, an availability, the status.
    pub fn revision(self) -> u64 {
        self.state.map_or(0, |s| s.revision)
    }
    /// Is `guid` on the Watchlist? `None` when the store cannot say: no list has landed for this
    /// profile, the profile has no Watchlist, or `guid` names nothing a Watchlist can hold. A
    /// detail page shows its Watchlist control only on `Some`.
    pub fn membership(self, guid: &str) -> Option<bool> {
        let s = self.state?;
        (s.loaded && s.status == WatchlistStatus::Ready && plx_plex::plex::discover::watchlistable(guid))
            .then(|| s.members.contains(guid))
    }
    /// The server a missing title's provider artwork is transcoded through: the current server
    /// when the list landed (the photo transcoder fetches an absolute URL for any client).
    pub fn art_sid(self) -> ServerId {
        self.state.map_or(ServerId::UNSET, |s| s.art_sid)
    }
    pub fn entry(self, guid: &str) -> Option<&'a Entry> {
        self.entries().iter().find(|e| e.item.guid == guid)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Job {
    Edit { guid: String, add: bool },
    List,
    Match { guids: Vec<String>, servers: Vec<ServerId> },
}

enum Landing {
    List(Result<WatchlistPage, WatchlistFailure>),
    Matched(Vec<(String, Option<PmsMovie>)>),
    Edited { guid: String, add: bool, result: Result<(), WatchlistFailure> },
    /// The worker found no plex.tv credential for this profile.
    NoAccount,
}

struct Mail {
    gen: u32,
    what: Landing,
}

#[derive(Default)]
pub struct WatchlistAdapter {
    fetch: crate::stores::Fetch<Mail>,
}

impl WatchlistAdapter {
    fn land(&self, gen: u32, what: Landing) {
        self.fetch.post(Mail { gen, what }, |old| old.gen <= gen);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn land_list_for_test(&self, gen: u32, items: Vec<WatchItem>) {
        let total = items.len();
        self.land(gen, Landing::List(Ok(WatchlistPage { items, total })));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn land_failure_for_test(&self, gen: u32, failure: WatchlistFailure) {
        self.land(gen, Landing::List(Err(failure)));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn land_matched_for_test(&self, gen: u32, rows: Vec<(String, Option<PmsMovie>)>) {
        self.land(gen, Landing::Matched(rows));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn land_edit_for_test(&self, gen: u32, guid: &str, add: bool, result: Result<(), WatchlistFailure>) {
        self.land(gen, Landing::Edited { guid: guid.into(), add, result });
    }
}

pub struct WatchlistState {
    status: WatchlistStatus,
    entries: Vec<Entry>,
    /// Guids on the list, optimistic edits applied, what [`WatchlistView::membership`] answers.
    members: HashSet<String>,
    /// A list has landed for this profile at least once.
    loaded: bool,
    /// The list must be asked again the next time a page wants it.
    stale: bool,
    /// Resolved availability by guid, kept across list refreshes for this profile and roster.
    matched: HashMap<String, Availability>,
    edits: VecDeque<(String, bool)>,
    art_sid: ServerId,
    /// Request epoch: moves on a reset and a profile change, never on a refresh.
    generation: u32,
    revision: u64,
    profile: Option<u32>,
    roster: Option<u32>,
    retry_cd: u32,
}

impl Default for WatchlistState {
    fn default() -> Self {
        Self {
            status: WatchlistStatus::Idle, entries: Vec::new(), members: HashSet::new(),
            loaded: false, stale: true, matched: HashMap::new(), edits: VecDeque::new(),
            art_sid: ServerId::UNSET, generation: 1, revision: 0, profile: None, roster: None,
            retry_cd: 0,
        }
    }
}

impl WatchlistState {
    pub fn view(&self) -> WatchlistView<'_> {
        WatchlistView { state: Some(self) }
    }

    fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Forget everything: a new profile, a sign-out. The request epoch moves, so a worker still
    /// out answers into nothing.
    fn reset(&mut self, adapter: &WatchlistAdapter) {
        let (generation, profile, roster) = (self.generation, self.profile, self.roster);
        *self = Self::default();
        self.generation = generation.wrapping_add(1).max(1);
        self.profile = profile;
        self.roster = roster;
        adapter.fetch.clear();
        self.bump();
    }

    pub fn run(&mut self, adapter: &WatchlistAdapter, cmd: crate::stores::watchlist::WatchlistCmd) -> bool {
        use crate::stores::watchlist::WatchlistCmd;
        let changed = match cmd {
            WatchlistCmd::Open => {
                // Asked again on the next wanted pump, keeping what is on screen meanwhile; a
                // refused or failed list is retried at once rather than on its timer.
                self.stale = true;
                self.retry_cd = 0;
                if matches!(self.status, WatchlistStatus::Unavailable | WatchlistStatus::Failed) {
                    self.status = if self.loaded { WatchlistStatus::Ready } else { WatchlistStatus::Idle };
                    true
                } else {
                    false
                }
            }
            WatchlistCmd::Add { item, row } => self.edit(item.guid.clone(), true, Some((item, row))),
            WatchlistCmd::Remove { guid } => self.edit(guid, false, None),
            WatchlistCmd::Reset => {
                self.reset(adapter);
                true
            }
        };
        if changed {
            self.bump();
        }
        changed
    }

    /// The optimistic half of an edit: membership and the list change NOW, the request is queued.
    /// A queued opposite edit for the same title cancels instead of sending two requests.
    fn edit(&mut self, guid: String, add: bool, item: Option<(WatchItem, Option<Box<PmsMovie>>)>) -> bool {
        if !plx_plex::plex::discover::watchlistable(&guid) || self.members.contains(&guid) == add {
            return false;
        }
        if add {
            self.members.insert(guid.clone());
            if let Some((item, row)) = item {
                let avail = row.map_or_else(|| self.matched.get(&guid).cloned().unwrap_or(Availability::Checking),
                    Availability::OnServer);
                self.entries.insert(0, Entry { item, avail });
            }
        } else {
            self.members.remove(&guid);
            self.entries.retain(|e| e.item.guid != guid);
        }
        match self.edits.iter().position(|(g, a)| *g == guid && *a != add) {
            Some(i) => { self.edits.remove(i); }
            None => self.edits.push_back((guid, add)),
        }
        true
    }

    /// The once-a-frame pass: notice a profile or roster change, land what arrived, start the next
    /// job. `wanted` says a page is showing that reads the store.
    pub fn pump_with_gate(&mut self, adapter: &Arc<WatchlistAdapter>, gate: &plx_machine::landgate::Gate,
        wanted: bool) -> bool {
        if crate::stores::tape::active() {
            return false;
        }
        let mut changed = false;
        let profile = plx_plex::plex::session::current_gen();
        if self.profile != Some(profile) {
            if self.profile.is_some() {
                self.reset(adapter);
                changed = true;
            }
            self.profile = Some(profile);
        }
        let roster = plx_plex::plex::server_roster_gen();
        if self.roster != Some(roster) {
            if self.roster.is_some() && !self.matched.is_empty() {
                self.matched.clear();
                for e in &mut self.entries { e.avail = Availability::Checking; }
                self.bump();
                changed = true;
            }
            self.roster = Some(roster);
        }
        if self.retry_cd > 0 {
            self.retry_cd -= 1;
        }
        let mail = crate::stores::take_landing_owed(gate, crate::stores::StoreId::Watchlist,
            || adapter.fetch.busy(), || adapter.fetch.take_current(|m| m.gen));
        if let Some(mail) = mail {
            plx_machine::idle::invalidate();
            if mail.gen == self.generation && self.apply(mail.what) {
                self.bump();
                changed = true;
            }
        }
        changed |= self.maybe_spawn(adapter, wanted);
        changed
    }

    fn next_job(&self, wanted: bool) -> Option<Job> {
        if let Some((guid, add)) = self.edits.front() {
            return Some(Job::Edit { guid: guid.clone(), add: *add });
        }
        if !wanted || self.retry_cd > 0 || self.status == WatchlistStatus::Unavailable {
            return None;
        }
        if self.stale || self.status == WatchlistStatus::Idle {
            return Some(Job::List);
        }
        let guids: Vec<String> = self.entries.iter()
            .filter(|e| matches!(e.avail, Availability::Checking))
            .map(|e| e.item.guid.clone())
            .take(MATCH_BATCH)
            .collect();
        if guids.is_empty() {
            return None;
        }
        // The current server first: the copy OK opens should be the one being browsed.
        let current = plx_plex::plex::current_server();
        let mut servers: Vec<ServerId> = plx_plex::plex::server_ids().collect();
        servers.sort_by_key(|sid| *sid != current);
        Some(Job::Match { guids, servers })
    }

    fn maybe_spawn(&mut self, adapter: &Arc<WatchlistAdapter>, wanted: bool) -> bool {
        if adapter.fetch.busy() {
            return false;
        }
        let Some(job) = self.next_job(wanted) else { return false };
        let mut changed = false;
        match &job {
            Job::Edit { .. } => { self.edits.pop_front(); }
            Job::List => {
                self.stale = false;
                if self.status == WatchlistStatus::Idle {
                    self.status = WatchlistStatus::Loading;
                    self.bump();
                    changed = true;
                }
            }
            Job::Match { .. } => {}
        }
        let gen = self.generation;
        adapter.fetch.claim(gen);
        let worker = Arc::clone(adapter);
        let spawned = plx_base::task::spawn_small("watchlist", move || {
            let what = std::panic::catch_unwind(|| run_job(job)).unwrap_or(Landing::List(Err(WatchlistFailure::Unreachable)));
            worker.land(gen, what);
        });
        if !spawned {
            adapter.fetch.release();
        }
        changed
    }

    fn apply(&mut self, landing: Landing) -> bool {
        match landing {
            Landing::NoAccount => {
                self.status = WatchlistStatus::Unavailable;
                true
            }
            Landing::List(Ok(page)) => {
                self.art_sid = plx_plex::plex::current_server();
                self.members = page.items.iter().map(|i| i.guid.clone()).collect();
                // An edit still queued is the person's latest word on that title.
                for (guid, add) in &self.edits {
                    if *add { self.members.insert(guid.clone()); } else { self.members.remove(guid); }
                }
                self.entries = page.items.into_iter()
                    .filter(|item| !self.edits.iter().any(|(g, add)| !add && *g == item.guid))
                    .map(|item| {
                        let avail = self.matched.get(&item.guid).cloned().unwrap_or(Availability::Checking);
                        Entry { item, avail }
                    })
                    .collect();
                self.loaded = true;
                self.status = WatchlistStatus::Ready;
                true
            }
            Landing::List(Err(WatchlistFailure::Refused(status))) => {
                plx_base::eventlog::log(&format!("watchlist: plex.tv refused this profile (HTTP {status}); no Watchlist shown"));
                self.status = WatchlistStatus::Unavailable;
                true
            }
            Landing::List(Err(WatchlistFailure::Unreachable)) => {
                self.retry_cd = RETRY_FRAMES;
                self.stale = true;
                if self.loaded {
                    false
                } else {
                    self.status = WatchlistStatus::Failed;
                    true
                }
            }
            Landing::Matched(rows) => {
                let mut hit = false;
                for (guid, row) in rows {
                    let avail = row.map_or(Availability::Missing, |row| Availability::OnServer(Box::new(row)));
                    for e in self.entries.iter_mut().filter(|e| e.item.guid == guid) {
                        e.avail = avail.clone();
                        hit = true;
                    }
                    self.matched.insert(guid, avail);
                }
                hit
            }
            Landing::Edited { guid, add, result } => match result {
                Ok(()) => {
                    // An add from a detail page has no provider row yet: the next look at the
                    // page asks for the list rather than guessing its order.
                    if add { self.stale = true; }
                    false
                }
                Err(failure) => {
                    plx_base::eventlog::log(&format!("watchlist: an {} was not accepted ({failure:?}); reverted",
                        if add { "add" } else { "remove" }));
                    if add {
                        self.members.remove(&guid);
                        self.entries.retain(|e| e.item.guid != guid);
                    } else {
                        self.members.insert(guid);
                    }
                    self.stale = true;
                    true
                }
            },
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn generation(&self) -> u32 { self.generation }

    #[cfg(any(test, feature = "test-support"))]
    pub fn take_landing_for_test(&mut self, adapter: &WatchlistAdapter) -> bool {
        let Some(mail) = adapter.fetch.take() else { return false };
        let changed = mail.gen == self.generation && self.apply(mail.what);
        if changed { self.bump(); }
        changed
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn next_job_for_test(&self, wanted: bool) -> Option<(&'static str, Vec<String>)> {
        self.next_job(wanted).map(|job| match job {
            Job::Edit { guid, add } => (if add { "add" } else { "remove" }, vec![guid]),
            Job::List => ("list", Vec::new()),
            Job::Match { guids, .. } => ("match", guids),
        })
    }

    /// Start the job [`Self::next_job_for_test`] names without a worker: what a spawn does to the
    /// state, so a test can then land its answer.
    #[cfg(any(test, feature = "test-support"))]
    pub fn start_for_test(&mut self, wanted: bool) {
        match self.next_job(wanted) {
            Some(Job::Edit { .. }) => { self.edits.pop_front(); }
            Some(Job::List) => {
                self.stale = false;
                if self.status == WatchlistStatus::Idle { self.status = WatchlistStatus::Loading; self.bump(); }
            }
            _ => {}
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn install_for_test(&mut self, entries: Vec<Entry>) {
        self.members = entries.iter().map(|e| e.item.guid.clone()).collect();
        self.entries = entries;
        self.loaded = true;
        self.stale = false;
        self.status = WatchlistStatus::Ready;
        self.bump();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_status_for_test(&mut self, status: WatchlistStatus) {
        self.status = status;
        self.bump();
    }
}

/// WORKER THREAD.
fn run_job(job: Job) -> Landing {
    match job {
        Job::Match { guids, servers } => Landing::Matched(guids.into_iter()
            .map(|guid| { let row = find_on_servers(&guid, &servers); (guid, row) })
            .collect()),
        Job::List => match client() {
            Some(client) => Landing::List(list(&client)),
            None => Landing::NoAccount,
        },
        Job::Edit { guid, add } => {
            let result = match client() {
                Some(client) => edit(&client, &guid, add),
                None => Err(WatchlistFailure::Refused(401)),
            };
            Landing::Edited { guid, add, result }
        }
    }
}

/// WORKER THREAD: the first of `servers` that holds `guid`, as that server's own row.
fn find_on_servers(guid: &str, servers: &[ServerId]) -> Option<PmsMovie> {
    for &sid in servers {
        let Some(client) = plx_plex::plex::client_for(sid) else { continue };
        let Some(mc) = client.find_by_guid(guid) else { continue };
        if let Some(row) = matching_row(&mc.metadata, guid) {
            return Some(crate::pms::parse_item(row, sid));
        }
    }
    None
}

/// The row of a `/library/all?guid=` answer that IS this title: same guid, a movie or a show.
/// A server that ignored the filter answers its whole library, and its first row is not the title.
pub fn matching_row<'a>(rows: &'a [plx_plex::plex::Metadata], guid: &str) -> Option<&'a plx_plex::plex::Metadata> {
    rows.iter().find(|row| row.guid == guid && matches!(row.kind.as_str(), "movie" | "show") && !row.rating_key.is_empty())
}

/// WORKER THREAD: this profile's plex.tv identity, `None` when it has none.
#[cfg(not(any(test, feature = "test-support")))]
fn client() -> Option<plx_plex::plex::account::AccountClient> {
    let snapshot = plx_plex::plex::session::current_snapshot();
    let session = plx_plex::plex::session::peek();
    let token = match snapshot.user.as_ref() {
        Some(user) => plx_plex::plex::session::plex_tv_credential(user),
        None => Some(session.account_token.clone()).filter(|t| !t.is_empty()),
    };
    // A developer build pointed at the loopback stand-in signs in with the injected test token
    // and no account; the stand-in is the only host that token can reach from here.
    let token = token.or_else(|| plx_plex::plex::account::plex_tv_stand_in()
        .then(|| plx_base::devtrig::read("token")).flatten().filter(|t| !t.is_empty()));
    let token = token?;
    Some(plx_plex::plex::account::AccountClient::new(&session.client_id, Some(&token)))
}

#[cfg(not(any(test, feature = "test-support")))]
fn list(client: &plx_plex::plex::account::AccountClient) -> Result<WatchlistPage, WatchlistFailure> {
    client.watchlist()
}

#[cfg(not(any(test, feature = "test-support")))]
fn edit(client: &plx_plex::plex::account::AccountClient, guid: &str, add: bool) -> Result<(), WatchlistFailure> {
    client.watchlist_edit(guid, add)
}

/// HOST SUITE: the plex.tv half is cut, as `person.rs` cuts its biography request, the logic
/// worth testing is the landing, which the tests drive through the adapter.
#[cfg(any(test, feature = "test-support"))]
fn client() -> Option<plx_plex::plex::account::AccountClient> {
    None
}

#[cfg(any(test, feature = "test-support"))]
fn list(_client: &plx_plex::plex::account::AccountClient) -> Result<WatchlistPage, WatchlistFailure> {
    Err(WatchlistFailure::Unreachable)
}

#[cfg(any(test, feature = "test-support"))]
fn edit(_client: &plx_plex::plex::account::AccountClient, _guid: &str, _add: bool) -> Result<(), WatchlistFailure> {
    Err(WatchlistFailure::Unreachable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::watchlist::WatchlistCmd;

    fn item(guid: &str, title: &str) -> WatchItem {
        WatchItem { guid: guid.into(), kind: if guid.contains("show") { "show" } else { "movie" }.into(),
            title: title.into(), ..Default::default() }
    }

    fn row(rk: &str) -> PmsMovie {
        PmsMovie { rk: rk.into(), title: format!("row {rk}"), ..Default::default() }
    }

    const A: &str = "plex://movie/aaa";
    const B: &str = "plex://show/bbb";
    const C: &str = "plex://movie/ccc";

    fn landed(adapter: &WatchlistAdapter) -> WatchlistState {
        let mut state = WatchlistState::default();
        assert_eq!(state.next_job_for_test(true).map(|j| j.0), Some("list"));
        state.start_for_test(true);
        assert_eq!(state.view().status(), WatchlistStatus::Loading);
        adapter.land_list_for_test(state.generation(), vec![item(A, "A"), item(B, "B"), item(C, "C")]);
        assert!(state.take_landing_for_test(adapter));
        state
    }

    /// The list lands in wire order (newest first), every title Checking, and membership answers
    /// only once a list is in.
    #[test]
    fn a_landed_list_keeps_wire_order_and_answers_membership() {
        let adapter = WatchlistAdapter::default();
        let state = WatchlistState::default();
        assert_eq!(state.view().membership(A), None, "nothing landed: the detail control stays hidden");
        let state2 = landed(&adapter);
        let view = state2.view();
        assert_eq!(view.status(), WatchlistStatus::Ready);
        assert_eq!(view.entries().iter().map(|e| e.item.title.as_str()).collect::<Vec<_>>(), ["A", "B", "C"]);
        assert!(view.entries().iter().all(|e| matches!(e.avail, Availability::Checking)));
        assert_eq!(view.membership(A), Some(true));
        assert_eq!(view.membership("plex://movie/zzz"), Some(false));
        assert_eq!(view.membership("plex://episode/zzz"), None, "an episode is never watchlisted");
        drop(state);
    }

    /// Matching runs in bounded batches, only while a page wants it, and its answers stick per
    /// guid: a refreshed list re-uses them instead of asking the servers again.
    #[test]
    fn availability_is_matched_in_batches_and_cached_across_a_refresh() {
        let adapter = WatchlistAdapter::default();
        let mut state = landed(&adapter);
        assert_eq!(state.next_job_for_test(false), None, "no page wants it: nothing is asked");
        let (kind, guids) = state.next_job_for_test(true).unwrap();
        assert_eq!((kind, guids.as_slice()), ("match", [A.to_string(), B.to_string(), C.to_string()].as_slice()));
        adapter.land_matched_for_test(state.generation(), vec![(A.into(), Some(row("101"))), (B.into(), None), (C.into(), Some(row("103")))]);
        assert!(state.take_landing_for_test(&adapter));
        let view = state.view();
        assert_eq!(view.entry(A).unwrap().avail.on_server().map(|r| r.rk.as_str()), Some("101"));
        assert!(view.entry(B).unwrap().avail.missing());
        assert_eq!(state.next_job_for_test(true), None, "everything answered");

        state.run(&adapter, WatchlistCmd::Open);
        assert_eq!(state.next_job_for_test(true).map(|j| j.0), Some("list"), "a revisit refreshes the list");
        state.start_for_test(true);
        adapter.land_list_for_test(state.generation(), vec![item(C, "C"), item(A, "A")]);
        assert!(state.take_landing_for_test(&adapter));
        assert!(state.view().entries().iter().all(|e| !matches!(e.avail, Availability::Checking)),
            "the cached answers carried over");
        assert_eq!(state.next_job_for_test(true), None);
    }

    #[test]
    fn a_batch_is_bounded() {
        let adapter = WatchlistAdapter::default();
        let mut state = WatchlistState::default();
        state.start_for_test(true);
        let items = (0..MATCH_BATCH * 2 + 3).map(|n| item(&format!("plex://movie/m{n}"), "T")).collect();
        adapter.land_list_for_test(state.generation(), items);
        assert!(state.take_landing_for_test(&adapter));
        assert_eq!(state.next_job_for_test(true).unwrap().1.len(), MATCH_BATCH);
    }

    /// Remove is optimistic (membership and the list change on the press), is the next job, and a
    /// refused request puts the title back.
    #[test]
    fn a_remove_is_optimistic_and_a_refusal_reverts_it() {
        let adapter = WatchlistAdapter::default();
        let mut state = landed(&adapter);
        assert!(state.run(&adapter, WatchlistCmd::Remove { guid: B.into() }));
        assert_eq!(state.view().membership(B), Some(false));
        assert!(state.view().entry(B).is_none());
        assert_eq!(state.next_job_for_test(false), Some(("remove", vec![B.to_string()])),
            "an edit is sent whether or not a page is still showing");
        state.start_for_test(false);
        adapter.land_edit_for_test(state.generation(), B, false, Err(WatchlistFailure::Refused(401)));
        assert!(state.take_landing_for_test(&adapter));
        assert_eq!(state.view().membership(B), Some(true), "reverted");
        assert_eq!(state.next_job_for_test(true).map(|j| j.0), Some("list"), "and the list is asked again");
    }

    /// Add from a detail page: membership flips at once and the title leads the list with the
    /// server row the page already had; the opposite press before it is sent cancels it.
    #[test]
    fn an_add_leads_the_list_and_an_opposite_press_cancels_it() {
        let adapter = WatchlistAdapter::default();
        let mut state = landed(&adapter);
        let d = "plex://movie/ddd";
        assert!(state.run(&adapter, WatchlistCmd::Add { item: item(d, "D"), row: Some(Box::new(row("104"))) }));
        assert_eq!(state.view().membership(d), Some(true));
        assert_eq!(state.view().entries()[0].item.title, "D");
        assert!(state.view().entries()[0].avail.on_server().is_some());
        assert!(!state.run(&adapter, WatchlistCmd::Add { item: item(d, "D"), row: None }), "already on it");
        assert!(state.run(&adapter, WatchlistCmd::Remove { guid: d.into() }));
        assert_eq!(state.next_job_for_test(false), None, "add then remove sends nothing");
        assert!(!state.run(&adapter, WatchlistCmd::Add { item: item("plex://episode/e", "E"), row: None }),
            "only a movie or a show can be added");
    }

    /// A profile plex.tv refuses reads as Unavailable and is not asked again on its own; the page
    /// opening again asks once more. A transport failure before any list is Failed and retried.
    #[test]
    fn a_refused_profile_is_unavailable_and_a_failure_retries() {
        let adapter = WatchlistAdapter::default();
        let mut state = WatchlistState::default();
        state.start_for_test(true);
        adapter.land_failure_for_test(state.generation(), WatchlistFailure::Refused(403));
        assert!(state.take_landing_for_test(&adapter));
        assert_eq!(state.view().status(), WatchlistStatus::Unavailable);
        assert_eq!(state.view().membership(A), None, "the detail page offers no control");
        assert_eq!(state.next_job_for_test(true), None);
        assert!(state.run(&adapter, WatchlistCmd::Open));
        assert_eq!(state.next_job_for_test(true).map(|j| j.0), Some("list"));

        state.start_for_test(true);
        adapter.land_failure_for_test(state.generation(), WatchlistFailure::Unreachable);
        assert!(state.take_landing_for_test(&adapter));
        assert_eq!(state.view().status(), WatchlistStatus::Failed);
        assert_eq!(state.next_job_for_test(true), None, "the retry waits out its timer");
    }

    /// A reset (sign-out, profile switch) moves the request epoch: a worker still out lands into
    /// nothing.
    #[test]
    fn a_reset_discards_a_late_answer() {
        let adapter = WatchlistAdapter::default();
        let mut state = WatchlistState::default();
        state.start_for_test(true);
        let old = state.generation();
        state.run(&adapter, WatchlistCmd::Reset);
        adapter.land_list_for_test(old, vec![item(A, "A")]);
        assert!(!state.take_landing_for_test(&adapter));
        assert_eq!(state.view().status(), WatchlistStatus::Idle);
    }

    /// `/library/all?guid=` from a server that ignored the filter answers other titles; only the
    /// row carrying this guid counts.
    #[test]
    fn the_matching_row_is_the_one_with_this_guid() {
        let rows: Vec<plx_plex::plex::Metadata> = serde_json::from_str(r#"[
            {"ratingKey":"7","type":"movie","guid":"plex://movie/other","title":"Other"},
            {"ratingKey":"8","type":"episode","guid":"plex://movie/aaa","title":"Odd"},
            {"ratingKey":"9","type":"movie","guid":"plex://movie/aaa","title":"This"}]"#).unwrap();
        assert_eq!(matching_row(&rows, A).map(|r| r.rating_key.as_str()), Some("9"));
        assert!(matching_row(&rows[..1], A).is_none());
    }
}
