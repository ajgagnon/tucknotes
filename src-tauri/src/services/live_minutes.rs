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
use crate::services::dedup;
use crate::services::model_manager;
use crate::services::summarization::{LiveMinutesPassInput, SummarizationState};

/// Minimum words of new transcript before a pass is worth an inference run.
/// Set high enough that each pass sees a whole thought — usually two finalized
/// ~30s windows — to summarize into one clean bullet rather than firing on a
/// fragment. Raising this also offsets the compute the gist updates add.
const MIN_PASS_WORDS: usize = 80;
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
/// Words of already-processed transcript carried into the next pass as
/// context, so a thought that straddles a chunk boundary reads whole instead
/// of being half-captured. ~1–2 sentences.
const CARRY_OVER_WORDS: usize = 60;
/// Words of consumed transcript accumulated before the rolling gist is worth
/// refreshing — roughly one small extra inference per 2–3 minutes of speech.
const GIST_UPDATE_MIN_WORDS: usize = 250;
/// Backstop on transcript awaiting a gist update when refreshes keep getting
/// skipped or interrupted; the tail (most recent speech) is kept.
const GIST_PENDING_MAX_CHARS: usize = 6_000;
/// Backstop on a runaway gist (~350 prompt tokens); the head is kept since the
/// gist leads with its labeled summary lines.
const GIST_MAX_CHARS: usize = 1_500;
/// Consecutive failed gist updates before gist refreshes stop for the rest of
/// the recording. Counted separately from [`MAX_FAILURES`]: a broken gist must
/// never take the minutes down with it — the last good gist keeps being used.
const GIST_MAX_FAILURES: u8 = 3;
/// Fraction of a new bullet's content tokens that must already appear in one
/// recorded bullet for it to count as a restatement. Containment (not Jaccard)
/// so a genuine update that adds new information survives.
const DEDUP_CONTAINMENT: f32 = 0.8;
/// Minimum content tokens before fuzzy matching applies — very short bullets
/// carry too little signal to call near-duplicates.
const DEDUP_MIN_TOKENS: usize = 2;
/// Common words ignored by fuzzy duplicate detection.
const DEDUP_STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "was", "are", "will", "has", "have", "not",
    "from", "about", "into", "them", "they",
];

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
    /// Tail of the transcript the last pass consumed, shown to the next pass
    /// as PRIOR TRANSCRIPT so boundary-straddling thoughts read whole.
    carry: String,
    /// Rolling model-facing summary of the meeting (MEETING CONTEXT SO FAR in
    /// the minutes prompt). Session-only: never persisted or displayed.
    gist: String,
    /// Transcript already consumed by minutes passes but not yet folded into
    /// the gist. Taken as one chunk when it reaches [`GIST_UPDATE_MIN_WORDS`].
    gist_pending: String,
    gist_failures: u8,
    /// Gist refreshes stopped for this recording (repeated failures); minutes
    /// keep running with the last good gist as context.
    gist_disabled: bool,
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

    let engine = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|dir| model_manager::resolve_llm_engine(&dir).ok().flatten());

    let state: tauri::State<'_, LiveMinutesState> = app.state();
    {
        let mut guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = guard.as_ref() {
            if existing.meeting_id == meeting_id {
                return; // resume: keep accumulated minutes
            }
        }
        let disabled = engine.is_none();
        if disabled {
            eprintln!(
                "[live-minutes] no LLM engine configured; live minutes off for this session"
            );
        }
        *guard = Some(Session {
            meeting_id: meeting_id.to_string(),
            pending: String::new(),
            bullets: Vec::new(),
            carry: String::new(),
            gist: String::new(),
            gist_pending: String::new(),
            gist_failures: 0,
            gist_disabled: false,
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
    let gist = session.gist.clone();
    let carry = session.carry.clone();
    // Take the accumulated gist backlog when it's big enough — but never on
    // the finalize path: a fresh gist is useless once the meeting is over and
    // would eat into the stop flow's timeout budget.
    let gist_chunk = if !force
        && !session.gist_disabled
        && session.gist_pending.split_whitespace().count() >= GIST_UPDATE_MIN_WORDS
    {
        Some(std::mem::take(&mut session.gist_pending))
    } else {
        None
    };

    let app = app.clone();
    let service = std::sync::Arc::clone(&summ.service);
    let interrupt = std::sync::Arc::clone(&summ.llm_interrupt);

    session.handle = Some(tokio::spawn(async move {
        let engine = app
            .path()
            .app_data_dir()
            .ok()
            .and_then(|dir| model_manager::resolve_llm_engine(&dir).ok().flatten());
        let (result, gist_outcome) = match engine {
            Some(engine) => {
                let chunk_in = chunk.clone();
                let gist_in = gist.clone();
                let carry_in = carry;
                let recorded_in = recorded_tail;
                let svc = std::sync::Arc::clone(&service);
                let intr = std::sync::Arc::clone(&interrupt);
                let engine_in = engine.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let input = LiveMinutesPassInput {
                        gist: &gist_in,
                        recorded_tail: &recorded_in,
                        carry: &carry_in,
                        new_transcript: &chunk_in,
                    };
                    svc.update_live_minutes(&engine_in, &input, &intr)
                })
                .await
                .unwrap_or_else(|e| {
                    Err(AppError::SummarizationFailed(format!("Task panicked: {e}")))
                });

                // Fold the backlog into the gist as a second inference in the
                // same pass, so the single pass_running gate and handle keep
                // covering everything. Skipped (chunk returned to the backlog,
                // encoded as `Interrupted` for the bookkeeping in
                // `finish_pass`) when the minutes pass didn't succeed, chat
                // grabbed the model in the gap, or the recording started
                // stopping — `finalize_session` awaits this handle, so a
                // throwaway gist refresh must not eat its timeout budget.
                let finalizing = {
                    let state: tauri::State<'_, LiveMinutesState> = app.state();
                    let guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
                    guard
                        .as_ref()
                        .map(|s| s.finalizing || s.meeting_id != meeting_id)
                        .unwrap_or(true)
                };
                let gist_outcome = match gist_chunk {
                    Some(gist_chunk) if result.is_ok() && !finalizing && !service.is_busy() => {
                        let chunk_in = gist_chunk.clone();
                        let gist_result = tokio::task::spawn_blocking(move || {
                            service.update_meeting_gist(&engine, &gist, &chunk_in, &interrupt)
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(AppError::SummarizationFailed(format!("Task panicked: {e}")))
                        });
                        Some((gist_chunk, gist_result))
                    }
                    Some(gist_chunk) => Some((gist_chunk, Err(AppError::Interrupted))),
                    None => None,
                };
                (result, gist_outcome)
            }
            None => (
                Err(AppError::SummarizationFailed("No LLM model".into())),
                gist_chunk.map(|c| (c, Err(AppError::Interrupted))),
            ),
        };
        finish_pass(&app, &meeting_id, &chunk, result, gist_outcome);
    }));
}

