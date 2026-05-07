import { useState } from "react";
import { Sun, Moon, Monitor } from "lucide-react";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  type Theme,
  getStoredTheme,
  setStoredTheme,
  applyTheme,
} from "@/features/theme";

export function AppearanceSection() {
  const [theme, setTheme] = useState<Theme>(getStoredTheme);

  return (
    <section>
      <h2 className="text-sm font-medium text-muted-foreground mb-4">
        Appearance
      </h2>
      <ToggleGroup
        variant="outline"
        value={[theme]}
        onValueChange={(newValue) => {
          const next = newValue.find((v) => v !== theme) as Theme | undefined;
          if (!next) return;
          setTheme(next);
          setStoredTheme(next);
          applyTheme(next);
        }}
      >
        <ToggleGroupItem value="light">
          <Sun className="size-4 mr-1" />
          Light
        </ToggleGroupItem>
        <ToggleGroupItem value="dark">
          <Moon className="size-4 mr-1" />
          Dark
        </ToggleGroupItem>
        <ToggleGroupItem value="system">
          <Monitor className="size-4 mr-1" />
          System
        </ToggleGroupItem>
      </ToggleGroup>
    </section>
  );
}
