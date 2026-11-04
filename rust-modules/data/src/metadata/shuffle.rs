//! **Shuffle Show / Shuffle Season**: the server's shuffled PlayQueue of every episode under a show
//! or one season, asked for as a job and answered into this owner's mailbox.
//!
//! `MetadataCmd::RequestShuffle` records the request (its generation) and leaves a
//! `StoreJob::MetaShuffle` on the outbox; the step starts no thread and reads no registry. The
//! application's runner resolves the server's client at the launch and runs
//! [`finish_shuffle_job`], whose one POST (`Client::create_shuffled_play_queue`, after a
//! `GET /identity` when the client does not know its server's machine id) is the whole read. The
//! answer comes back through [`MetaShufflePort`] and waits until the application takes it
//! ([`take_shuffle`], `MetadataStore::take_shuffle`), which then plays the queue's first row
//! through the player's queue continuation. A newer request supersedes an older one: an answer for
//! any generation but the latest is dropped.
//!
//! `Direct`: a controlled run never emits it (`bootstrap/effects.rs` has no arm for the command),
//! so the launch is the whole decision.

use super::{MetadataAdapter, MetadataState};
use plx_plex::plex::{Client, QueueRow, ServerId};

/// What one Shuffle asks for: the container's own server and ratingKey, a show's or a season's.
#[derive(Clone, Debug, PartialEq)]
pub struct MetaShuffleReq {
    pub sid: ServerId,
    pub container_rk: String,
}

/// A shuffled queue the server made: its id, the rows of the window it answered (in the server's
/// order) and the row it selected to play first. Every row is on `sid`.
#[derive(Clone)]
pub struct ShuffledQueue {
    pub sid: ServerId,
    pub id: i64,
    pub selected_item_id: i64,
    pub rows: Vec<QueueRow>,
}

impl ShuffledQueue {
    /// The row the server selected, by its `playQueueItemID` (a shuffled queue has no ratingKey of
    /// the caller's to fall back on).
    pub fn first(&self) -> Option<&QueueRow> {
        self.rows.iter().find(|r| r.item_id != 0 && r.item_id == self.selected_item_id)
    }
}

/// The latest Shuffle request and, once it has answered, its answer.
#[derive(Default)]
pub(super) struct ShuffleSlot {
    /// The latest request's generation; 0 before the first.
    sent: u32,
    /// That request's answer: `None` inside for a queue the server did not make (a failed POST, a
    /// server the runner held no client for, a refused launch, a first row that cannot play).
    landed: Option<(u32, Option<ShuffledQueue>)>,
}

/// A Shuffle job's address: its generation. Another kind of Metadata job can carry an identical
/// `Addr` (see `pages_addr`); nothing routes by it alone.
fn shuffle_addr(gen: u32) -> plx_machine::machine::Addr {
    super::season_addr(gen)
}

/// [`crate::stores::metadata::MetadataCmd::RequestShuffle`]: supersede any Shuffle still out and
/// leave this one's POST on the outbox. False (and nothing emitted) for an empty ratingKey.
pub(super) fn request_shuffle(
    state: &mut MetadataState,
    adapter: &std::sync::Arc<MetadataAdapter>,
    sid: ServerId,
    container_rk: String,
) -> bool {
    if container_rk.is_empty() {
        plx_base::eventlog::log("shuffle: a request with no ratingKey was dropped");
        return false;
    }
    let gen = {
        let mut slot = adapter.shuffle.lock().unwrap_or_else(|e| e.into_inner());
        slot.sent = slot.sent.wrapping_add(1).max(1);
        slot.landed = None;
        slot.sent
    };
    state.jobs.push(crate::stores::StoreJob::MetaShuffle(
        plx_machine::job::Job { addr: shuffle_addr(gen), req: MetaShuffleReq { sid, container_rk } },
        MetaShufflePort::of(adapter),
    ));
    true
}

/// The latest Shuffle request's generation: what a caller that awaits ITS answer reads right after
/// sending `RequestShuffle`, and hands to [`shuffle_answered`] / [`take_shuffle`].
pub(super) fn shuffle_gen(adapter: &MetadataAdapter) -> u32 {
    adapter.shuffle.lock().unwrap_or_else(|e| e.into_inner()).sent
}

