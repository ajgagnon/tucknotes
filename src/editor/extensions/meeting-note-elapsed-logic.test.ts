import { describe, it, expect } from "vitest";
import { getSchema } from "@tiptap/core";
import Document from "@tiptap/extension-document";
import Text from "@tiptap/extension-text";
import type { Node } from "@tiptap/pm/model";
import { EditorState, TextSelection } from "@tiptap/pm/state";
import { Transform } from "@tiptap/pm/transform";
import {
  appendMeetingNoteElapsedTransaction,
  findPreservedStampFromInvertedMapping,
  MEETING_NOTE_ELAPSED_ATTR,
} from "./meeting-note-elapsed-logic";
import {
  MeetingNoteHeading,
  MeetingNoteParagraph,
} from "./meeting-note-elapsed";

function makeSchema() {
  return getSchema([Document, Text, MeetingNoteParagraph, MeetingNoteHeading]);
}

function paragraphPos(doc: Node): number {
  let pos = -1;
  doc.descendants((node, p) => {
    if (node.type.name === "paragraph") {
      pos = p;
      return false;
    }
  });
  if (pos < 0) throw new Error("no paragraph");
  return pos;
}

describe("findPreservedStampFromInvertedMapping", () => {
  it("returns stamp from mapped old paragraph when attrs were stripped", () => {
    const schema = makeSchema();
    const oldDoc = schema.node("doc", null, [
      schema.node("paragraph", { [MEETING_NOTE_ELAPSED_ATTR]: 99 }, [
        schema.text("hi"),
      ]),
    ]);
    const p0 = paragraphPos(oldDoc);
    const t = new Transform(oldDoc);
    const para = oldDoc.nodeAt(p0)!;
    t.setNodeMarkup(p0, undefined, {
      ...para.attrs,
      [MEETING_NOTE_ELAPSED_ATTR]: null,
    });
    const inv = t.mapping.invert();
    const newP = paragraphPos(t.doc);
    expect(findPreservedStampFromInvertedMapping(oldDoc, inv, newP)).toBe(99);
  });
});

describe("appendMeetingNoteElapsedTransaction", () => {
  it("restores meetingElapsedSecs when a command dropped it (not recording)", () => {
    const schema = makeSchema();
    const doc = schema.node("doc", null, [
      schema.node("paragraph", { [MEETING_NOTE_ELAPSED_ATTR]: 42 }, [
        schema.text("hi"),
      ]),
    ]);
    const oldState = EditorState.create({ doc, schema });
    const p0 = paragraphPos(doc);
    const para = doc.nodeAt(p0)!;
    const trUser = oldState.tr.setNodeMarkup(p0, undefined, {
      ...para.attrs,
      [MEETING_NOTE_ELAPSED_ATTR]: null,
    });
    const newState = oldState.apply(trUser);
    const follow = appendMeetingNoteElapsedTransaction(
      [trUser],
      oldState,
      newState,
      () => null,
    );
    expect(follow).not.toBeNull();
    const finalState = newState.apply(follow!);
    const restored = finalState.doc.nodeAt(paragraphPos(finalState.doc))?.attrs[
      MEETING_NOTE_ELAPSED_ATTR
    ];
    expect(restored).toBe(42);
  });

  it("clears stamp on empty paragraph after text is deleted", () => {
    const schema = makeSchema();
    const doc = schema.node("doc", null, [
      schema.node("paragraph", { [MEETING_NOTE_ELAPSED_ATTR]: 7 }, [
        schema.text("x"),
      ]),
    ]);
    const oldState = EditorState.create({ doc, schema });
    const p0 = paragraphPos(doc);
    const $p = doc.resolve(p0 + 1);
    const from = $p.pos;
    const to = from + 1;
    const trUser = oldState.tr.delete(from, to);
    const newState = oldState.apply(trUser);
    const follow = appendMeetingNoteElapsedTransaction(
      [trUser],
      oldState,
      newState,
      () => null,
    );
    expect(follow).not.toBeNull();
    const finalState = newState.apply(follow!);
    const emptyP = finalState.doc.nodeAt(paragraphPos(finalState.doc))!;
    expect(emptyP.content.size).toBe(0);
    expect(emptyP.attrs[MEETING_NOTE_ELAPSED_ATTR]).toBeNull();
  });

  it("returns null when there is no document change", () => {
    const schema = makeSchema();
    const doc = schema.node("doc", null, [
      schema.node("paragraph", {}, [schema.text("a")]),
    ]);
    const oldState = EditorState.create({ doc, schema });
    const trUser = oldState.tr;
    const newState = oldState.apply(trUser);
    const follow = appendMeetingNoteElapsedTransaction(
      [trUser],
      oldState,
      newState,
      () => 100,
    );
    expect(follow).toBeNull();
  });

  it("stamps after inserting text into unstamped paragraph while recording", () => {
    const schema = makeSchema();
    const doc = schema.node("doc", null, [schema.node("paragraph", {}, [])]);
    const oldState = EditorState.create({ doc, schema });
    const p0 = paragraphPos(doc);
    const trUser = oldState.tr.insertText("hi", p0 + 1);
    const newState = oldState.apply(trUser);
    const sel = TextSelection.create(newState.doc, p0 + 3);
    const newState2 = EditorState.create({
      doc: newState.doc,
      selection: sel,
      schema,
    });
    const follow = appendMeetingNoteElapsedTransaction(
      [trUser],
      oldState,
      newState2,
      () => 100,
    );
    expect(follow).not.toBeNull();
    const finalState = newState2.apply(follow!);
    const stamp = finalState.doc.nodeAt(paragraphPos(finalState.doc))?.attrs[
      MEETING_NOTE_ELAPSED_ATTR
    ];
    expect(stamp).toBe(100);
  });

  it("does not stamp when not recording and paragraph was never stamped", () => {
    const schema = makeSchema();
    const doc = schema.node("doc", null, [schema.node("paragraph", {}, [])]);
    const oldState = EditorState.create({ doc, schema });
    const p0 = paragraphPos(doc);
    const trUser = oldState.tr.insertText("hi", p0 + 1);
    const newState = oldState.apply(trUser);
    const sel = TextSelection.create(newState.doc, p0 + 3);
    const newState2 = EditorState.create({
      doc: newState.doc,
      selection: sel,
      schema,
    });
    const follow = appendMeetingNoteElapsedTransaction(
      [trUser],
      oldState,
      newState2,
      () => null,
    );
    expect(follow).toBeNull();
    expect(
      newState2.doc.nodeAt(paragraphPos(newState2.doc))?.attrs[
        MEETING_NOTE_ELAPSED_ATTR
      ],
    ).toBeNull();
  });
});
