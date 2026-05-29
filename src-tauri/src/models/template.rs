//! Owned, serializable summary-template model.
//!
//! The built-in templates live in `services::templates` as `'static`
//! constants — they are the immutable "reset source". *This* model is the
//! owned counterpart that gets persisted to `templates.json`, edited by the
//! user, and exchanged with the frontend editor. Resolution at summarization
//! time always produces an [`OwnedTemplate`], whether it came from the user's
//! file or was converted from a built-in seed.
//!
//! See [`crate::services::template_store`] for load/save/CRUD and
//! [`crate::services::templates::build_system_prompt`] for how an
//! `OwnedTemplate` is assembled into the system prompt.

use serde::{Deserialize, Serialize};

/// One composable section of a summary template, in owned form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OwnedSection {
    /// Stable within a template; used for reordering and as a React key.
    pub id: String,
    /// The `##` heading text, without the leading `"## "` (e.g. `"Action items"`).
    pub heading: String,
    /// User-editable instructions — the text that follows `- ## {heading} — `
    /// in the assembled "Section bodies:" block.
    pub description: String,
    /// Optional per-section example, shown verbatim in the prompt's
    /// "Example shape" block under a `## {heading}` line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
}

/// An owned summary template: name + description + ordered sections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OwnedTemplate {
    /// Stable identifier persisted in `meetings.template` and exchanged with
    /// the frontend. Built-ins keep their fixed ids (`"default"`, `"minutes"`,
    /// …); user templates get a slug derived from their name.
    pub id: String,
    /// Human-readable label shown in the picker / settings UI.
    pub name: String,
    /// One-line description for the picker / settings UI.
    pub description: String,
    /// Ordered sections that make up the output.
    pub sections: Vec<OwnedSection>,
    /// `true` for templates that ship with the app. Built-ins can be edited
    /// and reset, but never deleted.
    #[serde(default)]
    pub builtin: bool,
    /// Built-in-seed-only frozen example block. The shipped Default template
    /// carries its combined "Example shape" here so its assembled prompt stays
    /// byte-identical to the legacy prompt. Not surfaced in the editor; user
    /// templates use per-section [`OwnedSection::example`] instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_example: Option<String>,
}

/// Persisted set of templates (built-in overrides + user-created), stored as
/// `templates.json`. Built-ins that the user has never edited are *not*
/// written here — they keep tracking the code seeds.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TemplateStore {
    #[serde(default)]
    pub templates: Vec<OwnedTemplate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_template_roundtrips() {
        let t = OwnedTemplate {
            id: "my_template".into(),
            name: "My Template".into(),
            description: "A custom template.".into(),
            sections: vec![
                OwnedSection {
                    id: "s1".into(),
                    heading: "Summary".into(),
                    description: "Two sentences.".into(),
                    example: None,
                },
                OwnedSection {
                    id: "s2".into(),
                    heading: "Notes".into(),
                    description: "Bullet points.".into(),
                    example: Some("- A note.".into()),
                },
            ],
            builtin: false,
            template_example: None,
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: OwnedTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn section_example_is_optional_and_omitted_when_none() {
        let json = r#"{"id":"s","heading":"H","description":"D"}"#;
        let s: OwnedSection = serde_json::from_str(json).unwrap();
        assert_eq!(s.example, None);
        // None serializes without the key.
        let out = serde_json::to_string(&s).unwrap();
        assert!(!out.contains("example"), "got: {out}");
    }

    #[test]
    fn template_builtin_defaults_false_when_missing() {
        let json = r#"{"id":"x","name":"X","description":"","sections":[]}"#;
        let t: OwnedTemplate = serde_json::from_str(json).unwrap();
        assert!(!t.builtin);
        assert_eq!(t.template_example, None);
    }

    #[test]
    fn store_defaults_to_empty() {
        let store: TemplateStore = serde_json::from_str("{}").unwrap();
        assert!(store.templates.is_empty());
        assert_eq!(TemplateStore::default().templates.len(), 0);
    }
}