/// Has request `gen` answered, while it is still the latest? `Some(true)` for a queue to play,
/// `Some(false)` when the server made none, `None` while it is out (or superseded).
pub(super) fn shuffle_answered(adapter: &MetadataAdapter, gen: u32) -> Option<bool> {
    let slot = adapter.shuffle.lock().unwrap_or_else(|e| e.into_inner());
    if slot.sent != gen {
        return None;
    }
    slot.landed.as_ref().filter(|(g, _)| *g == gen).map(|(_, answer)| answer.is_some())
}

/// Take request `gen`'s answer: `Some(Some(queue))` for a queue to play, `Some(None)` when the
/// server made none, `None` while it is still out or when a newer request superseded it.
pub(crate) fn take_shuffle(adapter: &MetadataAdapter, gen: u32) -> Option<Option<ShuffledQueue>> {
    let mut slot = adapter.shuffle.lock().unwrap_or_else(|e| e.into_inner());
    if slot.sent != gen || !slot.landed.as_ref().is_some_and(|(g, _)| *g == gen) {
        return None;
    }
    slot.landed.take().map(|(_, answer)| answer)
}

/// Land request `gen`'s answer, unless a newer request has superseded it.
fn land(adapter: &MetadataAdapter, gen: u32, answer: Option<ShuffledQueue>) {
    let mut slot = adapter.shuffle.lock().unwrap_or_else(|e| e.into_inner());
    if slot.sent == gen {
        slot.landed = Some((gen, answer));
    }
}

/// Where a Shuffle's ONE answer goes: the mailbox of the adapter the request was made from (an
/// owner that has since been `Reset` holds a new one; this port still lands into the retired one
/// nobody reads). Two ports are equal when they reach the same adapter.
#[derive(Clone)]
pub struct MetaShufflePort(std::sync::Arc<MetadataAdapter>);

impl MetaShufflePort {
    pub(crate) fn of(adapter: &std::sync::Arc<MetadataAdapter>) -> Self {
        Self(std::sync::Arc::clone(adapter))
    }
}

impl PartialEq for MetaShufflePort {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for MetaShufflePort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MetaShufflePort({:p})", std::sync::Arc::as_ptr(&self.0))
    }
}

impl plx_machine::job::Port<Option<ShuffledQueue>> for MetaShufflePort {
    fn done(&self, addr: plx_machine::machine::Addr, answer: Option<ShuffledQueue>) {
        land(&self.0, addr.req.0, answer);
    }

    /// Never started: the request answers as a queue the server did not make, so the caller that
    /// waits on it stops waiting.
    fn refused(&self, addr: plx_machine::machine::Addr) {
        land(&self.0, addr.req.0, None);
    }
}

/// The Shuffle's IO body and its completion: ask the server for the shuffled queue through `client`
/// and post the answer to `port`. `client` is the one the runner resolved at the launch; `None` is a
/// server it held none for, which answers the failure without a request. The answer is posted
/// OUTSIDE the guard, so a read that panics still lands, as a failure.
///
/// WORKER THREAD.
pub fn finish_shuffle_job(
    port: &MetaShufflePort,
    addr: plx_machine::machine::Addr,
    client: Option<&'static Client>,
    req: MetaShuffleReq,
) {
    finish_shuffle_with(port, addr, client, req, read_shuffle);
}

