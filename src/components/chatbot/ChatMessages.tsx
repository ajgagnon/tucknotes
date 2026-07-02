import { Loader2, MessageCircle, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Conversation,
  ConversationContent,
  ConversationEmptyState,
  ConversationScrollButton,
} from "@/components/ai-elements/conversation";
import { Message, MessageContent } from "@/components/ai-elements/message";
import { cn } from "@/lib/utils";
import { Streamdown } from "streamdown";
import { type SearchHit } from "./use-chat-stream";
import { type ChatMessage, type ToolCall } from "./use-chat-session";
import { CitationChip } from "./CitationChip";
import { rehypeCitations } from "./rehype-citations";

/** The scrollable conversation: empty states, messages, tool cards, citations. */
export function ChatMessages({
  messages,
  modelReady,
  onOpenSettings,
  onOpenMeeting,
}: {
  messages: ChatMessage[];
  modelReady: boolean | null;
  onOpenSettings?: () => void;
  onOpenMeeting?: (meetingId: string) => void;
}) {
  return (
    <Conversation className="flex-1">
      <ConversationContent className="gap-4 p-3">
        {messages.length === 0 && (
          <>
            <ConversationEmptyState
              icon={<MessageCircle className="size-8" />}
              title={modelReady === false ? "Model required" : "Ask me anything"}
              description={
                modelReady === false
                  ? "Download a model in Settings to use Tuck."
                  : "I'm here to help you get more out of your meetings."
              }
            />
            {modelReady === false && onOpenSettings && (
              <div className="-mt-4 flex justify-center">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={onOpenSettings}
                >
                  Open Settings
                </Button>
              </div>
            )}
          </>
        )}
        {messages.map((m) => {
          const sources: SearchHit[] =
            m.toolCalls?.flatMap((tc) => tc.hits ?? []) ?? [];
          return (
            <Message from={m.role} key={m.id}>
              <MessageContent>
                {m.role === "assistant" ? (
                  <>
                    {m.toolCalls?.map((tc) => (
                      <ToolCallCard
                        key={tc.id}
                        call={tc}
                        onOpenMeeting={onOpenMeeting}
                      />
                    ))}
                    <CitedResponse
                      text={m.text || "​"}
                      sources={sources}
                      onOpenMeeting={onOpenMeeting}
                    />
                  </>
                ) : (
                  m.text
                )}
              </MessageContent>
            </Message>
          );
        })}
      </ConversationContent>
      <ConversationScrollButton />
    </Conversation>
  );
}

/**
 * Streamdown-based renderer for assistant prose that may contain `[N]` citation
 * markers. The rehype plugin rewrites those into `<cite data-citation="N">[N]</cite>`
 * elements, which we override to render a hoverable, clickable `CitationChip`.
 */
function CitedResponse({
  text,
  sources,
  onOpenMeeting,
}: {
  text: string;
  sources: SearchHit[];
  onOpenMeeting?: (meetingId: string) => void;
}) {
  return (
    <Streamdown
      className="size-full [&>*:first-child]:mt-0 [&>*:last-child]:mb-0"
      rehypePlugins={[rehypeCitations]}
      components={{
        cite: (props) => (
          <CitationChip
            data-citation={
              (props as { "data-citation"?: string })["data-citation"]
            }
            sources={sources}
            onOpenMeeting={onOpenMeeting}
          >
            {props.children}
          </CitationChip>
        ),
      }}
    >
      {text}
    </Streamdown>
  );
}

function ToolCallCard({
  call,
  onOpenMeeting,
}: {
  call: ToolCall;
  onOpenMeeting?: (meetingId: string) => void;
}) {
  const isSearch = call.name === "search_meetings";
  let query = "";
  if (call.args) {
    try {
      query = (JSON.parse(call.args)?.query as string | undefined) ?? "";
    } catch {
      query = "";
    }
  }

  return (
    <div className="mb-2 flex flex-col gap-1.5 rounded-md border border-border bg-muted/40 px-2.5 py-2 text-xs">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        {call.status === "running" ? (
          <Loader2 className="size-3.5 shrink-0 animate-spin" />
        ) : (
          <Search className="size-3.5 shrink-0" />
        )}
        <span className="min-w-0 flex-1 truncate">
          {isSearch
            ? call.status === "running"
              ? query
                ? `Searching meetings: ${query}`
                : "Searching meetings…"
              : `Search results${query ? `: ${query}` : ""}`
            : call.name}
        </span>
        {call.status === "done" && call.hits && (
          <span className="shrink-0 tabular-nums">
            {call.hits.length} {call.hits.length === 1 ? "result" : "results"}
          </span>
        )}
      </div>
      {call.status === "done" && call.hits && call.hits.length > 0 && (
        <ul className="flex flex-col gap-1">
          {call.hits.map((hit, i) => (
            <li key={`${hit.meeting_id}-${hit.kind}-${i}`}>
              <button
                type="button"
                onClick={() => onOpenMeeting?.(hit.meeting_id)}
                disabled={!onOpenMeeting}
                className={cn(
                  "flex w-full flex-col gap-0.5 rounded border border-border/60 bg-background/60 px-2 py-1.5 text-left transition-colors",
                  onOpenMeeting && "hover:border-border hover:bg-background",
                  !onOpenMeeting && "cursor-default",
                )}
              >
                <div className="flex items-center gap-1.5">
                  <span className="min-w-0 flex-1 truncate font-medium text-foreground">
                    {hit.meeting_title || "Untitled meeting"}
                  </span>
                  <span
                    className={cn(
                      "shrink-0 rounded px-1 py-0.5 text-[10px] uppercase tracking-wide",
                      hit.kind === "summary"
                        ? "bg-blue-500/10 text-blue-600 dark:text-blue-400"
                        : "bg-muted text-muted-foreground",
                    )}
                  >
                    {hit.kind}
                  </span>
                </div>
                <div
                  className="line-clamp-2 text-muted-foreground [&_mark]:rounded [&_mark]:bg-yellow-200/60 [&_mark]:px-0.5 [&_mark]:text-foreground dark:[&_mark]:bg-yellow-300/30"
                  dangerouslySetInnerHTML={{ __html: hit.snippet }}
                />
              </button>
            </li>
          ))}
        </ul>
      )}
      {call.status === "done" && (!call.hits || call.hits.length === 0) && (
        <p className="text-muted-foreground italic">No matching meetings.</p>
      )}
    </div>
  );
}
