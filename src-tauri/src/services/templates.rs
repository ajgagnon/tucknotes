//! Summary templates: composable "sections" assembled into the system prompt
//! that drives summarization.
//!
//! A [`SummaryTemplate`] is an ordered list of reusable [`Section`]s. The
//! system prompt is *assembled* from a template's sections plus a shared
//! preamble and rule set, so the same section (e.g. "Action items", with its
//! carefully-tuned formatting rules) is written once and reused across every
//! template that includes it.
//!
//! Only the four built-in templates exist today. The data model is deliberately
//! section-composition based so that user-defined templates — a future cycle —
//! are simply "a template with a user-chosen ordered list of sections".
//!
//! INVARIANT: `build_system_prompt(&builtin_as_owned(&DEFAULT_TEMPLATE))` must
//! match the golden snapshot in `default_template_prompt_snapshot` below
//! byte-for-byte, so the shipped Recap prompt can never drift accidentally.

use crate::models::template::{OwnedSection, OwnedTemplate};

/// One composable section of a summary template.
pub struct Section {
    /// Stable identifier (e.g. `"action_items"`). Reserved for user-defined
    /// templates, which will reference/reorder sections by id.
    #[allow(dead_code)]
    pub id: &'static str,
    /// The exact `##` heading text, without the leading `"## "`
    /// (e.g. `"Action items"`).
    pub heading: &'static str,
    /// The full `- ## Heading — …` line that goes in the prompt's
    /// "Section bodies:" block. Holds the leading `- ` and `## Heading — `
    /// prefix so the legacy prompt can be reproduced exactly.
    pub body_spec: &'static str,
    /// Whether the section is always emitted (only the Summary is, today). The
    /// behavior is also stated in the prompt text; this is the structured
    /// source of truth for future template tooling.
    #[allow(dead_code)]
    pub always_present: bool,
}

/// A section as used within a specific template: the shared [`Section`] plus the
/// template-specific illustrative example (if any). Examples are paired here, not
/// on the shared `Section`, so the same section (e.g. Action items) can carry a
/// Recap-flavored example without leaking it into the Minutes / 1:1 templates.
pub struct TemplateSection {
    pub section: &'static Section,
    /// Optional verbatim example for this section, shown under a `## {heading}`
    /// line in the prompt's "Example shape" block. Seeds the editable per-section
    /// example field in the Template Editor.
    pub example: Option<&'static str>,
}

/// An ordered set of sections (each with an optional example).
pub struct SummaryTemplate {
    /// Stable identifier persisted in `meetings.template` and exchanged with
    /// the frontend (`"default"`, `"minutes"`, `"one_on_one"`, `"standup"`).
    pub id: &'static str,
    /// Human-readable label shown in the template picker.
    pub name: &'static str,
    /// One-line description for the picker / settings UI.
    pub description: &'static str,
    /// Ordered sections that make up the output, each with an optional example.
    pub sections: &'static [TemplateSection],
}

/// Serializable summary of a template for the frontend pickers.
#[derive(Clone, serde::Serialize)]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    /// `true` for templates that ship with the app (resettable, not deletable).
    pub builtin: bool,
}

// ---------------------------------------------------------------------------
// Shared prompt fragments (verbatim, template-independent)
// ---------------------------------------------------------------------------

const EMIT_RULE: &str = "Rule for emitting a section: if and only if the section has content, emit its `##` heading on its own line, then a blank line, then the body. If a section has no content, omit both the heading and the body entirely. Never emit a heading with no body beneath it. Never emit body content without its heading directly above it.";

// Per-template illustrative examples, one per section. Each template's examples
// are assembled into a single "Example shape" block by `build_system_prompt`
// (and shown per pass by `build_section_system_prompt`). These seed the
// editable per-section example fields in the Template Editor.
const EXAMPLE_SUMMARY: &str = "The team agreed to ship v2 onboarding on Friday. QA gets the full week for regression. Dev cuts the release branch tonight; Priya drafts the launch email by Wednesday.";

const EXAMPLE_ACTION_ITEMS: &str = "- [ ] Cut the release branch tonight.
- [ ] Draft the launch email. `— Wed`
- [ ] Publish the updated dark-mode docs.";

const EXAMPLE_DECISIONS: &str = "- Version 2 onboarding
  - Will ship on Friday
- Country restrictions
  - Will not restrict any countries for the first iteration of the feature.";

const EXAMPLE_OPEN_QUESTIONS: &str = "- Announce the launch in-app, or just over email?
  - Marketing prefers email-only; an in-app banner needs design time.
- Which regions are in scope for the beta?
  - Legal review of the EU requirements is still pending.";

const EXAMPLE_AGENDA: &str = "- Shipping launch readiness
  - Release branch timing and the QA regression window.
- Open security ticket
  - Proposed response to the customer and who sends it.";

const EXAMPLE_DISCUSSION_ONE_ON_ONE: &str = "- Self Evaluation
  - Overall feels they are doing well.
  - Wants to work more on soft skills (communication, presentation)
- Review of Last Quarter’s Accomplishments
  - Headed up the eCommerce optimization project.
    - Released 2 weeks ahead of schedule.
    - Could improve on QA testing instead of early releasing.";

