//! Transcript-level deduplication.
//!
//! The mic and system audio are transcribed as two separate streams, so the
//! same speech can land in the transcript twice:
//!
//!   * **Cross-stream echo** — on built-in speakers the mic re-captures the
//!     other party (whose voice also arrives cleanly on the system stream), so
//!     the same words appear on both `system` and `microphone` at overlapping
//!     timestamps. We keep the system copy and drop the mic echo: system audio
//!     never contains the user's own voice, so a content + time match must be
//!     the far party bleeding into the mic.
//!
//!   * **Window-boundary repeats** — the transcription loop feeds the previous
//!     window's text to Whisper as an initial prompt, and Whisper sometimes
//!     re-emits the seam. These are same-source near-duplicates; we drop the
//!     newer copy.
//!
//! Matching is intentionally conservative: when unsure, keep both. The
//! thresholds favour never dropping real speech over removing every last
//! duplicate.

use std::collections::{HashSet, VecDeque};

/// Blended token-similarity at/above which two longer segments are treated as
/// the same utterance. Deliberately high — favours keeping.
pub const SIM_THRESHOLD: f64 = 0.72;

/// Slack (ms) added to each segment's interval when checking time overlap.
/// Echo arrives near-simultaneously, but the two streams are windowed
/// independently so their segment boundaries for the "same" utterance differ.
pub const TIME_SLACK_MS: u64 = 1_500;

/// Segments with fewer tokens than this are only ever treated as duplicates on
/// an *exact*, *cross-source* match — this protects legitimately repeated short
/// affirmations ("yeah", "okay", "right").
pub const MIN_TOKENS_FOR_FUZZY: usize = 4;

/// How long an accepted segment stays in the rolling buffer, measured back from
/// the newest segment. Must exceed one full transcription window (30 s) so a
/// repeat at the next window's seam still finds its original.
pub const RETENTION_MS: u64 = 45_000;

/// Lowercase and split into alphanumeric word tokens, dropping punctuation and
/// collapsing whitespace. Apostrophes (straight and curly) are removed rather
/// than split on, so a contraction transcribed as "let's" on one stream and
/// "lets" on the other normalizes to the same token.
pub fn normalize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace(|c: char| c == '\'' || c == '\u{2019}', "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// Token-set similarity: Jaccard, escalating to containment when the two token
/// sets differ a lot in size (a short fragment contained in a longer line — the
/// window-boundary-repeat signature). Returns 0.0 when either side is empty.
pub fn similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let set_a: HashSet<&String> = a.iter().collect();
    let set_b: HashSet<&String> = b.iter().collect();
    let inter = set_a.intersection(&set_b).count();
    if inter == 0 {
        return 0.0;
    }
    let union = set_a.union(&set_b).count();
    let jaccard = inter as f64 / union as f64;

    let min_len = set_a.len().min(set_b.len());
    let max_len = set_a.len().max(set_b.len());
    // Size-disparate => one set is a fragment of the other; containment is the
    // better signal in that case.
    if (min_len as f64) < 0.6 * max_len as f64 {
        let containment = inter as f64 / min_len as f64;
        jaccard.max(containment)
    } else {
        jaccard
    }
}

