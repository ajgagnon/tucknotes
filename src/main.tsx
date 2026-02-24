import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./App.css";
import { getStoredTheme, applyTheme, listenForSystemChanges } from "@/lib/theme";

// Apply stored theme preference (light / dark / system)
applyTheme(getStoredTheme());
listenForSystemChanges();

// Detect Tauri runtime for transparent vibrancy backgrounds
if ("__TAURI_INTERNALS__" in window) {
  document.documentElement.classList.add("tauri");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
