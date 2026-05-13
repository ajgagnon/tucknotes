// Regression: a `## heading` that immediately follows a task list comes out
// of marked's lexer with `tokens: []` even though `text` is correct. Without
// a workaround in `MeetingNoteHeading.parseMarkdown`, tiptap renders an empty
// <h2> where the heading text should be, which manifests as a missing
// `## Open questions` header in the meeting summary editor.

import { describe, it, expect } from "vitest";
import { Markdown, MarkdownManager } from "@tiptap/markdown";
import { StarterKit } from "@tiptap/starter-kit";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import { MeetingNoteHeading, MeetingNoteParagraph } from "./meeting-note-elapsed";

function makeManager() {
  const starter = StarterKit.configure({
    horizontalRule: false,
    paragraph: false,
    heading: false,
  });
  return new MarkdownManager({
    extensions: [
      starter,
      MeetingNoteParagraph,
      MeetingNoteHeading,
      TaskList,
      TaskItem.configure({ nested: true }),
      Markdown,
    ],
  });
}

function headingTexts(doc: unknown): string[] {
  const out: string[] = [];
  function walk(n: { type?: string; content?: unknown[] }) {
    if (n.type === "heading") {
      const t = (n.content as Array<{ text?: string }> | undefined)
        ?.map((c) => c.text ?? "")
        .join("") ?? "";
      out.push(t);
    }
    if (Array.isArray(n.content)) n.content.forEach((c) => walk(c as never));
  }
  walk(doc as never);
  return out;
}

describe("meeting summary markdown round-trip", () => {
  it("preserves a heading that comes right after a task list", () => {
    const md = "- [ ] task\n\n## Open questions\n\n- one\n";
    expect(headingTexts(makeManager().parse(md))).toEqual(["Open questions"]);
  });

  it("preserves all section headings in a realistic summary body", () => {
    const md = [
      "## Summary",
      "",
      "Prose paragraph.",
      "",
      "## Action items",
      "",
      "- [ ] **Owner:** do the thing.",
      "",
      "## Open questions",
      "",
      "- Should we ship it?",
      "",
    ].join("\n");
    expect(headingTexts(makeManager().parse(md))).toEqual([
      "Summary",
      "Action items",
      "Open questions",
    ]);
  });
});
