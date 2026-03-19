use std::path::{Path, PathBuf};
use std::sync::Mutex;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::openai::OpenAIChatTemplateParams;
use llama_cpp_2::sampling::LlamaSampler;

use crate::errors::{lock_or_err, AppError};

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

const SYSTEM_PROMPT: &str = "\
You are a professional, detail-oriented Meeting Analyst AI designed to review meeting transcripts \
and provide comprehensive, clear minutes for effective follow-through. Your outputs must be concise, \
organized, and actionable for busy professionals who require only the essential information to maximize \
team productivity and accountability.\n\
You will be given a full transcript of a meeting, which may include a mix of speakers, topics, and \
discussion threads. Participants may use informal language, go off-topic, or interleave multiple subjects. \
Your job is to distill the transcript into a highly organized, digestible report that is chronologically organized.\n\
Write abbreviated bullet points, not full sentences. Be terse and scannable.\n\
Ideal line length is 6 words or less.\n\
\n\
Rules:\n\
- Group bullets under short topic headings.\n\
- Each bullet point should have nested sub-points.\n\
- Use fragments, not complete sentences\n\
- Use sub-bullets for details, numbered lists for sequences/steps\n\
- Name people only when assigning work or decisions\n\
- Skip filler, chit-chat, repeated points, and pleasantries\n\
- No labels like \"Status:\", \"Action:\", \"Process:\"\n\
- No editorializing or summarizing importance\n\
- Do not give a title to the summary, this is generated separately. \n\
\n\
Format:\n\
## Topic Heading\n\
* Key point or decision\n    - Detail or sub-point\n    - Another sub-point\n\
* Next point\n\
\n\
Next Topic Heading\n\
* ...";

const MAX_TOKENS: i32 = 4096;

