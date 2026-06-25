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
/// Set high enough that each pass sees a whole thought to summarize into one
/// clean bullet rather than firing on every couple of sentences.
const MIN_PASS_WORDS: usize = 50;
/// Consecutive failed passes before the session disables itself for the rest
/// of the recording (avoids burning inference on a persistent error).
const MAX_FAILURES: u8 = 3;
/// How many trailing recorded bullets to show the model as context so it can
/// avoid repeating points already logged — without re-feeding the whole
/// (unbounded) document to a small model on every pass.
const CONTEXT_TAIL_BULLETS: usize = 10;
/// Hard backstop on new bullets appended by a single pass. Real output is 0–2;
/// this only guards a model that ignores the "stay sparse" instruction.
const NEW_BULLETS_PER_PASS_MAX: usize = 2;

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
    /// Every bullet recorded so far, in order. Append-only: a pass may add new
    /// groups at the end but never edits or reorders existing ones.
    bullets: Vec<String>,
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
            bullets: Vec::new(),
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
    let recorded_tail = tail_context(&session.bullets);
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
                    service.update_live_minutes(&path, &recorded_tail, &chunk_in, &interrupt)
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
                // Empty output is the model staying silent — nothing noteworthy
                // in this chunk — so the document is left unchanged. Otherwise
                // append the new bullets; existing ones are never touched.
                if !body.is_empty() {
                    let mut new_groups = group_topics(&body);
                    dedupe_against_recorded(&session.bullets, &mut new_groups);
                    new_groups.truncate(NEW_BULLETS_PER_PASS_MAX);
                    if !new_groups.is_empty() {
                        session.bullets.extend(new_groups);
                        persist = Some(session.bullets.join("\n"));
                    }
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

/// The trailing recorded bullets shown to the model as de-dup context — the
/// last [`CONTEXT_TAIL_BULLETS`] groups, joined. Bounding this keeps the prompt
/// small for a tiny model even as the document grows long.
fn tail_context(bullets: &[String]) -> String {
    let start = bullets.len().saturating_sub(CONTEXT_TAIL_BULLETS);
    bullets[start..].join("\n")
}

/// Normalize a group's head line for cheap duplicate detection: drop the
/// leading marker and indentation and lowercase it, so a trivial re-statement
/// of an already-recorded point collapses to the same key.
fn norm_head(group: &str) -> String {
    group
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches(|c| c == '-' || c == '*' || c == ' ')
        .trim()
        .to_lowercase()
}

/// Drop new groups that merely restate a recently recorded bullet (a small
/// model occasionally repeats despite the prompt) or an earlier group in the
/// same batch, plus any group with an empty head. Compares against the same
/// trailing window the model was shown.
fn dedupe_against_recorded(recorded: &[String], new_groups: &mut Vec<String>) {
    let start = recorded.len().saturating_sub(CONTEXT_TAIL_BULLETS);
    let mut seen: std::collections::HashSet<String> =
        recorded[start..].iter().map(|g| norm_head(g)).collect();
    new_groups.retain(|g| {
        let key = norm_head(g);
        !key.is_empty() && seen.insert(key)
    });
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
    use super::{
        dedupe_against_recorded, group_topics, norm_head, tail_context, CONTEXT_TAIL_BULLETS,
    };

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
    fn tail_context_returns_trailing_bullets() {
        let all: Vec<String> = (0..CONTEXT_TAIL_BULLETS + 3)
            .map(|i| format!("- bullet {i}"))
            .collect();
        let tail = tail_context(&all);
        // Only the last CONTEXT_TAIL_BULLETS are shown to the model, in order.
        assert_eq!(tail.lines().count(), CONTEXT_TAIL_BULLETS);
        assert_eq!(tail.lines().next(), Some("- bullet 3"));
        let last = format!("- bullet {}", CONTEXT_TAIL_BULLETS + 2);
        assert_eq!(tail.lines().last(), Some(last.as_str()));
    }

    #[test]
    fn tail_context_handles_short_or_empty_document() {
        assert_eq!(tail_context(&bullets(&["- a", "- b"])), "- a\n- b");
        assert_eq!(tail_context(&[]), "");
    }

    #[test]
    fn norm_head_strips_marker_and_lowercases() {
        assert_eq!(norm_head("- Demo Friday"), "demo friday");
        // Only the head line matters; sub-bullets are ignored.
        assert_eq!(norm_head("  - Sub detail\n  - more"), "sub detail");
    }

    #[test]
    fn dedupe_drops_repeat_of_recorded_bullet() {
        let recorded = bullets(&["- demo friday", "- pricing agreed"]);
        let mut new_groups = bullets(&["- Demo Friday", "- new point"]);
        dedupe_against_recorded(&recorded, &mut new_groups);
        // The restated bullet is dropped; the genuinely new one is kept.
        assert_eq!(new_groups, bullets(&["- new point"]));
    }

    #[test]
    fn dedupe_drops_within_batch_dupes_and_empty_heads() {
        let mut new_groups = bullets(&["- a", "- A", "-   ", "- b"]);
        dedupe_against_recorded(&[], &mut new_groups);
        assert_eq!(new_groups, bullets(&["- a", "- b"]));
    }
}
