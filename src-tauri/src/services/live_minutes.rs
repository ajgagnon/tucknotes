//! Live meeting minutes: while a recording is active, each finalized
//! transcript window feeds a low-priority LLM pass that maintains a running,
//! append-only bullet-list document (`meeting_documents.kind = 'minutes'`).
//!
//! Scheduling model: at most one pass in flight per recording. New transcript
//! accumulates in `pending` and is coalesced — when a pass finishes and more
//! text has arrived, exactly one follow-up pass runs with everything new.
//! Passes are skipped (never queued) while the model serves user-initiated
//! work (chat / post-meeting summarization), and a pass aborted by chat
//! preemption returns its chunk to `pending` for the next window.

use std::sync::Mutex;

use tauri::{Emitter, Manager};

use crate::errors::AppError;
use crate::services::database::{self, DatabaseState};
use crate::services::model_manager;
use crate::services::summarization::SummarizationState;

/// Minimum words of new transcript before a pass is worth an inference run.
const MIN_PASS_WORDS: usize = 20;
/// Consecutive failed passes before the session disables itself for the rest
/// of the recording (avoids burning inference on a persistent error).
const MAX_FAILURES: u8 = 3;
/// Size of the editable window, in topic groups (a top-level bullet plus its
/// sub-bullets). Only the single in-progress ("current") topic stays editable;
/// once a newer topic begins, the current one graduates to the frozen list —
/// kept verbatim, in order, never rewritten — so the document is an append-only
/// chronological log and earlier bullets can never move or be eroded.
const RECENT_KEEP: usize = 1;

#[derive(Clone, serde::Serialize)]
struct MinutesUpdatedPayload<'a> {
    meeting_id: &'a str,
    body: &'a str,
}

#[derive(Default)]
pub struct LiveMinutesState {
    inner: Mutex<Option<Session>>,
}

struct Session {
    meeting_id: String,
    /// Finalized transcript lines accumulated since the last consumed chunk.
    pending: String,
    /// Bullets the model may no longer touch (kept verbatim across passes).
    frozen: Vec<String>,
    /// The editable tail of the document; each pass rewrites this window.
    recent: Vec<String>,
    pass_running: bool,
    /// Handle of the in-flight pass so `finalize_session` can await it.
    handle: Option<tokio::task::JoinHandle<()>>,
    /// Recording is stopping: the final pass may wait for the model instead
    /// of skipping, and no further windows are accepted.
    finalizing: bool,
    disabled: bool,
    failures: u8,
}

/// Begin (or keep, on resume) a live-minutes session for this meeting.
/// No-ops when the setting is off; marks the session disabled when no LLM
/// model is downloaded so later windows exit cheaply and silently.
pub fn start_session(app: &tauri::AppHandle, meeting_id: &str) {
    let enabled = match model_manager::load_settings(app) {
        Ok(s) => s.live_minutes_enabled,
        Err(e) => {
            eprintln!("[live-minutes] failed to load settings, disabling: {e}");
            false
        }
    };
    if !enabled {
        return;
    }

    let model_path = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|dir| model_manager::resolve_llm_path(&dir).ok().flatten());

    let state: tauri::State<'_, LiveMinutesState> = app.state();
    {
        let mut guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = guard.as_ref() {
            if existing.meeting_id == meeting_id {
                return; // resume: keep accumulated minutes
            }
        }
        let disabled = model_path.is_none();
        if disabled {
            eprintln!("[live-minutes] no LLM model downloaded; live minutes off for this session");
        }
        *guard = Some(Session {
            meeting_id: meeting_id.to_string(),
            pending: String::new(),
            frozen: Vec::new(),
            recent: Vec::new(),
            pass_running: false,
            handle: None,
            finalizing: false,
            disabled,
            failures: 0,
        });
    }

    // No model preload here: loading a multi-GB model the instant recording
    // starts stalls the whole machine right while the UI is navigating to the
    // new meeting. The first pass (~30s in) loads it lazily in the background
    // instead, where the extra seconds are invisible.
}

