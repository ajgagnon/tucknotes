export type Theme = "light" | "dark" | "system";

const STORAGE_KEY = "theme";

const mq = window.matchMedia("(prefers-color-scheme: dark)");

export function getStoredTheme(): Theme {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "dark" || stored === "system") {
    return stored;
  }
  return "system";
}

export function setStoredTheme(theme: Theme) {
  localStorage.setItem(STORAGE_KEY, theme);
}

export function applyTheme(theme: Theme) {
  const isDark =
    theme === "dark" || (theme === "system" && mq.matches);
  document.documentElement.classList.toggle("dark", isDark);
  syncNativeTheme(theme);
}

/** Tell Tauri to set the window's native appearance so vibrancy follows. */
function syncNativeTheme(theme: Theme) {
  if ("__TAURI_INTERNALS__" in window) {
    import("@tauri-apps/api/core").then(({ invoke }) => {
      invoke("set_app_theme", { theme });
    });
  }
}

/** Listen for OS theme changes; only applies when stored theme is "system". */
export function listenForSystemChanges() {
  const handler = () => {
    if (getStoredTheme() === "system") {
      applyTheme("system");
    }
  };
  mq.addEventListener("change", handler);
  return () => mq.removeEventListener("change", handler);
}