/// Record a pass result, persist + emit on success, and coalesce: if more
/// transcript arrived while the pass ran, immediately run one follow-up.
fn finish_pass(
    app: &tauri::AppHandle,
    meeting_id: &str,
    chunk: &str,
    result: Result<String, AppError>,
    gist_outcome: Option<(String, Result<String, AppError>)>,
) {
    let state: tauri::State<'_, LiveMinutesState> = app.state();
    let (persist, rerun) = {
        let mut guard = state.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(session) = guard.as_mut() else { return };
        if session.meeting_id != meeting_id {
            // A straggler from a previous recording's session; don't touch
            // the current one.
            return;
        }
        session.pass_running = false;
        session.handle = None;
        apply_pass_result(session, chunk, result, gist_outcome)
    };
    if let Some(body) = persist {
        persist_and_emit(app, meeting_id, &body);
    }
    if rerun {
        maybe_spawn_pass(app, false);
    }
}

/// Pure bookkeeping for one completed pass, applied under the session lock.
/// Returns the full document body to persist when new bullets were appended,
/// and whether a coalescing follow-up pass should run.
fn apply_pass_result(
    session: &mut Session,
    chunk: &str,
    result: Result<String, AppError>,
    gist_outcome: Option<(String, Result<String, AppError>)>,
) -> (Option<String>, bool) {
    let mut rerun = false;
    let mut persist: Option<String> = None;
    match result {
        Ok(body) => {
            session.failures = 0;
            consume_chunk(session, chunk);
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
            // window (or finalize) retries with everything pending. `carry`
            // stays untouched — the chunk will reappear as NEW TRANSCRIPT,
            // and carrying its tail too would duplicate text in the prompt.
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
            // The chunk is dropped for minutes, but the transcript itself was
            // fine — the gist should still eventually cover it.
            consume_chunk(session, chunk);
        }
    }
    apply_gist_result(session, gist_outcome);
    (persist, rerun)
}

