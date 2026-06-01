import { Editor, Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

/** Class applied to the hovered block's node DOM. Styled in meeting-notes-editor.scss. */
export const SUMMARY_HOVER_CLASS = "summary-block-active";

export const summaryHoverKey = new PluginKey<DecorationSet>(
  "summaryHoverHighlight",
);

/** Meta payload: a node range to highlight, or `null` to clear. */
type SummaryHoverMeta = { from: number; to: number } | null;

/**
 * Highlights the hovered summary block via a ProseMirror decoration rather than a
 * manual class mutation (which PM's DOMObserver would strip). The active block is
 * pushed in from React via `setSummaryHover`, so the band can persist while the
 * cursor is over the detached action toolbar.
 */
export const SummaryHoverHighlight = Extension.create({
  name: "summaryHoverHighlight",

  addProseMirrorPlugins() {
    return [
      new Plugin<DecorationSet>({
        key: summaryHoverKey,
        state: {
          init: () => DecorationSet.empty,
          apply(tr, set) {
            const meta = tr.getMeta(summaryHoverKey) as
              | SummaryHoverMeta
              | undefined;
            if (meta !== undefined) {
              if (meta === null) return DecorationSet.empty;
              return DecorationSet.create(tr.doc, [
                Decoration.node(meta.from, meta.to, {
                  class: SUMMARY_HOVER_CLASS,
                }),
              ]);
            }
            // Keep the band aligned across edits.
            return set.map(tr.mapping, tr.doc);
          },
        },
        props: {
          decorations(state) {
            return summaryHoverKey.getState(state);
          },
        },
      }),
    ];
  },
});

/**
 * Highlight the block containing `el`, or clear when `el` is null. List rows decorate
 * the `<li>` (matching the `li.summary-block-active` selector); other content decorates
 * the top-level block. Dispatches a meta-only transaction, so the doc/selection/markdown
 * are untouched.
 */
export function setSummaryHover(editor: Editor, el: HTMLElement | null): void {
  const { view } = editor;
  if (!el) {
    view.dispatch(
      view.state.tr.setMeta(summaryHoverKey, null).setMeta("addToHistory", false),
    );
    return;
  }

  let pos: number;
  try {
    pos = view.posAtDOM(el, 0);
  } catch {
    return;
  }
  const $pos = view.state.doc.resolve(pos);
  if ($pos.depth < 1) return;

  let depth = 1;
  for (let d = $pos.depth; d >= 1; d--) {
    const name = $pos.node(d).type.name;
    if (name === "listItem" || name === "taskItem") {
      depth = d;
      break;
    }
  }

  view.dispatch(
    view.state.tr
      .setMeta(summaryHoverKey, {
        from: $pos.before(depth),
        to: $pos.after(depth),
      })
      .setMeta("addToHistory", false),
  );
}