/// Feed one finalized transcript window (chronologically interleaved
/// `(source, text)` lines) into the session and try to run a pass.
pub fn on_finalized_window(app: &tauri::AppHandle, meeting_id: &str, lines: &[(String, String)]) {
    let state: tauri::State<'_, LiveMinutesState> = app.state();
    {
        let mut guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(session) = guard.as_mut() else { return };
        if session.meeting_id != meeting_id || session.disabled || session.finalizing {
            return;
        }
        for (source, text) in lines {
            let speaker = if source == "system" { "Speaker" } else { "You" };
            session.pending.push_str(&format!("{speaker}: {text}\n"));
        }
    }
    maybe_spawn_pass(app, false);
}

/// Run one pass if there's enough new transcript and the model is free.
/// `force` (finalize path) drops the size/busy gates: the recording is over,
/// so the last pass may wait its turn on the model mutex.
fn maybe_spawn_pass(app: &tauri::AppHandle, force: bool) {
    let state: tauri::State<'_, LiveMinutesState> = app.state();
    let summ: tauri::State<'_, SummarizationState> = app.state();

    // The guard is held across the spawn so `pass_running` and `handle` are
    // set atomically; the spawned task can't observe (or finish under) a
    // half-initialized session. No await happens while it's held.
    let mut guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
    let Some(session) = guard.as_mut() else { return };
    if session.disabled || session.pass_running {
        return;
    }
    if !force {
        if session.pending.split_whitespace().count() < MIN_PASS_WORDS {
            return;
        }
        // Low priority: skip while the model serves user-initiated work.
        let summarizing = summ
            .active_meeting_id
            .lock()
            .map(|a| a.is_some())
            .unwrap_or(true);
        if summarizing || summ.service.is_busy() {
            return;
        }
    } else if session.pending.trim().is_empty() {
        return;
    }
    session.pass_running = true;
    let meeting_id = session.meeting_id.clone();
    let frozen = session.frozen.join("\n");
    let recent = session.recent.join("\n");
    let chunk = std::mem::take(&mut session.pending);

    let app = app.clone();
    let service = std::sync::Arc::clone(&summ.service);
    let interrupt = std::sync::Arc::clone(&summ.llm_interrupt);

    session.handle = Some(tokio::spawn(async move {
        let model_path = app
            .path()
            .app_data_dir()
            .ok()
            .and_then(|dir| model_manager::resolve_llm_path(&dir).ok().flatten());
        let result = match model_path {
            Some(path) => {
                let chunk_in = chunk.clone();
                tokio::task::spawn_blocking(move || {
                    service.update_live_minutes(&path, &frozen, &recent, &chunk_in, &interrupt)
                })
                .await
                .unwrap_or_else(|e| {
                    Err(AppError::SummarizationFailed(format!("Task panicked: {e}")))
                })
            }
            None => Err(AppError::SummarizationFailed("No LLM model".into())),
        };
        finish_pass(&app, &meeting_id, &chunk, result);
    }));
}

