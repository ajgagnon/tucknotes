use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::openai::OpenAIChatTemplateParams;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

use crate::errors::{lock_or_err, AppError};
use crate::models::template::OwnedTemplate;
use crate::services::templates;

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

const MAX_SUMMARIZATION_TOKENS: i32 = 4096;
const MAX_CHAT_TOKENS: i32 = 1024;
/// Token budget for one tool-aware chat turn. Models tend to chain-of-thought
/// for a paragraph or two before emitting the `<tool_call>` block, then
/// produce a final answer after the tool result comes back, so we need
/// significantly more headroom than the plain-chat path.
const MAX_TOOL_CHAT_TOKENS: i32 = 4096;
const MIN_TOOL_CHAT_CTX: u32 = 16384;
const BATCH_SIZE: usize = 2048;

/// Structured events emitted during a tool-aware chat inference turn.
/// Borrowed strings so callers can copy as needed without forcing allocation.
#[derive(Debug)]
pub enum InferenceEvent<'a> {
    /// A piece of natural-language assistant content.
    ContentToken { text: &'a str, is_thinking: bool },
    /// The parser has identified the start of a tool call. Fires once per call.
    ToolCallStart { id: &'a str, name: &'a str },
    /// A fragment of the tool call's `arguments` JSON. May fire multiple times.
    ToolCallArgsDelta { id: &'a str, delta: &'a str },
    /// All tool calls collected by this turn have completed streaming.
    /// Fires once at the end of the turn for every tool call in `TurnOutcome`.
    ToolCallEnd { id: &'a str },
}

/// One tool call extracted from a completed inference turn. Arguments are kept
/// as a raw JSON string (per OpenAI conventions); callers parse to dispatch.
#[derive(Debug, Clone)]
pub struct ToolCallSpec {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// What an inference turn produced. Empty `tool_calls` means the assistant is
/// done and the natural-language reply (already streamed via `on_event`) stands.
#[derive(Debug, Default)]
pub struct TurnOutcome {
    pub tool_calls: Vec<ToolCallSpec>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub max_tokens: u32,
}

/// Wraps a lazily-loaded llama.cpp model and exposes blocking
/// `summarize()` / `generate_chat()` methods. The model is loaded on the
/// first call and reused for every subsequent call. If the model path
/// changes (e.g. user selects a different model), it is reloaded.
pub struct SummarizationService {
    backend: LlamaBackend,
    model: Mutex<Option<(PathBuf, LlamaModel)>>,
}

// SAFETY: LlamaModel is Send + Sync per llama-cpp-2 docs.
unsafe impl Send for SummarizationService {}
unsafe impl Sync for SummarizationService {}

impl SummarizationService {
    pub fn new() -> Result<Self, AppError> {
        let backend = LlamaBackend::init().map_err(|e| {
            AppError::SummarizationFailed(format!("Failed to init llama backend: {e}"))
        })?;
        Ok(Self {
            backend,
            model: Mutex::new(None),
        })
    }

    /// Load the LLM from disk if it hasn't been loaded yet, or if the
    /// model path has changed (e.g. user selected a different model).
    fn ensure_loaded(&self, model_path: &Path) -> Result<(), AppError> {
        let mut guard = lock_or_err(&self.model)?;
        if let Some((ref loaded_path, _)) = *guard {
            if loaded_path == model_path {
                return Ok(());
            }
            eprintln!(
                "[summarization] Model path changed from {:?} to {:?}, reloading",
                loaded_path, model_path
            );
        }

        let params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&self.backend, model_path, &params).map_err(
            |e| AppError::SummarizationFailed(format!("Failed to load LLM model: {e}")),
        )?;
        *guard = Some((model_path.to_path_buf(), model));
        Ok(())
    }

    /// Apply the model's built-in Jinja chat template to a pre-built
    /// OpenAI-format messages JSON string. Returns the templated prompt
    /// ready for tokenization.
    fn apply_template(
        model: &LlamaModel,
        messages_json: &str,
        enable_thinking: bool,
    ) -> Result<String, AppError> {
        let tmpl = model.chat_template(None).map_err(|e| {
            AppError::SummarizationFailed(format!("Failed to get chat template: {e}"))
        })?;

        let params = OpenAIChatTemplateParams {
            messages_json,
            tools_json: None,
            tool_choice: None,
            json_schema: None,
            grammar: None,
            reasoning_format: None,
            chat_template_kwargs: None,
            add_generation_prompt: true,
            use_jinja: true,
            parallel_tool_calls: false,
            enable_thinking,
            add_bos: false,
            add_eos: false,
            parse_tool_calls: false,
        };

        let result = model
            .apply_chat_template_oaicompat(&tmpl, &params)
            .map_err(|e| {
                AppError::SummarizationFailed(format!("Failed to apply chat template: {e}"))
            })?;

        eprintln!(
            "[summarization] Template applied (enable_thinking={enable_thinking}, \
             thinking_forced_open={})",
            result.thinking_forced_open
        );

        Ok(result.prompt)
    }

    /// Shared token-generation loop with streaming `<think>` filtering and
    /// cooperative cancellation. `on_token(text, is_thinking)` is called
    /// for each piece. Returns the raw output string (callers strip
    /// `<think>` tags for storage).
    ///
    /// **Blocking** — call from `spawn_blocking`. Returns
    /// `AppError::Interrupted` when `interrupt` is set during the loop.
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    fn run_inference<F>(
        &self,
        model: &LlamaModel,
        prompt: &str,
        max_tokens: i32,
        interrupt: &AtomicBool,
        mut on_token: F,
    ) -> Result<String, AppError>
    where
        F: FnMut(&str, bool),
    {
        let tokens_list = model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| AppError::SummarizationFailed(format!("Tokenization failed: {e}")))?;

        let n_input = tokens_list.len() as u32;
        let n_ctx = (n_input + max_tokens as u32 + 256).max(4096);

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(n_ctx))
            .with_flash_attention_policy(llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_AUTO);
        let mut ctx = model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| AppError::SummarizationFailed(format!("Context creation failed: {e}")))?;

        let mut batch = LlamaBatch::new(BATCH_SIZE, 1);

        // Feed prompt tokens in chunks to avoid one enormous batch allocation.
        let n_prompt = tokens_list.len();
        let last_prompt_idx = n_prompt as i32 - 1;
        for chunk_start in (0..n_prompt).step_by(BATCH_SIZE) {
            if interrupt.load(Ordering::Relaxed) {
                return Err(AppError::Interrupted);
            }
            batch.clear();
            let chunk_end = (chunk_start + BATCH_SIZE).min(n_prompt);
            for i in chunk_start..chunk_end {
                let is_last = i as i32 == last_prompt_idx;
                batch
                    .add(tokens_list[i], i as i32, &[0], is_last)
                    .map_err(|e| {
                        AppError::SummarizationFailed(format!("Batch add failed: {e}"))
                    })?;
            }
            ctx.decode(&mut batch)
                .map_err(|e| AppError::SummarizationFailed(format!("Decode failed: {e}")))?;
        }

        // n_cur tracks the absolute position in the context (prompt + generated)
        let mut n_cur = n_prompt as i32;
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut sampler = build_sampler();
        let mut output = String::new();

        let mut inside_think = false;
        let mut tag_buf = String::new();

        while n_cur < n_prompt as i32 + max_tokens {
            if interrupt.load(Ordering::Relaxed) {
                return Err(AppError::Interrupted);
            }

            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if token == model.token_eos() {
                break;
            }

            let piece = model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(|e| {
                    AppError::SummarizationFailed(format!("Token to string failed: {e}"))
                })?;

            output.push_str(&piece);
            eprint!("{}", piece);

            // Stream filtering: strip <think>...</think> blocks, only emit
            // answer content to the frontend.
            tag_buf.push_str(&piece);
            loop {
                if inside_think {
                    if let Some(end) = tag_buf.find(THINK_CLOSE) {
                        tag_buf = tag_buf[(end + THINK_CLOSE.len())..].to_string();
                        inside_think = false;
                    } else if tag_buf.contains('<') {
                        let lt = tag_buf.rfind('<').unwrap();
                        tag_buf = tag_buf[lt..].to_string();
                        break;
                    } else {
                        tag_buf.clear();
                        break;
                    }
                } else if let Some(start) = tag_buf.find(THINK_OPEN) {
                    let before = &tag_buf[..start];
                    if !before.is_empty() {
                        on_token(before, false);
                    }
                    tag_buf = tag_buf[start + THINK_OPEN.len()..].to_string();
                    inside_think = true;
                } else if tag_buf.contains('<') {
                    let lt = tag_buf.rfind('<').unwrap();
                    let before = &tag_buf[..lt];
                    if !before.is_empty() {
                        on_token(before, false);
                    }
                    tag_buf = tag_buf[lt..].to_string();
                    break;
                } else {
                    if !tag_buf.is_empty() {
                        on_token(&tag_buf, false);
                        tag_buf.clear();
                    }
                    break;
                }
            }

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| AppError::SummarizationFailed(format!("Batch add failed: {e}")))?;

            n_cur += 1;

            ctx.decode(&mut batch)
                .map_err(|e| AppError::SummarizationFailed(format!("Decode failed: {e}")))?;
        }

        if !tag_buf.is_empty() && !inside_think {
            on_token(&tag_buf, false);
        }

        Ok(output)
    }

    /// Run summarization on the given transcript text. See `run_inference`
    /// for streaming and cancellation semantics. **Blocking**.
    pub fn summarize<F>(
        &self,
        model_path: &Path,
        transcript: &str,
        template: &OwnedTemplate,
        interrupt: &AtomicBool,
        on_token: F,
    ) -> Result<String, AppError>
    where
        F: FnMut(&str, bool),
    {
        self.ensure_loaded(model_path)?;

        let guard = lock_or_err(&self.model)?;
        let (_, model) = guard
            .as_ref()
            .ok_or_else(|| AppError::SummarizationFailed("Model not loaded".into()))?;

        let system_prompt = templates::build_system_prompt(template);
        let messages_json = serde_json::json!([
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": transcript}
        ])
        .to_string();
        let prompt = Self::apply_template(model, &messages_json, false)?;

        let raw = self.run_inference(
            model,
            &prompt,
            MAX_SUMMARIZATION_TOKENS,
            interrupt,
            on_token,
        )?;
        let cleaned = strip_think_tags(raw.trim());
        Ok(cleaned.trim().to_string())
    }

    /// Run a chat completion against the supplied pre-built OpenAI-format
    /// `messages_json`. Streams via `on_token` and respects `interrupt`.
    /// **Blocking**.
    pub fn generate_chat<F>(
        &self,
        model_path: &Path,
        messages_json: &str,
        interrupt: &AtomicBool,
        on_token: F,
    ) -> Result<String, AppError>
    where
        F: FnMut(&str, bool),
    {
        self.ensure_loaded(model_path)?;

        let guard = lock_or_err(&self.model)?;
        let (_, model) = guard
            .as_ref()
            .ok_or_else(|| AppError::SummarizationFailed("Model not loaded".into()))?;

        // We're past the preemption hand-off: the lock is ours. Clear the
        // shared flag so we don't immediately self-abort. It remains
        // available for the stop button to signal *this* run to bail.
        interrupt.store(false, Ordering::Relaxed);

        let prompt = Self::apply_template(model, messages_json, false)?;

        let raw = self.run_inference(model, &prompt, MAX_CHAT_TOKENS, interrupt, on_token)?;
        let cleaned = strip_think_tags(raw.trim());
        Ok(cleaned.trim().to_string())
    }

    /// Run a chat completion with OpenAI-compatible tool support. Streams
    /// natural-language content via `on_event` and collects any tool calls
    /// the model emits, returning them in `TurnOutcome.tool_calls`. The
    /// caller drives the tool-execution loop: execute, append `tool` messages,
    /// and call this method again with the augmented history.
    ///
    /// **Blocking** — call from `spawn_blocking`. Respects `interrupt`.
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    pub fn generate_chat_with_tools<F>(
        &self,
        model_path: &Path,
        messages_json: &str,
        tools_json: Option<&str>,
        interrupt: &AtomicBool,
        mut on_event: F,
    ) -> Result<TurnOutcome, AppError>
    where
        F: FnMut(&InferenceEvent<'_>),
    {
        self.ensure_loaded(model_path)?;

        let guard = lock_or_err(&self.model)?;
        let (_, model) = guard
            .as_ref()
            .ok_or_else(|| AppError::SummarizationFailed("Model not loaded".into()))?;

        let max_tokens = model.n_ctx_train();

        // We have the lock; clear the shared flag so we don't immediately
        // self-abort. The stop button can re-set it to interrupt this run.
        interrupt.store(false, Ordering::Relaxed);

        // Apply the template with tools enabled. Keep the full ChatTemplateResult
        // for grammar, preserved tokens, stops, and (most importantly) the
        // response parser used after generation.
        let tmpl = model.chat_template(None).map_err(|e| {
            AppError::SummarizationFailed(format!("Failed to get chat template: {e}"))
        })?;
        let params = OpenAIChatTemplateParams {
            messages_json,
            tools_json,
            tool_choice: None,
            json_schema: None,
            grammar: None,
            reasoning_format: None,
            chat_template_kwargs: None,
            add_generation_prompt: true,
            use_jinja: true,
            parallel_tool_calls: false,
            // Thinking is disabled for the tool path: the in-template thinking
            // block can blow through the token budget before the model gets to
            // the `<tool_call>` it actually needs to emit. Our StreamFilter
            // still strips literal `<think>...</think>` blocks if a fine-tune
            // produces them anyway, so disabling here is the conservative
            // default.
            enable_thinking: false,
            add_bos: false,
            add_eos: false,
            parse_tool_calls: false,
        };
        let result = model
            .apply_chat_template_oaicompat(&tmpl, &params)
            .map_err(|e| {
                AppError::SummarizationFailed(format!("Failed to apply chat template: {e}"))
            })?;
        eprintln!(
            "[chat-tools] template applied: prompt_len={} grammar={} grammar_lazy={} triggers={} preserved={} stops={:?} thinking_forced_open={}",
            result.prompt.len(),
            result.grammar.is_some(),
            result.grammar_lazy,
            result.grammar_triggers.len(),
            result.preserved_tokens.len(),
            result.additional_stops,
            result.thinking_forced_open,
        );
        eprintln!("[chat-tools] PROMPT >>>\n{}\n<<< PROMPT", result.prompt);

        // Build the set of preserved token ids so we can decode them as
        // literal text (e.g. `<tool_call>`) — required so the streaming parser
        // sees its trigger strings.
        let mut preserved_ids: HashSet<LlamaToken> = HashSet::new();
        for piece in &result.preserved_tokens {
            if let Ok(tokens) = model.str_to_token(piece, AddBos::Never) {
                if tokens.len() == 1 {
                    preserved_ids.insert(tokens[0]);
                }
            }
        }

        // Greedy sampler — see `build_tool_sampler` for why we don't use the
        // template's grammar-constrained sampler here.
        let mut sampler = build_tool_sampler();
        let _ = &preserved_ids; // still used below for token-decoding decisions

        // Tokenize prompt and build a context sized for tool roundtrips.
        let tokens_list = model
            .str_to_token(&result.prompt, AddBos::Always)
            .map_err(|e| AppError::SummarizationFailed(format!("Tokenization failed: {e}")))?;
        let n_input = tokens_list.len() as u32;
        let n_ctx = (n_input + MAX_TOOL_CHAT_TOKENS as u32 + 256).max(MIN_TOOL_CHAT_CTX);

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(n_ctx))
            .with_flash_attention_policy(llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_AUTO);
        let mut ctx = model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| AppError::SummarizationFailed(format!("Context creation failed: {e}")))?;

        let mut batch = LlamaBatch::new(BATCH_SIZE, 1);

        // Feed prompt tokens in chunks.
        let n_prompt = tokens_list.len();
        let last_prompt_idx = n_prompt as i32 - 1;
        for chunk_start in (0..n_prompt).step_by(BATCH_SIZE) {
            if interrupt.load(Ordering::Relaxed) {
                return Err(AppError::Interrupted);
            }
            batch.clear();
            let chunk_end = (chunk_start + BATCH_SIZE).min(n_prompt);
            for i in chunk_start..chunk_end {
                let is_last = i as i32 == last_prompt_idx;
                batch
                    .add(tokens_list[i], i as i32, &[0], is_last)
                    .map_err(|e| {
                        AppError::SummarizationFailed(format!("Batch add failed: {e}"))
                    })?;
            }
            ctx.decode(&mut batch)
                .map_err(|e| AppError::SummarizationFailed(format!("Decode failed: {e}")))?;
        }

        eprintln!(
            "[chat-tools] starting inference: n_prompt={} n_ctx={} max_new={}",
            n_prompt, n_ctx, MAX_TOOL_CHAT_TOKENS
        );

        let mut n_cur = n_prompt as i32;
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut generated_text = String::new();
        let mut tokens_emitted = 0u32;
        let mut events_emitted = 0u32;
        // Stream filter state: skip content inside `<think>` and `<tool_call>` blocks.
        // The blocks are stripped from what the user sees; structured tool calls are
        // recovered from `generated_text` at the end via `extract_tool_calls`.
        let mut filter = StreamFilter::new();

        while n_cur < n_prompt as i32 + MAX_TOOL_CHAT_TOKENS {
            if interrupt.load(Ordering::Relaxed) {
                eprintln!("[chat-tools] interrupted after {} tokens", tokens_emitted);
                return Err(AppError::Interrupted);
            }

            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if model.is_eog_token(token) {
                eprintln!(
                    "[chat-tools] EOG after {} tokens (token id={:?})",
                    tokens_emitted, token
                );
                break;
            }

            // Preserved tokens (e.g. `<tool_call>`) decode to their literal text;
            // everything else decodes as plaintext (special tokens elided).
            let decode_special = preserved_ids.contains(&token);
            let piece = model
                .token_to_piece(token, &mut decoder, decode_special, None)
                .map_err(|e| {
                    AppError::SummarizationFailed(format!("Token to string failed: {e}"))
                })?;

            tokens_emitted += 1;
            generated_text.push_str(&piece);

            if tokens_emitted <= 20 || tokens_emitted % 50 == 0 {
                eprintln!(
                    "[chat-tools] tok#{} id={:?} special={} piece={:?}",
                    tokens_emitted, token, decode_special, piece
                );
            }

            // Check stop sequences against the cumulative output.
            // We add our own tool-call-closing markers as stops: once the model
            // has emitted a complete tool call, anything after that is at best
            // noise and at worst hallucinated tool-response simulation.
            let stop_now = result
                .additional_stops
                .iter()
                .any(|s| !s.is_empty() && generated_text.ends_with(s))
                || generated_text.ends_with(TOOL_CLOSE)
                || generated_text.ends_with(GEMMA_TOOL_CLOSE);

            let events_before = events_emitted;
            filter.push(&piece, &mut |e| {
                events_emitted += 1;
                if events_emitted <= 30 || events_emitted % 50 == 0 {
                    eprintln!("[chat-tools] emit#{} {:?}", events_emitted, e);
                }
                on_event(e);
            });
            if events_emitted > events_before && tokens_emitted <= 20 {
                // already logged above
            }

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| AppError::SummarizationFailed(format!("Batch add failed: {e}")))?;
            n_cur += 1;

            ctx.decode(&mut batch)
                .map_err(|e| AppError::SummarizationFailed(format!("Decode failed: {e}")))?;

            if stop_now {
                eprintln!(
                    "[chat-tools] hit additional_stop after {} tokens",
                    tokens_emitted
                );
                break;
            }
        }

        filter.flush(&mut |e| {
            events_emitted += 1;
            eprintln!("[chat-tools] flush emit#{} {:?}", events_emitted, e);
            on_event(e);
        });

        eprintln!(
            "[chat-tools] inference done: tokens={} events={} generated_text_len={}",
            tokens_emitted,
            events_emitted,
            generated_text.len()
        );
        eprintln!(
            "[chat-tools] GENERATED >>>\n{}\n<<< GENERATED",
            generated_text
        );

        // Strip a trailing stop sequence if present (it leaked into generated_text
        // but should not feed into the response parser).
        for stop in &result.additional_stops {
            if !stop.is_empty() && generated_text.ends_with(stop) {
                let new_len = generated_text.len().saturating_sub(stop.len());
                generated_text.truncate(new_len);
                break;
            }
        }

        // Extract tool calls directly from the generated text. We don't use
        // `parse_response_oaicompat` because llama.cpp's `common_chat_parse`
        // throws (LLAMA_RS_STATUS_EXCEPTION) for outputs from some templates,
        // and an FFI exception is fatal. Qwen-style models emit calls as
        // `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`, which
        // is what we look for.
        let tool_calls = extract_tool_calls(&generated_text);
        eprintln!(
            "[chat-tools] extracted {} tool calls: {:?}",
            tool_calls.len(),
            tool_calls
                .iter()
                .map(|t| (&t.name, &t.arguments))
                .collect::<Vec<_>>()
        );

        // Streaming the tool-call args char-by-char turned out to be fragile
        // (the C parser throws on partial state for some models). Instead, after
        // the bulk response parse, synthesize the full sequence of UI events for
        // each tool call: start, the full args as one delta, end.
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
            prompt_tokens: n_input,
            completion_tokens: tokens_emitted,
            max_tokens,
        })
    }

    /// Generate a short title for a meeting from its summary.
    /// **Blocking** -- call from `spawn_blocking`.
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    pub fn generate_title(
        &self,
        model_path: &Path,
        summary: &str,
    ) -> Result<String, AppError> {
        self.ensure_loaded(model_path)?;

        let guard = lock_or_err(&self.model)?;
        let (_, model) = guard
            .as_ref()
            .ok_or_else(|| AppError::SummarizationFailed("Model not loaded".into()))?;

        let system = "Generate a very short, descriptive meeting title (5-6 words maximum) from the summary below. Output ONLY the title text — no quotes, no dashes, no colons, nothing else.";
        let messages_json = serde_json::json!([
            {"role": "system", "content": system},
            {"role": "user", "content": summary}
        ])
        .to_string();
        let prompt = Self::apply_template(model, &messages_json, false)?;

        let tokens_list = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| AppError::SummarizationFailed(format!("Tokenization failed: {e}")))?;

        let n_input = tokens_list.len() as u32;
        let max_title_tokens: i32 = 512;
        let n_ctx = (n_input + max_title_tokens as u32 + 64).max(512);

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(n_ctx))
            .with_flash_attention_policy(llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_AUTO);
        let mut ctx = model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| AppError::SummarizationFailed(format!("Context creation failed: {e}")))?;

        let mut batch = LlamaBatch::new(BATCH_SIZE.min(tokens_list.len() + 64), 1);

        let n_prompt = tokens_list.len();
        let last_prompt_idx = n_prompt as i32 - 1;
        for chunk_start in (0..n_prompt).step_by(BATCH_SIZE) {
            batch.clear();
            let chunk_end = (chunk_start + BATCH_SIZE).min(n_prompt);
            for i in chunk_start..chunk_end {
                let is_last = i as i32 == last_prompt_idx;
                batch
                    .add(tokens_list[i], i as i32, &[0], is_last)
                    .map_err(|e| {
                        AppError::SummarizationFailed(format!("Batch add failed: {e}"))
                    })?;
            }
            ctx.decode(&mut batch)
                .map_err(|e| AppError::SummarizationFailed(format!("Decode failed: {e}")))?;
        }

        let mut n_cur = n_prompt as i32;
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut sampler = build_sampler();
        let mut output = String::new();
        let mut inside_think = false;
        let mut title_text = String::new();

        while n_cur < n_prompt as i32 + max_title_tokens {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if token == model.token_eos() {
                break;
            }

            let piece = model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(|e| {
                    AppError::SummarizationFailed(format!("Token to string failed: {e}"))
                })?;

            output.push_str(&piece);

            if !inside_think && piece.contains(THINK_OPEN) {
                inside_think = true;
            }
            if inside_think && output.ends_with(THINK_CLOSE) {
                inside_think = false;
            }

            if !inside_think {
                title_text.push_str(&piece);
                let trimmed = title_text.trim();
                if !trimmed.is_empty() && trimmed.contains('\n') {
                    break;
                }
            }

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| AppError::SummarizationFailed(format!("Batch add failed: {e}")))?;

            n_cur += 1;

            ctx.decode(&mut batch)
                .map_err(|e| AppError::SummarizationFailed(format!("Decode failed: {e}")))?;
        }

        eprintln!("[title-gen] Raw output ({} tokens): {:?}", n_cur - n_prompt as i32, output);
        let title = strip_think_tags(output.trim());
        eprintln!("[title-gen] After strip_think_tags: {:?}", title);
        let title = title
            .trim()
            .trim_matches('"')
            .trim_matches(|c| c == '-' || c == '–' || c == '—')
            .trim()
            .to_string();
        eprintln!("[title-gen] Final title: {:?}", title);
        Ok(title)
    }
}

