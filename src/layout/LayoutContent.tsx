import { FileText } from "lucide-react";
import { SidebarInset, useSidebar } from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";
import MeetingsView, { type MeetingTitleInfo } from "@/features/meetings";
import SettingsView from "@/features/settings/SettingsView";
import { TemplateEditorView } from "@/features/settings/TemplateEditorView";
import { SETTINGS_SECTION_TEMPLATES } from "@/features/settings/TemplateSection";
import { MeetingHeaderTitle } from "./MeetingHeaderTitle";

export type ActiveView =
  | { type: "meeting"; id: string }
  | { type: "settings"; section?: string }
  | { type: "template-editor"; id?: string }
  | null;

/** Reusable title bar / window drag region. */
function PageHeader({
  left,
  right,
  onDrag,
}: {
  left?: React.ReactNode;
  right?: React.ReactNode;
  onDrag: (e: React.MouseEvent) => void;
}) {
  const { state } = useSidebar();
  const needsStoplightPadding = state === "collapsed";

  return (
    <div
      className="min-h-[48px] shrink-0 border-b border-muted-foreground/10"
      onMouseDown={onDrag}
    >
      <div className="flex items-center justify-between px-5 py-2 pr-2 h-full gap-2">
        <div
          className={cn(
            "flex items-center gap-1 transition-all duration-200 ease-in-out min-w-0 flex-1",
            needsStoplightPadding && "pl-[110px]",
          )}
        >
          {left}
        </div>
        <div className="flex items-center gap-2 shrink-0">{right}</div>
      </div>
    </div>
  );
}

/** Main content area: page header plus the view for the current selection. */
export function LayoutContent({
  activeView,
  onDrag,
  onStartRecording,
  onDeleteMeeting,
  onTitleChange,
  meetingInfo,
  onSaveTitle,
  onOpenSettings,
  onEditTemplate,
  onDirtyChange,
}: {
  activeView: ActiveView;
  onDrag: (e: React.MouseEvent) => void;
  onStartRecording: (meetingId: string) => void;
  onDeleteMeeting: (meetingId: string) => void;
  onTitleChange: (info: MeetingTitleInfo) => void;
  meetingInfo: MeetingTitleInfo | null;
  onSaveTitle: (title: string) => void;
  onOpenSettings: (section?: string) => void;
  onEditTemplate: (id?: string) => void;
  onDirtyChange: (dirty: boolean) => void;
}) {
  // Build header left content based on active view
  const headerLeft =
    activeView?.type === "meeting" ? (
      <MeetingHeaderTitle
        info={meetingInfo}
        onSave={onSaveTitle}
        meetingId={activeView.id}
        onDeleteMeeting={onDeleteMeeting}
      />
    ) : activeView?.type === "settings" ? (
      <h1 className="text-lg font-semibold m-0">Settings</h1>
    ) : activeView?.type === "template-editor" ? (
      <h1 className="text-lg font-semibold m-0">Templates</h1>
    ) : null;

  return (
    <SidebarInset className="min-h-0 overflow-hidden">
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-background border border-muted-foreground/10 md:rounded-2xl">
        <PageHeader left={headerLeft} onDrag={onDrag} />
        <div className="min-h-0 flex-1 overflow-auto">
          {activeView?.type === "meeting" && (
            <MeetingsView
              meetingId={activeView.id}
              onTitleChange={onTitleChange}
              onRecordingStarted={onStartRecording}
              onOpenSettings={onOpenSettings}
            />
          )}
          {activeView?.type === "settings" && (
            <SettingsView
              section={activeView.section}
              onEditTemplate={onEditTemplate}
            />
          )}
          {activeView?.type === "template-editor" && (
            <TemplateEditorView
              templateId={activeView.id}
              onDone={() => onOpenSettings(SETTINGS_SECTION_TEMPLATES)}
              onDirtyChange={onDirtyChange}
            />
          )}
          {activeView === null && (
            <div className="flex h-full flex-col items-center justify-center p-8 text-center">
              <FileText className="mb-4 h-12 w-12 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">
                No meetings yet. Start a recording to create your first meeting.
              </p>
            </div>
          )}
        </div>
      </div>
    </SidebarInset>
  );
}
