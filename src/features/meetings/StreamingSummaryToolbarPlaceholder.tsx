import {
  AlignCenterIcon,
  AlignJustifyIcon,
  AlignLeftIcon,
  AlignRightIcon,
  TextQuoteIcon as BlockquoteIcon,
  BoldIcon,
  ChevronDownIcon,
  CodeXmlIcon as Code2Icon,
  SquareCodeIcon as CodeBlockIcon,
  HeadingIcon,
  HighlighterIcon,
  ItalicIcon,
  LinkIcon,
  ListIcon,
  Redo2Icon,
  StrikethroughIcon as StrikeIcon,
  SubscriptIcon,
  SuperscriptIcon,
  UnderlineIcon,
  Undo2Icon,
} from "lucide-react"

type IconComp = React.FC<React.SVGProps<SVGSVGElement>>

function PlaceholderButton({
  Icon,
  withChevron,
  label,
}: {
  Icon: IconComp
  withChevron?: boolean
  label: string
}) {
  return (
    <button
      type="button"
      className="tiptap-button"
      data-style="ghost"
      data-disabled
      disabled
      tabIndex={-1}
      aria-label={label}
    >
      <Icon className="tiptap-button-icon" />
      {withChevron && (
        <ChevronDownIcon className="tiptap-button-dropdown-small" />
      )}
    </button>
  )
}

function Group({ children }: { children: React.ReactNode }) {
  return (
    <div role="group" className="tiptap-toolbar-group">
      {children}
    </div>
  )
}

function Sep() {
  return (
    <div
      role="none"
      className="tiptap-separator"
      data-orientation="vertical"
    />
  )
}

export function StreamingSummaryToolbarPlaceholder() {
  return (
    <div
      role="toolbar"
      aria-label="toolbar"
      aria-hidden
      data-variant="fixed"
      className="tiptap-toolbar"
    >
      <Group>
        <PlaceholderButton Icon={Undo2Icon} label="Undo" />
        <PlaceholderButton Icon={Redo2Icon} label="Redo" />
      </Group>
      <Sep />
      <Group>
        <PlaceholderButton Icon={HeadingIcon} withChevron label="Heading" />
        <PlaceholderButton Icon={ListIcon} withChevron label="List" />
        <PlaceholderButton Icon={BlockquoteIcon} label="Blockquote" />
        <PlaceholderButton Icon={CodeBlockIcon} label="Code block" />
      </Group>
      <Sep />
      <Group>
        <PlaceholderButton Icon={BoldIcon} label="Bold" />
        <PlaceholderButton Icon={ItalicIcon} label="Italic" />
        <PlaceholderButton Icon={StrikeIcon} label="Strikethrough" />
        <PlaceholderButton Icon={Code2Icon} label="Code" />
        <PlaceholderButton Icon={UnderlineIcon} label="Underline" />
        <PlaceholderButton Icon={HighlighterIcon} label="Highlight" />
        <PlaceholderButton Icon={LinkIcon} label="Link" />
      </Group>
      <Sep />
      <Group>
        <PlaceholderButton Icon={SuperscriptIcon} label="Superscript" />
        <PlaceholderButton Icon={SubscriptIcon} label="Subscript" />
      </Group>
      <Sep />
      <Group>
        <PlaceholderButton Icon={AlignLeftIcon} label="Align left" />
        <PlaceholderButton Icon={AlignCenterIcon} label="Align center" />
        <PlaceholderButton Icon={AlignRightIcon} label="Align right" />
        <PlaceholderButton Icon={AlignJustifyIcon} label="Align justify" />
      </Group>
    </div>
  )
}
