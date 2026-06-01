//! Persistence + CRUD for user-editable summary templates.
//!
//! Templates are stored in `templates.json` in the app data dir, written with
//! the same atomic tmp-then-rename pattern as
//! [`crate::services::model_manager::save_settings_to`].
//!
//! The file holds only *overrides*: edited built-ins (keyed by their fixed id)
//! and user-created templates. Built-ins the user has never touched are NOT
//! written — they keep tracking the code seeds in
//! [`crate::services::templates`]. Resolution overlays the stored file on top
//! of [`templates::builtin_seeds`], so the merged view is always:
//! built-ins (with overrides applied) first, then user templates in store order.
//!
//! As with `model_manager`, the core functions take a plain `&Path` so they can
//! be unit-tested without a Tauri runtime; thin `AppHandle` wrappers live at the
//! bottom.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::errors::AppError;
use crate::models::template::{OwnedTemplate, TemplateStore};
use crate::services::templates;

fn resolve_data_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::IoError(e.to_string()))
}

// ---------------------------------------------------------------------------
// Raw store file (overrides only)
// ---------------------------------------------------------------------------

/// Read `templates.json`. Returns an empty store if the file doesn't exist yet.
pub fn load_store_from(base_dir: &Path) -> Result<TemplateStore, AppError> {
    let path = base_dir.join("templates.json");
    if !path.exists() {
        return Ok(TemplateStore::default());
    }
    let contents = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&contents)?)
}

