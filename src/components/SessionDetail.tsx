import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Agent } from "../types/agent";
import type {
  AgentErrorEvent,
  Message,
  RoundCompleteEvent,
  Session,
  SessionMode,
} from "../types/session";
import { ChatInput } from "./ChatInput";
import { MessageList, type ChatItem } from "./MessageList";
import { SessionAgentRoster } from "./SessionAgentRoster";

interface SessionDetailProps {
  sessionId: string;
  onBack: () => void;
}

/** The chat room: message timeline, input box, and (collapsible) roster
 * management. The backend owns the whole round; this view only triggers it
 * once and renders the events it emits as they arrive. */
export function SessionDetail({ sessionId, onBack }: SessionDetailProps) {
  const [session, setSession] = useState<Session | null>(null);
  const [roster, setRoster] = useState<Agent[]>([]);
  const [allAgents, setAllAgents] = useState<Agent[]>([]);
  const [items, setItems] = useState<ChatItem[]>([]);
  const [showRoster, setShowRoster] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const agentsById = useMemo(() => new Map(allAgents.map((a) => [a.id, a])), [allAgents]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    async function subscribe<T>(event: string, handler: (payload: T) => void) {
      const unlisten = await listen<T>(event, (e) => handler(e.payload));
      // The effect may already have been cleaned up while `listen` was
      // pending (React StrictMode double-invokes effects in dev).
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    }

    async function start() {
      // Subscribe before loading history so nothing emitted in between is
      // lost; the merge below de-duplicates by message id.
      await subscribe<Message>("message-added", (message) => {
        if (message.session_id !== sessionId) return;
        setItems((prev) =>
          prev.some((i) => i.kind === "message" && i.message.id === message.id)
            ? prev
            : [...prev, { kind: "message", message }],
        );
      });
      await subscribe<AgentErrorEvent>("agent-error", (agentError) => {
        if (agentError.session_id !== sessionId) return;
        setItems((prev) => [
          ...prev,
          { kind: "error", key: crypto.randomUUID(), error: agentError },
        ]);
      });
      await subscribe<RoundCompleteEvent>("round-complete", ({ session_id }) => {
        if (session_id !== sessionId) return;
        setSession((prev) => (prev ? { ...prev, status: "awaiting_user" } : prev));
      });
      if (disposed) return;

      try {
        const [sessionResult, rosterResult, agentsResult, messagesResult] = await Promise.all([
          invoke<Session>("get_session", { sessionId }),
          invoke<Agent[]>("list_session_agents", { sessionId }),
          invoke<Agent[]>("list_agents"),
          invoke<Message[]>("list_messages", { sessionId }),
        ]);
        if (disposed) return;
        setSession(sessionResult);
        setRoster(rosterResult);
        setAllAgents(agentsResult);
        setItems((prev) => {
          const loaded = new Set(messagesResult.map((m) => m.id));
          const fromHistory: ChatItem[] = messagesResult.map((message) => ({
            kind: "message",
            message,
          }));
          const arrivedMeanwhile = prev.filter(
            (i) => i.kind === "error" || !loaded.has(i.message.id),
          );
          return [...fromHistory, ...arrivedMeanwhile];
        });
        setError(null);
      } catch (e) {
        if (!disposed) setError(String(e));
      }
    }

    start();
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [sessionId]);

  async function handleSend(text: string) {
    setError(null);
    setSession((prev) => (prev ? { ...prev, status: "running" } : prev));
    try {
      await invoke("run_sequential_round", { sessionId, userMessage: text });
    } catch (e) {
      setError(String(e));
    } finally {
      // Whatever happened, the DB has the truth about the status.
      try {
        setSession(await invoke<Session>("get_session", { sessionId }));
      } catch (e) {
        setError(String(e));
      }
    }
  }

  async function handleModeChange(mode: SessionMode) {
    if (!session || session.mode === mode) return;
    try {
      await invoke("set_session_mode", { sessionId, mode });
      setSession((prev) => (prev ? { ...prev, mode } : prev));
    } catch (e) {
      setError(String(e));
    }
  }

  if (!session) {
    return (
      <div className="space-y-4">
        <button onClick={onBack} className="text-sm text-indigo-600 hover:underline">
          ← 返回会话列表
        </button>
        {error && (
          <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
            {error}
          </div>
        )}
      </div>
    );
  }

  const running = session.status === "running";
  const lastRound = items.reduce(
    (max, i) => Math.max(max, i.kind === "message" ? i.message.round_number : i.error.round_number),
    0,
  );

  return (
    <div className="space-y-4">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <button onClick={onBack} className="text-sm text-indigo-600 hover:underline">
            ← 返回会话列表
          </button>
          <h1 className="mt-2 break-words text-xl font-bold text-slate-900">{session.topic}</h1>
        </div>
        <button
          onClick={() => setShowRoster((v) => !v)}
          className="shrink-0 rounded-md border border-slate-300 px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-100"
        >
          管理名单（{roster.length}）
        </button>
      </div>

      {error && (
        <div className="whitespace-pre-wrap break-words rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
          {error}
        </div>
      )}

      {showRoster && (
        <section className="space-y-2 rounded-lg border border-slate-200 bg-white p-4">
          <h2 className="text-sm font-semibold text-slate-700">参会名单（发言顺序）</h2>
          <SessionAgentRoster
            sessionId={session.id}
            roster={roster}
            allAgents={allAgents}
            onRosterChange={setRoster}
            onError={setError}
          />
        </section>
      )}

      <MessageList items={items} agentsById={agentsById} running={running} />

      {session.status === "awaiting_user" && lastRound > 0 && (
        <p className="text-sm text-slate-500">
          第 {lastRound} 轮已结束。输入新消息开始下一轮，或用下方按钮切换发言模式。
        </p>
      )}

      <ChatInput
        mode={session.mode}
        running={running}
        roster={roster}
        onModeChange={handleModeChange}
        onSend={handleSend}
      />
    </div>
  );
}