const EXAMPLE_ACTION_ITEMS_ONE_ON_ONE: &str = "- [ ] Enroll in the presentation-skills course. `— Fri`
- [ ] Draft goals for next quarter before the next 1:1.
- [ ] Share the QA checklist from the eCommerce launch.";

const EXAMPLE_FOLLOW_UPS: &str = "- Soft-skills development
  - Check in on presentation practice after the next team demo.
- QA process
  - Revisit early-release issues once the next release ships.";

const EXAMPLE_PROGRESS: &str = "- A decision was made on shipping
  - Decided to revert a commit because of a reversal of the decision to restrict countries.
  - It’s ready for review.
- A security ticket was addressed
  - The issue allowed anyone to edit a checkout.
  - It’s a non-issue because they need a UUID stored in their browser.
  - Raj will respond today.";

const EXAMPLE_BLOCKERS: &str = "- Shipping countries
  - A decision is needed on whether to restrict them to the 7 supported by shippo.
- Security Ticket
  - Feedback on response to customer needed.";

// ---------------------------------------------------------------------------
// Built-in sections (defined once, referenced by multiple templates)
// ---------------------------------------------------------------------------

/// The recurring instruction block shared by the bullet-list sections: forces
/// specific, context-rich bullets with explanatory sub-bullets instead of
/// vague one-liners. A macro (not a `const`) so it can be `concat!`-ed into
/// each section's `&'static` `body_spec`.
macro_rules! detail_spec {
    () => {
        "Be VERY specific. Always make sure to provide the full context and details for each bullet. If there is much context, add sub-bullets for each point (`  - `). Each noun in the bullet points needs sub-bullets to explain the context and details. Top level bullet items should generally be chronological by topic discussed. Top level bullets should be highly specific and not general."
    };
}

pub static SECTION_SUMMARY: Section = Section {
    id: "summary",
    heading: "Summary",
    body_spec: "- ## Summary — 2 to 4 sentences of prose (no bullets, no bold). Factual, terse, scannable. What happened, what was decided, and the immediate next steps in plain language. The Summary section is always present.",
    always_present: true,
};

pub static SECTION_DECISIONS: Section = Section {
    id: "decisions",
    heading: "Decisions",
    body_spec: concat!(
        "- ## Decisions — A bullet list (`- `) of choices the group made. ",
        detail_spec!()
    ),
    always_present: false,
};

pub static SECTION_ACTION_ITEMS: Section = Section {
    id: "action_items",
    heading: "Action items",
    body_spec: "- ## Action items — GitHub-flavored task list of work that is still outstanding. Only major items. Every line is exactly `- [ ] action.` (unchecked square brackets, never `- [x]`). Only list tasks that still need to be done — exclude anything the transcript indicates is already finished or completed. Do not use any attribution (no personal names, `Owner:`, `Speaker:`, `Team:`, or roles). When the transcript explicitly mentions a concrete deadline (a date, weekday, or relative day like \"tomorrow\"), append a space then an inline-code span containing an em dash and the date, like `` `— Wed` ``, at the end of the line. NEVER invent or guess deadlines, and NEVER emit `` `— TBD` ``, `` `— soon` ``, or similar placeholders — if there's no real deadline, just omit the suffix.",
    always_present: false,
};

pub static SECTION_OPEN_QUESTIONS: Section = Section {
    id: "open_questions",
    heading: "Open questions",
    body_spec: concat!(
        "- ## Open questions — A bullet list (`- `) of unresolved items, each phrased as a question ending with `?`. ",
        detail_spec!(),
        " Skip any topics that do not have currently open questions that were not resolved on the call."
    ),
    always_present: false,
};

pub static SECTION_AGENDA: Section = Section {
    id: "agenda",
    heading: "Agenda",
    body_spec: concat!(
        "- ## Agenda — A bullet list (`- `) of the topics the meeting set out to cover, in the order they were raised. ",
        detail_spec!()
    ),
    always_present: false,
};

pub static SECTION_DISCUSSION: Section = Section {
    id: "discussion",
    heading: "Discussion",
    body_spec: concat!(
        "- ## Discussion — A bullet list (`- `) of the substantive points raised and the reasoning behind them, grouped loosely by topic. ",
        detail_spec!()
    ),
    always_present: false,
};

pub static SECTION_FOLLOW_UPS: Section = Section {
    id: "follow_ups",
    heading: "Follow-ups",
    body_spec: concat!(
        "- ## Follow-ups — A bullet list (`- `) of items to revisit next time: carried-over topics, things explicitly deferred, or check-ins promised. Please disregard and skip anything that was fully resolved on the call. ",
        detail_spec!()
    ),
    always_present: false,
};

pub static SECTION_PROGRESS: Section = Section {
    id: "progress",
    heading: "Progress",
    body_spec: concat!(
        "- ## Progress — A bullet list (`- `) of what was completed or moved forward since the last update. ",
        detail_spec!()
    ),
    always_present: false,
};

pub static SECTION_BLOCKERS: Section = Section {
    id: "blockers",
    heading: "Blockers",
    body_spec: concat!(
        "- ## Blockers — A bullet list (`- `) of current impediments, dependencies, or anything needing help. Please disregard and skip all items that are not currently a blocker. ",
        detail_spec!()
    ),
    always_present: false,
};

