//! Live executor for Settings preference effects. The bridge's controlled-IO guard must admit
//! the effect before this entrypoint: profile capture, thread admission and persistence all live
//! here, never in a screen constructor or step.
use plx_plex::plex::account::{PreferenceError, PreferenceRequest};
use plx_screens::registry::{AccountPreferenceReply, PreferenceCmd};

/// **Settings > Layout's persistence seam**, run on the storage worker: the live layout
/// (`plx_ui::layout`) changes only once the write is durable, so a failed save claims nothing.
///
/// It applies at once. What the layout changes today is the chrome of the bar-wearing pages (the
/// top bar or the rail, the strip a focus can reach, LEFT off a page's edge), which is read per
/// frame and per key, and no page's geometry depends on it; the picker is on the Settings surface,
/// so the page under it is redrawn in the new layout when the surface closes.
pub(super) fn set_layout_mode(mode: plx_plex::plex::session::LayoutMode) -> bool {
    let saved = plx_plex::plex::session::update_with_outcome(|s| Some(s.with_layout_mode(mode)))
        .is_some_and(|write| matches!(write.classify(),
            plx_plex::plex::session::async_persistence::CompletionOutcome::Durable(_)));
    if saved {
        plx_ui::layout::publish(mode.is_shield());
        plx_machine::idle::invalidate();
    }
    saved
}

/// **Settings > Detailed logging's persistence seam**, run on the storage worker like Layout's: the
/// kept history (`plx_base::eventlog::history`) moves and widens only once the write is durable, so
/// a failed save claims nothing, and the switch is logged with the file the history now goes to.
pub(super) fn set_detailed_logging(mode: plx_plex::plex::session::DetailedLogging) -> bool {
    let saved = plx_plex::plex::session::update_with_outcome(|s| Some(s.with_detailed_logging(mode)))
        .is_some_and(|write| matches!(write.classify(),
            plx_plex::plex::session::async_persistence::CompletionOutcome::Durable(_)));
    if saved {
        plx_base::eventlog::history::publish_detailed(mode.is_on());
        plx_machine::idle::invalidate();
    }
    saved
}

pub(super) fn execute(command: PreferenceCmd) {
    match command {
        PreferenceCmd::Load { reply } => {
            let Some(request) = PreferenceRequest::capture() else {
                let _ = reply.send(AccountPreferenceReply {
                    request: None, outcome: Err(PreferenceError::Unavailable),
                });
                plx_machine::idle::invalidate();
                return;
            };
            // A refused spawn drops the reply sender. The screen reports its disconnected
            // receipt as retryable without claiming that an account call ran.
            plx_base::task::spawn_small("account preferences", move || {
                let outcome = request.load();
                let _ = reply.send(AccountPreferenceReply { request: Some(request), outcome });
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::Save { request, base, update, reply } => {
            plx_base::task::spawn_small("account preferences", move || {
                let outcome = request.save(&base, update);
                let _ = reply.send(AccountPreferenceReply { request: Some(request), outcome });
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::Quality { quality, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_default_quality(quality));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::Language { language, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_plex::plex::session::set_language(language));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::DirectPlay { mode, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_direct_play_mode(mode));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::NextEpisode { mode, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_next_episode_mode(mode));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::DeckPress { mode, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_deck_press(mode));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::LibraryOpens { mode, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_data::browse::set_library_opens(mode));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::LayoutMode { mode, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(set_layout_mode(mode));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::DetailedLogging { mode, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(set_detailed_logging(mode));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::SkipInterval { interval, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_skip_interval(interval));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::SkipBack { interval, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_skip_back(interval));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::ResumeRewind { rewind, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_resume_rewind(rewind));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::PassoutProtection { protection, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_passout_protection(protection));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::SkipIntro { mode, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_skip_intro(mode));
                plx_machine::idle::invalidate();
            });
        }
        PreferenceCmd::SkipCredits { mode, reply } => {
            let _ = plx_base::storage_worker::submit_retained(move || {
                let _ = reply.send(plx_media::route::set_skip_credits(mode));
                plx_machine::idle::invalidate();
            });
        }
        // The optimistic picks run HERE, on the main thread: the live value is published before
        // anything is persisted and the persistence rides the storage worker on its own
        // (`route::select_subtitle_size`), so there is no outer worker submission to republish.
        PreferenceCmd::SubtitleSize { size, reply } => plx_media::route::select_subtitle_size(size, Some(reply)),
        PreferenceCmd::SubtitlePosition { position, reply } => {
            plx_media::route::select_subtitle_position(position, Some(reply))
        }
    }
}
