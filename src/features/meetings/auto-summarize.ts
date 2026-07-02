import { invoke } from "@tauri-apps/api/core";
import { type MeetingDetail, summaryBodyFromDocuments } from "./types";

/// Kicks off summarization for a just-finalized recording when there's no
/// existing summary and an LLM model is downloaded. Called at the layout level
/// so it fires regardless of which view (or window) is currently focused.
export async function autoSummarizeIfNeeded(meetingId: string): Promise<void> {
  try {
    const detail = await invoke<MeetingDetail>("get_meeting", { meetingId });
    if (summaryBodyFromDocuments(detail.documents)) return;

    const selected = await invoke<string | null>("get_selected_llm_model");
    if (!selected) return;
    const ready = await invoke<boolean>("get_llm_model_status", {
      modelId: selected,
    });
    if (!ready) return;

    await invoke<string>("summarize_meeting", { meetingId });
  } catch (e) {
    // Already in progress / queued — not an error worth surfacing here.
    console.debug("autoSummarizeIfNeeded:", e);
  }
}