/// Whether two segments' `[start, end]` intervals overlap, each expanded by
/// `TIME_SLACK_MS`.
pub fn intervals_compatible(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> bool {
    a_start <= b_end.saturating_add(TIME_SLACK_MS) && b_start <= a_end.saturating_add(TIME_SLACK_MS)
}

/// Core predicate: are these two segments the same speech?
///
/// `cross_source` is true when the segments come from different streams (mic vs
/// system). Short segments only match when cross-source *and* token-identical,
/// which is the echo signature for brief utterances while still letting a
/// speaker legitimately repeat a short word within one stream.
#[allow(clippy::too_many_arguments)]
pub fn is_duplicate(
    a_tokens: &[String],
    a_start: u64,
    a_end: u64,
    b_tokens: &[String],
    b_start: u64,
    b_end: u64,
    cross_source: bool,
) -> bool {
    if a_tokens.is_empty() || b_tokens.is_empty() {
        return false;
    }
    if !intervals_compatible(a_start, a_end, b_start, b_end) {
        return false;
    }
    if a_tokens.len().min(b_tokens.len()) < MIN_TOKENS_FOR_FUZZY {
        // Short-utterance guard.
        return cross_source && a_tokens == b_tokens;
    }
    similarity(a_tokens, b_tokens) >= SIM_THRESHOLD
}

/// A finalized segment proposed for emission, before dedup.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub source: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

impl Candidate {
    fn to_entry(&self) -> DedupEntry {
        DedupEntry {
            source: self.source.clone(),
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            tokens: normalize(&self.text),
        }
    }
}

/// An accepted segment retained for cross-call comparison.
#[derive(Clone, Debug)]
pub struct DedupEntry {
    pub source: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub tokens: Vec<String>,
}

/// Rolling buffer of recently-accepted finalized segments, used to catch
/// duplicates that span transcription windows.
pub struct RecentSegments {
    entries: VecDeque<DedupEntry>,
}

impl RecentSegments {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Append accepted segments, then evict everything older than `RETENTION_MS`
    /// behind the newest retained segment.
    pub fn extend_and_prune(&mut self, accepted: &[Candidate]) {
        for c in accepted {
            self.entries.push_back(c.to_entry());
        }
        let Some(newest) = self.entries.iter().map(|e| e.start_ms).max() else {
            return;
        };
        self.entries
            .retain(|e| e.end_ms.saturating_add(RETENTION_MS) >= newest);
    }
}

/// Deduplicate one finalized batch against the rolling buffer.
///
/// `candidates` must be sorted by `start_ms`. Returns the accepted segments in
/// input order. Does not mutate `recent` — the caller commits accepted segments
/// via [`RecentSegments::extend_and_prune`] after emitting/persisting them.
pub fn dedup_finalized(recent: &RecentSegments, candidates: Vec<Candidate>) -> Vec<Candidate> {
    let n = candidates.len();
    if n == 0 {
        return Vec::new();
    }
    let toks: Vec<Vec<String>> = candidates.iter().map(|c| normalize(&c.text)).collect();
    let mut dropped = vec![false; n];

    // Pass 1: cross-stream echo. A mic segment that matches any system segment
    // in this batch (overlapping time, similar text) is the echo — drop it and
    // keep the cleaner system copy.
    for i in 0..n {
        if candidates[i].source != "microphone" {
            continue;
        }
        for j in 0..n {
            if i == j || candidates[j].source == "microphone" {
                continue;
            }
            if is_duplicate(
                &toks[i],
                candidates[i].start_ms,
                candidates[i].end_ms,
                &toks[j],
                candidates[j].start_ms,
                candidates[j].end_ms,
                true,
            ) {
                dropped[i] = true;
                break;
            }
        }
    }

    // Pass 2: same-source repeats vs the rolling buffer and segments already
    // accepted in this batch — drop the newer copy (the window-boundary seam).
    let mut accepted_idx: Vec<usize> = Vec::new();
    for i in 0..n {
        if dropped[i] {
            continue;
        }
        let dup_in_recent = recent.entries.iter().any(|e| {
            e.source == candidates[i].source
                && is_duplicate(
                    &toks[i],
                    candidates[i].start_ms,
                    candidates[i].end_ms,
                    &e.tokens,
                    e.start_ms,
                    e.end_ms,
                    false,
                )
        });
        let dup_in_batch = accepted_idx.iter().any(|&j| {
            candidates[j].source == candidates[i].source
                && is_duplicate(
                    &toks[i],
                    candidates[i].start_ms,
                    candidates[i].end_ms,
                    &toks[j],
                    candidates[j].start_ms,
                    candidates[j].end_ms,
                    false,
                )
        });
        if dup_in_recent || dup_in_batch {
            dropped[i] = true;
        } else {
            accepted_idx.push(i);
        }
    }

    candidates
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !dropped[*i])
        .map(|(_, c)| c)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<String> {
        normalize(s)
    }

    fn cand(source: &str, start: u64, end: u64, text: &str) -> Candidate {
        Candidate {
            source: source.into(),
            start_ms: start,
            end_ms: end,
            text: text.into(),
        }
    }

    #[test]
    fn normalize_strips_punct_and_case() {
        assert_eq!(normalize("Hello, World!  Yes…"), vec!["hello", "world", "yes"]);
    }

    #[test]
    fn normalize_empty_on_punct_only() {
        assert!(normalize("  …,. ").is_empty());
    }

    #[test]
    fn normalize_collapses_contractions() {
        // Straight and curly apostrophes both fold to the bare word, matching a
        // stream that dropped the apostrophe entirely.
        assert_eq!(normalize("let\u{2019}s go"), vec!["lets", "go"]);
        assert_eq!(normalize("let's go"), normalize("lets go"));
    }

    #[test]
    fn jaccard_identical_is_one() {
        let a = toks("ship it on thursday");
        assert!((similarity(&a, &a) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn jaccard_disjoint_is_zero() {
        assert_eq!(similarity(&toks("alpha beta"), &toks("gamma delta")), 0.0);
    }

    #[test]
    fn echo_garble_is_duplicate() {
        // one-word substitution out of seven (ship -> shift)
        let a = toks("i think we should ship it thursday");
        let b = toks("i think we should shift it thursday");
        assert!(is_duplicate(&a, 1000, 4000, &b, 1200, 4200, true));
    }

    #[test]
    fn distinct_but_shared_function_words_kept() {
        let a = toks("i think we should wait until monday");
        let b = toks("i think we should ship it thursday");
        assert!(!is_duplicate(&a, 1000, 4000, &b, 1000, 4000, true));
    }

    #[test]
    fn boundary_repeat_via_containment() {
        let prev = toks("we will finalize the plan on thursday and then we ship the build");
        let frag = toks("thursday and then we ship");
        // same-source, overlapping time -> window-boundary repeat
        assert!(is_duplicate(&frag, 30000, 32000, &prev, 28000, 33000, false));
    }

    #[test]
    fn short_affirmation_same_source_kept() {
        let a = toks("yeah");
        let b = toks("yeah");
        assert!(!is_duplicate(&a, 1000, 1500, &b, 1200, 1700, false));
    }

    #[test]
    fn short_affirmation_cross_source_exact_dropped() {
        let a = toks("okay");
        let b = toks("okay");
        assert!(is_duplicate(&a, 1000, 1500, &b, 1100, 1600, true));
    }

    #[test]
    fn short_affirmation_cross_source_nonexact_kept() {
        let a = toks("okay");
        let b = toks("yep");
        assert!(!is_duplicate(&a, 1000, 1500, &b, 1000, 1500, true));
    }

    #[test]
    fn time_gate_blocks_far_apart() {
        let a = toks("i think we should ship it thursday");
        let b = toks("i think we should ship it thursday");
        // ~16s apart, well beyond slack
        assert!(!is_duplicate(&a, 1000, 4000, &b, 20000, 23000, true));
    }

    #[test]
    fn time_gate_allows_slack_offset() {
        let a = toks("i think we should ship it thursday");
        let b = toks("i think we should ship it thursday");
        // mic starts 1.2s after the system segment ends -> within slack
        assert!(is_duplicate(&a, 1000, 4000, &b, 5200, 8000, true));
    }

    #[test]
    fn dedup_finalized_drops_mic_echo_keeps_system() {
        let recent = RecentSegments::new();
        let cands = vec![
            cand("system", 1000, 4000, "let's reconvene next week to review"),
            cand("microphone", 1200, 4200, "lets reconvene next week to review"),
        ];
        let out = dedup_finalized(&recent, cands);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, "system");
    }

    #[test]
    fn dedup_finalized_keeps_distinct_cross_source() {
        let recent = RecentSegments::new();
        let cands = vec![
            cand("system", 1000, 4000, "let's reconvene next week to review"),
            cand("microphone", 1200, 4200, "sounds good i will send an agenda"),
        ];
        let out = dedup_finalized(&recent, cands);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn dedup_finalized_drops_same_source_boundary_repeat() {
        let mut recent = RecentSegments::new();
        recent.extend_and_prune(&[cand("system", 28000, 31000, "and then we ship the build on friday")]);
        let cands = vec![
            cand("system", 31000, 34000, "and then we ship the build on friday"),
            cand("system", 34000, 37000, "after that we start the next milestone"),
        ];
        let out = dedup_finalized(&recent, cands);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "after that we start the next milestone");
    }

    #[test]
    fn dedup_finalized_empty() {
        let recent = RecentSegments::new();
        assert!(dedup_finalized(&recent, vec![]).is_empty());
    }

    #[test]
    fn extend_and_prune_evicts_old() {
        let mut recent = RecentSegments::new();
        recent.extend_and_prune(&[cand("system", 0, 2000, "old segment far in the past")]);
        assert_eq!(recent.len(), 1);
        // newest jumps far ahead: 2000 + 45000 = 47000 < 60000 -> old evicted
        recent.extend_and_prune(&[cand("system", 60000, 62000, "a brand new segment right now")]);
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn extend_and_prune_keeps_within_retention() {
        let mut recent = RecentSegments::new();
        recent.extend_and_prune(&[cand("system", 0, 2000, "first segment spoken here")]);
        recent.extend_and_prune(&[cand("system", 10000, 12000, "second segment a bit later")]);
        // 2000 + 45000 = 47000 >= 12000 -> both retained
        assert_eq!(recent.len(), 2);
    }
}
