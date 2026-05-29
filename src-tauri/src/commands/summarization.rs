use std::sync::Arc;

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::commands::licensing::require_paid_entitlement;
use crate::errors::{lock_or_err, AppError};
use crate::models::licensing::LicensingState;
use crate::models::llm::LlmModel;
use crate::models::template::OwnedTemplate;
use crate::models::{Model, ModelInfo};
use crate::services::database::{self, DatabaseState};
use crate::services::model_manager;
use crate::services::summarization::SummarizationState;
use crate::services::templates::TemplateInfo;
use crate::services::template_store;

// ---------------------------------------------------------------------------
// Event payloads — scoped to a specific meeting ID
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
struct TokenPayload<'a> {
    meeting_id: &'a str,
    token: &'a str,
}

#[derive(Clone, Serialize)]
struct TitlePayload<'a> {
    meeting_id: &'a str,
    title: &'a str,
}

// ---------------------------------------------------------------------------
// LLM model management commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_available_llm_models() -> Vec<ModelInfo<LlmModel>> {
    model_manager::list_llm_models()
}

#[tauri::command]
pub fn get_llm_model_status(app: tauri::AppHandle, model_id: String) -> Result<bool, AppError> {
    let model =
        LlmModel::from_id(&model_id).ok_or_else(|| AppError::InvalidModel(model_id))?;
    model_manager::is_downloaded(&app, &model)
}

#[tauri::command]
pub async fn download_llm_model(app: tauri::AppHandle, model_id: String) -> Result<(), AppError> {
    let model =
        LlmModel::from_id(&model_id).ok_or_else(|| AppError::InvalidModel(model_id))?;
    model_manager::download(&app, &model).await
}

#[tauri::command]
pub fn get_selected_llm_model(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
    let settings = model_manager::load_settings(&app)?;
    Ok(settings.selected_llm_model.map(|m| m.id().to_owned()))
}

#[tauri::command]
pub fn set_selected_llm_model(app: tauri::AppHandle, model_id: String) -> Result<(), AppError> {
    let model =
        LlmModel::from_id(&model_id).ok_or_else(|| AppError::InvalidModel(model_id))?;
    let mut settings = model_manager::load_settings(&app)?;
    settings.selected_llm_model = Some(model);
    model_manager::save_settings(&app, &settings)
}

// ---------------------------------------------------------------------------
// Summary template commands
// ---------------------------------------------------------------------------

/// All summary templates (built-ins + user-created), for the picker / settings
/// list. Lightweight — section content is fetched per-template for the editor.
#[tauri::command]
pub fn list_summary_templates(app: tauri::AppHandle) -> Result<Vec<TemplateInfo>, AppError> {
    Ok(template_store::list_resolved_app(&app)?
        .into_iter()
        .map(|t| TemplateInfo {
            id: t.id,
            name: t.name,
            description: t.description,
            builtin: t.builtin,
        })
        .collect())
}

/// Full content of one template, for the editor.
#[tauri::command]
pub fn get_summary_template(
    app: tauri::AppHandle,
    id: String,
) -> Result<OwnedTemplate, AppError> {
    template_store::get_resolved_app(&app, &id)?
        .ok_or_else(|| AppError::InvalidTemplate(format!("Unknown template \"{id}\"")))
}

/// Create a new user template; returns it with its assigned id.
#[tauri::command]
pub fn create_summary_template(
    app: tauri::AppHandle,
    template: OwnedTemplate,
) -> Result<OwnedTemplate, AppError> {
    template_store::create_app(&app, template)
}

/// Update an existing template (built-in override or user template).
#[tauri::command]
pub fn update_summary_template(
    app: tauri::AppHandle,
    template: OwnedTemplate,
) -> Result<(), AppError> {
    template_store::update_app(&app, template)
}

/// Reset a built-in template back to its shipped defaults; returns the seed.
#[tauri::command]
pub fn reset_summary_template(
    app: tauri::AppHandle,
    id: String,
) -> Result<OwnedTemplate, AppError> {
    template_store::reset_app(&app, &id)
}