// ---------------------------------------------------------------------------
// Built-in templates
// ---------------------------------------------------------------------------

pub static DEFAULT_TEMPLATE: SummaryTemplate = SummaryTemplate {
    id: "default",
    name: "Recap",
    description: "General-purpose summary with decisions, action items, and open questions.",
    sections: &[
        TemplateSection { section: &SECTION_SUMMARY, example: Some(EXAMPLE_SUMMARY) },
        TemplateSection { section: &SECTION_ACTION_ITEMS, example: Some(EXAMPLE_ACTION_ITEMS) },
        TemplateSection { section: &SECTION_DECISIONS, example: Some(EXAMPLE_DECISIONS) },
        TemplateSection { section: &SECTION_OPEN_QUESTIONS, example: Some(EXAMPLE_OPEN_QUESTIONS) },
    ],
};

pub static MINUTES_TEMPLATE: SummaryTemplate = SummaryTemplate {
    id: "minutes",
    name: "Meeting minutes",
    description: "Formal minutes: agenda and action items.",
    sections: &[
        TemplateSection { section: &SECTION_AGENDA, example: Some(EXAMPLE_AGENDA) },
        TemplateSection { section: &SECTION_ACTION_ITEMS, example: Some(EXAMPLE_ACTION_ITEMS) },
    ],
};

pub static ONE_ON_ONE_TEMPLATE: SummaryTemplate = SummaryTemplate {
    id: "one_on_one",
    name: "1:1",
    description: "One-on-one notes: discussion, action items, and follow-ups.",
    sections: &[
        TemplateSection {
            section: &SECTION_DISCUSSION,
            example: Some(EXAMPLE_DISCUSSION_ONE_ON_ONE),
        },
        TemplateSection {
            section: &SECTION_ACTION_ITEMS,
            example: Some(EXAMPLE_ACTION_ITEMS_ONE_ON_ONE),
        },
        TemplateSection { section: &SECTION_FOLLOW_UPS, example: Some(EXAMPLE_FOLLOW_UPS) },
    ],
};

pub static STANDUP_TEMPLATE: SummaryTemplate = SummaryTemplate {
    id: "standup",
    name: "Standup",
    description: "Daily standup: progress and blockers.",
    sections: &[
        TemplateSection { section: &SECTION_PROGRESS, example: Some(EXAMPLE_PROGRESS) },
        TemplateSection { section: &SECTION_BLOCKERS, example: Some(EXAMPLE_BLOCKERS) },
    ],
};

/// All built-in templates, in display order. The first entry is the default.
pub static BUILT_IN_TEMPLATES: &[&SummaryTemplate] = &[
    &DEFAULT_TEMPLATE,
    &MINUTES_TEMPLATE,
    &ONE_ON_ONE_TEMPLATE,
    &STANDUP_TEMPLATE,
];

/// Resolve a template id to a built-in template. `None` or an unknown id falls
/// back to the Default template (matches the `meetings.template IS NULL` →
/// default persistence convention).
pub fn template_by_id(id: Option<&str>) -> &'static SummaryTemplate {
    match id {
        Some(id) => BUILT_IN_TEMPLATES
            .iter()
            .find(|t| t.id == id)
            .copied()
            .unwrap_or(&DEFAULT_TEMPLATE),
        None => &DEFAULT_TEMPLATE,
    }
}

