use std::sync::Arc;

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::errors::{lock_or_err, AppError};
use crate::models::llm::LlmModel;
use crate::models::{Model, ModelInfo};
use crate::services::database::{self, DatabaseState};
use crate::services::model_manager;
use crate::services::summarization::SummarizationState;

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
    meeting_id: String,
) -> Result<String, AppError> {
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

    // We're the active summarization — run it
    match do_summarize(&app, &*db_state, &*summ_state, &meeting_id).await {
        Ok(_) => Ok("started".into()),
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
    // 1. Load meeting transcript from DB
    let transcript = {
        let conn = lock_or_err(&db_state.conn)?;
        let (_, segments, _) = database::get_meeting_with_segments(&conn, meeting_id)?;
        segments
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
            .join("\n")
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

    // 3. Run summarization with streaming (events scoped to meeting_id)
    let service = Arc::clone(&summ_state.service);
    let app_clone = app.clone();
    let model_path_clone = model_path.clone();
    let mid_owned = meeting_id.to_owned();

    let summary = tokio::task::spawn_blocking(move || {
        service.summarize(&model_path, &transcript, |token, is_thinking| {
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
        database::set_minutes_body(&conn, meeting_id, &summary)?;
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
                let _ = app_title.emit(
                    "summary:title",
                    TitlePayload {
                        meeting_id: &mid_for_title,
                        title: title.as_str(),
                    },
                );
                if let Some(db) = app_title.try_state::<DatabaseState>() {
                    if let Ok(conn) = db.conn.lock() {
                        let _ = database::update_meeting_title(&conn, &mid_for_title, title);
                    }
                }
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
        if let Err(e) = do_summarize(&app, &*db_state, &*summ_state, &next_id).await {
            eprintln!("[queue] Summarization failed for {}: {:?}", next_id, e);
            // Clear active and try next
            if let Ok(mut active) = summ_state.active_meeting_id.lock() {
                *active = None;
            }
            process_next_in_queue(&app);
        }
        // On success, do_summarize spawns title gen which will
        // clear active_meeting_id and call process_next_in_queue when done.
    });
}

