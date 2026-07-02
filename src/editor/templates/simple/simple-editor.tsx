"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import {
  EditorContent,
  EditorContext,
  useEditor,
  type Editor,
} from "@tiptap/react"

// --- Tiptap Core Extensions ---
import { Markdown } from "@tiptap/markdown"
import { StarterKit } from "@tiptap/starter-kit"
import {
  MeetingNoteElapsed,
  MeetingNoteHeading,
  MeetingNoteParagraph,
} from "@/editor/extensions/meeting-note-elapsed"
import { SummaryHoverHighlight } from "@/editor/extensions/summary-hover-highlight"
import { Image } from "@tiptap/extension-image"
import { TaskItem, TaskList } from "@tiptap/extension-list"
import { TextAlign } from "@tiptap/extension-text-align"
import { Typography } from "@tiptap/extension-typography"
import { Highlight } from "@tiptap/extension-highlight"
import { Subscript } from "@tiptap/extension-subscript"
import { Superscript } from "@tiptap/extension-superscript"
import { Selection } from "@tiptap/extensions"

// --- UI Primitives ---
import { Button } from "@/editor/primitives/button"
import {
  Toolbar,
  ToolbarGroup,
  ToolbarSeparator,
} from "@/editor/primitives/toolbar"

// --- Tiptap Node ---
import { HorizontalRule } from "@/editor/nodes/horizontal-rule-node/horizontal-rule-node-extension"
import "@/editor/nodes/blockquote-node/blockquote-node.scss"
import "@/editor/nodes/code-block-node/code-block-node.scss"
import "@/editor/nodes/horizontal-rule-node/horizontal-rule-node.scss"
import "@/editor/nodes/list-node/list-node.scss"
import "@/editor/nodes/image-node/image-node.scss"
import "@/editor/nodes/heading-node/heading-node.scss"
import "@/editor/nodes/paragraph-node/paragraph-node.scss"

// --- Tiptap UI ---
import { HeadingDropdownMenu } from "@/editor/ui/heading-dropdown-menu"
import { ListDropdownMenu } from "@/editor/ui/list-dropdown-menu"
import { BlockquoteButton } from "@/editor/ui/blockquote-button"
import { CodeBlockButton } from "@/editor/ui/code-block-button"
import {
  ColorHighlightPopover,
  ColorHighlightPopoverContent,
  ColorHighlightPopoverButton,
} from "@/editor/ui/color-highlight-popover"
import {
  LinkPopover,
  LinkContent,
  LinkButton,
} from "@/editor/ui/link-popover"
import { MarkButton } from "@/editor/ui/mark-button"
import { TextAlignButton } from "@/editor/ui/text-align-button"
import { UndoRedoButton } from "@/editor/ui/undo-redo-button"

// --- Icons ---
import { ArrowLeftIcon, HighlighterIcon, LinkIcon } from "lucide-react"

// --- Hooks ---
import { useIsBreakpoint } from "@/hooks/use-is-breakpoint"
import { useWindowSize } from "@/hooks/use-window-size"
import { useCursorVisibility } from "@/editor/use-cursor-visibility"

// --- Styles ---
import "@/editor/templates/simple/simple-editor.scss"

const MainToolbarContent = ({
  onHighlighterClick,
  onLinkClick,
  isMobile,
}: {
  onHighlighterClick: () => void
  onLinkClick: () => void
  isMobile: boolean
}) => {
  return (
    <>
      <ToolbarGroup>
        <UndoRedoButton action="undo" />
        <UndoRedoButton action="redo" />
      </ToolbarGroup>

      <ToolbarSeparator />

      <ToolbarGroup>
        <HeadingDropdownMenu modal={false} levels={[1, 2, 3, 4]} />
        <ListDropdownMenu
          modal={false}
          types={["bulletList", "orderedList", "taskList"]}
        />
        <BlockquoteButton />
        <CodeBlockButton />
      </ToolbarGroup>

      <ToolbarSeparator />

      <ToolbarGroup>
        <MarkButton type="bold" />
        <MarkButton type="italic" />
        <MarkButton type="strike" />
        <MarkButton type="code" />
        <MarkButton type="underline" />
        {!isMobile ? (
          <ColorHighlightPopover />
        ) : (
          <ColorHighlightPopoverButton onClick={onHighlighterClick} />
        )}
        {!isMobile ? <LinkPopover /> : <LinkButton onClick={onLinkClick} />}
      </ToolbarGroup>

      <ToolbarSeparator />

      <ToolbarGroup>
        <MarkButton type="superscript" />
        <MarkButton type="subscript" />
      </ToolbarGroup>

      <ToolbarSeparator />

      <ToolbarGroup>
        <TextAlignButton align="left" />
        <TextAlignButton align="center" />
        <TextAlignButton align="right" />
        <TextAlignButton align="justify" />
      </ToolbarGroup>

    </>
  )
}

const MobileToolbarContent = ({
  type,
  onBack,
}: {
  type: "highlighter" | "link"
  onBack: () => void
}) => (
  <>
    <ToolbarGroup>
      <Button variant="ghost" onClick={onBack}>
        <ArrowLeftIcon className="tiptap-button-icon" />
        {type === "highlighter" ? (
          <HighlighterIcon className="tiptap-button-icon" />
        ) : (
          <LinkIcon className="tiptap-button-icon" />
        )}
      </Button>
    </ToolbarGroup>

    <ToolbarSeparator />

    {type === "highlighter" ? (
      <ColorHighlightPopoverContent />
    ) : (
      <LinkContent />
    )}
  </>
)

