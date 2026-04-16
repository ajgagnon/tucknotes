import { Extension, mergeAttributes } from "@tiptap/core";
import { Heading } from "@tiptap/extension-heading";
import { Paragraph } from "@tiptap/extension-paragraph";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import {
  appendMeetingNoteElapsedTransaction,
  MEETING_NOTE_ELAPSED_ATTR,
} from "./meeting-note-elapsed-logic";

export { MEETING_NOTE_ELAPSED_ATTR };

const ATTR = MEETING_NOTE_ELAPSED_ATTR;
const EMPTY_PARAGRAPH_MD = "&nbsp;";
const NBSP = "\u00A0";

function formatElapsed(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

// --- Shared attribute definition ---

const elapsedAttr = {
  [ATTR]: {
    default: null as number | null,
    parseHTML: (el: HTMLElement) => {
      const raw = el.getAttribute("data-elapsed");
      if (raw == null || raw === "") return null;
      const n = parseInt(raw, 10);
      return Number.isNaN(n) ? null : n;
    },
    renderHTML: (attrs: Record<string, unknown>) => {
      const secs = attrs[ATTR] as number | null;
      if (secs == null) return {};
      return {
        "data-elapsed": String(secs),
        "data-elapsed-display": formatElapsed(secs),
      };
    },
  },
};

// --- Minimal Paragraph extend (attribute + markdown only) ---

export const MeetingNoteParagraph = Paragraph.extend({
  name: "paragraph",

  addAttributes() {
    return { ...(this.parent?.() ?? {}), ...elapsedAttr };
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "p",
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes),
      0,
    ];
  },

  renderMarkdown: (node, h, ctx) => {
    const content = Array.isArray(node.content) ? node.content : [];
    const secs = node.attrs?.[ATTR] as number | null | undefined;

    if (content.length === 0) {
      const prev = Array.isArray(ctx?.previousNode?.content)
        ? ctx.previousNode.content
        : [];
      const prevEmpty =
        ctx?.previousNode?.type === "paragraph" && prev.length === 0;
      const empty = prevEmpty ? EMPTY_PARAGRAPH_MD : "";
      if (secs != null && empty !== "")
        return `<p data-elapsed="${secs}">${empty}</p>`;
      if (secs != null) return `<p data-elapsed="${secs}"></p>`;
      return empty;
    }

    const inner = h.renderChildren(content);
    return secs != null ? `<p data-elapsed="${secs}">${inner}</p>` : inner;
  },

  parseMarkdown: (token, helpers) => {
    const tokens = token.tokens || [];
    if (tokens.length === 1 && tokens[0].type === "image") {
      return helpers.parseChildren([tokens[0]]);
    }
    const content = helpers.parseInline(tokens);
    if (
      content.length === 1 &&
      content[0].type === "text" &&
      (content[0].text === EMPTY_PARAGRAPH_MD || content[0].text === NBSP)
    ) {
      return helpers.createNode("paragraph", undefined, []);
    }
    return helpers.createNode("paragraph", undefined, content);
  },
});

// --- Minimal Heading extend (attribute + markdown only) ---

export const MeetingNoteHeading = Heading.extend({
  name: "heading",

  addAttributes() {
    return { ...(this.parent?.() ?? {}), ...elapsedAttr };
  },

  renderHTML({ node, HTMLAttributes }) {
    const hasLevel = this.options.levels.includes(node.attrs.level);
    const level = hasLevel ? node.attrs.level : this.options.levels[0];
    return [
      `h${level}`,
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes),
      0,
    ];
  },

  renderMarkdown: (node, h) => {
    const level = node.attrs?.level
      ? parseInt(String(node.attrs.level), 10)
      : 1;
    const secs = node.attrs?.[ATTR] as number | null | undefined;
    if (!node.content) return "";
    const inner = h.renderChildren(node.content);
    if (secs != null)
      return `<h${level} data-elapsed="${secs}">${inner}</h${level}>`;
    return `${"#".repeat(level)} ${inner}`;
  },

  parseMarkdown: (token, helpers) => {
    return helpers.createNode(
      "heading",
      { level: token.depth || 1 },
      helpers.parseInline(token.tokens || []),
    );
  },
});

// --- Stamp extension (auto-stamp new blocks with elapsed time) ---

export interface MeetingNoteElapsedOptions {
  getElapsedSecs: () => number | null;
}

export const MeetingNoteElapsed = Extension.create<MeetingNoteElapsedOptions>({
  name: "meetingNoteElapsed",

  addOptions() {
    return { getElapsedSecs: () => null as number | null };
  },

  addProseMirrorPlugins() {
    const getElapsedSecs = () => this.options.getElapsedSecs();

    return [
      new Plugin({
        key: new PluginKey("meetingNoteElapsed"),

        appendTransaction(transactions, oldState, newState) {
          return appendMeetingNoteElapsedTransaction(
            transactions,
            oldState,
            newState,
            getElapsedSecs,
          );
        },
      }),
    ];
  },
});
