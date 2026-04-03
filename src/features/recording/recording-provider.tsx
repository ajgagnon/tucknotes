import type { ReactNode } from "react";
import { AudioLevelProvider } from "./audio-level-provider";
import { RecordingProviderInner } from "./recording-provider-inner";

export function RecordingProvider({ children }: { children: ReactNode }) {
  return (
    <RecordingProviderInner>
      <AudioLevelProvider>{children}</AudioLevelProvider>
    </RecordingProviderInner>
  );
}
