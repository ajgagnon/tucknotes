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
//! reproduce the original hardcoded system prompt byte-for-byte. This is locked
//! down by `default_template_is_byte_identical_to_legacy` below.

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

/// An ordered set of sections plus optional illustrative example.
pub struct SummaryTemplate {
    /// Stable identifier persisted in `meetings.template` and exchanged with
    /// the frontend (`"default"`, `"minutes"`, `"one_on_one"`, `"standup"`).
    pub id: &'static str,
    /// Human-readable label shown in the template picker.
    pub name: &'static str,
    /// One-line description for the picker / settings UI.
    pub description: &'static str,
    /// Ordered sections that make up the output.
    pub sections: &'static [&'static Section],
    /// Optional verbatim "Example shape …" block appended to the prompt.
    pub example: Option<&'static str>,
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

/// The Default template's illustrative example, kept verbatim so the assembled
/// Default prompt matches the original byte-for-byte.
const DEFAULT_EXAMPLE: &str = "Example shape (illustrative only, do not copy the content):
## Summary
The team agreed to ship v2 onboarding on Friday. QA gets the full week for regression. Dev cuts the release branch tonight; Priya drafts the launch email by Wednesday.

## Decisions
- Ship v2 onboarding on Friday.
- Hold the redesigned empty state for v2.1 — not a launch blocker.

## Action items
- [ ] Cut the release branch tonight.
- [ ] Draft the launch email. `— Wed`
- [ ] Publish the updated dark-mode docs.

## Open questions
- Announce in-app, or just over email?";

// ---------------------------------------------------------------------------
// Built-in sections (defined once, referenced by multiple templates)
// ---------------------------------------------------------------------------

pub static SECTION_SUMMARY: Section = Section {
    id: "summary",
    heading: "Summary",
    body_spec: "- ## Summary — 2 to 4 sentences of prose (no bullets, no bold). Factual, terse, scannable. What happened, what was decided, and the immediate next steps in plain language. The Summary section is always present.",
    always_present: true,
};

pub static SECTION_DECISIONS: Section = Section {
    id: "decisions",
    heading: "Decisions",
    body_spec: "- ## Decisions — bullet list (`- `) of choices the group made. Fragments, not full sentences. No \"Decision:\" prefix.",
    always_present: false,
};

pub static SECTION_ACTION_ITEMS: Section = Section {
    id: "action_items",
    heading: "Action items",
    body_spec: "- ## Action items — GitHub-flavored task list. Every line is exactly `- [ ] action.` (unchecked square brackets, never `- [x]`). NEVER prefix the action with an owner, assignee, name, or role — no `**Name:**`, `Speaker:`, `Owner:`, `Team:`, or similar. The action stands on its own. When the transcript explicitly mentions a concrete deadline (a date, weekday, or relative day like \"tomorrow\"), append a space then an inline-code span containing an em dash and the date, like `` `— Wed` ``. NEVER invent or guess deadlines, and NEVER emit `` `— TBD` ``, `` `— soon` ``, or similar placeholders — if there's no real deadline, just omit the suffix.",
    always_present: false,
};

pub static SECTION_OPEN_QUESTIONS: Section = Section {
    id: "open_questions",
    heading: "Open questions",
    body_spec: "- ## Open questions — bullet list (`- `) of unresolved items, each phrased as a question ending with `?`. If you have any open question to list, you MUST emit the `## Open questions` heading line directly above the bullets.",
    always_present: false,
};

pub static SECTION_AGENDA: Section = Section {
    id: "agenda",
    heading: "Agenda",
    body_spec: "- ## Agenda — bullet list (`- `) of the topics the meeting set out to cover, in the order they were raised. Fragments, not full sentences. Omit this section if no agenda or clear topic list is evident.",
    always_present: false,
};

pub static SECTION_DISCUSSION: Section = Section {
    id: "discussion",
    heading: "Discussion",
    body_spec: "- ## Discussion — bullet list (`- `) of the substantive points raised and the reasoning behind them, grouped loosely by topic. Fragments, not full sentences. Exclude decisions and action items — those belong in their own sections.",
    always_present: false,
};

pub static SECTION_FOLLOW_UPS: Section = Section {
    id: "follow_ups",
    heading: "Follow-ups",
    body_spec: "- ## Follow-ups — bullet list (`- `) of items to revisit next time: carried-over topics, things explicitly deferred, or check-ins promised. Fragments, not full sentences. Omit this section if there are none.",
    always_present: false,
};

pub static SECTION_PROGRESS: Section = Section {
    id: "progress",
    heading: "Progress",
    body_spec: "- ## Progress — bullet list (`- `) of what was completed or moved forward since the last update. Fragments, not full sentences. Omit this section if none is stated.",
    always_present: false,
};

