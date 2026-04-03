import { useState, useEffect, useRef } from "react";
import { MoreVertical, Trash2, FolderOpen } from "lucide-react";

/** Menu label for revealing the model file in the system file manager. */
function revealInFolderMenuLabel(): string {
  if (typeof navigator === "undefined") return "Show in folder";
  const p = navigator.platform;
  if (p.startsWith("Mac") || p === "iPhone") return "Show in Finder";
  if (p.startsWith("Win")) return "Show in File Explorer";
  return "Show in folder";
}

/** Stops events from reaching the parent FieldLabel (label) so the radio does not toggle. */
function stopLabelBubbling(e: React.SyntheticEvent) {
  e.stopPropagation();
}

export function ModelActionsMenu({
  onShowInFolder,
  onRemove,
  canRemove,
}: {
  onShowInFolder: () => void;
  onRemove: () => void;
  /** When false, Remove is hidden (e.g. active model). */
  canRemove: boolean;
}) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  return (
    <div
      className="relative shrink-0"
      ref={menuRef}
      data-slot="model-actions-menu"
      onPointerDown={stopLabelBubbling}
      onClick={stopLabelBubbling}
    >
      <button
        type="button"
        onClick={(e) => {
          e.preventDefault();
          stopLabelBubbling(e);
          setOpen((prev) => !prev);
        }}
        className="p-1.5 rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label="Model options"
        aria-expanded={open}
      >
        <MoreVertical className="size-4" />
      </button>
      {open && (
        <div
          className="absolute right-0 top-full mt-1 bg-popover text-popover-foreground border border-border rounded-lg shadow-lg py-1 z-50 min-w-[180px]"
          onPointerDown={stopLabelBubbling}
          onClick={stopLabelBubbling}
        >
          {canRemove && (
            <button
              type="button"
              onClick={(e) => {
                e.preventDefault();
                stopLabelBubbling(e);
                setOpen(false);
                onRemove();
              }}
              className="flex items-center gap-2 w-full px-3 py-1.5 text-sm text-destructive hover:bg-destructive/10 transition-colors cursor-pointer text-left"
            >
              <Trash2 className="size-3.5 shrink-0" />
              Remove
            </button>
          )}
          <button
            type="button"
            onClick={(e) => {
              e.preventDefault();
              stopLabelBubbling(e);
              setOpen(false);
              onShowInFolder();
            }}
            className="flex items-center gap-2 w-full px-3 py-1.5 text-sm hover:bg-muted transition-colors cursor-pointer text-left"
          >
            <FolderOpen className="size-3.5 shrink-0 opacity-70" />
            {revealInFolderMenuLabel()}
          </button>
        </div>
      )}
    </div>
  );
}
