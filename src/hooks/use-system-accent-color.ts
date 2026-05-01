import { useEffect } from "react";

export function useSystemAccentColor() {
  useEffect(() => {
    let unsubscribe: (() => void) | null = null;
    let cancelled = false;

    (async () => {
      try {
        const mod = await import("tauri-plugin-accent-color");
        if (cancelled) return;
        unsubscribe = mod.accentColor.subscribe((color) => {
          if (!color) return;
          const root = document.documentElement;
          root.style.setProperty("--primary", color);
          root.style.setProperty("--primary-foreground", "#ffffff");
          root.style.setProperty("--ring", color);
        });
      } catch {
        // Plugin unavailable (non-macOS, or permission/registration missing).
        // Fall back to the existing theme tokens.
      }
    })();

    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, []);
}