fn finish_shuffle_with(
    port: &MetaShufflePort,
    addr: plx_machine::machine::Addr,
    client: Option<&'static Client>,
    req: MetaShuffleReq,
    read: impl FnOnce(&'static Client, &MetaShuffleReq) -> Option<ShuffledQueue>,
) {
    use plx_machine::job::Port;
    let answer = super::read_through(client, |client| read(client, &req));
    if answer.is_none() {
        plx_base::eventlog::log(&format!("shuffle: no queue for rk={}", req.container_rk));
    }
    port.done(addr, answer);
}

/// The request itself: this server's machine id (the registry's, else `GET /identity`), then the
/// shuffled POST. A queue whose selected row has no ratingKey or no part is no queue to play: a
/// server that answered the container's seasons instead of their episodes lands here, and the log
/// line names the kind it sent.
fn read_shuffle(client: &'static Client, req: &MetaShuffleReq) -> Option<ShuffledQueue> {
    let known = client.machine_id();
    let machine_id = if known.is_empty() { client.machine_identity()? } else { known.to_string() };
    let q = client.create_shuffled_play_queue(&machine_id, &req.container_rk)?;
    let queue = ShuffledQueue { sid: req.sid, id: q.id, selected_item_id: q.selected_item_id, rows: q.items };
    let playable = queue.first().is_some_and(|r| !r.rk.is_empty() && !r.part.is_empty());
    plx_base::eventlog::log(&format!(
        "shuffle: queue id={} rows={} selected={} first={} for rk={}",
        queue.id,
        queue.rows.len(),
        queue.selected_item_id,
        queue.first().map_or("-", |r| r.kind.as_str()),
        req.container_rk,
    ));
    (queue.id > 0 && playable).then_some(queue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::metadata::{MetadataCmd, MetadataStore};
    use crate::stores::StoreJob;

    fn row(rk: &str, item_id: i64) -> QueueRow {
        QueueRow {
            item_id,
            rk: rk.into(),
            kind: "episode".into(),
            part: format!("/library/parts/{rk}/file.mkv"),
            ..Default::default()
        }
    }

    fn queue(sid: ServerId) -> ShuffledQueue {
        ShuffledQueue { sid, id: 7, selected_item_id: 702, rows: vec![row("30", 702), row("10", 703)] }
    }

    /// Take the one Shuffle job the store emitted.
    fn the_job(store: &mut MetadataStore) -> (plx_machine::job::Job<MetaShuffleReq>, MetaShufflePort) {
        let mut jobs = store.take_jobs();
        assert_eq!(jobs.len(), 1, "one job per Shuffle");
        let StoreJob::MetaShuffle(job, port) = jobs.remove(0) else { panic!("a Shuffle emits a MetaShuffle job") };
        (job, port)
    }

    /// The step only asks: one job, addressed by the request's generation and naming the container,
    /// on this owner's port; nothing has landed until the job answers.
    #[test]
    fn a_shuffle_leaves_one_post_on_the_outbox_and_nothing_landed() {
        let sid = ServerId::from_raw(3);
        let mut store = MetadataStore::default();
        assert!(store.run(MetadataCmd::RequestShuffle { sid, rk: "100".into() }));
        let gen = store.view().shuffle_gen();
        assert_eq!(gen, 1);
        plx_machine::assert_fx!(store.take_jobs(), [StoreJob::MetaShuffle(
            plx_machine::job::Job { addr: shuffle_addr(gen), req: MetaShuffleReq { sid, container_rk: "100".into() } },
            MetaShufflePort::of(&store.adapter_for_test()),
        )]);
        assert_eq!(store.view().shuffle_answered(gen), None);
        assert!(store.take_shuffle(gen).is_none(), "still out");
        // an empty key asks for nothing
        assert!(!store.run(MetadataCmd::RequestShuffle { sid, rk: String::new() }));
        plx_machine::assert_fx!(store.take_jobs(), []);
    }

    /// The answer lands into the mailbox and is taken once, by the generation that asked for it.
    #[test]
    fn the_answer_is_taken_once_by_the_request_that_asked() {
        use plx_machine::job::Port;
        let sid = ServerId::from_raw(3);
        let mut store = MetadataStore::default();
        store.run(MetadataCmd::RequestShuffle { sid, rk: "100".into() });
        let gen = store.view().shuffle_gen();
        let (job, port) = the_job(&mut store);
        port.done(job.addr, Some(queue(sid)));
        assert_eq!(store.view().shuffle_answered(gen), Some(true));
        let landed = store.take_shuffle(gen).expect("answered").expect("a queue");
        assert_eq!(landed.id, 7);
        assert_eq!(landed.first().map(|r| r.rk.as_str()), Some("30"), "the server's selected row");
        assert!(store.take_shuffle(gen).is_none(), "taken once");
    }

    /// A second Shuffle before the first answered supersedes it: the first's late answer is dropped,
    /// and only the second's is taken.
    #[test]
    fn a_newer_shuffle_drops_the_older_ones_answer() {
        use plx_machine::job::Port;
        let sid = ServerId::from_raw(3);
        let mut store = MetadataStore::default();
        store.run(MetadataCmd::RequestShuffle { sid, rk: "100".into() });
        let first = store.view().shuffle_gen();
        let (old, old_port) = the_job(&mut store);
        store.run(MetadataCmd::RequestShuffle { sid, rk: "200".into() });
        let second = store.view().shuffle_gen();
        assert_ne!(first, second);
        let (new, new_port) = the_job(&mut store);
        assert_eq!(new.req.container_rk, "200");
        old_port.done(old.addr, Some(queue(sid)));
        assert_eq!(store.view().shuffle_answered(second), None, "the older answer is not the newer request's");
        assert_eq!(store.view().shuffle_answered(first), None, "and no longer anybody's");
        assert!(store.take_shuffle(first).is_none(), "nobody can take it");
        new_port.done(new.addr, None);
        assert_eq!(store.view().shuffle_answered(second), Some(false));
        assert!(matches!(store.take_shuffle(second), Some(None)), "the newer request's failure is its answer");
    }

    /// A launch that never started answers as a queue the server did not make, so the wait ends.
    #[test]
    fn a_refused_launch_answers_as_no_queue() {
        use plx_machine::job::Port;
        let sid = ServerId::from_raw(3);
        let mut store = MetadataStore::default();
        store.run(MetadataCmd::RequestShuffle { sid, rk: "100".into() });
        let gen = store.view().shuffle_gen();
        let (job, port) = the_job(&mut store);
        port.refused(job.addr);
        assert!(matches!(store.take_shuffle(gen), Some(None)));
    }

    /// The body posts exactly one answer: a server the runner held no client for answers the failure
    /// without a request, and a read that panics still answers, as a failure.
    #[test]
    fn the_body_answers_once_even_without_a_client_or_after_a_panic() {
        let sid = ServerId::from_raw(3);
        let mut store = MetadataStore::default();
        store.run(MetadataCmd::RequestShuffle { sid, rk: "100".into() });
        let gen = store.view().shuffle_gen();
        let (job, port) = the_job(&mut store);
        finish_shuffle_with(&port, job.addr, None, job.req, |_, _| panic!("no client, no read"));
        assert!(matches!(store.take_shuffle(gen), Some(None)));

        let _serial = plx_base::testlock::serial();
        plx_plex::plex::reset_servers_for_test();
        let held = plx_plex::plex::register_for_test("shuffle-body", "127.0.0.1", 1, "t", "c-shuffle-body");
        let client = plx_plex::plex::client_for(held);
        store.run(MetadataCmd::RequestShuffle { sid: held, rk: "100".into() });
        let gen = store.view().shuffle_gen();
        let (job, port) = the_job(&mut store);
        finish_shuffle_with(&port, job.addr, client, job.req, |_, _| panic!("the read fails"));
        assert!(matches!(store.take_shuffle(gen), Some(None)), "a panicking read lands as a failure");
        let (sid_seen, rk_seen) = {
            store.run(MetadataCmd::RequestShuffle { sid: held, rk: "200".into() });
            let gen = store.view().shuffle_gen();
            let (job, port) = the_job(&mut store);
            let seen = std::sync::Mutex::new(None);
            finish_shuffle_with(&port, job.addr, client, job.req, |_, req| {
                *seen.lock().unwrap() = Some((req.sid, req.container_rk.clone()));
                Some(queue(req.sid))
            });
            assert!(store.take_shuffle(gen).is_some_and(|q| q.is_some()));
            seen.into_inner().unwrap().expect("the read ran")
        };
        assert_eq!((sid_seen, rk_seen.as_str()), (held, "200"), "the read is handed the request it answers");
        plx_plex::plex::reset_servers_for_test();
    }

    /// The server's selected row is the one to start, wherever it sits in the window; a queue whose
    /// selection names no row has no first row.
    #[test]
    fn the_first_row_is_the_servers_selection() {
        let sid = ServerId::from_raw(3);
        let mut q = queue(sid);
        q.selected_item_id = 703;
        assert_eq!(q.first().map(|r| r.rk.as_str()), Some("10"));
        q.selected_item_id = 999;
        assert!(q.first().is_none());
    }
}