/// Delete a user template. Clears it from the app-wide default if it was set.
#[tauri::command]
pub fn delete_summary_template(app: tauri::AppHandle, id: String) -> Result<(), AppError> {
    template_store::delete_app(&app, &id)?;
    let mut settings = model_manager::load_settings(&app)?;
    if settings.default_template.as_deref() == Some(id.as_str()) {
        settings.default_template = None;
        model_manager::save_settings(&app, &settings)?;
    }
    Ok(())
}

/// The app-wide default template id (`None` = Default).
#[tauri::command]
pub fn get_default_template(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
    let settings = model_manager::load_settings(&app)?;
    Ok(settings.default_template)
}

#[tauri::command]
pub fn set_default_template(app: tauri::AppHandle, template: String) -> Result<(), AppError> {
    let mut settings = model_manager::load_settings(&app)?;
    settings.default_template = Some(template);
    model_manager::save_settings(&app, &settings)
}

#[tauri::command]
pub fn remove_llm_model(app: tauri::AppHandle, model_id: String) -> Result<(), AppError> {
    let model =
        LlmModel::from_id(&model_id).ok_or_else(|| AppError::InvalidModel(model_id))?;
    model_manager::remove_llm_model(&app, &model)
}

#[tauri::command]
pub fn get_llm_model_file_path(
    app: tauri::AppHandle,
    model_id: String,
) -> Result<Option<String>, AppError> {
    model_manager::llm_model_file_path(&app, &model_id)
}

#[tauri::command]
pub async fn update_meeting_title(
    db_state: tauri::State<'_, DatabaseState>,
    meeting_id: String,
    title: String,
) -> Result<(), AppError> {
    let conn = lock_or_err(&db_state.conn)?;
    database::update_meeting_title(&conn, &meeting_id, &title)
}

// ---------------------------------------------------------------------------
// Summarization queue commands
// ---------------------------------------------------------------------------

/// Returns the queue state: which meeting is active + which are pending.
#[derive(Clone, Serialize)]
pub struct SummarizationQueue {
    pub active: Option<String>,
    pub pending: Vec<String>,
}

#[tauri::command]
pub fn get_summarization_queue(
    summ_state: tauri::State<'_, SummarizationState>,
) -> Result<SummarizationQueue, AppError> {
    let active = lock_or_err(&summ_state.active_meeting_id)?;
    let pending = lock_or_err(&summ_state.pending_queue)?;
    Ok(SummarizationQueue {
        active: active.clone(),
        pending: pending.iter().cloned().collect(),
    })
}

