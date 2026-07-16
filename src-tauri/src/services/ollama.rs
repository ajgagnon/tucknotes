//! HTTP client for a user-managed local Ollama server (native `/api/chat`).
//!
//! Mirrors the semantics of the in-process llama.cpp path in
//! `summarization.rs`: greedy sampling (`temperature: 0`), thinking disabled,
//! streamed content filtered through [`StreamFilter`], cooperative
//! cancellation via the shared `AtomicBool`, and tool calls recovered from
//! Ollama's structured `tool_calls` chunks with [`extract_tool_calls`] as the
//! fallback for models whose templates emit inline tags instead.
//!
//! The blocking entry points ([`chat_text`], [`chat_with_tools`]) substitute
//! for llama.cpp inference and are called from the same `spawn_blocking`
//! threads. Internally they drive async reqwest via `Handle::block_on` and
//! poll `interrupt` every [`INTERRUPT_POLL`] so Stop stays responsive even
//! while Ollama silently loads a model into memory (which can take minutes on
//! first request — hence a generous idle-chunk deadline instead of a total
//! request timeout).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::errors::AppError;
use crate::services::summarization::{
    extract_tool_calls, InferenceEvent, StreamFilter, ToolCallSpec, TurnOutcome,
};

pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

/// `keep_alive` for live-minutes/gist passes: a quiet stretch mid-recording
/// longer than Ollama's 5-minute default would unload the model and make the
/// next pass pay a reload. Bounded (never `-1`) so we don't pin the user's
/// RAM after the recording ends.
pub const KEEP_ALIVE_RECORDING: &str = "30m";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Total budget for the detection ping — onboarding fires it on mount, so it
/// must fail fast when nothing is listening.
const DETECT_TIMEOUT: Duration = Duration::from_millis(1500);
/// How long the stream may go without a single chunk before we give up. Must
/// exceed a cold model load (Ollama sends nothing while loading).
const IDLE_CHUNK_TIMEOUT: Duration = Duration::from_secs(180);
/// Interrupt-flag polling cadence while waiting on the stream. Bounds Stop
/// latency in every phase: connecting, model load, and mid-generation.
const INTERRUPT_POLL: Duration = Duration::from_millis(100);
/// Coarse `num_ctx` buckets. Changing `num_ctx` makes Ollama reload the model
/// runner, so fine-grained per-request sizing would thrash reloads between
/// chat and summarize passes. The floor matches `MIN_TOOL_CHAT_CTX` in
/// `summarization.rs`; Ollama's own default (4096) would silently truncate
/// meeting transcripts.
const CTX_BUCKETS: &[u32] = &[16384, 32768, 65536];

// ---------------------------------------------------------------------------
// Detection / model listing (async — called directly from Tauri commands)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct OllamaStatus {
    pub reachable: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub size_bytes: u64,
    pub parameter_size: Option<String>,
    pub quantization: Option<String>,
    pub family: Option<String>,
}

/// Probe `GET /api/version`. Never errors — an unreachable server is a normal
/// outcome (Ollama not installed / not running), reported as `reachable: false`.
pub async fn detect(base_url: &str) -> OllamaStatus {
    #[derive(Deserialize)]
    struct VersionResponse {
        version: String,
    }
    let url = format!("{}/api/version", normalize_base_url(base_url));
    let resp = http_client().get(&url).timeout(DETECT_TIMEOUT).send().await;
    match resp {
        Ok(resp) if resp.status().is_success() => {
            let version = resp.json::<VersionResponse>().await.ok().map(|v| v.version);
            OllamaStatus {
                reachable: true,
                version,
            }
        }
        _ => OllamaStatus {
            reachable: false,
            version: None,
        },
    }
}

