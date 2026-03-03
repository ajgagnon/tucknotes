use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::errors::{lock_or_err, AppError};
use crate::models::llm::{LlmModel, LlmModelInfo};
use crate::services::database::{self, DatabaseState};
use crate::services::model_manager;
use crate::services::summarization::SummarizationState;

#[tauri::command]
pub fn list_available_llm_models() -> Vec<LlmModelInfo> {
    model_manager::list_llm_models()
}

#[tauri::command]
pub fn get_llm_model_status(app: tauri::AppHandle, model_id: String) -> Result<bool, AppError> {
    let model =
        LlmModel::from_id(&model_id).ok_or_else(|| AppError::InvalidModel(model_id))?;
    model_manager::is_llm_model_downloaded(&app, &model)
}

#[tauri::command]
pub async fn download_llm_model(app: tauri::AppHandle, model_id: String) -> Result<(), AppError> {
    let model =
        LlmModel::from_id(&model_id).ok_or_else(|| AppError::InvalidModel(model_id))?;
    model_manager::download_llm_model(&app, &model).await
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
pub async fn update_meeting_title(
    db_state: tauri::State<'_, DatabaseState>,
    meeting_id: String,
    title: String,
) -> Result<(), AppError> {
    let conn = lock_or_err(&db_state.conn)?;
    database::update_meeting_title(&conn, &meeting_id, &title)
}

#[tauri::command]
pub async fn summarize_meeting(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DatabaseState>,
    summ_state: tauri::State<'_, SummarizationState>,
    meeting_id: String,
) -> Result<String, AppError> {
    // 1. Load meeting transcript from DB
    let transcript = {
        let conn = lock_or_err(&db_state.conn)?;
        let (_, segments) = database::get_meeting_with_segments(&conn, &meeting_id)?;
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
    let model_path = model_manager::resolve_llm_model_path(&base_dir)?.ok_or_else(|| {
        AppError::SummarizationFailed("No LLM model selected or downloaded".into())
    })?;

    // 3. Run summarization with streaming
    let service = Arc::clone(&summ_state.service);
    let app_clone = app.clone();
    let model_path_clone = model_path.clone();

    let summary = tokio::task::spawn_blocking(move || {
        service.summarize(&model_path, &transcript, |token, is_thinking| {
            if is_thinking {
                let _ = app_clone.emit("summary:thinking", token);
            } else {
                let _ = app_clone.emit("summary:token", token);
            }
        })
    })
    .await
    .map_err(|e| AppError::SummarizationFailed(format!("Task panicked: {e}")))??;

    // 4. Persist summary to database
    {
        let conn = lock_or_err(&db_state.conn)?;
        database::update_meeting_summary(&conn, &meeting_id, &summary)?;
    }

    // 5. Signal summary completion
    let _ = app.emit("summary:complete", &meeting_id);

    // 6. Generate title from the summary (separate LLM call).
    //    Fire-and-forget — the invoke returns immediately with the summary
    //    while title generation continues in the background. The result is
    //    delivered via a "summary:title" event.
    let service = Arc::clone(&summ_state.service);
    let app_title = app.clone();
    let summary_for_title = summary.clone();
    let mid = meeting_id.clone();

    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            eprintln!("[title-gen] Starting title generation…");
            service.generate_title(&model_path_clone, &summary_for_title)
        })
        .await;

        match result {
            Ok(Ok(ref title)) if !title.is_empty() => {
                eprintln!("[title-gen] Generated title: {:?}", title);
                let _ = app_title.emit("summary:title", title.as_str());
                if let Some(db) = app_title.try_state::<DatabaseState>() {
                    if let Ok(conn) = db.conn.lock() {
                        let _ = database::update_meeting_title(&conn, &mid, title);
                    }
                }
            }
            Ok(Ok(ref title)) => {
                eprintln!("[title-gen] Title was empty after processing: {:?}", title);
                // Signal done even on empty so frontend can reset loading state
                let _ = app_title.emit("summary:title", "");
            }
            Ok(Err(ref e)) => {
                eprintln!("[title-gen] Title generation failed: {:?}", e);
                let _ = app_title.emit("summary:title", "");
            }
            Err(ref e) => {
                eprintln!("[title-gen] Title gen task panicked: {:?}", e);
                let _ = app_title.emit("summary:title", "");
            }
        }
    });

    Ok(summary)
}
