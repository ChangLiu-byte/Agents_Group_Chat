import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Agent } from "../types/agent";
import type { Session, SessionMode } from "../types/session";
import { ModeToggle } from "./ModeToggle";
import { SessionAgentRoster } from "./SessionAgentRoster";

interface SessionDetailProps {
  sessionId: string;
  onBack: () => void;
}

/** Session roster management (Part 1 - no chat/message view yet). */
export function SessionDetail({ sessionId, onBack }: SessionDetailProps) {
  const [session, setSession] = useState<Session | null>(null);
  const [roster, setRoster] = useState<Agent[]>([]);
  const [allAgents, setAllAgents] = useState<Agent[]>([]);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    try {
      const [sessionResult, rosterResult, agentsResult] = await Promise.all([
        invoke<Session>("get_session", { sessionId }),
        invoke<Agent[]>("list_session_agents", { sessionId }),
        invoke<Agent[]>("list_agents"),
      ]);
      setSession(sessionResult);
      setRoster(rosterResult);
      setAllAgents(agentsResult);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  function handleModeChange(mode: SessionMode) {
    setSession((prev) => (prev ? { ...prev, mode } : prev));
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

  return (
    <div className="space-y-6">
      <div className="flex items-start justify-between">
        <div>
          <button onClick={onBack} className="text-sm text-indigo-600 hover:underline">
            ← 返回会话列表
          </button>
          <h1 className="mt-2 text-xl font-bold text-slate-900">{session.topic}</h1>
        </div>
        <ModeToggle sessionId={session.id} mode={session.mode} onModeChange={handleModeChange} />
      </div>

      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
          {error}
        </div>
      )}

      <section className="space-y-2">
        <h2 className="text-sm font-semibold text-slate-700">参会名单（发言顺序）</h2>
        <SessionAgentRoster
          sessionId={session.id}
          roster={roster}
          allAgents={allAgents}
          onRosterChange={setRoster}
          onError={setError}
        />
      </section>

      <section className="rounded-lg border border-dashed border-slate-300 bg-slate-50 p-4 text-sm text-slate-500">
        聊天消息视图（Part 2）尚未实现，当前只完成会话创建与名单管理。
      </section>
    </div>
  );
}
