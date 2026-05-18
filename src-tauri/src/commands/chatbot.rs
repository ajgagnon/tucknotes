use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::commands::licensing::require_paid_entitlement;
use crate::errors::{lock_or_err, AppError};
use crate::models::licensing::LicensingState;
use crate::services::database::{self, DatabaseState, SearchHit};
use crate::services::model_manager;
use crate::services::summarization::{InferenceEvent, SummarizationState, ToolCallSpec};

const CHAT_SYSTEM_PROMPT: &str = "\
You are Tuck, an assistant embedded in a meeting-notes app. Help the user think \
about, recall, and act on their meetings.\n\
\n\
You have a search_meetings tool that performs full-text search across the \
user's past meeting transcripts and summaries. Use it whenever a question \
might benefit from content from prior meetings — by topic, name, decision, \
project, or any specific term. Prefer calling the tool over guessing.\n\
\n\
When the user is viewing a specific meeting, you'll receive its transcript below. \
Use it to ground answers about the current meeting. If the user asks something \
that spans multiple meetings or refers to past discussions, use search_meetings. \
Be concise. Use markdown (lists, headings) when it helps. Keep replies short \
unless the user asks for depth.";

const TOOL_NAME_SEARCH_MEETINGS: &str = "search_meetings";
const MAX_TOOL_ITERATIONS: usize = 3;

fn tools_json() -> String {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": TOOL_NAME_SEARCH_MEETINGS,
                "description": "Full-text search across the user's past meeting transcripts and summaries. Call this when the user asks about specific topics, decisions, names, or anything that may have been discussed in a prior meeting.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Natural-language search phrase. Words are AND'd together; the FTS index uses Porter stemming."
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 20,
                            "description": "Maximum number of hits to return. Default 8."
                        }
                    },
                    "required": ["query"]
                }
            }
        }
    ])
    .to_string()
}

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

#[derive(Clone, Serialize)]
struct ToolCallStartPayload<'a> {
    chat_id: &'a str,
    call_id: &'a str,
    name: &'a str,
}

#[derive(Clone, Serialize)]
struct ToolCallArgsDeltaPayload<'a> {
    chat_id: &'a str,
    call_id: &'a str,
    delta: &'a str,
}

#[derive(Clone, Serialize)]
struct ToolCallEndPayload<'a> {
    chat_id: &'a str,
    call_id: &'a str,
}

#[derive(Clone, Serialize)]
struct ToolResultPayload<'a> {
    chat_id: &'a str,
    call_id: &'a str,
    name: &'a str,
    hits: Vec<SearchHit>,
}

// ---------------------------------------------------------------------------
// Frontend-facing message type
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" | "assistant"
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

    // 1. Build the initial system message (with optional transcript block).
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

    // 2. Build the messages vector (will be mutated across tool-call iterations).
    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(history.len() + 1);
    messages.push(serde_json::json!({ "role": "system", "content": system_content }));
    for msg in &history {
        let role = match msg.role.as_str() {
            "user" => "user",
            "assistant" => "assistant",
            _ => continue,
        };
        messages.push(serde_json::json!({ "role": role, "content": msg.text }));
    }

    // 3. Resolve model path.
    let base_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::IoError(e.to_string()))?;
    let model_path = model_manager::resolve_llm_path(&base_dir)?.ok_or_else(|| {
        AppError::SummarizationFailed("No LLM model selected or downloaded".into())
    })?;

    // 4. Signal preemption to any running summarization.
    summ_state.llm_interrupt.store(true, Ordering::Relaxed);

    let tools = tools_json();

    // 5. Tool-call loop. Each iteration runs one inference turn; if it produced
    // tool calls, we execute them, append the results to `messages`, and loop.
    for iteration in 0..MAX_TOOL_ITERATIONS {
        if summ_state.llm_interrupt.load(Ordering::Relaxed) && iteration > 0 {
            // The first iteration clears the flag inside the service; later
            // iterations should bail if the user has hit stop.
            eprintln!("[chatbot] interrupt observed before iteration {}", iteration);
            break;
        }

        eprintln!(
            "[chatbot] iteration {} starting (messages={})",
            iteration,
            messages.len()
        );
        let messages_json = serde_json::Value::Array(messages.clone()).to_string();
        let service = Arc::clone(&summ_state.service);
        let interrupt = Arc::clone(&summ_state.llm_interrupt);
        let app_clone = app.clone();
        let chat_id_clone = chat_id.clone();
        let tools_clone = tools.clone();
        let model_path_clone = model_path.clone();

        let result = tokio::task::spawn_blocking(move || {
            service.generate_chat_with_tools(
                &model_path_clone,
                &messages_json,
                &tools_clone,
                &interrupt,
                |event| dispatch_inference_event(&app_clone, &chat_id_clone, event),
            )
        })
        .await;

        let outcome = match result {
            Ok(Ok(o)) => o,
            Ok(Err(AppError::Interrupted)) => {
                // User stopped — leave any partial reply intact.
                let _ = app.emit(
                    "chat:complete",
                    ChatCompletePayload { chat_id: &chat_id },
                );
                return Ok(());
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
                return Err(e);
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
                return Err(AppError::SummarizationFailed(msg));
            }
        };

        eprintln!(
            "[chatbot] iteration {} returned {} tool_calls",
            iteration,
            outcome.tool_calls.len()
        );

        if outcome.tool_calls.is_empty() {
            eprintln!("[chatbot] no tool calls -> emitting chat:complete");
            let _ = app.emit(
                "chat:complete",
                ChatCompletePayload { chat_id: &chat_id },
            );
            return Ok(());
        }

        // Append the assistant message recording the tool calls. The OAI
        // convention is that this message has `tool_calls` and no content.
        let tool_calls_value = serde_json::Value::Array(
            outcome
                .tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "type": "function",
                        "function": { "name": tc.name, "arguments": tc.arguments }
                    })
                })
                .collect(),
        );
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": serde_json::Value::Null,
            "tool_calls": tool_calls_value
        }));

        // Execute each tool call and append its result.
        for tc in &outcome.tool_calls {
            let (content_str, hits) = match execute_tool(&db_state, tc) {
                Ok((content, hits)) => (content, hits),
                Err(e) => (
                    serde_json::json!({ "error": e.to_string() }).to_string(),
                    Vec::new(),
                ),
            };

            let _ = app.emit(
                "chat:tool_result",
                ToolResultPayload {
                    chat_id: &chat_id,
                    call_id: &tc.id,
                    name: &tc.name,
                    hits,
                },
            );

            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": content_str
            }));
        }
    }

    // Iteration cap exhausted with pending calls — close the chat anyway so
    // any partial reply stays in the UI.
    let _ = app.emit(
        "chat:complete",
        ChatCompletePayload { chat_id: &chat_id },
    );
    Ok(())
}

