import { invoke } from "@tauri-apps/api/core";
import { type AppError } from "@/hooks/useRecording";

export function RecordingErrorBanner({ error }: { error: AppError }) {
  return (
    <div className="bg-red-50 border border-red-200 text-red-700 rounded-lg py-3 px-4 text-sm mb-4 text-center dark:bg-danger/10 dark:border-danger/25 dark:text-red-300">
      {error.kind === "PermissionDenied" ? (
        <>
          <p className="m-0 mb-2 font-medium">
            Permission needed to capture audio
          </p>
          <p className="m-0 mb-3 text-xs text-red-500 dark:text-red-400">
            Enable Screen Recording in macOS settings to get started.
          </p>
          <button
            type="button"
            className="border-[1.5px] border-red-300 dark:border-red-400/50 text-red-700 dark:text-red-300 bg-transparent rounded-lg py-1.5 px-4 text-xs font-semibold cursor-pointer transition-all duration-200 hover:bg-red-100 dark:hover:bg-red-400/10"
            onClick={() => invoke("open_screen_recording_settings")}
          >
            Open System Settings
          </button>
        </>
      ) : (
        error.message
      )}
    </div>
  );
}
