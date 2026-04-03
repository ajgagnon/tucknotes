import React from "react";
import ReactDOM from "react-dom/client";
import MeetingOverlay from "@/layout/MeetingOverlay";
import "./overlay.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <MeetingOverlay />
  </React.StrictMode>,
);