/// List installed models via `GET /api/tags`.
pub async fn list_models(base_url: &str) -> Result<Vec<OllamaModelInfo>, AppError> {
    #[derive(Deserialize)]
    struct TagsResponse {
        #[serde(default)]
        models: Vec<TagModel>,
    }
    #[derive(Deserialize)]
    struct TagModel {
        name: String,
        #[serde(default)]
        size: u64,
        #[serde(default)]
        details: Option<TagDetails>,
    }
    #[derive(Deserialize)]
    struct TagDetails {
        #[serde(default)]
        parameter_size: Option<String>,
        #[serde(default)]
        quantization_level: Option<String>,
        #[serde(default)]
        family: Option<String>,
    }

    let base = normalize_base_url(base_url);
    let url = format!("{base}/api/tags");
    let resp = http_client()
        .get(&url)
        .timeout(DETECT_TIMEOUT)
        .send()
        .await
        .map_err(|_| unreachable_error(&base))?;
    if !resp.status().is_success() {
        return Err(AppError::SummarizationFailed(format!(
            "Ollama returned {} when listing models",
            resp.status()
        )));
    }
    let tags: TagsResponse = resp
        .json()
        .await
        .map_err(|e| AppError::SummarizationFailed(format!("Invalid /api/tags response: {e}")))?;
    Ok(tags
        .models
        .into_iter()
        .map(|m| {
            let details = m.details.unwrap_or(TagDetails {
                parameter_size: None,
                quantization_level: None,
                family: None,
            });
            OllamaModelInfo {
                name: m.name,
                size_bytes: m.size,
                parameter_size: details.parameter_size,
                quantization: details.quantization_level,
                family: details.family,
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Chat completion (blocking — called from spawn_blocking, like llama.cpp)
// ---------------------------------------------------------------------------

/// Options for a plain-text generation pass ([`chat_text`]).
pub struct TextOptions<'a> {
    pub max_tokens: i32,
    /// Extra stop sequences (Ollama `options.stop`).
    pub stop: Option<Vec<String>>,
    /// Ollama `keep_alive` override (e.g. `"30m"` during recording so quiet
    /// stretches don't unload the model). `None` respects the server default.
    pub keep_alive: Option<&'a str>,
}

/// Text-only chat completion: the Ollama counterpart of `run_inference` for
/// the summarize / minutes / gist / title / plain-chat paths.
///
/// `on_token(text, is_thinking)` receives content with literal
/// `<think>`/`<tool_call>` blocks stripped (as the local path does); the
/// returned string is the raw accumulated content so existing callers'
/// `strip_think_tags` post-processing behaves identically.
///
/// **Blocking** — call from `spawn_blocking`.
pub fn chat_text<F>(
    base_url: &str,
    model: &str,
    messages_json: &str,
    opts: &TextOptions<'_>,
    interrupt: &AtomicBool,
    mut on_token: F,
) -> Result<String, AppError>
where
    F: FnMut(&str, bool),
{
    let params = ChatRequestParams {
        base_url,
        model,
        messages_json,
        tools_json: None,
        max_tokens: opts.max_tokens,
        stop: opts.stop.clone(),
        keep_alive: opts.keep_alive,
    };
    let mut filter = StreamFilter::new();
    let outcome = run_chat(&params, interrupt, &mut |delta| match delta {
        Delta::Content(text) => {
            filter.push(text, &mut |e| {
                if let InferenceEvent::ContentToken { text, is_thinking } = e {
                    on_token(text, *is_thinking);
                }
            });
        }
        Delta::Thinking(text) => on_token(text, true),
    })?;
    filter.flush(&mut |e| {
        if let InferenceEvent::ContentToken { text, is_thinking } = e {
            on_token(text, *is_thinking);
        }
    });
    Ok(outcome.content)
}

/// Tool-aware chat completion: the Ollama counterpart of
/// `generate_chat_with_tools`. Streams [`InferenceEvent`]s and returns the
/// turn's tool calls — structured `tool_calls` from the stream when the model
/// supports them, otherwise recovered from inline tags in the generated text.
///
/// **Blocking** — call from `spawn_blocking`.
pub fn chat_with_tools<F>(
    base_url: &str,
    model: &str,
    messages_json: &str,
    tools_json: Option<&str>,
    max_tokens: i32,
    interrupt: &AtomicBool,
    mut on_event: F,
) -> Result<TurnOutcome, AppError>
where
    F: FnMut(&InferenceEvent<'_>),
{
    let params = ChatRequestParams {
        base_url,
        model,
        messages_json,
        tools_json,
        max_tokens,
        stop: None,
        keep_alive: None,
    };
    let mut filter = StreamFilter::new();
    let outcome = run_chat(&params, interrupt, &mut |delta| match delta {
        Delta::Content(text) => filter.push(text, &mut |e| on_event(e)),
        Delta::Thinking(text) => on_event(&InferenceEvent::ContentToken {
            text,
            is_thinking: true,
        }),
    })?;
    filter.flush(&mut |e| on_event(e));

    // Prefer structured calls from the stream; fall back to inline-tag
    // extraction for models whose Ollama template lacks tool support but that
    // emit Qwen/Gemma/code-fence style calls in plain content anyway.
    let tool_calls = if outcome.structured_tool_calls.is_empty() {
        extract_tool_calls(&outcome.content)
    } else {
        outcome.structured_tool_calls
    };

    // Same post-hoc event synthesis as the local path: start, full args as one
    // delta, end.
    for tc in &tool_calls {
        on_event(&InferenceEvent::ToolCallStart {
            id: &tc.id,
            name: &tc.name,
        });
        if !tc.arguments.is_empty() {
            on_event(&InferenceEvent::ToolCallArgsDelta {
                id: &tc.id,
                delta: &tc.arguments,
            });
        }
        on_event(&InferenceEvent::ToolCallEnd { id: &tc.id });
    }

    Ok(TurnOutcome {
        tool_calls,
        prompt_tokens: outcome.prompt_eval_count,
        completion_tokens: outcome.eval_count,
        max_tokens: outcome.num_ctx,
    })
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            // No total timeout: generation streams for as long as it needs.
            .build()
            .expect("failed to build reqwest client")
    })
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

fn unreachable_error(base_url: &str) -> AppError {
    AppError::OllamaUnavailable(format!(
        "Can't reach Ollama at {base_url}. Start Ollama, or switch the summarization engine in Settings."
    ))
}

struct ChatRequestParams<'a> {
    base_url: &'a str,
    model: &'a str,
    messages_json: &'a str,
    tools_json: Option<&'a str>,
    max_tokens: i32,
    stop: Option<Vec<String>>,
    keep_alive: Option<&'a str>,
}

enum Delta<'a> {
    Content(&'a str),
    Thinking(&'a str),
}

struct StreamOutcome {
    /// Accumulated raw assistant content (unfiltered).
    content: String,
    structured_tool_calls: Vec<ToolCallSpec>,
    prompt_eval_count: u32,
    eval_count: u32,
    num_ctx: u32,
    done_reason: Option<String>,
}

/// One streamed NDJSON line from `/api/chat`.
#[derive(Deserialize)]
struct ChatChunk {
    #[serde(default)]
    message: Option<ChunkMessage>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct ChunkMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ChunkToolCall>>,
}

#[derive(Deserialize)]
struct ChunkToolCall {
    function: ChunkToolFunction,
}

#[derive(Deserialize)]
struct ChunkToolFunction {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

/// ~Bytes per token for prose transcripts. Slightly conservative so the
/// estimate errs toward a bigger context; the JSON overhead in what we measure
/// adds further headroom.
fn estimate_tokens(text: &str) -> u32 {
    ((text.len() as f64) / 3.5).ceil() as u32
}

/// Pick the `num_ctx` bucket that fits the estimated prompt (plus an eighth of
/// slack for estimation error), the generation budget, and a fixed margin.
/// Requests beyond the largest bucket get the largest bucket — the truncation
/// check on the done chunk logs when that actually bit.
fn compute_num_ctx(est_prompt_tokens: u32, max_tokens: i32) -> u32 {
    let needed = est_prompt_tokens + est_prompt_tokens / 8 + max_tokens.max(0) as u32 + 256;
    for &bucket in CTX_BUCKETS {
        if needed <= bucket {
            return bucket;
        }
    }
    *CTX_BUCKETS.last().expect("CTX_BUCKETS non-empty")
}

/// Convert OpenAI-format messages (what the app assembles everywhere) into
/// Ollama's native `/api/chat` shape:
///  - assistant `tool_calls[].function.arguments` JSON-string → object, and
///    the OpenAI-only `id`/`type` fields dropped;
///  - `tool` messages: `tool_call_id` → `tool_name` (resolved via the
///    preceding assistant message's calls);
///  - `content: null` → `""`.
fn normalize_messages(messages_json: &str) -> Result<serde_json::Value, AppError> {
    let mut messages: serde_json::Value = serde_json::from_str(messages_json)
        .map_err(|e| AppError::SummarizationFailed(format!("Invalid messages JSON: {e}")))?;
    let arr = messages.as_array_mut().ok_or_else(|| {
        AppError::SummarizationFailed("Messages JSON must be an array".to_string())
    })?;

    let mut id_to_name: HashMap<String, String> = HashMap::new();
    for msg in arr.iter_mut() {
        let Some(obj) = msg.as_object_mut() else {
            continue;
        };
        if obj.get("content").is_some_and(serde_json::Value::is_null) {
            obj.insert("content".to_string(), json!(""));
        }
        let role = obj
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or_default()
            .to_string();
        if role == "assistant" {
            if let Some(tool_calls) = obj.get_mut("tool_calls").and_then(|t| t.as_array_mut()) {
                for tc in tool_calls.iter_mut() {
                    let Some(tc_obj) = tc.as_object_mut() else {
                        continue;
                    };
                    let id = tc_obj
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    if let Some(func) = tc_obj.get_mut("function").and_then(|f| f.as_object_mut()) {
                        if let (Some(id), Some(name)) = (
                            id,
                            func.get("name")
                                .and_then(|v| v.as_str())
                                .map(str::to_string),
                        ) {
                            id_to_name.insert(id, name);
                        }
                        if let Some(args_str) = func
                            .get("arguments")
                            .and_then(|a| a.as_str())
                            .map(str::to_string)
                        {
                            let parsed: serde_json::Value =
                                serde_json::from_str(&args_str).unwrap_or_else(|_| json!({}));
                            func.insert("arguments".to_string(), parsed);
                        }
                    }
                    tc_obj.remove("id");
                    tc_obj.remove("type");
                }
            }
        } else if role == "tool" {
            let id = obj
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            obj.remove("tool_call_id");
            if let Some(name) = id.and_then(|id| id_to_name.get(&id).cloned()) {
                obj.insert("tool_name".to_string(), json!(name));
            }
        }
    }
    Ok(messages)
}

fn build_request_body(
    p: &ChatRequestParams<'_>,
    num_ctx: u32,
    include_think: bool,
    include_tools: bool,
) -> Result<serde_json::Value, AppError> {
    let messages = normalize_messages(p.messages_json)?;
    let mut options = json!({
        "num_ctx": num_ctx,
        "num_predict": p.max_tokens,
        // Greedy, matching the deterministic local sampler.
        "temperature": 0.0,
    });
    if let Some(stop) = &p.stop {
        if !stop.is_empty() {
            options["stop"] = json!(stop);
        }
    }
    let mut body = json!({
        "model": p.model,
        "messages": messages,
        "stream": true,
        "options": options,
    });
    if include_think {
        // Parity with `enable_thinking: false` on every local path. Old
        // servers that reject the field are handled by the retry ladder.
        body["think"] = json!(false);
    }
    if include_tools {
        if let Some(tools) = p.tools_json {
            body["tools"] = serde_json::from_str(tools)
                .map_err(|e| AppError::SummarizationFailed(format!("Invalid tools JSON: {e}")))?;
        }
    }
    if let Some(keep_alive) = p.keep_alive {
        body["keep_alive"] = json!(keep_alive);
    }
    Ok(body)
}

/// Extract Ollama's `{"error": "..."}` body, falling back to the raw text.
fn parse_error_body(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or_else(|| text.trim().to_string())
}

/// Run a future to completion from a blocking context. Inside the app this is
/// a `spawn_blocking` thread with the Tauri runtime's handle available; unit
/// tests without a runtime get a throwaway current-thread one.
fn run_blocking<F, T>(fut: F) -> Result<T, AppError>
where
    F: std::future::Future<Output = Result<T, AppError>>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.block_on(fut),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| AppError::SummarizationFailed(format!("Tokio runtime: {e}")))?
            .block_on(fut),
    }
}

/// Shared request/stream loop behind [`chat_text`] and [`chat_with_tools`].
fn run_chat(
    p: &ChatRequestParams<'_>,
    interrupt: &AtomicBool,
    on_delta: &mut dyn FnMut(Delta<'_>),
) -> Result<StreamOutcome, AppError> {
    // A pre-set flag means we were preempted before starting (summarize passes
    // don't clear it) — bail before any network work, like the local prefill
    // loop does.
    if interrupt.load(Ordering::Relaxed) {
        return Err(AppError::Interrupted);
    }

    let base = normalize_base_url(p.base_url);
    let url = format!("{base}/api/chat");
    let est_prompt_tokens = estimate_tokens(p.messages_json);
    let num_ctx = compute_num_ctx(est_prompt_tokens, p.max_tokens);
    eprintln!(
        "[ollama] chat request: model={} est_prompt_tokens={} num_ctx={} num_predict={} tools={}",
        p.model,
        est_prompt_tokens,
        num_ctx,
        p.max_tokens,
        p.tools_json.is_some(),
    );

    run_blocking(async move {
        // Retry ladder: drop `think`, then `tools`, when a 400 names them —
        // covers pre-0.9 servers and models without thinking/tool support.
        let mut include_think = true;
        let mut include_tools = p.tools_json.is_some();
        let resp = loop {
            let body = build_request_body(p, num_ctx, include_think, include_tools)?;
            let resp = http_client()
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|_| unreachable_error(&base))?;
            let status = resp.status();
            if status.is_success() {
                break resp;
            }
            let err_msg = parse_error_body(&resp.text().await.unwrap_or_default());
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(AppError::InvalidModel(format!(
                    "Ollama can't find model \"{}\" — pick another model in Settings. ({err_msg})",
                    p.model
                )));
            }
            if status == reqwest::StatusCode::BAD_REQUEST {
                let lower = err_msg.to_lowercase();
                if include_think && lower.contains("think") {
                    eprintln!("[ollama] server rejected think field, retrying without: {err_msg}");
                    include_think = false;
                    continue;
                }
                if include_tools && lower.contains("tool") {
                    eprintln!("[ollama] server rejected tools, retrying without: {err_msg}");
                    include_tools = false;
                    continue;
                }
            }
            return Err(AppError::SummarizationFailed(format!(
                "Ollama error ({status}): {err_msg}"
            )));
        };

        // Stream NDJSON lines, polling `interrupt` between waits. Dropping the
        // stream on interrupt closes the connection, which cancels the request
        // server-side.
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut outcome = StreamOutcome {
            content: String::new(),
            structured_tool_calls: Vec::new(),
            prompt_eval_count: 0,
            eval_count: 0,
            num_ctx,
            done_reason: None,
        };
        let mut idle_deadline = tokio::time::Instant::now() + IDLE_CHUNK_TIMEOUT;
        'stream: loop {
            if interrupt.load(Ordering::Relaxed) {
                return Err(AppError::Interrupted);
            }
            let polled = tokio::select! {
                chunk = stream.next() => Some(chunk),
                () = tokio::time::sleep(INTERRUPT_POLL) => None,
            };
            let Some(chunk) = polled else {
                if tokio::time::Instant::now() >= idle_deadline {
                    return Err(AppError::SummarizationFailed(format!(
                        "Ollama stopped responding (no data for {}s)",
                        IDLE_CHUNK_TIMEOUT.as_secs()
                    )));
                }
                continue;
            };
            let Some(chunk) = chunk else {
                break 'stream; // EOF
            };
            let bytes = chunk
                .map_err(|e| AppError::SummarizationFailed(format!("Ollama stream error: {e}")))?;
            idle_deadline = tokio::time::Instant::now() + IDLE_CHUNK_TIMEOUT;
            buf.extend_from_slice(&bytes);

            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = &line[..line.len() - 1];
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                let parsed: ChatChunk = serde_json::from_slice(line).map_err(|e| {
                    AppError::SummarizationFailed(format!("Invalid Ollama stream chunk: {e}"))
                })?;
                if let Some(err) = parsed.error {
                    return Err(AppError::SummarizationFailed(format!(
                        "Ollama error: {err}"
                    )));
                }
                if let Some(message) = parsed.message {
                    if let Some(thinking) = message.thinking.as_deref() {
                        if !thinking.is_empty() {
                            on_delta(Delta::Thinking(thinking));
                        }
                    }
                    if let Some(content) = message.content.as_deref() {
                        if !content.is_empty() {
                            outcome.content.push_str(content);
                            on_delta(Delta::Content(content));
                        }
                    }
                    for tc in message.tool_calls.unwrap_or_default() {
                        let id = format!("call_{}", outcome.structured_tool_calls.len());
                        let arguments = if tc.function.arguments.is_null() {
                            "{}".to_string()
                        } else {
                            tc.function.arguments.to_string()
                        };
                        outcome.structured_tool_calls.push(ToolCallSpec {
                            id,
                            name: tc.function.name,
                            arguments,
                        });
                    }
                }
                if parsed.done {
                    outcome.prompt_eval_count = parsed.prompt_eval_count.unwrap_or(0);
                    outcome.eval_count = parsed.eval_count.unwrap_or(0);
                    outcome.done_reason = parsed.done_reason;
                    break 'stream;
                }
            }
        }

        // A prompt that filled the context minus the generation budget was
        // almost certainly truncated by Ollama (it trims from the front).
        let budget = p.max_tokens.max(0) as u32;
        if outcome.prompt_eval_count > 0
            && outcome.prompt_eval_count >= num_ctx.saturating_sub(budget)
        {
            eprintln!(
                "[ollama] prompt likely truncated: prompt_eval_count={} num_ctx={} num_predict={}",
                outcome.prompt_eval_count, num_ctx, budget
            );
        }
        eprintln!(
            "[ollama] chat done: prompt_eval={} eval={} done_reason={:?} content_len={} structured_tool_calls={}",
            outcome.prompt_eval_count,
            outcome.eval_count,
            outcome.done_reason,
            outcome.content.len(),
            outcome.structured_tool_calls.len(),
        );
        Ok(outcome)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn no_events() -> impl FnMut(&InferenceEvent<'_>) {
        |_| {}
    }

    fn text_opts(max_tokens: i32) -> TextOptions<'static> {
        TextOptions {
            max_tokens,
            stop: None,
            keep_alive: None,
        }
    }

    fn ndjson(lines: &[serde_json::Value]) -> String {
        lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    fn chunk(content: &str) -> serde_json::Value {
        serde_json::json!({"message": {"role": "assistant", "content": content}, "done": false})
    }

    fn done_chunk(prompt_eval: u32, eval: u32) -> serde_json::Value {
        serde_json::json!({
            "message": {"role": "assistant", "content": ""},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": prompt_eval,
            "eval_count": eval,
        })
    }

    /// Production shape: a multi-thread tokio runtime (like Tauri's) with the
    /// sync client called from `spawn_blocking`, exercising the
    /// `Handle::try_current` + `block_on` path rather than the test-only
    /// fallback runtime the plain `#[test]`s hit.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn chat_text_works_from_spawn_blocking_under_runtime() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/api/chat");
                then.status(200)
                    .body(ndjson(&[chunk("hi"), done_chunk(3, 1)]));
            })
            .await;

        let base = server.base_url();
        let raw = tokio::task::spawn_blocking(move || {
            let interrupt = AtomicBool::new(false);
            chat_text(
                &base,
                "m",
                r#"[{"role":"user","content":"q"}]"#,
                &text_opts(16),
                &interrupt,
                |_, _| {},
            )
        })
        .await
        .unwrap()
        .unwrap();

        assert_eq!(raw, "hi");
    }

    #[test]
    fn chat_text_streams_content_and_sets_options() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/api/chat").json_body_partial(
                serde_json::json!({
                    "model": "qwen3:4b",
                    "stream": true,
                    "think": false,
                    "options": {"num_ctx": 16384, "num_predict": 1024, "temperature": 0.0}
                })
                .to_string(),
            );
            then.status(200)
                .header("content-type", "application/x-ndjson")
                .body(ndjson(&[
                    chunk("Hello "),
                    chunk("world"),
                    done_chunk(12, 2),
                ]));
        });

        let interrupt = AtomicBool::new(false);
        let mut streamed = String::new();
        let messages = r#"[{"role":"user","content":"hi"}]"#;
        let raw = chat_text(
            &server.base_url(),
            "qwen3:4b",
            messages,
            &text_opts(1024),
            &interrupt,
            |t, _thinking| streamed.push_str(t),
        )
        .unwrap();

        mock.assert();
        assert_eq!(raw, "Hello world");
        assert_eq!(streamed, "Hello world");
    }

    #[test]
    fn chat_text_strips_inline_think_tags_from_stream_but_not_raw() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200).body(ndjson(&[
                chunk("<think>pondering"),
                chunk("</think>answer"),
                done_chunk(10, 5),
            ]));
        });

        let interrupt = AtomicBool::new(false);
        let mut visible = String::new();
        let raw = chat_text(
            &server.base_url(),
            "m",
            r#"[{"role":"user","content":"q"}]"#,
            &text_opts(64),
            &interrupt,
            |t, is_thinking| {
                if !is_thinking {
                    visible.push_str(t);
                }
            },
        )
        .unwrap();

        assert_eq!(visible, "answer");
        assert!(raw.contains("<think>"), "raw keeps tags for caller cleanup");
    }

    #[test]
    fn chat_text_routes_thinking_field_to_thinking_tokens() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200).body(ndjson(&[
                serde_json::json!({"message": {"role": "assistant", "thinking": "hmm"}, "done": false}),
                chunk("answer"),
                done_chunk(10, 5),
            ]));
        });

        let interrupt = AtomicBool::new(false);
        let mut visible = String::new();
        let mut thinking = String::new();
        let raw = chat_text(
            &server.base_url(),
            "m",
            r#"[{"role":"user","content":"q"}]"#,
            &text_opts(64),
            &interrupt,
            |t, is_thinking| {
                if is_thinking {
                    thinking.push_str(t);
                } else {
                    visible.push_str(t);
                }
            },
        )
        .unwrap();

        assert_eq!(visible, "answer");
        assert_eq!(thinking, "hmm");
        assert_eq!(raw, "answer", "thinking field stays out of raw content");
    }

    #[test]
    fn chat_with_tools_collects_structured_calls() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200).body(ndjson(&[
                serde_json::json!({
                    "message": {
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [
                            {"function": {"name": "search_meetings", "arguments": {"query": "roadmap", "limit": 5}}}
                        ]
                    },
                    "done": false
                }),
                done_chunk(100, 20),
            ]));
        });

        let interrupt = AtomicBool::new(false);
        let mut starts = Vec::new();
        let outcome = chat_with_tools(
            &server.base_url(),
            "qwen3:4b",
            r#"[{"role":"user","content":"find roadmap"}]"#,
            Some(r#"[{"type":"function","function":{"name":"search_meetings","parameters":{}}}]"#),
            4096,
            &interrupt,
            |e| {
                if let InferenceEvent::ToolCallStart { name, .. } = e {
                    starts.push(name.to_string());
                }
            },
        )
        .unwrap();

        assert_eq!(outcome.tool_calls.len(), 1);
        assert_eq!(outcome.tool_calls[0].name, "search_meetings");
        let args: serde_json::Value =
            serde_json::from_str(&outcome.tool_calls[0].arguments).unwrap();
        assert_eq!(args["query"], "roadmap");
        assert_eq!(starts, vec!["search_meetings"]);
        assert_eq!(outcome.prompt_tokens, 100);
        assert_eq!(outcome.completion_tokens, 20);
        assert_eq!(outcome.max_tokens, 16384);
    }

    #[test]
    fn chat_with_tools_falls_back_to_inline_extraction() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200).body(ndjson(&[
                chunk("<tool_call>{\"name\":\"search_meetings\",\"arguments\":{\"query\":\"q3\"}}</tool_call>"),
                done_chunk(50, 30),
            ]));
        });

        let interrupt = AtomicBool::new(false);
        let outcome = chat_with_tools(
            &server.base_url(),
            "m",
            r#"[{"role":"user","content":"q"}]"#,
            None,
            4096,
            &interrupt,
            no_events(),
        )
        .unwrap();

        assert_eq!(outcome.tool_calls.len(), 1);
        assert_eq!(outcome.tool_calls[0].name, "search_meetings");
    }

    #[test]
    fn retries_without_think_when_server_rejects_it() {
        let server = MockServer::start();
        let rejected = server.mock(|when, then| {
            when.method(POST)
                .path("/api/chat")
                .json_body_partial(r#"{"think": false}"#);
            then.status(400)
                .body(r#"{"error":"\"m\" does not support thinking"}"#);
        });
        let accepted = server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200)
                .body(ndjson(&[chunk("ok"), done_chunk(5, 1)]));
        });

        let interrupt = AtomicBool::new(false);
        let raw = chat_text(
            &server.base_url(),
            "m",
            r#"[{"role":"user","content":"q"}]"#,
            &text_opts(64),
            &interrupt,
            |_, _| {},
        )
        .unwrap();

        assert_eq!(raw, "ok");
        // First-match-wins: the think-bearing attempt hit `rejected`, the
        // think-less retry hit `accepted`.
        rejected.assert();
        accepted.assert();
    }

    #[test]
    fn retries_without_tools_when_server_rejects_them() {
        let server = MockServer::start();
        let rejected = server.mock(|when, then| {
            when.method(POST)
                .path("/api/chat")
                .json_body_partial(r#"{"tools": [{"type":"function"}]}"#);
            then.status(400)
                .body(r#"{"error":"\"m\" does not support tools"}"#);
        });
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200)
                .body(ndjson(&[chunk("plain reply"), done_chunk(5, 2)]));
        });

        let interrupt = AtomicBool::new(false);
        let outcome = chat_with_tools(
            &server.base_url(),
            "m",
            r#"[{"role":"user","content":"q"}]"#,
            Some(r#"[{"type":"function","function":{"name":"search_meetings"}}]"#),
            4096,
            &interrupt,
            no_events(),
        )
        .unwrap();

        rejected.assert();
        assert!(outcome.tool_calls.is_empty());
    }

    #[test]
    fn missing_model_maps_to_invalid_model() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(404)
                .body(r#"{"error":"model \"ghost\" not found"}"#);
        });

        let interrupt = AtomicBool::new(false);
        let err = chat_text(
            &server.base_url(),
            "ghost",
            r#"[{"role":"user","content":"q"}]"#,
            &text_opts(64),
            &interrupt,
            |_, _| {},
        )
        .unwrap_err();

        assert!(matches!(err, AppError::InvalidModel(_)), "got {err:?}");
    }

    #[test]
    fn unreachable_server_maps_to_ollama_unavailable() {
        let interrupt = AtomicBool::new(false);
        // Nothing listens on this port.
        let err = chat_text(
            "http://127.0.0.1:1",
            "m",
            r#"[{"role":"user","content":"q"}]"#,
            &text_opts(64),
            &interrupt,
            |_, _| {},
        )
        .unwrap_err();

        assert!(matches!(err, AppError::OllamaUnavailable(_)), "got {err:?}");
    }

    #[test]
    fn pre_set_interrupt_returns_interrupted_without_network() {
        let interrupt = AtomicBool::new(true);
        let err = chat_text(
            "http://127.0.0.1:1",
            "m",
            r#"[{"role":"user","content":"q"}]"#,
            &text_opts(64),
            &interrupt,
            |_, _| {},
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Interrupted), "got {err:?}");
    }

    #[test]
    fn mid_stream_error_line_fails_the_turn() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200).body(ndjson(&[
                chunk("partial"),
                serde_json::json!({"error": "runner crashed"}),
            ]));
        });

        let interrupt = AtomicBool::new(false);
        let err = chat_text(
            &server.base_url(),
            "m",
            r#"[{"role":"user","content":"q"}]"#,
            &text_opts(64),
            &interrupt,
            |_, _| {},
        )
        .unwrap_err();

        assert!(
            matches!(&err, AppError::SummarizationFailed(m) if m.contains("runner crashed")),
            "got {err:?}"
        );
    }

    #[test]
    fn num_ctx_buckets_floor_and_growth() {
        // Small prompts get the floor (which satisfies MIN_TOOL_CHAT_CTX).
        assert_eq!(compute_num_ctx(100, 1024), 16384);
        assert_eq!(compute_num_ctx(10_000, 4096), 16384);
        // Growing prompts bump buckets.
        assert_eq!(compute_num_ctx(20_000, 4096), 32768);
        assert_eq!(compute_num_ctx(50_000, 4096), 65536);
        // Beyond the largest bucket, clamp (truncation is logged at runtime).
        assert_eq!(compute_num_ctx(500_000, 4096), 65536);
    }

    #[test]
    fn estimate_tokens_rounds_up() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 2); // 4 / 3.5 → 2
    }

    #[test]
    fn normalize_messages_maps_tool_plumbing() {
        let messages = serde_json::json!([
            {"role": "system", "content": "sys"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_0", "type": "function", "function": {"name": "search_meetings", "arguments": "{\"query\":\"x\"}"}}
            ]},
            {"role": "tool", "tool_call_id": "call_0", "content": "hits"}
        ])
        .to_string();

        let normalized = normalize_messages(&messages).unwrap();
        let arr = normalized.as_array().unwrap();

        let assistant = &arr[1];
        assert_eq!(assistant["content"], "");
        let tc = &assistant["tool_calls"][0];
        assert!(tc.get("id").is_none());
        assert!(tc.get("type").is_none());
        assert_eq!(tc["function"]["arguments"]["query"], "x");

        let tool = &arr[2];
        assert!(tool.get("tool_call_id").is_none());
        assert_eq!(tool["tool_name"], "search_meetings");
    }

    #[tokio::test]
    async fn detect_reports_version() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/api/version");
                then.status(200)
                    .json_body(serde_json::json!({"version": "0.9.6"}));
            })
            .await;

        let status = detect(&server.base_url()).await;
        assert!(status.reachable);
        assert_eq!(status.version.as_deref(), Some("0.9.6"));

        let missing = detect("http://127.0.0.1:1").await;
        assert!(!missing.reachable);
    }

    #[tokio::test]
    async fn list_models_parses_tags() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/api/tags");
                then.status(200).json_body(serde_json::json!({
                    "models": [
                        {"name": "qwen3:4b", "size": 2_600_000_000u64, "details": {"parameter_size": "4.0B", "quantization_level": "Q4_K_M", "family": "qwen3"}},
                        {"name": "llama3.2:latest", "size": 2_000_000_000u64}
                    ]
                }));
            })
            .await;

        let models = list_models(&server.base_url()).await.unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "qwen3:4b");
        assert_eq!(models[0].parameter_size.as_deref(), Some("4.0B"));
        assert_eq!(models[1].quantization, None);
    }

    #[tokio::test]
    async fn list_models_unreachable_maps_to_ollama_unavailable() {
        let err = list_models("http://127.0.0.1:1").await.unwrap_err();
        assert!(matches!(err, AppError::OllamaUnavailable(_)), "got {err:?}");
    }
}