/// Record a pass result, persist + emit on success, and coalesce: if more
/// transcript arrived while the pass ran, immediately run one follow-up.
fn finish_pass(
    app: &tauri::AppHandle,
    meeting_id: &str,
    chunk: &str,
    result: Result<String, AppError>,
) {
    let state: tauri::State<'_, LiveMinutesState> = app.state();
    let mut rerun = false;
    let mut persist: Option<String> = None;
    {
        let mut guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(session) = guard.as_mut() else { return };
        if session.meeting_id != meeting_id {
            // A straggler from a previous recording's session; don't touch
            // the current one.
            return;
        }
        session.pass_running = false;
        session.handle = None;
        match result {
            Ok(body) => {
                session.failures = 0;
                // An empty result (model emitted no bullets) is not worth
                // overwriting the recent window with.
                if !body.is_empty() {
                    let revised = group_topics(&body);
                    session.recent = integrate_revised(&mut session.frozen, revised);
                    let doc = session
                        .frozen
                        .iter()
                        .chain(session.recent.iter())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    persist = Some(doc);
                }
                rerun = !session.pending.trim().is_empty() && !session.finalizing;
            }
            Err(AppError::Interrupted) => {
                // Chat preempted us: give the chunk back; the next finalized
                // window (or finalize) retries with everything pending.
                session.pending = format!("{chunk}{}", session.pending);
            }
            Err(e) => {
                session.failures += 1;
                eprintln!(
                    "[live-minutes] pass failed ({}/{MAX_FAILURES}): {e}",
                    session.failures
                );
                if session.failures >= MAX_FAILURES {
                    session.disabled = true;
                    eprintln!("[live-minutes] disabled for the rest of this recording");
                }
            }
        }
    }
    if let Some(body) = persist {
        persist_and_emit(app, meeting_id, &body);
    }
    if rerun {
        maybe_spawn_pass(app, false);
    }
}

/// Split a normalized bullet document into topic groups: each group is one
/// top-level bullet plus its indented sub-bullet lines, joined with `\n`.
/// A leading indented line (no parent yet) defensively becomes its own group.
fn group_topics(body: &str) -> Vec<String> {
    let mut groups: Vec<String> = Vec::new();
    for line in body.lines() {
        let indented = line.starts_with(char::is_whitespace);
        match groups.last_mut() {
            Some(last) if indented => {
                last.push('\n');
                last.push_str(line);
            }
            _ => groups.push(line.to_string()),
        }
    }
    groups
}

/// Fold a pass's revised tail back into the document. The model returns the
/// (refined) current topic followed by any new topics the segment started;
/// everything beyond the last [`RECENT_KEEP`] groups graduates into `frozen`,
/// in order, where later passes can no longer touch it. `frozen` only ever
/// grows by appending, so the document stays an append-only chronological log —
/// a topic the meeting returns to becomes a new bullet at the end rather than a
/// rewrite of an earlier one. Groups with an empty head are dropped.
/// Returns the new recent window.
fn integrate_revised(frozen: &mut Vec<String>, revised: Vec<String>) -> Vec<String> {
    let mut recent: Vec<String> = revised
        .into_iter()
        .filter(|g| g.lines().next().map(|h| !h.trim().is_empty()).unwrap_or(false))
        .collect();
    while recent.len() > RECENT_KEEP {
        frozen.push(recent.remove(0));
    }
    recent
}

fn persist_and_emit(app: &tauri::AppHandle, meeting_id: &str, body: &str) {
    let db: tauri::State<'_, DatabaseState> = app.state();
    match db.conn.lock() {
        Ok(conn) => {
            // Meeting deleted mid-recording is the only expected failure;
            // nothing actionable either way.
            if let Err(e) =
                database::upsert_minutes_body(&conn, meeting_id, body, database::now_unix_ms())
            {
                eprintln!("[live-minutes] failed to persist: {e}");
                return;
            }
        }
        Err(e) => {
            eprintln!("[live-minutes] db lock poisoned: {e}");
            return;
        }
    }
    let _ = app.emit("minutes:updated", MinutesUpdatedPayload { meeting_id, body });
}