#[tauri::command]
pub fn chat_stop(summ_state: tauri::State<'_, SummarizationState>) -> Result<(), AppError> {
    // The currently-running chat (if any) will see this and exit cleanly.
    summ_state.llm_interrupt.store(true, Ordering::Relaxed);
    Ok(())
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Forward one InferenceEvent from the blocking inference thread to the
/// frontend via Tauri events.
fn dispatch_inference_event(app: &tauri::AppHandle, chat_id: &str, event: &InferenceEvent<'_>) {
    match *event {
        InferenceEvent::ContentToken { text, is_thinking } => {
            let event_name = if is_thinking {
                "chat:thinking"
            } else {
                "chat:token"
            };
            let _ = app.emit(
                event_name,
                ChatTokenPayload {
                    chat_id,
                    token: text,
                },
            );
        }
        InferenceEvent::ToolCallStart { id, name } => {
            let _ = app.emit(
                "chat:tool_call_start",
                ToolCallStartPayload {
                    chat_id,
                    call_id: id,
                    name,
                },
            );
        }
        InferenceEvent::ToolCallArgsDelta { id, delta } => {
            let _ = app.emit(
                "chat:tool_call_args_delta",
                ToolCallArgsDeltaPayload {
                    chat_id,
                    call_id: id,
                    delta,
                },
            );
        }
        InferenceEvent::ToolCallEnd { id } => {
            let _ = app.emit(
                "chat:tool_call_end",
                ToolCallEndPayload {
                    chat_id,
                    call_id: id,
                },
            );
        }
    }
}

/// Execute one tool call and return (content_for_model, hits_for_ui).
/// The content string is what we feed back into the LLM context as the `tool`
/// message body; the hits vector is the structured payload for the chat UI.
fn execute_tool(
    db_state: &tauri::State<'_, DatabaseState>,
    tc: &ToolCallSpec,
) -> Result<(String, Vec<SearchHit>), AppError> {
    match tc.name.as_str() {
        TOOL_NAME_SEARCH_MEETINGS => {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or_else(|_| {
                serde_json::json!({})
            });
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let limit = args
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(8)
                .clamp(1, 20);

            let conn = lock_or_err(&db_state.conn)?;
            let hits = database::search_meetings(&conn, &query, limit)?;
            drop(conn);

            // Strip <mark> markers from the snippet before feeding to the
            // model (it doesn't need them; the UI keeps the marked version).
            let model_content = serde_json::json!({
                "query": query,
                "hits": hits.iter().map(|h| serde_json::json!({
                    "meeting_id": h.meeting_id,
                    "meeting_title": h.meeting_title,
                    "meeting_created_at": h.meeting_created_at,
                    "kind": h.kind,
                    "snippet": h.snippet.replace("<mark>", "").replace("</mark>", ""),
                })).collect::<Vec<_>>()
            })
            .to_string();
            Ok((model_content, hits))
        }
        other => {
            let msg = format!("Unknown tool: {other}");
            Ok((
                serde_json::json!({ "error": msg }).to_string(),
                Vec::new(),
            ))
        }
    }
}