pub static SECTION_PLANS: Section = Section {
    id: "plans",
    heading: "Plans",
    body_spec: "- ## Plans — bullet list (`- `) of what is planned next or currently being worked on. Fragments, not full sentences. Omit this section if none is stated.",
    always_present: false,
};

pub static SECTION_BLOCKERS: Section = Section {
    id: "blockers",
    heading: "Blockers",
    body_spec: "- ## Blockers — bullet list (`- `) of impediments, dependencies, or anything needing help. Fragments, not full sentences. Omit this section if there are no blockers.",
    always_present: false,
};

// ---------------------------------------------------------------------------
// Built-in templates
// ---------------------------------------------------------------------------

pub static DEFAULT_TEMPLATE: SummaryTemplate = SummaryTemplate {
    id: "default",
    name: "Default",
    description: "General-purpose summary with decisions, action items, and open questions.",
    sections: &[
        &SECTION_SUMMARY,
        &SECTION_DECISIONS,
        &SECTION_ACTION_ITEMS,
        &SECTION_OPEN_QUESTIONS,
    ],
    example: Some(DEFAULT_EXAMPLE),
};

pub static MINUTES_TEMPLATE: SummaryTemplate = SummaryTemplate {
    id: "minutes",
    name: "Meeting minutes",
    description: "Formal minutes: agenda, discussion, decisions, and action items.",
    sections: &[
        &SECTION_AGENDA,
        &SECTION_DISCUSSION,
        &SECTION_DECISIONS,
        &SECTION_ACTION_ITEMS,
    ],
    example: None,
};

pub static ONE_ON_ONE_TEMPLATE: SummaryTemplate = SummaryTemplate {
    id: "one_on_one",
    name: "1:1",
    description: "One-on-one notes: discussion, action items, and follow-ups.",
    sections: &[
        &SECTION_DISCUSSION,
        &SECTION_ACTION_ITEMS,
        &SECTION_FOLLOW_UPS,
    ],
    example: None,
};

