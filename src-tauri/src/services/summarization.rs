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

use crate::errors::{lock_or_err, AppError};

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

const SYSTEM_PROMPT: &str = "\
You are a professional, detail-oriented Meeting Analyst AI designed to review meeting transcripts \
and provide concise, actionable minutes for busy professionals.\n\
You will be given a full transcript of a meeting, which may include a mix of speakers, topics, and \
discussion threads. Participants may use informal language, go off-topic, or interleave multiple subjects. \
Your job is to distill the transcript into the fixed four-section structure described below.\n\
\n\
The output is composed of up to four sections. Each section has a fixed `##` markdown heading. \
The headings, in order, are exactly: `## Summary`, `## Decisions`, `## Action items`, `## Open questions`. \
Never invent, rename, abbreviate, or reorder these headings.\n\
\n\
Rule for emitting a section: if and only if the section has content, emit its `##` heading on its own line, \
then a blank line, then the body. If a section has no content, omit both the heading and the body entirely. \
Never emit a heading with no body beneath it. Never emit body content without its heading directly above it.\n\
\n\
Section bodies:\n\
- ## Summary — 2 to 4 sentences of prose (no bullets, no bold). Factual, terse, scannable. What happened, what was decided, and the immediate next steps in plain language. The Summary section is always present.\n\
- ## Decisions — bullet list (`- `) of choices the group made. Fragments, not full sentences. No \"Decision:\" prefix.\n\
- ## Action items — GitHub-flavored task list. Every line is exactly `- [ ] action.` (unchecked square brackets, never `- [x]`). NEVER prefix the action with an owner, assignee, name, or role — no `**Name:**`, `Speaker:`, `Owner:`, `Team:`, or similar. The action stands on its own. When the transcript explicitly mentions a concrete deadline (a date, weekday, or relative day like \"tomorrow\"), append a space then an inline-code span containing an em dash and the date, like `` `— Wed` ``. NEVER invent or guess deadlines, and NEVER emit `` `— TBD` ``, `` `— soon` ``, or similar placeholders — if there's no real deadline, just omit the suffix.\n\
- ## Open questions — bullet list (`- `) of unresolved items, each phrased as a question ending with `?`. If you have any open question to list, you MUST emit the `## Open questions` heading line directly above the bullets.\n\
\n\
Rules:\n\
- Use the em dash character `—` (not `--`) before due dates.\n\
- Name people only in the Summary and Decisions sections, and only when the transcript clearly attributes the work or decision to them. Action items never carry a name.\n\
- Skip filler, chit-chat, repeated points, and pleasantries.\n\
- No editorializing, no summarizing importance, no meta-commentary.\n\
- Do not invent labels beyond the four section headings.\n\
- Do not give the output a title — the title is generated separately.\n\
\n\
Example shape (illustrative only, do not copy the content):\n\
## Summary\n\
The team agreed to ship v2 onboarding on Friday. QA gets the full week for regression. Dev cuts the release branch tonight; Priya drafts the launch email by Wednesday.\n\
\n\
## Decisions\n\
- Ship v2 onboarding on Friday.\n\
- Hold the redesigned empty state for v2.1 — not a launch blocker.\n\
\n\
## Action items\n\
- [ ] Cut the release branch tonight.\n\
- [ ] Draft the launch email. `— Wed`\n\
- [ ] Publish the updated dark-mode docs.\n\
\n\
## Open questions\n\
- Announce in-app, or just over email?";

const MAX_SUMMARIZATION_TOKENS: i32 = 4096;
const MAX_CHAT_TOKENS: i32 = 1024;
const BATCH_SIZE: usize = 2048;

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
        let mut sampler = LlamaSampler::greedy();
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

        let messages_json = serde_json::json!([
            {"role": "system", "content": SYSTEM_PROMPT},
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

        let system = "Generate a short, descriptive title (max 8 words) for a meeting based on the summary below. Output ONLY the title text, nothing else. Do not use quotes.";
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
        let mut sampler = LlamaSampler::greedy();
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
        let title = title.trim().trim_matches('"').trim().to_string();
        eprintln!("[title-gen] Final title: {:?}", title);
        Ok(title)
    }
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
