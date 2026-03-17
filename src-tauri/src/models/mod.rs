pub mod audio;
pub mod llm;
pub mod meeting_detection;
pub mod settings;

pub use audio::{
    AccumulatedAudio, AudioChunkEvent, AudioSource, PcmAccumulator, RecordingState,
    RecordingStateEvent, TranscriptEvent,
};
pub use llm::LlmModel;
pub use settings::{AppSettings, DownloadProgress, ModelInfo, WhisperModel};

/// Shared interface for downloadable model types (Whisper, LLM).
///
/// Both `WhisperModel` and `LlmModel` implement this trait, allowing
/// `model_manager` to use a single set of generic functions for
/// download, status checks, and path resolution.
pub trait Model: Clone + serde::Serialize + Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn filename(&self) -> &'static str;
    fn download_url(&self) -> &'static str;
    fn from_id(id: &str) -> Option<Self>;
    /// Event prefix used for download progress events (e.g. "model" or "llm-model").
    fn event_prefix() -> &'static str;
}
