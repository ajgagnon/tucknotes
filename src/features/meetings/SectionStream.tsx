import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { Skeleton } from "@/components/ui/skeleton";
import type { SummarySection } from "./types";
import "./SectionStream.scss";

/**
 * Progressive per-section summary view. The summary is generated one section at
 * a time (one focused LLM pass each), so each section advances through visibly
 * distinct states — a pause between sections then reads as anticipation, not a
 * freeze:
 *
 *   pending  → skeleton lines (not reached yet)
 *   thinking → a breathing write-head (with ripple) in the gutter + a quiet
 *              status line, while that section's transcript prefills (no tokens yet)
 *   writing  → steady write-head + a caret, body streaming in as live Markdown
 *   done     → a tick in the gutter
 *
 * Empty sections (`skipped`) collapse out. Section bodies render inside the same
 * `.tiptap.ProseMirror` prose classes the persisted summary uses, so task-list
 * checkboxes, **You/Them**, em-dash bullets, and inline code render correctly
 * even mid-stream. Mount inside `.simple-editor-wrapper.meeting-summary-prose`.
 */
export function SectionStream({ sections }: { sections: SummarySection[] }) {
  return (
    <div className="secstream">
      {sections.map((section, index) => {
        if (section.state === "skipped") return null;

        const { heading, body, state } = section;
        const active = state === "thinking" || state === "writing";
        const started = state === "writing" || state === "done";

        return (
          <section key={index} className="secstream-row" data-state={state}>
            <div className="secstream-gutter" aria-hidden>
              {active && (
                <span className="secstream-head">
                  {state === "thinking" && (
                    <span className="secstream-ripple" />
                  )}
                </span>
              )}
            </div>

            <div className="secstream-content">
              <h2 className="secstream-heading">{heading}</h2>

              {state === "pending" && (
                <div className="secstream-skeleton" aria-hidden>
                  <Skeleton className="h-3.5 w-[92%]" />
                  <Skeleton className="h-3.5 w-[64%]" />
                </div>
              )}

              {state === "thinking" && (
                <p className="secstream-thinking">
                  <span className="secstream-dots">
                    <i />
                    <i />
                    <i />
                  </span>
                </p>
              )}

              {started && (
                <div className="secstream-body">
                  <div
                    className="tiptap ProseMirror simple-editor secstream-md"
                    style={{ padding: 0, whiteSpace: "normal" }}
                  >
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>
                      {body}
                    </ReactMarkdown>
                  </div>
                  {state === "writing" && (
                    <span className="secstream-caret" aria-hidden />
                  )}
                </div>
              )}
            </div>
          </section>
        );
      })}
    </div>
  );
}
