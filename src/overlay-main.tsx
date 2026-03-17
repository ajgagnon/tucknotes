import React from "react";
import ReactDOM from "react-dom/client";
import MeetingOverlay from "./components/MeetingOverlay";
import "./App.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <MeetingOverlay />
  </React.StrictMode>,
);
