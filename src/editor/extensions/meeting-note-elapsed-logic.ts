import {
  combineTransactionSteps,
  findChildrenInRange,
  getChangedRanges,
} from "@tiptap/core";
import { isHistoryTransaction } from "@tiptap/pm/history";
import type { Node as PMNode } from "@tiptap/pm/model";
import type { Mappable } from "@tiptap/pm/transform";
import type { EditorState, Transaction } from "@tiptap/pm/state";

export const MEETING_NOTE_ELAPSED_ATTR = "meetingElapsedSecs" as const;
export const MEETING_NOTE_STAMP_META = "meetingNoteStamp" as const;

const ATTR = MEETING_NOTE_ELAPSED_ATTR;

/**
 * Reads the elapsed stamp from the pre-image block in `oldDoc` for a position in the new doc,
 * using the inverted step mapping (new → old coordinates).
 */
export function findPreservedStampFromInvertedMapping(
  oldDoc: PMNode,
  invertedMapping: Mappable,
  blockPosInNewDoc: number,
): number | null {
  // Use map(), not mapResult().deleted — setNodeMarkup replaces the node and mapResult
  // can mark the position before the block as "deleted" even though it maps cleanly via map().
  const oldPos = invertedMapping.map(blockPosInNewDoc, -1);

  const innerSize = oldDoc.content.size;

  const stampFromBlockNode = (n: PMNode | null | undefined): number | null => {
    if (!n) return null;
    if (n.type.name !== "paragraph" && n.type.name !== "heading") return null;
    const s = n.attrs[ATTR] as number | null | undefined;
    return s != null && !Number.isNaN(Number(s)) ? Number(s) : null;
  };

  // nodeAt first: mapped position can sit on a doc boundary where resolve() yields depth 0,
  // so the ancestor walk never runs; nodeAt still returns the block at that offset.
  for (const p of [oldPos, oldPos - 1, oldPos + 1]) {
    if (p < 0 || p > innerSize) continue;
    const n = oldDoc.nodeAt(p);
    if (!n) continue;
    if (n.type.name === "paragraph" || n.type.name === "heading") {
      return stampFromBlockNode(n);
    }
  }

  const resolveCandidates = new Set<number>();
  resolveCandidates.add(Math.max(1, Math.min(oldPos, innerSize)));
  resolveCandidates.add(Math.max(1, Math.min(oldPos + 1, innerSize)));
  if (oldPos - 1 >= 0) resolveCandidates.add(Math.min(oldPos - 1, innerSize));

  for (const clamped of resolveCandidates) {
    const $old = oldDoc.resolve(clamped);
    for (let d = $old.depth; d > 0; d--) {
      const n = $old.node(d);
      if (n.type.name === "paragraph" || n.type.name === "heading") {
        const s = n.attrs[ATTR] as number | null | undefined;
        return s != null && !Number.isNaN(Number(s)) ? Number(s) : null;
      }
    }
  }
  return null;
}

/**
 * Follow-up transaction for meeting elapsed stamping: restore dropped attrs, clear empty lines,
 * stamp new content when `getElapsedSecs` is set. Returns `null` if nothing to do.
 */
export function appendMeetingNoteElapsedTransaction(
  transactions: readonly Transaction[],
  oldState: EditorState,
  newState: EditorState,
  getElapsedSecs: () => number | null,
): Transaction | null {
  if (transactions.some((t) => t.getMeta(MEETING_NOTE_STAMP_META))) return null;
  if (transactions.some((t) => isHistoryTransaction(t))) return null;
  if (!transactions.some((t) => t.docChanged)) return null;
  if (oldState.doc.eq(newState.doc)) return null;

  const secs = getElapsedSecs();
  const canStamp = secs != null && secs > 0;

  const transform = combineTransactionSteps(
    oldState.doc,
    transactions as Transaction[],
  );
  const changes = getChangedRanges(transform);

  const tr = newState.tr.setMeta(MEETING_NOTE_STAMP_META, true);
  let changed = false;

  const isStampableBlock = (name: string) =>
    name === "paragraph" || name === "heading";

  const restoreLostStampFromOldState = (blockPos: number) => {
    const at = tr.doc.nodeAt(blockPos);
    if (!at || !isStampableBlock(at.type.name)) return;
    if (at.content.size === 0) return;
    if (at.attrs[ATTR] != null) return;

    const inv = transform.mapping.invert();
    const preserved = findPreservedStampFromInvertedMapping(
      oldState.doc,
      inv,
      blockPos,
    );
    if (preserved == null) return;

    const latest = tr.doc.nodeAt(blockPos);
    if (!latest || latest.attrs[ATTR] != null) return;
    tr.setNodeMarkup(blockPos, undefined, {
      ...latest.attrs,
      [ATTR]: preserved,
    });
    changed = true;
  };

  const clearStampIfEmpty = (node: PMNode, pos: number) => {
    if (!isStampableBlock(node.type.name)) return;
    if (node.content.size !== 0) return;
    const at = tr.doc.nodeAt(pos);
    if (!at || at.attrs[ATTR] == null) return;
    tr.setNodeMarkup(pos, undefined, { ...at.attrs, [ATTR]: null });
    changed = true;
  };

  const stampIfNeeded = (node: PMNode, pos: number) => {
    if (!canStamp) return;
    if (!isStampableBlock(node.type.name)) return;
    if (node.content.size === 0) return;
    const stamped = tr.doc.nodeAt(pos)?.attrs[ATTR];
    if (stamped != null) return;
    tr.setNodeMarkup(pos, undefined, {
      ...node.attrs,
      [ATTR]: secs,
    });
    changed = true;
  };

  for (const { newRange } of changes) {
    const nodes = findChildrenInRange(newState.doc, newRange, (n) =>
      isStampableBlock(n.type.name),
    );
    for (const { pos } of nodes) {
      restoreLostStampFromOldState(pos);
    }
  }

  for (const { newRange } of changes) {
    const nodes = findChildrenInRange(newState.doc, newRange, (n) =>
      isStampableBlock(n.type.name),
    );
    for (const { node, pos } of nodes) {
      clearStampIfEmpty(node, pos);
      stampIfNeeded(node, pos);
    }
  }

  const { $head } = newState.selection;
  const blockNode = $head.parent;
  const blockPos = $head.before($head.depth);
  if (isStampableBlock(blockNode.type.name)) {
    if (blockNode.content.size > 0) {
      restoreLostStampFromOldState(blockPos);
    }
    if (blockNode.content.size === 0) {
      clearStampIfEmpty(blockNode, blockPos);
    } else if (canStamp && tr.doc.nodeAt(blockPos)?.attrs[ATTR] == null) {
      tr.setNodeMarkup(blockPos, undefined, {
        ...blockNode.attrs,
        [ATTR]: secs,
      });
      changed = true;
    }
  }

  if (!changed) return null;

  tr.setStoredMarks(newState.tr.storedMarks);

  return tr;
}
