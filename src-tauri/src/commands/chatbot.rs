use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::commands::licensing::require_paid_entitlement;
use crate::errors::{lock_or_err, AppError};
use crate::models::licensing::LicensingState;
use crate::services::database::{self, DatabaseState};
use crate::services::model_manager;
use crate::services::summarization::SummarizationState;

const CHAT_SYSTEM_PROMPT: &str = "\
You are Tuck, an assistant embedded in a meeting-notes app. Help the user think \
about, recall, and act on their meetings.\n\
\n\
When the user is viewing a specific meeting, you'll receive its transcript below. \
Use it to ground answers. If the user asks something the transcript doesn't speak \
to, draw on your general knowledge — say what you're sure of and flag what you're \
not. Be concise. Use markdown (lists, headings) when it helps. Keep replies short \
unless the user asks for depth.";

// ---------------------------------------------------------------------------
// Event payloads — scoped to a specific chat turn
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
struct ChatTokenPayload<'a> {
    chat_id: &'a str,
    token: &'a str,
}

#[derive(Clone, Serialize)]
struct ChatCompletePayload<'a> {
    chat_id: &'a str,
}

#[derive(Clone, Serialize)]
struct ChatErrorPayload<'a> {
    chat_id: &'a str,
    error: &'a str,
}

// ---------------------------------------------------------------------------
// Frontend-facing message type
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,   // "user" | "assistant"
    pub text: String,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn chat_send_message(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DatabaseState>,
    summ_state: tauri::State<'_, SummarizationState>,
    licensing: tauri::State<'_, LicensingState>,
    chat_id: String,
    meeting_id: Option<String>,
    history: Vec<ChatMessage>,
) -> Result<(), AppError> {
    require_paid_entitlement(&licensing)?;

    // 1. Build system message (with optional transcript block).
    let system_content = match meeting_id.as_deref() {
        Some(mid) => {
            let conn = lock_or_err(&db_state.conn)?;
            let (meeting, segments, _) = database::get_meeting_with_segments(&conn, mid)?;
            drop(conn);

            let transcript = segments
                .iter()
                .map(|s| {
                    let speaker = if s.source == "system" { "Speaker" } else { "You" };
                    format!("{speaker}: {}", s.text)
                })
                .collect::<Vec<_>>()
                .join("\n");

            if transcript.is_empty() {
                CHAT_SYSTEM_PROMPT.to_string()
            } else {
                let title = meeting.title.as_deref().unwrap_or("Untitled");
                format!(
                    "{CHAT_SYSTEM_PROMPT}\n\nActive meeting: {title}\n\nTranscript:\n{transcript}"
                )
            }
        }
        None => CHAT_SYSTEM_PROMPT.to_string(),
    };

    // 2. Build OpenAI-format messages array: system + history.
    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(history.len() + 1);
    messages.push(serde_json::json!({ "role": "system", "content": system_content }));
    for msg in &history {
        // Defensive: only forward known roles.
        let role = match msg.role.as_str() {
            "user" => "user",
            "assistant" => "assistant",
            _ => continue,
        };
        messages.push(serde_json::json!({ "role": role, "content": msg.text }));
    }
    let messages_json = serde_json::Value::Array(messages).to_string();

    // 3. Resolve model path.
    let base_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::IoError(e.to_string()))?;
    let model_path = model_manager::resolve_llm_path(&base_dir)?.ok_or_else(|| {
        AppError::SummarizationFailed("No LLM model selected or downloaded".into())
    })?;

    // 4. Signal preemption to any running summarization, then run.
    summ_state
        .llm_interrupt
        .store(true, Ordering::Relaxed);

    let service = Arc::clone(&summ_state.service);
    let interrupt = Arc::clone(&summ_state.llm_interrupt);
    let app_clone = app.clone();
    let chat_id_clone = chat_id.clone();

    let result = tokio::task::spawn_blocking(move || {
        service.generate_chat(
            &model_path,
            &messages_json,
            &interrupt,
            |token, is_thinking| {
                let event_name = if is_thinking {
                    "chat:thinking"
                } else {
                    "chat:token"
                };
                let _ = app_clone.emit(
                    event_name,
                    ChatTokenPayload {
                        chat_id: &chat_id_clone,
                        token,
                    },
                );
            },
        )
    })
    .await;

    match result {
        Ok(Ok(_)) | Ok(Err(AppError::Interrupted)) => {
            // Treat user-stop as a graceful end so the frontend keeps the partial reply.
            let _ = app.emit(
                "chat:complete",
                ChatCompletePayload { chat_id: &chat_id },
            );
            Ok(())
        }
        Ok(Err(e)) => {
            let msg = e.to_string();
            let _ = app.emit(
                "chat:error",
                ChatErrorPayload {
                    chat_id: &chat_id,
                    error: &msg,
                },
            );
            Err(e)
        }
        Err(e) => {
            let msg = format!("Task panicked: {e}");
            let _ = app.emit(
                "chat:error",
                ChatErrorPayload {
                    chat_id: &chat_id,
                    error: &msg,
                },
            );
            Err(AppError::SummarizationFailed(msg))
        }
    }
}

#[tauri::command]
pub fn chat_stop(summ_state: tauri::State<'_, SummarizationState>) -> Result<(), AppError> {
    // The currently-running chat (if any) will see this and exit cleanly.
    summ_state.llm_interrupt.store(true, Ordering::Relaxed);
    Ok(())
}
