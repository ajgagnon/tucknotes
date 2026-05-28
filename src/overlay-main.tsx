import React from "react";
import ReactDOM from "react-dom/client";
import MeetingOverlay from "@/layout/MeetingOverlay";
import {
  applyTheme,
  getStoredTheme,
  listenForSystemChanges,
} from "@/features/theme/theme";
import "./overlay.css";

applyTheme(getStoredTheme());
listenForSystemChanges();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <MeetingOverlay />
  </React.StrictMode>,
);