/// Serializable list of built-in templates for the frontend pickers.
pub fn list_templates() -> Vec<TemplateInfo> {
    BUILT_IN_TEMPLATES
        .iter()
        .map(|t| TemplateInfo {
            id: t.id.to_string(),
            name: t.name.to_string(),
            description: t.description.to_string(),
            builtin: true,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Conversion to the owned model (used to seed the store and as the "reset"
// source for built-in templates).
// ---------------------------------------------------------------------------

/// Recover a section's user-editable instructions from its `body_spec` by
/// stripping the `"- ## {heading} — "` prefix the prompt assembler re-adds.
fn section_description(s: &Section) -> &str {
    let prefix = format!("- ## {} — ", s.heading);
    s.body_spec.strip_prefix(prefix.as_str()).unwrap_or(s.body_spec)
}

/// Convert a built-in `'static` template into its owned (seed) form. Each
/// section's illustrative example is carried into the editable per-section
/// `example` field, so the assembled prompt stays byte-identical to the legacy
/// prompt and every example is editable in the Template Editor.
pub fn builtin_as_owned(t: &SummaryTemplate) -> OwnedTemplate {
    OwnedTemplate {
        id: t.id.to_string(),
        name: t.name.to_string(),
        description: t.description.to_string(),
        sections: t
            .sections
            .iter()
            .map(|ts| OwnedSection {
                id: ts.section.id.to_string(),
                heading: ts.section.heading.to_string(),
                description: section_description(ts.section).to_string(),
                example: ts.example.map(|e| e.to_string()),
            })
            .collect(),
        builtin: true,
    }
}

/// All built-in templates as owned seeds, in display order. This is the
/// "reset source" — resetting a built-in restores its entry here.
pub fn builtin_seeds() -> Vec<OwnedTemplate> {
    BUILT_IN_TEMPLATES.iter().map(|t| builtin_as_owned(t)).collect()
}

/// The owned seed for a built-in id, or `None` if `id` is not a built-in.
pub fn builtin_seed_by_id(id: &str) -> Option<OwnedTemplate> {
    BUILT_IN_TEMPLATES
        .iter()
        .find(|t| t.id == id)
        .map(|t| builtin_as_owned(t))
}

/// Whether `id` names a built-in template (reserved id).
pub fn is_builtin_id(id: &str) -> bool {
    BUILT_IN_TEMPLATES.iter().any(|t| t.id == id)
}

/// Spell out small counts so the prompt reads naturally ("four sections").
/// Falls back to the digit string for counts beyond nine so a longer template
/// can never panic.
fn count_word(n: usize) -> String {
    match n {
        1 => "one".to_string(),
        2 => "two".to_string(),
        3 => "three".to_string(),
        4 => "four".to_string(),
        5 => "five".to_string(),
        6 => "six".to_string(),
        7 => "seven".to_string(),
        8 => "eight".to_string(),
        9 => "nine".to_string(),
        _ => n.to_string(),
    }
}

/// Assemble the full system prompt for a template from a shared preamble, the
/// template's sections, and a shared rule set. Blocks are joined by a blank
/// line (`\n\n`); only the count word, the heading list, and the section-body
/// block vary per template.
pub fn build_system_prompt(template: &OwnedTemplate) -> String {
    let count = count_word(template.sections.len());
    let heading_list = template
        .sections
        .iter()
        .map(|s| format!("`## {}`", s.heading))
        .collect::<Vec<_>>()
        .join(", ");
    let bodies = template
        .sections
        .iter()
        .map(|s| format!("- ## {} — {}", s.heading, s.description))
        .collect::<Vec<_>>()
        .join("\n");

    let mut blocks: Vec<String> = Vec::with_capacity(6);

    // (A) Preamble intro.
    blocks.push(format!(
        "You are a professional, detail-oriented Meeting Analyst AI designed to review meeting transcripts and provide concise, actionable minutes for busy professionals.\nYou will be given a full transcript of a meeting, which may include a mix of speakers, topics, and discussion threads. Participants may use informal language, go off-topic, or interleave multiple subjects. Your job is to distill the transcript into the fixed {count}-section structure described below."
    ));

    // (B) Headings declaration.
    blocks.push(format!(
        "The output is composed of up to {count} sections. Each section has a fixed `##` markdown heading. The headings, in order, are exactly: {heading_list}. Never invent, rename, abbreviate, or reorder these headings."
    ));

    // (C) Emit rule.
    blocks.push(EMIT_RULE.to_string());

    // (D) Section bodies.
    blocks.push(format!("Section bodies:\n{bodies}"));

    // (E) Rules — universal only. Anything specific to a section (e.g. the
    // Action items checkbox and deadline format) lives in that section's
    // body, never here: a user template may contain any sections, so the
    // shared block must not name one.
    blocks.push(format!(
        "Rules:\n- Only name a person when the transcript clearly attributes the work, decision, or statement to them — never guess at attribution.\n- Skip filler, chit-chat, repeated points, and pleasantries.\n- No editorializing, no summarizing importance, no meta-commentary.\n- Do not invent labels beyond the {count} section headings.\n- Do not give the output a title — the title is generated separately."
    ));

    // (F) Examples. Each section's optional example is rendered under a
    // `## {heading}` line. The Recap seed populates these, which keeps its
    // assembled prompt byte-identical to the legacy prompt.
    let examples: Vec<String> = template
        .sections
        .iter()
        .filter_map(|s| s.example.as_ref().map(|ex| format!("## {}\n{}", s.heading, ex)))
        .collect();
    if !examples.is_empty() {
        blocks.push(format!(
            "Example shape (illustrative only, do not copy the content):\n{}",
            examples.join("\n\n")
        ));
    }

    blocks.join("\n\n")
}

/// Assemble the system prompt for a SINGLE section's pass. Multi-pass
/// summarization runs one focused inference per section (see
/// [`crate::services::summarization::SummarizationService::summarize`]), so the
/// model sees only this section's spec and can give it its full attention.
///
/// The shared blocks stay section-agnostic — the only section-specific text is
/// this section's own `heading`, `description`, and `example`, so no other
/// section's rules can leak in (each pass sees just one section).
///
/// The app injects the `## {heading}` line itself, so the model is told to write
/// the body ONLY. This guarantees the exact heading and makes "omit an empty
/// section" trivial: no body → no heading.
pub fn build_section_system_prompt(section: &OwnedSection) -> String {
    let heading = &section.heading;
    let mut blocks: Vec<String> = Vec::with_capacity(6);

    // (A) Preamble — same role framing as the full prompt, narrowed to one section.
    blocks.push(format!(
        "You are a professional, detail-oriented Meeting Analyst AI designed to review meeting transcripts and provide concise, specific, actionable minutes for busy professionals.\nYou will be given a full transcript of a meeting, which may include a mix of speakers, topics, and discussion threads. Participants may use informal language, go off-topic, or interleave multiple subjects. Your job is to write a single section of the minutes — the `## {heading}` section — described below."
    ));

    // (B) Body-only contract. The heading is added by the app, so the model must
    // not emit it — even if the section spec below happens to mention the heading.
    blocks.push(format!(
        "Write ONLY the body of the `## {heading}` section — the content that belongs beneath that heading. Do NOT output the `## {heading}` line itself, any other `##` heading, or a title; the heading is added for you. Output the body and nothing else."
    ));

    // (C) The section's own tuned spec (e.g. the Action items checkbox and
    // deadline rules live here, and only reach the Action items pass).
    blocks.push(format!("Section spec:\n- ## {heading} — {}", section.description));

    // (D) Emit rule — the single-section variant of the full prompt's EMIT_RULE.
    blocks.push(
        "If the transcript contains nothing for this section, output nothing at all — no heading, no body, and no placeholder such as \"None\" or \"N/A\". Only write content that is genuinely supported by the transcript. A vague topic-label with no substance does not count as content — omit it rather than pad the section.".to_string(),
    );

    // (E) Universal rules — section-agnostic, mirroring the full prompt's Rules
    // block (minus its multi-section "do not invent labels" line), plus the
    // specificity rules that keep every line concrete rather than abstract.
    blocks.push(
        "Rules:\n- Be concrete and self-contained. Every sentence or bullet must state the specific substance — the actual change, decision, number, name, file, or result — so a reader who was not in the meeting understands it without the transcript. Naming the activity is not enough.\n- Never write a meta-label that only names a topic with no content — no lines like <topic> discussed, <topic> covered, or <topic> reviewed. BAD: \"Potential implementation discussed\" (which implementation? proposing what?), \"Refactoring completed and tested\" (refactored what?). GOOD: \"Proposed caching search results in Redis to cut p95 latency\", \"Refactored the billing retry path; unit tests added and passing.\"\n- If the transcript does not give the specifics, include the concrete detail it does give, or omit the point — never pad with an abstraction.\n- Only name a person when the transcript clearly attributes the work, decision, or statement to them — never guess at attribution.\n- Skip filler, chit-chat, repeated points, and pleasantries.\n- No editorializing, no summarizing importance, no meta-commentary.\n- Do not give the output a title — the title is generated separately.".to_string(),
    );

    // (F) Optional example — the body only (no `## {heading}` line, since the
    // model writes the body only).
    if let Some(example) = &section.example {
        blocks.push(format!(
            "Example shape (illustrative only, do not copy the content):\n{example}"
        ));
    }

    blocks.join("\n\n")
}

/// System prompt for the live-minutes pass that runs while a meeting is still
/// being recorded. The document is an append-only log of short bullets: the
/// model is shown the MEETING CONTEXT SO FAR (a model-facing rolling gist,
/// never displayed), the bullets ALREADY RECORDED (context only — never
/// changed), the PRIOR TRANSCRIPT it already processed, and the NEW TRANSCRIPT
/// chunk, and returns only bullets for genuinely noteworthy NEW information,
/// or nothing at all. Code appends whatever comes back; nothing already
/// recorded is ever rewritten. Keeping the per-pass task this small (judge one
/// chunk, usually stay silent) is what lets a small quantized model produce
/// sparse, high-signal minutes. This prompt is standalone rather than
/// assembled from sections.
pub fn live_minutes_system_prompt() -> String {
    "You maintain a running bullet-point log of a meeting happening live. Each turn you are shown:
- MEETING CONTEXT SO FAR: a rough summary of the meeting up to now (background only — it may be incomplete; never copy anything from it into a bullet).
- ALREADY RECORDED: the most recent bullets on the log (context only — never repeat, reorder, or change them).
- PRIOR TRANSCRIPT: the last lines you already processed (context only — never record a point that appears only here).
- NEW TRANSCRIPT: the lines that just occurred. This is the ONLY text you may record from. Use PRIOR TRANSCRIPT only to understand a sentence that starts there and finishes in NEW TRANSCRIPT.

Write a bullet ONLY for genuinely noteworthy NEW information in the NEW TRANSCRIPT. Most chunks contain nothing worth recording — when that is the case, output NOTHING AT ALL. Before writing a bullet, ask: would this still matter to someone reading the notes tomorrow? If not, stay silent.

Each bullet must state the actual takeaway — the decision, the fact, the number, the position someone took — so a reader who wasn't there learns the substance from the bullet alone. Naming the topic is not enough.

Be specific. Carry exact numbers, names, dates, amounts, and owners from the transcript into the bullet. Use MEETING CONTEXT to judge what is important to this meeting and to resolve references: if the transcript says \"that earlier proposal\" and the context makes clear which one, name it — but never guess.

Record only:
- a decision or conclusion the group reached
- an action item, commitment, owner, or deadline
- a concrete fact, number, name, or date that matters
- a specific position or claim someone stated

Never record: greetings, small talk, thinking out loud, a question with no answer yet, anything already on the list or equivalent to a listed point in different words, or a topic that was raised but reached no concrete point. When in doubt, say nothing.

Output format:
- One short bullet per point, starting at the start of the line with \"- \". A terse fragment, not a sentence.
- State the point itself, never just the topic. Never write meta-labels like \"Discussed X\", \"Talked about Y\", or \"Covered Z\" — if you cannot state a concrete takeaway, output nothing.
- No headings, numbering, sub-bullets, commentary, or blank lines — output bullets only, or nothing at all.
- Never invent anything not stated in the transcript.
- \"You\" is the person recording the meeting; \"Speaker\" is another participant. Use a name only when the transcript clearly attributes a statement to that name.

Examples (illustrative only):
- BAD (names the topic, says nothing): \"- Discussed performance expectations for newer titles\"
- GOOD (states the takeaway): \"- Newer titles must hold 60fps on current hardware\"
- BAD (vague, no specifics): \"- Budget concerns raised\"
- GOOD (specific): \"- Q3 marketing budget cut 15%, to roughly $85k\"
- Restates an ALREADY RECORDED point in different words → output nothing
- Topic raised but no conclusion reached → output nothing
- Greetings or small talk → output nothing"
        .to_string()
}

/// System prompt for the gist-update pass: maintains the model-facing rolling
/// summary of the meeting (the MEETING CONTEXT SO FAR block in the minutes
/// prompt). The gist is private working state — never persisted, displayed,
/// or copied into the minutes — so unlike the minutes it is rewritten
/// wholesale on every update.
pub fn meeting_gist_system_prompt() -> String {
    "You maintain a short private working summary of a meeting in progress. It is used only as background context for another note-taking step and is never shown to anyone.

You are given the CURRENT CONTEXT (your previous summary — may be \"(start of meeting)\") and a NEW TRANSCRIPT chunk. Output the UPDATED CONTEXT: one rewritten summary that folds the new information into the old.

Keep it under 120 words, in this shape, each line terse:
Participants: <who is speaking; names or roles only if actually stated>
Topics: <topics in the order raised, a few words each>
Key threads: <unresolved questions, recurring themes, decisions in progress>

Rules:
- Rewrite freely — merge, compress, and drop stale detail to stay under the limit; recent discussion outweighs old small talk.
- Keep exact names, numbers, and dates that look load-bearing.
- Never invent anything not present in the context or transcript.
- \"You\" is the person recording; \"Speaker\" is another participant.
- Output only the updated context. No commentary, no bullets, no headings other than the three labels."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The expected assembled system prompt for the built-in Recap template — a
    /// golden snapshot guarding the built-in prompt against accidental drift.
    /// `build_system_prompt(&DEFAULT_TEMPLATE)` must reproduce this verbatim.
    const DEFAULT_SYSTEM_PROMPT: &str = "\
You are a professional, detail-oriented Meeting Analyst AI designed to review meeting transcripts \
and provide concise, actionable minutes for busy professionals.\n\
You will be given a full transcript of a meeting, which may include a mix of speakers, topics, and \
discussion threads. Participants may use informal language, go off-topic, or interleave multiple subjects. \
Your job is to distill the transcript into the fixed four-section structure described below.\n\
\n\
The output is composed of up to four sections. Each section has a fixed `##` markdown heading. \
The headings, in order, are exactly: `## Summary`, `## Action items`, `## Decisions`, `## Open questions`. \
Never invent, rename, abbreviate, or reorder these headings.\n\
\n\
Rule for emitting a section: if and only if the section has content, emit its `##` heading on its own line, \
then a blank line, then the body. If a section has no content, omit both the heading and the body entirely. \
Never emit a heading with no body beneath it. Never emit body content without its heading directly above it.\n\
\n\
Section bodies:\n\
- ## Summary — 2 to 4 sentences of prose (no bullets, no bold). Factual, terse, scannable. What happened, what was decided, and the immediate next steps in plain language. The Summary section is always present.\n\
- ## Action items — GitHub-flavored task list of work that is still outstanding. Only major items. Every line is exactly `- [ ] action.` (unchecked square brackets, never `- [x]`). Only list tasks that still need to be done — exclude anything the transcript indicates is already finished or completed. Do not use any attribution (no personal names, `Owner:`, `Speaker:`, `Team:`, or roles). When the transcript explicitly mentions a concrete deadline (a date, weekday, or relative day like \"tomorrow\"), append a space then an inline-code span containing an em dash and the date, like `` `— Wed` ``, at the end of the line. NEVER invent or guess deadlines, and NEVER emit `` `— TBD` ``, `` `— soon` ``, or similar placeholders — if there's no real deadline, just omit the suffix.\n\
- ## Decisions — A bullet list (`- `) of choices the group made. Be VERY specific. Always make sure to provide the full context and details for each bullet. If there is much context, add sub-bullets for each point (`  - `). Each noun in the bullet points needs sub-bullets to explain the context and details. Top level bullet items should generally be chronological by topic discussed. Top level bullets should be highly specific and not general.\n\
- ## Open questions — A bullet list (`- `) of unresolved items, each phrased as a question ending with `?`. Be VERY specific. Always make sure to provide the full context and details for each bullet. If there is much context, add sub-bullets for each point (`  - `). Each noun in the bullet points needs sub-bullets to explain the context and details. Top level bullet items should generally be chronological by topic discussed. Top level bullets should be highly specific and not general. Skip any topics that do not have currently open questions that were not resolved on the call.\n\
\n\
Rules:\n\
- Only name a person when the transcript clearly attributes the work, decision, or statement to them — never guess at attribution.\n\
- Skip filler, chit-chat, repeated points, and pleasantries.\n\
- No editorializing, no summarizing importance, no meta-commentary.\n\
- Do not invent labels beyond the four section headings.\n\
- Do not give the output a title — the title is generated separately.\n\
\n\
Example shape (illustrative only, do not copy the content):\n\
## Summary\n\
The team agreed to ship v2 onboarding on Friday. QA gets the full week for regression. Dev cuts the release branch tonight; Priya drafts the launch email by Wednesday.\n\
\n\
## Action items\n\
- [ ] Cut the release branch tonight.\n\
- [ ] Draft the launch email. `— Wed`\n\
- [ ] Publish the updated dark-mode docs.\n\
\n\
## Decisions\n\
- Version 2 onboarding\n\
\x20 - Will ship on Friday\n\
- Country restrictions\n\
\x20 - Will not restrict any countries for the first iteration of the feature.\n\
\n\
## Open questions\n\
- Announce the launch in-app, or just over email?\n\
\x20 - Marketing prefers email-only; an in-app banner needs design time.\n\
- Which regions are in scope for the beta?\n\
\x20 - Legal review of the EU requirements is still pending.";

    #[test]
    fn default_template_prompt_snapshot() {
        assert_eq!(
            build_system_prompt(&builtin_as_owned(&DEFAULT_TEMPLATE)),
            DEFAULT_SYSTEM_PROMPT
        );
    }

    #[test]
    fn custom_template_rules_block_is_section_agnostic() {
        // A user template that doesn't use the built-in Action items section must
        // not inherit its rules: the shared Rules block names no section, so the
        // model is never told to attribute owners the template didn't ask for.
        // Mirrors the real "Action Items" custom template (id `quick_summary`).
        let t = OwnedTemplate {
            id: "quick_summary".to_string(),
            name: "Action Items".to_string(),
            description: String::new(),
            sections: vec![OwnedSection {
                id: "new-2".to_string(),
                heading: "Action Items".to_string(),
                description:
                    "GitHub-flavored task list of outstanding work. Every line is exactly `- [ ] action.`"
                        .to_string(),
                example: None,
            }],
            builtin: false,
        };
        let prompt = build_system_prompt(&t);
        assert!(!prompt.contains("Only major items"), "leaked the built-in Action items spec");
        assert!(!prompt.contains("no personal names"), "leaked an Action items rule");
        assert!(
            !prompt.contains("Name people only in the Summary and Decisions"),
            "named sections the template does not contain"
        );
        assert!(prompt.contains("Skip filler, chit-chat"), "lost the universal rules");

        // Built-ins without an Action items section likewise carry none of its rules.
        assert!(
            !build_system_prompt(&builtin_as_owned(&STANDUP_TEMPLATE)).contains("Only major items")
        );
    }

    #[test]
    fn all_built_in_templates_build_without_panic() {
        for t in BUILT_IN_TEMPLATES {
            let prompt = build_system_prompt(&builtin_as_owned(t));
            assert!(!prompt.is_empty(), "{} produced an empty prompt", t.id);
            // Every template's headings must appear in order.
            let mut cursor = 0;
            for ts in t.sections {
                let needle = format!("`## {}`", ts.section.heading);
                let at = prompt[cursor..].find(&needle).unwrap_or_else(|| {
                    panic!("{}: heading {:?} missing or out of order", t.id, needle)
                });
                cursor += at + needle.len();
            }
        }
    }

    #[test]
    fn templates_reuse_action_items_section_verbatim() {
        // The tuned Action items rules must carry into every template that uses it.
        for t in [&MINUTES_TEMPLATE, &ONE_ON_ONE_TEMPLATE] {
            let prompt = build_system_prompt(&builtin_as_owned(t));
            assert!(
                prompt.contains(SECTION_ACTION_ITEMS.body_spec),
                "{} lost the verbatim Action items body_spec",
                t.id
            );
        }
    }

    #[test]
    fn section_description_is_lossless() {
        // Splitting body_spec into heading + description must reassemble exactly.
        let s = &SECTION_ACTION_ITEMS;
        let rebuilt = format!("- ## {} — {}", s.heading, section_description(s));
        assert_eq!(rebuilt, s.body_spec);
    }

    #[test]
    fn builtin_seeds_match_static_ids() {
        let ids: Vec<String> = builtin_seeds().into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["default", "minutes", "one_on_one", "standup"]);
        assert!(builtin_seeds().iter().all(|t| t.builtin));
        assert!(builtin_seed_by_id("minutes").is_some());
        assert!(builtin_seed_by_id("nope").is_none());
        assert!(is_builtin_id("standup"));
        assert!(!is_builtin_id("custom"));
    }

    #[test]
    fn per_section_example_emits_example_block() {
        let mut t = builtin_as_owned(&STANDUP_TEMPLATE);
        for s in &mut t.sections {
            s.example = None; // strip the shipped examples
        }
        assert!(!build_system_prompt(&t).contains("Example shape"));
        t.sections[0].example = Some("- Shipped the login flow.".to_string());
        let prompt = build_system_prompt(&t);
        assert!(prompt.contains("Example shape (illustrative only, do not copy the content):"));
        assert!(prompt.contains("## Progress\n- Shipped the login flow."));
    }

    #[test]
    fn minutes_template_shape() {
        let prompt = build_system_prompt(&builtin_as_owned(&MINUTES_TEMPLATE));
        assert!(prompt.contains("up to two sections"));
        for h in ["## Agenda", "## Action items"] {
            assert!(prompt.contains(h), "minutes missing {h}");
        }
        // Sections from other templates must not leak in.
        assert!(!prompt.contains("## Discussion"));
        assert!(!prompt.contains("## Decisions"));
        assert!(!prompt.contains("## Open questions"));
        assert!(!prompt.contains("## Blockers"));
        // Every section ships an illustrative example.
        assert!(prompt.contains("Example shape"));
    }

    #[test]
    fn standup_template_shape() {
        let prompt = build_system_prompt(&builtin_as_owned(&STANDUP_TEMPLATE));
        assert!(prompt.contains("up to two sections"));
        for h in ["## Progress", "## Blockers"] {
            assert!(prompt.contains(h), "standup missing {h}");
        }
        assert!(!prompt.contains("## Plans"));
        assert!(!prompt.contains("## Summary"));
        assert!(!prompt.contains("## Action items"));
    }

    #[test]
    fn one_on_one_template_shape() {
        let prompt = build_system_prompt(&builtin_as_owned(&ONE_ON_ONE_TEMPLATE));
        assert!(prompt.contains("up to three sections"));
        for h in ["## Discussion", "## Action items", "## Follow-ups"] {
            assert!(prompt.contains(h), "1:1 missing {h}");
        }
    }

    #[test]
    fn template_by_id_resolves_known_and_falls_back() {
        assert_eq!(template_by_id(Some("default")).id, "default");
        assert_eq!(template_by_id(Some("minutes")).id, "minutes");
        assert_eq!(template_by_id(Some("one_on_one")).id, "one_on_one");
        assert_eq!(template_by_id(Some("standup")).id, "standup");
        // Unknown / None → default.
        assert_eq!(template_by_id(Some("does_not_exist")).id, "default");
        assert_eq!(template_by_id(None).id, "default");
    }

    #[test]
    fn list_templates_exposes_all_built_ins() {
        let infos = list_templates();
        let ids: Vec<&str> = infos.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["default", "minutes", "one_on_one", "standup"]);
    }

    #[test]
    fn count_word_spells_small_numbers() {
        assert_eq!(count_word(3), "three");
        assert_eq!(count_word(4), "four");
        assert_eq!(count_word(10), "10");
    }

    #[test]
    fn section_prompt_writes_body_only_and_keeps_section_rules() {
        let recap = builtin_as_owned(&DEFAULT_TEMPLATE);
        let action_items = recap
            .sections
            .iter()
            .find(|s| s.heading == "Action items")
            .expect("Recap has an Action items section");
        let prompt = build_section_system_prompt(action_items);

        // The body-only contract is present (the app injects the heading).
        assert!(prompt.contains("Write ONLY the body"));
        assert!(prompt.contains("Do NOT output the `## Action items` line"));
        // The section's own tuned rules carry through.
        assert!(prompt.contains("Only major items"));
        assert!(prompt.contains("NEVER invent or guess deadlines"));
        // Its example is shown, but WITHOUT a `## Action items` heading above it.
        assert!(prompt.contains("Example shape (illustrative only, do not copy the content):"));
        assert!(!prompt.contains("## Action items\n- [ ]"));
        // Universal rules are present.
        assert!(prompt.contains("Skip filler, chit-chat"));
    }

    #[test]
    fn section_prompt_does_not_leak_other_sections_rules() {
        // A pass for a non-Action-items section must not carry its checkbox rules.
        let recap = builtin_as_owned(&DEFAULT_TEMPLATE);
        let summary = recap
            .sections
            .iter()
            .find(|s| s.heading == "Summary")
            .expect("Recap has a Summary section");
        let prompt = build_section_system_prompt(summary);
        assert!(!prompt.contains("Only major items"), "leaked the Action items spec");
        assert!(!prompt.contains("- [ ]"), "leaked the Action items checkbox format");
    }

    #[test]
    fn section_prompt_example_only_when_present() {
        // With the example stripped, no example block appears.
        let standup = builtin_as_owned(&STANDUP_TEMPLATE);
        let mut progress = standup.sections[0].clone();
        progress.example = None;
        assert!(!build_section_system_prompt(&progress).contains("Example shape"));

        // With an example set, the example block appears verbatim.
        progress.example = Some("- Shipped the login flow.".to_string());
        let prompt = build_section_system_prompt(&progress);
        assert!(prompt.contains("Example shape (illustrative only, do not copy the content):"));
        assert!(prompt.contains("- Shipped the login flow."));
    }

    #[test]
    fn section_prompt_demands_concrete_specifics() {
        // The universal Rules block must push the model toward concrete, self-
        // contained lines (with the abstract-line example) — for any section.
        let recap = builtin_as_owned(&DEFAULT_TEMPLATE);
        let summary = recap
            .sections
            .iter()
            .find(|s| s.heading == "Summary")
            .expect("Recap has a Summary section");
        let prompt = build_section_system_prompt(summary);
        assert!(prompt.contains("Be concrete and self-contained"));
        assert!(prompt.contains("Refactoring completed and tested"));
    }
}
