import { useEffect, useRef } from "react";
import type { Agent } from "../types/agent";
import type { AgentErrorEvent, Message } from "../types/session";

/** One entry of the chat timeline, in arrival order. Errors are live-only
 * (never stored), so they only exist for the current view. */
export type ChatItem =
  | { kind: "message"; message: Message }
  | { kind: "error"; key: string; error: AgentErrorEvent };

interface MessageListProps {
  items: ChatItem[];
  agentsById: Map<string, Agent>;
  running: boolean;
}

const REMOVED_AGENT_NAME = "已移除的 Agent";

export function MessageList({ items, agentsById, running }: MessageListProps) {
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [items.length, running]);

  let lastRound: number | null = null;

  return (
    <div
      ref={scrollRef}
      className="min-h-[240px] max-h-[55vh] overflow-y-auto rounded-lg border border-slate-200 bg-slate-50 p-4 space-y-3"
    >
      {items.length === 0 && (
        <p className="text-sm text-slate-400">还没有消息，在下方输入话题开始第一轮。</p>
      )}

      {items.map((item) => {
        const round = item.kind === "message" ? item.message.round_number : item.error.round_number;
        const showDivider = round !== lastRound;
        lastRound = round;

        return (
          <div key={item.kind === "message" ? item.message.id : item.key} className="space-y-3">
            {showDivider && (
              <div className="flex items-center gap-3 pt-1 text-xs text-slate-400">
                <div className="h-px flex-1 bg-slate-200" />
                第 {round} 轮
                <div className="h-px flex-1 bg-slate-200" />
              </div>
            )}
            {item.kind === "message" ? (
              <MessageBubble message={item.message} agentsById={agentsById} />
            ) : (
              <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                <span className="font-medium">{item.error.agent_name}</span> 这一轮没有回复：
                <span className="whitespace-pre-wrap break-words">{item.error.error}</span>
              </div>
            )}
          </div>
        );
      })}

      {running && <p className="text-sm text-slate-400">本轮进行中，Agent 依次发言…</p>}
    </div>
  );
}

function MessageBubble({
  message,
  agentsById,
}: {
  message: Message;
  agentsById: Map<string, Agent>;
}) {
  if (message.agent_id === null) {
    const mentionedAgent = message.refers_to ? agentsById.get(message.refers_to) : undefined;
    return (
      <div className="flex flex-col items-end">
        {message.refers_to && (
          <p className="mb-1 text-xs text-slate-400">
            → 点名 {mentionedAgent?.name ?? REMOVED_AGENT_NAME}
          </p>
        )}
        <div className="max-w-[80%] rounded-lg bg-indigo-600 px-3 py-2 text-sm text-white whitespace-pre-wrap break-words">
          {message.content}
        </div>
      </div>
    );
  }

  const agent = agentsById.get(message.agent_id);
  const color = agent?.color ?? "#94a3b8";

  return (
    <div className="flex justify-start">
      <div className="max-w-[80%]">
        <div className="mb-1 flex items-center gap-2 text-xs text-slate-500">
          <span
            className="h-2.5 w-2.5 rounded-full border border-black/10"
            style={{ backgroundColor: color }}
            aria-hidden
          />
          {agent?.name ?? REMOVED_AGENT_NAME}
        </div>
        <div
          className="rounded-lg border border-slate-200 border-l-4 bg-white px-3 py-2 text-sm text-slate-800 whitespace-pre-wrap break-words"
          style={{ borderLeftColor: color }}
        >
          {message.content}
        </div>
      </div>
    </div>
  );
}
