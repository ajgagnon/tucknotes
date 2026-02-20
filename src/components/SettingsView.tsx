import { Settings } from "lucide-react";

function SettingsView() {
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <Settings className="w-12 h-12 text-muted-foreground mb-4" />
      <h1 className="text-xl font-semibold mb-2">Settings</h1>
      <p className="text-sm text-muted-foreground">
        Settings will be available here soon.
      </p>
    </div>
  );
}

export default SettingsView;