/// Streaming filter for the tool-aware chat path.
///
/// Recognised blocks:
///  - `<think>...</think>` — body emitted as `is_thinking=true` content.
///  - `<tool_call>...</tool_call>` (Qwen) — suppressed; tool calls are recovered
///    from the full text via `extract_tool_calls`.
///  - `<|tool_call>...<tool_call|>` (Gemma) — suppressed.
///  - `<|tool_response>...<channel|>` (Gemma hallucinated tool reply) —
///    suppressed; the model is imagining a tool response that hasn't happened
///    yet.
///  - `<|tool_call_start|>...<|tool_call_end|>` (LFM2) — suppressed.
///
/// Buffers across token boundaries so tags split across pieces are still
/// recognised.
struct StreamFilter {
    state: FilterState,
    /// Pending text we haven't decided how to emit yet (may contain the start
    /// of a tag).
    buf: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterState {
    Plain,
    InThink,
    /// In a suppressed block; the close marker is one of TOOL_CLOSE,
    /// GEMMA_TOOL_CLOSE, GEMMA_RESPONSE_CLOSE, or LFM2_TOOL_CLOSE.
    Suppressed { close: &'static str },
}

/// All known opening markers and their corresponding state. The longer
/// opening should be checked first if there's any prefix overlap (`<|tool_call>`
/// vs `<|tool_response>` — different but both start with `<|`).
const OPEN_MARKERS: &[(&str, FilterState)] = &[
    (THINK_OPEN, FilterState::InThink),
    (TOOL_OPEN, FilterState::Suppressed { close: TOOL_CLOSE }),
    (
        GEMMA_TOOL_OPEN,
        FilterState::Suppressed {
            close: GEMMA_TOOL_CLOSE,
        },
    ),
    (
        GEMMA_RESPONSE_OPEN,
        FilterState::Suppressed {
            close: GEMMA_RESPONSE_CLOSE,
        },
    ),
    (
        LFM2_TOOL_OPEN,
        FilterState::Suppressed {
            close: LFM2_TOOL_CLOSE,
        },
    ),
];

const TOOL_OPEN: &str = "<tool_call>";
const TOOL_CLOSE: &str = "</tool_call>";
/// Gemma-family tool-call markers. Note the asymmetric `|`: opens with
/// `<|tool_call>` and closes with `<tool_call|>`.
const GEMMA_TOOL_OPEN: &str = "<|tool_call>";
const GEMMA_TOOL_CLOSE: &str = "<tool_call|>";
/// Gemma-family hallucinated tool-response wrapper. We never emit a real
/// `<|tool_response>` to the model (we use the OAI `tool` role for that), so
/// any of these in output is the model imagining a response — suppress.
const GEMMA_RESPONSE_OPEN: &str = "<|tool_response>";
const GEMMA_RESPONSE_CLOSE: &str = "<channel|>";
/// Gemma-specific string-literal markers used inside tool-call bodies.
const GEMMA_STR_MARKER: &str = "<|\"|>";
/// LFM2-family tool-call markers. The body is a Pythonic list of calls, e.g.
/// `[get_weather(city="Paris"), get_time()]`.
const LFM2_TOOL_OPEN: &str = "<|tool_call_start|>";
const LFM2_TOOL_CLOSE: &str = "<|tool_call_end|>";

impl StreamFilter {
    fn new() -> Self {
        Self {
            state: FilterState::Plain,
            buf: String::new(),
        }
    }

    fn push<F>(&mut self, piece: &str, on_event: &mut F)
    where
        F: FnMut(&InferenceEvent<'_>),
    {
        self.buf.push_str(piece);
        self.drain(on_event, false);
    }

    fn flush<F>(&mut self, on_event: &mut F)
    where
        F: FnMut(&InferenceEvent<'_>),
    {
        self.drain(on_event, true);
    }

    fn drain<F>(&mut self, on_event: &mut F, is_final: bool)
    where
        F: FnMut(&InferenceEvent<'_>),
    {
        loop {
            match self.state {
                FilterState::Plain => {
                    // Find the earliest opening marker.
                    let mut earliest: Option<(usize, &'static str, FilterState)> = None;
                    for (open, new_state) in OPEN_MARKERS {
                        if let Some(idx) = self.buf.find(*open) {
                            if earliest.map_or(true, |(prev, _, _)| idx < prev) {
                                earliest = Some((idx, *open, *new_state));
                            }
                        }
                    }
                    match earliest {
                        Some((idx, open, new_state)) => {
                            let before = self.buf[..idx].to_string();
                            if !before.is_empty() {
                                on_event(&InferenceEvent::ContentToken {
                                    text: &before,
                                    is_thinking: false,
                                });
                            }
                            self.buf.drain(..idx + open.len());
                            self.state = new_state;
                        }
                        None => {
                            // No opening tag in buf. Emit everything up to the last '<' (in
                            // case a tag is being assembled across tokens).
                            if let Some(lt) = self.buf.rfind('<') {
                                let before: String = self.buf[..lt].to_string();
                                if !before.is_empty() {
                                    on_event(&InferenceEvent::ContentToken {
                                        text: &before,
                                        is_thinking: false,
                                    });
                                }
                                self.buf.drain(..lt);
                            } else if !self.buf.is_empty() {
                                let chunk = std::mem::take(&mut self.buf);
                                on_event(&InferenceEvent::ContentToken {
                                    text: &chunk,
                                    is_thinking: false,
                                });
                            }
                            if is_final && !self.buf.is_empty() {
                                let chunk = std::mem::take(&mut self.buf);
                                on_event(&InferenceEvent::ContentToken {
                                    text: &chunk,
                                    is_thinking: false,
                                });
                            }
                            return;
                        }
                    }
                }
                FilterState::InThink => {
                    if let Some(end) = self.buf.find(THINK_CLOSE) {
                        let inner = self.buf[..end].to_string();
                        if !inner.is_empty() {
                            on_event(&InferenceEvent::ContentToken {
                                text: &inner,
                                is_thinking: true,
                            });
                        }
                        self.buf.drain(..end + THINK_CLOSE.len());
                        self.state = FilterState::Plain;
                    } else if let Some(lt) = self.buf.rfind('<') {
                        let inner = self.buf[..lt].to_string();
                        if !inner.is_empty() {
                            on_event(&InferenceEvent::ContentToken {
                                text: &inner,
                                is_thinking: true,
                            });
                        }
                        self.buf.drain(..lt);
                        return;
                    } else {
                        if !self.buf.is_empty() {
                            let inner = std::mem::take(&mut self.buf);
                            on_event(&InferenceEvent::ContentToken {
                                text: &inner,
                                is_thinking: true,
                            });
                        }
                        return;
                    }
                }
                FilterState::Suppressed { close } => {
                    if let Some(end) = self.buf.find(close) {
                        // Suppress the body; structured tool calls are recovered
                        // from the full generated_text via `extract_tool_calls`.
                        self.buf.drain(..end + close.len());
                        self.state = FilterState::Plain;
                    } else {
                        // Body is being assembled — keep waiting.
                        return;
                    }
                }
            }
        }
    }
}

/// Extract structured tool calls from raw assistant output. Four sources are
/// scanned, in order:
///
///  1. `<tool_call>...</tool_call>` blocks (Qwen-family). The block body may
///     be JSON (`{"name":"X","arguments":{...}}`) or XML/Hermes
///     (`<function=NAME><parameter=K>V</parameter></function>`).
///  2. `<|tool_call>...<tool_call|>` blocks (Gemma-family). Body is
///     `call:NAME{KEY:<|"|>VALUE<|"|>, ...}`.
///  3. `<|tool_call_start|>...<|tool_call_end|>` blocks (LFM2-family). Body is
///     a Pythonic list of calls, e.g. `[name(k="v"), other()]`.
///  4. Fenced code blocks with language tags `tool_code`, `tool_call`,
///     `python`, or `json`. The body may be a Python-style call
///     (`print(name(k="v"))` or `name(k="v")`) or a JSON object.
///
/// Malformed blocks are silently skipped — better to degrade to a plain reply
/// than to crash.
fn extract_tool_calls(text: &str) -> Vec<ToolCallSpec> {
    let mut out = Vec::new();
    let mut push = |name: String, arguments: String, out: &mut Vec<ToolCallSpec>| {
        let id = format!("call_{}", out.len());
        out.push(ToolCallSpec {
            id,
            name,
            arguments,
        });
    };

    // 1. Qwen-style <tool_call>...</tool_call> blocks.
    let mut cursor = 0;
    while let Some(rel_start) = text[cursor..].find(TOOL_OPEN) {
        let body_start = cursor + rel_start + TOOL_OPEN.len();
        let Some(rel_end) = text[body_start..].find(TOOL_CLOSE) else {
            break;
        };
        let body = text[body_start..body_start + rel_end].trim();
        cursor = body_start + rel_end + TOOL_CLOSE.len();

        if let Some((name, arguments)) = parse_tool_call_body(body) {
            push(name, arguments, &mut out);
        }
    }

    // 2. Gemma-style <|tool_call>...<tool_call|> blocks.
    let mut cursor = 0;
    while let Some(rel_start) = text[cursor..].find(GEMMA_TOOL_OPEN) {
        let body_start = cursor + rel_start + GEMMA_TOOL_OPEN.len();
        let Some(rel_end) = text[body_start..].find(GEMMA_TOOL_CLOSE) else {
            break;
        };
        let body = text[body_start..body_start + rel_end].trim();
        cursor = body_start + rel_end + GEMMA_TOOL_CLOSE.len();

        if let Some((name, arguments)) = parse_gemma_tool_call_body(body) {
            push(name, arguments, &mut out);
        }
    }

    // 3. LFM2-style <|tool_call_start|>[ name(...), name(...) ]<|tool_call_end|>
    //    blocks. The body is a Pythonic list of calls; split it into individual
    //    `name(...)` calls and reuse the Python-style parser for each.
    let mut cursor = 0;
    while let Some(rel_start) = text[cursor..].find(LFM2_TOOL_OPEN) {
        let body_start = cursor + rel_start + LFM2_TOOL_OPEN.len();
        let Some(rel_end) = text[body_start..].find(LFM2_TOOL_CLOSE) else {
            break;
        };
        let body = text[body_start..body_start + rel_end].trim();
        cursor = body_start + rel_end + LFM2_TOOL_CLOSE.len();

        // Strip the surrounding list brackets if present, then split on
        // top-level commas (commas inside each call's parens/strings are kept).
        let inner = body
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(body);
        for call in split_top_level_commas(inner) {
            let call = call.trim();
            if call.is_empty() {
                continue;
            }
            if let Some((name, arguments)) = parse_python_style_call(call) {
                push(name, arguments, &mut out);
            }
        }
    }

    // 4. Fenced code blocks.
    for (lang, body) in iter_code_fences(text) {
        let lang_lower = lang.to_ascii_lowercase();
        if !matches!(
            lang_lower.as_str(),
            "tool_code" | "tool_call" | "python" | "json"
        ) {
            continue;
        }
        let candidate = body.trim();
        // Try JSON first (handles `{"name":"X", "arguments":{...}}` and
        // `{"name":"X", "parameters":{...}}` shapes).
        if let Some((name, arguments)) = parse_json_call(candidate) {
            push(name, arguments, &mut out);
            continue;
        }
        // Then try Python-style: `print(name(k=v, k=v))` or `name(k=v)`.
        if let Some((name, arguments)) = parse_python_style_call(candidate) {
            push(name, arguments, &mut out);
            continue;
        }
    }

    out
}

/// Iterate over ```language\n...\n``` fenced blocks in `text`.
/// Returns (lang, body) pairs. Yields nothing if there are no fences.
fn iter_code_fences(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        // Find next ```
        let Some(rel) = text[i..].find("```") else {
            break;
        };
        let open = i + rel;
        // Language tag runs until newline.
        let after_open = open + 3;
        let lang_end = text[after_open..]
            .find('\n')
            .map(|n| after_open + n)
            .unwrap_or(text.len());
        let lang = text[after_open..lang_end].trim().to_string();
        let body_start = lang_end.saturating_add(1);
        if body_start >= text.len() {
            break;
        }
        // Find closing ```.
        let Some(rel_close) = text[body_start..].find("```") else {
            break;
        };
        let body_end = body_start + rel_close;
        out.push((lang, text[body_start..body_end].to_string()));
        i = body_end + 3;
    }
    out
}

/// Parse one `<tool_call>` body into (name, arguments-as-json-string).
/// Tries JSON first, then the XML/Hermes form.
fn parse_tool_call_body(body: &str) -> Option<(String, String)> {
    // Try the documented JSON form.
    if let Some(parsed) = parse_json_call(body) {
        return Some(parsed);
    }

    // Fall back to the XML/Hermes form: `<function=NAME>...<parameter=KEY>V</parameter>...</function>`.
    let func_open_idx = body.find("<function=")?;
    let name_start = func_open_idx + "<function=".len();
    let name_end = body[name_start..].find('>')?;
    let name = body[name_start..name_start + name_end].trim().to_string();
    if name.is_empty() {
        return None;
    }

    let body_after_func = &body[name_start + name_end + 1..];
    let func_close = body_after_func.find("</function>").unwrap_or(body_after_func.len());
    let func_body = &body_after_func[..func_close];

    let mut args = serde_json::Map::<String, serde_json::Value>::new();
    let mut p_cursor = 0;
    while let Some(rel_open) = func_body[p_cursor..].find("<parameter=") {
        let key_start = p_cursor + rel_open + "<parameter=".len();
        let key_end_rel = match func_body[key_start..].find('>') {
            Some(i) => i,
            None => break,
        };
        let key = func_body[key_start..key_start + key_end_rel].trim().to_string();
        let val_start = key_start + key_end_rel + 1;
        let val_end_rel = match func_body[val_start..].find("</parameter>") {
            Some(i) => i,
            None => break,
        };
        let raw_val = func_body[val_start..val_start + val_end_rel].trim();
        // Heuristic typing: try JSON (number / bool / null / array / object) first,
        // fall back to string.
        let json_val: serde_json::Value = match serde_json::from_str(raw_val) {
            Ok(v) => v,
            Err(_) => serde_json::Value::String(raw_val.to_string()),
        };
        if !key.is_empty() {
            args.insert(key, json_val);
        }
        p_cursor = val_start + val_end_rel + "</parameter>".len();
    }

    Some((name, serde_json::Value::Object(args).to_string()))
}

/// Parse a Gemma `<|tool_call>...<tool_call|>` body into (name, json-args).
///
/// Body shape: `call:NAME{KEY:<|"|>VALUE<|"|>, KEY:VALUE, ...}`. String values
/// are wrapped in Gemma's `<|"|>` markers; numeric / bool / null literals are
/// emitted bare.
fn parse_gemma_tool_call_body(body: &str) -> Option<(String, String)> {
    let s = body.trim();
    // Strip optional "call:" prefix.
    let s = s.strip_prefix("call:").unwrap_or(s);

    let brace_open = s.find('{')?;
    let brace_close = s.rfind('}')?;
    if brace_close <= brace_open {
        return None;
    }
    let name = s[..brace_open].trim().to_string();
    if name.is_empty() {
        return None;
    }
    let inner = &s[brace_open + 1..brace_close];

    // Split on top-level commas (skipping `<|"|>...<|"|>` quoted spans).
    let pairs = split_gemma_pairs(inner);
    let mut args = serde_json::Map::<String, serde_json::Value>::new();
    for pair in pairs {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some(colon) = pair.find(':') else {
            continue;
        };
        let key = pair[..colon].trim().to_string();
        let raw_val = pair[colon + 1..].trim();
        if key.is_empty() {
            continue;
        }
        let value = if let Some(inner) = raw_val
            .strip_prefix(GEMMA_STR_MARKER)
            .and_then(|s| s.strip_suffix(GEMMA_STR_MARKER))
        {
            serde_json::Value::String(inner.to_string())
        } else {
            // Try JSON literal (numbers, bool, null, etc.) then fall back to string.
            serde_json::from_str(raw_val)
                .unwrap_or_else(|_| serde_json::Value::String(raw_val.to_string()))
        };
        args.insert(key, value);
    }

    Some((name, serde_json::Value::Object(args).to_string()))
}

/// Split a Gemma tool-call body's inner content on top-level commas, treating
/// `<|"|>...<|"|>` as opaque quoted spans.
fn split_gemma_pairs(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_str = false;
    let mut start = 0usize;
    let mut i = 0usize;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        // Check for the multi-char string marker at position i.
        if s[i..].starts_with(GEMMA_STR_MARKER) {
            in_str = !in_str;
            i += GEMMA_STR_MARKER.len();
            continue;
        }
        if !in_str && bytes[i] == b',' {
            out.push(s[start..i].to_string());
            start = i + 1;
        }
        i += 1;
    }
    if start < s.len() {
        out.push(s[start..].to_string());
    }
    out
}

/// Try to parse a tool-call body as JSON. Accepts both `arguments` and
/// `parameters` keys (Gemma docs use `parameters`).
fn parse_json_call(body: &str) -> Option<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let name = value.get("name").and_then(|v| v.as_str())?.to_string();
    let args_value = value
        .get("arguments")
        .or_else(|| value.get("parameters"))
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    let arguments = match args_value {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    };
    Some((name, arguments))
}

/// Try to parse a Python-style function call: optional `print(` wrapper,
/// optional `module.` prefix on the name, keyword args only.
///
/// Examples that parse:
///   `print(search_meetings(query="foo"))`
///   `default_api.search_meetings(query="foo", limit=5)`
///   `search_meetings(query="foo")`
///
/// Values may be string literals (`"..."` or `'...'`), numeric literals,
/// booleans (`True`/`False`), `None`, or already-JSON-parseable expressions.
fn parse_python_style_call(body: &str) -> Option<(String, String)> {
    let mut s = body.trim();

    // Strip optional `print(` wrapper.
    if let Some(stripped) = s.strip_prefix("print(") {
        s = stripped.strip_suffix(')')?.trim();
    }

    // Find `name(` opening — name may contain dots (e.g. `default_api.search_meetings`).
    let paren_idx = s.find('(')?;
    let full_name = s[..paren_idx].trim();
    let name = full_name.rsplit('.').next()?.to_string();
    if name.is_empty() {
        return None;
    }

    // Body inside outermost parens.
    let after = &s[paren_idx + 1..];
    let args_str = after.strip_suffix(')')?.trim();

    let mut args = serde_json::Map::<String, serde_json::Value>::new();
    for raw in split_top_level_commas(args_str) {
        let pair = raw.trim();
        if pair.is_empty() {
            continue;
        }
        let Some(eq_idx) = pair.find('=') else {
            // Positional args not supported.
            continue;
        };
        let key = pair[..eq_idx].trim().to_string();
        let raw_val = pair[eq_idx + 1..].trim();
        let value = python_literal_to_json(raw_val);
        if !key.is_empty() {
            args.insert(key, value);
        }
    }

    Some((name, serde_json::Value::Object(args).to_string()))
}

/// Split a string at commas that are NOT inside brackets/quotes. Used to
/// separate keyword args in a Python call without pulling in a full parser.
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if let Some(q) = in_str {
            if ch == '\\' {
                i += 2;
                continue;
            }
            if ch == q {
                in_str = None;
            }
        } else {
            match ch {
                '"' | '\'' => in_str = Some(ch),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(s[start..i].to_string());
                    start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    if start < s.len() {
        out.push(s[start..].to_string());
    }
    out
}

/// Coerce a Python-literal substring to a JSON value. String literals lose
/// their quotes; `True`/`False`/`None` map to their JSON equivalents;
/// everything else is tried as JSON first and falls back to a string.
fn python_literal_to_json(s: &str) -> serde_json::Value {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        return serde_json::Value::String(s[1..s.len() - 1].to_string());
    }
    match s {
        "True" | "true" => return serde_json::Value::Bool(true),
        "False" | "false" => return serde_json::Value::Bool(false),
        "None" | "null" => return serde_json::Value::Null,
        _ => {}
    }
    serde_json::from_str(s).unwrap_or_else(|_| serde_json::Value::String(s.to_string()))
}

/// Sampling parameters for free-text generation. Pure greedy decoding can fall
/// into phrase-repetition loops on some models (observed with LFM2.5 burning the
/// whole token budget repeating one sentence). Instead of a repetition penalty
/// (whose llama.cpp implementation is notoriously finicky — it has to flip the
/// multiply/divide for negative logits because the paper's formula misbehaves
/// there), we use min-p truncation + temperature: min-p drops the unlikely tail
/// so output stays coherent, and a high temperature flattens the survivors
/// enough to break out of loops. All tunable.
const MIN_P: f32 = 0.05;
const TEMPERATURE: f32 = 1.5;
/// Fixed seed so the same input yields the same output across runs.
const SAMPLER_SEED: u32 = 0xC0FFEE;

/// Free-text generation sampler: min-p truncation, then temperature, then a
/// seeded distribution sample. Used by summarization, plain chat, and titles.
fn build_sampler() -> LlamaSampler {
    LlamaSampler::chain_simple([
        LlamaSampler::min_p(MIN_P, 1),
        LlamaSampler::temp(TEMPERATURE),
        LlamaSampler::dist(SAMPLER_SEED),
    ])
}

/// Sampler for the tool-aware chat path. We deliberately **do not** use the
/// grammar-constrained sampler produced by the chat template: llama.cpp's
/// grammar implementation has a `GGML_ASSERT(!stacks.empty())` invariant that
/// fires when the generated tokens drive the grammar into a dead state — it
/// crashes the whole process. The Qwen-style instruction-tuned models we ship
/// emit tool calls in their trained `<tool_call>{...}</tool_call>` format
/// without coaxing, and `parse_response_oaicompat` extracts them at the end of
/// the turn either way.
fn build_tool_sampler() -> LlamaSampler {
    // Tool calls are structured output — keep greedy for deterministic,
    // well-formed calls rather than sampling.
    LlamaSampler::greedy()
}

/// Strip `<think>...</think>` blocks from model output.
fn strip_think_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut remaining = s;
    while let Some(start) = remaining.find(THINK_OPEN) {
        result.push_str(&remaining[..start]);
        if let Some(end) = remaining[start..].find(THINK_CLOSE) {
            remaining = &remaining[(start + end + THINK_CLOSE.len())..];
        } else {
            return result.trim().to_string();
        }
    }
    result.push_str(remaining);
    result
}

/// Tauri managed-state wrapper. The `Arc` allows cloning a handle into
/// spawned Tokio tasks that outlive the command handler's borrow.
pub struct SummarizationState {
    pub service: std::sync::Arc<SummarizationService>,
    /// The meeting ID currently being summarized (including title generation),
    /// or `None` if idle.
    pub active_meeting_id: std::sync::Mutex<Option<String>>,
    /// Meeting IDs waiting to be summarized. Processed sequentially after the
    /// active summarization (including its title generation) completes.
    pub pending_queue: std::sync::Mutex<std::collections::VecDeque<String>>,
    /// Cooperative-cancellation flag shared between summarization and chat.
    /// When `true`, a running inference loop will exit early with
    /// `AppError::Interrupted` on its next token. Whoever sets it should
    /// clear it once they've acquired the model lock and are about to start
    /// their own work.
    pub llm_interrupt: std::sync::Arc<AtomicBool>,
}

#[cfg(test)]
mod tests {
    use super::{extract_tool_calls, strip_think_tags, InferenceEvent, StreamFilter};

    #[test]
    fn extract_tool_calls_handles_object_arguments() {
        let text = r#"<tool_call>{"name": "search_meetings", "arguments": {"query": "budget"}}</tool_call>"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_meetings");
        // Object arguments are re-serialized to a JSON string.
        let parsed: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(parsed["query"], "budget");
    }

    #[test]
    fn extract_tool_calls_handles_string_arguments() {
        let text = r#"<tool_call>{"name": "search_meetings", "arguments": "{\"query\": \"x\"}"}</tool_call>"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(parsed["query"], "x");
    }

    #[test]
    fn extract_tool_calls_handles_multiple_blocks() {
        let text = r#"Pre.<tool_call>{"name":"a","arguments":{"q":1}}</tool_call> mid <tool_call>{"name":"b","arguments":{}}</tool_call>post"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "a");
        assert_eq!(calls[1].name, "b");
        assert_ne!(calls[0].id, calls[1].id);
    }

    #[test]
    fn extract_tool_calls_skips_malformed_body() {
        let text = r#"<tool_call>not json</tool_call><tool_call>{"name":"ok","arguments":{}}</tool_call>"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "ok");
    }

    #[test]
    fn extract_tool_calls_skips_missing_name() {
        let text = r#"<tool_call>{"arguments":{}}</tool_call>"#;
        assert!(extract_tool_calls(text).is_empty());
    }

    #[test]
    fn extract_tool_calls_no_blocks_returns_empty() {
        assert!(extract_tool_calls("just a plain reply").is_empty());
    }

    #[test]
    fn extract_tool_calls_handles_unterminated_block() {
        // An unclosed block should stop iteration rather than crash.
        let text = r#"<tool_call>{"name":"oops"#;
        assert!(extract_tool_calls(text).is_empty());
    }

    #[test]
    fn extract_tool_calls_handles_xml_hermes_format() {
        // The exact shape the local Qwen3.5 build emits.
        let text = "<tool_call>\n<function=search_meetings>\n<parameter=query>\nproduct bundles\n</parameter>\n<parameter=limit>\n5\n</parameter>\n</function>\n</tool_call>";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_meetings");
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "product bundles");
        // limit was emitted as `5` -> typed as JSON number.
        assert_eq!(args["limit"], 5);
    }

    #[test]
    fn extract_tool_calls_xml_form_strings_are_strings() {
        let text = "<tool_call><function=search_meetings><parameter=query>hello world</parameter></function></tool_call>";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "hello world");
        assert!(args["query"].is_string());
    }