/// Persist `store` to `<base_dir>/templates.json` atomically.
pub fn save_store_to(base_dir: &Path, store: &TemplateStore) -> Result<(), AppError> {
    if !base_dir.exists() {
        std::fs::create_dir_all(base_dir)?;
    }
    let path = base_dir.join("templates.json");
    let tmp_path = base_dir.join("templates.json.tmp");
    let json = serde_json::to_string_pretty(store)?;
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Resolved view (seeds overlaid with overrides)
// ---------------------------------------------------------------------------

/// The full, ordered list of templates: built-in seeds (with any stored
/// override applied) first, then user-created templates in store order.
pub fn list_resolved(base_dir: &Path) -> Result<Vec<OwnedTemplate>, AppError> {
    let store = load_store_from(base_dir)?;
    let mut resolved = templates::builtin_seeds();

    // Apply overrides to built-ins; collect user templates separately.
    let mut user: Vec<OwnedTemplate> = Vec::new();
    for stored in store.templates {
        match resolved.iter_mut().find(|t| t.id == stored.id) {
            Some(seed) => {
                // Edited built-in: replace, but keep the builtin flag honest.
                *seed = OwnedTemplate {
                    builtin: true,
                    ..stored
                };
            }
            None => user.push(OwnedTemplate {
                builtin: false,
                ..stored
            }),
        }
    }
    resolved.extend(user);
    Ok(resolved)
}

/// Resolve a single template by id, or `None` if unknown.
pub fn get_resolved(base_dir: &Path, id: &str) -> Result<Option<OwnedTemplate>, AppError> {
    Ok(list_resolved(base_dir)?.into_iter().find(|t| t.id == id))
}

/// Resolve the template to summarize with. `None` or an unknown id falls back
/// to the Default template (matching the `meetings.template IS NULL` → default
/// convention and degrading gracefully when a referenced template was deleted).
pub fn resolve_owned(base_dir: &Path, id: Option<&str>) -> Result<OwnedTemplate, AppError> {
    let list = list_resolved(base_dir)?;
    let chosen = id
        .and_then(|id| list.iter().find(|t| t.id == id).cloned())
        .or_else(|| list.iter().find(|t| t.id == "default").cloned());
    chosen.ok_or_else(|| {
        AppError::SummarizationFailed("No Default template available".into())
    })
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Validate a template's user-editable fields. Returns the trimmed template.
fn validate(mut t: OwnedTemplate) -> Result<OwnedTemplate, AppError> {
    t.name = t.name.trim().to_string();
    t.description = t.description.trim().to_string();
    if t.name.is_empty() {
        return Err(AppError::InvalidTemplate("Template name is required".into()));
    }
    if t.sections.is_empty() {
        return Err(AppError::InvalidTemplate(
            "A template needs at least one section".into(),
        ));
    }
    for s in &mut t.sections {
        s.heading = s.heading.trim().to_string();
        s.description = s.description.trim().to_string();
        if let Some(ex) = &s.example {
            let ex = ex.trim();
            s.example = if ex.is_empty() { None } else { Some(ex.to_string()) };
        }
        if s.heading.is_empty() {
            return Err(AppError::InvalidTemplate(
                "Every section needs a heading".into(),
            ));
        }
        if s.description.is_empty() {
            return Err(AppError::InvalidTemplate(format!(
                "Section \"{}\" needs instructions",
                s.heading
            )));
        }
    }
    Ok(t)
}

/// Derive a unique, url-ish slug id from a name, avoiding `taken` ids.
fn generate_id(name: &str, taken: &[String]) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            slug.push('_');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('_').to_string();
    let base = if slug.is_empty() { "template".to_string() } else { slug };

    if !taken.iter().any(|t| t == &base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Create a new user template, assigning a fresh id. Returns the stored
/// template (with its assigned id and `builtin: false`).
pub fn create(base_dir: &Path, template: OwnedTemplate) -> Result<OwnedTemplate, AppError> {
    let mut store = load_store_from(base_dir)?;
    // Reserve built-in ids and any id already in the store.
    let mut taken: Vec<String> = templates::builtin_seeds().into_iter().map(|t| t.id).collect();
    taken.extend(store.templates.iter().map(|t| t.id.clone()));

    let mut t = validate(template)?;
    t.id = generate_id(&t.name, &taken);
    t.builtin = false;

    store.templates.push(t.clone());
    save_store_to(base_dir, &store)?;
    Ok(t)
}

/// Update an existing template (built-in override or user template) by id.
pub fn update(base_dir: &Path, template: OwnedTemplate) -> Result<(), AppError> {
    let mut store = load_store_from(base_dir)?;
    let builtin = templates::is_builtin_id(&template.id);

    // The target must already exist as a resolvable template.
    if !builtin && !store.templates.iter().any(|t| t.id == template.id) {
        return Err(AppError::InvalidTemplate(format!(
            "Unknown template \"{}\"",
            template.id
        )));
    }

    let mut t = validate(template)?;
    // A built-in's id stays fixed (reserved) and it never becomes a user
    // template, but its sections — including their examples — are fully editable.
    t.builtin = builtin;

    match store.templates.iter_mut().find(|s| s.id == t.id) {
        Some(existing) => *existing = t,
        None => store.templates.push(t), // first edit of a built-in
    }
    save_store_to(base_dir, &store)?;
    Ok(())
}

/// Delete a user template. Built-ins cannot be deleted (use [`reset`]).
pub fn delete(base_dir: &Path, id: &str) -> Result<(), AppError> {
    if templates::is_builtin_id(id) {
        return Err(AppError::InvalidTemplate(
            "Built-in templates can't be deleted — reset them instead".into(),
        ));
    }
    let mut store = load_store_from(base_dir)?;
    let before = store.templates.len();
    store.templates.retain(|t| t.id != id);
    if store.templates.len() == before {
        return Err(AppError::InvalidTemplate(format!("Unknown template \"{id}\"")));
    }
    save_store_to(base_dir, &store)?;
    Ok(())
}

/// Reset a built-in template to its shipped seed by dropping any stored
/// override. Returns the seed. Errors for non-built-in ids.
pub fn reset(base_dir: &Path, id: &str) -> Result<OwnedTemplate, AppError> {
    let seed = templates::builtin_seed_by_id(id).ok_or_else(|| {
        AppError::InvalidTemplate(format!("\"{id}\" is not a built-in template"))
    })?;
    let mut store = load_store_from(base_dir)?;
    store.templates.retain(|t| t.id != id);
    save_store_to(base_dir, &store)?;
    Ok(seed)
}

// ---------------------------------------------------------------------------
// AppHandle wrappers
// ---------------------------------------------------------------------------

pub fn list_resolved_app(app: &AppHandle) -> Result<Vec<OwnedTemplate>, AppError> {
    list_resolved(&resolve_data_dir(app)?)
}

pub fn get_resolved_app(app: &AppHandle, id: &str) -> Result<Option<OwnedTemplate>, AppError> {
    get_resolved(&resolve_data_dir(app)?, id)
}

pub fn create_app(app: &AppHandle, template: OwnedTemplate) -> Result<OwnedTemplate, AppError> {
    create(&resolve_data_dir(app)?, template)
}

pub fn update_app(app: &AppHandle, template: OwnedTemplate) -> Result<(), AppError> {
    update(&resolve_data_dir(app)?, template)
}

pub fn delete_app(app: &AppHandle, id: &str) -> Result<(), AppError> {
    delete(&resolve_data_dir(app)?, id)
}

pub fn reset_app(app: &AppHandle, id: &str) -> Result<OwnedTemplate, AppError> {
    reset(&resolve_data_dir(app)?, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::template::OwnedSection;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir()
                .join(format!("tucknotes_tmpl_test_{}_{id}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample(name: &str) -> OwnedTemplate {
        OwnedTemplate {
            id: String::new(),
            name: name.into(),
            description: "desc".into(),
            sections: vec![OwnedSection {
                id: "s1".into(),
                heading: "Notes".into(),
                description: "Bullet points.".into(),
                example: Some("- a note".into()),
            }],
            builtin: false,
        }
    }

    #[test]
    fn list_resolved_returns_seeds_when_no_file() {
        let dir = TempDir::new();
        let list = list_resolved(dir.path()).unwrap();
        let ids: Vec<&str> = list.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["default", "minutes", "one_on_one", "standup"]);
        assert!(list.iter().all(|t| t.builtin));
    }

    #[test]
    fn create_assigns_slug_and_appends_after_builtins() {
        let dir = TempDir::new();
        let created = create(dir.path(), sample("My Notes!")).unwrap();
        assert_eq!(created.id, "my_notes");
        assert!(!created.builtin);

        let list = list_resolved(dir.path()).unwrap();
        assert_eq!(list.len(), 5);
        assert_eq!(list.last().unwrap().id, "my_notes");
    }

    #[test]
    fn create_avoids_builtin_and_existing_id_collisions() {
        let dir = TempDir::new();
        // "Default" slugifies to "default", which is reserved → bumped.
        let a = create(dir.path(), sample("Default")).unwrap();
        assert_eq!(a.id, "default_2");
        let b = create(dir.path(), sample("Default")).unwrap();
        assert_eq!(b.id, "default_3");
    }

    #[test]
    fn editing_builtin_stores_override_and_keeps_id() {
        let dir = TempDir::new();
        let mut standup = templates::builtin_seed_by_id("standup").unwrap();
        standup.sections[0].description = "Edited progress instructions.".into();
        update(dir.path(), standup).unwrap();

        let resolved = get_resolved(dir.path(), "standup").unwrap().unwrap();
        assert!(resolved.builtin);
        assert_eq!(resolved.id, "standup");
        assert_eq!(resolved.sections[0].description, "Edited progress instructions.");
        // Still listed in built-in position (first four), not appended.
        let list = list_resolved(dir.path()).unwrap();
        assert_eq!(list.len(), 4);
    }

    #[test]
    fn reset_restores_seed() {
        let dir = TempDir::new();
        let mut standup = templates::builtin_seed_by_id("standup").unwrap();
        standup.sections[0].description = "Edited.".into();
        update(dir.path(), standup).unwrap();

        let seed = reset(dir.path(), "standup").unwrap();
        assert_eq!(seed, templates::builtin_seed_by_id("standup").unwrap());
        let resolved = get_resolved(dir.path(), "standup").unwrap().unwrap();
        assert_eq!(resolved, templates::builtin_seed_by_id("standup").unwrap());
    }

    #[test]
    fn delete_removes_user_template_and_rejects_builtin() {
        let dir = TempDir::new();
        let created = create(dir.path(), sample("Temp")).unwrap();
        delete(dir.path(), &created.id).unwrap();
        assert!(get_resolved(dir.path(), &created.id).unwrap().is_none());

        assert!(delete(dir.path(), "default").is_err());
    }

    #[test]
    fn resolve_owned_falls_back_to_default_for_unknown_or_none() {
        let dir = TempDir::new();
        assert_eq!(resolve_owned(dir.path(), None).unwrap().id, "default");
        assert_eq!(resolve_owned(dir.path(), Some("ghost")).unwrap().id, "default");
        assert_eq!(resolve_owned(dir.path(), Some("minutes")).unwrap().id, "minutes");
    }

    #[test]
    fn validate_rejects_empty_sections_and_headings() {
        let dir = TempDir::new();
        let mut bad = sample("X");
        bad.sections.clear();
        assert!(create(dir.path(), bad).is_err());

        let mut bad2 = sample("Y");
        bad2.sections[0].description = "   ".into();
        assert!(create(dir.path(), bad2).is_err());
    }

    #[test]
    fn editing_builtin_persists_per_section_example() {
        let dir = TempDir::new();
        let mut default = templates::builtin_seed_by_id("default").unwrap();
        // The Recap seed now ships editable per-section examples.
        assert!(default.sections[0].example.is_some());
        // The user edits one section's example; it must round-trip.
        default.sections[0].example = Some("- A fresh example.".into());
        update(dir.path(), default).unwrap();
        let resolved = get_resolved(dir.path(), "default").unwrap().unwrap();
        assert_eq!(
            resolved.sections[0].example.as_deref(),
            Some("- A fresh example.")
        );
        assert!(resolved.builtin);
    }
}