export type MeetingNoteEditorConfig = {
  /** Seconds to stamp on new blocks; `null` disables stamping. */
  stampElapsedSecs: number | null
}

export type SimpleEditorProps = {
  /** Initial content as markdown (`null` for an empty document). */
  initialMarkdown: string | null
  /** Fired with the full document as markdown after edits (serialized on a short debounce; parent may debounce persistence further). */
  onMarkdownChange?: (markdown: string) => void
  /** Meeting notes: elapsed stamping + timestamp UI (only used from `MeetingNotesEditor`). */
  meetingNote?: MeetingNoteEditorConfig | null
  /** Load the per-block hover-highlight plugin (AI summary only). */
  summaryHover?: boolean
  /** Surfaces the editor instance to the parent (e.g. to drive hover decorations). */
  onEditorReady?: (editor: Editor | null) => void
}

export function SimpleEditor({
  initialMarkdown,
  onMarkdownChange,
  meetingNote = null,
  summaryHover = false,
  onEditorReady,
}: SimpleEditorProps) {
  const isMobile = useIsBreakpoint()
  const { height } = useWindowSize()
  const [mobileView, setMobileView] = useState<"main" | "highlighter" | "link">(
    "main"
  )
  const toolbarRef = useRef<HTMLDivElement>(null)

  const meetingNoteRef = useRef(meetingNote)
  meetingNoteRef.current = meetingNote

  const onEditorReadyRef = useRef(onEditorReady)
  onEditorReadyRef.current = onEditorReady

  /** Stable identity: parent often passes a new object when `stampElapsedSecs` ticks; extension list must not rebuild or the editor remounts and focus is lost. */
  const meetingNoteMode = meetingNote != null

  /** Full-doc `getMarkdown()` is expensive; debounce so it does not run on the typing critical path. */
  const markdownEmitDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const onMarkdownChangeRef = useRef(onMarkdownChange)
  onMarkdownChangeRef.current = onMarkdownChange

  const extensions = useMemo(() => {
    const baseStarter = StarterKit.configure({
      horizontalRule: false,
      paragraph: meetingNoteMode ? false : undefined,
      heading: meetingNoteMode ? false : undefined,
      link: {
        openOnClick: false,
        enableClickSelection: true,
      },
    })

    const meetingExtensions = meetingNoteMode
      ? [
          MeetingNoteParagraph,
          MeetingNoteHeading,
          MeetingNoteElapsed.configure({
            getElapsedSecs: () => meetingNoteRef.current?.stampElapsedSecs ?? null,
          }),
        ]
      : []

    return [
      baseStarter,
      ...meetingExtensions,
      HorizontalRule,
      TextAlign.configure({ types: ["heading", "paragraph"] }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Highlight.configure({ multicolor: true }),
      Image,
      Typography,
      Superscript,
      Subscript,
      Selection,
      Markdown,
      ...(summaryHover ? [SummaryHoverHighlight] : []),
    ]
  }, [meetingNoteMode, summaryHover])

  const editor = useEditor(
    {
      immediatelyRender: false,
      autofocus: "end",
      editorProps: {
        attributes: {
          autocomplete: "off",
          autocorrect: "off",
          autocapitalize: "off",
          "aria-label": "Main content area, start typing to enter text.",
          class: "simple-editor",
        },
      },
      extensions,
      content: initialMarkdown ?? "",
      contentType: "markdown",
      onUpdate: ({ editor: ed }) => {
        const emit = onMarkdownChangeRef.current
        if (!emit) return
        if (markdownEmitDebounceRef.current) {
          clearTimeout(markdownEmitDebounceRef.current)
        }
        markdownEmitDebounceRef.current = setTimeout(() => {
          markdownEmitDebounceRef.current = null
          emit(ed.getMarkdown())
        }, 300)
      },
    },
    [extensions],
  )

  useEffect(() => {
    return () => {
      // Flush only a pending (debounced) change on unmount. Emitting
      // unconditionally would round-trip the untouched document through the
      // markdown serializer, and the normalized output reads as an edit to
      // consumers — overwriting externally-updated bodies (live minutes).
      if (!markdownEmitDebounceRef.current) return
      clearTimeout(markdownEmitDebounceRef.current)
      markdownEmitDebounceRef.current = null
      const emit = onMarkdownChangeRef.current
      if (editor && !editor.isDestroyed && emit) {
        emit(editor.getMarkdown())
      }
    }
  }, [editor])

  useEffect(() => {
    onEditorReadyRef.current?.(editor ?? null)
  }, [editor])

  const rect = useCursorVisibility({
    editor,
    overlayHeight: toolbarRef.current?.getBoundingClientRect().height ?? 0,
  })

  useEffect(() => {
    if (!isMobile && mobileView !== "main") {
      setMobileView("main")
    }
  }, [isMobile, mobileView])

  return (
    <div className="simple-editor-wrapper">
      <EditorContext.Provider value={{ editor }}>
        <Toolbar
          ref={toolbarRef}
          style={{
            ...(isMobile
              ? {
                  bottom: `calc(100% - ${height - rect.y}px)`,
                }
              : {}),
          }}
        >
          {mobileView === "main" ? (
            <MainToolbarContent
              onHighlighterClick={() => setMobileView("highlighter")}
              onLinkClick={() => setMobileView("link")}
              isMobile={isMobile}
            />
          ) : (
            <MobileToolbarContent
              type={mobileView === "highlighter" ? "highlighter" : "link"}
              onBack={() => setMobileView("main")}
            />
          )}
        </Toolbar>

        <EditorContent
          editor={editor}
          role="presentation"
          className="simple-editor-content"
        />
      </EditorContext.Provider>
    </div>
  )
}
