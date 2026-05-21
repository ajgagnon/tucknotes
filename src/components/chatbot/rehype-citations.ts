import type { Plugin } from "unified";
import type { Element, Root, Text, ElementContent } from "hast";
import { visit, SKIP } from "unist-util-visit";

const CITATION_RE = /\[(\d+)\]/g;

/**
 * Rehype plugin that walks text nodes and replaces `[N]` patterns with
 *   `<cite data-citation="N">[N]</cite>`
 * elements. A custom Streamdown `components.cite` override turns those into
 * interactive citation chips.
 *
 * Skips text inside `<code>` and `<pre>` so inline code like ``[1]`` and fenced
 * code blocks stay literal.
 */
export const rehypeCitations: Plugin<[], Root> = () => {
  return (tree) => {
    visit(tree, "text", (node: Text, index, parent) => {
      if (index === undefined || parent === null || parent === undefined) {
        return;
      }
      if (parent.type === "element") {
        const tag = (parent as Element).tagName;
        if (tag === "code" || tag === "pre") return;
      }

      const value = node.value;
      // Quick reject — avoid building regex state for the common case.
      if (!value.includes("[")) return;

      const matches = [...value.matchAll(CITATION_RE)];
      if (matches.length === 0) return;

      const replacement: ElementContent[] = [];
      let cursor = 0;
      for (const match of matches) {
        const start = match.index ?? 0;
        if (start > cursor) {
          replacement.push({
            type: "text",
            value: value.slice(cursor, start),
          });
        }
        const marker = match[0]; // e.g. "[3]"
        const num = match[1]; // e.g. "3"
        replacement.push({
          type: "element",
          tagName: "cite",
          properties: { dataCitation: num },
          children: [{ type: "text", value: marker }],
        });
        cursor = start + marker.length;
      }
      if (cursor < value.length) {
        replacement.push({ type: "text", value: value.slice(cursor) });
      }

      // Splice into parent.
      (parent.children as ElementContent[]).splice(
        index,
        1,
        ...replacement,
      );
      // Skip the replaced subtree — visiting newly-inserted nodes would re-
      // process them, and we know they don't contain further `[N]` patterns.
      return [SKIP, index + replacement.length];
    });
  };
};
