import type { Editor } from "@tiptap/react";
import { useCurrentEditor, useEditorState } from "@tiptap/react";
import { useEffect, useState } from "react";

function getActivePageEditor(editor: Editor): Editor | null {
  const storage = editor.storage as unknown as Record<string, unknown>;
  const pages = storage.pages as { activeEditor?: Editor | null } | undefined;
  if (!pages || !("activeEditor" in pages)) return null;
  return pages.activeEditor ?? null;
}

export function useTiptapEditor(providedEditor?: Editor | null): {
  editor: Editor | null;
} {
  const { editor: coreEditor } = useCurrentEditor();
  const mainEditor = providedEditor ?? coreEditor;

  const [storageEditor, setStorageEditor] = useState<Editor | null>(null);

  useEffect(() => {
    if (!mainEditor) {
      setStorageEditor(null);
      return;
    }

    const updateHandler = () =>
      setStorageEditor(getActivePageEditor(mainEditor));

    updateHandler();

    mainEditor.on("update", updateHandler);
    mainEditor.on("selectionUpdate", updateHandler);

    return () => {
      mainEditor.off("update", updateHandler);
      mainEditor.off("selectionUpdate", updateHandler);
    };
  }, [mainEditor]);

  useEffect(() => {
    if (!storageEditor) return;

    const handleDestroy = () => setStorageEditor(null);

    storageEditor.on("destroy", handleDestroy);
    return () => {
      storageEditor.off("destroy", handleDestroy);
    };
  }, [storageEditor]);

  const editorState = useEditorState({
    editor: storageEditor ?? mainEditor,
    selector(context) {
      if (!context.editor) {
        return { editor: null as Editor | null, txn: 0 };
      }
      return {
        editor: context.editor,
        txn: context.transactionNumber,
      };
    },
    equalityFn(a, b) {
      if (a === b) return true;
      if (!a || !b) return a === b;
      if (!a.editor || !b.editor) return false;
      if (a.editor !== b.editor) return false;
      return a.txn === b.txn;
    },
  });

  return editorState ?? { editor: null };
}
