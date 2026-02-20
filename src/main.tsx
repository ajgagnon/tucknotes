import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./App.css";

// Sync dark mode class with OS preference
const mq = window.matchMedia("(prefers-color-scheme: dark)");
document.documentElement.classList.toggle("dark", mq.matches);
mq.addEventListener("change", (e) =>
  document.documentElement.classList.toggle("dark", e.matches),
);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