    #[test]
    fn extract_tool_calls_xml_form_missing_function_skipped() {
        let text = "<tool_call>no function tag here</tool_call>";
        assert!(extract_tool_calls(text).is_empty());
    }

    // --- Gemma-style fenced-code-block tool calls ---------------------------

    #[test]
    fn extract_tool_calls_gemma_tool_code_python_call() {
        // Gemma 3 documented format: a ```tool_code fenced block containing a
        // Python call wrapped in `print(...)`.
        let text = "Sure, let me check.\n```tool_code\nprint(search_meetings(query=\"product bundles\"))\n```";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_meetings");
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "product bundles");
    }

    #[test]
    fn extract_tool_calls_gemma_with_module_prefix() {
        let text = "```tool_code\nprint(default_api.search_meetings(query=\"foo\", limit=5))\n```";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        // Module prefix is dropped — the dispatcher only knows by short name.
        assert_eq!(calls[0].name, "search_meetings");
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "foo");
        assert_eq!(args["limit"], 5);
    }

    #[test]
    fn extract_tool_calls_gemma_bare_call_no_print_wrapper() {
        let text = "```tool_code\nsearch_meetings(query=\"foo\")\n```";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_meetings");
    }

    #[test]
    fn extract_tool_calls_gemma_single_quotes() {
        let text = "```tool_code\nprint(search_meetings(query='product bundles'))\n```";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "product bundles");
    }

    #[test]
    fn extract_tool_calls_fenced_json_block() {
        // Some models emit a ```json fence with the OpenAI shape.
        let text = "```json\n{\"name\":\"search_meetings\",\"arguments\":{\"query\":\"foo\"}}\n```";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_meetings");
    }

    #[test]
    fn extract_tool_calls_fenced_json_with_parameters_key() {
        // Gemma docs use `parameters` instead of `arguments` in some samples.
        let text = "```json\n{\"name\":\"search_meetings\",\"parameters\":{\"query\":\"foo\"}}\n```";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "foo");
    }

    #[test]
    fn extract_tool_calls_ignores_unrelated_fences() {
        // A ```rust or ```text fence must not be treated as a tool call.
        let text = "```rust\nfn main() {}\n``` and ```text\nhello\n```";
        assert!(extract_tool_calls(text).is_empty());
    }

    #[test]
    fn extract_tool_calls_handles_multiple_fenced_calls() {
        let text = "```tool_code\nprint(search_meetings(query=\"a\"))\n```\nthen\n```tool_code\nprint(search_meetings(query=\"b\"))\n```";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        let a0: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        let a1: serde_json::Value = serde_json::from_str(&calls[1].arguments).unwrap();
        assert_eq!(a0["query"], "a");
        assert_eq!(a1["query"], "b");
    }

    // --- Gemma `<|tool_call>...<tool_call|>` format ------------------------

    #[test]
    fn extract_tool_calls_gemma_native_format() {
        // Verbatim from the user's terminal output.
        let text = "<|tool_call>call:search_meetings{query:<|\"|>status of product bundles<|\"|>}<tool_call|>";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_meetings");
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "status of product bundles");
    }

    #[test]
    fn extract_tool_calls_gemma_native_multiple_args() {
        let text = "<|tool_call>call:search_meetings{query:<|\"|>foo<|\"|>, limit:5}<tool_call|>";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "foo");
        assert_eq!(args["limit"], 5);
    }

    #[test]
    fn extract_tool_calls_gemma_with_comma_inside_string() {
        let text = "<|tool_call>call:search_meetings{query:<|\"|>foo, bar<|\"|>}<tool_call|>";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "foo, bar");
    }

    #[test]
    fn extract_tool_calls_gemma_only_first_when_repeated() {
        // The model sometimes emits the same call twice (hallucinating a
        // re-call). Both should be returned; the caller can dedupe if it cares.
        let text = "<|tool_call>call:search_meetings{query:<|\"|>x<|\"|>}<tool_call|> blah <|tool_call>call:search_meetings{query:<|\"|>x<|\"|>}<tool_call|>";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
    }

    // --- StreamFilter for Gemma blocks -------------------------------------

    #[test]
    fn filter_suppresses_gemma_tool_call_block() {
        let (c, _) = collapsed(&run_filter(&[
            "Sure!<|tool_call>call:search_meetings{query:<|\"|>x<|\"|>}<tool_call|>",
        ]));
        assert_eq!(c, "Sure!");
    }

    #[test]
    fn filter_suppresses_gemma_tool_response_block() {
        let (c, _) = collapsed(&run_filter(&[
            "ok<|tool_response>thought The user asked X. I should...<channel|>after",
        ]));
        assert_eq!(c, "okafter");
    }

    #[test]
    fn filter_suppresses_gemma_blocks_split_across_pieces() {
        let (c, _) = collapsed(&run_filter(&[
            "Sure!<|tool",
            "_call>call:search_meetings{query:<|\"|>x<|",
            "\"|>}<tool_call|>",
        ]));
        assert_eq!(c, "Sure!");
    }

    // --- LFM2 `<|tool_call_start|>[...]<|tool_call_end|>` format ------------

    #[test]
    fn extract_tool_calls_lfm2_single_call() {
        let text = "<|tool_call_start|>[get_weather(city=\"Paris\")]<|tool_call_end|>";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["city"], "Paris");
    }

    #[test]
    fn extract_tool_calls_lfm2_multiple_calls() {
        let text =
            "<|tool_call_start|>[search_meetings(query=\"x\", limit=5), get_time()]<|tool_call_end|>";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "search_meetings");
        let args0: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args0["query"], "x");
        assert_eq!(args0["limit"], 5);
        assert_eq!(calls[1].name, "get_time");
    }

    #[test]
    fn extract_tool_calls_lfm2_comma_inside_string() {
        let text = "<|tool_call_start|>[search_meetings(query=\"foo, bar\")]<|tool_call_end|>";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "foo, bar");
    }

    #[test]
    fn filter_suppresses_lfm2_tool_call_block() {
        let (c, _) = collapsed(&run_filter(&[
            "Sure!<|tool_call_start|>[get_weather(city=\"Paris\")]<|tool_call_end|>",
        ]));
        assert_eq!(c, "Sure!");
    }

    #[test]
    fn filter_suppresses_lfm2_block_split_across_pieces() {
        let (c, _) = collapsed(&run_filter(&[
            "Sure!<|tool_call",
            "_start|>[get_weather(city=\"Paris\")]<|tool_call",
            "_end|>",
        ]));
        assert_eq!(c, "Sure!");
    }

    #[test]
    fn extract_tool_calls_python_call_with_comma_in_string() {
        let text = "```tool_code\nprint(search_meetings(query=\"foo, bar\", limit=3))\n```";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(args["query"], "foo, bar");
        assert_eq!(args["limit"], 3);
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Captured {
        Content(String, bool),
    }

    fn run_filter(pieces: &[&str]) -> Vec<Captured> {
        let mut filter = StreamFilter::new();
        let mut out = Vec::new();
        let mut collect = |e: &InferenceEvent<'_>| match *e {
            InferenceEvent::ContentToken { text, is_thinking } => {
                out.push(Captured::Content(text.to_string(), is_thinking));
            }
            _ => {}
        };
        for p in pieces {
            filter.push(p, &mut collect);
        }
        filter.flush(&mut collect);
        out
    }

    fn collapsed(events: &[Captured]) -> (String, String) {
        let mut content = String::new();
        let mut think = String::new();
        for e in events {
            match e {
                Captured::Content(t, true) => think.push_str(t),
                Captured::Content(t, false) => content.push_str(t),
            }
        }
        (content, think)
    }

    #[test]
    fn filter_plain_text_passes_through() {
        let (c, t) = collapsed(&run_filter(&["hello", " ", "world"]));
        assert_eq!(c, "hello world");
        assert_eq!(t, "");
    }

    #[test]
    fn filter_strips_think_block_emits_as_thinking() {
        let (c, t) = collapsed(&run_filter(&["before<think>reasoning</think>after"]));
        assert_eq!(c, "beforeafter");
        assert_eq!(t, "reasoning");
    }

    #[test]
    fn filter_strips_tool_call_block_entirely() {
        let (c, t) = collapsed(&run_filter(&[
            "Sure!<tool_call>{\"name\":\"search_meetings\"}</tool_call>",
        ]));
        assert_eq!(c, "Sure!");
        assert_eq!(t, "");
    }

    #[test]
    fn filter_handles_tag_split_across_pieces() {
        let (c, t) = collapsed(&run_filter(&[
            "before<th", "ink>reason", "ing</thi", "nk>after",
        ]));
        assert_eq!(c, "beforeafter");
        assert_eq!(t, "reasoning");
    }

    #[test]
    fn filter_handles_tool_call_split_across_pieces() {
        let (c, _) = collapsed(&run_filter(&[
            "Let me search.<tool_", "call>{\"query\":\"foo\"}</tool", "_call>",
        ]));
        assert_eq!(c, "Let me search.");
    }

    #[test]
    fn filter_emits_partial_content_on_flush() {
        // Bare '<' is held in buffer until flush in case it starts a tag.
        let (c, _) = collapsed(&run_filter(&["x < y"]));
        assert_eq!(c, "x < y");
    }

    #[test]
    fn filter_handles_back_to_back_blocks() {
        let (c, t) = collapsed(&run_filter(&[
            "<think>a</think><tool_call>{}</tool_call>answer",
        ]));
        assert_eq!(c, "answer");
        assert_eq!(t, "a");
    }

    #[test]
    fn strips_empty_think_tags() {
        assert_eq!(strip_think_tags("<think></think>hello"), "hello");
    }

    #[test]
    fn strips_think_tags_with_content() {
        assert_eq!(
            strip_think_tags("<think>some reasoning</think>answer"),
            "answer"
        );
    }

    #[test]
    fn strips_multiple_think_blocks() {
        assert_eq!(
            strip_think_tags("<think>a</think>one<think>b</think>two"),
            "onetwo"
        );
    }

    #[test]
    fn no_think_tags_unchanged() {
        assert_eq!(strip_think_tags("just text"), "just text");
    }

    #[test]
    fn unclosed_think_tag_drops_rest() {
        assert_eq!(strip_think_tags("before<think>oops"), "before");
    }
}
