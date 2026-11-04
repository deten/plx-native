//! Explicit queue navigation and Up Next share one stop → resolve → start ritual.
use super::*;
use plx_media::route::{previous_restarts, queue_neighbor, QueueStep};
use plx_plex::plex::QueueRow;

pub(super) fn step(
    ps: &mut plx_media::route::PlaybackSession,
    pa: &mut plx_media::player::adapter::PlayerAdapter,
    direction: QueueStep,
    pages: &mut plx_ui::dispatch::Dispatcher<super::super::bridge::AppHost>,
    bridge: &mut super::super::bridge::Bridge,
) {
    if direction == QueueStep::Previous && previous_restarts(plx_media::player::intended_pos_ns(ps))
    {
        request_seek(0);
        resume_if_paused(pa);
        return;
    }
    let Some(row) = queue_neighbor(ps, direction) else {
        return;
    };
    log(&format!("queue transport: {direction:?} rk={}", row.rk));
    play(ps, pa, row, HUD_LINGER_MS, pages, bridge);
}

pub(super) fn play(
    ps: &mut plx_media::route::PlaybackSession,
    pa: &mut plx_media::player::adapter::PlayerAdapter,
    row: QueueRow,
    hud_ms: u32,
    pages: &mut plx_ui::dispatch::Dispatcher<super::super::bridge::AppHost>,
    bridge: &mut super::super::bridge::Bridge,
) -> bool {
    // The row is owned before the outgoing queue or engine can be retired. Its server identity
    // follows it across the handoff, independent of the server being browsed behind the player.
    let resume = plx_data::metadata::resume_ns(row.resume_ms, row.dur_ms);
    let context = context_of(&row);
    close_player_overlays(pages);
    plx_media::player::stop_bufferfeed(ps, pa);
    let requested = plx_media::route::request_play_queue_item(ps, bridge.metadata_mut(), start_caps(), &row, &context);
    bridge.start_store_jobs();
    if !requested { return false; }
    describe(bridge, row);
    start_playback(
        ps,
        pa,
        resume,
        Origin::Unchanged,
        hud_ms,
        None,
        pages,
        bridge,
    );
    true
}

/// The first row of a queue the server has just made for the item menu's Shuffle Show / Shuffle
/// Season: [`play`]'s request and description, started from the page the menu was on
/// (`Origin::Here`). Nothing is playing to stop: the caller has cleared the engine through
/// `content::clear_engine_for_play`, the door every Play from a page goes through. Every Up Next
/// and Next after it is [`play`] of the next row of the same queue.
pub(super) fn start(
    ps: &mut plx_media::route::PlaybackSession,
    pa: &mut plx_media::player::adapter::PlayerAdapter,
    queue: plx_media::route::QueueContinuation,
    hud_ms: u32,
    pages: &mut plx_ui::dispatch::Dispatcher<super::super::bridge::AppHost>,
    bridge: &mut super::super::bridge::Bridge,
) -> bool {
    let row = queue.row().clone();
    let resume = plx_data::metadata::resume_ns(row.resume_ms, row.dur_ms);
    let context = context_of(&row);
    log(&format!("queue transport: start rk={}", row.rk));
    let requested = plx_media::route::request_play_new_queue(ps, bridge.metadata_mut(), start_caps(), queue, &context);
    bridge.start_store_jobs();
    if !requested { return false; }
    describe(bridge, row);
    start_playback(ps, pa, resume, Origin::Here, hud_ms, None, pages, bridge);
    true
}

/// The HUD's context line for a queue row: an episode's kicker, nothing for anything else.
fn context_of(row: &QueueRow) -> String {
    if row.kind == "episode" {
        plx_ui::fmt::episode_kicker(row.season, row.index, &row.title)
    } else {
        String::new()
    }
}

/// Retire the outgoing item's descriptor and ask for the row's own, which the HUD caption and
/// Info card read once it lands.
fn describe(bridge: &mut super::super::bridge::Bridge, row: QueueRow) {
    bridge.metadata_run(plx_data::stores::metadata::MetadataCmd::RetirePlaying);
    bridge.metadata_run(plx_data::stores::metadata::MetadataCmd::RequestDetail {
            sid: row.sid,
            rk: row.rk,
        });
}