/// Wraps a lazily-loaded llama.cpp model and exposes a blocking
/// `summarize()` method. The model is loaded on the first call
/// and reused for every subsequent call. If the model path changes
/// (e.g. user selects a different model), it is reloaded automatically.
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

    /// Apply the model's built-in Jinja chat template with the given
    /// `enable_thinking` flag. This uses the template baked into the GGUF
    /// rather than manually constructing prompt strings.
    fn apply_template(
        model: &LlamaModel,
        system: &str,
        user: &str,
        enable_thinking: bool,
    ) -> Result<String, AppError> {
        let tmpl = model.chat_template(None).map_err(|e| {
            AppError::SummarizationFailed(format!("Failed to get chat template: {e}"))
        })?;

        // Build OpenAI-compatible messages JSON
        let messages_json = serde_json::json!([
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ])
        .to_string();

        let params = OpenAIChatTemplateParams {
            messages_json: &messages_json,
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

    /// Run summarization on the given transcript text.
    ///
    /// Calls `on_token(text, is_thinking)` for each generated token so
    /// callers can stream results to the frontend. The `is_thinking` flag
    /// is `true` for tokens inside `<think>...</think>` blocks.
    /// Returns the complete summary text (with thinking stripped).
    ///
    /// **Blocking** -- call from `spawn_blocking`.
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    pub fn summarize<F>(
        &self,
        model_path: &Path,
        transcript: &str,
        mut on_token: F,
    ) -> Result<String, AppError>
    where
        F: FnMut(&str, bool),
    {
        self.ensure_loaded(model_path)?;

        let guard = lock_or_err(&self.model)?;
        let (_, model) = guard
            .as_ref()
            .ok_or_else(|| AppError::SummarizationFailed("Model not loaded".into()))?;

        let prompt = Self::apply_template(model, SYSTEM_PROMPT, transcript, false)?;
        let tokens_list = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| AppError::SummarizationFailed(format!("Tokenization failed: {e}")))?;

        let n_input = tokens_list.len() as u32;
        let n_ctx = (n_input + MAX_TOKENS as u32 + 256).max(4096);

        // Use a batch size large enough for chunked prompt ingestion.
        // We process the prompt in chunks of BATCH_SIZE tokens to avoid
        // allocating one enormous batch for long transcripts.
        const BATCH_SIZE: usize = 2048;

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(n_ctx))
            .with_flash_attention_policy(llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_AUTO);
        let mut ctx = model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| AppError::SummarizationFailed(format!("Context creation failed: {e}")))?;

        let mut batch = LlamaBatch::new(BATCH_SIZE, 1);

        // Feed prompt tokens in chunks
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

        // n_cur tracks the absolute position in the context (prompt + generated)
        let mut n_cur = n_prompt as i32;
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut sampler = LlamaSampler::greedy();
        let mut output = String::new();

        // Track <think> tags so we can separate thinking from answer in the stream.
        let mut inside_think = false;
        // Buffer for partial tag detection (e.g. we see "<thi" and need to
        // wait for more tokens before deciding whether to emit or suppress).
        let mut tag_buf = String::new();

        while n_cur < n_prompt as i32 + MAX_TOKENS {
            // Sample from the last logit position in the most recent batch
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
            eprint!("{}", piece); // DEBUG: show raw model output

            // Stream filtering: strip <think>...</think> blocks, only emit
            // answer content to the frontend. Thinking tokens are silently
            // discarded (they're also stripped from the final DB output).
            tag_buf.push_str(&piece);
            loop {
                if inside_think {
                    if let Some(end) = tag_buf.find(THINK_CLOSE) {
                        // Discard thinking content, keep anything after the tag
                        tag_buf = tag_buf[(end + THINK_CLOSE.len())..].to_string();
                        inside_think = false;
                    } else if tag_buf.contains('<') {
                        // Might be a partial "</think>" — discard up to '<', keep rest
                        let lt = tag_buf.rfind('<').unwrap();
                        tag_buf = tag_buf[lt..].to_string();
                        break;
                    } else {
                        // Still inside think block — discard everything
                        tag_buf.clear();
                        break;
                    }
                } else if let Some(start) = tag_buf.find(THINK_OPEN) {
                    // Emit everything before <think> as answer content
                    let before = &tag_buf[..start];
                    if !before.is_empty() {
                        on_token(before, false);
                    }
                    tag_buf = tag_buf[start + THINK_OPEN.len()..].to_string();
                    inside_think = true;
                } else if tag_buf.contains('<') {
                    // Might be a partial "<think>" tag starting; emit
                    // everything before the '<' and keep the rest buffered.
                    let lt = tag_buf.rfind('<').unwrap();
                    let before = &tag_buf[..lt];
                    if !before.is_empty() {
                        on_token(before, false);
                    }
                    tag_buf = tag_buf[lt..].to_string();
                    break;
                } else {
                    // No tags at all — flush the buffer as answer content
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

        // Flush any remaining buffered text (only if not inside a think block)
        if !tag_buf.is_empty() && !inside_think {
            on_token(&tag_buf, false);
        }

        // Also strip from the final output for DB storage
        let cleaned = strip_think_tags(output.trim());
        Ok(cleaned.trim().to_string())
    }

    /// Generate a short title for a meeting from its summary.
    ///
    /// Uses the same model as `summarize()` (already loaded) with a tiny
    /// context and a small token budget. **Blocking** -- call from `spawn_blocking`.
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

        let system = "Generate a short, descriptive title (max 8 words) for a meeting based on the summary below. Output ONLY the title text, nothing else. Do not use quotes.";
        let prompt = Self::apply_template(model, system, summary, false)?;

        let tokens_list = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| AppError::SummarizationFailed(format!("Tokenization failed: {e}")))?;

        let n_input = tokens_list.len() as u32;
        let max_title_tokens: i32 = 512;
        let n_ctx = (n_input + max_title_tokens as u32 + 64).max(512);

        const BATCH_SIZE: usize = 2048;

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
        let mut sampler = LlamaSampler::greedy();
        let mut output = String::new();
        // Track think blocks incrementally to avoid O(n^2) scans
        let mut inside_think = false;
        // Accumulates only non-thinking content for newline detection
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

            // Track <think> blocks incrementally
            if !inside_think && piece.contains(THINK_OPEN) {
                inside_think = true;
            }
            if inside_think && output.ends_with(THINK_CLOSE) {
                inside_think = false;
            }

            // Accumulate non-thinking content for newline detection
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
        let title = title.trim().trim_matches('"').trim().to_string();
        eprintln!("[title-gen] Final title: {:?}", title);
        Ok(title)
    }
}

/// Strip `<think>...</think>` blocks from model output.
/// Qwen3 may emit these even with `/no_think`.
fn strip_think_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut remaining = s;
    while let Some(start) = remaining.find(THINK_OPEN) {
        result.push_str(&remaining[..start]);
        if let Some(end) = remaining[start..].find(THINK_CLOSE) {
            remaining = &remaining[(start + end + THINK_CLOSE.len())..];
        } else {
            // Unclosed tag — drop everything from <think> onward
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
}

#[cfg(test)]
mod tests {
    use super::strip_think_tags;

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