/// Start or enqueue a summarization. Returns `"started"` if it began
/// immediately, or `"queued"` if another summarization is in progress.
#[tauri::command]
pub async fn summarize_meeting(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DatabaseState>,
    summ_state: tauri::State<'_, SummarizationState>,
    licensing: tauri::State<'_, LicensingState>,
    meeting_id: String,
    template: Option<String>,
) -> Result<String, AppError> {
    require_paid_entitlement(&licensing)?;

    // Persist the chosen template up front so it's the single source of truth:
    // both this run and any queued run resolve it from the DB in do_summarize.
    {
        let conn = lock_or_err(&db_state.conn)?;
        database::set_meeting_template(&conn, &meeting_id, template.as_deref())?;
    }

    // Single lock scope: check for duplicates and either start or enqueue atomically.
    {
        let mut active = lock_or_err(&summ_state.active_meeting_id)?;
        let mut queue = lock_or_err(&summ_state.pending_queue)?;

        if active.as_deref() == Some(&meeting_id) || queue.iter().any(|id| id == &meeting_id) {
            return Err(AppError::SummarizationFailed(
                "This meeting is already being summarized or queued".into(),
            ));
        }

        if active.is_some() {
            queue.push_back(meeting_id);
            return Ok("queued".into());
        }
        *active = Some(meeting_id.clone());
    }

    let _ = app.emit("summary:started", &meeting_id);

    // We're the active summarization — run it
    match do_summarize(&app, &*db_state, &*summ_state, &meeting_id).await {
        Ok(_) => Ok("started".into()),
        Err(AppError::Interrupted) => {
            // Chat preempted us. Re-queue at the front so we resume after
            // chat releases the model lock.
            if let Ok(mut active) = summ_state.active_meeting_id.lock() {
                *active = None;
            }
            if let Ok(mut queue) = summ_state.pending_queue.lock() {
                queue.push_front(meeting_id);
            }
            process_next_in_queue(&app);
            Ok("preempted".into())
        }
        Err(e) => {
            // Clear active and try to process queue
            if let Ok(mut active) = summ_state.active_meeting_id.lock() {
                *active = None;
            }
            process_next_in_queue(&app);
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Internal summarization pipeline
// ---------------------------------------------------------------------------

async fn do_summarize(
    app: &tauri::AppHandle,
    db_state: &DatabaseState,
    summ_state: &SummarizationState,
    meeting_id: &str,
) -> Result<String, AppError> {
    // 1. Load meeting transcript + selected template from DB
    let (transcript, template_id) = {
        let conn = lock_or_err(&db_state.conn)?;
        let (meeting, segments, _) = database::get_meeting_with_segments(&conn, meeting_id)?;
        let transcript = segments
            .iter()
            .map(|s| {
                let speaker = if s.source == "system" {
                    "Speaker"
                } else {
                    "You"
                };
                format!("{speaker}: {}", s.text)
            })
            .collect::<Vec<_>>()
            .join("\n");
        (transcript, meeting.template)
    };

    if transcript.is_empty() {
        return Err(AppError::SummarizationFailed(
            "No transcript to summarize".into(),
        ));
    }

    // 2. Resolve model path
    let base_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::IoError(e.to_string()))?;
    let model_path = model_manager::resolve_llm_path(&base_dir)?.ok_or_else(|| {
        AppError::SummarizationFailed("No LLM model selected or downloaded".into())
    })?;

    // Resolve the template (NULL / unknown / deleted → Default).
    let template = template_store::resolve_owned(&base_dir, template_id.as_deref())?;

    // 3. Run summarization with streaming (events scoped to meeting_id).
    // The interrupt flag is shared with the chatbot; if chat sets it, we exit
    // early with AppError::Interrupted and the caller re-queues this meeting.
    let service = Arc::clone(&summ_state.service);
    let interrupt = Arc::clone(&summ_state.llm_interrupt);
    let app_clone = app.clone();
    let model_path_clone = model_path.clone();
    let mid_owned = meeting_id.to_owned();

    let summary = tokio::task::spawn_blocking(move || {
        service.summarize(&model_path, &transcript, &template, &interrupt, |token, is_thinking| {
            let event_name = if is_thinking {
                "summary:thinking"
            } else {
                "summary:token"
            };
            let _ = app_clone.emit(
                event_name,
                TokenPayload {
                    meeting_id: &mid_owned,
                    token,
                },
            );
        })
    })
    .await
    .map_err(|e| AppError::SummarizationFailed(format!("Task panicked: {e}")))??;

    // 4. Persist summary to database
    {
        let conn = lock_or_err(&db_state.conn)?;
        database::set_summary_body(&conn, meeting_id, &summary)?;
    }

    // 5. Signal summary completion (scoped to meeting_id)
    let _ = app.emit("summary:complete", meeting_id);

    // 6. Generate title in background, then process next queued item
    let service = Arc::clone(&summ_state.service);
    let app_title = app.clone();
    let summary_for_title = summary.clone();
    let mid = meeting_id.to_owned();

    tokio::spawn(async move {
        let mid_for_title = mid.clone();
        let result = tokio::task::spawn_blocking(move || {
            eprintln!("[title-gen] Starting title generation…");
            service.generate_title(&model_path_clone, &summary_for_title)
        })
        .await;

        match result {
            Ok(Ok(ref title)) if !title.is_empty() => {
                eprintln!("[title-gen] Generated title: {:?}", title);
                let final_title = app_title
                    .try_state::<DatabaseState>()
                    .and_then(|db| {
                        let conn = db.conn.lock().ok()?;
                        let existing = database::get_meeting_title(&conn, &mid_for_title)
                            .ok()
                            .flatten();
                        let combined = match existing.as_deref().map(str::trim) {
                            Some(t) if !t.is_empty() && t != "Recording" => {
                                format!("{t} — {title}")
                            }
                            _ => title.clone(),
                        };
                        let _ = database::update_meeting_title(
                            &conn,
                            &mid_for_title,
                            &combined,
                        );
                        Some(combined)
                    })
                    .unwrap_or_else(|| title.clone());
                let _ = app_title.emit(
                    "summary:title",
                    TitlePayload {
                        meeting_id: &mid_for_title,
                        title: final_title.as_str(),
                    },
                );
            }
            other => {
                match &other {
                    Ok(Ok(title)) => eprintln!("[title-gen] Title was empty after processing: {:?}", title),
                    Ok(Err(e)) => eprintln!("[title-gen] Title generation failed: {:?}", e),
                    Err(e) => eprintln!("[title-gen] Title gen task panicked: {:?}", e),
                }
                let _ = app_title.emit(
                    "summary:title",
                    TitlePayload {
                        meeting_id: &mid_for_title,
                        title: "",
                    },
                );
            }
        }

        // Clear active meeting and process next queued item
        if let Some(state) = app_title.try_state::<SummarizationState>() {
            if let Ok(mut active) = state.active_meeting_id.lock() {
                *active = None;
            }
        }
        process_next_in_queue(&app_title);
    });

    Ok(summary)
}

/// Pop the next meeting from the queue and kick off its summarization.
/// Runs as a fire-and-forget background task.
fn process_next_in_queue(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<SummarizationState>() else {
        return;
    };

    let next_id = {
        let Ok(mut queue) = state.pending_queue.lock() else {
            return;
        };
        queue.pop_front()
    };

    let Some(next_id) = next_id else {
        return;
    };

    // Set as active before spawning
    if let Ok(mut active) = state.active_meeting_id.lock() {
        *active = Some(next_id.clone());
    }

    let _ = app.emit("summary:started", &next_id);

    let app = app.clone();
    tokio::spawn(async move {
        // We need DatabaseState and SummarizationState from managed state.
        // Use try_state since we're in a spawned task.
        let Some(db_state) = app.try_state::<DatabaseState>() else {
            eprintln!("[queue] Failed to get DatabaseState");
            if let Some(s) = app.try_state::<SummarizationState>() {
                if let Ok(mut active) = s.active_meeting_id.lock() {
                    *active = None;
                }
            }
            process_next_in_queue(&app);
            return;
        };
        let Some(summ_state) = app.try_state::<SummarizationState>() else {
            eprintln!("[queue] Failed to get SummarizationState");
            return;
        };

        eprintln!("[queue] Processing next meeting: {}", next_id);
        match do_summarize(&app, &*db_state, &*summ_state, &next_id).await {
            Ok(_) => {
                // do_summarize spawns title gen which clears
                // active_meeting_id and calls process_next_in_queue.
            }
            Err(AppError::Interrupted) => {
                eprintln!("[queue] Summarization preempted for {}, re-queueing", next_id);
                if let Ok(mut active) = summ_state.active_meeting_id.lock() {
                    *active = None;
                }
                if let Ok(mut queue) = summ_state.pending_queue.lock() {
                    queue.push_front(next_id);
                }
                process_next_in_queue(&app);
            }
            Err(e) => {
                eprintln!("[queue] Summarization failed for {}: {:?}", next_id, e);
                if let Ok(mut active) = summ_state.active_meeting_id.lock() {
                    *active = None;
                }
                process_next_in_queue(&app);
            }
        }
    });
}

