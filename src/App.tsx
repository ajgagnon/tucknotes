import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import PermissionSetup from "./components/PermissionSetup";
import RecordingView from "./components/RecordingView";
import "./App.css";

function App() {
  const [permissionsReady, setPermissionsReady] = useState<boolean | null>(null);

  useEffect(() => {
    Promise.all([
      invoke<boolean>("check_screen_recording_permission"),
      invoke<string>("check_microphone_permission"),
    ])
      .then(([screen, mic]) => {
        setPermissionsReady(screen && mic === "authorized");
      })
      .catch(() => {
        setPermissionsReady(true);
      });
  }, []);

  if (permissionsReady === null) return null;

  if (!permissionsReady) {
    return <PermissionSetup onComplete={() => setPermissionsReady(true)} />;
  }

  return <RecordingView />;
}

export default App;