/// Stop-flow hook: run one final pass over whatever transcript remains
/// (waiting for the model if needed), await the in-flight pass, and clear the
/// session. The caller bounds this with a timeout; passes persist as they
/// complete, so timing out only risks the final tail.
pub async fn finalize_session(app: &tauri::AppHandle, meeting_id: &str) {
    let state: tauri::State<'_, LiveMinutesState> = app.state();

    let handle = {
        let mut guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(session) = guard.as_mut() else { return };
        if session.meeting_id != meeting_id {
            return;
        }
        session.finalizing = true;
        session.handle.take()
    };
    if let Some(handle) = handle {
        let _ = handle.await;
    }

    // One last coalesced pass over the tail (final_flush windows land in
    // `pending` before the stop flow calls us).
    let disabled = {
        let guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|s| s.disabled).unwrap_or(true)
    };
    if !disabled {
        // Interrupt-clearing on lock acquisition means a stale chat interrupt
        // can't wedge this pass; if chat genuinely preempts it, we drop the
        // tail rather than fight a user-initiated request.
        maybe_spawn_pass(app, true);
        let handle = {
            let mut guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
            guard.as_mut().and_then(|s| s.handle.take())
        };
        if let Some(handle) = handle {
            let _ = handle.await;
        }
    }

    let mut guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().map(|s| s.meeting_id == meeting_id).unwrap_or(false) {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{group_topics, integrate_revised, RECENT_KEEP};

    fn bullets(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn group_topics_keeps_flat_input_one_per_line() {
        assert_eq!(group_topics("- a\n- b"), bullets(&["- a", "- b"]));
    }

    #[test]
    fn group_topics_attaches_sub_bullets_to_parent() {
        assert_eq!(
            group_topics("- a\n  - a1\n  - a2\n- b"),
            bullets(&["- a\n  - a1\n  - a2", "- b"])
        );
    }

    #[test]
    fn group_topics_leading_orphan_becomes_own_group() {
        assert_eq!(
            group_topics("  - orphan\n- a"),
            bullets(&["  - orphan", "- a"])
        );
    }

    #[test]
    fn integrate_keeps_only_current_topic_editable() {
        // Only the last group stays editable; earlier ones graduate in order.
        let mut frozen = bullets(&["- a"]);
        let recent = integrate_revised(&mut frozen, bullets(&["- b", "- c"]));
        assert_eq!(frozen, bullets(&["- a", "- b"]));
        assert_eq!(recent, bullets(&["- c"]));
    }

    #[test]
    fn integrate_appends_in_order_across_passes() {
        // Refined current topic C' plus a new topic D: C' graduates after the
        // existing frozen bullets, D becomes the new current topic.
        let mut frozen = bullets(&["- a", "- b"]);
        let recent = integrate_revised(&mut frozen, bullets(&["- c prime", "- d"]));
        assert_eq!(frozen, bullets(&["- a", "- b", "- c prime"]));
        assert_eq!(recent, bullets(&["- d"]));
    }

    #[test]
    fn integrate_graduates_overflow_in_order() {
        let mut frozen = bullets(&["- a"]);
        let revised: Vec<String> = (0..RECENT_KEEP + 2)
            .map(|i| format!("- bullet {i}"))
            .collect();
        let recent = integrate_revised(&mut frozen, revised);
        // Everything but the last bullet graduates, in chronological order.
        assert_eq!(frozen, bullets(&["- a", "- bullet 0", "- bullet 1"]));
        assert_eq!(recent.len(), RECENT_KEEP);
        assert_eq!(recent[0], "- bullet 2");
    }

    #[test]
    fn integrate_keeps_revisited_topic_as_new_bullet() {
        // Append-only: a group whose head matches a frozen one is a revisit,
        // kept and appended in order rather than dropped.
        let mut frozen = bullets(&["- roadmap\n  - q3 launch"]);
        let recent = integrate_revised(
            &mut frozen,
            bullets(&["- roadmap\n  - launch moved to q4", "- pricing"]),
        );
        assert_eq!(
            frozen,
            bullets(&["- roadmap\n  - q3 launch", "- roadmap\n  - launch moved to q4"])
        );
        assert_eq!(recent, bullets(&["- pricing"]));
    }

    #[test]
    fn integrate_graduates_topic_with_sub_bullets_as_one_unit() {
        let mut frozen = Vec::new();
        let mut revised: Vec<String> = vec!["- topic 0\n  - detail".to_string()];
        revised.extend((1..=RECENT_KEEP).map(|i| format!("- topic {i}")));
        let recent = integrate_revised(&mut frozen, revised);
        assert_eq!(frozen, bullets(&["- topic 0\n  - detail"]));
        assert_eq!(recent.len(), RECENT_KEEP);
    }
}