pub static STANDUP_TEMPLATE: SummaryTemplate = SummaryTemplate {
    id: "standup",
    name: "Standup",
    description: "Daily standup: progress, plans, and blockers.",
    sections: &[&SECTION_PROGRESS, &SECTION_PLANS, &SECTION_BLOCKERS],
    example: None,
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

/// Convert a built-in `'static` template into its owned (seed) form. The
/// template-level example (Default only) is carried in `template_example` so
/// the assembled prompt stays byte-identical to the legacy prompt.
pub fn builtin_as_owned(t: &SummaryTemplate) -> OwnedTemplate {
    OwnedTemplate {
        id: t.id.to_string(),
        name: t.name.to_string(),
        description: t.description.to_string(),
        sections: t
            .sections
            .iter()
            .map(|s| OwnedSection {
                id: s.id.to_string(),
                heading: s.heading.to_string(),
                description: section_description(s).to_string(),
                example: None,
            })
            .collect(),
        builtin: true,
        template_example: t.example.map(|e| e.to_string()),
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

    // (E) Rules.
    blocks.push(format!(
        "Rules:\n- Use the em dash character `—` (not `--`) before due dates.\n- Name people only in the Summary and Decisions sections, and only when the transcript clearly attributes the work or decision to them. Action items never carry a name.\n- Skip filler, chit-chat, repeated points, and pleasantries.\n- No editorializing, no summarizing importance, no meta-commentary.\n- Do not invent labels beyond the {count} section headings.\n- Do not give the output a title — the title is generated separately."
    ));

    // (F) Examples. Per-section examples (user templates) take precedence; the
    // shipped built-in seeds fall back to their frozen template-level example,
    // which keeps the Default prompt byte-identical to the legacy prompt.
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
    } else if let Some(ex) = &template.template_example {
        blocks.push(ex.clone());
    }

    blocks.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact system prompt as it existed before templates were introduced.
    /// `build_system_prompt(&DEFAULT_TEMPLATE)` must reproduce this verbatim.
    const LEGACY_SYSTEM_PROMPT: &str = "\
You are a professional, detail-oriented Meeting Analyst AI designed to review meeting transcripts \
and provide concise, actionable minutes for busy professionals.\n\
You will be given a full transcript of a meeting, which may include a mix of speakers, topics, and \
discussion threads. Participants may use informal language, go off-topic, or interleave multiple subjects. \
Your job is to distill the transcript into the fixed four-section structure described below.\n\
\n\
The output is composed of up to four sections. Each section has a fixed `##` markdown heading. \
The headings, in order, are exactly: `## Summary`, `## Decisions`, `## Action items`, `## Open questions`. \
Never invent, rename, abbreviate, or reorder these headings.\n\
\n\
Rule for emitting a section: if and only if the section has content, emit its `##` heading on its own line, \
then a blank line, then the body. If a section has no content, omit both the heading and the body entirely. \
Never emit a heading with no body beneath it. Never emit body content without its heading directly above it.\n\
\n\
Section bodies:\n\
- ## Summary — 2 to 4 sentences of prose (no bullets, no bold). Factual, terse, scannable. What happened, what was decided, and the immediate next steps in plain language. The Summary section is always present.\n\
- ## Decisions — bullet list (`- `) of choices the group made. Fragments, not full sentences. No \"Decision:\" prefix.\n\
- ## Action items — GitHub-flavored task list. Every line is exactly `- [ ] action.` (unchecked square brackets, never `- [x]`). NEVER prefix the action with an owner, assignee, name, or role — no `**Name:**`, `Speaker:`, `Owner:`, `Team:`, or similar. The action stands on its own. When the transcript explicitly mentions a concrete deadline (a date, weekday, or relative day like \"tomorrow\"), append a space then an inline-code span containing an em dash and the date, like `` `— Wed` ``. NEVER invent or guess deadlines, and NEVER emit `` `— TBD` ``, `` `— soon` ``, or similar placeholders — if there's no real deadline, just omit the suffix.\n\
- ## Open questions — bullet list (`- `) of unresolved items, each phrased as a question ending with `?`. If you have any open question to list, you MUST emit the `## Open questions` heading line directly above the bullets.\n\
\n\
Rules:\n\
- Use the em dash character `—` (not `--`) before due dates.\n\
- Name people only in the Summary and Decisions sections, and only when the transcript clearly attributes the work or decision to them. Action items never carry a name.\n\
- Skip filler, chit-chat, repeated points, and pleasantries.\n\
- No editorializing, no summarizing importance, no meta-commentary.\n\
- Do not invent labels beyond the four section headings.\n\
- Do not give the output a title — the title is generated separately.\n\
\n\
Example shape (illustrative only, do not copy the content):\n\
## Summary\n\
The team agreed to ship v2 onboarding on Friday. QA gets the full week for regression. Dev cuts the release branch tonight; Priya drafts the launch email by Wednesday.\n\
\n\
## Decisions\n\
- Ship v2 onboarding on Friday.\n\
- Hold the redesigned empty state for v2.1 — not a launch blocker.\n\
\n\
## Action items\n\
- [ ] Cut the release branch tonight.\n\
- [ ] Draft the launch email. `— Wed`\n\
- [ ] Publish the updated dark-mode docs.\n\
\n\
## Open questions\n\
- Announce in-app, or just over email?";

    #[test]
    fn default_template_is_byte_identical_to_legacy() {
        assert_eq!(
            build_system_prompt(&builtin_as_owned(&DEFAULT_TEMPLATE)),
            LEGACY_SYSTEM_PROMPT
        );
    }

    #[test]
    fn all_built_in_templates_build_without_panic() {
        for t in BUILT_IN_TEMPLATES {
            let prompt = build_system_prompt(&builtin_as_owned(t));
            assert!(!prompt.is_empty(), "{} produced an empty prompt", t.id);
            // Every template's headings must appear in order.
            let mut cursor = 0;
            for section in t.sections {
                let needle = format!("`## {}`", section.heading);
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
        let mut t = builtin_as_owned(&STANDUP_TEMPLATE); // no template_example
        assert!(!build_system_prompt(&t).contains("Example shape"));
        t.sections[0].example = Some("- Shipped the login flow.".to_string());
        let prompt = build_system_prompt(&t);
        assert!(prompt.contains("Example shape (illustrative only, do not copy the content):"));
        assert!(prompt.contains("## Progress\n- Shipped the login flow."));
    }

    #[test]
    fn minutes_template_shape() {
        let prompt = build_system_prompt(&builtin_as_owned(&MINUTES_TEMPLATE));
        assert!(prompt.contains("up to four sections"));
        for h in ["## Agenda", "## Discussion", "## Decisions", "## Action items"] {
            assert!(prompt.contains(h), "minutes missing {h}");
        }
        // Sections from other templates must not leak in.
        assert!(!prompt.contains("## Open questions"));
        assert!(!prompt.contains("## Blockers"));
        // No illustrative example for the new templates.
        assert!(!prompt.contains("Example shape"));
    }

    #[test]
    fn standup_template_shape() {
        let prompt = build_system_prompt(&builtin_as_owned(&STANDUP_TEMPLATE));
        assert!(prompt.contains("up to three sections"));
        for h in ["## Progress", "## Plans", "## Blockers"] {
            assert!(prompt.contains(h), "standup missing {h}");
        }
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
}