/// The pass consumed `chunk`: the next pass sees its tail as PRIOR TRANSCRIPT,
/// and it joins the backlog awaiting a gist refresh.
fn consume_chunk(session: &mut Session, chunk: &str) {
    session.carry = tail_words(chunk, CARRY_OVER_WORDS);
    session.gist_pending.push_str(chunk);
    cap_keep_tail(&mut session.gist_pending, GIST_PENDING_MAX_CHARS);
}

/// Bookkeeping for the optional gist refresh that ran (or was skipped) inside
/// a pass. Failures are counted separately from minutes failures — a broken
/// gist freezes gist refreshes but never disables the minutes themselves.
fn apply_gist_result(
    session: &mut Session,
    gist_outcome: Option<(String, Result<String, AppError>)>,
) {
    let Some((gist_chunk, gist_result)) = gist_outcome else { return };
    match gist_result {
        Ok(new_gist) => {
            session.gist = new_gist;
            cap_keep_head(&mut session.gist, GIST_MAX_CHARS);
            session.gist_failures = 0;
            eprintln!(
                "[live-minutes] gist updated ({} chars)",
                session.gist.len()
            );
        }
        // Interrupted — or the refresh never ran (minutes pass failed / chat
        // took the model / finalizing / no model): the backlog goes back in
        // front so no words are lost and ordering is preserved.
        Err(AppError::Interrupted) => {
            session.gist_pending = format!("{gist_chunk}{}", session.gist_pending);
            cap_keep_tail(&mut session.gist_pending, GIST_PENDING_MAX_CHARS);
        }
        Err(e) => {
            // Unlike the interrupt path, the backlog chunk is dropped, not
            // requeued: a chunk the model chokes on would otherwise poison
            // every following refresh until the failure cap disables gists.
            session.gist_failures += 1;
            eprintln!(
                "[live-minutes] gist update failed ({}/{GIST_MAX_FAILURES}): {e}",
                session.gist_failures
            );
            if session.gist_failures >= GIST_MAX_FAILURES {
                session.gist_disabled = true;
                eprintln!("[live-minutes] gist frozen for the rest of this recording");
            }
        }
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

/// The last `n` whitespace-separated words of `s`, with the original spacing
/// and line breaks between them preserved (so speaker labels survive).
fn tail_words(s: &str, n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    let base = s.as_ptr() as usize;
    let starts: Vec<usize> = s
        .split_whitespace()
        .map(|w| w.as_ptr() as usize - base)
        .collect();
    if starts.is_empty() {
        return String::new();
    }
    let from = starts[starts.len().saturating_sub(n)];
    s[from..].trim_end().to_string()
}

/// Truncate `s` to at most `max` bytes keeping the tail (most recent text),
/// cutting on a char boundary.
fn cap_keep_tail(s: &mut String, max: usize) {
    if s.len() <= max {
        return;
    }
    let mut from = s.len() - max;
    while !s.is_char_boundary(from) {
        from += 1;
    }
    *s = s[from..].to_string();
}

/// Truncate `s` to at most `max` bytes keeping the head, cutting on a char
/// boundary.
fn cap_keep_head(s: &mut String, max: usize) {
    if s.len() <= max {
        return;
    }
    let mut to = max;
    while !s.is_char_boundary(to) {
        to -= 1;
    }
    s.truncate(to);
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

/// Lowercased alphanumeric content tokens of a group's head line — minus
/// stopwords and tokens under 3 chars — for fuzzy duplicate detection.
/// Tokenization is [`dedup::normalize`], shared with transcript echo dedup.
fn head_tokens(group: &str) -> std::collections::HashSet<String> {
    dedup::normalize(group.lines().next().unwrap_or(""))
        .into_iter()
        .filter(|t| t.len() >= 3 && !DEDUP_STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// True when `new` restates `old`: at least [`DEDUP_MIN_TOKENS`] content
/// tokens, of which at least [`DEDUP_CONTAINMENT`] already appear in `old`.
/// Containment (|new ∩ old| / |new|) rather than Jaccard, so a genuine update
/// that adds new information to an old point survives.
fn is_near_duplicate(
    new: &std::collections::HashSet<String>,
    old: &std::collections::HashSet<String>,
) -> bool {
    if new.len() < DEDUP_MIN_TOKENS {
        return false;
    }
    let overlap = new.iter().filter(|t| old.contains(*t)).count();
    overlap as f32 >= DEDUP_CONTAINMENT * new.len() as f32
}

/// Drop new groups that merely restate an already recorded bullet (a small
/// model occasionally repeats despite the prompt) or an earlier group in the
/// same batch, plus any group with an empty head. Compares against the WHOLE
/// document — not just the trailing window the model was shown — first by
/// exact normalized head, then by token containment, so paraphrased repeats
/// of long-scrolled-away bullets are caught too.
fn dedupe_against_recorded(recorded: &[String], new_groups: &mut Vec<String>) {
    let mut seen_heads: std::collections::HashSet<String> =
        recorded.iter().map(|g| norm_head(g)).collect();
    let mut seen_tokens: Vec<std::collections::HashSet<String>> =
        recorded.iter().map(|g| head_tokens(g)).collect();
    new_groups.retain(|g| {
        let key = norm_head(g);
        if key.is_empty() || !seen_heads.insert(key) {
            return false;
        }
        let tokens = head_tokens(g);
        if seen_tokens.iter().any(|old| is_near_duplicate(&tokens, old)) {
            return false;
        }
        seen_tokens.push(tokens);
        true
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
        apply_pass_result, cap_keep_head, cap_keep_tail, dedupe_against_recorded, group_topics,
        head_tokens, is_near_duplicate, norm_head, tail_context, tail_words, Session,
        CONTEXT_TAIL_BULLETS, GIST_MAX_FAILURES, MAX_FAILURES,
    };
    use crate::errors::AppError;

    fn bullets(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn session() -> Session {
        Session {
            meeting_id: "m1".to_string(),
            pending: String::new(),
            bullets: Vec::new(),
            carry: String::new(),
            gist: String::new(),
            gist_pending: String::new(),
            gist_failures: 0,
            gist_disabled: false,
            pass_running: false,
            handle: None,
            finalizing: false,
            disabled: false,
            failures: 0,
        }
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

    #[test]
    fn dedupe_drops_paraphrase_of_old_bullet_beyond_context_tail() {
        // The paraphrased bullet is far older than the tail the model saw.
        let mut recorded = bullets(&["- Q3 budget cut 15% to $85k"]);
        recorded.extend((0..CONTEXT_TAIL_BULLETS).map(|i| format!("- filler point number {i}")));
        let mut new_groups = bullets(&["- Budget for Q3 cut by 15%, about $85k"]);
        dedupe_against_recorded(&recorded, &mut new_groups);
        assert!(new_groups.is_empty());
    }

    #[test]
    fn dedupe_keeps_update_that_adds_new_information() {
        let recorded = bullets(&["- Demo Friday"]);
        let mut new_groups = bullets(&["- Demo moved to Monday, room 4B"]);
        dedupe_against_recorded(&recorded, &mut new_groups);
        assert_eq!(new_groups, bullets(&["- Demo moved to Monday, room 4B"]));
    }

    #[test]
    fn dedupe_does_not_fuzzy_match_short_bullets() {
        // One content token is too little signal to call a near-duplicate.
        let recorded = bullets(&["- Q3 approved"]);
        let mut new_groups = bullets(&["- Q4 approved"]);
        dedupe_against_recorded(&recorded, &mut new_groups);
        assert_eq!(new_groups, bullets(&["- Q4 approved"]));
    }

    #[test]
    fn head_tokens_drops_stopwords_short_tokens_and_lowercases() {
        let tokens = head_tokens("- The Demo WAS moved to Friday at 3pm");
        let expect: std::collections::HashSet<String> = ["demo", "moved", "friday", "3pm"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(tokens, expect);
    }

    #[test]
    fn near_duplicate_requires_high_containment() {
        let old = head_tokens("- Q3 budget cut 15% to $85k");
        assert!(is_near_duplicate(&head_tokens("- Budget for Q3 cut, about $85k"), &old));
        assert!(!is_near_duplicate(&head_tokens("- Q3 budget review moved to Monday"), &old));
    }

    #[test]
    fn tail_words_keeps_last_n_words_with_original_text() {
        assert_eq!(tail_words("You: hello there\nSpeaker: demo friday", 3), "Speaker: demo friday");
        assert_eq!(tail_words("You: hello there\nSpeaker: demo friday", 4), "there\nSpeaker: demo friday");
        assert_eq!(tail_words("one two", 5), "one two");
        assert_eq!(tail_words("", 5), "");
        assert_eq!(tail_words("one two", 0), "");
    }

    #[test]
    fn cap_keep_tail_and_head_respect_char_boundaries() {
        let mut s = "abcdé".to_string();
        cap_keep_tail(&mut s, 2);
        assert_eq!(s, "é");
        let mut s = "éabcd".to_string();
        cap_keep_head(&mut s, 1);
        assert_eq!(s, "");
        let mut s = "short".to_string();
        cap_keep_tail(&mut s, 10);
        assert_eq!(s, "short");
    }

    #[test]
    fn pass_ok_appends_bullets_sets_carry_and_gist_pending() {
        let mut s = session();
        let (persist, rerun) = apply_pass_result(
            &mut s,
            "You: chunk text here\n",
            Ok("- Demo Friday".to_string()),
            None,
        );
        assert_eq!(persist.as_deref(), Some("- Demo Friday"));
        assert!(!rerun);
        assert_eq!(s.carry, "You: chunk text here");
        assert_eq!(s.gist_pending, "You: chunk text here\n");
        assert_eq!(s.failures, 0);
    }

    #[test]
    fn pass_interrupted_returns_chunk_to_pending_and_gist_chunk_to_backlog() {
        let mut s = session();
        s.pending = "newer text\n".to_string();
        s.carry = "old carry".to_string();
        let (persist, rerun) = apply_pass_result(
            &mut s,
            "chunk text\n",
            Err(AppError::Interrupted),
            Some(("gist backlog\n".to_string(), Err(AppError::Interrupted))),
        );
        assert!(persist.is_none());
        assert!(!rerun);
        assert_eq!(s.pending, "chunk text\nnewer text\n");
        // Carry untouched: the chunk will reappear as NEW TRANSCRIPT.
        assert_eq!(s.carry, "old carry");
        assert_eq!(s.gist_pending, "gist backlog\n");
    }

    #[test]
    fn pass_failures_disable_minutes_but_gist_failures_do_not() {
        let mut s = session();
        for _ in 0..MAX_FAILURES {
            apply_pass_result(
                &mut s,
                "chunk\n",
                Err(AppError::SummarizationFailed("boom".into())),
                None,
            );
        }
        assert!(s.disabled);

        let mut s = session();
        for _ in 0..GIST_MAX_FAILURES {
            apply_pass_result(
                &mut s,
                "chunk\n",
                Ok(String::new()),
                Some((
                    "backlog\n".to_string(),
                    Err(AppError::SummarizationFailed("boom".into())),
                )),
            );
        }
        assert!(s.gist_disabled);
        assert!(!s.disabled);
        assert_eq!(s.failures, 0);
    }

    #[test]
    fn gist_ok_replaces_gist_and_resets_failures() {
        let mut s = session();
        s.gist = "old gist".to_string();
        s.gist_failures = 2;
        apply_pass_result(
            &mut s,
            "chunk\n",
            Ok(String::new()),
            Some((
                "backlog\n".to_string(),
                Ok("Participants: You\nTopics: demo".to_string()),
            )),
        );
        assert_eq!(s.gist, "Participants: You\nTopics: demo");
        assert_eq!(s.gist_failures, 0);
    }
}
